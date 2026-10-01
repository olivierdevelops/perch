//! macOS backend: rewrite the spawn to `sandbox-exec -p <profile> prog args…`.
use super::{ConfineError, Scopes, Support};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

pub fn probe() -> Support {
    if !Path::new(SANDBOX_EXEC).exists() {
        return Support::Unsupported("sandbox-exec not found".to_string());
    }
    match Command::new(SANDBOX_EXEC)
        .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
        .output()
    {
        Ok(o) if o.status.success() => Support::Enforced,
        Ok(o) => Support::Unsupported(format!(
            "sandbox-exec unusable (already inside a sandbox?): {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Support::Unsupported(format!("sandbox-exec failed to start: {e}")),
    }
}

/// `probe()` memoised: the answer is a property of the process, and confine()
/// runs once per spawn.
fn probe_cached() -> &'static Support {
    static PROBE: OnceLock<Support> = OnceLock::new();
    PROBE.get_or_init(probe)
}

/// The directories the resolved program binary lives in (literal and real, so a
/// symlinked `/opt/homebrew/bin/x` also opens its `Cellar` target): a declared
/// bin installed anywhere must be able to load itself and its sibling files.
fn program_dirs(program: &std::ffi::OsStr) -> Vec<PathBuf> {
    let p = Path::new(program);
    let found: Option<PathBuf> = if program.to_string_lossy().contains('/') {
        Some(p.to_path_buf())
    } else {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path).map(|d| d.join(p)).find(|c| c.is_file())
        })
    };
    let mut out = Vec::new();
    if let Some(f) = found {
        for cand in [Some(f.clone()), std::fs::canonicalize(&f).ok()].into_iter().flatten() {
            if let Some(d) = cand.parent() {
                let d = d.to_path_buf();
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
    }
    out
}

/// The child's working directory (literal and real) as a read-data literal.
/// With a scrubbed environment there is no `$PWD`, so `getcwd(3)` must open the
/// directory itself; without this every confined shell prints
/// "getcwd: cannot access parent directories". Literal only: the directory's
/// entry names become listable, its files' contents do not.
fn cwd_literals(cwd: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(c) = cwd {
        out.push(c.to_path_buf());
        if let Ok(r) = std::fs::canonicalize(c) {
            if r != c {
                out.push(r);
            }
        }
    }
    out
}

/// Rewrites `cmd` in place. NOTE: `Command` does not expose whether
/// `env_clear()` was called, so the rewritten command copies only explicit env
/// changes. Callers that scrub the environment must therefore apply it AFTER
/// `confine` (perch's `confine_spawn` contract). argv[0] becomes the program
/// path (`arg0` is not observable either).
pub fn confine(cmd: &mut Command, scopes: &Scopes) -> Result<(), ConfineError> {
    if let Support::Unsupported(r) = probe_cached() {
        return Err(ConfineError::Unsupported(r.clone()));
    }
    let program = cmd.get_program().to_os_string();
    let cwd = cmd.get_current_dir().map(Path::to_path_buf).or_else(|| std::env::current_dir().ok());
    let profile = profile(scopes, &program_dirs(&program), &cwd_literals(cwd.as_deref()));
    let args: Vec<OsString> = cmd.get_args().map(|a| a.to_os_string()).collect();
    // Command cannot swap its program in place; rebuild it and copy the
    // settings we can observe (cwd, env changes). Stdio is configured by the
    // caller after confine(), as with any Command.
    let mut n = Command::new(SANDBOX_EXEC);
    n.arg("-p").arg(profile).arg(program).args(args);
    if let Some(d) = cmd.get_current_dir() {
        n.current_dir(d);
    }
    for (k, v) in cmd.get_envs() {
        match v {
            Some(v) => {
                n.env(k, v);
            }
            None => {
                n.env_remove(k);
            }
        }
    }
    *cmd = n;
    Ok(())
}

/// Escapes a string for a SBPL double-quoted literal.
pub(crate) fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c => o.push(c),
        }
    }
    o
}

fn subpaths(paths: &[std::path::PathBuf]) -> String {
    // Both the literal given path and its canonical form: macOS resolves
    // symlinks (/tmp, /var) before matching.
    let mut out = Vec::new();
    for p in paths {
        let s = p.to_string_lossy().to_string();
        if let Ok(c) = std::fs::canonicalize(p) {
            let c = c.to_string_lossy().to_string();
            if c != s {
                out.push(c);
            }
        }
        out.push(s);
    }
    out.iter().map(|s| format!("(subpath \"{}\")", escape(s))).collect::<Vec<_>>().join(" ")
}

/// Pure profile generator (testable on any OS). `exec_dirs` are the real
/// directories of the program being launched: readable (and, since
/// `process-exec*` is global, executable) but never writable. `cwd_dirs` get a
/// non-recursive `file-read-data` so `getcwd(3)` works.
pub fn profile(scopes: &Scopes, exec_dirs: &[PathBuf], cwd_dirs: &[PathBuf]) -> String {
    let mut p = String::new();
    p.push_str("(version 1)\n(deny default)\n");
    p.push_str("(allow process-exec*)\n(allow process-fork)\n(allow signal (target self))\n");
    p.push_str("(allow sysctl-read)\n(allow mach-lookup)\n(allow ipc-posix-shm-read*)\n");
    p.push_str("(allow file-read-metadata)\n");
    // Minimum system reads for ordinary binaries: dyld, libs, frameworks,
    // system tools, config basics, timezone, devices.
    p.push_str(
        "(allow file-read*\n  (subpath \"/usr/lib\") (subpath \"/usr/bin\") (subpath \"/usr/sbin\") (subpath \"/usr/libexec\") (subpath \"/usr/share\")\n  \
(subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") (subpath \"/Library/Apple\") (subpath \"/Library/Preferences\")\n  \
(subpath \"/Library/Developer/CommandLineTools\") (subpath \"/private/etc\") (subpath \"/etc\")\n  \
(subpath \"/private/var/db/timezone\") (subpath \"/private/var/db/dyld\") (subpath \"/var/db/timezone\")\n  \
(literal \"/\") (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/urandom\") (literal \"/dev/random\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n",
    );
    p.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n");
    let mut reads = scopes.read.clone();
    reads.extend(scopes.write.iter().cloned());
    reads.extend(exec_dirs.iter().cloned());
    if !reads.is_empty() {
        p.push_str(&format!("(allow file-read* {})\n", subpaths(&reads)));
    }
    if !cwd_dirs.is_empty() {
        let lits: Vec<String> = cwd_dirs.iter().map(|d| format!("(literal \"{}\")", escape(&d.to_string_lossy()))).collect();
        p.push_str(&format!("(allow file-read-data {})\n", lits.join(" ")));
    }
    if !scopes.write.is_empty() {
        p.push_str(&format!("(allow file-write* {})\n", subpaths(&scopes.write)));
    }
    if !scopes.hosts.is_empty() {
        // SBPL cannot filter by hostname: best effort, advisory for the host.
        p.push_str("(allow network-outbound)\n(allow network-inbound (local ip))\n(allow system-socket)\n");
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(escape(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escape("a\nb"), "a\\nb");
    }

    #[test]
    fn profile_shape() {
        let s = Scopes { read: vec![PathBuf::from("/r\"x")], write: vec![PathBuf::from("/w")], hosts: vec![] };
        let p = profile(&s, &[], &[]);
        assert!(p.starts_with("(version 1)\n(deny default)"));
        assert!(p.contains(r#"(subpath "/r\"x")"#));
        assert!(p.contains("(allow file-write* (subpath \"/w\"))"));
        assert!(!p.contains("network-outbound"));
        let mut s2 = s.clone();
        s2.hosts.push("example.com".into());
        assert!(profile(&s2, &[], &[]).contains("(allow network-outbound)"));
    }

    #[test]
    fn default_reads_include_homebrew_and_usr_local_read_only() {
        let p = profile(&Scopes { write: vec![PathBuf::from("/w")], ..Default::default() }, &[], &[]);
        assert!(p.contains("(subpath \"/opt/homebrew\")") && p.contains("(subpath \"/usr/local\")"));
        // They sit in the file-read* allow-list; the only file-write* grants are
        // the fixed devices and the declared write root.
        let writes: Vec<&str> = p.lines().filter(|l| l.starts_with("(allow file-write*")).collect();
        assert_eq!(writes.len(), 2, "{writes:?}");
        assert!(writes.iter().all(|l| !l.contains("/opt/homebrew") && !l.contains("/usr/local")));
    }

    #[test]
    fn program_dir_is_readable_never_writable() {
        let s = Scopes { write: vec![PathBuf::from("/w")], ..Default::default() };
        let p = profile(&s, &[PathBuf::from("/srv/tools/bin")], &[PathBuf::from("/work/cwd")]);
        assert!(p.contains("(allow file-read-data (literal \"/work/cwd\"))"));
        let read_line = p.lines().find(|l| l.contains("/srv/tools/bin")).unwrap();
        assert!(read_line.starts_with("(allow file-read*"));
        assert!(p.lines().filter(|l| l.starts_with("(allow file-write*")).all(|l| !l.contains("/srv/tools")));
    }

    #[test]
    fn program_dirs_resolves_symlink_target() {
        let d = std::env::temp_dir().join(format!("perch-macos-pd-{}", std::process::id()));
        let real = d.join("real");
        let link = d.join("link");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&link).unwrap();
        std::fs::write(real.join("tool"), "x").unwrap();
        std::os::unix::fs::symlink(real.join("tool"), link.join("tool")).unwrap();
        let dirs = program_dirs(link.join("tool").as_os_str());
        let canon_real = std::fs::canonicalize(&real).unwrap();
        assert!(dirs.contains(&link) && dirs.contains(&canon_real), "{dirs:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}

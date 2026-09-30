//! macOS backend: rewrite the spawn to `sandbox-exec -p <profile> prog args…`.
use super::{ConfineError, Scopes, Support};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

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

pub fn confine(cmd: &mut Command, scopes: &Scopes) -> Result<(), ConfineError> {
    if let Support::Unsupported(r) = probe() {
        return Err(ConfineError::Unsupported(r));
    }
    let profile = profile(scopes);
    let program = cmd.get_program().to_os_string();
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

/// Pure profile generator (testable on any OS).
pub fn profile(scopes: &Scopes) -> String {
    let mut p = String::new();
    p.push_str("(version 1)\n(deny default)\n");
    p.push_str("(allow process-exec*)\n(allow process-fork)\n(allow signal (target self))\n");
    p.push_str("(allow sysctl-read)\n(allow mach-lookup)\n(allow ipc-posix-shm-read*)\n");
    p.push_str("(allow file-read-metadata)\n");
    // Minimum system reads for ordinary binaries: dyld, libs, frameworks,
    // system tools, config basics, timezone, devices.
    p.push_str(
        "(allow file-read*\n  (subpath \"/usr/lib\") (subpath \"/usr/bin\") (subpath \"/usr/sbin\") (subpath \"/usr/libexec\") (subpath \"/usr/share\")\n  \
(subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/Library/Apple\") (subpath \"/Library/Preferences\")\n  \
(subpath \"/Library/Developer/CommandLineTools\") (subpath \"/private/etc\") (subpath \"/etc\")\n  \
(subpath \"/private/var/db/timezone\") (subpath \"/private/var/db/dyld\") (subpath \"/var/db/timezone\")\n  \
(literal \"/\") (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/urandom\") (literal \"/dev/random\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n",
    );
    p.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n");
    let mut reads = scopes.read.clone();
    reads.extend(scopes.write.iter().cloned());
    if !reads.is_empty() {
        p.push_str(&format!("(allow file-read* {})\n", subpaths(&reads)));
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
        let p = profile(&s);
        assert!(p.starts_with("(version 1)\n(deny default)"));
        assert!(p.contains(r#"(subpath "/r\"x")"#));
        assert!(p.contains("(allow file-write* (subpath \"/w\"))"));
        assert!(!p.contains("network-outbound"));
        let mut s2 = s.clone();
        s2.hosts.push("example.com".into());
        assert!(profile(&s2).contains("(allow network-outbound)"));
    }
}

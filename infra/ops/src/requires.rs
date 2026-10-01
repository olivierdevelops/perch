//! File-declared requirements enforcement.
//!
//! When a `requires ... end` block is declared at file scope, the interpreter
//! switches to a strict-manifest model: any shell binary, HTTP host, or env-var
//! the program touches must be enumerated in the block. Undeclared use surfaces
//! as bin_not_declared / host_not_declared / env_not_declared so static analysis
//! (and the runtime) can refuse the operation.
//!
//! Three responsibilities live here:
//!
//!  1. Preflight — runs once per command invocation BEFORE any op fires.
//!     Verifies every required bin is on PATH (and, when a hash is pinned, that
//!     its bytes match), every required env var is set, and the host OS/arch is
//!     on the declared list. NO version checking: that would require executing
//!     the binary before the sandbox exists, and a trojaned binary lies about
//!     its version. Hash pinning needs no execution and pins the exact artifact.
//!
//!  2. Per-op enforcement — `check_shell_bin_declared`, `check_host_declared`,
//!     and `check_env_declared` consult the parsed Requirements.
//!
//!  3. Helpers used by --check / simulate to pre-validate without running.
use crate::common::{arg_string, go_clean, go_dir, go_join, go_io_msg, is_abs, look_path};
use crate::seams::bundle_read_file;
use perch_domain::{BinReq, ErrorKind, OpError, Program};
use perch_interpreter::{
    go_arch, go_os, handler, interpolate, Args, Bindings, Handler, Interpreter, Result,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;

fn unmet(msg: String, detail: &str) -> OpError {
    OpError::new("requires", ErrorKind::RequirementUnmet, &msg).with_detail(detail)
}

/// Runs all `requires`-block checks. Returns the FIRST failure as an OpError
/// tagged requirement_unmet (with detail naming the missing piece). Caller is
/// the interpreter, just after `parse_args` succeeds. No-op when the program
/// has no declared requirements.
pub fn preflight(_i: &Interpreter, prog: &Program) -> Result<()> {
    preflight_program(prog).map_err(Into::into)
}

fn preflight_program(prog: &Program) -> std::result::Result<(), OpError> {
    if !prog.requirements.declared {
        return Ok(());
    }
    let r = &prog.requirements;
    let goos = go_os();
    let goarch = go_arch();

    // OS / arch first — fastest, no IO.
    if !r.os.is_empty() && !in_list(&r.os, goos) && !matches_unix(&r.os) {
        return Err(unmet(
            format!("host OS {:?} not in declared list: {}", goos, r.os.join(", ")),
            goos,
        ));
    }
    if !r.arch.is_empty() && !in_list(&r.arch, goarch) {
        return Err(unmet(
            format!("host arch {:?} not in declared list: {}", goarch, r.arch.join(", ")),
            goarch,
        ));
    }

    // Required env vars.
    for e in &r.envs {
        if e.optional {
            continue;
        }
        if std::env::var(&e.name).unwrap_or_default().is_empty() {
            return Err(unmet(format!("required env var {:?} is not set", e.name), &e.name));
        }
    }

    // Required bins (existence + optional hash pin — no execution).
    for bin in &r.bins {
        let mut bin = bin.clone();
        // Interpolated names (`bin "${TOOL}"`) can't be resolved at preflight —
        // defer to the runtime guard, like interpolated hosts/paths.
        if bin.name.contains("${") {
            continue;
        }
        let path: String;
        if bin.name.contains(['/', '\\']) {
            // Path-form bin: check the file exists on disk instead of a PATH
            // lookup. A relative path is resolved against the .perch script
            // directory (where it was declared), not the process cwd.
            let mut p = bin.name.clone();
            if !is_abs(&p) && !prog.script_path.is_empty() && prog.script_path != "-" {
                p = go_join(&[&go_dir(&prog.script_path), &p]);
            }
            match std::fs::metadata(&p) {
                Ok(st) if !st.is_dir() => {}
                _ => {
                    if bin.optional {
                        continue;
                    }
                    return Err(unmet(format!("required bin path {:?} not found", bin.name), &bin.name));
                }
            }
            path = p;
        } else {
            match look_path(&bin.name) {
                Some(p) => path = p.to_string_lossy().into_owned(),
                None => {
                    if bin.optional {
                        continue;
                    }
                    return Err(unmet(format!("required bin {:?} not found on PATH", bin.name), &bin.name));
                }
            }
        }
        // Hash pin. Inline `hash "..."` and external `hash_file "PATH"` both
        // feed into the same comparison; this is a read-only check — perch
        // reads the binary's bytes, never runs it.
        if !bin.hash_file.is_empty() {
            match load_hash_file(prog, &bin.hash_file) {
                Ok(loaded) => {
                    if bin.hash.is_empty() {
                        bin.hash = loaded;
                    } else if normalize_hash(&bin.hash) != normalize_hash(&loaded) {
                        return Err(unmet(
                            format!("bin {:?}: inline hash and hash_file disagree", bin.name),
                            &bin.name,
                        ));
                    }
                }
                Err(e) => {
                    if bin.optional {
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        if !bin.hash.is_empty() {
            if let Err(e) = check_bin_hash(&bin, &path) {
                if bin.optional {
                    continue;
                }
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Hashes the resolved binary file and compares it against the declared pin.
/// Format is "ALGO:HEX". Only sha256 is supported today.
pub fn check_bin_hash(bin: &BinReq, path: &str) -> std::result::Result<(), OpError> {
    let Some((algo, want)) = bin.hash.split_once(':') else {
        return Err(unmet(
            format!("bin {:?}: hash must be ALGO:HEX (got {:?})", bin.name, bin.hash),
            &bin.name,
        ));
    };
    let algo = algo.trim().to_lowercase();
    let want = want.trim().to_lowercase();
    if algo != "sha256" {
        return Err(unmet(
            format!("bin {:?}: unsupported hash algorithm {:?} (only sha256)", bin.name, algo),
            &bin.name,
        ));
    }
    let mut f = std::fs::File::open(path).map_err(|e| {
        unmet(
            format!("bin {:?}: cannot open {} for hashing: open {}: {}", bin.name, path, path, go_io_msg(&e)),
            &bin.name,
        )
    })?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 32 * 1024];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(e) => {
                return Err(unmet(
                    format!("bin {:?}: read error while hashing: {}", bin.name, go_io_msg(&e)),
                    &bin.name,
                ))
            }
        }
    }
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != want {
        return Err(unmet(
            format!(
                "bin {:?}: hash mismatch\n  expected sha256:{}\n  got      sha256:{}\n  path     {}",
                bin.name, want, got, path
            ),
            &format!("{} sha256:{}", bin.name, got),
        ));
    }
    Ok(())
}

/// Resolves `bin.hash_file` to a hash string. Two path forms:
///
///  - "bundle:RELPATH" — read from the embedded bundle archive.
///  - regular path    — read from the filesystem, resolved relative to the
///    .perch script directory (so checksums files travel with the source).
///
/// The file's first whitespace-delimited token is taken as the hash.
pub fn load_hash_file(prog: &Program, reference: &str) -> std::result::Result<String, OpError> {
    let raw: Vec<u8>;
    if let Some(entry) = reference.strip_prefix("bundle:") {
        match bundle_read_file(entry) {
            Some(Ok(data)) => raw = data,
            Some(Err(e)) => {
                return Err(unmet(format!("hash_file: read bundle entry {:?}: {}", entry, e), reference))
            }
            None => return Err(unmet(format!("hash_file: bundle entry {:?} not found", entry), reference)),
        }
    } else {
        let mut path = reference.to_string();
        if !is_abs(&path) && !prog.script_path.is_empty() {
            path = go_join(&[&go_dir(&prog.script_path), &path]);
        }
        match std::fs::read(&path) {
            Ok(d) => raw = d,
            Err(e) => {
                return Err(unmet(
                    format!("hash_file: read {}: open {}: {}", path, path, go_io_msg(&e)),
                    reference,
                ))
            }
        }
    }
    // Take the first whitespace-delimited token. Supports plain `<hex>` files,
    // `sha256:<hex>` files, and shasum-format `<hex>  filename`.
    let text = String::from_utf8_lossy(&raw).into_owned();
    let mut tok = text.trim();
    if let Some(i) = tok.find([' ', '\t', '\n', '\r']) {
        tok = &tok[..i];
    }
    if tok.is_empty() {
        return Err(unmet(format!("hash_file: {:?} is empty", reference), reference));
    }
    if !tok.contains(':') {
        // Bare hex — default to sha256.
        return Ok(format!("sha256:{tok}"));
    }
    Ok(tok.to_string())
}

/// Lowercases the hash so comparison between inline `hash` and `hash_file` is
/// case-insensitive.
fn normalize_hash(s: &str) -> String {
    s.trim().to_lowercase()
}

fn declared(i: &Interpreter) -> bool {
    i.program.requirements.declared
}

/// Verifies that `bin` (the first token of a shell command) is in the program's
/// declared bin list. No-op when no requires block is declared.
pub fn check_shell_bin_declared(i: &Interpreter, raw: &str) -> std::result::Result<(), OpError> {
    if !declared(i) {
        return Ok(());
    }
    let bin = first_shell_token(raw);
    if bin.is_empty() {
        return Ok(());
    }
    if i.program.requirements.bins.iter().any(|b| b.name == bin) {
        return Ok(());
    }
    // Also allow shell built-ins (the user can't really declare these).
    if is_shell_builtin(&bin) {
        return Ok(());
    }
    Err(OpError::new("shell", ErrorKind::BinNotDeclared, &format!("bin {:?} is not declared in `requires`", bin))
        .with_detail(bin))
}

fn is_shell_builtin(name: &str) -> bool {
    matches!(name, "echo" | "cd" | "true" | "false" | "pwd" | ":" | "set" | "unset" | "export" | "test" | "[")
}

/// Verifies `host` matches one of the declared hosts (exact match, or matches a
/// `*.suffix` wildcard).
pub fn check_host_declared(i: &Interpreter, host: &str) -> std::result::Result<(), OpError> {
    if !declared(i) {
        return Ok(());
    }
    let host = host.to_lowercase();
    for h in &i.program.requirements.hosts {
        let want = h.name.to_lowercase();
        if want == host {
            return Ok(());
        }
        if want.starts_with("*.") && host.ends_with(&want[1..]) {
            return Ok(());
        }
    }
    Err(OpError::new("http", ErrorKind::HostNotDeclared, &format!("host {:?} is not declared in `requires`", host))
        .with_detail(host))
}

/// Verifies that `name` was listed under `requires env`.
pub fn check_env_declared(i: &Interpreter, name: &str) -> std::result::Result<(), OpError> {
    if !declared(i) {
        return Ok(());
    }
    if i.program.requirements.envs.iter().any(|e| e.name == name) {
        return Ok(());
    }
    Err(OpError::new(
        "get_env",
        ErrorKind::EnvNotDeclared,
        &format!("env var {:?} is not declared in `requires`", name),
    )
    .with_detail(name))
}

/// Gates network ops that don't name a specific host (public_ip, local_ip,
/// interfaces, mac_address, port_free, find_free_port). When a requires block
/// is present, the program must have declared at least one `host` — or the op
/// is refused. No-op when no requires block is declared.
pub fn check_net_declared(i: &Interpreter) -> std::result::Result<(), OpError> {
    if !declared(i) || !i.program.requirements.hosts.is_empty() {
        return Ok(());
    }
    Err(OpError::new(
        "net",
        ErrorKind::HostNotDeclared,
        "network access is not declared in `requires` (declare a `host`)",
    ))
}

/// Gates ops that spawn an external program WITHOUT going through the `shell`
/// op (pkg_install, bin_version, os_version, kill_by_name, …). The spawned
/// binary must be in the declared bin list, exactly like a `shell` first token.
pub fn check_subprocess_bin(i: &Interpreter, name: &str) -> std::result::Result<(), OpError> {
    if !declared(i) || name.is_empty() {
        return Ok(());
    }
    let base = match name.rfind(['/', '\\']) {
        Some(idx) => &name[idx + 1..],
        None => name,
    };
    if i.program.requirements.bins.iter().any(|b| b.name == base) {
        return Ok(());
    }
    Err(OpError::new(
        "subprocess",
        ErrorKind::BinNotDeclared,
        &format!("subprocess bin {:?} is not declared in `requires`", base),
    )
    .with_detail(base))
}

/// Gates the `exec` op: when a `requires` block is present, the binary must be
/// a declared `bin "…"` or the op fails bin_not_declared.
pub fn check_exec_bin(i: &Interpreter, name: &str) -> std::result::Result<(), OpError> {
    if !declared(i) || name.is_empty() {
        return Ok(());
    }
    if i.program.requirements.bin_allowed(name) {
        return Ok(());
    }
    Err(OpError::new("exec", ErrorKind::BinNotDeclared, &format!("bin {:?} is not declared in `requires`", name))
        .with_detail(name))
}

/// Maps the bin token the user typed to the executable to actually spawn. A
/// declared alias resolves to its path; a relative aliased path is resolved
/// against the .perch script directory. Plain bins and non-aliased literal
/// paths pass through unchanged.
pub fn resolve_exec_path(i: &Interpreter, name: &str) -> String {
    for b in &i.program.requirements.bins {
        if b.alias.is_empty() || name != b.alias {
            continue;
        }
        let p = &b.name;
        if p.contains(['/', '\\'])
            && !is_abs(p)
            && !i.program.script_path.is_empty()
            && i.program.script_path != "-"
        {
            return go_join(&[&go_dir(&i.program.script_path), p]);
        }
        return p.clone();
    }
    name.to_string()
}

/// Extracts the hostname from a URL for host-allowlist checks.
pub fn host_of_url(u: &str) -> String {
    let mut s = u;
    if let Some(idx) = s.find("://") {
        s = &s[idx + 3..];
    }
    if let Some(idx) = s.find(['/', '?', '#']) {
        s = &s[..idx];
    }
    if let Some(idx) = s.rfind('@') {
        s = &s[idx + 1..];
    }
    if let Some(idx) = s.rfind(':') {
        s = &s[..idx];
    }
    s.to_string()
}

/// The basename of the first non-assignment token of a shell command.
pub fn first_shell_token(raw: &str) -> String {
    for f in raw.split_whitespace() {
        // Skip `KEY=VAL` prefixes like `GOOS=linux`.
        if let Some(eq) = f.find('=') {
            let before = &f[..eq];
            if !before.is_empty() && before == before.to_uppercase() {
                continue;
            }
        }
        return match f.rfind(['/', '\\']) {
            Some(idx) => f[idx + 1..].to_string(),
            None => f.to_string(),
        };
    }
    String::new()
}

fn in_list(xs: &[String], want: &str) -> bool {
    xs.iter().any(|x| x == want)
}

/// True when the declared OS list contains "unix" and the host is a unix.
fn matches_unix(list: &[String]) -> bool {
    in_list(list, "unix") && matches!(go_os(), "darwin" | "linux" | "freebsd" | "openbsd" | "netbsd")
}

// ─── Filesystem path gating (read / write roots) ──────────────────────────
//
// The filesystem is an external resource like bins / hosts / env, so when a
// `requires` block is present every filesystem op's path must fall inside a
// declared `read` / `write` root — otherwise it errors with read_not_declared /
// write_not_declared. A write root implies read on the same tree.

type Keys = &'static [&'static [&'static str]];

/// Which arg keys hold write paths and which hold read paths per fs op; each
/// inner slice is an `arg_string` fallback chain.
struct FsPathSpec {
    kind: &'static str,
    write_keys: Keys,
    read_keys: Keys,
}

const P0: &[&str] = &["path", "_0"];
const SRC01: &[&str] = &["src", "_0"];
const DST01: &[&str] = &["dst", "_1"];

const FS_PATH_OPS: &[FsPathSpec] = &[
    // write-only (single path)
    FsPathSpec { kind: "mkdir", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "rm", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "touch", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "chmod", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "write_file", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "append_file", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "append_line", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "ensure_dir", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "make_executable", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "ensure_line_in_file", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "replace_in_file", write_keys: &[P0], read_keys: &[] },
    FsPathSpec { kind: "symlink", write_keys: &[&["link", "_1"]], read_keys: &[] },
    // read+write (src read, dst write)
    FsPathSpec { kind: "cp", write_keys: &[DST01], read_keys: &[SRC01] },
    FsPathSpec { kind: "mv", write_keys: &[DST01], read_keys: &[SRC01] },
    FsPathSpec { kind: "copy_dir", write_keys: &[DST01], read_keys: &[SRC01] },
    FsPathSpec { kind: "backup_file", write_keys: &[P0], read_keys: &[P0] },
    // read-only
    FsPathSpec { kind: "read_file", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "exists", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "is_dir", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "is_file", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "file_size", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "list_dir", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "read_link", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "sha256_file", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "md5_file", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "glob", write_keys: &[], read_keys: &[&["pattern", "_0"]] },
    // archive / compression — read src, write dst
    FsPathSpec { kind: "tar_create", write_keys: &[&["dst"]], read_keys: &[&["src"]] },
    FsPathSpec { kind: "tar_extract", write_keys: &[&["dst"]], read_keys: &[&["src"]] },
    FsPathSpec { kind: "zip_create", write_keys: &[&["dst"]], read_keys: &[&["src"]] },
    FsPathSpec { kind: "zip_extract", write_keys: &[&["dst"]], read_keys: &[&["src"]] },
    FsPathSpec { kind: "gzip", write_keys: &[&["dst"]], read_keys: &[&["src"]] },
    FsPathSpec { kind: "ungzip", write_keys: &[&["dst"]], read_keys: &[&["src"]] },
    // bundle extraction writes the destination tree
    FsPathSpec { kind: "bundle_extract", write_keys: &[&["dst", "_0"]], read_keys: &[] },
    // sha-of-file ops read the file
    FsPathSpec { kind: "sha1_file", write_keys: &[], read_keys: &[P0] },
    FsPathSpec { kind: "verify_sha256", write_keys: &[], read_keys: &[P0] },
];

/// Wraps every filesystem op so that, when a `requires` block is declared, its
/// read/write paths are checked against the declared roots before the handler
/// runs. No-op for programs without a requires block.
pub fn apply_requires_path_gating(m: &mut HashMap<String, Handler>) {
    for spec in FS_PATH_OPS {
        let Some(inner) = m.get(spec.kind).cloned() else { continue };
        let (write_keys, read_keys) = (spec.write_keys, spec.read_keys);
        m.insert(
            spec.kind.to_string(),
            handler(move |i: &Interpreter, b: &mut Bindings, args: &Args<'_>| {
                if i.program.requirements.declared {
                    for keys in write_keys {
                        let p = arg_string(args, keys);
                        if !p.is_empty() {
                            check_path_declared(i, b, &p, true)?;
                        }
                    }
                    for keys in read_keys {
                        let p = arg_string(args, keys);
                        if !p.is_empty() {
                            check_path_declared(i, b, &p, false)?;
                        }
                    }
                }
                inner(i, b, args)
            }),
        );
    }
}

/// Verifies `raw` is inside a declared root. Write paths must be within a
/// WriteRoot; read paths within a ReadRoot OR a WriteRoot (you may read what
/// you may write). Paths are cleaned + made absolute (relative to b.cwd).
pub fn check_path_declared(
    i: &Interpreter,
    b: &Bindings,
    raw: &str,
    is_write: bool,
) -> std::result::Result<(), OpError> {
    if !declared(i) || raw.is_empty() {
        return Ok(());
    }
    let r = &i.program.requirements;
    let abs = abs_under(raw, &b.cwd);
    let write_roots = expand_roots(&r.write_roots, b);
    if is_write {
        if path_within_any(&abs, &write_roots, &b.cwd) {
            return Ok(());
        }
        return Err(OpError::new(
            "fs",
            ErrorKind::WriteNotDeclared,
            &format!("write to {:?} is outside every declared `write` root in `requires`", raw),
        )
        .with_detail(raw));
    }
    if path_within_any(&abs, &expand_roots(&r.read_roots, b), &b.cwd) || path_within_any(&abs, &write_roots, &b.cwd) {
        return Ok(());
    }
    Err(OpError::new(
        "fs",
        ErrorKind::ReadNotDeclared,
        &format!("read of {:?} is outside every declared `read` root in `requires`", raw),
    )
    .with_detail(raw))
}

/// Interpolates `${…}` placeholders (script_dir, temp_dir, home, declared
/// globals, …) in each declared root against the live bindings. A root that
/// fails to interpolate is dropped: it can never match a concrete path, and
/// surfacing the gate error is clearer than an opaque interpolation failure.
fn expand_roots(roots: &[String], b: &Bindings) -> Vec<String> {
    roots.iter().filter_map(|root| interpolate(root, b).ok()).collect()
}

/// Resolves ONE declared root exactly as the file-op gate does
/// ([`expand_roots`] then [`abs_under`]): `${…}` interpolated against `b`,
/// relative paths made absolute under `b.cwd`. A root that fails to
/// interpolate falls back to its raw text under cwd (it then matches nothing
/// real, which is the safe direction for a confinement allow-list).
pub(crate) fn resolve_root(raw: &str, b: &Bindings) -> String {
    let expanded = interpolate(raw, b).unwrap_or_else(|_| raw.to_string());
    abs_under(&expanded, &b.cwd)
}

/// Cleans `p` and makes it absolute, resolving relatives under `cwd`.
pub fn abs_under(p: &str, cwd: &str) -> String {
    if !is_abs(p) {
        return go_join(&[cwd, p]);
    }
    go_clean(p)
}

/// Whether `abs` equals, or is nested under, any root (roots are themselves
/// cleaned + made absolute relative to `cwd`).
pub fn path_within_any(abs: &str, roots: &[String], cwd: &str) -> bool {
    roots.iter().any(|root| {
        let r = abs_under(root, cwd);
        abs == r || abs.starts_with(&format!("{r}/"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::{BinReq, Program};
    use std::path::PathBuf;

    pub fn tmpdir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!("perch-ops-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_temp(content: &str) -> (String, String) {
        let path = tmpdir("req").join("artifact.bin");
        std::fs::write(&path, content).unwrap();
        let sum: String = Sha256::digest(content.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        (path.to_string_lossy().into_owned(), sum)
    }

    fn bin(name: &str, hash: &str) -> BinReq {
        BinReq { name: name.into(), hash: hash.into(), ..Default::default() }
    }

    #[test]
    fn check_bin_hash_match() {
        let (path, sum) = write_temp("fake-binary-bytes");
        check_bin_hash(&bin("tool", &format!("sha256:{sum}")), &path).unwrap();
        // Case-insensitive on the hex digest.
        check_bin_hash(&bin("tool", &format!("sha256:{}", sum.to_uppercase())), &path).unwrap();
    }

    #[test]
    fn check_bin_hash_mismatch() {
        let (path, _) = write_temp("fake-binary-bytes");
        let err = check_bin_hash(&bin("tool", &format!("sha256:{}", "0".repeat(64))), &path).unwrap_err();
        assert_eq!(err.kind, ErrorKind::RequirementUnmet);
        assert!(err.to_string().contains("hash mismatch"), "{err}");
    }

    #[test]
    fn check_bin_hash_bad_format() {
        let (path, _) = write_temp("x");
        assert!(check_bin_hash(&bin("t", "deadbeef"), &path).is_err());
        let e = check_bin_hash(&bin("t", "md5:deadbeef"), &path).unwrap_err();
        assert!(e.to_string().contains("sha256"), "{e}");
    }

    #[test]
    fn load_hash_file_formats() {
        let dir = tmpdir("hf");
        let prog = Program { script_path: dir.join("commands.perch").to_string_lossy().into_owned(), ..Default::default() };
        let cases = [
            ("bare-hex", format!("{}\n", "a".repeat(64)), format!("sha256:{}", "a".repeat(64))),
            ("prefixed", format!("sha256:{}", "b".repeat(64)), format!("sha256:{}", "b".repeat(64))),
            ("shasum-line", format!("{}  ./tool\n", "c".repeat(64)), format!("sha256:{}", "c".repeat(64))),
        ];
        for (name, content, want) in cases {
            let fname = format!("{name}.sha256");
            std::fs::write(dir.join(&fname), content).unwrap();
            assert_eq!(load_hash_file(&prog, &fname).unwrap(), want, "{name}");
        }
    }

    #[test]
    fn load_hash_file_missing() {
        let prog = Program { script_path: tmpdir("hfm").join("commands.perch").to_string_lossy().into_owned(), ..Default::default() };
        assert!(load_hash_file(&prog, "does-not-exist.sha256").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn path_within_any_cases() {
        let cwd = "/work/project";
        let roots = vec!["./src".to_string(), "/abs/data".to_string()];
        for (path, want) in [
            ("/work/project/src", true),
            ("/work/project/src/main.go", true),
            ("/abs/data/file.txt", true),
            ("/abs/data", true),
            ("/work/project/other", false),
            ("/abs/database", false),
            ("/etc/passwd", false),
        ] {
            assert_eq!(path_within_any(&go_clean(path), &roots, cwd), want, "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn abs_under_dotdot_escape() {
        let cwd = "/work/project";
        let abs = abs_under("../secrets", cwd);
        assert_eq!(abs, "/work/secrets");
        assert!(!path_within_any(&abs, &["./data".to_string()], cwd));
    }
}

//! Install / uninstall / build helpers — the ops most needed when a .perch file
//! is shipped as a binary that has to bootstrap itself on the recipient's
//! machine. Everything here is cross-platform; on Windows the helpers degrade
//! gracefully (e.g. add_to_path prints instructions when it can't safely edit a
//! shell rc).
use crate::common::{arg_string, go_abs, go_base, go_io_msg, go_join, look_path, path_err, user_home_dir};
use crate::requires::check_subprocess_bin;
use perch_interpreter::{err, go_os, go_quote, handler, Args, Bindings, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

pub fn register_install(m: &mut HashMap<String, Handler>) {
    let mut reg = |k: &str, h: Handler| {
        m.insert(k.to_string(), h);
    };
    reg("which", handler(op_which));
    reg("has_bin", handler(op_has_bin));
    reg("bin_version", handler(op_bin_version));

    reg("path_contains", handler(op_path_contains));
    reg("shell_rc_path", handler(op_shell_rc_path));
    reg("add_to_path", handler(op_add_to_path));
    reg("link_into_path", handler(op_link_into_path));

    reg("detect_pkg_mgr", handler(op_detect_pkg_mgr));
    reg("pkg_install", handler(op_pkg_install));
    reg("pkg_installed", handler(op_pkg_installed));
    reg("pkg_uninstall", handler(op_pkg_uninstall));

    reg("is_admin", handler(op_is_admin));
    reg("is_ci", handler(op_is_ci));
    reg("is_tty", handler(op_is_tty));
}

fn sv(s: impl Into<String>) -> Result<Value> {
    Ok(Value::String(s.into()))
}

/// The absolute path to BIN on PATH, or "" if not found. `if has_bin "python3"`
/// is the truthy form; use `let p = which "python3"` for the full path.
fn op_which(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    if name.is_empty() {
        return sv("");
    }
    let Some(p) = look_path(&name) else { return sv("") };
    let p = p.to_string_lossy().into_owned();
    match go_abs(&p) {
        Ok(abs) => sv(abs),
        Err(_) => sv(p),
    }
}

fn op_has_bin(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    if name.is_empty() {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(look_path(&name).is_some()))
}

/// Runs `BIN --version` and returns the trimmed combined output. Empty string on
/// failure (no error — callers usually want a fallback).
fn op_bin_version(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    check_subprocess_bin(i, &name)?;
    let mut flag = arg_string(args, &["flag", "_1"]);
    if flag.is_empty() {
        flag = "--version".to_string();
    }
    let Some(path) = (if name.contains('/') { Some(name.clone().into()) } else { look_path(&name) }) else {
        return sv("");
    };
    match Command::new(path).arg(flag).output() {
        Ok(o) if o.status.success() => {
            let mut v = o.stdout;
            v.extend(o.stderr);
            sv(String::from_utf8_lossy(&v).trim())
        }
        _ => sv(""),
    }
}

fn path_dirs() -> Vec<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ';' } else { ':' };
    path.split(sep).map(String::from).collect()
}

fn op_path_contains(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let dir = arg_string(args, &["dir", "_0"]);
    if dir.is_empty() {
        return Ok(Value::Bool(false));
    }
    let want = go_abs(&dir).unwrap_or_default();
    Ok(Value::Bool(path_dirs().iter().any(|p| go_abs(p).is_ok_and(|got| got == want))))
}

/// The best-guess shell rc file for the current user — ~/.zshrc, ~/.bashrc, …
/// Picks based on $SHELL. Returns "" on Windows.
fn shell_rc_path() -> std::result::Result<String, String> {
    if go_os() == "windows" {
        return Ok(String::new());
    }
    let h = user_home_dir()?;
    let shell = go_base(&std::env::var("SHELL").unwrap_or_default());
    Ok(match shell.as_str() {
        "zsh" => go_join(&[&h, ".zshrc"]),
        "fish" => go_join(&[&h, ".config", "fish", "config.fish"]),
        // macOS conventionally uses .bash_profile for login shells.
        "bash" if go_os() == "darwin" => go_join(&[&h, ".bash_profile"]),
        "bash" => go_join(&[&h, ".bashrc"]),
        _ => go_join(&[&h, ".profile"]),
    })
}

fn op_shell_rc_path(_i: &Interpreter, _b: &mut Bindings, _args: &Args<'_>) -> Result<Value> {
    shell_rc_path().map(Value::String).map_err(err)
}

/// Idempotently appends `export PATH="DIR:$PATH"` to the user's shell rc if DIR
/// is not already on PATH and the line isn't already there. On Windows it prints
/// instructions instead of editing the registry.
fn op_add_to_path(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let dir = arg_string(args, &["dir", "_0"]);
    if dir.is_empty() {
        return Err(err("add_to_path: dir is required"));
    }
    let abs = go_abs(&dir).unwrap_or_default();
    if already_on_path(&abs) {
        return Ok(Value::Bool(false));
    }
    if go_os() == "windows" {
        let _ = i.stdout.write_str(&format!(
            "To add {} to PATH, run:\n  setx PATH \"%PATH%;{}\"\n",
            go_quote(&abs),
            abs
        ));
        return Ok(Value::Bool(false));
    }
    let rc_path = shell_rc_path().map_err(err)?;
    if rc_path.is_empty() {
        return Ok(Value::Bool(false));
    }
    let shell = go_base(&std::env::var("SHELL").unwrap_or_default());
    let line = if shell == "fish" { format!("set -gx PATH {abs} $PATH") } else { format!("export PATH=\"{abs}:$PATH\"") };
    let added = ensure_line_in_file(&rc_path, &line)?;
    if added {
        let _ = i.stdout.write_str(&format!(
            "Added {} to {} — start a new shell or `source {}`.\n",
            go_quote(&abs),
            rc_path,
            rc_path
        ));
    }
    Ok(Value::Bool(added))
}

/// Creates a symlink (or copies, on Windows) so SRC is reachable as
/// `BASENAME(SRC)` inside DIR. Useful for "drop a launcher in ~/.local/bin".
fn op_link_into_path(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let src = arg_string(args, &["src", "_0"]);
    let dir = arg_string(args, &["dir", "_1"]);
    if src.is_empty() || dir.is_empty() {
        return Err(err("link_into_path: src and dir are required"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| err(path_err("mkdir", &dir, &e)))?;
    let dst = go_join(&[&dir, &go_base(&src)]);
    let _ = std::fs::remove_file(&dst);
    if go_os() == "windows" {
        // Copy instead of symlink: Windows symlinks need elevation.
        copy_file(&src, &dst)?;
        return sv(dst);
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&src, &dst)
            .map_err(|e| err(format!("symlink {} {}: {}", src, dst, go_io_msg(&e))))?;
    }
    sv(dst)
}

/// "brew" / "apt" / "dnf" / "yum" / "pacman" / "apk" / "zypper" / "choco" /
/// "winget" / "scoop" — the first one found on PATH. Empty string if none.
fn detect_pkg_mgr() -> String {
    for c in pkg_mgr_candidates() {
        if look_path(c).is_some() {
            return c.to_string();
        }
    }
    String::new()
}

fn op_detect_pkg_mgr(_i: &Interpreter, _b: &mut Bindings, _args: &Args<'_>) -> Result<Value> {
    sv(detect_pkg_mgr())
}

fn pkg_mgr_candidates() -> &'static [&'static str] {
    match go_os() {
        "darwin" => &["brew", "port"],
        "linux" => &["apt", "dnf", "yum", "pacman", "apk", "zypper"],
        "windows" => &["winget", "choco", "scoop"],
        _ => &[],
    }
}

fn run_pkg(i: &Interpreter, cmd: &[String]) -> Result<Value> {
    check_subprocess_bin(i, &cmd[0])?;
    let mut c = Command::new(&cmd[0]);
    c.args(&cmd[1..]);
    let (e, _) = crate::process::run_cmd(
        &mut c,
        crate::process::Src::Reader(&i.stdin),
        crate::process::Dest::Writer(&i.stdout),
        crate::process::Dest::Writer(&i.stderr),
    );
    match e {
        None => Ok(Value::Null),
        Some(crate::process::RunErr::Exit { text, .. }) => Err(err(text)),
        Some(crate::process::RunErr::Other(m)) => Err(err(m)),
    }
}

/// Installs NAME using the detected package manager. The package name MUST match
/// the manager's catalog — there's no cross-manager name remapping.
fn op_pkg_install(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    if name.is_empty() {
        return Err(err("pkg_install: name is required"));
    }
    let Some(cmd) = pkg_install_cmd(&detect_pkg_mgr(), &name) else {
        return Err(err("pkg_install: no package manager found on PATH"));
    };
    run_pkg(i, &cmd)
}

fn op_pkg_uninstall(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    if name.is_empty() {
        return Err(err("pkg_uninstall: name is required"));
    }
    let Some(cmd) = pkg_uninstall_cmd(&detect_pkg_mgr(), &name) else {
        return Err(err("pkg_uninstall: no package manager found on PATH"));
    };
    run_pkg(i, &cmd)
}

fn op_pkg_installed(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    let Some(cmd) = pkg_installed_cmd(&detect_pkg_mgr(), &name) else {
        return Ok(Value::Bool(false));
    };
    check_subprocess_bin(i, &cmd[0])?;
    let ok = Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    Ok(Value::Bool(ok))
}

fn v(parts: &[&str], name: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
    out.push(name.to_string());
    Some(out)
}

fn pkg_install_cmd(mgr: &str, name: &str) -> Option<Vec<String>> {
    match mgr {
        "brew" => v(&["brew", "install"], name),
        "apt" => v(&["sudo", "apt-get", "install", "-y"], name),
        "dnf" => v(&["sudo", "dnf", "install", "-y"], name),
        "yum" => v(&["sudo", "yum", "install", "-y"], name),
        "pacman" => v(&["sudo", "pacman", "-S", "--noconfirm"], name),
        "apk" => v(&["sudo", "apk", "add"], name),
        "zypper" => v(&["sudo", "zypper", "install", "-y"], name),
        "winget" => v(&["winget", "install", "--silent", "-e", "--id"], name),
        "choco" => v(&["choco", "install", "-y"], name),
        "scoop" => v(&["scoop", "install"], name),
        _ => None,
    }
}

fn pkg_uninstall_cmd(mgr: &str, name: &str) -> Option<Vec<String>> {
    match mgr {
        "brew" => v(&["brew", "uninstall"], name),
        "apt" => v(&["sudo", "apt-get", "remove", "-y"], name),
        "dnf" | "yum" => v(&["sudo", mgr, "remove", "-y"], name),
        "pacman" => v(&["sudo", "pacman", "-R", "--noconfirm"], name),
        "apk" => v(&["sudo", "apk", "del"], name),
        "zypper" => v(&["sudo", "zypper", "remove", "-y"], name),
        "winget" => v(&["winget", "uninstall", "--silent", "-e", "--id"], name),
        "choco" => v(&["choco", "uninstall", "-y"], name),
        "scoop" => v(&["scoop", "uninstall"], name),
        _ => None,
    }
}

fn pkg_installed_cmd(mgr: &str, name: &str) -> Option<Vec<String>> {
    match mgr {
        "brew" => v(&["brew", "list"], name),
        "apt" => v(&["dpkg", "-s"], name),
        "dnf" | "yum" => v(&["rpm", "-q"], name),
        "pacman" => v(&["pacman", "-Qi"], name),
        "apk" => v(&["apk", "info", "-e"], name),
        _ => None,
    }
}

#[cfg(unix)]
fn geteuid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { geteuid() }
}

/// euid 0 on unix; on Windows, a probe via `net session`.
fn op_is_admin(_i: &Interpreter, _b: &mut Bindings, _args: &Args<'_>) -> Result<Value> {
    if go_os() == "windows" {
        let ok = Command::new("net")
            .arg("session")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        return Ok(Value::Bool(ok));
    }
    #[cfg(unix)]
    {
        Ok(Value::Bool(geteuid() == 0))
    }
    #[cfg(not(unix))]
    {
        Ok(Value::Bool(false))
    }
}

/// True if any common CI env var is set.
fn op_is_ci(_i: &Interpreter, _b: &mut Bindings, _args: &Args<'_>) -> Result<Value> {
    let set = ["CI", "GITHUB_ACTIONS", "GITLAB_CI", "CIRCLECI", "TRAVIS", "BUILDKITE", "JENKINS_URL", "TEAMCITY_VERSION"]
        .iter()
        .any(|k| !std::env::var(k).unwrap_or_default().is_empty());
    Ok(Value::Bool(set))
}

/// stdout looks like a terminal (vs piped / redirected). Mirrors Go's
/// `ModeCharDevice` check, so /dev/null counts as a terminal too.
fn op_is_tty(_i: &Interpreter, _b: &mut Bindings, _args: &Args<'_>) -> Result<Value> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        Ok(Value::Bool(std::fs::metadata("/dev/fd/1").map(|m| m.file_type().is_char_device()).unwrap_or(false)))
    }
    #[cfg(not(unix))]
    {
        use std::io::IsTerminal;
        Ok(Value::Bool(std::io::stdout().is_terminal()))
    }
}

// shared helpers (also used by the file ops) ---------------------------------

fn already_on_path(abs: &str) -> bool {
    path_dirs().iter().any(|p| go_abs(p).is_ok_and(|got| got == abs))
}

/// Appends LINE to the file at `path` if not already present. Returns true if
/// the line was added (false if it was already there).
pub fn ensure_line_in_file(path: &str, line: &str) -> Result<bool> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(err(path_err("open", path, &e))),
    };
    if contains_line(&String::from_utf8_lossy(&data), line) {
        return Ok(false);
    }
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(|e| err(path_err("open", path, &e)))?;
    let prefix = if !data.is_empty() && data[data.len() - 1] != b'\n' { "\n" } else { "" };
    f.write_all(format!("{prefix}{line}\n").as_bytes()).map_err(|e| err(path_err("write", path, &e)))?;
    Ok(true)
}

/// Whether any line of `haystack` (ignoring a trailing `\r`) equals `line`.
pub fn contains_line(haystack: &str, line: &str) -> bool {
    haystack.split('\n').any(|l| l.trim_end_matches('\r') == line)
}

/// Copies file contents from `src` to a newly created/truncated `dst`.
pub fn copy_file(src: &str, dst: &str) -> Result<()> {
    let mut input = std::fs::File::open(src).map_err(|e| err(path_err("open", src, &e)))?;
    let mut out = std::fs::File::create(dst).map_err(|e| err(path_err("open", dst, &e)))?;
    std::io::copy(&mut input, &mut out).map_err(|e| err(go_io_msg(&e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkg_commands() {
        assert_eq!(pkg_install_cmd("apt", "jq").unwrap(), ["sudo", "apt-get", "install", "-y", "jq"]);
        assert_eq!(pkg_uninstall_cmd("yum", "jq").unwrap(), ["sudo", "yum", "remove", "-y", "jq"]);
        assert!(pkg_installed_cmd("scoop", "jq").is_none());
        assert!(pkg_install_cmd("", "jq").is_none());
    }

    #[test]
    fn lines() {
        assert!(contains_line("a\r\nb\n", "a"));
        assert!(!contains_line("ab\n", "a"));
    }

    #[test]
    fn ensure_line_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("perch-ops-install-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("rc").to_string_lossy().into_owned();
        assert!(ensure_line_in_file(&f, "export X=1").unwrap());
        assert!(!ensure_line_in_file(&f, "export X=1").unwrap());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "export X=1\n");
    }
}

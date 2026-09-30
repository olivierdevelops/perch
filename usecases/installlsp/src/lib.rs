//! Installs the perch-lsp binary onto the user's machine via `go install`.
//!
//! Falls back to a clear actionable error if Go isn't on PATH (we don't
//! download pre-built binaries yet — the release workflow produces them at
//! github.com/olivierdevelops/perch/releases and a future revision will fetch
//! + sha256-verify them).
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Default)]
pub struct Impl;

/// The `@main` ref tracks the bleeding-edge branch. Once releases catch up
/// (perch-lsp landed after v0.1.0), this should switch back to `@latest`. The
/// release workflow also publishes pre-built perch-lsp binaries on each tag.
pub const MODULE_PATH: &str = "github.com/olivierdevelops/perch/cmd/perch-lsp@main";

/// The error text when `go` is missing from PATH.
pub const GO_NOT_FOUND: &str = "`go` not found on PATH. Install Go (https://go.dev/dl/) and retry, or download a perch-lsp binary from https://github.com/olivierdevelops/perch/releases";

/// Finds an executable named `name` on PATH (Go: `exec.LookPath`).
fn look_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if is_executable(&cand) {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let cand = dir.join(format!("{name}.exe"));
            if is_executable(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    let Ok(md) = std::fs::metadata(p) else { return false };
    if !md.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        md.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

impl Impl {
    /// Runs `go install` with the child's stdout/stderr inherited; progress
    /// lines go to `out`.
    pub fn execute(&self, out: &mut dyn Write) -> Result<(), Error> {
        let go_bin = look_path("go").ok_or_else(|| -> Error { GO_NOT_FOUND.into() })?;
        writeln!(out, "→ go install {MODULE_PATH}")?;
        out.flush()?;
        let status = Command::new(&go_bin)
            .arg("install")
            .arg(MODULE_PATH)
            .envs(std::env::vars_os())
            .status()
            .map_err(|e| -> Error { format!("go install failed: {e}").into() })?;
        if !status.success() {
            return Err(format!("go install failed: {status}").into());
        }

        // Best-effort: figure out where it landed and tell the user.
        match where_installed(&go_bin) {
            Some(loc) => writeln!(out, "✓ installed: {loc}")?,
            None => writeln!(
                out,
                "✓ perch-lsp installed (check `$(go env GOBIN)` or `$(go env GOPATH)/bin`)"
            )?,
        }
        Ok(())
    }
}

fn where_installed(go_bin: &Path) -> Option<String> {
    // Prefer GOBIN if set.
    for candidate in ["GOBIN", "GOPATH"] {
        let Ok(o) = Command::new(go_bin).args(["env", candidate]).output() else { continue };
        if !o.status.success() {
            continue;
        }
        let mut dir = trim(&String::from_utf8_lossy(&o.stdout)).to_string();
        if dir.is_empty() {
            continue;
        }
        if candidate == "GOPATH" {
            dir.push_str("/bin");
        }
        let path = format!("{dir}/perch-lsp");
        if Path::new(&path).exists() {
            return Some(path);
        }
    }
    None
}

/// Trims trailing newlines and spaces only (as the Go helper does).
fn trim(s: &str) -> &str {
    s.trim_end_matches(['\n', ' '])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_trailing_only() {
        assert_eq!(trim(" /x/bin \n\n"), " /x/bin");
        assert_eq!(trim(""), "");
    }

    #[test]
    fn go_missing_message_is_byte_exact() {
        assert_eq!(
            GO_NOT_FOUND,
            "`go` not found on PATH. Install Go (https://go.dev/dl/) and retry, or download a perch-lsp binary from https://github.com/olivierdevelops/perch/releases"
        );
    }

    #[test]
    fn look_path_finds_sh() {
        #[cfg(unix)]
        assert!(look_path("sh").is_some());
        assert!(look_path("definitely-not-a-binary-xyz").is_none());
    }
}

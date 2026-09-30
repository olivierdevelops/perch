//! Installs the perch VS Code extension. It:
//!
//!   1. ensures `perch-lsp` is installed (via the installlsp use case)
//!   2. extracts the embedded extension files into a temp directory
//!   3. runs `npm install` (silently) to fetch vscode-languageclient
//!   4. runs `vsce package` to produce a .vsix
//!   5. runs `code --install-extension <vsix>`
//!
//! Requires: go, node + npm, and the VS Code `code` CLI on PATH.
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub type InstallLSPFn = Box<dyn Fn() -> Result<(), Error> + Send + Sync>;

#[derive(Default)]
pub struct Impl {
    pub install_lsp: Option<InstallLSPFn>,
}

impl Impl {
    pub fn execute(&self) -> Result<(), Error> {
        require_binary("node")?;
        require_binary("npm")?;
        if let Err(e) = require_binary("code") {
            return Err(format!(
                "{e}\n\nThe VS Code `code` CLI isn't on PATH. From VS Code:\n  Cmd-Shift-P → 'Shell Command: Install code command in PATH'"
            )
            .into());
        }

        if let Some(f) = &self.install_lsp {
            println!("→ Installing perch-lsp");
            f().map_err(|e| -> Error { format!("install perch-lsp: {e}").into() })?;
        }

        let tmp = TempDir::new("perch-vscode-")?;

        println!("→ Extracting embedded extension to {}", tmp.path().display());
        perch_vscodeext::extract(tmp.path())
            .map_err(|e| -> Error { format!("extract embedded extension: {e}").into() })?;

        run(tmp.path(), "npm", &["install", "--silent", "--no-audit", "--no-fund"])
            .map_err(|e| -> Error { format!("npm install: {e}").into() })?;

        run(
            tmp.path(),
            "npx",
            &["--yes", "@vscode/vsce", "package", "--no-dependencies", "--skip-license", "-o", "perch.vsix"],
        )
        .map_err(|e| -> Error { format!("vsce package: {e}").into() })?;

        let vsix = tmp.path().join("perch.vsix");
        run(tmp.path(), "code", &["--install-extension", &vsix.to_string_lossy(), "--force"])
            .map_err(|e| -> Error { format!("code --install-extension: {e}").into() })?;

        println!();
        println!("✓ Installed. Open a .perch file to activate the extension.");
        println!("  If perch-lsp isn't on your PATH, set perch.lsp.path in VS Code settings.");
        Ok(())
    }
}

/// A temp directory removed on drop (Go: `os.MkdirTemp` + `defer os.RemoveAll`).
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> std::io::Result<TempDir> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("{prefix}{}{nanos}", std::process::id()));
        std::fs::create_dir(&p)?;
        Ok(TempDir(p))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Finds an executable named `name` on PATH (Go: `exec.LookPath`).
fn look_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|c| is_executable(c))
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

fn require_binary(name: &str) -> Result<(), Error> {
    if look_path(name).is_none() {
        return Err(format!("\"{name}\" is required but not on PATH").into());
    }
    Ok(())
}

/// Go's `%v` of a `[]string`: `[a b c]`.
fn go_slice(args: &[&str]) -> String {
    format!("[{}]", args.join(" "))
}

fn run(dir: &Path, name: &str, args: &[&str]) -> Result<(), Error> {
    println!("→ {name} {}", go_slice(args));
    let bin = look_path(name)
        .ok_or_else(|| -> Error { format!("exec: \"{name}\": executable file not found in $PATH").into() })?;
    let status = Command::new(bin).args(args).current_dir(dir).status()?;
    if status.success() {
        return Ok(());
    }
    match status.code() {
        Some(c) => Err(format!("exit status {c}").into()),
        None => Err(format!("{status}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_format_matches_go() {
        assert_eq!(go_slice(&["install", "--silent"]), "[install --silent]");
    }

    #[test]
    fn missing_binary_message() {
        let e = require_binary("definitely-not-a-real-bin-xyz").unwrap_err();
        assert_eq!(e.to_string(), "\"definitely-not-a-real-bin-xyz\" is required but not on PATH");
    }

    #[test]
    fn temp_dir_cleans_up() {
        let p;
        {
            let t = TempDir::new("perch-vscode-test-").unwrap();
            p = t.path().to_path_buf();
            assert!(p.is_dir());
        }
        assert!(!p.exists());
    }
}

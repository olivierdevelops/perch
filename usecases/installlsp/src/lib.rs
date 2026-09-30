//! Installs the perch-lsp binary onto the user's machine.
//!
//! Downloads the release asset `perch-lsp-<os>-<arch>[.exe]` and
//! `checksums.txt` from the latest GitHub release, verifies the asset's sha256
//! against `checksums.txt` (refusing on a mismatch or a missing entry), and
//! installs it next to the running `perch` executable when that directory is
//! writable, else into `~/.local/bin` (unix) / `%LOCALAPPDATA%\perch`
//! (windows).
//!
//! All network access goes through the injected `fetch` function (VHCO:
//! parameters, not globals); tests pass a fake.
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
/// GETs a URL and returns the body bytes.
pub type FetchFn = Box<dyn Fn(&str) -> Result<Vec<u8>, Error>>;

/// Base URL of the latest release's assets.
pub const RELEASE_BASE: &str = "https://github.com/olivierdevelops/perch/releases/latest/download";

pub struct Impl {
    pub fetch: FetchFn,
    /// Release-naming OS (`darwin`|`linux`|`windows`) or a raw Rust OS name;
    /// see [`release_os`].
    pub os: String,
    /// Raw Rust arch name (`x86_64`, `aarch64`, ...) or release name.
    pub arch: String,
    /// Install directory override (None: pick per the rules above).
    pub install_dir: Option<PathBuf>,
}

impl Impl {
    /// Targets the running platform and default install directory.
    pub fn new(fetch: FetchFn) -> Self {
        Impl {
            fetch,
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            install_dir: None,
        }
    }

    /// Downloads, verifies and installs; progress lines go to `out`.
    pub fn execute(&self, out: &mut dyn Write) -> Result<(), Error> {
        let (os, arch) = match (release_os(&self.os), release_arch(&self.arch)) {
            (Some(o), Some(a)) => (o, a),
            _ => {
                return Err(format!(
                    "no prebuilt perch-lsp for {}/{}; build it from source: cargo install --git https://github.com/olivierdevelops/perch perch-lsp",
                    self.os, self.arch
                )
                .into())
            }
        };
        let ext = if os == "windows" { ".exe" } else { "" };
        let asset = format!("perch-lsp-{os}-{arch}{ext}");

        writeln!(out, "→ downloading {asset} from {RELEASE_BASE}")?;
        out.flush()?;
        let body = (self.fetch)(&format!("{RELEASE_BASE}/{asset}"))
            .map_err(|e| -> Error { format!("could not download {asset}: {e} (are you online? nothing was installed)").into() })?;
        let sums = (self.fetch)(&format!("{RELEASE_BASE}/checksums.txt"))
            .map_err(|e| -> Error { format!("could not download checksums.txt: {e} (are you online? nothing was installed)").into() })?;

        let sums = String::from_utf8_lossy(&sums);
        let want = find_checksum(&sums, &asset)
            .ok_or_else(|| -> Error { format!("checksums.txt has no entry for {asset}; refusing to install").into() })?;
        let got = hex(&Sha256::digest(&body));
        if !got.eq_ignore_ascii_case(&want) {
            return Err(format!("sha256 mismatch for {asset}: expected {want}, got {got}; refusing to install").into());
        }
        writeln!(out, "✓ sha256 verified ({got})")?;

        let dir = match &self.install_dir {
            Some(d) => d.clone(),
            None => choose_install_dir(os)?,
        };
        std::fs::create_dir_all(&dir).map_err(|e| -> Error { format!("create {}: {e}", dir.display()) .into() })?;
        let dest = dir.join(format!("perch-lsp{ext}"));
        write_executable(&dest, &body)?;
        writeln!(out, "✓ installed: {}", dest.display())?;
        Ok(())
    }
}

/// Maps a Rust OS name to the release's; None when there is no asset.
pub fn release_os(os: &str) -> Option<&'static str> {
    match os {
        "macos" | "darwin" => Some("darwin"),
        "linux" => Some("linux"),
        "windows" => Some("windows"),
        _ => None,
    }
}

/// Maps a Rust arch name to the release's; None when there is no asset.
pub fn release_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" | "amd64" => Some("amd64"),
        "aarch64" | "arm64" => Some("arm64"),
        _ => None,
    }
}

/// Finds the hex digest for `asset` in `sha256sum`-style text
/// (`<hex>  <name>` or `<hex> *<name>`; a directory prefix is ignored).
pub fn find_checksum(text: &str, asset: &str) -> Option<String> {
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(sum), Some(name)) = (it.next(), it.next()) else { continue };
        let name = name.trim_start_matches('*');
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        if base == asset && sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(sum.to_ascii_lowercase());
        }
    }
    None
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".perch-write-test-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// The running executable's directory if writable, else the per-user dir.
fn choose_install_dir(os: &str) -> Result<PathBuf, Error> {
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        if dir_writable(&d) {
            return Ok(d);
        }
    }
    if os == "windows" {
        let base = std::env::var_os("LOCALAPPDATA").ok_or_else(|| -> Error { "LOCALAPPDATA is not set".into() })?;
        Ok(PathBuf::from(base).join("perch"))
    } else {
        let home = std::env::var_os("HOME").ok_or_else(|| -> Error { "HOME is not set".into() })?;
        Ok(PathBuf::from(home).join(".local").join("bin"))
    }
}

/// Writes via a temp file + rename so a failed install never leaves a partial
/// binary; chmod 0755 on unix.
fn write_executable(dest: &Path, body: &[u8]) -> Result<(), Error> {
    let tmp = dest.with_extension("tmp-install");
    std::fs::write(&tmp, body).map_err(|e| -> Error { format!("write {}: {e}", tmp.display()).into() })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| -> Error {
        let _ = std::fs::remove_file(&tmp);
        format!("install {}: {e}", dest.display()).into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn fake(asset: &[u8], sums: Option<String>, log: Arc<Mutex<Vec<String>>>) -> FetchFn {
        let asset = asset.to_vec();
        Box::new(move |url| {
            log.lock().unwrap().push(url.to_string());
            if url.ends_with("checksums.txt") {
                sums.clone().map(String::into_bytes).ok_or_else(|| "404".into())
            } else {
                Ok(asset.clone())
            }
        })
    }

    fn imp(fetch: FetchFn, dir: &Path) -> Impl {
        Impl { fetch, os: "macos".into(), arch: "aarch64".into(), install_dir: Some(dir.to_path_buf()) }
    }

    fn sums_for(name: &str, body: &[u8]) -> String {
        format!("{}  other\n{}  {name}\n", "0".repeat(64), hex(&Sha256::digest(body)))
    }

    #[test]
    fn success_installs_verified_binary() {
        let d = tempfile::tempdir().unwrap();
        let log = Arc::new(Mutex::new(vec![]));
        let body = b"#!/bin/sh\necho lsp\n";
        let i = imp(fake(body, Some(sums_for("perch-lsp-darwin-arm64", body)), log.clone()), d.path());
        let mut out = Vec::new();
        i.execute(&mut out).unwrap();
        let dest = d.path().join("perch-lsp");
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("sha256 verified") && s.contains("installed:"), "{s}");
        assert_eq!(
            log.lock().unwrap()[0],
            "https://github.com/olivierdevelops/perch/releases/latest/download/perch-lsp-darwin-arm64"
        );
    }

    #[test]
    fn t40_checksum_mismatch_refuses_and_installs_nothing() {
        let d = tempfile::tempdir().unwrap();
        let log = Arc::new(Mutex::new(vec![]));
        let sums = sums_for("perch-lsp-darwin-arm64", b"different bytes");
        let i = imp(fake(b"tampered", Some(sums), log), d.path());
        let e = i.execute(&mut Vec::new()).unwrap_err().to_string();
        assert!(e.contains("sha256 mismatch") && e.contains("refusing"), "{e}");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn missing_checksum_entry_refuses() {
        let d = tempfile::tempdir().unwrap();
        let log = Arc::new(Mutex::new(vec![]));
        let i = imp(fake(b"x", Some(sums_for("perch-lsp-linux-amd64", b"x")), log), d.path());
        let e = i.execute(&mut Vec::new()).unwrap_err().to_string();
        assert!(e.contains("no entry") && e.contains("perch-lsp-darwin-arm64"), "{e}");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn t41_offline_gives_clear_message() {
        let d = tempfile::tempdir().unwrap();
        let i = imp(Box::new(|_| Err("dns error: no network".into())), d.path());
        let e = i.execute(&mut Vec::new()).unwrap_err().to_string();
        assert!(e.contains("could not download") && e.contains("online") && e.contains("no network"), "{e}");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
        // checksums.txt failing alone is also clear
        let i = imp(
            Box::new(|u| if u.ends_with("checksums.txt") { Err("404".into()) } else { Ok(vec![1]) }),
            d.path(),
        );
        let e = i.execute(&mut Vec::new()).unwrap_err().to_string();
        assert!(e.contains("checksums.txt"), "{e}");
    }

    #[test]
    fn unsupported_platform_message() {
        let d = tempfile::tempdir().unwrap();
        let mut i = imp(Box::new(|_| panic!("must not fetch")), d.path());
        i.os = "freebsd".into();
        let e = i.execute(&mut Vec::new()).unwrap_err().to_string();
        assert!(e.contains("no prebuilt perch-lsp for freebsd/aarch64"), "{e}");
        i.os = "linux".into();
        i.arch = "riscv64".into();
        assert!(i.execute(&mut Vec::new()).unwrap_err().to_string().contains("linux/riscv64"));
    }

    #[test]
    fn windows_asset_has_exe() {
        let d = tempfile::tempdir().unwrap();
        let body = b"MZ";
        let log = Arc::new(Mutex::new(vec![]));
        let mut i = imp(fake(body, Some(sums_for("perch-lsp-windows-amd64.exe", body)), log), d.path());
        i.os = "windows".into();
        i.arch = "x86_64".into();
        i.execute(&mut Vec::new()).unwrap();
        assert!(d.path().join("perch-lsp.exe").exists());
    }

    #[test]
    fn checksum_parsing() {
        let h = "a".repeat(64);
        assert_eq!(find_checksum(&format!("{h} *dist/n\n"), "n"), Some(h.clone()));
        assert_eq!(find_checksum("short  n\n", "n"), None);
        assert_eq!(find_checksum(&format!("{h}  m\n"), "n"), None);
    }
}

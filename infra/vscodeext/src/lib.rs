//! Embeds the VS Code extension source files so that `perch --install-vscode`
//! can extract them to a temp directory and run npm + vsce +
//! `code --install-extension` without requiring the user to have a perch repo
//! checked out.
//!
//! The canonical copy of these files lives at editors/vscode-perch/. This crate
//! is a mirror (`template/`); keep them in sync (a CI guard is on the roadmap).
use std::io::Write;
use std::path::Path;

/// Every file under `template/`, as (relative path, contents). Rust has no
/// directory embed, so the list is explicit; the `embedded_list_matches_disk`
/// test fails if a template file is added without being listed here.
static FILES: &[(&str, &[u8])] = &[
    ("README.md", include_bytes!("../template/README.md")),
    ("extension.js", include_bytes!("../template/extension.js")),
    ("language-configuration.json", include_bytes!("../template/language-configuration.json")),
    ("package.json", include_bytes!("../template/package.json")),
    ("syntaxes/perch.tmLanguage.json", include_bytes!("../template/syntaxes/perch.tmLanguage.json")),
];

/// Copies the embedded VS Code extension into `dst` (must exist), preserving
/// directory structure under `template/`.
pub fn extract(dst: &Path) -> std::io::Result<()> {
    for (rel, data) in FILES {
        let out = dst.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::File::create(&out)
            .map_err(|e| std::io::Error::new(e.kind(), format!("create {}: {}", out.display(), e)))?;
        f.write_all(data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, base, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }

    #[test]
    fn embedded_list_matches_disk() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("template");
        let mut on_disk = vec![];
        walk(&base, &base, &mut on_disk);
        on_disk.sort();
        let mut listed: Vec<String> = FILES.iter().map(|(p, _)| p.to_string()).collect();
        listed.sort();
        assert_eq!(on_disk, listed);
    }

    #[test]
    fn extract_writes_tree() {
        let dst = std::env::temp_dir().join(format!("perch-vscodeext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        std::fs::create_dir_all(&dst).unwrap();
        extract(&dst).unwrap();
        assert!(dst.join("package.json").is_file());
        assert!(dst.join("syntaxes/perch.tmLanguage.json").is_file());
        assert_eq!(
            std::fs::read(dst.join("extension.js")).unwrap(),
            include_bytes!("../template/extension.js")
        );
    }
}

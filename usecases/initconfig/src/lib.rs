//! Writes a starter commands.perch in the current working directory if one
//! does not already exist.
use std::io::Write;
use std::path::Path;

#[derive(Default)]
pub struct Impl;

/// Go's `filepath.Base`: last element, "." for empty, "/" for all-slashes.
fn base(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return "/".into();
    }
    t.rsplit('/').next().unwrap().to_string()
}

impl Impl {
    pub fn execute(&self, path: &str, out: &mut dyn Write) -> std::io::Result<()> {
        if Path::new(path).exists() {
            writeln!(out, "File already exists at: {path}")?;
            return Ok(());
        }
        let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        // The shebang line makes the file directly executable once the user
        // `chmod +x`s it — `./commands.perch hello` runs without typing
        // `perch` first. Capy treats `#` lines as comments, so the shebang
        // has no effect on parsing; it's just a hint to the kernel + a hint
        // to the user that "this file is itself a program."
        let data = format!("{}\n", template(&base(&cwd)).trim());
        write_executable(path, data.as_bytes())?;
        writeln!(out, "✓ wrote {path}")?;
        writeln!(out)?;
        writeln!(out, "Try:")?;
        writeln!(out, "  perch -f {path} --help")?;
        writeln!(out, "  perch -f {path} hello")?;
        writeln!(out)?;
        writeln!(out, "Or run it as a script (perch must be on $PATH):")?;
        writeln!(out, "  chmod +x {path}")?;
        writeln!(out, "  ./{path}          # runs the `main` command")?;
        writeln!(out, "  ./{path} hello    # runs `hello` directly")?;
        Ok(())
    }
}

fn template(name: &str) -> String {
    format!(
        r#"#!/usr/bin/env perch
name    "{name}"
about   "A perch project"
version "0.1.0"

# Shared bindings are declared bare at top level (no globals block).
verbose = false

# The manifest is mandatory. An empty block means "pure ops only, spawns
# nothing"; add bin/host/env/read/write lines as your commands need them.
requires
end

command hello
    description "Say hello"
    do
        print "Hello from perch"
    end
end

command main
    description "Default action — runs when the file is invoked with no command"
    do
        hello
    end
end
"#
    )
}

#[cfg(unix)]
fn write_executable(path: &str, data: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o755).open(path)?;
    f.write_all(data)
}

#[cfg(not(unix))]
fn write_executable(path: &str, data: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_then_refuses_to_overwrite() {
        let dir = std::env::temp_dir().join(format!("perch-init-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("commands.perch");
        let path = path.to_str().unwrap();
        let mut out = Vec::new();
        Impl.execute(path, &mut out).unwrap();
        let body = std::fs::read_to_string(path).unwrap();
        assert!(body.starts_with("#!/usr/bin/env perch\nname    \""));
        assert!(body.ends_with("    end\nend\n"));
        assert!(String::from_utf8(out).unwrap().starts_with(&format!("✓ wrote {path}\n\nTry:\n")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o111, 0o111);
        }
        let mut out = Vec::new();
        Impl.execute(path, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), format!("File already exists at: {path}\n"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn base_matches_go() {
        assert_eq!(base("/a/b"), "b");
        assert_eq!(base("/"), "/");
        assert_eq!(base(""), ".");
    }
}

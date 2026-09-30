//! Writes a JSON catalog of built-in perch ops (the statements people use
//! inside `.perch` files: exec, print, if, ...).
use std::io::Write;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Supplies the handler kinds to document.
pub type KindsFn = Box<dyn Fn() -> Vec<String> + Send + Sync>;

pub struct Impl {
    pub kinds: Option<KindsFn>,
}

impl Impl {
    /// Writes the catalog to `path` (mode 0644); `""` or `"-"` means stdout.
    pub fn execute(&self, path: &str) -> Result<(), Error> {
        self.execute_to(path, &mut std::io::stdout())
    }

    /// Like [`Impl::execute`] but with an explicit stdout sink.
    pub fn execute_to(&self, path: &str, stdout: &mut dyn Write) -> Result<(), Error> {
        let kinds = self.kinds.as_ref().ok_or_else(|| Error::from("export ops catalog: no kinds provider"))?;
        let data = perch_opcatalog::marshal_json(&kinds())?;
        if path.is_empty() || path == "-" {
            stdout.write_all(&data)?;
            stdout.flush()?;
            return Ok(());
        }
        write_file(path, &data)?;
        Ok(())
    }
}

#[cfg(unix)]
fn write_file(path: &str, data: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o644).open(path)?;
    f.write_all(data)
}

#[cfg(not(unix))]
fn write_file(path: &str, data: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_stdout() {
        let i = Impl { kinds: Some(Box::new(|| vec!["print".to_string()])) };
        let mut buf = Vec::new();
        i.execute_to("-", &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains(r#""schema": "perch.catalog.v1""#), "missing schema: {text}");
        assert!(text.contains(r#""example""#));
    }

    #[test]
    fn execute_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ops.json");
        let i = Impl { kinds: Some(Box::new(perch_ops::builtin_kinds)) };
        i.execute(path.to_str().unwrap()).unwrap();
        let data = std::fs::read_to_string(&path).unwrap();
        assert!(data.contains(r#""name": "exec""#), "missing exec op");
        assert!(data.contains(r#""example""#));
    }

    #[test]
    fn no_kinds_provider() {
        let i = Impl { kinds: None };
        assert_eq!(i.execute("-").unwrap_err().to_string(), "export ops catalog: no kinds provider");
    }
}

//! The self-extracting fat-binary feature.
//!
//! `perch --build -f commands.perch -o myapp` copies the running perch
//! executable, marshals the loaded `Program` to JSON, optionally appends an
//! arbitrary file-tree as a gzipped tarball ("the bundle"), and writes a 24-byte
//! footer:
//!
//! ```text
//!   <original executor bytes>
//!   <json bytes>
//!   <archive bytes>                 (may be empty)
//!   <8 bytes: big-endian uint64 archive length>
//!   <8 bytes: big-endian uint64 json length>
//!   <8 bytes: magic = "PRCHEMB2">
//! ```
//!
//! At startup, perch reads the last 24 bytes of the current executable — if the
//! magic matches, it loads the embedded JSON (and remembers the archive for the
//! bundle_dir / bundle_hash / bundle_extract ops).
//!
//! V1 binaries (footer "PRCHEMB1", 16 bytes, no archive) still load — existing
//! fat binaries built with older perch keep working.
use perch_domain::Program;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

/// The current footer sentinel — 8 ASCII bytes at EOF.
pub const MAGIC_V2: [u8; 8] = *b"PRCHEMB2";

/// The legacy footer (no archive section). Still loaded if encountered so old
/// binaries don't break.
pub const MAGIC_V1: [u8; 8] = *b"PRCHEMB1";

/// What [`load`] returns: the parsed program plus an optional payload archive
/// (gzipped tarball) embedded by `--build --include`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub program: Program,
    /// May be empty (Go's nil).
    pub archive: Vec<u8>,
    /// SHA-256 hex of the embedded archive bytes (empty string when there is no
    /// archive). Useful for content-addressable install paths.
    pub archive_hash: String,
}

/// Go-style `open PATH: no such file or directory` text for an IO failure.
fn go_io(op: &str, path: &str, e: &std::io::Error) -> String {
    let why = match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => e.to_string(),
    };
    format!("{op} {path}: {why}")
}

/// Writes a copy of the source binary to `out_path`, appending the program
/// JSON, the optional archive, and a V2 footer.
pub fn embed(source_binary: &str, p: &Program, archive: &[u8], out_path: &str) -> Result<(), Error> {
    let src_bytes = std::fs::read(source_binary)
        .map_err(|e| format!("read source binary: {}", go_io("open", source_binary, &e)))?;
    // Strip any existing footer (idempotent rebuilds).
    let src_bytes = strip_existing(&src_bytes);

    let prog_json = serde_json::to_vec(p).map_err(|e| format!("marshal program: {e}"))?;

    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o755);
    }
    let mut out = opts.open(out_path).map_err(|e| format!("open output: {}", go_io("open", out_path, &e)))?;

    out.write_all(src_bytes)?;
    out.write_all(&prog_json)?;
    if !archive.is_empty() {
        out.write_all(archive)?;
    }
    out.write_all(&(archive.len() as u64).to_be_bytes())?;
    out.write_all(&(prog_json.len() as u64).to_be_bytes())?;
    out.write_all(&MAGIC_V2)?;
    Ok(())
}

/// Reads the embedded bundle from the current executable. Returns `Ok(None)`
/// when nothing is embedded.
pub fn load() -> Result<Option<Bundle>, Error> {
    let Ok(exe) = std::env::current_exe() else {
        return Ok(None);
    };
    load_from(&exe)
}

/// [`load`] against an arbitrary file (used by `load` and tests).
pub fn load_from(path: &Path) -> Result<Option<Bundle>, Error> {
    let Ok(mut f) = File::open(path) else {
        return Ok(None);
    };
    let Ok(meta) = f.metadata() else {
        return Ok(None);
    };
    let size = meta.len() as i64;
    if size < 16 {
        return Ok(None);
    }

    // Peek at the last 8 bytes for the magic.
    let mut magic = [0u8; 8];
    if f.seek(SeekFrom::Start((size - 8) as u64)).is_err() || f.read_exact(&mut magic).is_err() {
        return Ok(None);
    }

    if magic == MAGIC_V2 {
        load_v2(&mut f, size)
    } else if magic == MAGIC_V1 {
        load_v1(&mut f, size)
    } else {
        Ok(None)
    }
}

fn read_u64(f: &mut File) -> Result<i64, Error> {
    let mut b = [0u8; 8];
    f.read_exact(&mut b)?;
    Ok(u64::from_be_bytes(b) as i64)
}

fn load_v2(f: &mut File, size: i64) -> Result<Option<Bundle>, Error> {
    // Footer: <a_len 8><j_len 8><magic 8> = 24 bytes total.
    if size < 24 {
        return Ok(None);
    }
    f.seek(SeekFrom::Start((size - 24) as u64))?;
    let a_len = read_u64(f)?;
    let j_len = read_u64(f)?;
    let total = a_len.wrapping_add(j_len).wrapping_add(24);
    if total > size || a_len < 0 || j_len < 0 {
        return Err(format!("embedded length {total} exceeds file size").into());
    }

    // Read program JSON.
    f.seek(SeekFrom::Start((size - 24 - a_len - j_len) as u64))?;
    let mut prog_buf = vec![0u8; j_len as usize];
    f.read_exact(&mut prog_buf)?;
    let program: Program =
        serde_json::from_slice(&prog_buf).map_err(|e| format!("decode embedded program: {e}"))?;

    let mut archive = Vec::new();
    let mut archive_hash = String::new();
    if a_len > 0 {
        archive = vec![0u8; a_len as usize];
        f.read_exact(&mut archive)?;
        archive_hash = hex(&Sha256::digest(&archive));
    }
    Ok(Some(Bundle { program, archive, archive_hash }))
}

fn load_v1(f: &mut File, size: i64) -> Result<Option<Bundle>, Error> {
    // V1 footer: <j_len 8><magic 8> = 16 bytes total.
    if size < 16 {
        return Ok(None);
    }
    f.seek(SeekFrom::Start((size - 16) as u64))?;
    let j_len = read_u64(f)?;
    if j_len < 0 || j_len + 16 > size {
        return Err(format!("embedded length {j_len} exceeds file size").into());
    }
    f.seek(SeekFrom::Start((size - 16 - j_len) as u64))?;
    let mut prog_buf = vec![0u8; j_len as usize];
    f.read_exact(&mut prog_buf)?;
    let program: Program =
        serde_json::from_slice(&prog_buf).map_err(|e| format!("decode embedded program: {e}"))?;
    Ok(Some(Bundle { program, archive: Vec::new(), archive_hash: String::new() }))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Removes a V1 or V2 footer if present (idempotent rebuilds).
fn strip_existing(b: &[u8]) -> &[u8] {
    if b.len() < 8 {
        return b;
    }
    let len = b.len() as i64;
    let tail = &b[b.len() - 8..];
    let be = |off: usize| i64::from_be_bytes(b[off..off + 8].try_into().unwrap());
    // V2?
    if tail == MAGIC_V2 && b.len() >= 24 {
        let a_len = be(b.len() - 24);
        let j_len = be(b.len() - 16);
        let total = a_len.wrapping_add(j_len).wrapping_add(24);
        if a_len >= 0 && j_len >= 0 && total <= len {
            return &b[..(len - total) as usize];
        }
        return b;
    }
    // V1?
    if tail == MAGIC_V1 && b.len() >= 16 {
        let j_len = be(b.len() - 16);
        let total = j_len.wrapping_add(16);
        if j_len >= 0 && total <= len {
            return &b[..(len - total) as usize];
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::Command;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("perch-embed-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn embed_round_trip() {
        let dir = tmpdir("rt");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::write(&src, b"ABCDEFG_HELLO_FAKE_EXE").unwrap();
        let mut p = Program { name: "demo".into(), version: "1.0.0".into(), ..Default::default() };
        p.commands.insert(
            "hello".into(),
            Command { name: "hello".into(), description: "say hi".into(), ..Default::default() },
        );
        embed(src.to_str().unwrap(), &p, &[], out.to_str().unwrap()).unwrap();

        let data = std::fs::read(&out).unwrap();
        assert!(data.len() > "ABCDEFG_HELLO_FAKE_EXE".len(), "output is not larger than source");
        assert_eq!(&data[data.len() - 8..], &MAGIC_V2);

        let b = load_from(&out).unwrap().unwrap();
        assert_eq!(b.program, p);
        assert!(b.archive.is_empty());
        assert_eq!(b.archive_hash, "");
    }

    #[test]
    fn embed_strips_existing_footer() {
        let dir = tmpdir("strip");
        let (src, out1, out2) = (dir.join("src"), dir.join("out1"), dir.join("out2"));
        std::fs::write(&src, b"STOCK_PERCH_BINARY").unwrap();
        let p = Program { name: "demo".into(), version: "1".into(), ..Default::default() };
        embed(src.to_str().unwrap(), &p, &[], out1.to_str().unwrap()).unwrap();
        embed(out1.to_str().unwrap(), &p, &[], out2.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::metadata(&out1).unwrap().len(), std::fs::metadata(&out2).unwrap().len());
    }

    #[test]
    fn embed_with_archive() {
        // Archives go in the V2 layout. After embed + load round trip, the
        // bytes come back identical and the SHA-256 is populated.
        let dir = tmpdir("arch");
        let (src, out, out2) = (dir.join("src"), dir.join("out"), dir.join("out2"));
        std::fs::write(&src, b"STOCK").unwrap();
        let p = Program { name: "demo".into(), version: "1".into(), ..Default::default() };
        let archive = b"\x1f\x8b\x08\x00FAKE_GZIP_BYTES";
        embed(src.to_str().unwrap(), &p, archive, out.to_str().unwrap()).unwrap();
        // Idempotent rebuild on top of a binary that already carries a V2
        // footer with an archive — output size must not grow.
        embed(out.to_str().unwrap(), &p, archive, out2.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().len(), std::fs::metadata(&out2).unwrap().len());

        let b = load_from(&out).unwrap().unwrap();
        assert_eq!(b.archive, archive);
        assert_eq!(b.archive_hash.len(), 64);
    }

    #[test]
    fn plain_file_has_no_bundle() {
        let dir = tmpdir("none");
        let f = dir.join("plain");
        std::fs::write(&f, b"just some bytes that are long enough").unwrap();
        assert!(load_from(&f).unwrap().is_none());
    }

    #[test]
    fn v1_footer_loads() {
        let dir = tmpdir("v1");
        let f = dir.join("v1");
        let json = serde_json::to_vec(&Program { name: "old".into(), ..Default::default() }).unwrap();
        let mut bytes = b"EXE".to_vec();
        bytes.extend_from_slice(&json);
        bytes.extend_from_slice(&(json.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&MAGIC_V1);
        std::fs::write(&f, &bytes).unwrap();
        let b = load_from(&f).unwrap().unwrap();
        assert_eq!(b.program.name, "old");
    }
}

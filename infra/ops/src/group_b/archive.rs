//! tar / zip ops (archive.go). Also hosts the shared tar.gz extractor the
//! bundle ops reuse.
use crate::group_b::compression::{gz_read_err, gzip_reader};
use crate::group_b::util::*;
use flate2::write::GzEncoder;
use flate2::Compression;
use perch_interpreter::{err, handler, Bindings, Error, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;

type Args<'a> = perch_interpreter::Args<'a>;

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert("tar_create".into(), handler(op_tar_create));
    m.insert("tar_extract".into(), handler(op_tar_extract));
    m.insert("zip_create".into(), handler(op_zip_create));
    m.insert("zip_extract".into(), handler(op_zip_extract));
}

fn tar_read_err(e: &std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::UnexpectedEof {
        return err("unexpected EOF");
    }
    if e.to_string().contains("header") || e.to_string().contains("checksum") {
        return err("archive/tar: invalid tar header");
    }
    gz_read_err(e)
}

// tar_create SRC_DIR DST.tar.gz — gzipped tarball of SRC_DIR.
fn op_tar_create(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src"]), b);
    let dst = resolve(&arg_string(a, &["dst"]), b);
    let out = create_file(&dst)?;
    let gz = GzEncoder::new(out, Compression::default());
    let mut tw = tar::Builder::new(gz);
    let r = walk(&src, &mut |path, md| {
        let rel = go_rel(&src, path).map_err(err)?;
        if rel == "." {
            return Ok(());
        }
        let mut hdr = tar::Header::new_gnu();
        hdr.set_metadata(md);
        hdr.set_mode(super::fsx::mode_of(md) & 0o7777);
        if md.is_dir() {
            hdr.set_entry_type(tar::EntryType::Directory);
            hdr.set_size(0);
            tw.append_data(&mut hdr, &rel, std::io::empty()).map_err(|e| err(go_io_msg(&e)))?;
            return Ok(());
        }
        if md.file_type().is_symlink() {
            // Go writes a symlink header with no target, then copies the
            // followed file's bytes into the 0-size entry (write too long).
            hdr.set_size(0);
            hdr.set_entry_type(tar::EntryType::Symlink);
            tw.append_data(&mut hdr, &rel, std::io::empty()).map_err(|e| err(go_io_msg(&e)))?;
            let mut f = open_file(path)?;
            let mut byte = [0u8; 1];
            return match f.read(&mut byte) {
                Ok(0) => Ok(()),
                Ok(_) => Err(err("archive/tar: write too long")),
                Err(e) => Err(err(format!("read {path}: {}", go_io_msg(&e)))),
            };
        }
        hdr.set_entry_type(tar::EntryType::Regular);
        let f = open_file(path)?;
        hdr.set_size(md.len());
        tw.append_data(&mut hdr, &rel, f).map_err(|e| err(go_io_msg(&e)))
    });
    if let Ok(gz) = tw.into_inner() {
        let _ = gz.finish();
    }
    r?;
    Ok(Value::Null)
}

/// Extracts a gzipped tar stream into `dst`. Names containing ".." are skipped
/// silently; dirs and regular files are created; symlinks only when
/// `symlinks` (the bundle extractor's behavior).
pub fn extract_tar_gz<R: Read>(inp: R, dst: &str, symlinks: bool, gz_prefix: bool) -> Result<()> {
    let gz = gzip_reader(inp).map_err(|e| if gz_prefix { err(format!("gzip: {e}")) } else { e })?;
    let mut ar = tar::Archive::new(gz);
    let entries = ar.entries().map_err(|e| tar_read_err(&e))?;
    for ent in entries {
        let mut ent = ent.map_err(|e| tar_read_err(&e))?;
        let name = String::from_utf8_lossy(&ent.path_bytes()).into_owned();
        let out = go_join(&[dst, &name]);
        if name.contains("..") {
            continue;
        }
        let mode = ent.header().mode().unwrap_or(0) & 0o777;
        let et = ent.header().entry_type();
        if et.is_dir() {
            mkdir_all(&out, mode)?;
        } else if et.is_file() {
            mkdir_all(&go_dir(&out), 0o755)?;
            let mut f = create_with_mode(&out, mode)?;
            let mut buf = vec![0u8; 32 * 1024];
            loop {
                let n = match ent.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) => return Err(tar_read_err(&e)),
                };
                std::io::Write::write_all(&mut f, &buf[..n]).map_err(|e| err(format!("write: {}", go_io_msg(&e))))?;
            }
        } else if symlinks && et.is_symlink() {
            if let Ok(Some(l)) = ent.link_name() {
                let _ = super::fsx::symlink(&l.to_string_lossy(), &out);
            }
        }
    }
    Ok(())
}

fn op_tar_extract(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src"]), b);
    let dst = resolve(&arg_string(a, &["dst"]), b);
    let inp = open_file(&src)?;
    extract_tar_gz(inp, &dst, false, false)?;
    Ok(Value::Null)
}

fn op_zip_create(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    use zip::write::SimpleFileOptions;
    let src = resolve(&arg_string(a, &["src"]), b);
    let dst = resolve(&arg_string(a, &["dst"]), b);
    let out = create_file(&dst)?;
    let mut zw = zip::ZipWriter::new(out);
    let r = walk(&src, &mut |path, md| {
        if md.is_dir() {
            return Ok(());
        }
        let rel = go_rel(&src, path).map_err(err)?;
        zw.start_file(rel, SimpleFileOptions::default()).map_err(|e| err(e.to_string()))?;
        let mut inp = open_file(path)?;
        copy_stream(&mut zw, &mut inp, path)
    });
    let _ = zw.finish();
    r?;
    Ok(Value::Null)
}

fn zip_err(e: zip::result::ZipError) -> Error {
    match e {
        zip::result::ZipError::InvalidArchive(_) => err("zip: not a valid zip file"),
        zip::result::ZipError::Io(e) => err(go_io_msg(&e)),
        e => err(e.to_string()),
    }
}

fn op_zip_extract(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src"]), b);
    let dst = resolve(&arg_string(a, &["dst"]), b);
    let inp = open_file(&src)?;
    let mut zr = zip::ZipArchive::new(inp).map_err(zip_err)?;
    for idx in 0..zr.len() {
        let mut f = zr.by_index(idx).map_err(zip_err)?;
        let name = f.name().to_string();
        let out = go_join(&[&dst, &name]);
        if name.contains("..") {
            continue;
        }
        if f.is_dir() || name.ends_with('/') {
            mkdir_all(&out, 0o755)?;
            continue;
        }
        mkdir_all(&go_dir(&out), 0o755)?;
        let mode = f.unix_mode().map(|m| m & 0o777).unwrap_or(0o666);
        let mut w = create_with_mode(&out, mode)?;
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let n = match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => return Err(gz_read_err(&e)),
            };
            std::io::Write::write_all(&mut w, &buf[..n]).map_err(|e| err(format!("write: {}", go_io_msg(&e))))?;
        }
    }
    Ok(Value::Null)
}

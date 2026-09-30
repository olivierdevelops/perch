//! Embedded-payload ops (bundle.go): `bundle_hash`, `bundle_dir`,
//! `bundle_extract`. The orchestrator hands the payload in with [`set_bundle`].
use crate::group_b::archive::extract_tar_gz;
use crate::group_b::compression::gzip_reader;
use crate::group_b::util::*;
use perch_interpreter::{err, go_quote, handler, Handler, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{Mutex, OnceLock};

struct State {
    archive: Option<Vec<u8>>,
    hash: String,
}

static BUNDLE: Mutex<State> = Mutex::new(State { archive: None, hash: String::new() });
static EXTRACTED: OnceLock<std::result::Result<String, String>> = OnceLock::new();

fn state() -> std::sync::MutexGuard<'static, State> {
    BUNDLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called by the orchestrator at startup with the bundle payload (or `None`
/// when the binary doesn't include one).
#[allow(dead_code)] // consumed by the orchestrator / group A seam
pub fn set_bundle(archive: Option<Vec<u8>>, hash: &str) {
    let mut s = state();
    s.archive = archive;
    s.hash = hash.to_string();
}

/// sha256 of the embedded archive ("" if none).
#[allow(dead_code)] // consumed by the orchestrator / group A seam
pub fn bundle_hash() -> String {
    state().hash.clone()
}

/// Go `BundleReadFile`: `None` = no bundle loaded; `Some(Err(msg))` = bundle
/// present but the entry is missing/unreadable.
#[allow(dead_code)] // consumed by the orchestrator / group A seam
pub fn bundle_read_file(path: &str) -> Option<std::result::Result<Vec<u8>, String>> {
    let archive = state().archive.clone()?;
    let path = path.strip_prefix("./").unwrap_or(path);
    let path = path.strip_prefix('/').unwrap_or(path);
    if path.contains("..") {
        return Some(Err(format!("bundle: path traversal rejected: {}", go_quote(path))));
    }
    let gz = match gzip_reader(Cursor::new(archive)) {
        Ok(g) => g,
        Err(e) => return Some(Err(format!("bundle: gzip: {e}"))),
    };
    let mut ar = tar::Archive::new(gz);
    let entries = match ar.entries() {
        Ok(e) => e,
        Err(e) => return Some(Err(format!("bundle: tar: {e}"))),
    };
    for ent in entries {
        let mut ent = match ent {
            Ok(e) => e,
            Err(e) => return Some(Err(format!("bundle: tar: {e}"))),
        };
        if !ent.header().entry_type().is_file() {
            continue;
        }
        let raw = String::from_utf8_lossy(&ent.path_bytes()).into_owned();
        let name = raw.strip_prefix("./").unwrap_or(&raw);
        if name == path {
            let mut buf = Vec::new();
            return Some(match ent.read_to_end(&mut buf) {
                Ok(_) => Ok(buf),
                Err(e) => Err(format!("bundle: read {}: {e}", go_quote(path))),
            });
        }
    }
    Some(Err(format!("bundle: entry {} not found", go_quote(path))))
}

fn short_hash(h: &str) -> &str {
    if h.len() >= 12 {
        &h[..12]
    } else {
        h
    }
}

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert(
        "bundle_hash".into(),
        handler(|_i, _b, _a| {
            let s = state();
            if s.archive.is_none() {
                return Err(err("no embedded bundle (build with `perch --build --include <path>`)"));
            }
            Ok(Value::String(s.hash.clone()))
        }),
    );
    m.insert(
        "bundle_dir".into(),
        handler(|_i, b, _a| -> Result<Value> {
            let (archive, hash) = {
                let s = state();
                (s.archive.clone(), s.hash.clone())
            };
            let Some(archive) = archive else {
                // No bundle: fall back to script_dir so a single .perch file
                // can use ${bundle_dir} uniformly.
                if let Some(sd) = b.lookup("script_dir").filter(|s| !s.is_empty()) {
                    return Ok(Value::String(sd));
                }
                return Err(err("no embedded bundle and script_dir is empty — provide a file via -f"));
            };
            let r = EXTRACTED.get_or_init(|| {
                let dir = mktemp(&format!("perch-bundle-{}-*", short_hash(&hash)), true).map_err(|e| e.to_string())?;
                extract_tar_gz(Cursor::new(archive), &dir, true, true).map_err(|e| e.to_string())?;
                Ok(dir)
            });
            match r {
                Ok(d) => Ok(Value::String(d.clone())),
                Err(e) => Err(err(e.clone())),
            }
        }),
    );
    m.insert(
        "bundle_extract".into(),
        handler(|_i, _b, a| -> Result<Value> {
            let Some(archive) = state().archive.clone() else {
                return Err(err("no embedded bundle"));
            };
            let dst = arg_string(a, &["dst", "_0"]);
            if dst.is_empty() {
                return Err(err("bundle_extract: missing destination"));
            }
            mkdir_all(&dst, 0o755)?;
            extract_tar_gz(Cursor::new(archive), &dst, true, true)?;
            Ok(Value::String(dst))
        }),
    );
}

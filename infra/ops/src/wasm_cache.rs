//! Persistent compiled-module cache for wasm_run (F02).
//!
//! wasmtime needs several seconds to compile a ~1.5 MB module; the serialized
//! artifact loads in milliseconds. Entries live under
//! `<UserCacheDir>/perch/wasm/` (macOS `~/Library/Caches`, Linux
//! `$XDG_CACHE_HOME` or `~/.cache`, Windows `%LocalAppData%`; the
//! `PERCH_WASM_CACHE_DIR` env var overrides the directory, mainly for tests).
//!
//! File name: `<sha256(module bytes)>-<wasmtime version>-<engine fingerprint>.cwasm`.
//! File format: `b"PERCHWC1"` + sha256(payload) (32 bytes) + payload, where the
//! payload is `Engine::precompile_module` output. `Module::deserialize` is
//! `unsafe` (it trusts its input), so an entry is only deserialized after its
//! self-checksum verifies; any validation/deserialize failure falls back to
//! compiling and rewrites the entry. Writes are atomic (temp file + rename) and
//! best-effort: a read-only or missing cache dir never breaks execution.
//! `PERCH_WASM_CACHE=off` disables both reads and writes.
use sha2::{Digest, Sha256};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use wasmtime::{Engine, Module};

/// Major version of the wasmtime dependency (Cargo.toml pins `40`); the exact
/// release is additionally captured by wasmtime's compatibility hash.
const WASMTIME_VERSION: &str = "wt40";

const MAGIC: &[u8; 8] = b"PERCHWC1";

/// Bump when the engine `Config` in `runtime()` changes in a way the
/// wasmtime compatibility hash would not capture.
pub const ENGINE_FINGERPRINT: &str = "epoch1";

/// Whether the persistent cache is enabled (`PERCH_WASM_CACHE=off` disables).
pub fn enabled() -> bool {
    !matches!(
        std::env::var("PERCH_WASM_CACHE").map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Ok("off" | "0" | "false" | "no" | "disabled")
    )
}

/// `<UserCacheDir>/perch/wasm`, or None when no user cache dir can be found.
pub fn cache_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("PERCH_WASM_CACHE_DIR") {
        if !d.is_empty() {
            return Some(PathBuf::from(d));
        }
    }
    let base = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("LocalAppData")?)
    } else if let Some(x) = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(x)
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".cache")
    };
    Some(base.join("perch").join("wasm"))
}

/// Cache key file name for `bytes` compiled by `engine`.
pub fn entry_name(engine: &Engine, bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    engine.precompile_compatibility_hash().hash(&mut h);
    format!(
        "{}-{}-{}{:016x}.cwasm",
        hex::encode(Sha256::digest(bytes)),
        WASMTIME_VERSION,
        ENGINE_FINGERPRINT,
        h.finish()
    )
}

fn read_entry(engine: &Engine, path: &Path) -> Option<Module> {
    let raw = std::fs::read(path).ok()?;
    if raw.len() < MAGIC.len() + 32 || &raw[..MAGIC.len()] != MAGIC {
        return None;
    }
    let (sum, payload) = raw[MAGIC.len()..].split_at(32);
    if Sha256::digest(payload).as_slice() != sum {
        return None;
    }
    // SAFETY: the payload is byte-for-byte what `write_entry` (this code)
    // produced via `Engine::precompile_module` — its checksum just verified —
    // and wasmtime additionally rejects artifacts from another version/config.
    unsafe { Module::deserialize(engine, payload).ok() }
}

fn write_entry(path: &Path, payload: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no parent"))?;
    std::fs::create_dir_all(dir)?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = dir.join(format!(".tmp-{}-{nanos}", std::process::id()));
    let mut buf = Vec::with_capacity(MAGIC.len() + 32 + payload.len());
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&Sha256::digest(payload));
    buf.extend_from_slice(payload);
    let res = std::fs::write(&tmp, &buf).and_then(|_| std::fs::rename(&tmp, path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Loads the module from the persistent cache under `dir`, or compiles it and
/// (best-effort) stores the serialized artifact. Never fails because of the
/// cache itself; the error is only a genuine compile error message.
pub fn load_or_compile(engine: &Engine, dir: Option<&Path>, bytes: &[u8]) -> Result<Module, String> {
    let entry = dir.map(|d| d.join(entry_name(engine, bytes)));
    if let Some(path) = &entry {
        if let Some(m) = read_entry(engine, path) {
            return Ok(m);
        }
    }
    let Some(path) = entry else {
        return Module::new(engine, bytes).map_err(|e| format!("{:#}", e.root_cause()));
    };
    // Compile once via precompile so the same artifact is cached and loaded.
    let payload = engine.precompile_module(bytes).map_err(|e| format!("{:#}", e.root_cause()))?;
    let _ = write_entry(&path, &payload);
    // SAFETY: `payload` was just produced in-process by this engine.
    unsafe { Module::deserialize(engine, &payload) }.map_err(|e| format!("{:#}", e.root_cause()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use wasmtime::Config;

    fn engine() -> Engine {
        let mut cfg = Config::new();
        cfg.epoch_interruption(true);
        Engine::new(&cfg).unwrap()
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("perch-wasmcache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn hello() -> Vec<u8> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/wasm-hello/hello.wasm");
        std::fs::read(p).unwrap()
    }

    fn entries(d: &Path) -> Vec<PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(d)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "cwasm"))
            .collect();
        v.sort();
        v
    }

    // T-35: warm load is faster than cold compile and yields the same module.
    #[test]
    fn warm_is_faster_and_identical() {
        let (e, d, bytes) = (engine(), tmp("warm"), hello());
        let t = Instant::now();
        let cold = load_or_compile(&e, Some(&d), &bytes).unwrap();
        let cold_t = t.elapsed();
        assert_eq!(entries(&d).len(), 1);
        let t = Instant::now();
        let warm = load_or_compile(&e, Some(&d), &bytes).unwrap();
        let warm_t = t.elapsed();
        assert!(warm_t < cold_t, "warm {warm_t:?} !< cold {cold_t:?}");
        let names = |m: &Module| m.exports().map(|x| x.name().to_string()).collect::<Vec<_>>();
        assert_eq!(names(&cold), names(&warm));
        assert_eq!(cold.serialize().unwrap(), warm.serialize().unwrap());
    }

    // T-36: a different module gets a different entry; corrupt entries heal.
    #[test]
    fn keyed_by_module_and_corrupt_entry_heals() {
        let (e, d, bytes) = (engine(), tmp("heal"), hello());
        load_or_compile(&e, Some(&d), &bytes).unwrap();
        let mut other = bytes.clone();
        other.extend_from_slice(&[0x00, 0x02, 0x01, b'x']); // trailing custom section
        load_or_compile(&e, Some(&d), &other).unwrap();
        let es = entries(&d);
        assert_eq!(es.len(), 2, "{es:?}");

        let victim = es[0].clone();
        let good = std::fs::read(&victim).unwrap();
        // flip a payload byte: checksum mismatch -> recompile + overwrite
        let mut bad = good.clone();
        let n = bad.len() / 2;
        bad[n] ^= 0xff;
        std::fs::write(&victim, &bad).unwrap();
        assert!(read_entry(&e, &victim).is_none());
        // truncated and garbage entries too
        std::fs::write(&victim, b"garbage").unwrap();
        assert!(read_entry(&e, &victim).is_none());
        std::fs::write(&victim, &good[..good.len() / 3]).unwrap();
        assert!(read_entry(&e, &victim).is_none());
        for b in [&bytes, &other] {
            load_or_compile(&e, Some(&d), b).unwrap();
        }
        assert_eq!(std::fs::read(&victim).unwrap(), good, "entry healed to the original artifact");
    }

    #[test]
    fn unwritable_dir_does_not_break() {
        let (e, bytes) = (engine(), hello());
        // a path under a regular file can never be created
        let f = tmp("ro").join("file");
        std::fs::write(&f, b"x").unwrap();
        assert!(load_or_compile(&e, Some(&f.join("sub")), &bytes).is_ok());
        assert!(load_or_compile(&e, None, &bytes).is_ok());
    }

    #[test]
    fn env_switch() {
        // Only reads the var; does not set it (tests run in parallel).
        let v = std::env::var("PERCH_WASM_CACHE").unwrap_or_default();
        assert_eq!(enabled(), !matches!(v.to_ascii_lowercase().as_str(), "off" | "0" | "false" | "no" | "disabled"));
    }
}

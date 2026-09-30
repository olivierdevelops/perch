//! User-keyed body cache (Flavor A).
//!
//! Form:
//!
//! ```text
//! cache "build-${target}-${sha256_file('go.sum')}" "24h"
//!     shell "go build -o bin/${target} ./cmd"
//!     let size = file_size "bin/${target}"
//! end
//! ```
//!
//! First positional arg = cache key (after `${}` interpolation). Second = TTL
//! duration. On miss: run the body, capture every `let X = …` binding that came
//! out of it, persist the captures + a timestamp under
//! `<user cache dir>/perch/blocks/<sha256(key)>.json`. On hit within TTL: skip
//! the body, replay the captured bindings into the current scope, continue.
//!
//! Honest framing: perch does NOT hash the body's transitively-read inputs. The
//! user picks the key, and the key is the contract. If they leave a stale input
//! out of the key, they get stale cache.
use crate::common::{
    arg_string, go_duration_secs_string, go_join, go_parse_duration, now_unix_nanos, parse_rfc3339, rfc3339_nano,
    user_cache_dir,
};
use perch_interpreter::{err, go_quote, handler, Args, Bindings, Handler, Interpreter, Result};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub fn register_cache(m: &mut HashMap<String, Handler>) {
    m.insert("cache".into(), handler(op_cache));
}

/// Format is versioned via `v` so future schema changes can be detected and old
/// entries treated as cache misses rather than mis-parsed.
const CACHE_VERSION: i64 = 1;

fn op_cache(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let key = arg_string(args, &["key", "_0"]);
    if key.is_empty() {
        return Err(err("cache: missing key (first positional arg)"));
    }
    let ttl_str = arg_string(args, &["ttl", "_1"]);
    let mut ttl: i64 = 24 * 3600 * 1_000_000_000;
    if !ttl_str.is_empty() {
        ttl = go_parse_duration(&ttl_str)
            .map_err(|e| err(format!("cache: invalid ttl {}: {}", go_quote(&ttl_str), e)))?;
    }
    let root = match cache_root() {
        Ok(r) => r,
        Err(_) => {
            // Cache directory unavailable — silently fall through to running the
            // body. A failed cache should never block real work.
            i.run_ops(args.body, b)?;
            return Ok(Value::Null);
        }
    };
    let hash: String = Sha256::digest(key.as_bytes()).iter().map(|x| format!("{x:02x}")).collect();
    let path = go_join(&[&root, &format!("{hash}.json")]);

    // Hit?
    if let Ok(data) = std::fs::read(&path) {
        if let Ok(Value::Object(entry)) = serde_json::from_slice::<Value>(&data) {
            let expires = entry.get("expires_at").and_then(|v| v.as_str()).and_then(parse_rfc3339);
            let hit = entry.get("v").and_then(|v| v.as_i64()) == Some(CACHE_VERSION)
                && entry.get("key").and_then(|v| v.as_str()) == Some(key.as_str())
                && expires.is_some_and(|e| now_unix_nanos() < e);
            if hit {
                let empty = Map::new();
                let bindings = entry.get("bindings").and_then(|v| v.as_object()).unwrap_or(&empty);
                let left_secs = {
                    // Duration.Round(time.Second): half away from zero.
                    let ns = expires.unwrap_or(0) - now_unix_nanos();
                    ((ns + 500_000_000).div_euclid(1_000_000_000)) as i64
                };
                let _ = i.stderr.write_str(&format!(
                    "↪ cache hit: {} (replayed {} bindings, {} left)\n",
                    truncate_key(&key),
                    bindings.len(),
                    go_duration_secs_string(left_secs)
                ));
                for (k, v) in bindings {
                    b.set(k, v.clone());
                }
                return Ok(Value::Null);
            }
        }
    }

    // Miss: run the body and capture every binding it newly sets.
    let before = snapshot_vars(b);
    i.run_ops(args.body, b)?;
    let captured = diff_vars(&before, b);
    let now = now_unix_nanos();
    // Keys are sorted, like Go's json.Marshal of a map.
    let mut sorted: Vec<(String, Value)> = captured.into_iter().collect();
    sorted.sort_by(|a, c| a.0.cmp(&c.0));
    let bindings: Map<String, Value> = sorted.into_iter().collect();
    let mut entry = Map::new();
    entry.insert("v".into(), Value::from(CACHE_VERSION));
    entry.insert("key".into(), Value::String(key));
    entry.insert("stored_at".into(), Value::String(rfc3339_nano(now)));
    entry.insert("expires_at".into(), Value::String(rfc3339_nano(now + ttl as i128)));
    entry.insert("bindings".into(), Value::Object(bindings));
    if std::fs::create_dir_all(&root).is_ok() {
        if let Ok(data) = serde_json::to_vec(&Value::Object(entry)) {
            let _ = std::fs::write(&path, data);
        }
    }
    Ok(Value::Null)
}

/// The path under which body-cache entries live. Each entry is one JSON file
/// named by sha256(key).
pub fn cache_root() -> std::result::Result<String, String> {
    let base = user_cache_dir()?;
    Ok(go_join(&[&base, "perch", "blocks"]))
}

/// Copies the current Vars map. Used to diff before/after the body runs so only
/// NEW or CHANGED bindings get cached.
fn snapshot_vars(b: &Bindings) -> HashMap<String, Value> {
    b.vars.clone()
}

/// Auto-bound names (os, arch, home, …) filtered out of the cache so the file
/// stays small and recognisable.
const SKIP: &[&str] = &[
    "os", "arch", "home", "home_dir", "config_dir", "cache_dir", "temp_dir", "data_dir", "exe_path", "exe_dir",
    "exe_name", "script_path", "script_dir", "user", "uid", "hostname", "pid", "now_unix", "cpu_count", "is_windows",
    "is_macos", "is_linux", "is_unix", "is_arm64", "is_amd64", "path_sep", "path_list_sep", "exe_ext", "null_device",
    "shell_name",
];

/// Bindings that were added or modified after the snapshot.
fn diff_vars(before: &HashMap<String, Value>, b: &Bindings) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for (k, v) in &b.vars {
        if SKIP.contains(&k.as_str()) {
            continue;
        }
        match before.get(k) {
            Some(prev) if equal_any(prev, v) => {}
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Loose equality of two binding values. Numeric types are compared after a
/// float widening so int(5) and float64(5) match — JSON round-tripping through
/// cache files normalises everything to float64, and replays should compare
/// equal pre/post.
fn equal_any(a: &Value, b: &Value) -> bool {
    if a.is_null() || b.is_null() {
        return a.is_null() && b.is_null();
    }
    if let (Value::Number(x), Value::Number(y)) = (a, b) {
        return x.as_f64() == y.as_f64();
    }
    perch_interpreter::to_string_value(a) == perch_interpreter::to_string_value(b)
}

/// Shortens long keys for the cache-hit message.
fn truncate_key(s: &str) -> String {
    if s.len() <= 60 {
        return s.to_string();
    }
    let mut end = 57;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_any_widens_numbers() {
        assert!(equal_any(&Value::from(5), &Value::from(5.0)));
        assert!(!equal_any(&Value::from(5), &Value::from(6)));
        assert!(equal_any(&Value::Null, &Value::Null));
        assert!(!equal_any(&Value::Null, &Value::from("")));
    }

    #[test]
    fn truncates_long_keys() {
        assert_eq!(truncate_key("short"), "short");
        let long = "k".repeat(80);
        assert_eq!(truncate_key(&long), format!("{}...", "k".repeat(57)));
    }
}

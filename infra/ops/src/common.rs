//! Helpers shared by the group-A op files: Go-flavoured arg access, value
//! coercion, path cleaning, duration parsing, error text, PATH lookup, and the
//! per-OS user directories. Everything mirrors the Go standard-library behavior
//! the original ops relied on so messages stay byte-identical.
use perch_interpreter::{go_quote, to_string_value, Bindings};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// Go `argString`: the string form of the FIRST of `names` present in `args`
/// (a present key with a null value yields ""), or "" when none are present.
pub fn arg_string(args: &Map<String, Value>, names: &[&str]) -> String {
    for n in names {
        if let Some(v) = args.get(*n) {
            return to_string_value(v);
        }
    }
    String::new()
}

/// Go `truthy`: "", "false" and "0" are false; everything else true.
pub fn truthy(s: &str) -> bool {
    !(s.is_empty() || s == "false" || s == "0")
}

/// Go `truthyValue`: the raw return value of an op handler.
pub fn truthy_value(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => truthy(s),
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        _ => true,
    }
}

/// Go `toFloat`: numbers widen, bools are 0/1, strings ParseFloat (0 on error).
pub fn to_float(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

// ── filepath ──────────────────────────────────────────────────────────────

/// Go `filepath.IsAbs` (Unix flavour; Windows defers to std).
pub fn is_abs(p: &str) -> bool {
    #[cfg(windows)]
    {
        std::path::Path::new(p).is_absolute()
    }
    #[cfg(not(windows))]
    {
        p.starts_with('/')
    }
}

/// Windows path text normalised for lexical work: the `\\?\` verbatim prefix
/// (what `canonicalize` yields) is dropped and `\` becomes `/`, so verbatim,
/// drive-letter and mixed-separator spellings of one path compare equal.
/// A no-op on Unix, where `\` is an ordinary file-name character.
pub fn slashed(path: &str) -> std::borrow::Cow<'_, str> {
    #[cfg(windows)]
    {
        let p = path
            .strip_prefix(r"\\?\")
            .or_else(|| path.strip_prefix("//?/"))
            .unwrap_or(path);
        let p = match p.strip_prefix(r"UNC\") {
            Some(rest) => format!(r"\\{rest}"),
            None => p.to_string(),
        };
        std::borrow::Cow::Owned(p.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        std::borrow::Cow::Borrowed(path)
    }
}

/// Go `filepath.FromSlash`: `/` becomes the native separator (`\` on Windows,
/// identity on Unix). Used for user-visible path results; internal path logic
/// stays `/`-separated.
pub fn to_native(path: String) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        path
    }
}

/// Splits a leading Windows drive (`C:`) off `path`; always `("", path)` on Unix.
fn split_drive(path: &str) -> (&str, &str) {
    #[cfg(windows)]
    {
        let b = path.as_bytes();
        if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
            return path.split_at(2);
        }
    }
    ("", path)
}

/// Whether `abs` equals, or is nested under, `root` (both already cleaned).
/// Case-insensitive on Windows, whose file systems are.
pub fn path_is_within(abs: &str, root: &str) -> bool {
    #[cfg(windows)]
    let (abs, root) = (abs.to_lowercase(), root.to_lowercase());
    #[cfg(windows)]
    let (abs, root) = (abs.as_str(), root.trim_end_matches('/'));
    abs == root || abs.starts_with(&format!("{root}/"))
}

/// Go `filepath.Clean` (lexical, `/`-separated; on Windows `\` is a separator,
/// the verbatim prefix is stripped and a drive letter acts as the root).
pub fn go_clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let path = slashed(path);
    let (drive, path) = split_drive(&path);
    let rooted = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for elem in path.split('/') {
        match elem {
            "" | "." => {}
            ".." => {
                if stack.last().is_some_and(|l| *l != "..") {
                    stack.pop();
                } else if !rooted {
                    stack.push("..");
                }
            }
            e => stack.push(e),
        }
    }
    let joined = stack.join("/");
    match (rooted, joined.is_empty()) {
        (true, _) => format!("{drive}/{joined}"),
        (false, true) if drive.is_empty() => ".".to_string(),
        (false, true) => drive.to_string(),
        (false, false) => format!("{drive}{joined}"),
    }
}

/// Go `filepath.Join`: non-empty elements joined and cleaned; "" when all empty.
pub fn go_join(elems: &[&str]) -> String {
    let parts: Vec<&str> = elems.iter().copied().filter(|e| !e.is_empty()).collect();
    if parts.is_empty() {
        return String::new();
    }
    go_clean(&parts.join("/"))
}

/// Go `filepath.Dir`.
pub fn go_dir(path: &str) -> String {
    let path = &*slashed(path);
    let i = path.rfind('/').map(|i| i + 1).unwrap_or(0);
    let dir = go_clean(&path[..i]);
    if dir.is_empty() {
        ".".to_string()
    } else {
        dir
    }
}

/// Go `filepath.Base`.
pub fn go_base(path: &str) -> String {
    let path = &*slashed(path);
    if path.is_empty() {
        return ".".to_string();
    }
    let t = path.trim_end_matches('/');
    if t.is_empty() {
        return "/".to_string();
    }
    match t.rfind('/') {
        Some(i) => t[i + 1..].to_string(),
        None => t.to_string(),
    }
}

/// Go `filepath.Abs` (relative paths are joined under the process cwd).
pub fn go_abs(p: &str) -> std::io::Result<String> {
    if is_abs(p) {
        return Ok(go_clean(p));
    }
    let cwd = std::env::current_dir()?;
    Ok(go_join(&[&cwd.to_string_lossy(), p]))
}

/// Go `resolve` (files.go): absolute paths pass through, relative ones are
/// joined under the binding cwd.
pub fn resolve(p: &str, b: &Bindings) -> String {
    if is_abs(p) {
        p.to_string()
    } else {
        go_join(&[&b.cwd, p])
    }
}

// ── errors ────────────────────────────────────────────────────────────────

/// Go-style text of an OS error: lower-cased first letter, no `(os error N)`.
pub fn go_io_msg(e: &std::io::Error) -> String {
    let s = e.to_string();
    let s = match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    };
    let mut cs = s.chars();
    match cs.next() {
        Some(c) => c.to_lowercase().collect::<String>() + cs.as_str(),
        None => s,
    }
}

/// `*PathError` text: `<op> <path>: <cause>`.
pub fn path_err(op: &str, path: &str, e: &std::io::Error) -> String {
    format!("{op} {path}: {}", go_io_msg(e))
}

// ── time ──────────────────────────────────────────────────────────────────

/// Go `time.ParseDuration`; the value is nanoseconds. Errors carry Go's text.
pub fn go_parse_duration(orig: &str) -> Result<i64, String> {
    let invalid = || format!("time: invalid duration {}", go_quote(orig));
    let mut s = orig;
    let mut neg = false;
    if let Some(c) = s.chars().next() {
        if c == '-' || c == '+' {
            neg = c == '-';
            s = &s[1..];
        }
    }
    if s == "0" {
        return Ok(0);
    }
    if s.is_empty() {
        return Err(invalid());
    }
    let mut total: u128 = 0;
    while !s.is_empty() {
        let b = s.as_bytes();
        if !(b[0] == b'.' || b[0].is_ascii_digit()) {
            return Err(invalid());
        }
        // integer part
        let pl = s.len();
        let n = b.iter().take_while(|c| c.is_ascii_digit()).count();
        let mut v: u128 = 0;
        for &c in &b[..n] {
            v = v * 10 + (c - b'0') as u128;
            if v > (1u128 << 63) {
                return Err(invalid());
            }
        }
        s = &s[n..];
        let pre = pl != s.len();
        // fraction
        let mut f: u128 = 0;
        let mut scale: f64 = 1.0;
        let mut post = false;
        if s.starts_with('.') {
            s = &s[1..];
            let pl2 = s.len();
            let b = s.as_bytes();
            let n = b.iter().take_while(|c| c.is_ascii_digit()).count();
            let mut overflow = false;
            for &c in &b[..n] {
                if overflow {
                    continue;
                }
                if f > (u64::MAX as u128 - 10) / 10 {
                    overflow = true;
                    continue;
                }
                f = f * 10 + (c - b'0') as u128;
                scale *= 10.0;
            }
            s = &s[n..];
            post = pl2 != s.len();
        }
        if !pre && !post {
            return Err(invalid());
        }
        // unit
        let b = s.as_bytes();
        let i = b.iter().take_while(|&&c| c != b'.' && !c.is_ascii_digit()).count();
        if i == 0 {
            return Err(format!("time: missing unit in duration {}", go_quote(orig)));
        }
        let u = &s[..i];
        s = &s[i..];
        let unit: u128 = match u {
            "ns" => 1,
            "us" | "\u{b5}s" | "\u{3bc}s" => 1_000,
            "ms" => 1_000_000,
            "s" => 1_000_000_000,
            "m" => 60_000_000_000,
            "h" => 3_600_000_000_000,
            _ => {
                return Err(format!("time: unknown unit {} in duration {}", go_quote(u), go_quote(orig)));
            }
        };
        let mut piece = v * unit;
        if f > 0 {
            piece += (f as f64 * (unit as f64 / scale)) as u128;
        }
        total += piece;
        if total > (1u128 << 63) {
            return Err(invalid());
        }
    }
    if neg {
        return Ok(-(total as i128) as i64);
    }
    if total > i64::MAX as u128 {
        return Err(invalid());
    }
    Ok(total as i64)
}

/// Go `Duration.String()` for a whole number of seconds (what the ops print
/// after `.Round(time.Second)`).
pub fn go_duration_secs_string(secs: i64) -> String {
    let neg = secs < 0;
    let u = secs.unsigned_abs();
    let (h, m, s) = (u / 3600, (u % 3600) / 60, u % 60);
    let body = if h > 0 {
        format!("{h}h{m}m{s}s")
    } else if m > 0 {
        format!("{m}m{s}s")
    } else {
        format!("{s}s")
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Go `time.Time` JSON form (RFC3339Nano, UTC) for a unix-nanosecond instant.
pub fn rfc3339_nano(unix_nanos: i128) -> String {
    let secs = unix_nanos.div_euclid(1_000_000_000) as i64;
    let nanos = unix_nanos.rem_euclid(1_000_000_000) as u64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    let mut out = format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, (rem % 3600) / 60, rem % 60);
    if nanos > 0 {
        let frac = format!("{nanos:09}");
        out.push('.');
        out.push_str(frac.trim_end_matches('0'));
    }
    out.push('Z');
    out
}

/// Parses an RFC3339 timestamp (as Go writes them) into unix nanoseconds.
pub fn parse_rfc3339(s: &str) -> Option<i128> {
    let (date, rest) = s.split_once('T')?;
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;
    if dp.next().is_some() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let (time, off_secs): (&str, i64) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0)
    } else {
        let idx = rest.rfind(['+', '-'])?;
        let (t, o) = rest.split_at(idx);
        let sign = if o.starts_with('-') { -1 } else { 1 };
        let (oh, om) = o[1..].split_once(':')?;
        (t, sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60))
    };
    let (hms, frac) = match time.split_once('.') {
        Some((h, f)) => (h, f),
        None => (time, ""),
    };
    let mut tp = hms.split(':');
    let hh: i64 = tp.next()?.parse().ok()?;
    let mm: i64 = tp.next()?.parse().ok()?;
    let ss: i64 = tp.next()?.parse().ok()?;
    if tp.next().is_some() || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let mut nanos: i128 = 0;
    if !frac.is_empty() {
        if !frac.bytes().all(|c| c.is_ascii_digit()) || frac.len() > 9 {
            return None;
        }
        nanos = format!("{frac:0<9}").parse().ok()?;
    }
    let secs = days_from_civil(y, mo, d) * 86400 + hh * 3600 + mm * 60 + ss - off_secs;
    Some(secs as i128 * 1_000_000_000 + nanos)
}

/// Now as unix nanoseconds.
pub fn now_unix_nanos() -> i128 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(_) => 0,
    }
}

// ── PATH lookup / user dirs ───────────────────────────────────────────────

fn is_executable_file(p: &std::path::Path) -> bool {
    let Ok(md) = std::fs::metadata(p) else { return false };
    if md.is_dir() {
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

/// Go `exec.LookPath`. Names containing a path separator are checked directly;
/// bare names are searched along `$PATH`.
pub fn look_path(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') || (cfg!(windows) && name.contains('\\')) {
        let p = PathBuf::from(name);
        return if is_executable_file(&p) { Some(p) } else { None };
    }
    let path = std::env::var("PATH").unwrap_or_default();
    #[cfg(windows)]
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .map(|s| s.to_string())
        .collect();
    for dir in std::env::split_paths(&path) {
        let dir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir };
        #[cfg(windows)]
        {
            for e in std::iter::once(String::new()).chain(exts.iter().cloned()) {
                let cand = dir.join(format!("{name}{e}"));
                if is_executable_file(&cand) {
                    return Some(cand);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let cand = dir.join(name);
            if is_executable_file(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// Go `os.UserHomeDir`.
pub fn user_home_dir() -> Result<String, String> {
    if cfg!(windows) {
        return env_nonempty("USERPROFILE").ok_or_else(|| "%userprofile% is not defined".to_string());
    }
    env_nonempty("HOME").ok_or_else(|| "$HOME is not defined".to_string())
}

/// Go `os.UserCacheDir`.
pub fn user_cache_dir() -> Result<String, String> {
    if cfg!(windows) {
        return env_nonempty("LocalAppData").ok_or_else(|| "%LocalAppData% is not defined".to_string());
    }
    if cfg!(target_os = "macos") {
        let h = env_nonempty("HOME").ok_or_else(|| "neither $XDG_CACHE_HOME nor $HOME are defined".to_string())?;
        return Ok(format!("{h}/Library/Caches"));
    }
    match env_nonempty("XDG_CACHE_HOME") {
        Some(d) => {
            if !is_abs(&d) {
                return Err("path in $XDG_CACHE_HOME is relative".to_string());
            }
            Ok(d)
        }
        None => {
            let h = env_nonempty("HOME").ok_or_else(|| "neither $XDG_CACHE_HOME nor $HOME are defined".to_string())?;
            Ok(format!("{h}/.cache"))
        }
    }
}

/// Go `os.UserConfigDir`.
pub fn user_config_dir() -> Result<String, String> {
    if cfg!(windows) {
        return env_nonempty("AppData").ok_or_else(|| "%AppData% is not defined".to_string());
    }
    if cfg!(target_os = "macos") {
        let h = env_nonempty("HOME").ok_or_else(|| "$HOME is not defined".to_string())?;
        return Ok(format!("{h}/Library/Application Support"));
    }
    match env_nonempty("XDG_CONFIG_HOME") {
        Some(d) => {
            if !is_abs(&d) {
                return Err("path in $XDG_CONFIG_HOME is relative".to_string());
            }
            Ok(d)
        }
        None => {
            let h = env_nonempty("HOME").ok_or_else(|| "neither $XDG_CONFIG_HOME nor $HOME are defined".to_string())?;
            Ok(format!("{h}/.config"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_join_dir_base() {
        assert_eq!(go_clean("a//b/./c/.."), "a/b");
        assert_eq!(go_clean("../x/../.."), "../..");
        assert_eq!(go_clean("/../a"), "/a");
        assert_eq!(go_clean(""), ".");
        assert_eq!(go_join(&["", ""]), "");
        assert_eq!(go_join(&["/a", "../b"]), "/b");
        assert_eq!(go_dir("/a/b/c.txt"), "/a/b");
        assert_eq!(go_dir("c.txt"), ".");
        assert_eq!(go_base("/a/b/"), "b");
    }

    #[test]
    fn durations() {
        assert_eq!(go_parse_duration("1h30m").unwrap(), 5_400_000_000_000);
        assert_eq!(go_parse_duration("1.5s").unwrap(), 1_500_000_000);
        assert_eq!(go_parse_duration("300ms").unwrap(), 300_000_000);
        assert_eq!(go_parse_duration("0").unwrap(), 0);
        assert_eq!(go_parse_duration("abc").unwrap_err(), "time: invalid duration \"abc\"");
        assert_eq!(go_parse_duration("5").unwrap_err(), "time: missing unit in duration \"5\"");
        assert_eq!(go_parse_duration("5x").unwrap_err(), "time: unknown unit \"x\" in duration \"5x\"");
        assert_eq!(go_duration_secs_string(86399), "23h59m59s");
        assert_eq!(go_duration_secs_string(60), "1m0s");
        assert_eq!(go_duration_secs_string(0), "0s");
    }

    #[test]
    fn rfc3339_round_trip() {
        let n: i128 = 1_790_000_000_123_456_789;
        let s = rfc3339_nano(n);
        assert_eq!(parse_rfc3339(&s), Some(n));
        assert_eq!(parse_rfc3339("2026-10-01T12:00:00+02:00"), parse_rfc3339("2026-10-01T10:00:00Z"));
    }
}

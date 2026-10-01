//! Private helpers for the group-B op files: Go-flavoured arg access, the
//! `filepath` functions the ops lean on, Go-style OS error text, a `Walk`
//! equivalent, temp-file naming, RE2-flavoured regex compilation and Go's
//! `json.Marshal` text. The helpers group A also uses (arg access, `filepath`
//! Clean/Join/Dir/Base, `resolve`, OS error text) are re-exported from
//! `common.rs`, not duplicated.
pub use crate::common::{arg_string, go_base, go_clean, go_dir, go_io_msg, go_join, resolve, to_float};
use crate::common;
use perch_interpreter::{err, Args, Error, Handler, Result};
use serde_json::Value;
use std::fs;
use std::io;

/// Go `int(f)` for a float64 (saturating; NaN -> 0).
pub fn f2i(f: f64) -> i64 {
    f as i64
}

/// Registers a pure `fn(&Args) -> Result<Value>` (no interpreter / bindings).
pub fn pure(f: fn(&Args<'_>) -> Result<Value>) -> Handler {
    perch_interpreter::handler(move |_i, _b, args| f(args))
}

// ── filepath (Unix flavour) ───────────────────────────────────────────────

pub fn is_abs(p: &str) -> bool {
    p.starts_with('/')
}

/// Go `filepath.Ext`.
pub fn go_ext(path: &str) -> String {
    for (i, c) in path.char_indices().rev() {
        if c == '/' {
            break;
        }
        if c == '.' {
            return path[i..].to_string();
        }
    }
    String::new()
}

/// Go `filepath.Abs`.
pub fn go_abs(p: &str) -> io::Result<String> {
    if is_abs(p) {
        return Ok(go_clean(p));
    }
    let cwd = std::env::current_dir()?;
    Ok(go_join(&[&cwd.to_string_lossy(), p]))
}

/// Go `filepath.Rel`.
pub fn go_rel(basepath: &str, targpath: &str) -> std::result::Result<String, String> {
    let mut base = go_clean(basepath);
    let targ = go_clean(targpath);
    if targ == base {
        return Ok(".".to_string());
    }
    if base == "." {
        base = String::new();
    }
    let base_slashed = base.starts_with('/');
    let targ_slashed = targ.starts_with('/');
    if base_slashed != targ_slashed {
        return Err(format!("Rel: can't make {targpath} relative to {basepath}"));
    }
    let (bb, tb) = (base.as_bytes(), targ.as_bytes());
    let (bl, tl) = (bb.len(), tb.len());
    let (mut b0, mut bi, mut t0, mut ti) = (0usize, 0usize, 0usize, 0usize);
    loop {
        while bi < bl && bb[bi] != b'/' {
            bi += 1;
        }
        while ti < tl && tb[ti] != b'/' {
            ti += 1;
        }
        if tb[t0..ti] != bb[b0..bi] {
            break;
        }
        if bi < bl {
            bi += 1;
        }
        if ti < tl {
            ti += 1;
        }
        b0 = bi;
        t0 = ti;
    }
    if &base[b0..bi] == ".." {
        return Err(format!("Rel: can't make {targpath} relative to {basepath}"));
    }
    if b0 != bl {
        let seps = base[b0..bl].matches('/').count();
        let mut out = String::from("..");
        for _ in 0..seps {
            out.push_str("/..");
        }
        if t0 != tl {
            out.push('/');
            out.push_str(&targ[t0..]);
        }
        return Ok(out);
    }
    Ok(targ[t0..].to_string())
}

// ── Go-style OS errors ────────────────────────────────────────────────────

/// `*PathError`: `<op> <path>: <cause>`.
pub fn path_err(op: &str, path: &str, e: &io::Error) -> Error {
    err(common::path_err(op, path, e))
}

/// `*LinkError`: `<op> <old> <new>: <cause>`.
pub fn link_err(op: &str, old: &str, new: &str, e: &io::Error) -> Error {
    err(format!("{op} {old} {new}: {}", go_io_msg(e)))
}

pub fn is_not_exist(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound
}

/// `os.MkdirAll(path, mode)`.
pub fn mkdir_all(path: &str, mode: u32) -> Result<()> {
    if path.is_empty() {
        return Err(err("mkdir : no such file or directory"));
    }
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => return Ok(()),
        Ok(_) => return Err(err(format!("mkdir {path}: not a directory"))),
        Err(_) => {}
    }
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    super::fsx::dir_mode(&mut b, mode);
    match b.create(path) {
        Ok(()) => Ok(()),
        Err(e) => {
            if fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
                Ok(())
            } else if e.kind() == io::ErrorKind::AlreadyExists {
                Err(err(format!("mkdir {path}: not a directory")))
            } else {
                Err(path_err("mkdir", path, &e))
            }
        }
    }
}

/// `os.Create(path)` (0666, truncate).
pub fn create_file(path: &str) -> Result<fs::File> {
    fs::File::create(path).map_err(|e| path_err("open", path, &e))
}

/// `os.Open(path)`.
pub fn open_file(path: &str) -> Result<fs::File> {
    fs::File::open(path).map_err(|e| path_err("open", path, &e))
}

/// `os.OpenFile(path, O_CREATE|O_WRONLY|O_TRUNC, mode)`.
pub fn create_with_mode(path: &str, mode: u32) -> Result<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.create(true).write(true).truncate(true);
    super::fsx::open_mode(&mut o, mode).open(path).map_err(|e| path_err("open", path, &e))
}

/// `io.Copy(dst, src)` with Go-ish error text for a failed read.
pub fn copy_stream(dst: &mut dyn io::Write, src: &mut dyn io::Read, src_name: &str) -> Result<()> {
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(err(format!("read {src_name}: {}", go_io_msg(&e)))),
        };
        dst.write_all(&buf[..n]).map_err(|e| err(format!("write: {}", go_io_msg(&e))))?;
    }
}

/// `copyFile` (install.go): Create(dst) + ReadFrom(src).
pub fn copy_file(src: &str, dst: &str) -> Result<()> {
    let mut inp = open_file(src)?;
    let mut out = create_file(dst)?;
    copy_stream(&mut out, &mut inp, src)
}

/// `filepath.Walk`: lexical order, symlinks not followed (Lstat). The callback
/// gets each path and its Lstat metadata; any error it (or a stat/readdir)
/// returns stops the walk and is returned unchanged.
pub fn walk(root: &str, f: &mut dyn FnMut(&str, &fs::Metadata) -> Result<()>) -> Result<()> {
    let md = fs::symlink_metadata(root).map_err(|e| path_err("lstat", root, &e))?;
    walk_rec(root, &md, f)
}

fn walk_rec(path: &str, md: &fs::Metadata, f: &mut dyn FnMut(&str, &fs::Metadata) -> Result<()>) -> Result<()> {
    f(path, md)?;
    if !md.is_dir() {
        return Ok(());
    }
    let mut names: Vec<String> = Vec::new();
    for ent in fs::read_dir(path).map_err(|e| path_err("open", path, &e))? {
        let ent = ent.map_err(|e| path_err("readdirent", path, &e))?;
        names.push(ent.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    for n in names {
        let child = go_join(&[path, &n]);
        let cmd = fs::symlink_metadata(&child).map_err(|e| path_err("lstat", &child, &e))?;
        walk_rec(&child, &cmd, f)?;
    }
    Ok(())
}

// ── temp names ────────────────────────────────────────────────────────────

fn next_rand() -> u32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut s = STATE.load(Ordering::Relaxed);
    if s == 0 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        s = nanos ^ (u64::from(std::process::id()) << 32) | 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    STATE.store(s, Ordering::Relaxed);
    (s >> 16) as u32
}

/// `os.MkdirTemp("", pattern)` (dir = true) / `os.CreateTemp("", pattern)`
/// (dir = false; the file is created and closed). Returns the path.
pub fn mktemp(pattern: &str, dir: bool) -> Result<String> {
    if pattern.contains('/') {
        return Err(err("pattern contains path separator"));
    }
    let (prefix, suffix) = match pattern.rfind('*') {
        Some(i) => (&pattern[..i], &pattern[i + 1..]),
        None => (pattern, ""),
    };
    let tmp = std::env::temp_dir().to_string_lossy().into_owned();
    let base = if tmp.ends_with('/') { tmp } else { format!("{tmp}/") };
    for _ in 0..10000 {
        let path = format!("{base}{prefix}{}{suffix}", next_rand());
        let r = if dir {
            super::fsx::dir_mode(&mut fs::DirBuilder::new(), 0o700).create(&path).map(|_| ())
        } else {
            let mut o = fs::OpenOptions::new();
            o.read(true).write(true).create_new(true);
            super::fsx::open_mode(&mut o, 0o600).open(&path).map(|_| ())
        };
        match r {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(path_err(if dir { "mkdir" } else { "open" }, &path, &e)),
        }
    }
    Err(err(format!("{}: too many temp name collisions", if dir { "mkdir" } else { "open" })))
}

// ── Go string helpers ─────────────────────────────────────────────────────

/// `strings.Split(s, sep)` (empty `sep` splits into UTF-8 characters).
pub fn go_split(s: &str, sep: &str) -> Vec<String> {
    if sep.is_empty() {
        return s.chars().map(|c| c.to_string()).collect();
    }
    s.split(sep).map(str::to_string).collect()
}

/// `unicode.ToUpper` / `ToLower` for one rune (simple case mapping only).
pub fn simple_upper(c: char) -> char {
    let mut it = c.to_uppercase();
    match (it.next(), it.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

pub fn simple_lower(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

// ── RE2 flavour ───────────────────────────────────────────────────────────

/// Compiles a Go (RE2) pattern with the `regex` crate. Go's `\d \w \s \b` are
/// ASCII-only whereas the crate's are Unicode-aware, so they are rewritten to
/// the equivalent ASCII forms first.
pub fn go_regex(pat: &str) -> std::result::Result<regex::Regex, String> {
    let mut out = String::with_capacity(pat.len());
    let cs: Vec<char> = pat.chars().collect();
    let mut i = 0;
    let mut in_class = false;
    while i < cs.len() {
        let c = cs[i];
        if c == '\\' && i + 1 < cs.len() {
            let n = cs[i + 1];
            let (plain, neg): (Option<&str>, Option<&str>) = match n {
                'd' => (Some("0-9"), None),
                'w' => (Some("0-9A-Za-z_"), None),
                's' => (Some("\\t\\n\\x0C\\r "), None),
                'D' => (None, Some("0-9")),
                'W' => (None, Some("0-9A-Za-z_")),
                'S' => (None, Some("\\t\\n\\x0C\\r ")),
                _ => (None, None),
            };
            if let Some(p) = plain {
                if in_class {
                    out.push_str(p);
                } else {
                    out.push('[');
                    out.push_str(p);
                    out.push(']');
                }
                i += 2;
                continue;
            }
            if let Some(p) = neg {
                if in_class {
                    // negated shorthand inside a class: keep the crate's own
                    out.push('\\');
                    out.push(n);
                } else {
                    out.push_str("[^");
                    out.push_str(p);
                    out.push(']');
                }
                i += 2;
                continue;
            }
            if n == 'b' && !in_class {
                out.push_str("(?-u:\\b)");
                i += 2;
                continue;
            }
            if n == 'B' && !in_class {
                out.push_str("(?-u:\\B)");
                i += 2;
                continue;
            }
            out.push(c);
            out.push(n);
            i += 2;
            continue;
        }
        if in_class {
            if c == ']' {
                in_class = false;
            }
        } else if c == '[' {
            in_class = true;
            out.push(c);
            i += 1;
            if i < cs.len() && cs[i] == '^' {
                out.push('^');
                i += 1;
            }
            // a leading ']' is literal
            if i < cs.len() && cs[i] == ']' {
                out.push_str("\\]");
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    regex::Regex::new(&out).map_err(|e| format!("error parsing regexp: {}", first_line(&e.to_string())))
}

fn first_line(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    match lines.last() {
        Some(l) => l.trim().strip_prefix("error: ").unwrap_or(l.trim()).to_string(),
        None => s.to_string(),
    }
}

// ── Go json.Marshal ───────────────────────────────────────────────────────

/// `json.Marshal` of a decoded value: sorted object keys, HTML-escaped strings.
pub fn go_json_marshal(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else {
                out.push_str(&go_float_json(n.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(s) => go_json_string(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                go_json_marshal(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                go_json_string(k, out);
                out.push(':');
                go_json_marshal(&m[*k], out);
            }
            out.push('}');
        }
    }
}

fn go_float_json(f: f64) -> String {
    let a = f.abs();
    if a != 0.0 && !(1e-6..1e21).contains(&a) {
        let s = format!("{f:e}");
        // Rust: 1e21 / 1e-7 ; Go: 1e+21 / 1e-07 -> cleaned to 1e-7
        return match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        };
    }
    format!("{f}")
}

fn go_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rel_matches_go() {
        assert_eq!(go_rel("/a/b", "/a/b/c/d").unwrap(), "c/d");
        assert_eq!(go_rel("/a/b", "/a/x").unwrap(), "../x");
        assert_eq!(go_rel("a", "a").unwrap(), ".");
        assert!(go_rel("/a", "b").is_err());
    }

    #[test]
    fn ext_base() {
        assert_eq!(go_ext("a/b.tar.gz"), ".gz");
        assert_eq!(go_ext("a.b/c"), "");
    }

    #[test]
    fn json_marshal_sorted_and_escaped() {
        let mut s = String::new();
        go_json_marshal(&json!({"b": "<x>", "a": [1, null, true]}), &mut s);
        assert_eq!(s, r#"{"a":[1,null,true],"b":"\u003cx\u003e"}"#);
    }

    #[test]
    fn go_regex_ascii_classes() {
        let re = go_regex(r"\d+").unwrap();
        assert!(re.is_match("42"));
        assert!(!re.is_match("\u{0664}")); // Arabic-Indic digit
        let re = go_regex(r"[\w.]+").unwrap();
        assert_eq!(re.find("ab.c d").unwrap().as_str(), "ab.c");
    }
}

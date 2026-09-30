//! Filesystem ops (files.go).
use crate::group_b::util::*;
use perch_interpreter::{err, handler, Bindings, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;

type Args<'a> = perch_interpreter::Args<'a>;

pub fn register(m: &mut HashMap<String, perch_interpreter::Handler>) {
    let mut add = |k: &str, f: fn(&Interpreter, &mut Bindings, &Args<'_>) -> Result<Value>| {
        m.insert(k.to_string(), handler(f));
    };
    add("mkdir", op_mkdir);
    add("cp", op_cp);
    add("mv", op_mv);
    add("rm", op_rm);
    add("cd", op_cd);
    add("chmod", op_chmod);
    add("touch", op_touch);
    add("write_file", op_write_file);
    add("read_file", op_read_file);
    add("exists", op_exists);
    add("is_dir", op_is_dir);
    add("is_file", op_is_file);
    add("file_size", op_file_size);
    add("make_executable", op_make_executable);
    add("ensure_dir", op_ensure_dir);
    add("copy_dir", op_copy_dir);
    add("append_file", op_append_file);
    add("append_line", op_append_line);
    add("ensure_line_in_file", op_ensure_line_in_file);
    add("replace_in_file", op_replace_in_file);
    add("backup_file", op_backup_file);
    add("glob", op_glob);
    add("list_dir", op_list_dir);
    add("symlink", op_symlink);
    add("read_link", op_read_link);
    add("mktemp_dir", op_mktemp_dir);
    add("mktemp_file", op_mktemp_file);
}

fn op_mkdir(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    mkdir_all(&resolve(&arg_string(a, &["path", "_0"]), b), 0o755)?;
    Ok(Value::Null)
}

fn op_cp(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src", "_0"]), b);
    let dst = resolve(&arg_string(a, &["dst", "_1"]), b);
    let mut inp = open_file(&src)?;
    mkdir_all(&go_dir(&dst), 0o755)?;
    let mut out = create_file(&dst)?;
    copy_stream(&mut out, &mut inp, &src)?;
    Ok(Value::Null)
}

fn op_mv(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src", "_0"]), b);
    let dst = resolve(&arg_string(a, &["dst", "_1"]), b);
    fs::rename(&src, &dst).map_err(|e| link_err("rename", &src, &dst, &e))?;
    Ok(Value::Null)
}

fn op_rm(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    match fs::symlink_metadata(&p) {
        Err(e) if is_not_exist(&e) || super::fsx::is_not_dir(&e) => Ok(Value::Null),
        Err(e) => Err(path_err("lstat", &p, &e)),
        Ok(md) => {
            let r = if md.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
            match r {
                Ok(()) => Ok(Value::Null),
                Err(e) if is_not_exist(&e) => Ok(Value::Null),
                Err(e) => Err(path_err("unlinkat", &p, &e)),
            }
        }
    }
}

fn op_cd(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    b.cwd = resolve(&arg_string(a, &["path", "_0"]), b);
    Ok(Value::Null)
}

fn op_chmod(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let mode = arg_string(a, &["mode"]);
    let valid = !mode.is_empty() && mode.bytes().all(|c| (b'0'..=b'7').contains(&c));
    let n = if valid { u32::from_str_radix(&mode, 8).ok() } else { None };
    let Some(n) = n else {
        return Err(err(format!("chmod: invalid mode {}", perch_interpreter::go_quote(&mode))));
    };
    let p = resolve(&arg_string(a, &["path"]), b);
    super::fsx::set_mode(&p, n & 0o777).map_err(|e| path_err("chmod", &p, &e))?;
    Ok(Value::Null)
}

fn op_touch(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let mut o = fs::OpenOptions::new();
    o.read(true).write(true).create(true).truncate(false);
    super::fsx::open_mode(&mut o, 0o644)
        .open(&p)
        .map_err(|e| path_err("open", &p, &e))?;
    Ok(Value::Null)
}

fn write_bytes(p: &str, data: &[u8]) -> Result<()> {
    let mut f = create_with_mode(p, 0o644)?;
    f.write_all(data).map_err(|e| path_err("write", p, &e))
}

fn op_write_file(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path"]), b);
    write_bytes(&p, arg_string(a, &["content"]).as_bytes())?;
    Ok(Value::Null)
}

fn read_file(p: &str) -> Result<Vec<u8>> {
    let mut f = open_file(p)?;
    let mut v = Vec::new();
    std::io::Read::read_to_end(&mut f, &mut v).map_err(|e| path_err("read", p, &e))?;
    Ok(v)
}

fn op_read_file(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    Ok(Value::String(String::from_utf8_lossy(&read_file(&p)?).into_owned()))
}

fn op_exists(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    Ok(Value::Bool(fs::metadata(resolve(&arg_string(a, &["path", "_0"]), b)).is_ok()))
}

fn op_is_dir(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    Ok(Value::Bool(fs::metadata(resolve(&arg_string(a, &["path", "_0"]), b)).map(|m| m.is_dir()).unwrap_or(false)))
}

fn op_is_file(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    Ok(Value::Bool(fs::metadata(resolve(&arg_string(a, &["path", "_0"]), b)).map(|m| !m.is_dir()).unwrap_or(false)))
}

fn op_file_size(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let md = fs::metadata(&p).map_err(|e| path_err("stat", &p, &e))?;
    Ok(Value::from(md.len() as i64))
}

fn op_make_executable(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let md = fs::metadata(&p).map_err(|e| path_err("stat", &p, &e))?;
    super::fsx::set_mode(&p, (super::fsx::mode_of(&md) & 0o7777) | 0o111)
        .map_err(|e| path_err("chmod", &p, &e))?;
    Ok(Value::Null)
}

fn op_ensure_dir(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    mkdir_all(&p, 0o755)?;
    Ok(Value::String(go_abs(&p).unwrap_or_default()))
}

fn op_copy_dir(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src", "_0"]), b);
    let dst = resolve(&arg_string(a, &["dst", "_1"]), b);
    walk(&src, &mut |path, md| {
        let rel = go_rel(&src, path).map_err(err)?;
        let out = go_join(&[&dst, &rel]);
        if md.is_dir() {
            if let Ok(m) = fs::metadata(&out) {
                if m.is_dir() {
                    return Ok(());
                }
            }
            let mut db = fs::DirBuilder::new();
            db.recursive(true);
            return super::fsx::dir_mode(&mut db, super::fsx::mode_of(md) & 0o777)
                .create(&out)
                .map_err(|e| path_err("mkdir", &out, &e));
        }
        copy_file(path, &out)
    })?;
    Ok(Value::Null)
}

fn open_append(p: &str) -> Result<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.append(true).create(true);
    super::fsx::open_mode(&mut o, 0o644).open(p).map_err(|e| path_err("open", p, &e))
}

fn op_append_file(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let content = arg_string(a, &["content", "_1"]);
    let mut f = open_append(&p)?;
    f.write_all(content.as_bytes()).map_err(|e| path_err("write", &p, &e))?;
    Ok(Value::Null)
}

fn op_append_line(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let line = arg_string(a, &["line", "_1"]);
    let mut f = open_append(&p)?;
    let data = read_file(&p).unwrap_or_default();
    let prefix = if !data.is_empty() && data[data.len() - 1] != b'\n' { "\n" } else { "" };
    f.write_all(format!("{prefix}{line}\n").as_bytes()).map_err(|e| path_err("write", &p, &e))?;
    Ok(Value::Null)
}

fn contains_line(haystack: &[u8], line: &str) -> bool {
    haystack.split(|&c| c == b'\n').any(|l| {
        let mut l = l;
        while let Some((&b'\r', rest)) = l.split_last() {
            l = rest;
        }
        l == line.as_bytes()
    })
}

/// install.go `ensureLineInFile`.
fn ensure_line_in_file(path: &str, line: &str) -> Result<bool> {
    let data = match read_file(path) {
        Ok(d) => d,
        Err(e) => {
            // only "does not exist" is tolerated
            if fs::metadata(path).is_err() && fs::symlink_metadata(path).is_err() {
                Vec::new()
            } else {
                return Err(e);
            }
        }
    };
    if contains_line(&data, line) {
        return Ok(false);
    }
    let mut f = open_append(path)?;
    let prefix = if !data.is_empty() && data[data.len() - 1] != b'\n' { "\n" } else { "" };
    f.write_all(format!("{prefix}{line}\n").as_bytes()).map_err(|e| path_err("write", path, &e))?;
    Ok(true)
}

fn op_ensure_line_in_file(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    Ok(Value::Bool(ensure_line_in_file(&p, &arg_string(a, &["line", "_1"]))?))
}

fn replace_all_bytes(data: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    if old.is_empty() {
        // strings.ReplaceAll with "" inserts between every rune.
        let s = String::from_utf8_lossy(data);
        let mut out = Vec::new();
        out.extend_from_slice(new);
        for c in s.chars() {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            out.extend_from_slice(new);
        }
        return out;
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i..].starts_with(old) {
            out.extend_from_slice(new);
            i += old.len();
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

fn op_replace_in_file(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let old = arg_string(a, &["old", "_1"]);
    let new = arg_string(a, &["new", "_2"]);
    let data = read_file(&p)?;
    write_bytes(&p, &replace_all_bytes(&data, old.as_bytes(), new.as_bytes()))?;
    Ok(Value::Null)
}

fn op_backup_file(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let bak = format!("{p}.bak");
    copy_file(&p, &bak)?;
    Ok(Value::String(bak))
}

// ── glob (Go filepath.Match / filepath.Glob) ──────────────────────────────

const BAD_PATTERN: &str = "syntax error in pattern";

fn scan_chunk(pattern: &[char]) -> (bool, Vec<char>, Vec<char>) {
    let mut p = pattern;
    let mut star = false;
    while !p.is_empty() && p[0] == '*' {
        p = &p[1..];
        star = true;
    }
    let mut inrange = false;
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            '\\' => {
                if i + 1 < p.len() {
                    i += 1;
                }
            }
            '[' => inrange = true,
            ']' => inrange = false,
            '*' if !inrange => break,
            _ => {}
        }
        i += 1;
    }
    (star, p[..i].to_vec(), p[i..].to_vec())
}

fn get_esc(chunk: &[char]) -> std::result::Result<(char, Vec<char>), ()> {
    if chunk.is_empty() || chunk[0] == '-' || chunk[0] == ']' {
        return Err(());
    }
    let mut c = chunk;
    if c[0] == '\\' {
        c = &c[1..];
        if c.is_empty() {
            return Err(());
        }
    }
    let r = c[0];
    let next = c[1..].to_vec();
    if next.is_empty() {
        return Err(());
    }
    Ok((r, next))
}

/// Returns (rest, matched) or a bad-pattern error.
fn match_chunk(chunk: &[char], s: &[char]) -> std::result::Result<(Vec<char>, bool), ()> {
    let mut chunk = chunk.to_vec();
    let mut s = s.to_vec();
    let mut failed = false;
    while !chunk.is_empty() {
        if !failed && s.is_empty() {
            failed = true;
        }
        match chunk[0] {
            '[' => {
                let mut r = '\0';
                if !failed {
                    r = s[0];
                    s.remove(0);
                }
                chunk.remove(0);
                let mut negated = false;
                if !chunk.is_empty() && chunk[0] == '^' {
                    negated = true;
                    chunk.remove(0);
                }
                let mut matched = false;
                let mut nrange = 0;
                loop {
                    if !chunk.is_empty() && chunk[0] == ']' && nrange > 0 {
                        chunk.remove(0);
                        break;
                    }
                    let (lo, rest) = get_esc(&chunk)?;
                    chunk = rest;
                    let mut hi = lo;
                    if chunk[0] == '-' {
                        let (h, rest) = get_esc(&chunk[1..])?;
                        hi = h;
                        chunk = rest;
                    }
                    if lo <= r && r <= hi {
                        matched = true;
                    }
                    nrange += 1;
                }
                if matched == negated {
                    failed = true;
                }
            }
            '?' => {
                if !failed {
                    if s[0] == '/' {
                        failed = true;
                    }
                    s.remove(0);
                }
                chunk.remove(0);
            }
            c => {
                if c == '\\' {
                    chunk.remove(0);
                    if chunk.is_empty() {
                        return Err(());
                    }
                }
                if !failed {
                    if chunk[0] != s[0] {
                        failed = true;
                    }
                    s.remove(0);
                }
                chunk.remove(0);
            }
        }
    }
    if failed {
        return Ok((Vec::new(), false));
    }
    Ok((s, true))
}

fn go_match(pattern: &str, name: &str) -> std::result::Result<bool, ()> {
    let mut pattern: Vec<char> = pattern.chars().collect();
    let mut name: Vec<char> = name.chars().collect();
    'pat: while !pattern.is_empty() {
        let (star, chunk, rest) = scan_chunk(&pattern);
        pattern = rest;
        if star && chunk.is_empty() {
            return Ok(!name.contains(&'/'));
        }
        let m = match_chunk(&chunk, &name);
        if let Ok((t, true)) = &m {
            if t.is_empty() || !pattern.is_empty() {
                name = t.clone();
                continue;
            }
        }
        m?;
        if star {
            let mut i = 0;
            while i < name.len() && name[i] != '/' {
                let (t, ok) = match_chunk(&chunk, &name[i + 1..])?;
                if ok {
                    if pattern.is_empty() && !t.is_empty() {
                        i += 1;
                        continue;
                    }
                    name = t;
                    continue 'pat;
                }
                i += 1;
            }
        }
        while !pattern.is_empty() {
            let (_, chunk, rest) = scan_chunk(&pattern);
            pattern = rest;
            match_chunk(&chunk, &[])?;
        }
        return Ok(false);
    }
    Ok(name.is_empty())
}

fn has_meta(p: &str) -> bool {
    p.contains(['*', '?', '[', '\\'])
}

fn glob_dir(dir: &str, pattern: &str, matches: &mut Vec<String>) -> std::result::Result<(), ()> {
    let Ok(md) = fs::metadata(dir) else { return Ok(()) };
    if !md.is_dir() {
        return Ok(());
    }
    let Ok(rd) = fs::read_dir(dir) else { return Ok(()) };
    let mut names: Vec<String> = rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    for n in names {
        if go_match(pattern, &n)? {
            matches.push(go_join(&[dir, &n]));
        }
    }
    Ok(())
}

fn go_glob(pattern: &str) -> std::result::Result<Vec<String>, ()> {
    go_match(pattern, "")?;
    if !has_meta(pattern) {
        return Ok(if fs::symlink_metadata(pattern).is_ok() { vec![pattern.to_string()] } else { Vec::new() });
    }
    let split = pattern.rfind('/').map(|i| i + 1).unwrap_or(0);
    let (dir, file) = (&pattern[..split], &pattern[split..]);
    let dir = match dir {
        "" => ".".to_string(),
        "/" => "/".to_string(),
        d => d[..d.len() - 1].to_string(),
    };
    let mut matches = Vec::new();
    if !has_meta(&dir) {
        glob_dir(&dir, file, &mut matches)?;
        return Ok(matches);
    }
    if dir == pattern {
        return Err(());
    }
    for d in go_glob(&dir)? {
        glob_dir(&d, file, &mut matches)?;
    }
    Ok(matches)
}

fn op_glob(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let pattern = resolve(&arg_string(a, &["pattern", "_0"]), b);
    match go_glob(&pattern) {
        Ok(m) => Ok(Value::String(m.join("\n"))),
        Err(()) => Err(err(BAD_PATTERN)),
    }
}

fn op_list_dir(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let rd = fs::read_dir(&p).map_err(|e| path_err("open", &p, &e))?;
    let mut names = Vec::new();
    for e in rd {
        names.push(e.map_err(|e| path_err("readdirent", &p, &e))?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(Value::String(names.join("\n")))
}

fn op_symlink(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let target = arg_string(a, &["target", "_0"]);
    let link = resolve(&arg_string(a, &["link", "_1"]), b);
    let _ = fs::remove_file(&link);
    super::fsx::symlink(&target, &link).map_err(|e| link_err("symlink", &target, &link, &e))?;
    Ok(Value::Null)
}

fn op_read_link(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let p = resolve(&arg_string(a, &["path", "_0"]), b);
    let t = fs::read_link(&p).map_err(|e| path_err("readlink", &p, &e))?;
    Ok(Value::String(t.to_string_lossy().into_owned()))
}

fn op_mktemp_dir(_i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let mut prefix = arg_string(a, &["prefix", "_0"]);
    if prefix.is_empty() {
        prefix = "perch-".into();
    }
    Ok(Value::String(mktemp(&format!("{prefix}*"), true)?))
}

fn op_mktemp_file(_i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let mut prefix = arg_string(a, &["prefix", "_0"]);
    if prefix.is_empty() {
        prefix = "perch-".into();
    }
    Ok(Value::String(mktemp(&format!("{prefix}*"), false)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_basics() {
        assert_eq!(go_match("*.go", "a.go"), Ok(true));
        assert_eq!(go_match("*.go", "a/b.go"), Ok(false));
        assert_eq!(go_match("a?c", "abc"), Ok(true));
        assert_eq!(go_match("[a-c]x", "bx"), Ok(true));
        assert_eq!(go_match("[^a-c]x", "bx"), Ok(false));
        assert_eq!(go_match("[", "a"), Err(()));
        assert_eq!(go_match("a\\*", "a*"), Ok(true));
    }

    #[test]
    fn ensure_line_idempotent() {
        let d = mktemp("perch-t-*", true).unwrap();
        let p = format!("{d}/f");
        assert!(ensure_line_in_file(&p, "x").unwrap());
        assert!(!ensure_line_in_file(&p, "x").unwrap());
        assert_eq!(fs::read_to_string(&p).unwrap(), "x\n");
        fs::remove_dir_all(&d).unwrap();
    }
}

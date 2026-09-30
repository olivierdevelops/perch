//! Small helpers that reproduce Go's `fmt` / `path/filepath` behavior where the
//! report text depends on it (`%q`, `%v`, `filepath.Clean`).
use serde_json::Value;

/// Go's `strconv.IsPrint`, approximated for non-ASCII (letters, marks, numbers,
/// punctuation, symbols; not spaces, controls, format or private-use chars).
fn is_print(c: char) -> bool {
    let u = c as u32;
    if u < 0x80 {
        return (0x20..0x7f).contains(&u);
    }
    if c.is_control() || c.is_whitespace() {
        return false;
    }
    !matches!(u,
        0x00ad | 0x0600..=0x0605 | 0x061c | 0x06dd | 0x070f | 0x180e
        | 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x206f
        | 0xe000..=0xf8ff | 0xfeff | 0xfff9..=0xfffb | 0xfffe | 0xffff
        | 0xf0000..=0x10ffff)
}

fn push_valid(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            c if is_print(c) => out.push(c),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
}

/// Go's `%q` on a byte string (invalid UTF-8 bytes become `\xNN`).
pub fn quote_bytes(b: &[u8]) -> String {
    let mut out = String::from("\"");
    let mut rest = b;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                push_valid(&mut out, s);
                break;
            }
            Err(e) => {
                let (valid, after) = rest.split_at(e.valid_up_to());
                push_valid(&mut out, std::str::from_utf8(valid).unwrap());
                let n = e.error_len().unwrap_or(after.len());
                for x in &after[..n] {
                    out.push_str(&format!("\\x{x:02x}"));
                }
                rest = &after[n..];
            }
        }
    }
    out.push('"');
    out
}

/// Go's `%q` on a string.
pub fn quote(s: &str) -> String {
    quote_bytes(s.as_bytes())
}

/// Go's `%v` on a `[]string`.
pub fn v_strings(v: &[String]) -> String {
    format!("[{}]", v.join(" "))
}

/// Go's `%v` on a decoded JSON value (unused by the simulator's own output but
/// kept for symmetry with sibling crates).
#[allow(dead_code)]
pub fn v_value(v: &Value) -> String {
    match v {
        Value::Null => "<nil>".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Go's `filepath.Clean` for slash-separated paths.
pub fn clean(path: &str) -> String {
    if path.is_empty() {
        return ".".into();
    }
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    match (rooted, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".into(),
        (false, false) => joined,
    }
}

/// Go's `filepath.Join` for two elements.
pub fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => String::new(),
        (true, false) => clean(b),
        (false, true) => clean(a),
        _ => clean(&format!("{a}/{b}")),
    }
}

/// Formats an io error the way Go's `*PathError` prints (`open P: no such file
/// or directory`).
pub fn path_err(op: &str, path: &str, e: &std::io::Error) -> String {
    let msg = match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => {
            let s = e.to_string();
            let s = match s.rfind(" (os error") {
                Some(i) => s[..i].to_string(),
                None => s,
            };
            let mut cs = s.chars();
            match cs.next() {
                Some(c) => c.to_lowercase().collect::<String>() + cs.as_str(),
                None => s,
            }
        }
    };
    format!("{op} {path}: {msg}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_matches_go() {
        assert_eq!(quote("a\"b\\c\n"), r#""a\"b\\c\n""#);
        assert_eq!(quote("é✓"), "\"é✓\"");
        assert_eq!(quote("\u{7f}\u{1}"), "\"\\x7f\\x01\"");
        assert_eq!(quote("\u{a0}"), "\"\\u00a0\"");
        assert_eq!(quote_bytes(&[b'a', 0xff, b'b']), "\"a\\xffb\"");
    }

    #[test]
    fn clean_matches_go() {
        for (i, o) in [("", "."), ("/", "/"), ("a//b/./c", "a/b/c"), ("/a/../..", "/"), ("../a/..", ".."), ("a/b/..", "a"), ("/a/b/", "/a/b"), ("./", ".")] {
            assert_eq!(clean(i), o, "{i}");
        }
        assert_eq!(join("/", "x/y"), "/x/y");
        assert_eq!(join("/srv", "../etc"), "/etc");
    }
}

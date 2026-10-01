//! Cross-platform path ops (paths.go). Unix `filepath` semantics.
use crate::common;
use crate::group_b::util::*;
use perch_interpreter::{err, handler, to_string_value, Args, Handler, Result};
use serde_json::Value;
use std::collections::HashMap;

pub fn register(m: &mut HashMap<String, Handler>) {
    let mut add = |k: &str, f: fn(&Args<'_>) -> Result<Value>| {
        m.insert(k.to_string(), pure(f));
    };
    add("path_join", |a| {
        let parts = collect_positional(a);
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        Ok(Value::String(common::to_native(go_join(&refs))))
    });
    add("path_dir", |a| Ok(Value::String(common::to_native(go_dir(&arg_string(a, &["path", "_0"]))))));
    add("path_base", |a| Ok(Value::String(go_base(&arg_string(a, &["path", "_0"])))));
    add("path_ext", |a| Ok(Value::String(go_ext(&arg_string(a, &["path", "_0"])))));
    add("path_abs", |a| {
        let p = arg_string(a, &["path", "_0"]);
        go_abs(&p).map(|s| Value::String(common::to_native(s))).map_err(|e| err(go_io_msg(&e)))
    });
    add("path_clean", |a| Ok(Value::String(common::to_native(go_clean(&arg_string(a, &["path", "_0"]))))));
    add("path_rel", |a| {
        go_rel(&arg_string(a, &["base", "_0"]), &arg_string(a, &["target", "_1"])).map(Value::String).map_err(err)
    });
    add("path_with_ext", |a| {
        let p = arg_string(a, &["path", "_0"]);
        let mut ext = arg_string(a, &["ext", "_1"]);
        if !ext.starts_with('.') && !ext.is_empty() {
            ext = format!(".{ext}");
        }
        let cur = go_ext(&p);
        let stem = p.strip_suffix(cur.as_str()).unwrap_or(&p);
        Ok(Value::String(format!("{stem}{ext}")))
    });
    add("is_abs", |a| Ok(Value::Bool(is_abs(&arg_string(a, &["path", "_0"])))));
    // ToSlash / FromSlash: identity on Unix; `\` <-> `/` on Windows.
    add("to_slash", |a| Ok(Value::String(common::slashed(&arg_string(a, &["path", "_0"])).into_owned())));
    add("from_slash", |a| Ok(Value::String(common::to_native(arg_string(a, &["path", "_0"])))));

    // expand_path "~/.config/x" -> "/Users/me/.config/x"; also $VAR expansion.
    m.insert(
        "expand_path".into(),
        handler(|_i, _b, a| {
            let p = arg_string(a, &["path", "_0"]);
            if p.is_empty() {
                return Ok(Value::String(String::new()));
            }
            if let Some(rest) = p.strip_prefix('~') {
                if rest.is_empty() || rest.starts_with('/') {
                    if let Some(h) = std::env::var("HOME").ok().filter(|h| !h.is_empty()) {
                        return Ok(Value::String(go_join(&[&h, rest.trim_start_matches('/')])));
                    }
                }
            }
            Ok(Value::String(expand_env(&p)))
        }),
    );
}

/// Args `_0`, `_1`, … in order, stopping at the first gap.
fn collect_positional(a: &Args<'_>) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0;
    while let Some(v) = a.map.get(&format!("_{idx}")) {
        out.push(to_string_value(v));
        idx += 1;
    }
    out
}

fn is_shell_special(c: u8) -> bool {
    matches!(c, b'*' | b'#' | b'$' | b'@' | b'!' | b'?' | b'-') || c.is_ascii_digit()
}

/// Go `getShellName`: (name, bytes consumed).
fn get_shell_name(s: &[u8]) -> (String, usize) {
    if s[0] == b'{' {
        if s.len() > 2 && is_shell_special(s[1]) && s[2] == b'}' {
            return ((s[1] as char).to_string(), 3);
        }
        for i in 1..s.len() {
            if s[i] == b'}' {
                if i == 1 {
                    return (String::new(), 2);
                }
                return (String::from_utf8_lossy(&s[1..i]).into_owned(), i + 1);
            }
        }
        return (String::new(), 1);
    }
    if is_shell_special(s[0]) {
        return ((s[0] as char).to_string(), 1);
    }
    let mut i = 0;
    while i < s.len() && (s[i] == b'_' || s[i].is_ascii_alphanumeric()) {
        i += 1;
    }
    (String::from_utf8_lossy(&s[..i]).into_owned(), i)
}

/// Go `os.ExpandEnv`.
pub fn expand_env(s: &str) -> String {
    let b = s.as_bytes();
    let mut buf: Vec<u8> = Vec::new();
    let mut used = false;
    let mut i = 0;
    let mut j = 0;
    while j < b.len() {
        if b[j] == b'$' && j + 1 < b.len() {
            used = true;
            buf.extend_from_slice(&b[i..j]);
            let (name, w) = get_shell_name(&b[j + 1..]);
            if name.is_empty() && w > 0 {
                // invalid syntax: eat the characters
            } else if name.is_empty() {
                buf.push(b[j]);
            } else {
                buf.extend_from_slice(std::env::var(&name).unwrap_or_default().as_bytes());
            }
            j += w;
            i = j + 1;
        }
        j += 1;
    }
    if !used {
        return s.to_string();
    }
    buf.extend_from_slice(&b[i.min(b.len())..]);
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand() {
        std::env::set_var("PERCH_T_X", "v");
        assert_eq!(expand_env("a/$PERCH_T_X/${PERCH_T_X}b$"), "a/v/vb$");
        assert_eq!(expand_env("${}x"), "x");
    }
}

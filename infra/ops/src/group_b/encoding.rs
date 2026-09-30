//! base64 / hex / url / json ops (encoding.go).
use crate::group_b::util::*;
use base64::engine::{general_purpose::GeneralPurpose, general_purpose::GeneralPurposeConfig, DecodePaddingMode};
use base64::{alphabet, Engine};
use perch_interpreter::{err, go_quote, Args, Error, Handler, Result};
use serde_json::Value;
use std::collections::HashMap;

pub fn register(m: &mut HashMap<String, Handler>) {
    let mut add = |k: &str, f: fn(&Args<'_>) -> Result<Value>| {
        m.insert(k.to_string(), pure(f));
    };
    add("base64_encode", |a| {
        use base64::engine::general_purpose::STANDARD;
        Ok(Value::String(STANDARD.encode(arg_string(a, &["value", "_0"]))))
    });
    add("base64_decode", |a| base64_decode(&arg_string(a, &["value", "_0"])).map(Value::String));
    add("hex_encode", |a| Ok(Value::String(hex::encode(arg_string(a, &["value", "_0"])))));
    add("hex_decode", |a| hex_decode(&arg_string(a, &["value", "_0"])).map(Value::String));
    add("url_encode", |a| Ok(Value::String(query_escape(&arg_string(a, &["value", "_0"])))));
    add("url_decode", |a| query_unescape(&arg_string(a, &["value", "_0"])).map(Value::String));
    add("json_parse", |a| json_parse(&arg_string(a, &["value", "_0"])));
    add("json_stringify", |a| {
        let mut out = String::new();
        go_json_marshal(a.map.get("_0").unwrap_or(&Value::Null), &mut out);
        Ok(Value::String(out))
    });
    add("json_get", |a| {
        let mut v = a.map.get("_0").cloned().unwrap_or(Value::Null);
        let path = arg_string(a, &["path", "_1"]);
        if let Value::String(s) = &v {
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                v = parsed;
            }
        }
        for part in path.split('.') {
            let Value::Object(m) = &v else { return Ok(Value::Null) };
            v = m.get(part).cloned().unwrap_or(Value::Null);
        }
        Ok(v)
    });
}

fn base64_decode(s: &str) -> Result<String> {
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_allow_trailing_bits(true)
            .with_decode_padding_mode(DecodePaddingMode::RequireCanonical),
    );
    // Go's decoder ignores \r and \n.
    let clean: String = s.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    match engine.decode(clean.as_bytes()) {
        Ok(v) => Ok(String::from_utf8_lossy(&v).into_owned()),
        Err(e) => {
            let off = match e {
                base64::DecodeError::InvalidByte(o, _) => o,
                base64::DecodeError::InvalidLastSymbol(o, _) => o,
                base64::DecodeError::InvalidLength(_) => clean.len().min(clean.trim_end_matches('=').len()),
                _ => 0,
            };
            Err(err(format!("illegal base64 data at input byte {off}")))
        }
    }
}

fn hex_invalid(b: u8) -> Error {
    let c = b as char;
    let printable = !c.is_control();
    if printable {
        err(format!("encoding/hex: invalid byte: U+{:04X} '{c}'", b as u32))
    } else {
        err(format!("encoding/hex: invalid byte: U+{:04X}", b as u32))
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_decode(s: &str) -> Result<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks_exact(2) {
        let Some(hi) = hex_val(pair[0]) else { return Err(hex_invalid(pair[0])) };
        let Some(lo) = hex_val(pair[1]) else { return Err(hex_invalid(pair[1])) };
        out.push(hi << 4 | lo);
    }
    if b.len() % 2 == 1 {
        if hex_val(b[b.len() - 1]).is_none() {
            return Err(hex_invalid(b[b.len() - 1]));
        }
        return Err(err("encoding/hex: odd length hex string"));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::new();
    for &c in s.as_bytes() {
        match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(c as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{c:02X}")),
        }
    }
    out
}

/// Go `url.QueryUnescape`.
fn query_unescape(s: &str) -> Result<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let ok = i + 2 < b.len() && hex_val(b[i + 1]).is_some() && hex_val(b[i + 2]).is_some();
                if !ok {
                    let mut t = &s[i..];
                    if t.len() > 3 {
                        let mut end = 3;
                        while !t.is_char_boundary(end) {
                            end += 1;
                        }
                        t = &t[..end];
                    }
                    return Err(err(format!("invalid URL escape {}", go_quote(t))));
                }
                out.push(hex_val(b[i + 1]).unwrap() << 4 | hex_val(b[i + 2]).unwrap());
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// `json.Unmarshal` into `any`, with Go-flavoured error text for the common
/// failure shapes.
fn json_parse(s: &str) -> Result<Value> {
    match serde_json::from_str::<Value>(s) {
        Ok(v) => Ok(v),
        Err(e) => {
            if s.trim().is_empty() || e.is_eof() {
                return Err(err("unexpected end of JSON input"));
            }
            let msg = e.to_string();
            let lines: Vec<&str> = s.split('\n').collect();
            let ch = lines
                .get(e.line().saturating_sub(1))
                .and_then(|l| l.get(e.column().saturating_sub(1)..))
                .and_then(|t| t.chars().next());
            let ch = match ch {
                Some(c) => c,
                None => return Err(err(msg)),
            };
            let q = if ch == '\'' { "'\\''".to_string() } else { format!("'{ch}'") };
            let ctx = if msg.contains("trailing characters") { "after top-level value" } else { "looking for beginning of value" };
            Err(err(format!("invalid character {q} {ctx}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_url() {
        assert_eq!(hex_decode("6869").unwrap(), "hi");
        assert_eq!(hex_decode("zz").unwrap_err().to_string(), "encoding/hex: invalid byte: U+007A 'z'");
        assert_eq!(hex_decode("abc").unwrap_err().to_string(), "encoding/hex: odd length hex string");
        assert_eq!(query_escape("a b&c/é"), "a+b%26c%2F%C3%A9");
        assert_eq!(query_unescape("a+b%26").unwrap(), "a b&");
        assert_eq!(query_unescape("%zz").unwrap_err().to_string(), "invalid URL escape \"%zz\"");
        assert_eq!(query_unescape("%4").unwrap_err().to_string(), "invalid URL escape \"%4\"");
    }

    #[test]
    fn base64_round() {
        assert_eq!(base64_decode("aGk=").unwrap(), "hi");
        assert_eq!(base64_decode("aG\nk=").unwrap(), "hi");
        assert!(base64_decode("aGk").is_err());
    }

    #[test]
    fn json_errors() {
        assert_eq!(json_parse("").unwrap_err().to_string(), "unexpected end of JSON input");
        assert_eq!(json_parse("{").unwrap_err().to_string(), "unexpected end of JSON input");
        assert_eq!(json_parse("x").unwrap_err().to_string(), "invalid character 'x' looking for beginning of value");
    }
}

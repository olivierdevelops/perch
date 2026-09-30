//! Go `encoding/json` compatible encoding (sorted map keys, HTML-safe escaping,
//! float64 numbers) and Go-worded decode errors.
use crate::gofmt::json_float;
use serde_json::Value;
use std::collections::BTreeMap;

/// A JSON tree that keeps Go's two object flavours apart: struct objects keep
/// declaration order, maps sort their keys.
#[derive(Debug, Clone)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    /// Go struct: fields in declaration order.
    Obj(Vec<(String, J)>),
    /// Go map: keys sorted bytewise.
    Map(BTreeMap<String, J>),
}

impl J {
    pub fn s(v: impl Into<String>) -> J {
        J::Str(v.into())
    }
    /// A nil Go slice marshals as `null`; a non-nil one as an array.
    pub fn arr_or_null(v: Vec<J>) -> J {
        if v.is_empty() {
            J::Null
        } else {
            J::Arr(v)
        }
    }
    pub fn strs(v: &[String]) -> J {
        J::Arr(v.iter().map(|s| J::Str(s.clone())).collect())
    }
    pub fn map(pairs: Vec<(&str, J)>) -> J {
        J::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

/// Converts a decoded value; numbers become float64 like Go's `any` decoding.
pub fn from_value(v: &Value) -> J {
    match v {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Number(n) => J::Num(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => J::Str(s.clone()),
        Value::Array(a) => J::Arr(a.iter().map(from_value).collect()),
        Value::Object(m) => J::Map(m.iter().map(|(k, v)| (k.clone(), from_value(v))).collect()),
    }
}

fn write_str(out: &mut String, s: &str) {
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
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_j(out: &mut String, j: &J) {
    match j {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Num(n) => out.push_str(&json_float(*n)),
        J::Str(s) => write_str(out, s),
        J::Arr(a) => {
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_j(out, e);
            }
            out.push(']');
        }
        J::Obj(fields) => {
            out.push('{');
            for (i, (k, v)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(out, k);
                out.push(':');
                write_j(out, v);
            }
            out.push('}');
        }
        J::Map(m) => {
            out.push('{');
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(out, k);
                out.push(':');
                write_j(out, v);
            }
            out.push('}');
        }
    }
}

/// `json.NewEncoder(w).Encode(v)`: compact JSON plus a trailing newline.
pub fn encode_line(j: &J) -> Vec<u8> {
    let mut s = String::new();
    write_j(&mut s, j);
    s.push('\n');
    s.into_bytes()
}

// ── decoding ────────────────────────────────────────────────────────────

fn quote_char(c: u8) -> String {
    if c == b'\'' {
        return "'\\''".into();
    }
    if c == b'"' {
        return "'\"'".into();
    }
    let ch = c as char;
    let q = match ch {
        '\\' => "\\\\".to_string(),
        '\n' => "\\n".into(),
        '\r' => "\\r".into(),
        '\t' => "\\t".into(),
        c if (c as u32) < 0x20 || c as u32 == 0x7f => format!("\\x{:02x}", c as u32),
        c => c.to_string(),
    };
    format!("'{q}'")
}

struct Scan<'a> {
    b: &'a [u8],
    i: usize,
}

type R = Result<(), String>;

impl Scan<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }
    fn bad(&self, ctx: &str) -> String {
        format!("invalid character {} {}", quote_char(self.b[self.i]), ctx)
    }
    fn peek(&mut self) -> Result<u8, String> {
        if self.i >= self.b.len() {
            return Err("unexpected EOF".into());
        }
        Ok(self.b[self.i])
    }
    fn value(&mut self) -> R {
        self.ws();
        let c = self.peek()?;
        match c {
            b'{' => {
                self.i += 1;
                self.ws();
                let c = self.peek()?;
                if c == b'}' {
                    self.i += 1;
                    return Ok(());
                }
                loop {
                    self.ws();
                    let c = self.peek()?;
                    if c != b'"' {
                        return Err(self.bad("looking for beginning of object key string"));
                    }
                    self.string()?;
                    self.ws();
                    let c = self.peek()?;
                    if c != b':' {
                        return Err(self.bad("after object key"));
                    }
                    self.i += 1;
                    self.value()?;
                    self.ws();
                    let c = self.peek()?;
                    match c {
                        b',' => self.i += 1,
                        b'}' => {
                            self.i += 1;
                            return Ok(());
                        }
                        _ => return Err(self.bad("after object key:value pair")),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                self.ws();
                let c = self.peek()?;
                if c == b']' {
                    self.i += 1;
                    return Ok(());
                }
                loop {
                    self.value()?;
                    self.ws();
                    let c = self.peek()?;
                    match c {
                        b',' => self.i += 1,
                        b']' => {
                            self.i += 1;
                            return Ok(());
                        }
                        _ => return Err(self.bad("after array element")),
                    }
                }
            }
            b'"' => self.string(),
            b'-' | b'0'..=b'9' => self.number(),
            b't' => self.literal(b"true"),
            b'f' => self.literal(b"false"),
            b'n' => self.literal(b"null"),
            _ => Err(self.bad("looking for beginning of value")),
        }
    }
    fn literal(&mut self, lit: &[u8]) -> R {
        self.i += 1;
        for &want in &lit[1..] {
            if self.i >= self.b.len() {
                return Err("unexpected EOF".into());
            }
            if self.b[self.i] != want {
                return Err(self.bad(&format!(
                    "in literal {} (expecting {})",
                    String::from_utf8_lossy(lit),
                    quote_char(want)
                )));
            }
            self.i += 1;
        }
        Ok(())
    }
    fn string(&mut self) -> R {
        self.i += 1;
        loop {
            let c = self.peek()?;
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(());
                }
                b'\\' => {
                    self.i += 1;
                    let e = self.peek()?;
                    match e {
                        b'b' | b'f' | b'n' | b'r' | b't' | b'\\' | b'/' | b'"' => self.i += 1,
                        b'u' => {
                            self.i += 1;
                            for _ in 0..4 {
                                let h = self.peek()?;
                                if !h.is_ascii_hexdigit() {
                                    return Err(self.bad("in \\u hexadecimal character escape"));
                                }
                                self.i += 1;
                            }
                        }
                        _ => return Err(self.bad("in string escape code")),
                    }
                }
                c if c < 0x20 => return Err(self.bad("in string literal")),
                _ => self.i += 1,
            }
        }
    }
    fn digits(&mut self) -> usize {
        let s = self.i;
        while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
            self.i += 1;
        }
        self.i - s
    }
    fn number(&mut self) -> R {
        if self.b[self.i] == b'-' {
            self.i += 1;
            let c = self.peek()?;
            if !c.is_ascii_digit() {
                return Err(self.bad("in numeric literal"));
            }
        }
        if self.b[self.i] == b'0' {
            self.i += 1;
        } else {
            self.digits();
        }
        if self.i < self.b.len() && self.b[self.i] == b'.' {
            self.i += 1;
            let c = self.peek()?;
            if !c.is_ascii_digit() {
                return Err(self.bad("after decimal point in numeric literal"));
            }
            self.digits();
        }
        if self.i < self.b.len() && matches!(self.b[self.i], b'e' | b'E') {
            self.i += 1;
            if self.i < self.b.len() && matches!(self.b[self.i], b'+' | b'-') {
                self.i += 1;
            }
            let c = self.peek()?;
            if !c.is_ascii_digit() {
                return Err(self.bad("in exponent of numeric literal"));
            }
            self.digits();
        }
        Ok(())
    }
}

/// Decodes the first JSON value of `body` like `json.NewDecoder(body).Decode`:
/// returns the value or Go's error text (`EOF`, `unexpected EOF`,
/// `invalid character ...`).
pub fn decode_first(body: &[u8]) -> Result<Value, String> {
    let mut sc = Scan { b: body, i: 0 };
    sc.ws();
    if sc.i >= body.len() {
        return Err("EOF".into());
    }
    let start = sc.i;
    sc.value()?;
    serde_json::from_slice::<Value>(&body[start..sc.i]).map_err(|e| e.to_string())
}

/// Go's name for the JSON kind of a value in `UnmarshalTypeError`.
pub fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_match_go() {
        assert_eq!(decode_first(b"").unwrap_err(), "EOF");
        assert_eq!(decode_first(b"  ").unwrap_err(), "EOF");
        assert_eq!(decode_first(b"{\"a\":").unwrap_err(), "unexpected EOF");
        assert_eq!(decode_first(b"x").unwrap_err(), "invalid character 'x' looking for beginning of value");
        assert_eq!(
            decode_first(b"{a:1}").unwrap_err(),
            "invalid character 'a' looking for beginning of object key string"
        );
        assert_eq!(decode_first(b"{\"a\" 1}").unwrap_err(), "invalid character '1' after object key");
        assert_eq!(
            decode_first(b"{\"a\":1 \"b\"}").unwrap_err(),
            "invalid character '\"' after object key:value pair"
        );
        assert_eq!(decode_first(b"[1 2]").unwrap_err(), "invalid character '2' after array element");
        assert_eq!(decode_first(b"tru!").unwrap_err(), "invalid character '!' in literal true (expecting 'e')");
        assert!(decode_first(b"{\"a\":1} trailing").is_ok());
    }

    #[test]
    fn encode_sorted_and_escaped() {
        let j = J::map(vec![("b", J::s("<&>")), ("a", J::Num(1.0))]);
        assert_eq!(String::from_utf8(encode_line(&j)).unwrap(), "{\"a\":1,\"b\":\"\\u003c\\u0026\\u003e\"}\n");
    }
}

//! A small port of Go's `fmt.Sprintf` for exactly one operand (the shape the
//! `format` op needs). Covers the common verbs/flags and Go's `%!verb(...)`
//! error forms.
use perch_interpreter::{go_quote, to_string_value};
use serde_json::Value;

#[derive(Default, Clone)]
struct Spec {
    minus: bool,
    plus: bool,
    sharp: bool,
    space: bool,
    zero: bool,
    width: Option<usize>,
    prec: Option<usize>,
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "<nil>",
        Value::Bool(_) => "bool",
        Value::String(_) => "string",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "int"
            } else {
                "float64"
            }
        }
        Value::Array(_) => "[]interface {}",
        Value::Object(_) => "map[string]interface {}",
    }
}

fn pad(s: String, sp: &Spec, numeric: bool) -> String {
    let Some(w) = sp.width else { return s };
    let n = s.chars().count();
    if n >= w {
        return s;
    }
    let fill = w - n;
    if sp.minus {
        format!("{s}{}", " ".repeat(fill))
    } else if sp.zero && numeric {
        let (sign, rest) = match s.chars().next() {
            Some(c @ ('-' | '+' | ' ')) => (c.to_string(), s[1..].to_string()),
            _ => (String::new(), s),
        };
        format!("{sign}{}{rest}", "0".repeat(fill))
    } else {
        format!("{}{s}", " ".repeat(fill))
    }
}

fn sign_prefix(neg: bool, sp: &Spec) -> &'static str {
    if neg {
        "-"
    } else if sp.plus {
        "+"
    } else if sp.space {
        " "
    } else {
        ""
    }
}

/// Go's shortest `%g` rendering (what `%v` uses for floats): exponent form when
/// the decimal exponent is < -4 or >= 6 (strconv's shortest-precision rule).
fn fmt_g_shortest(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf".into() } else { "-Inf".into() };
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let e = format!("{:e}", f.abs());
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let neg = if f < 0.0 { "-" } else { "" };
    if !(-4..6).contains(&exp) {
        return format!("{neg}{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    format!("{neg}{}", f.abs())
}

fn fmt_e(f: f64, prec: usize, upper: bool) -> String {
    let s = format!("{:.*e}", prec, f.abs());
    let (m, e) = s.split_once('e').unwrap();
    let exp: i32 = e.parse().unwrap();
    let r = format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    if upper {
        r.to_uppercase()
    } else {
        r
    }
}

fn fmt_g_prec(f: f64, prec: usize) -> String {
    let p = prec.max(1);
    if f == 0.0 {
        return "0".into();
    }
    let e = format!("{:.*e}", p - 1, f.abs());
    let exp: i32 = e.split_once('e').unwrap().1.parse().unwrap();
    let trim = |s: String| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };
    if exp < -4 || exp >= p as i32 {
        let (m, _) = e.split_once('e').unwrap();
        format!("{}e{}{:02}", trim(m.to_string()), if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        trim(format!("{:.*}", (p as i32 - 1 - exp).max(0) as usize, f.abs()))
    }
}

fn bad_verb(verb: char, v: &Value) -> String {
    match v {
        Value::Null => format!("%!{verb}(<nil>)"),
        _ => format!("%!{verb}({}={})", type_name(v), to_string_value(v)),
    }
}

fn trunc(s: String, sp: &Spec) -> String {
    match sp.prec {
        Some(p) => s.chars().take(p).collect(),
        None => s,
    }
}

fn fmt_one(verb: char, sp: &Spec, v: &Value) -> String {
    match verb {
        'T' => return pad(type_name(v).to_string(), sp, false),
        'v' => {
            let s = match v {
                Value::Null => "<nil>".to_string(),
                Value::Number(n) if !(n.is_i64() || n.is_u64()) => {
                    let f = n.as_f64().unwrap_or(0.0);
                    let g = match sp.prec {
                        Some(p) => fmt_g_prec(f, p),
                        None => fmt_g_shortest(f),
                    };
                    if sp.plus && f >= 0.0 {
                        format!("+{g}")
                    } else {
                        g
                    }
                }
                Value::Number(n) => {
                    let s = n.to_string();
                    if sp.plus && !s.starts_with('-') {
                        format!("+{s}")
                    } else {
                        s
                    }
                }
                _ => to_string_value(v),
            };
            let s = if matches!(v, Value::String(_)) { trunc(s, sp) } else { s };
            return pad(s, sp, matches!(v, Value::Number(_)));
        }
        _ => {}
    }
    match (verb, v) {
        ('s', Value::String(s)) => pad(trunc(s.clone(), sp), sp, false),
        ('s', Value::Array(_) | Value::Object(_)) => pad(to_string_value(v), sp, false),
        ('q', Value::String(s)) => pad(go_quote(&trunc(s.clone(), sp)), sp, false),
        ('x' | 'X', Value::String(s)) => {
            let h = hex::encode(s.as_bytes());
            pad(if verb == 'X' { h.to_uppercase() } else { h }, sp, false)
        }
        ('t', Value::Bool(b)) => pad(b.to_string(), sp, false),
        ('d' | 'b' | 'o' | 'x' | 'X' | 'c' | 'q' | 'U', Value::Number(n)) if n.is_i64() || n.is_u64() => {
            let i = n.as_i64().map(i128::from).unwrap_or_else(|| n.as_u64().unwrap() as i128);
            let neg = i < 0;
            let mag = i.unsigned_abs();
            let mut body = match verb {
                'd' => mag.to_string(),
                'b' => format!("{mag:b}"),
                'o' => format!("{mag:o}"),
                'x' => format!("{mag:x}"),
                'X' => format!("{mag:X}"),
                'c' => return pad(char::from_u32(i as u32).unwrap_or('\u{fffd}').to_string(), sp, false),
                'q' => return pad(format!("'{}'", char::from_u32(i as u32).unwrap_or('\u{fffd}')), sp, false),
                _ => return pad(format!("U+{mag:04X}"), sp, false),
            };
            if let Some(p) = sp.prec {
                while body.len() < p {
                    body.insert(0, '0');
                }
            }
            let prefix = match (verb, sp.sharp) {
                ('x', true) => "0x",
                ('X', true) => "0X",
                ('o', true) => "0",
                ('b', true) => "0b",
                _ => "",
            };
            pad(format!("{}{prefix}{body}", sign_prefix(neg, sp)), sp, true)
        }
        ('e' | 'E' | 'f' | 'F' | 'g' | 'G', Value::Number(n)) => {
            let f = n.as_f64().unwrap_or(0.0);
            let neg = f < 0.0 || (f == 0.0 && f.is_sign_negative());
            if f.is_nan() || f.is_infinite() {
                let s = if f.is_nan() { "NaN".to_string() } else { format!("{}Inf", if neg { "-" } else { "+" }) };
                return pad(s, &Spec { zero: false, ..sp.clone() }, false);
            }
            let body = match verb {
                'e' | 'E' => fmt_e(f, sp.prec.unwrap_or(6), verb == 'E'),
                'f' | 'F' => format!("{:.*}", sp.prec.unwrap_or(6), f.abs()),
                _ => match sp.prec {
                    Some(p) => fmt_g_prec(f, p),
                    None => fmt_g_shortest(f.abs()),
                },
            };
            let body = if verb == 'G' { body.to_uppercase() } else { body };
            pad(format!("{}{body}", sign_prefix(neg, sp)), sp, true)
        }
        _ => bad_verb(verb, v),
    }
}

/// `fmt.Sprintf(format, operand)` where `operand` is `None` for Go's nil.
pub fn sprintf(format: &str, operand: Option<&Value>) -> String {
    let nil = Value::Null;
    let operand = operand.unwrap_or(&nil);
    let cs: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut used = false;
    let mut i = 0;
    while i < cs.len() {
        if cs[i] != '%' {
            out.push(cs[i]);
            i += 1;
            continue;
        }
        i += 1;
        let mut sp = Spec::default();
        while i < cs.len() {
            match cs[i] {
                '-' => {
                    sp.minus = true;
                    sp.zero = false;
                }
                '+' => sp.plus = true,
                '#' => sp.sharp = true,
                ' ' => sp.space = true,
                '0' => sp.zero = !sp.minus,
                _ => break,
            }
            i += 1;
        }
        let mut w = String::new();
        while i < cs.len() && cs[i].is_ascii_digit() {
            w.push(cs[i]);
            i += 1;
        }
        if !w.is_empty() {
            sp.width = w.parse().ok();
        }
        if i < cs.len() && cs[i] == '.' {
            i += 1;
            let mut p = String::new();
            while i < cs.len() && cs[i].is_ascii_digit() {
                p.push(cs[i]);
                i += 1;
            }
            sp.prec = Some(p.parse().unwrap_or(0));
        }
        if i >= cs.len() {
            out.push_str("%!(NOVERB)");
            break;
        }
        let verb = cs[i];
        i += 1;
        if verb == '%' {
            out.push('%');
            continue;
        }
        if used {
            out.push_str(&format!("%!{verb}(MISSING)"));
            continue;
        }
        used = true;
        out.push_str(&fmt_one(verb, &sp, operand));
    }
    if !used {
        match operand {
            Value::Null => out.push_str("%!(EXTRA <nil>)"),
            v => out.push_str(&format!("%!(EXTRA {}={})", type_name(v), to_string_value(v))),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sprintf_cases() {
        assert_eq!(sprintf("Hello %s", Some(&json!("world"))), "Hello world");
        assert_eq!(sprintf("%5d|%-5d", Some(&json!(42))), "   42|%!d(MISSING)");
        assert_eq!(sprintf("%05d", Some(&json!(-42))), "-0042");
        assert_eq!(sprintf("%d", Some(&json!("x"))), "%!d(string=x)");
        assert_eq!(sprintf("%s", None), "%!s(<nil>)");
        assert_eq!(sprintf("hi", Some(&json!("x"))), "hi%!(EXTRA string=x)");
        assert_eq!(sprintf("%.2f", Some(&json!(2.5))), "2.50");
        assert_eq!(sprintf("%q 100%%", Some(&json!("a"))), "\"a\" 100%");
    }
}

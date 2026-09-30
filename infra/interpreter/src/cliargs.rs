//! A port of the slice of Go's `flag` package that `parse_args` relies on
//! (ContinueOnError, single- or double-dash, `-f=v` / `-f v`, `--` terminator,
//! parsing stops at the first non-flag), with Go's exact error strings.
use std::collections::HashMap;

/// Go's `strconv.Quote`: a double-quoted string with Go escapes.
pub fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
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
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if c.is_control() => {
                let n = c as u32;
                if n < 0x10000 {
                    out.push_str(&format!("\\u{n:04x}"));
                } else {
                    out.push_str(&format!("\\U{n:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FlagKind {
    Str,
    Bool,
    Int,
    Float,
}

pub(crate) struct FlagDef {
    pub name: String,
    pub kind: FlagKind,
}

/// Raw string values of the flags that were provided, plus remaining args.
pub(crate) struct Parsed {
    pub provided: HashMap<String, String>,
    pub rest: Vec<String>,
}

pub(crate) fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}

pub(crate) enum NumErr {
    Syntax,
    Range,
}

/// `strconv.ParseInt(s, 0, 64)`.
pub(crate) fn parse_int_base0(s: &str) -> Result<i64, NumErr> {
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if body.is_empty() {
        return Err(NumErr::Syntax);
    }
    let lower = body.to_ascii_lowercase();
    let (radix, digits, prefixed) = if let Some(r) = lower.strip_prefix("0x") {
        (16, r.to_string(), true)
    } else if let Some(r) = lower.strip_prefix("0b") {
        (2, r.to_string(), true)
    } else if let Some(r) = lower.strip_prefix("0o") {
        (8, r.to_string(), true)
    } else if lower.len() > 1 && lower.starts_with('0') {
        (8, lower[1..].to_string(), true)
    } else {
        (10, lower.clone(), false)
    };
    // Underscores are only legal with a base prefix, between digits.
    let digits = if digits.contains('_') {
        if !prefixed || digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
            return Err(NumErr::Syntax);
        }
        digits.replace('_', "")
    } else {
        digits
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return Err(NumErr::Syntax);
    }
    let mag = match u128::from_str_radix(&digits, radix) {
        Ok(m) => m,
        Err(_) => return Err(NumErr::Range),
    };
    let v = if neg { -(mag as i128) } else { mag as i128 };
    if v < i64::MIN as i128 || v > i64::MAX as i128 {
        return Err(NumErr::Range);
    }
    Ok(v as i64)
}

fn num_err_text(e: &NumErr) -> &'static str {
    match e {
        NumErr::Syntax => "parse error",
        NumErr::Range => "value out of range",
    }
}

pub(crate) fn parse_float(s: &str) -> Result<f64, NumErr> {
    let t = s.replace('_', "");
    if t != s && !(s.starts_with("0x") || s.starts_with("0X")) {
        return Err(NumErr::Syntax);
    }
    match s.parse::<f64>() {
        Ok(f) if f.is_infinite() && !s.to_ascii_lowercase().contains("inf") => Err(NumErr::Range),
        Ok(f) => Ok(f),
        Err(_) => Err(NumErr::Syntax),
    }
}

/// `flag.FlagSet.Parse` for the given definitions. Errors carry Go's text.
pub(crate) fn parse_flags(defs: &[FlagDef], args: &[String]) -> Result<Parsed, String> {
    let mut provided: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let s = &args[i];
        if s.len() < 2 || !s.starts_with('-') {
            break;
        }
        let mut minuses = 1;
        if s.as_bytes()[1] == b'-' {
            minuses = 2;
            if s.len() == 2 {
                // "--" terminates the flags.
                i += 1;
                break;
            }
        }
        let name_full = &s[minuses..];
        if name_full.is_empty() || name_full.starts_with('-') || name_full.starts_with('=') {
            return Err(format!("bad flag syntax: {s}"));
        }
        i += 1;
        let (name, mut value, has_value) = match name_full.find('=') {
            Some(p) => (&name_full[..p], name_full[p + 1..].to_string(), true),
            None => (name_full, String::new(), false),
        };
        let def = match defs.iter().find(|d| d.name == name) {
            Some(d) => d,
            None => {
                if name == "help" || name == "h" {
                    return Err("flag: help requested".to_string());
                }
                return Err(format!("flag provided but not defined: -{name}"));
            }
        };
        if def.kind == FlagKind::Bool {
            if has_value {
                if parse_bool(&value).is_none() {
                    return Err(format!("invalid boolean value {} for -{}: parse error", go_quote(&value), name));
                }
            } else {
                value = "true".to_string();
            }
        } else {
            let mut has = has_value;
            if !has && i < args.len() {
                has = true;
                value = args[i].clone();
                i += 1;
            }
            if !has {
                return Err(format!("flag needs an argument: -{name}"));
            }
            let bad = match def.kind {
                FlagKind::Int => parse_int_base0(&value).err().map(|e| num_err_text(&e)),
                FlagKind::Float => parse_float(&value).err().map(|e| num_err_text(&e)),
                _ => None,
            };
            if let Some(why) = bad {
                return Err(format!("invalid value {} for flag -{}: {}", go_quote(&value), name, why));
            }
        }
        provided.insert(name.to_string(), value);
    }
    Ok(Parsed { provided, rest: args[i..].to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> Vec<FlagDef> {
        vec![
            FlagDef { name: "name".into(), kind: FlagKind::Str },
            FlagDef { name: "n".into(), kind: FlagKind::Int },
            FlagDef { name: "v".into(), kind: FlagKind::Bool },
        ]
    }

    fn sv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flags_and_positionals() {
        let p = parse_flags(&defs(), &sv(&["-name=a", "--n", "0x10", "-v", "pos", "-name=b"])).unwrap();
        assert_eq!(p.provided["name"], "a");
        assert_eq!(p.provided["n"], "0x10");
        assert_eq!(p.provided["v"], "true");
        assert_eq!(p.rest, sv(&["pos", "-name=b"]));
    }

    #[test]
    fn errors_match_go() {
        assert_eq!(parse_flags(&defs(), &sv(&["-x"])).err().unwrap(), "flag provided but not defined: -x");
        assert_eq!(parse_flags(&defs(), &sv(&["-name"])).err().unwrap(), "flag needs an argument: -name");
        assert_eq!(
            parse_flags(&defs(), &sv(&["-n=zz"])).err().unwrap(),
            "invalid value \"zz\" for flag -n: parse error"
        );
        assert_eq!(
            parse_flags(&defs(), &sv(&["-v=maybe"])).err().unwrap(),
            "invalid boolean value \"maybe\" for -v: parse error"
        );
        assert_eq!(parse_flags(&defs(), &sv(&["-h"])).err().unwrap(), "flag: help requested");
    }

    #[test]
    fn double_dash_stops() {
        let p = parse_flags(&defs(), &sv(&["--", "-name=a"])).unwrap();
        assert_eq!(p.rest, sv(&["-name=a"]));
    }

    #[test]
    fn quote() {
        assert_eq!(go_quote("a\"b\n"), "\"a\\\"b\\n\"");
    }
}

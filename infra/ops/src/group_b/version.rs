//! Version ops (version.go): extract + compare. Dependency-free comparator over
//! dotted numeric tuples with an optional `v` prefix and pre-release tail.
use crate::group_b::util::*;
use perch_domain::{ErrorKind, OpError};
use perch_interpreter::{go_quote, handler, Args, Bindings, Error, Handler, Interpreter, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Matches the common shapes in real `--version` output (v1.29.3, 3.14,
/// v20.10.0+meta123, 1.29.3-rc.1); group 1 is the version proper.
fn default_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| go_regex(r"v?(\d+(?:\.\d+)+(?:[-+][\w.+-]+)?)").expect("static pattern"))
}

fn op_err(op: &str, kind: ErrorKind, msg: &str, detail: Option<String>) -> Error {
    let mut e = OpError::new(op, kind, msg);
    if let Some(d) = detail {
        e = e.with_detail(d);
    }
    Box::new(e)
}

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert("version_extract".into(), pure(op_version_extract));
    m.insert("version_eq".into(), cmp_op(|c| c == 0));
    m.insert("version_ne".into(), cmp_op(|c| c != 0));
    m.insert("version_gt".into(), cmp_op(|c| c > 0));
    m.insert("version_ge".into(), cmp_op(|c| c >= 0));
    m.insert("version_lt".into(), cmp_op(|c| c < 0));
    m.insert("version_le".into(), cmp_op(|c| c <= 0));
    // version_compat — same major version.
    m.insert(
        "version_compat".into(),
        pure(|a| {
            let x = version_parts(&arg_string(a, &["a", "_0"]));
            let y = version_parts(&arg_string(a, &["b", "_1"]));
            if x.is_empty() || y.is_empty() {
                return Ok(Value::String("false".into()));
            }
            Ok(Value::String((x[0] == y[0]).to_string()))
        }),
    );
    m.insert(
        "assert_version_ge".into(),
        pure(|a| {
            let got = arg_string(a, &["got", "_0"]);
            let want = arg_string(a, &["want", "_1"]);
            if version_compare(&got, &want) >= 0 {
                return Ok(Value::Null);
            }
            Err(op_err(
                "assert_version_ge",
                ErrorKind::AssertFailed,
                &format!("version {} is below required {}", go_quote(&got), go_quote(&want)),
                Some(format!("got={} want_at_least={}", go_quote(&got), go_quote(&want))),
            ))
        }),
    );
    // `assert_version "X" OP "Y"`: args.op carries the operator.
    m.insert("assert_version".into(), handler(op_assert_version_infix));
}

fn op_assert_version_infix(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let op = match a.map.get("op") {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    // lhs is args.lhs (string form) or args._lhs_var (ident form, looked up at runtime).
    let lhs = match a.map.get("_lhs_var") {
        Some(Value::String(v)) if !v.is_empty() => b.lookup(v).unwrap_or_default(),
        _ => arg_string(a, &["lhs"]),
    };
    let rhs = arg_string(a, &["rhs"]);
    let cmp = version_compare(&lhs, &rhs);
    let (ok, symbol) = match op.as_str() {
        "ge" => (cmp >= 0, ">="),
        "gt" => (cmp > 0, ">"),
        "le" => (cmp <= 0, "<="),
        "lt" => (cmp < 0, "<"),
        "eq" => (cmp == 0, "=="),
        "ne" => (cmp != 0, "!="),
        "compat" => {
            let la = version_parts(&lhs);
            let lb = version_parts(&rhs);
            (!la.is_empty() && !lb.is_empty() && la[0] == lb[0], "~")
        }
        _ => {
            return Err(op_err(
                "assert_version",
                ErrorKind::Unclassified,
                &format!("unknown comparison operator {}", go_quote(&op)),
                None,
            ))
        }
    };
    if ok {
        return Ok(Value::Null);
    }
    Err(op_err(
        "assert_version",
        ErrorKind::AssertFailed,
        &format!("version assertion failed: {} {symbol} {} is not true", go_quote(&lhs), go_quote(&rhs)),
        Some(format!("got={} op={symbol} want={}", go_quote(&lhs), go_quote(&rhs))),
    ))
}

/// Pulls a version out of arbitrary text. With a custom pattern the first
/// capture group wins (whole match when there is none); "" when nothing matches.
fn op_version_extract(a: &Args<'_>) -> Result<Value> {
    let src = arg_string(a, &["src", "_0"]);
    let pat = arg_string(a, &["pattern", "_1"]);
    if pat.is_empty() {
        return Ok(Value::String(
            default_pattern().captures(&src).and_then(|c| c.get(1)).map(|m| m.as_str().to_string()).unwrap_or_default(),
        ));
    }
    let re = go_regex(&pat)
        .map_err(|e| op_err("version_extract", ErrorKind::Unclassified, &format!("regex compile: {e}"), None))?;
    let Some(caps) = re.captures(&src) else { return Ok(Value::String(String::new())) };
    let s = if caps.len() >= 2 {
        caps.get(1).map(|m| m.as_str()).unwrap_or("")
    } else {
        caps.get(0).map(|m| m.as_str()).unwrap_or("")
    };
    Ok(Value::String(s.to_string()))
}

fn cmp_op(pred: fn(i32) -> bool) -> Handler {
    handler(move |_i, _b, a| {
        let x = arg_string(a, &["a", "_0"]);
        let y = arg_string(a, &["b", "_1"]);
        Ok(Value::String(pred(version_compare(&x, &y)).to_string()))
    })
}

fn ord(o: std::cmp::Ordering) -> i32 {
    match o {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// -1 if a<b, 0 if equal, +1 if a>b. Missing elements are 0 ("1.2" == "1.2.0");
/// an un-suffixed version beats a pre-release; unparseable input falls back to
/// plain string comparison.
pub fn version_compare(a: &str, b: &str) -> i32 {
    let (apts, atail) = split_version(a);
    let (bpts, btail) = split_version(b);
    if apts.is_empty() || bpts.is_empty() {
        return ord(a.cmp(b));
    }
    for i in 0..apts.len().max(bpts.len()) {
        let ai = apts.get(i).copied().unwrap_or(0);
        let bi = bpts.get(i).copied().unwrap_or(0);
        if ai < bi {
            return -1;
        }
        if ai > bi {
            return 1;
        }
    }
    match (atail.is_empty(), btail.is_empty()) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => -1,
        (false, false) => ord(atail.cmp(&btail)),
    }
}

/// `splitVersion("v1.29.3-rc.1+meta5")` -> `([1,29,3], "rc.1")`.
pub fn split_version(s: &str) -> (Vec<i64>, String) {
    let s = s.trim();
    let mut s = s.strip_prefix('v').unwrap_or(s).to_string();
    if let Some(i) = s.find('+') {
        s.truncate(i);
    }
    let mut tail = String::new();
    if let Some(i) = s.find('-') {
        tail = s[i + 1..].to_string();
        s.truncate(i);
    }
    if s.is_empty() {
        return (Vec::new(), tail);
    }
    let mut out = Vec::new();
    for p in s.split('.') {
        match p.parse::<i64>() {
            Ok(n) => out.push(n),
            Err(_) => return (Vec::new(), tail),
        }
    }
    (out, tail)
}

/// Just the numeric tuple (version_compat looks at the major component).
fn version_parts(s: &str) -> Vec<i64> {
    let (pts, _) = split_version(s);
    if !pts.is_empty() {
        return pts;
    }
    if let Some(c) = default_pattern().captures(s) {
        if let Some(m) = c.get(1) {
            return split_version(m.as_str()).0;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_cases() {
        let cases: &[(&str, &str, i32)] = &[
            ("1.0.0", "1.0.0", 0),
            ("v1.0.0", "1.0.0", 0),
            ("1.0", "1.0.0", 0),
            ("1.2.3", "1.2.4", -1),
            ("1.2.4", "1.2.3", 1),
            ("1.10.0", "1.9.0", 1),
            ("2.0.0", "1.99.99", 1),
            ("v1.29.3", "v1.28.0", 1),
            ("1.0.0", "1.0.0-rc.1", 1),
            ("1.0.0-rc.1", "1.0.0", -1),
            ("1.0.0-rc.1", "1.0.0-rc.2", -1),
            ("1.0.0+build1", "1.0.0+build2", 0),
            ("3.14", "3.14.0", 0),
            ("3.14.1", "3.14", 1),
        ];
        for (a, b, want) in cases {
            assert_eq!(version_compare(a, b), *want, "versionCompare({a:?}, {b:?})");
        }
    }

    #[test]
    fn split_version_cases() {
        let cases: &[(&str, &[i64], &str)] = &[
            ("1.2.3", &[1, 2, 3], ""),
            ("v1.2.3", &[1, 2, 3], ""),
            ("1.0.0-rc.1", &[1, 0, 0], "rc.1"),
            ("v20.10.0+build123", &[20, 10, 0], ""),
            ("  v3.14 ", &[3, 14], ""),
            ("not a version", &[], ""),
        ];
        for (input, parts, tail) in cases {
            let (p, t) = split_version(input);
            assert_eq!((p.as_slice(), t.as_str()), (*parts, *tail), "splitVersion({input:?})");
        }
    }

    #[test]
    fn extract_default() {
        let c = default_pattern().captures("kubectl v1.29.3 client").unwrap();
        assert_eq!(&c[1], "1.29.3");
    }
}

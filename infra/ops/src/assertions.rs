//! Assertion ops — thin sugar over `if X fail "msg"`. They exist so tests read
//! naturally and produce helpful failure messages that name the values that
//! didn't match.
//!
//! These ops are NOT gated by --no-shell / --no-network / etc. — they touch
//! nothing the runtime can't see. Existence checks are FS reads only; the
//! FS-write restriction doesn't apply.
use crate::common::{arg_string, resolve};
use perch_interpreter::{err, go_quote, handler, Args, Bindings, Handler, Interpreter, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

pub fn register_assertions(m: &mut HashMap<String, Handler>) {
    m.insert("assert_eq".into(), handler(op_assert_eq));
    m.insert("assert_neq".into(), handler(op_assert_neq));
    m.insert("assert_contains".into(), handler(op_assert_contains));
    m.insert("assert_not_contains".into(), handler(op_assert_not_contains));
    m.insert("assert_exists".into(), handler(op_assert_exists));
    m.insert("assert_not_exists".into(), handler(op_assert_not_exists));
    m.insert("assert_match".into(), handler(op_assert_match));
}

fn op_assert_eq(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let actual = arg_string(args, &["actual", "_0"]);
    let expected = arg_string(args, &["expected", "_1"]);
    if actual != expected {
        return Err(err(format!("assert_eq failed: expected {}, got {}", go_quote(&expected), go_quote(&actual))));
    }
    Ok(Value::Null)
}

fn op_assert_neq(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let a = arg_string(args, &["actual", "_0"]);
    let not_expected = arg_string(args, &["not_expected", "_1"]);
    if a == not_expected {
        return Err(err(format!("assert_neq failed: value should not be {}", go_quote(&not_expected))));
    }
    Ok(Value::Null)
}

fn op_assert_contains(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let haystack = arg_string(args, &["haystack", "_0"]);
    let needle = arg_string(args, &["needle", "_1"]);
    if !haystack.contains(&needle) {
        return Err(err(format!(
            "assert_contains failed: {} not found in {}",
            go_quote(&needle),
            go_quote(&truncate_for_error(&haystack))
        )));
    }
    Ok(Value::Null)
}

fn op_assert_not_contains(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let haystack = arg_string(args, &["haystack", "_0"]);
    let needle = arg_string(args, &["needle", "_1"]);
    if haystack.contains(&needle) {
        return Err(err(format!(
            "assert_not_contains failed: {} unexpectedly found in {}",
            go_quote(&needle),
            go_quote(&truncate_for_error(&haystack))
        )));
    }
    Ok(Value::Null)
}

fn op_assert_exists(_i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let raw = arg_string(args, &["path", "_0"]);
    let p = resolve(&raw, b);
    if let Err(e) = std::fs::metadata(&p) {
        if e.kind() == std::io::ErrorKind::NotFound {
            return Err(err(format!("assert_exists failed: {} does not exist", go_quote(&raw))));
        }
        return Err(err(format!(
            "assert_exists failed: {}: {}",
            go_quote(&raw),
            crate::common::path_err("stat", &p, &e)
        )));
    }
    Ok(Value::Null)
}

fn op_assert_not_exists(_i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let raw = arg_string(args, &["path", "_0"]);
    let p = resolve(&raw, b);
    match std::fs::metadata(&p) {
        Ok(_) => Err(err(format!("assert_not_exists failed: {} exists but shouldn't", go_quote(&raw)))),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(err(format!(
            "assert_not_exists failed: {}: {}",
            go_quote(&raw),
            crate::common::path_err("stat", &p, &e)
        ))),
        Err(_) => Ok(Value::Null),
    }
}

fn op_assert_match(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let actual = arg_string(args, &["actual", "_0"]);
    let pattern = arg_string(args, &["pattern", "_1"]);
    let re = Regex::new(&pattern)
        .map_err(|e| err(format!("assert_match: invalid regex {}: {}", go_quote(&pattern), first_line(&e.to_string()))))?;
    if !re.is_match(&actual) {
        return Err(err(format!(
            "assert_match failed: {} did not match /{}/",
            go_quote(&truncate_for_error(&actual)),
            pattern
        )));
    }
    Ok(Value::Null)
}

fn first_line(s: &str) -> String {
    s.lines().last().unwrap_or(s).to_string()
}

/// Shortens long values so error messages stay readable: the prefix is kept,
/// without burying the actual failure under a wall of output.
pub fn truncate_for_error(s: &str) -> String {
    const MAX: usize = 120;
    if s.len() <= MAX {
        return s.to_string();
    }
    let mut end = MAX - 3;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation() {
        assert_eq!(truncate_for_error("abc"), "abc");
        let long = "x".repeat(200);
        let t = truncate_for_error(&long);
        assert_eq!(t.len(), 120);
        assert!(t.ends_with("..."));
    }
}

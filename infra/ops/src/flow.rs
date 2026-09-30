//! Control-flow ops: if / if_call / for_each and the os / arch blocks.
use crate::common::{arg_string, to_float, truthy, truthy_value};
use crate::seams::version_compare;
use perch_interpreter::{handler, to_string_value, Args, Bindings, Handler, Interpreter, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub fn register_flow(m: &mut HashMap<String, Handler>) {
    m.insert("if".into(), handler(op_if));
    m.insert("if_call".into(), handler(op_if_call));
    m.insert("for_each".into(), handler(op_for_each));
    m.insert("os".into(), handler(op_os_block));
    m.insert("arch".into(), handler(op_arch_block));
}

/// Architecture execution-context block. Body runs only when `${arch}` matches
/// the declared target. Targets are Go GOARCH values ("amd64", "arm64", …);
/// exact match only.
fn op_arch_block(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let current = b.lookup("arch").unwrap_or_default();
    if !arch_target_matches(target, &current) {
        return Ok(Value::Null);
    }
    i.run_ops(args.body, b)?;
    Ok(Value::Null)
}

/// Whether the declared target matches the host's `${arch}` (exact match).
pub fn arch_target_matches(target: &str, host: &str) -> bool {
    if target.is_empty() || host.is_empty() {
        return false;
    }
    target == host
}

/// OS execution-context block. `os "linux" ... end` runs its body only when the
/// host's `${os}` matches the declared target.
///
/// Targets: "darwin" | "linux" | "windows" | "freebsd" | "openbsd" | "netbsd"
/// (exact `${os}` match) or "unix" (umbrella matching the same set as
/// `${is_unix}`).
fn op_os_block(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let current = b.lookup("os").unwrap_or_default();
    if !os_target_matches(target, &current) {
        return Ok(Value::Null);
    }
    i.run_ops(args.body, b)?;
    Ok(Value::Null)
}

/// Whether the declared target matches the host's `${os}`. Exposed for the
/// simulator + scanner.
pub fn os_target_matches(target: &str, host: &str) -> bool {
    if target.is_empty() || host.is_empty() {
        return false;
    }
    if target == host {
        return true;
    }
    target == "unix" && matches!(host, "darwin" | "linux" | "freebsd" | "openbsd" | "netbsd")
}

/// Iterates over a newline-separated string value, binding each non-empty line
/// to the loop variable and running the body. Empty input is a clean no-op. The
/// previous value of the loop variable (if any) is restored after the loop so
/// for_each blocks compose cleanly.
fn op_for_each(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let source = arg_string(args, &["value", "_0"]);
    let mut loop_var = arg_string(args, &["var", "_1"]);
    if loop_var.is_empty() {
        loop_var = "item".to_string();
    }
    let prev = b.vars.get(&loop_var).cloned();
    let mut result = Ok(Value::Null);
    for line in source.split('\n') {
        if line.is_empty() {
            continue;
        }
        b.set(&loop_var, line);
        if let Err(e) = i.run_ops(args.body, b) {
            result = Err(e);
            break;
        }
    }
    match prev {
        Some(v) => {
            b.vars.insert(loop_var, v);
        }
        None => {
            b.vars.remove(&loop_var);
        }
    }
    result
}

/// Runs the block's nested body (Go: `runBody`).
pub(crate) fn run_body(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<()> {
    i.run_ops(args.body, b)
}

/// Evaluates `if NAME OP VALUE`, `if NAME`, or `if not NAME`.
/// `args.op` is one of eq/neq/gt/lt/ge/le/truthy/falsy; `args.lhs` is a binding
/// name (auto-bound + globals + args + lets + env); `args.rhs` is the literal
/// to compare against (comparison ops only).
fn op_if(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let op = args.get("op").and_then(|v| v.as_str()).unwrap_or("");
    let lhs_name = args.get("lhs").and_then(|v| v.as_str()).unwrap_or("");
    let lhs_val = b.lookup(lhs_name).unwrap_or_default();
    let rhs = args.get("rhs").unwrap_or(&Value::Null);
    let lhs_v = Value::String(lhs_val.clone());

    let matched = match op {
        "eq" => lhs_val == to_string_value(rhs),
        "neq" => lhs_val != to_string_value(rhs),
        "gt" => compare_values(&lhs_v, rhs) > 0,
        "lt" => compare_values(&lhs_v, rhs) < 0,
        "ge" => compare_values(&lhs_v, rhs) >= 0,
        "le" => compare_values(&lhs_v, rhs) <= 0,
        "truthy" => truthy(&lhs_val),
        "falsy" => !truthy(&lhs_val),
        _ => false,
    };
    if matched {
        run_body(i, b, args)?;
    }
    Ok(Value::Null)
}

/// The ordering used by `if X >= Y` / `if X > Y` / etc. Auto-detects two
/// regimes: both sides parse as versions (semver-aware: "1.10.0" > "1.9.0"), or
/// otherwise numeric (`to_float`).
pub fn compare_values(lhs: &Value, rhs: &Value) -> i32 {
    let ls = to_string_value(lhs);
    let rs = to_string_value(rhs);
    if looks_like_version(&ls) && looks_like_version(&rs) {
        return version_compare(&ls, &rs);
    }
    let lf = to_float(lhs);
    let rf = to_float(rhs);
    if lf < rf {
        -1
    } else if lf > rf {
        1
    } else {
        0
    }
}

/// Whether `s` is shaped like a dotted-number version string ("1.28.0",
/// "v1.29.3", "20.10.0-rc.1"): optional leading 'v', at least one '.', and the
/// segments before any '-' / '+' tail all digits.
pub fn looks_like_version(s: &str) -> bool {
    let s = s.trim();
    let mut s = s.strip_prefix('v').unwrap_or(s);
    if !s.contains('.') {
        return false;
    }
    // Strip pre-release/build tail.
    if let Some(i) = s.find(['-', '+']) {
        s = &s[..i];
    }
    s.split('.').all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
}

/// Evaluates `if FUNC ARG ... end` by invoking the named op with one argument
/// and running the body if the return value is truthy.
fn op_if_call(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let fname = args.get("func").and_then(|v| v.as_str()).unwrap_or("");
    if fname.is_empty() {
        return Ok(Value::Null);
    }
    let Some(h) = i.handlers.get(fname) else {
        return Ok(Value::Null);
    };
    let mut map = Map::new();
    map.insert("_0".to_string(), args.get("_0").cloned().unwrap_or(Value::Null));
    let val = h(i, b, &Args { map, body: &[] })?;
    if truthy_value(&val) {
        run_body(i, b, args)?;
    }
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_shapes() {
        assert!(looks_like_version("1.28.0"));
        assert!(looks_like_version("v1.29.3"));
        assert!(looks_like_version("20.10.0-rc.1"));
        assert!(!looks_like_version("10"));
        assert!(!looks_like_version("1..2"));
        assert!(!looks_like_version("a.b"));
    }

    #[test]
    fn comparisons() {
        let s = |x: &str| Value::String(x.to_string());
        assert_eq!(compare_values(&s("1.10.0"), &s("1.9.0")), 1);
        assert_eq!(compare_values(&s("10"), &s("9")), 1);
        assert_eq!(compare_values(&s("2"), &s("2.0")), 0);
    }

    #[test]
    fn targets() {
        assert!(os_target_matches("unix", "linux"));
        assert!(!os_target_matches("unix", "windows"));
        assert!(!os_target_matches("", "linux"));
        assert!(arch_target_matches("arm64", "arm64"));
        assert!(!arch_target_matches("arm64", ""));
    }
}

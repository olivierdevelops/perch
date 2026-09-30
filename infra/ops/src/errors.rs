//! Error-handling ops: try / rescue / finally + match / case / else.
//!
//! The body of a `try` block is one flat list of ops with sentinel dividers —
//! `_catch` and `_finally`. The handler walks the list, splits on the
//! sentinels, and dispatches:
//!
//!  1. Try-body: every op before _catch (and before _finally if no _catch).
//!  2. Catch-body: every op after _catch and before _finally. Only runs if the
//!     try-body errored; populates ${BIND.kind} etc.
//!  3. Finally-body: every op after _finally. Runs unconditionally, AFTER catch
//!     (or after the try-body if no catch and no error).
//!
//! match works the same way: `_case <value>` and `_else` are sentinels; the
//! handler picks the first matching case (or _else) and runs that arm's body.
use perch_domain::{ErrorKind, Op, OpError};
use perch_interpreter::{handler, to_string_value, Args, Bindings, Error, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;

pub fn register_error_ops(m: &mut HashMap<String, Handler>) {
    m.insert("try".into(), handler(op_try));
    m.insert("match".into(), handler(op_match));

    // Sentinel handlers — these ops are markers inside try/match bodies, not
    // standalone ops. Calling them outside their parent block is an error.
    m.insert("_catch".into(), sentinel_err("catch", "try"));
    m.insert("_finally".into(), sentinel_err("finally", "try"));
    m.insert("_case".into(), sentinel_err("case", "match"));
    m.insert("_else".into(), sentinel_err("else", "match"));
}

fn sentinel_err(name: &'static str, parent: &'static str) -> Handler {
    handler(move |_i, _b, _args| {
        Err(Box::new(OpError::new(
            name,
            ErrorKind::Unclassified,
            &format!("{name} is only valid inside a {parent} block"),
        )))
    })
}

/// Executes the try-body; on error, populates ${BIND.*} bindings and runs the
/// catch-body. The finally-body runs unconditionally last. If the catch-body
/// itself errors (or there is no catch and the try-body errored), the error
/// propagates after finally runs.
fn op_try(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let (try_body, catch_body, finally_body, catch_bind) = split_try_body(args.body);

    // Run try-body; capture the error (if any) without aborting yet.
    let mut try_err: Option<Error> = i.run_ops(&try_body, b).err();

    // Catch arm — only if try errored AND a NON-EMPTY rescue arm exists. (The
    // grammar always emits the `_catch` marker via block_sections, so an absent
    // or empty `rescue` yields an empty catch body; that must NOT swallow the
    // error — `try … end` / `try … finally … end` re-raise.)
    if try_err.is_some() && catch_body.as_ref().is_some_and(|c| !c.is_empty()) {
        let oe = OpError::classify("", &**try_err.as_ref().unwrap());
        populate_err_bindings(b, &catch_bind, &oe);
        // Whatever bindings catch produced/clobbered remain visible to the
        // finally body. Catch-body errors take precedence over the original try
        // error (the user's recovery code "decided" to re-raise; their error wins).
        try_err = i.run_ops(catch_body.as_deref().unwrap_or(&[]), b).err();
    }

    // Finally — always runs. Its errors override everything else (otherwise
    // users couldn't observe a failure in their cleanup code).
    if let Some(fin) = &finally_body {
        i.run_ops(fin, b)?;
    }
    match try_err {
        Some(e) => Err(e),
        None => Ok(Value::Null),
    }
}

type TrySections = (Vec<Op>, Option<Vec<Op>>, Option<Vec<Op>>, String);

/// Partitions a try block's flat body on `_catch` and `_finally` sentinel ops.
/// Returns the three sections + the catch-binding name. `None` for a section
/// means its sentinel never appeared.
fn split_try_body(body: &[Op]) -> TrySections {
    #[derive(PartialEq)]
    enum State {
        Try,
        Catch,
        Finally,
    }
    let mut state = State::Try;
    let mut catch_bind = "err".to_string(); // default if user wrote `catch err`
    let (mut try_body, mut catch_body, mut finally_body): (Vec<Op>, Option<Vec<Op>>, Option<Vec<Op>>) =
        (Vec::new(), None, None);
    for op in body {
        match op.kind.as_str() {
            "_catch" => {
                state = State::Catch;
                if let Some(Value::String(bind)) = op.args.get("bind") {
                    if !bind.is_empty() {
                        catch_bind = bind.clone();
                    }
                }
                catch_body.get_or_insert_with(Vec::new);
                continue;
            }
            "_finally" => {
                state = State::Finally;
                finally_body.get_or_insert_with(Vec::new);
                continue;
            }
            _ => {}
        }
        match state {
            State::Try => try_body.push(op.clone()),
            State::Catch => catch_body.get_or_insert_with(Vec::new).push(op.clone()),
            State::Finally => finally_body.get_or_insert_with(Vec::new).push(op.clone()),
        }
    }
    (try_body, catch_body, finally_body, catch_bind)
}

/// Sets ${BIND.kind}, ${BIND.message}, ${BIND.code}, ${BIND.op}, ${BIND.detail}
/// so the catch body can discriminate.
fn populate_err_bindings(b: &mut Bindings, bind: &str, oe: &OpError) {
    b.set(&format!("{bind}.kind"), oe.kind.as_str());
    b.set(&format!("{bind}.message"), oe.message.as_str());
    b.set(&format!("{bind}.code"), oe.code.as_str());
    b.set(&format!("{bind}.op"), oe.op.as_str());
    b.set(&format!("{bind}.detail"), oe.detail.as_str());
    // Plain ${err} -> message, for the common "just rethrow" case.
    b.set(bind, oe.message.as_str());
}

/// Evaluates args.target and dispatches to the first matching `case` arm (or the
/// `else` arm if none match).
fn op_match(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    // Both `match "${X}"` (string form) and `match X` (ident form) land in
    // args.target — the ident form's `_target_var` is auto-resolved by
    // interpolate_args into `target` before we run.
    let target = to_string_value(args.get("target").unwrap_or(&Value::Null));
    let arms = split_match_body(args.body);

    for arm in &arms {
        if arm.is_else {
            continue;
        }
        if arm.value == target {
            i.run_ops(&arm.body, b)?;
            return Ok(Value::Null);
        }
    }
    // No case matched — find the else arm.
    for arm in &arms {
        if arm.is_else {
            i.run_ops(&arm.body, b)?;
            return Ok(Value::Null);
        }
    }
    // No match, no else — silent no-op. (Strict-exhaustive mode is future work;
    // for now we mirror chained-if behavior.)
    Ok(Value::Null)
}

struct MatchArm {
    is_else: bool,
    value: String,
    body: Vec<Op>,
}

/// Partitions a match block's flat body on `_case` / `_else` sentinel ops. Each
/// arm carries its match value (or is_else) and the body to run.
fn split_match_body(body: &[Op]) -> Vec<MatchArm> {
    let mut arms: Vec<MatchArm> = Vec::new();
    let mut cur: Option<MatchArm> = None;
    for op in body {
        match op.kind.as_str() {
            "_case" => {
                if let Some(c) = cur.take() {
                    arms.push(c);
                }
                let v = to_string_value(op.args.get("value").unwrap_or(&Value::Null));
                cur = Some(MatchArm { is_else: false, value: v, body: Vec::new() });
                continue;
            }
            "_else" => {
                if let Some(c) = cur.take() {
                    arms.push(c);
                }
                cur = Some(MatchArm { is_else: true, value: String::new(), body: Vec::new() });
                continue;
            }
            _ => {}
        }
        match cur.as_mut() {
            // Op appears before any case/else — silently ignored. (perch --check
            // warns on this shape.)
            None => continue,
            Some(c) => c.body.push(op.clone()),
        }
    }
    if let Some(c) = cur {
        arms.push(c);
    }
    arms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(kind: &str) -> Op {
        Op { kind: kind.into(), ..Default::default() }
    }

    #[test]
    fn split_try() {
        let body = vec![op("a"), op("_catch"), op("b"), op("_finally"), op("c")];
        let (t, c, f, bind) = split_try_body(&body);
        assert_eq!((t.len(), c.unwrap().len(), f.unwrap().len()), (1, 1, 1));
        assert_eq!(bind, "err");
        let (_, c, f, _) = split_try_body(&[op("a")]);
        assert!(c.is_none() && f.is_none());
    }

    #[test]
    fn split_match() {
        let mut case = op("_case");
        case.args.insert("value".into(), Value::String("x".into()));
        let arms = split_match_body(&[op("ignored"), case, op("a"), op("_else"), op("b")]);
        assert_eq!(arms.len(), 2);
        assert_eq!(arms[0].value, "x");
        assert!(arms[1].is_else);
    }
}

//! Execution-context block ops. Each wraps a body and modifies *how* the body
//! runs — concurrency (parallel), deadline (timeout), retry policy (retry), env
//! overlay (with_env), cwd override (with_cwd), capability mask (sandbox). They
//! are declarative wrappers, not new control flow.
//!
//! Identity guardrails:
//!  - parallel forbids `let` captures and `cd` inside (the validator enforces
//!    this statically). Each thread gets its own Bindings copy to avoid races.
//!  - timeout's deadline is wall-clock; a long-running op can't be interrupted
//!    mid-call, but the next op after it returns ErrTimeout.
//!  - retry never retries past the outer command's deadline.
//!  - with_env / with_cwd auto-restore on block exit so they compose.
use crate::common::{arg_string, go_parse_duration, path_err, resolve, to_float};
use perch_interpreter::{
    err, go_quote, handler, is_quit, is_timeout, wrap, Args, Bindings, CapMask, Error, Handler, Interpreter, Result,
};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub fn register_contexts(m: &mut HashMap<String, Handler>) {
    m.insert("timeout".into(), handler(op_timeout));
    m.insert("retry".into(), handler(op_retry));
    m.insert("parallel".into(), handler(op_parallel));
    m.insert("with_env".into(), handler(op_with_env));
    m.insert("with_cwd".into(), handler(op_with_cwd));
    m.insert("sandbox".into(), handler(op_sandbox));
}

fn nanos(n: i64) -> Duration {
    Duration::from_nanos(n.max(0) as u64)
}

/// Caps wall-clock for the inner body. Temporarily narrows the interpreter's
/// deadline to min(existing, now+duration), runs the body, restores. Long-running
/// ops can't be interrupted mid-call; the *next* op after the deadline trips
/// returns ErrTimeout.
fn op_timeout(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let dur_str = arg_string(args, &["duration", "_0"]);
    let dur = go_parse_duration(&dur_str)
        .map_err(|e| err(format!("timeout: invalid duration {}: {}", go_quote(&dur_str), e)))?;
    let new_deadline = Instant::now() + nanos(dur);
    let prev = i.deadline();
    if prev.is_none_or(|p| new_deadline < p) {
        i.set_deadline(Some(new_deadline));
    }
    let res = i.run_ops(args.body, b);
    i.set_deadline(prev);
    res.map(|_| Value::Null)
}

/// Runs the body up to `attempts` times. On error it sleeps according to the
/// chosen backoff and retries. Backoff strategies: "fixed" (constant), "linear"
/// (n*base), "exponential" (2^(n-1)*base). Default base = 1s. max_delay caps
/// each sleep. Returns the LAST error if every attempt failed.
fn op_retry(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let mut attempts = to_float(args.get("attempts").unwrap_or(&Value::Null)) as i64;
    if attempts <= 0 {
        attempts = 3;
    }
    let mut backoff = arg_string(args, &["backoff"]);
    if backoff.is_empty() {
        backoff = "exponential".to_string();
    }
    let mut base: i64 = 1_000_000_000;
    let s = arg_string(args, &["base"]);
    if !s.is_empty() {
        if let Ok(d) = go_parse_duration(&s) {
            base = d;
        }
    }
    let mut max_delay: i64 = 5 * 60 * 1_000_000_000;
    let s = arg_string(args, &["max_delay"]);
    if !s.is_empty() {
        if let Ok(d) = go_parse_duration(&s) {
            max_delay = d;
        }
    }

    let mut last_err: Option<Error> = None;
    for n in 1..=attempts {
        match i.run_ops(args.body, b) {
            Ok(()) => return Ok(Value::Null),
            Err(e) => {
                if is_timeout(&e) || is_quit(&e) {
                    return Err(e);
                }
                last_err = Some(e);
            }
        }
        if n == attempts {
            break;
        }
        // Compute the next sleep duration.
        let mut d: i64 = match backoff.as_str() {
            "fixed" => base,
            "linear" => n.saturating_mul(base),
            _ => (2f64.powi((n - 1) as i32) as i64).saturating_mul(base), // exponential
        };
        if d > max_delay {
            d = max_delay;
        }
        // Don't retry if doing so would breach the outer deadline.
        if let Some(dl) = i.deadline() {
            if d > 0 && Instant::now() + nanos(d) > dl {
                break;
            }
        }
        if d > 0 {
            std::thread::sleep(nanos(d));
        }
    }
    let last = last_err.unwrap_or_else(|| err("<nil>"));
    Err(wrap(format!("retry: {attempts} attempts failed; last error"), last))
}

/// Runs each direct child op of the body concurrently. Waits for ALL to
/// complete; returns the first error encountered (subsequent errors are joined
/// into the message).
///
/// Each child runs against a *copy* of Bindings (Vars/Env/Cwd), which avoids
/// races on Vars without locking every op. The validator statically rejects
/// `let X = …` and `cd …` inside parallel. Nesting is permitted.
fn op_parallel(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let body = args.body;
    if body.is_empty() {
        return Ok(Value::Null);
    }
    if body.len() == 1 {
        // Degenerate case — one child, just run it inline.
        i.run_op(&body[0], b)?;
        return Ok(Value::Null);
    }
    let bref: &Bindings = b;
    let results: Vec<Option<Error>> = std::thread::scope(|s| {
        let handles: Vec<_> = body
            .iter()
            .map(|op| {
                s.spawn(move || {
                    let mut child = copy_bindings(bref);
                    i.run_op(op, &mut child).err()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Some(err("parallel: branch panicked"))))
            .collect()
    });
    let mut errs: Vec<Error> = results.into_iter().flatten().collect();
    if let Some(pos) = errs.iter().position(is_quit) {
        return Err(errs.swap_remove(pos));
    }
    match errs.len() {
        0 => Ok(Value::Null),
        1 => Err(errs.remove(0)),
        n => {
            let msgs: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
            Err(err(format!("parallel: {} branches failed: {}", n, msgs.join(" | "))))
        }
    }
}

/// A deep-enough copy for parallel branches: Vars and Env are copied (so
/// branches can't see each other's writes); the cap mask and env allowlist are
/// shared (immutable from the inside — pushes return new masks).
fn copy_bindings(b: &Bindings) -> Bindings {
    Bindings {
        cwd: b.cwd.clone(),
        env: b.env.clone(),
        vars: b.vars.clone(),
        env_allowlist: b.env_allowlist.clone(),
        cap_mask: b.cap_mask.clone(),
        in_hook: false,
    }
}

/// Overlays per-block environment variables onto the bindings for the duration
/// of the body, then restores. Variables arrive as args["env"], a comma-joined
/// "KEY=val" string.
fn op_with_env(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let raw = arg_string(args, &["env", "_0"]);
    if raw.is_empty() {
        i.run_ops(args.body, b)?;
        return Ok(Value::Null);
    }
    // Save prior values for restore. Order matters only for duplicate keys, where
    // the later save sees the earlier overlay (matching the Go map overwrite).
    let mut prior: HashMap<String, Option<String>> = HashMap::new();
    for pair in raw.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let eq = match pair.find('=') {
            Some(eq) if eq > 0 => eq,
            _ => return Err(err(format!("with_env: expected KEY=value, got {}", go_quote(pair)))),
        };
        let k = pair[..eq].trim().to_string();
        let v = pair[eq + 1..].trim().trim_matches('"').to_string();
        prior.insert(k.clone(), b.env.get(&k).cloned());
        b.env.insert(k, v);
    }
    let res = i.run_ops(args.body, b);
    for (k, p) in prior {
        match p {
            Some(v) => {
                b.env.insert(k, v);
            }
            None => {
                b.env.remove(&k);
            }
        }
    }
    res.map(|_| Value::Null)
}

/// Temporarily switches cwd for the body, then restores. Unlike the standalone
/// `cd` op (which persists for the rest of the command), `with_cwd`
/// auto-restores even when the body errors.
fn op_with_cwd(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let raw = arg_string(args, &["path", "_0"]);
    if raw.is_empty() {
        return Err(err("with_cwd: missing path"));
    }
    let path = resolve(&raw, b);
    match std::fs::metadata(&path) {
        Err(e) => {
            return Err(err(format!("with_cwd {}: {}", go_quote(&path), path_err("stat", &path, &e))));
        }
        Ok(m) if !m.is_dir() => {
            return Err(err(format!("with_cwd {}: not a directory", go_quote(&path))));
        }
        Ok(_) => {}
    }
    let prev = std::mem::replace(&mut b.cwd, path);
    let res = i.run_ops(args.body, b);
    b.cwd = prev;
    res.map(|_| Value::Null)
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// Narrows the active capability mask for the body. Can only remove
/// capabilities, never add them — the intersection with any outer mask (and the
/// process-level CLI flags) is what's enforced. Args (all optional): `flags`
/// (comma-joined no_shell,no_subprocess,no_network,no_write), `allow_bin`,
/// `allow_host`, `env`, `read_only`, `max_runtime`.
fn op_sandbox(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let mut next = CapMask::default();
    let s = arg_string(args, &["flags"]);
    if !s.is_empty() {
        for f in s.split(',') {
            match f.trim() {
                "no_shell" => next.no_shell = true,
                "no_subprocess" => next.no_subprocess = true,
                "no_network" => next.no_network = true,
                "no_write" => next.no_write = true,
                "" => {}
                other => return Err(err(format!("sandbox: unknown flag {}", go_quote(other)))),
            }
        }
    }
    let s = arg_string(args, &["allow_bin"]);
    if !s.is_empty() {
        next.allowed_bins = Some(split_csv(&s).into_iter().map(|n| (n, true)).collect());
    }
    let s = arg_string(args, &["allow_host"]);
    if !s.is_empty() {
        next.allowed_hosts = split_csv(&s);
    }
    let s = arg_string(args, &["env"]);
    if !s.is_empty() {
        next.env_allow = Some(split_csv(&s).into_iter().map(|n| (n, true)).collect());
    }
    let s = arg_string(args, &["read_only"]);
    if !s.is_empty() {
        next.read_only_roots = split_csv(&s);
    }
    let prev = b.cap_mask.clone();
    b.cap_mask = Some(CapMask::push(prev.as_ref(), next));

    // Optional sandbox-scoped timeout.
    let mut prev_dl: Option<Option<Instant>> = None;
    let s = arg_string(args, &["max_runtime"]);
    if !s.is_empty() {
        if let Ok(d) = go_parse_duration(&s) {
            let new_deadline = Instant::now() + nanos(d);
            let p = i.deadline();
            if p.is_none_or(|p| new_deadline < p) {
                i.set_deadline(Some(new_deadline));
            }
            prev_dl = Some(p);
        }
    }
    let res = i.run_ops(args.body, b);
    if let Some(p) = prev_dl {
        i.set_deadline(p);
    }
    b.cap_mask = prev;
    res.map(|_| Value::Null)
}

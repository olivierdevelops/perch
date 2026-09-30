//! The stateful simulator. Threads [`SimState`] through the op walk so later
//! ops see the effects of earlier ones (file written → exists downstream; `let
//! X = shell_output Y` → `${X}` resolves to oracle value; `cd PATH` →
//! cwd-relative paths resolve correctly).
//!
//! Kept in its own file because it overlays the existing static `simulate_op`
//! dispatch. The static path remains the fallback when no Fixture / no oracles
//! are provided — same behavior as before this refactor.
use crate::gofmt::quote;
use crate::{
    apply_block_env, bool_str, classify_wasm_run, eval_cond, extract_host, first_path_arg, first_shell_token,
    host_allowed, os_matches, path_under, rollup_children, sorted_bins, string_arg, tally_tree, OpResult, Outcome,
    OracleSet, Scenario, SimEnv, SimResult, SimState,
};
use perch_domain::{Op, Program};
use serde_json::{Map, Value};

/// Walks the named command's ops with full state threading and oracle
/// consultation. The state mutates as ops "run":
///
/// ```text
/// write_file X           → state.files[X] = true
/// rm X                   → state.files[X] = false
/// cd PATH                → state.cwd = PATH
/// let X = shell_output Y → state.vars[X] = oracles.shell_output[Y]
/// ```
///
/// Returns the per-op results AND the final state (useful for chained
/// scenarios — not yet wired through the CLI, but available to callers).
pub fn simulate_with_state(
    p: &Program,
    cmd_name: &str,
    env: &SimEnv,
    oracles: &OracleSet,
) -> (SimResult, Option<SimState>) {
    let mut res = SimResult { command: cmd_name.to_string(), ..Default::default() };
    let Some(cmd) = p.commands.get(cmd_name) else {
        res.ops = vec![OpResult {
            outcome: Outcome::WillFail,
            reasons: vec![format!("command {} not found", quote(cmd_name))],
            ..Default::default()
        }];
        res.will_fail = 1;
        return (res, None);
    };
    let mut state = SimState::new(env, oracles);
    for op in &cmd.ops {
        let r = simulate_op_stateful(op, &mut state, env, 0, p, cmd_name);
        tally_tree(&mut res, &r);
        res.ops.push(r);
    }
    (res, Some(state))
}

/// The state-threading counterpart to `simulate_op`. It dispatches the same
/// kinds but:
///   - resolves ${name} against the live state
///   - consults oracles before declaring an op MIGHT_FAIL
///   - mutates state after side-effectful ops
fn simulate_op_stateful(op: &Op, state: &mut SimState, env: &SimEnv, depth: usize, p: &Program, cmd_name: &str) -> OpResult {
    let mut r = OpResult { op: op.clone(), depth, outcome: Outcome::WillRun, ..Default::default() };

    // Resolve interpolation against the live state.
    let resolved = substitute_all(&op.args, state);

    match op.kind.as_str() {
        "shell" | "shell_output" | "shell_detached" | "shell_in" | "try_shell" => {
            classify_shell_stateful(&mut r, op, &resolved, env, state)
        }

        "pkg_install" | "pkg_uninstall" | "kill_by_name" | "process_running" | "bin_version" | "os_version" => {
            if env.no_subprocess {
                r.outcome = Outcome::WillFail;
                r.reasons.push("subprocess capability denied by sim --no-subprocess".into());
            }
        }

        "http_get" | "http_post" | "http_put" | "http_delete" | "http_status" | "download" => {
            classify_http_stateful(&mut r, &resolved, env, state)
        }

        "write_file" | "append_file" | "ensure_line_in_file" | "replace_in_file" | "touch" => {
            classify_write_stateful(&mut r, &resolved, env, state, true)
        }

        "cp" | "mv" | "mkdir" | "copy_dir" | "ensure_dir" | "symlink" => {
            classify_write_stateful(&mut r, &resolved, env, state, true)
        }

        "rm" => classify_write_stateful(&mut r, &resolved, env, state, false),

        "cd" => {
            let path = string_arg(&resolved, &["path", "_0"]);
            if !path.is_empty() && !path.contains("${") {
                state.cwd = path.to_string();
                r.reasons.push(format!("cwd → {}", quote(path)));
            } else {
                r.outcome = Outcome::MightFail;
                r.reasons.push("cwd target not statically determinable".into());
            }
        }

        "read_file" | "exists" | "is_dir" | "is_file" | "file_size" | "file_mtime" | "sha256_file" | "md5_file" => {
            classify_read_stateful(&mut r, op, &resolved, env, state)
        }

        "has_bin" => classify_has_bin_stateful(&mut r, &resolved, env, state),

        "if" => {
            classify_if_stateful(&mut r, op, env, state, depth, p, cmd_name);
            return r;
        }

        "if_call" => {
            classify_if_call_stateful(&mut r, op, env, state, depth, p, cmd_name);
            return r;
        }

        "os" => {
            r.is_block_entry = true;
            let target = op.args.get("target").and_then(Value::as_str).unwrap_or("");
            if env.os.is_empty() || os_matches(target, &env.os) {
                r.reasons.push(format!("os {} matches — body runs", quote(target)));
                r.children = simulate_body_stateful(&op.body, env, state, depth + 1, p, cmd_name);
            } else {
                r.reasons.push(format!("os {} != sim-os {} — body skipped", quote(target), quote(&env.os)));
            }
            rollup_children(&mut r);
            return r;
        }

        "arch" => {
            r.is_block_entry = true;
            let target = op.args.get("target").and_then(Value::as_str).unwrap_or("");
            if env.arch.is_empty() || target == env.arch {
                r.reasons.push(format!("arch {} matches — body runs", quote(target)));
                r.children = simulate_body_stateful(&op.body, env, state, depth + 1, p, cmd_name);
            } else {
                r.reasons.push(format!("arch {} != sim-arch {} — body skipped", quote(target), quote(&env.arch)));
            }
            rollup_children(&mut r);
            return r;
        }

        "parallel" | "retry" | "timeout" | "with_env" | "with_cwd" | "sandbox" | "cache" | "for_each" => {
            r.is_block_entry = true;
            // Block ops snapshot the state so concurrent branches don't mutate
            // each other (and so the not-taken-if branch leaves no trace).
            let body_env = apply_block_env(op, env);
            let mut body_state = state.snapshot();
            // for_each / with_cwd should also propagate their effect into the
            // parent state — for now keep it isolated to the body for safety;
            // future enhancement can merge.
            r.children = simulate_body_stateful(&op.body, &body_env, &mut body_state, depth + 1, p, cmd_name);
            rollup_children(&mut r);
            return r;
        }

        "run" => {
            classify_run_stateful(&mut r, op, env, state, depth, p);
            return r;
        }

        "wasm_run" => {
            classify_wasm_run(&mut r, op, depth); // static; no state effects
            return r;
        }

        "_template_call" => {
            r.outcome = Outcome::WillFail;
            r.reasons.push("unresolved template call (check imports + spelling)".into());
        }

        "fail" => {
            r.outcome = Outcome::WillFail;
            let mut msg = string_arg(&resolved, &["msg", "_0"]);
            if msg.is_empty() {
                msg = "(no message)";
            }
            r.reasons.push(format!("explicit fail: {msg}"));
        }

        _ => {}
    }

    // `let X = ...` capture — bind oracle output (if any) into state so
    // downstream interpolation works.
    if !op.capture_into.is_empty() {
        capture_value(&mut r, op, &resolved, state);
    }

    r
}

fn simulate_body_stateful(ops: &[Op], env: &SimEnv, state: &mut SimState, depth: usize, p: &Program, cmd_name: &str) -> Vec<OpResult> {
    ops.iter().map(|op| simulate_op_stateful(op, state, env, depth, p, cmd_name)).collect()
}

fn substitute_all(args: &Map<String, Value>, state: &SimState) -> Map<String, Value> {
    args.iter()
        .map(|(k, v)| match v {
            Value::String(s) => (k.clone(), Value::String(state.substitute(s).0)),
            other => (k.clone(), other.clone()),
        })
        .collect()
}

// ── Stateful classifiers ──────────────────────────────────────────

fn classify_shell_stateful(r: &mut OpResult, op: &Op, args: &Map<String, Value>, env: &SimEnv, state: &SimState) {
    if env.no_shell {
        r.outcome = Outcome::WillFail;
        r.reasons.push("shell capability denied by sim --no-shell".into());
        return;
    }
    let cmd = string_arg(args, &["cmd", "_0"]);
    let first = first_shell_token(cmd);
    if first.contains("${") {
        r.outcome = Outcome::MightFail;
        r.scenarios.push(Scenario {
            description: format!("argv[0] still unresolved after interpolation: {}", quote(first)),
            outcome: Outcome::MightFail,
            reason: "depends on a value not in state.Vars (likely a `let` from an oracled call that was missing)".into(),
        });
        return;
    }
    if first.is_empty() {
        r.outcome = Outcome::MightFail;
        r.scenarios.push(Scenario {
            description: "argv[0] is empty after interpolation".into(),
            outcome: Outcome::MightFail,
            reason: String::new(),
        });
        return;
    }
    if let Some(bins) = &env.bins {
        if !bins.get(first).copied().unwrap_or(false) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!(
                "shell binary {} not in --sim-have-bin allowlist (have: {})",
                quote(first),
                sorted_bins(bins)
            ));
            return;
        }
    }

    // Consult shell_output oracle if the op's result will be captured.
    if op.kind == "shell_output" && !op.capture_into.is_empty() {
        if let Some(v) = state.oracles.shell_output.get(cmd) {
            r.reasons.push(format!("oracle: shell_output({}) = {} → ${{{}}}", quote(cmd), quote(v), op.capture_into));
        } else {
            r.reasons.push(format!(
                "shell_output: no oracle for {} — ${{{}}} downstream will be MIGHT_FAIL",
                quote(cmd),
                op.capture_into
            ));
        }
    }
}

fn classify_http_stateful(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv, state: &SimState) {
    if env.no_network {
        r.outcome = Outcome::WillFail;
        r.reasons.push("network capability denied by sim --no-network".into());
        return;
    }
    let url = string_arg(args, &["url", "_0"]);
    if url.is_empty() || url.contains("${") {
        r.outcome = Outcome::MightFail;
        r.reasons.push("URL not fully resolvable from state".into());
        return;
    }
    let host = extract_host(url);
    if let Some(net) = &env.network {
        if !host_allowed(host, net) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!("HTTP host {} not in --sim-allow-host allowlist", quote(host)));
            return;
        }
    }
    // Consult HTTP oracle.
    if let Some(resp) = state.oracles.http.get(url) {
        let status = if resp.status == 0 { 200 } else { resp.status };
        if (200..300).contains(&status) {
            r.reasons.push(format!("oracle: {} → {} {}", url, status, truncate(&resp.body, 60)));
        } else if (300..400).contains(&status) {
            r.outcome = Outcome::MightFail;
            r.reasons.push(format!("oracle: {} → {} redirect to {}", url, status, quote(&resp.redirect)));
            if let (Some(net), false) = (&env.network, resp.redirect.is_empty()) {
                let redirect_host = extract_host(&resp.redirect);
                if !host_allowed(redirect_host, net) {
                    r.outcome = Outcome::WillFail;
                    r.reasons.push(format!(
                        "redirect destination {} is NOT in --sim-allow-host allowlist",
                        quote(redirect_host)
                    ));
                }
            }
        } else if status >= 400 {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!("oracle: {} → {} {}", url, status, truncate(&resp.body, 60)));
        }
    } else if env.network.is_some() {
        // No oracle, allowlist active — same uncertainty note as the static path.
        r.scenarios.push(Scenario {
            description: format!("no oracle for {}; server could return any status / redirect anywhere", quote(url)),
            outcome: Outcome::MightFail,
            reason: "supply an oracles.http entry to pin the simulated response".into(),
        });
        if r.outcome == Outcome::WillRun {
            r.outcome = Outcome::MightFail;
        }
    }
}

fn classify_write_stateful(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv, state: &mut SimState, exists: bool) {
    if env.no_write {
        r.outcome = Outcome::WillFail;
        r.reasons.push("write capability denied by sim --no-write".into());
        return;
    }
    let path = first_path_arg(args);
    if path.is_empty() || path.contains("${") {
        r.outcome = Outcome::MightFail;
        r.reasons.push("write target not fully resolvable from state".into());
        return;
    }
    if let Some(roots) = &env.fs_write {
        if !path_under(path, roots) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!("write path {} outside --sim-fs-write roots ({})", quote(path), roots.join(", ")));
            return;
        }
    }
    // State effect: mark the file as existing (or not, for rm).
    state.mark_file(path, exists);
    if exists {
        r.reasons.push(format!("state: {} now exists", quote(path)));
    } else {
        r.reasons.push(format!("state: {} removed", quote(path)));
    }
}

fn classify_read_stateful(r: &mut OpResult, op: &Op, args: &Map<String, Value>, env: &SimEnv, state: &SimState) {
    let path = first_path_arg(args);
    if path.is_empty() || path.contains("${") {
        r.outcome = Outcome::MightFail;
        r.reasons.push("read path not fully resolvable from state".into());
        return;
    }
    if let Some(roots) = &env.fs_read {
        if !path_under(path, roots) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!("read path {} outside --sim-fs-read roots ({})", quote(path), roots.join(", ")));
            return;
        }
    }
    // State + oracle: does the file actually exist for the simulator?
    if op.kind == "exists" {
        // `let X = exists "PATH"` — return true/false from state.
        let (ex, known) = state.file_exists(path);
        if known {
            r.reasons.push(format!("state: exists({}) = {}", quote(path), ex));
        } else {
            r.scenarios.push(Scenario {
                description: format!("exists({}): no oracle, no prior write — outcome uncertain", quote(path)),
                outcome: Outcome::MightFail,
                reason: String::new(),
            });
            if r.outcome == Outcome::WillRun {
                r.outcome = Outcome::MightFail;
            }
        }
    }
}

fn classify_has_bin_stateful(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv, state: &SimState) {
    let bin = string_arg(args, &["_0", "name"]);
    if bin.is_empty() {
        r.outcome = Outcome::MightFail;
        r.reasons.push("bin name not resolvable".into());
        return;
    }
    // Oracle takes precedence over env.bins capability list.
    if let Some(v) = state.oracles.has_bin.get(bin) {
        r.reasons.push(format!("oracle: has_bin({}) = {}", quote(bin), v));
        return;
    }
    if let Some(bins) = &env.bins {
        r.reasons.push(format!(
            "has_bin({}) = {} (from --sim-have-bin)",
            quote(bin),
            bins.get(bin).copied().unwrap_or(false)
        ));
    }
}

fn classify_if_stateful(r: &mut OpResult, op: &Op, env: &SimEnv, state: &mut SimState, depth: usize, p: &Program, cmd_name: &str) {
    r.is_block_entry = true;
    let cond = string_arg(&op.args, &["op"]);
    let lhs = string_arg(&op.args, &["lhs"]);
    let rhs = string_arg(&op.args, &["rhs"]);

    // Not bound in vars; check env interpolation.
    let value = state.vars.get(lhs).or_else(|| state.env.get(lhs)).cloned();
    if let Some(value) = value.filter(|_| !state.unknown.contains_key(lhs)) {
        if eval_cond(cond, &value, rhs) {
            r.reasons.push(format!(
                "condition {} {} {} TRUE (state {}={}) — body runs",
                lhs,
                cond,
                quote(rhs),
                lhs,
                quote(&value)
            ));
            r.children = simulate_body_stateful(&op.body, env, state, depth + 1, p, cmd_name);
        } else {
            r.reasons.push(format!(
                "condition {} {} {} FALSE (state {}={}) — body skipped",
                lhs,
                cond,
                quote(rhs),
                lhs,
                quote(&value)
            ));
        }
        rollup_children(r);
        return;
    }
    // Can't resolve — body presented as MIGHT-RUN.
    let mut branch_state = state.snapshot();
    r.children = simulate_body_stateful(&op.body, env, &mut branch_state, depth + 1, p, cmd_name);
    r.reasons.push(format!("condition ({lhs} {cond}) depends on a runtime value — body presented as MIGHT-RUN"));
    rollup_children(r);
    if r.outcome == Outcome::WillRun {
        r.outcome = Outcome::MightFail;
    }
}

fn classify_if_call_stateful(r: &mut OpResult, op: &Op, env: &SimEnv, state: &mut SimState, depth: usize, p: &Program, cmd_name: &str) {
    r.is_block_entry = true;
    let func = string_arg(&op.args, &["func"]);
    let mut arg = string_arg(&op.args, &["_0"]).to_string();
    let (resolved_arg, all_ok) = state.substitute(&arg);
    if all_ok {
        arg = resolved_arg;
    }
    let mut resolved_now = false;
    let mut take = false;
    let mut reason = String::new();
    match func {
        "exists" => {
            let (ex, known) = state.file_exists(&arg);
            if known {
                take = ex;
                resolved_now = true;
                reason = format!("state: exists({}) = {}", quote(&arg), ex);
            } else if let Some(roots) = &env.fs_read {
                take = path_under(&arg, roots);
                resolved_now = true;
                reason = format!("exists({}) → {} under --sim-fs-read (no specific oracle)", quote(&arg), take);
            }
        }
        "has_bin" => {
            if let Some(v) = state.oracles.has_bin.get(&arg) {
                take = *v;
                resolved_now = true;
                reason = format!("oracle: has_bin({}) = {}", quote(&arg), v);
            } else if let Some(bins) = &env.bins {
                take = bins.get(&arg).copied().unwrap_or(false);
                resolved_now = true;
                // Go's message interpolates `v` — the (zero-valued, since the
                // oracle lookup missed) variable from the failed `if` — so this
                // always prints "false" regardless of `take`. Preserved.
                reason = format!("has_bin({}) = {} (--sim-have-bin)", quote(&arg), false);
            }
        }
        _ => {}
    }
    if resolved_now {
        r.reasons.push(reason);
        if take {
            r.children = simulate_body_stateful(&op.body, env, state, depth + 1, p, cmd_name);
        }
        rollup_children(r);
        return;
    }
    let mut branch_state = state.snapshot();
    r.children = simulate_body_stateful(&op.body, env, &mut branch_state, depth + 1, p, cmd_name);
    r.reasons.push(format!("predicate {}({}) not resolvable — body MIGHT run", func, quote(&arg)));
    rollup_children(r);
    if r.outcome == Outcome::WillRun {
        r.outcome = Outcome::MightFail;
    }
}

fn classify_run_stateful(r: &mut OpResult, op: &Op, env: &SimEnv, state: &mut SimState, depth: usize, p: &Program) {
    let target = string_arg(&op.args, &["target"]);
    let Some(cmd) = p.commands.get(target) else {
        r.outcome = Outcome::WillFail;
        r.reasons.push(format!("run: target command {} not found", quote(target)));
        return;
    };
    r.is_block_entry = true;
    r.reasons.push(format!("dispatches to command {}", quote(target)));
    r.children = simulate_body_stateful(&cmd.ops, env, state, depth + 1, p, target);
    rollup_children(r);
}

/// Binds the op's simulated output into state.vars, flagging unknown if no
/// oracle was found for an op whose result is otherwise unknowable statically.
fn capture_value(r: &mut OpResult, op: &Op, args: &Map<String, Value>, state: &mut SimState) {
    let into = op.capture_into.as_str();
    let symbolic = format!("${{{into}}}");
    match op.kind.as_str() {
        "shell_output" | "try_shell" => {
            let cmd = string_arg(args, &["cmd", "_0"]);
            match state.oracles.shell_output.get(cmd).cloned() {
                Some(v) => state.set_var(into, &v, false),
                None => state.set_var(into, &symbolic, true),
            }
        }
        "http_get" | "http_post" => {
            let url = string_arg(args, &["url", "_0"]);
            match state.oracles.http.get(url).map(|r| r.body.clone()) {
                Some(body) => state.set_var(into, &body, false),
                None => state.set_var(into, &symbolic, true),
            }
        }
        "exists" => {
            let path = first_path_arg(args);
            let (ex, known) = state.file_exists(path);
            if known {
                state.set_var(into, bool_str(ex), false);
            } else {
                state.set_var(into, &symbolic, true);
            }
        }
        "has_bin" => {
            let bin = string_arg(args, &["_0", "name"]);
            if let Some(v) = state.oracles.has_bin.get(bin).copied() {
                state.set_var(into, bool_str(v), false);
            }
        }
        // Unknown capture — value is symbolic.
        _ => state.set_var(into, &symbolic, true),
    }
    if state.unknown.contains_key(into) {
        r.reasons.push(format!("captured ${{{}}} from {} — no oracle, value is symbolic downstream", into, op.kind));
    }
}

/// Byte-truncates like Go (`s[:max-3] + "..."`); a split multi-byte sequence
/// becomes U+FFFD here where Go would emit the raw bytes.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut b = s.as_bytes()[..max - 3].to_vec();
    b.extend_from_slice(b"...");
    String::from_utf8_lossy(&b).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::HTTPResponse;
    use crate::tests::{block, op, prog};
    use serde_json::json;

    fn cap(mut o: Op, into: &str) -> Op {
        o.capture_into = into.into();
        o
    }

    fn run(p: &Program, env: &SimEnv, oracles: &OracleSet) -> (SimResult, SimState) {
        let (r, s) = simulate_with_state(p, "go", env, oracles);
        (r, s.unwrap())
    }

    #[test]
    fn writes_thread_into_exists_and_capture() {
        let p = prog(vec![(
            "go",
            vec![
                op("write_file", json!({"path": "/tmp/x"})),
                cap(op("shell_output", json!({"cmd": "git rev-parse HEAD"})), "sha"),
                op("print", json!({"msg": "at ${sha}"})),
                cap(op("shell_output", json!({"cmd": "uname"})), "u"),
                op("shell", json!({"cmd": "${u} -a"})),
                block("if_call", json!({"func": "exists", "_0": "/tmp/x"}), vec![op("print", json!({"msg": "yes"}))]),
                op("rm", json!({"path": "/tmp/x"})),
                block("if_call", json!({"func": "exists", "_0": "/tmp/x"}), vec![op("fail", json!({"msg": "no"}))]),
            ],
        )]);
        let mut oracles = OracleSet::default();
        oracles.shell_output.insert("git rev-parse HEAD".into(), "1f1db7b".into());
        let env = SimEnv { fs_write: Some(vec!["/tmp".into()]), ..Default::default() };
        let (res, st) = run(&p, &env, &oracles);
        let r = &res.ops;
        assert_eq!(r[0].reasons, vec!["state: \"/tmp/x\" now exists"]);
        assert_eq!(r[1].reasons, vec!["oracle: shell_output(\"git rev-parse HEAD\") = \"1f1db7b\" → ${sha}"]);
        assert_eq!(st.vars["sha"], "1f1db7b");
        assert_eq!(r[3].reasons.last().unwrap(), "captured ${u} from shell_output — no oracle, value is symbolic downstream");
        assert_eq!(r[4].outcome, Outcome::MightFail);
        assert_eq!(r[5].reasons, vec!["state: exists(\"/tmp/x\") = true"]);
        assert_eq!(r[5].children.len(), 1);
        assert_eq!(r[7].reasons, vec!["state: exists(\"/tmp/x\") = false"]);
        assert!(r[7].children.is_empty());
        assert_eq!(res.will_fail, 0);
    }

    #[test]
    fn http_oracles() {
        let p = prog(vec![(
            "go",
            vec![
                op("http_get", json!({"url": "https://a.com/ok"})),
                op("http_get", json!({"url": "https://a.com/moved"})),
                op("http_get", json!({"url": "https://a.com/down"})),
                op("http_get", json!({"url": "https://a.com/none"})),
            ],
        )]);
        let mut o = OracleSet::default();
        o.http.insert("https://a.com/ok".into(), HTTPResponse { body: "OK".into(), ..Default::default() });
        o.http.insert("https://a.com/moved".into(), HTTPResponse { status: 302, redirect: "https://evil.com/x".into(), ..Default::default() });
        o.http.insert("https://a.com/down".into(), HTTPResponse { status: 500, body: "x".repeat(70), ..Default::default() });
        let env = SimEnv { network: Some(vec!["a.com".into()]), ..Default::default() };
        let (res, _) = run(&p, &env, &o);
        assert_eq!(res.ops[0].reasons, vec!["oracle: https://a.com/ok → 200 OK"]);
        assert_eq!(res.ops[1].outcome, Outcome::WillFail);
        assert_eq!(res.ops[1].reasons[0], "oracle: https://a.com/moved → 302 redirect to \"https://evil.com/x\"");
        assert_eq!(res.ops[1].reasons[1], "redirect destination \"evil.com\" is NOT in --sim-allow-host allowlist");
        assert_eq!(res.ops[2].reasons[0], format!("oracle: https://a.com/down → 500 {}...", "x".repeat(57)));
        assert_eq!(res.ops[3].outcome, Outcome::MightFail);
        assert_eq!((res.will_run, res.will_fail, res.uncertain), (1, 2, 1));
    }

    #[test]
    fn has_bin_message_quirk_and_cd() {
        let p = prog(vec![(
            "go",
            vec![
                op("cd", json!({"path": "/srv"})),
                op("touch", json!({"path": "f"})),
                block("if_call", json!({"func": "has_bin", "_0": "git"}), vec![]),
                op("has_bin", json!({"_0": "kubectl"})),
            ],
        )]);
        let mut o = OracleSet::default();
        o.has_bin.insert("kubectl".into(), false);
        let env = SimEnv { bins: Some([("git".to_string(), true)].into()), ..Default::default() };
        let (res, st) = run(&p, &env, &o);
        assert_eq!(res.ops[0].reasons, vec!["cwd → \"/srv\""]);
        assert_eq!(st.file_exists("/srv/f"), (true, true));
        assert_eq!(res.ops[2].reasons, vec!["has_bin(\"git\") = false (--sim-have-bin)"]);
        assert_eq!(res.ops[3].reasons, vec!["oracle: has_bin(\"kubectl\") = false"]);
    }

    #[test]
    fn if_on_state_and_unknown() {
        let p = prog(vec![(
            "go",
            vec![
                cap(op("shell_output", json!({"cmd": "x"})), "v"),
                block("if", json!({"lhs": "v", "op": "eq", "rhs": "1"}), vec![op("print", json!({"msg": "a"}))]),
                block("if", json!({"lhs": "os", "op": "eq", "rhs": "linux"}), vec![op("print", json!({"msg": "b"}))]),
            ],
        )]);
        let env = SimEnv { os: "linux".into(), ..Default::default() };
        let (res, _) = run(&p, &env, &OracleSet::default());
        assert_eq!(res.ops[1].outcome, Outcome::MightFail);
        assert_eq!(res.ops[2].reasons, vec!["condition os eq \"linux\" TRUE (state os=\"linux\") — body runs"]);
    }

    #[test]
    fn missing_command() {
        let p = prog(vec![]);
        let (r, s) = simulate_with_state(&p, "zz", &SimEnv::default(), &OracleSet::default());
        assert!(s.is_none());
        assert_eq!(r.ops[0].reasons, vec!["command \"zz\" not found"]);
    }
}

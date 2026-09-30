//! Analyzes a perch program against a hypothetical runtime environment and
//! produces a per-op outcome report.
//!
//! The missing third tool in perch's pre-flight suite:
//!
//! ```text
//! --check     : syntactic validation (no env, no execution)
//! --scan      : static capability analysis (no env, no execution)
//! --dry-run   : real env, walks the plan, skips execution
//! simulate    : HYPOTHETICAL env, walks the plan, classifies each op
//!               as WILL_RUN / WILL_FAIL(why) / MIGHT_FAIL(scenarios)
//! ```
//!
//! Use it to answer "what would this perch program do on a host with these env
//! vars / these allowed binaries / this network allowlist?" without executing
//! anything.
use perch_domain::{Op, Program};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::OnceLock;

mod fixture;
mod gofmt;
mod state;
mod stateful;

pub use fixture::{load_fixture, merge_oracles, Fixture, HTTPResponse, OracleSet, Scenario_};
pub use state::SimState;
pub use stateful::simulate_with_state;

use gofmt::{quote, v_strings};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Parses a .perch file into a Program (same shape the loader uses; injected so
/// the use case can be tested without filesystem).
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;

/// The production implementation.
pub struct Impl {
    pub load: LoadFn,
}

impl Impl {
    /// Simulates `command_name` against `env`, writing a human report to `w`.
    /// Returns an error if the simulation reports any WILL_FAIL outcome — so
    /// this is CI-droppable like --check / perch test.
    pub fn execute(
        &self,
        config_path: &str,
        command_name: &str,
        env: SimEnv,
        fixture_path: &str,
        w: &mut dyn Write,
    ) -> Result<(), Error> {
        let p = (self.load)(config_path).map_err(|e| -> Error { format!("loading {config_path}: {e}").into() })?;

        // If a fixture file is supplied, iterate its scenarios — each scenario
        // is one independent state-threading walk against effective oracles.
        if !fixture_path.is_empty() {
            let fix = load_fixture(fixture_path)
                .map_err(|e| -> Error { format!("loading fixture {fixture_path}: {e}").into() })?;
            let merged = merge_fixture_into_env(&fix, &env);
            let scenarios: Vec<Scenario_> = if fix.scenarios.is_empty() {
                vec![Scenario_ { name: "default".into(), ..Default::default() }]
            } else {
                fix.scenarios.clone()
            };
            let mut failures = 0;
            for (idx, sc) in scenarios.iter().enumerate() {
                if idx > 0 {
                    writeln!(w)?;
                }
                let oracles = merge_oracles(&fix.oracles, &sc.overrides);
                let mut sc_env = merged.clone();
                if !sc.env.is_empty() {
                    sc_env = with_env_overlay(sc_env, &sc.env);
                }
                writeln!(w, "═══ Scenario: {} ═══", sc.name)?;
                let names = if command_name.is_empty() { command_names(&p) } else { vec![command_name.to_string()] };
                for (j, name) in names.iter().enumerate() {
                    if j > 0 {
                        writeln!(w)?;
                    }
                    let (res, _) = simulate_with_state(&p, name, &sc_env, &oracles);
                    render_result(w, &res, &p, name)?;
                    failures += res.will_fail;
                }
            }
            if failures > 0 {
                return Err(format!("{failures} op(s) would fail across simulated scenarios").into());
            }
            return Ok(());
        }

        if command_name.is_empty() {
            // Simulate every command.
            let mut failures = 0;
            for (idx, name) in command_names(&p).iter().enumerate() {
                if idx > 0 {
                    writeln!(w)?;
                }
                let res = simulate_command_at(&p, name, &env, 0);
                render_result(w, &res, &p, name)?;
                failures += res.will_fail;
            }
            if failures > 0 {
                return Err(format!("{failures} op(s) would fail under the simulated environment").into());
            }
            return Ok(());
        }
        let res = simulate_command_at(&p, command_name, &env, 0);
        render_result(w, &res, &p, command_name)?;
        if res.will_fail > 0 {
            return Err(format!("{} op(s) would fail under the simulated environment", res.will_fail).into());
        }
        Ok(())
    }
}

/// Layers fixture capabilities on top of CLI-flag env. CLI flags win when both
/// are set (so a user can `--sim-no-network` a fixture that has Network
/// entries). Fields the CLI didn't touch fall through from the fixture.
fn merge_fixture_into_env(f: &Fixture, cli: &SimEnv) -> SimEnv {
    let fx = f.to_sim_env();
    let mut out = cli.clone();
    if out.os.is_empty() {
        out.os = fx.os;
    }
    if out.arch.is_empty() {
        out.arch = fx.arch;
    }
    if out.env.is_none() {
        out.env = fx.env;
    }
    if !out.env_restrict {
        out.env_restrict = fx.env_restrict;
    }
    if out.fs_read.is_none() {
        out.fs_read = fx.fs_read;
    }
    if out.fs_write.is_none() {
        out.fs_write = fx.fs_write;
    }
    if out.bins.is_none() {
        out.bins = fx.bins;
    }
    if out.network.is_none() {
        out.network = fx.network;
    }
    out.no_shell = out.no_shell || fx.no_shell;
    out.no_subprocess = out.no_subprocess || fx.no_subprocess;
    out.no_network = out.no_network || fx.no_network;
    out.no_write = out.no_write || fx.no_write;
    out
}

fn with_env_overlay(mut base: SimEnv, overlay: &BTreeMap<String, String>) -> SimEnv {
    let mut merged = base.env.clone().unwrap_or_default();
    merged.extend(overlay.iter().map(|(k, v)| (k.clone(), v.clone())));
    base.env = Some(merged);
    base
}

/// Describes the hypothetical host the program will run on. Fields default to
/// "anything allowed" — an empty SimEnv simulates a fully-permissive host.
/// Restrict by populating each field. (`None` mirrors Go's nil map/slice, which
/// is distinct from an empty one.)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SimEnv {
    /// What runtime.GOOS returns.
    pub os: String,
    /// What runtime.GOARCH returns.
    pub arch: String,

    /// The simulated host environment. Names not in this map are reported as
    /// "unset" — the perch ${NAME} lookup would fail. `None` means "every env
    /// var is set to its real host value" (no restriction).
    pub env: Option<BTreeMap<String, String>>,
    /// When true, `env` is exhaustive — anything not in it is invisible.
    /// Mirrors `perch --env A,B,C`.
    pub env_restrict: bool,

    /// Absolute paths or path roots the simulated process can read. `None` =
    /// read-anywhere (no restriction).
    pub fs_read: Option<Vec<String>>,
    /// Paths the simulated process can write under. `None` = write-anywhere.
    pub fs_write: Option<Vec<String>>,

    /// The set of binaries the simulated host has on PATH. Used for `shell "X
    /// args"` argv[0] checks and for `has_bin "X"` predicate evaluation. `None`
    /// = every bin available.
    pub bins: Option<BTreeMap<String, bool>>,

    /// The set of allowed host names (exact or wildcard). `None` = network is
    /// open. `Some(empty)` = network blocked entirely.
    pub network: Option<Vec<String>>,

    // Capability flags (mirror perch's --no-* CLI). When true, the corresponding
    // op class will-fail regardless of bins / network / fs_write contents.
    pub no_shell: bool,
    pub no_subprocess: bool,
    pub no_network: bool,
    pub no_write: bool,
}

impl SimEnv {
    /// Reports whether the SimEnv applies any restriction. Used to add a banner
    /// explaining "no restrictions configured — every op passes by default" to
    /// the report.
    pub fn is_zero(&self) -> bool {
        *self == SimEnv::default()
    }
}

/// The simulator's verdict for a single op. (Go names the middle variant
/// `WillFailOut` to avoid clashing with the `WillFail` counter.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Outcome {
    /// Every check the simulator can perform passes.
    #[default]
    WillRun,
    /// At least one check definitively fails.
    WillFail,
    /// Depends on runtime data the simulator can't know.
    MightFail,
}

/// The simulator's analysis of one op.
#[derive(Debug, Clone, Default)]
pub struct OpResult {
    pub op: Op,
    /// e.g. `command setup → if os == "darwin" → shell` (unused by the walk).
    pub path: String,
    pub outcome: Outcome,
    /// Why-fail / why-uncertain.
    pub reasons: Vec<String>,
    /// For MightFail: possible cases.
    pub scenarios: Vec<Scenario>,
    /// True for the block-op itself (children render under).
    pub is_block_entry: bool,
    /// For tree indent.
    pub depth: usize,
    /// For block ops.
    pub children: Vec<OpResult>,
}

/// One possible runtime path the op could take. Used for http_get with
/// redirects, shell with computed argv, etc.
#[derive(Debug, Clone, Default)]
pub struct Scenario {
    pub description: String,
    pub outcome: Outcome,
    pub reason: String,
}

/// The aggregate outcome of simulating one command.
#[derive(Debug, Clone, Default)]
pub struct SimResult {
    pub command: String,
    pub ops: Vec<OpResult>,
    pub will_run: usize,
    pub will_fail: usize,
    pub uncertain: usize,
}

/// The exported entry-point for callers (the HTTP UI in particular). Equivalent
/// to the private walk with depth=0.
pub fn simulate_command(p: &Program, name: &str, env: &SimEnv) -> SimResult {
    simulate_command_at(p, name, env, 0)
}

fn simulate_command_at(p: &Program, name: &str, env: &SimEnv, depth: usize) -> SimResult {
    let mut res = SimResult { command: name.to_string(), ..Default::default() };
    let Some(cmd) = p.commands.get(name) else {
        res.ops = vec![OpResult {
            outcome: Outcome::WillFail,
            reasons: vec![format!("command {} not found in program", quote(name))],
            ..Default::default()
        }];
        res.will_fail = 1;
        return res;
    };
    // Platform pre-checks.
    if !env.os.is_empty() && !cmd.modifiers.require_os.is_empty() {
        let ok = cmd.modifiers.require_os.contains(&env.os);
        if !ok {
            res.ops = vec![OpResult {
                outcome: Outcome::WillFail,
                reasons: vec![format!(
                    "require_os: command needs {}; sim env is OS={}",
                    v_strings(&cmd.modifiers.require_os),
                    quote(&env.os)
                )],
                ..Default::default()
            }];
            res.will_fail = 1;
            return res;
        }
    }
    for op in &cmd.ops {
        let r = simulate_op(op, env, depth, p, name);
        tally_tree(&mut res, &r);
        res.ops.push(r);
    }
    res
}

pub(crate) fn tally_tree(res: &mut SimResult, r: &OpResult) {
    match r.outcome {
        Outcome::WillRun => res.will_run += 1,
        Outcome::WillFail => res.will_fail += 1,
        Outcome::MightFail => res.uncertain += 1,
    }
    for c in &r.children {
        tally_tree(res, c);
    }
}

/// Classifies one op against the SimEnv. Block ops recurse.
fn simulate_op(op: &Op, env: &SimEnv, depth: usize, p: &Program, cmd_name: &str) -> OpResult {
    let mut r = OpResult { op: op.clone(), depth, outcome: Outcome::WillRun, ..Default::default() };

    // Args that depend on values we can't know stay as ${name} — the
    // classifiers then mark the op MightFail.
    let args = &op.args;

    match op.kind.as_str() {
        "shell" | "shell_output" | "shell_detached" | "shell_in" | "try_shell" => classify_shell(&mut r, args, env),

        "pkg_install" | "pkg_uninstall" | "kill_by_name" | "process_running" | "bin_version" | "os_version" => {
            if env.no_subprocess {
                r.outcome = Outcome::WillFail;
                r.reasons.push("subprocess capability denied by sim --no-subprocess".into());
            }
        }

        "http_get" | "http_post" | "http_put" | "http_delete" | "http_status" | "download" => {
            classify_http(&mut r, args, env)
        }

        "write_file" | "append_file" | "ensure_line_in_file" | "replace_in_file" | "cp" | "mv" | "rm" | "mkdir"
        | "chmod" | "touch" | "copy_dir" | "ensure_dir" | "make_executable" | "symlink" | "tar_extract"
        | "zip_extract" | "gzip" | "ungzip" => classify_write(&mut r, args, env),

        "read_file" | "exists" | "is_dir" | "is_file" | "list_dir" | "walk_dir" | "file_size" | "file_mtime"
        | "sha256_file" | "md5_file" => classify_read(&mut r, args, env),

        "has_bin" => classify_has_bin(&mut r, args, env),

        "if" => {
            classify_if(&mut r, op, env, depth, p, cmd_name);
            return r;
        }

        "if_call" => {
            // Predicate calls (`if exists "X"`). For now, evaluate the
            // predicate if we recognize it; otherwise mark uncertain.
            classify_if_call(&mut r, op, env, depth, p, cmd_name);
            return r;
        }

        "parallel" | "retry" | "timeout" | "with_env" | "with_cwd" | "sandbox" | "cache" | "for_each" => {
            r.is_block_entry = true;
            r.children = simulate_body(&op.body, &apply_block_env(op, env), depth + 1, p, cmd_name);
            rollup_children(&mut r);
            return r;
        }

        "try" => {
            classify_try(&mut r, op, env, depth, p, cmd_name);
            return r;
        }

        "os" => {
            // OS execution context block. Prune by --sim-os.
            r.is_block_entry = true;
            let target = op.args.get("target").and_then(Value::as_str).unwrap_or("");
            if env.os.is_empty() || os_matches(target, &env.os) {
                r.reasons.push(format!("os {} matches sim-os — body will run", quote(target)));
                r.children = simulate_body(&op.body, env, depth + 1, p, cmd_name);
            } else {
                r.reasons.push(format!(
                    "os {} does NOT match sim-os {} — body skipped",
                    quote(target),
                    quote(&env.os)
                ));
            }
            rollup_children(&mut r);
            return r;
        }

        "arch" => {
            // Architecture execution context block. Prune by --sim-arch.
            r.is_block_entry = true;
            let target = op.args.get("target").and_then(Value::as_str).unwrap_or("");
            if env.arch.is_empty() || target == env.arch {
                r.reasons.push(format!("arch {} matches sim-arch — body will run", quote(target)));
                r.children = simulate_body(&op.body, env, depth + 1, p, cmd_name);
            } else {
                r.reasons.push(format!(
                    "arch {} does NOT match sim-arch {} — body skipped",
                    quote(target),
                    quote(&env.arch)
                ));
            }
            rollup_children(&mut r);
            return r;
        }

        "run" => {
            classify_run(&mut r, op, env, depth, p);
            return r;
        }

        "wasm_run" => {
            classify_wasm_run(&mut r, op, depth);
            return r;
        }

        "_template_call" => {
            r.outcome = Outcome::WillFail;
            r.reasons.push("unresolved template call (check imports + spelling)".into());
        }

        "fail" => {
            r.outcome = Outcome::WillFail;
            let mut msg = string_arg(args, &["msg", "_0"]);
            if msg.is_empty() {
                msg = "(no message)";
            }
            r.reasons.push(format!("explicit fail: {msg}"));
        }

        "exec" => classify_env_prefix(&mut r, args, env, p),

        // Explicit exit. Treat as terminator — runs but stops the flow. We
        // still mark WILL_RUN since the op itself succeeds.
        _ => {}
    }

    // Env-var interpolation check applies to nearly every op.
    let (env_fail, env_scenarios) = check_env_interpolation(&op.args, env);
    if !env_fail.is_empty() {
        // If we already failed for a stronger reason, keep that.
        if r.outcome == Outcome::WillRun {
            r.outcome = Outcome::WillFail;
        }
        r.reasons.push(env_fail);
    }
    for s in env_scenarios {
        r.scenarios.push(s);
        if r.outcome == Outcome::WillRun {
            r.outcome = Outcome::MightFail;
        }
    }

    r
}

/// `try … rescue … finally … end` (also what a command-level `do … finally`
/// lowers to). Models R02: the body and rescue arm are simulated as written; the
/// `finally` section ALWAYS runs, so its ops are classified in their own right
/// and never masked by a body failure. A body failure with no non-empty
/// `rescue` arm re-raises after `finally` (the try fails); with a rescue arm it
/// is caught (the try may still run clean).
fn classify_try(r: &mut OpResult, op: &Op, env: &SimEnv, depth: usize, p: &Program, cmd_name: &str) {
    r.is_block_entry = true;
    // Split on the `_catch` / `_finally` dividers (same partition as opTry).
    let (mut body, mut rescue, mut fin): (Vec<&Op>, Vec<&Op>, Vec<&Op>) = (Vec::new(), Vec::new(), Vec::new());
    let mut section = 0;
    for o in &op.body {
        match o.kind.as_str() {
            "_catch" => section = 1,
            "_finally" => section = 2,
            _ => match section {
                0 => body.push(o),
                1 => rescue.push(o),
                _ => fin.push(o),
            },
        }
    }
    let sim = |ops: &[&Op]| -> Vec<OpResult> { ops.iter().map(|o| simulate_op(o, env, depth + 1, p, cmd_name)).collect() };
    let (body_res, rescue_res, fin_res) = (sim(&body), sim(&rescue), sim(&fin));
    let worst = |rs: &[OpResult]| {
        if rs.iter().any(|c| c.outcome == Outcome::WillFail) {
            Outcome::WillFail
        } else if rs.iter().any(|c| c.outcome == Outcome::MightFail) {
            Outcome::MightFail
        } else {
            Outcome::WillRun
        }
    };
    let (b, rc, f) = (worst(&body_res), worst(&rescue_res), worst(&fin_res));
    // Pending-error outcome after the body and (if present) the rescue arm.
    let after_body = match (b, rescue.is_empty()) {
        (Outcome::WillRun, _) => Outcome::WillRun,
        (Outcome::MightFail, true) => Outcome::MightFail,
        (Outcome::WillFail, true) => Outcome::WillFail,
        (_, false) => rc,
    };
    if b != Outcome::WillRun && !fin.is_empty() {
        r.reasons.push("finally runs even though the body can fail; the body error is re-raised first (a failing finally is appended, not substituted)".into());
    }
    // A failing finally only decides the outcome when the body was clean.
    r.outcome = match (after_body, f) {
        (Outcome::WillFail, _) => Outcome::WillFail,
        (Outcome::MightFail, _) => Outcome::MightFail,
        (Outcome::WillRun, f) => f,
    };
    r.children = body_res.into_iter().chain(rescue_res).chain(fin_res).collect();
}

/// R05: a `NAME=VALUE` prefix on an exec op. Notes the overlay and flags host
/// env refs the file did not declare (the runtime refuses those with
/// `env_not_declared` unless `--env` allows them).
fn classify_env_prefix(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv, p: &Program) {
    let Some(Value::Object(m)) = args.get("env_prefix") else { return };
    let names: Vec<&str> = m.keys().map(String::as_str).collect();
    r.reasons.push(format!("env prefix applies to this process only: {}", names.join(", ")));
    for (k, v) in m {
        let Value::String(s) = v else { continue };
        for m in env_ref_re().captures_iter(s) {
            let name = &m[1];
            let is_global = p.globals.bindings.iter().any(|g| g.name == name);
            let declared = !p.requirements.declared || p.requirements.envs.iter().any(|e| e.name == name);
            let allowed = env.env.as_ref().is_some_and(|e| e.contains_key(name));
            if !is_global && !declared && !allowed {
                r.scenarios.push(Scenario {
                    description: format!("env prefix {k}=${{{name}}} reads host env {}", quote(name)),
                    outcome: Outcome::MightFail,
                    reason: "not declared in `requires env` — refused with env_not_declared unless a binding or --env provides it".into(),
                });
                if r.outcome == Outcome::WillRun {
                    r.outcome = Outcome::MightFail;
                }
            }
        }
    }
}

fn simulate_body(ops: &[Op], env: &SimEnv, depth: usize, p: &Program, cmd_name: &str) -> Vec<OpResult> {
    ops.iter().map(|op| simulate_op(op, env, depth, p, cmd_name)).collect()
}

/// Returns a SimEnv with the block-op's modifications applied. `sandbox
/// no_shell` narrows the env; `with_env "K=v"` adds to the env map; etc.
pub(crate) fn apply_block_env(op: &Op, env: &SimEnv) -> SimEnv {
    let mut out = env.clone();
    match op.kind.as_str() {
        "sandbox" => {
            let flags = string_arg(&op.args, &["flags", "_0"]);
            for f in flags.split(',') {
                match f.trim() {
                    "no_shell" => out.no_shell = true,
                    "no_subprocess" => out.no_subprocess = true,
                    "no_network" => out.no_network = true,
                    "no_write" => out.no_write = true,
                    _ => {}
                }
            }
        }
        "with_env" => {
            // Add the declared env vars to the SimEnv. We don't know their
            // runtime values; treat them as "set to something".
            let envs = string_arg(&op.args, &["env", "_0"]);
            // Clone above: don't mutate the caller's map.
            let map = out.env.get_or_insert_with(BTreeMap::new);
            for pair in envs.split(',') {
                let pair = pair.trim();
                if let Some(eq) = pair.find('=') {
                    if eq > 0 {
                        map.insert(pair[..eq].to_string(), pair[eq + 1..].trim_matches('"').to_string());
                    }
                }
            }
        }
        _ => {}
    }
    out
}

pub(crate) fn rollup_children(r: &mut OpResult) {
    r.outcome = Outcome::WillRun;
    for c in &r.children {
        if c.outcome == Outcome::WillFail {
            r.outcome = Outcome::WillFail;
            return;
        }
        if c.outcome == Outcome::MightFail && r.outcome == Outcome::WillRun {
            r.outcome = Outcome::MightFail;
        }
    }
}

// ── Per-op classifiers ─────────────────────────────────────────────

fn classify_shell(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv) {
    if env.no_shell {
        r.outcome = Outcome::WillFail;
        r.reasons.push("shell capability denied by sim --no-shell".into());
        return;
    }
    let cmd = string_arg(args, &["cmd", "_0"]);
    let first = first_shell_token(cmd);
    if first.is_empty() {
        r.outcome = Outcome::MightFail;
        r.scenarios.push(Scenario {
            description: "argv[0] is computed at runtime".into(),
            outcome: Outcome::MightFail,
            reason: "can't verify bin against allowlist without knowing the value".into(),
        });
        return;
    }
    if first.contains("${") {
        r.outcome = Outcome::MightFail;
        r.scenarios.push(Scenario {
            description: format!("argv[0] = {} (contains unresolved interpolation)", quote(first)),
            outcome: Outcome::MightFail,
            reason: "value depends on runtime bindings".into(),
        });
        return;
    }
    if let Some(bins) = &env.bins {
        if !bins.get(first).copied().unwrap_or(false) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!(
                "shell binary {} not in sim --allow-bin allowlist (have: {})",
                quote(first),
                sorted_bins(bins)
            ));
            return;
        }
    }
    // Shell metacharacters in the command are a yellow flag — at runtime they
    // could expand to anything.
    if cmd.contains(['|', '`', '$', ';', '&']) && !cmd.contains("${") {
        // Shell substitution likely; flag as uncertain.
        r.scenarios.push(Scenario {
            description: "command contains shell metachars (pipes / substitution / chained)".into(),
            outcome: Outcome::MightFail,
            reason: "subsequent processes are not bin-allowlist-checked".into(),
        });
        if r.outcome == Outcome::WillRun {
            r.outcome = Outcome::MightFail;
        }
    }
}

fn classify_http(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv) {
    if env.no_network {
        r.outcome = Outcome::WillFail;
        r.reasons.push("network capability denied by sim --no-network".into());
        return;
    }
    let url = string_arg(args, &["url", "_0"]);
    if url.is_empty() {
        r.outcome = Outcome::MightFail;
        r.reasons.push("URL not statically determinable".into());
        return;
    }
    let host = extract_host(url);
    if host.is_empty() || host.contains("${") {
        r.outcome = Outcome::MightFail;
        r.scenarios.push(Scenario {
            description: format!("URL host = {} (interpolated)", quote(host)),
            outcome: Outcome::MightFail,
            reason: "can't verify against host allowlist without runtime values".into(),
        });
        return;
    }
    if let Some(net) = &env.network {
        if !host_allowed(host, net) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!(
                "HTTP host {} not in sim --allow-host allowlist (have: {})",
                quote(host),
                net.join(", ")
            ));
            return;
        }
    }
    // Redirect scenario: even if `host` is allowed, the server can redirect
    // anywhere. Worth flagging when an allowlist is in play.
    if env.network.as_ref().is_some_and(|n| !n.is_empty()) {
        r.scenarios.push(Scenario {
            description: format!("server at {} could redirect to any host", quote(host)),
            outcome: Outcome::MightFail,
            reason: "perch re-checks every redirect against the allowlist; this op succeeds if redirects stay within the allowlist or there are no redirects".into(),
        });
        if r.outcome == Outcome::WillRun {
            r.outcome = Outcome::MightFail;
        }
    }
}

fn classify_write(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv) {
    if env.no_write {
        r.outcome = Outcome::WillFail;
        r.reasons.push("write capability denied by sim --no-write".into());
        return;
    }
    let path = first_path_arg(args);
    if path.is_empty() || path.contains("${") {
        r.outcome = Outcome::MightFail;
        r.reasons.push("target path not statically determinable".into());
        return;
    }
    if let Some(roots) = &env.fs_write {
        if !path_under(path, roots) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!(
                "write path {} is outside sim --allow-write roots (allowed: {})",
                quote(path),
                roots.join(", ")
            ));
        }
    }
}

fn classify_read(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv) {
    let path = first_path_arg(args);
    if path.is_empty() || path.contains("${") {
        r.outcome = Outcome::MightFail;
        r.reasons.push("read path not statically determinable".into());
        return;
    }
    if let Some(roots) = &env.fs_read {
        if !path_under(path, roots) {
            r.outcome = Outcome::WillFail;
            r.reasons.push(format!(
                "read path {} is outside sim --allow-read roots (allowed: {})",
                quote(path),
                roots.join(", ")
            ));
        }
    }
}

fn classify_has_bin(r: &mut OpResult, args: &Map<String, Value>, env: &SimEnv) {
    let bin = string_arg(args, &["_0", "name"]);
    if bin.is_empty() || bin.contains("${") {
        r.outcome = Outcome::MightFail;
        r.reasons.push("bin name not statically determinable".into());
        return;
    }
    if let Some(bins) = &env.bins {
        if bins.get(bin).copied().unwrap_or(false) {
            r.reasons.push(format!("has_bin({}) = true (in sim --have-bin)", quote(bin)));
        } else {
            r.reasons.push(format!("has_bin({}) = false (not in sim --have-bin)", quote(bin)));
        }
    }
}

/// The if-condition operators the simulator can decide once the lhs is known.
pub(crate) fn eval_cond(cond: &str, value: &str, rhs: &str) -> bool {
    match cond {
        "eq" => value == rhs,
        "neq" => value != rhs,
        "truthy" => !value.is_empty() && value != "false" && value != "0",
        "falsy" => value.is_empty() || value == "false" || value == "0",
        _ => false,
    }
}

fn classify_if(r: &mut OpResult, op: &Op, env: &SimEnv, depth: usize, p: &Program, cmd_name: &str) {
    r.is_block_entry = true;
    let cond = string_arg(&op.args, &["op"]);
    let lhs = string_arg(&op.args, &["lhs"]);
    let rhs = string_arg(&op.args, &["rhs"]);

    // Try to evaluate the condition against the SimEnv.
    if let Some(value) = resolve_auto_bound(lhs, env) {
        // We can decide this branch statically.
        if eval_cond(cond, &value, rhs) {
            r.reasons.push(format!(
                "condition {} {} {} evaluates TRUE (sim {}={}) — body runs",
                lhs,
                cond,
                quote(rhs),
                lhs,
                quote(&value)
            ));
            r.children = simulate_body(&op.body, env, depth + 1, p, cmd_name);
        } else {
            r.reasons.push(format!(
                "condition {} {} {} evaluates FALSE (sim {}={}) — body skipped",
                lhs,
                cond,
                quote(rhs),
                lhs,
                quote(&value)
            ));
            // Don't simulate the body — it would mislead.
        }
        rollup_children(r);
        return;
    }
    // Condition can't be resolved against the sim env. Simulate the body as a
    // "maybe runs" branch.
    r.children = simulate_body(&op.body, env, depth + 1, p, cmd_name);
    r.reasons.push(format!("condition ({lhs} {cond}) depends on runtime value — body presented as MIGHT-RUN"));
    rollup_children(r);
    if r.outcome == Outcome::WillRun {
        r.outcome = Outcome::MightFail;
    }
}

fn classify_if_call(r: &mut OpResult, op: &Op, env: &SimEnv, depth: usize, p: &Program, cmd_name: &str) {
    r.is_block_entry = true;
    let func = string_arg(&op.args, &["func"]);
    let arg = string_arg(&op.args, &["_0"]);
    let mut resolved = false;
    let mut take = false;
    let mut reason = String::new();
    match func {
        "exists" => {
            if let Some(roots) = &env.fs_read {
                take = path_under(arg, roots);
                resolved = true;
                reason = format!("exists({}) → {} under sim --allow-read", quote(arg), take);
            }
        }
        "has_bin" => {
            if let Some(bins) = &env.bins {
                take = bins.get(arg).copied().unwrap_or(false);
                resolved = true;
                reason = format!("has_bin({}) → {} under sim --have-bin", quote(arg), take);
            }
        }
        _ => {}
    }
    if resolved {
        r.reasons.push(reason);
        if take {
            r.children = simulate_body(&op.body, env, depth + 1, p, cmd_name);
        }
        rollup_children(r);
        return;
    }
    // Unknown predicate or no env restriction — simulate as MIGHT-RUN.
    r.children = simulate_body(&op.body, env, depth + 1, p, cmd_name);
    r.reasons.push(format!("predicate {}({}) not resolvable in sim env — body MIGHT run", func, quote(arg)));
    rollup_children(r);
    if r.outcome == Outcome::WillRun {
        r.outcome = Outcome::MightFail;
    }
}

fn classify_run(r: &mut OpResult, op: &Op, env: &SimEnv, depth: usize, p: &Program) {
    let target = string_arg(&op.args, &["target"]);
    if target.is_empty() {
        r.outcome = Outcome::WillFail;
        r.reasons.push("run: empty target".into());
        return;
    }
    let Some(cmd) = p.commands.get(target) else {
        r.outcome = Outcome::WillFail;
        r.reasons.push(format!("run: target command {} not found", quote(target)));
        return;
    };
    r.is_block_entry = true;
    r.reasons.push(format!("dispatches to command {}", quote(target)));
    r.children = simulate_body(&cmd.ops, env, depth + 1, p, target);
    rollup_children(r);
}

pub(crate) fn classify_wasm_run(r: &mut OpResult, op: &Op, depth: usize) {
    let module_path = string_arg(&op.args, &["path", "_0"]);
    // We can't statically open the .wasm to verify; just note it.
    r.reasons.push(format!("wasm module: {module_path} (capability-gated execution)"));
    // The body has marker ops (wasm_arg etc.); they don't really "fail" but we
    // list them for completeness.
    r.is_block_entry = true;
    for b in &op.body {
        r.children.push(OpResult { op: b.clone(), depth: depth + 1, outcome: Outcome::WillRun, ..Default::default() });
    }
}

// ── env-interpolation check (cross-cutting) ────────────────────────

fn env_ref_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\$\{([A-Z][A-Z0-9_]*)\}").unwrap())
}

fn check_env_interpolation(args: &Map<String, Value>, env: &SimEnv) -> (String, Vec<Scenario>) {
    let Some(host_env) = &env.env else { return (String::new(), Vec::new()) };
    if !env.env_restrict {
        return (String::new(), Vec::new());
    }
    // Env-prefix values (R05) are nested in an object; scan them too.
    let prefix_vals: Vec<&Value> = match args.get("env_prefix") {
        Some(Value::Object(m)) => m.values().collect(),
        _ => Vec::new(),
    };
    for v in args.values().chain(prefix_vals) {
        let Value::String(s) = v else { continue };
        for m in env_ref_re().captures_iter(s) {
            let name = &m[1];
            if !host_env.contains_key(name) {
                return (
                    format!(
                        "references ${{{}}} but sim --env restricts host envs to {}",
                        name,
                        sorted_keys(host_env)
                    ),
                    Vec::new(),
                );
            }
        }
    }
    (String::new(), Vec::new())
}

// ── helpers ────────────────────────────────────────────────────────

pub(crate) fn command_names(p: &Program) -> Vec<String> {
    // BTreeMap keys are sorted already.
    p.commands
        .iter()
        .filter(|(_, c)| !(c.modifiers.private || c.modifiers.test))
        .map(|(n, _)| n.clone())
        .collect()
}

/// First non-empty string value among `keys`.
pub(crate) fn string_arg<'a>(args: &'a Map<String, Value>, keys: &[&str]) -> &'a str {
    for k in keys {
        if let Some(Value::String(s)) = args.get(*k) {
            if !s.is_empty() {
                return s;
            }
        }
    }
    ""
}

pub(crate) fn first_path_arg(args: &Map<String, Value>) -> &str {
    string_arg(args, &["path", "dst", "_0", "_1"])
}

pub(crate) fn first_shell_token(s: &str) -> &str {
    let s = s.trim();
    match s.find([' ', '\t']) {
        Some(i) => &s[..i],
        None => s,
    }
}

pub(crate) fn extract_host(url: &str) -> &str {
    // crude: strip scheme then take up to the first / or ?
    let url = match url.find("://") {
        Some(idx) => &url[idx + 3..],
        None => url,
    };
    match url.find(['/', '?', '#']) {
        Some(i) => &url[..i],
        None => url,
    }
}

pub(crate) fn host_allowed(host: &str, allow: &[String]) -> bool {
    for a in allow {
        if a == host {
            return true;
        }
        // single-label wildcard: *.example.com matches a.example.com (not a.b.example.com)
        if a.starts_with("*.") {
            let suffix = &a[1..]; // ".example.com"
            // require no extra label between * and suffix
            if host.strip_suffix(suffix).is_some_and(|head| !head.contains('.')) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn path_under(path: &str, roots: &[String]) -> bool {
    let clean = gofmt::clean(path);
    for r in roots {
        let cr = gofmt::clean(r);
        if clean == cr || clean.starts_with(&format!("{cr}/")) {
            return true;
        }
    }
    false
}

/// Returns the value if `name` is an auto-bound variable the simulator can
/// resolve against the SimEnv (os / arch / etc.).
fn resolve_auto_bound(name: &str, env: &SimEnv) -> Option<String> {
    let os = |f: &dyn Fn(&str) -> String| if env.os.is_empty() { None } else { Some(f(&env.os)) };
    let arch = |f: &dyn Fn(&str) -> String| if env.arch.is_empty() { None } else { Some(f(&env.arch)) };
    match name {
        "os" => os(&|o| o.to_string()),
        "arch" => arch(&|a| a.to_string()),
        "is_windows" => os(&|o| bool_str(o == "windows").into()),
        "is_macos" => os(&|o| bool_str(o == "darwin").into()),
        "is_linux" => os(&|o| bool_str(o == "linux").into()),
        "is_unix" => os(&|o| bool_str(o != "windows").into()),
        "is_arm64" => arch(&|a| bool_str(a == "arm64").into()),
        "is_amd64" => arch(&|a| bool_str(a == "amd64").into()),
        _ => None,
    }
}

/// Mirrors ops.OsTargetMatches but lives here to avoid a dependency on the ops
/// crate from usecases. Kept in sync by hand.
pub(crate) fn os_matches(target: &str, host: &str) -> bool {
    if target.is_empty() || host.is_empty() {
        return false;
    }
    if target == host {
        return true;
    }
    target == "unix" && matches!(host, "darwin" | "linux" | "freebsd" | "openbsd" | "netbsd")
}

pub(crate) fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

pub(crate) fn sorted_bins(m: &BTreeMap<String, bool>) -> String {
    m.keys().map(String::as_str).collect::<Vec<_>>().join(", ")
}

fn sorted_keys(m: &BTreeMap<String, String>) -> String {
    m.keys().map(String::as_str).collect::<Vec<_>>().join(", ")
}

// ── report rendering ───────────────────────────────────────────────

/// Renders the per-op tree the CLI prints, into `w`.
pub fn render_result(w: &mut dyn Write, res: &SimResult, p: &Program, name: &str) -> std::io::Result<()> {
    let cmd = p.commands.get(name);
    write!(w, "── command {name} ")?;
    if let Some(c) = cmd {
        if !c.description.is_empty() {
            write!(w, "— {}", c.description)?;
        }
    }
    writeln!(w)?;
    for op in &res.ops {
        render_op_result(w, op, "")?;
    }
    writeln!(w)?;
    writeln!(w, "summary: {} will-run · {} will-fail · {} uncertain", res.will_run, res.will_fail, res.uncertain)
}

fn render_op_result(w: &mut dyn Write, r: &OpResult, indent: &str) -> std::io::Result<()> {
    let glyph = match r.outcome {
        Outcome::WillRun => "✓",
        Outcome::WillFail => "✗",
        Outcome::MightFail => "?",
    };
    writeln!(w, "{}{} {}", indent, glyph, summarize_op(&r.op))?;
    for reason in &r.reasons {
        writeln!(w, "{indent}   ↳ {reason}")?;
    }
    for s in &r.scenarios {
        writeln!(w, "{}   • {}", indent, s.description)?;
        writeln!(w, "{}     {}", indent, s.reason)?;
    }
    for c in &r.children {
        render_op_result(w, c, &format!("{indent}   "))?;
    }
    Ok(())
}

fn summarize_op(op: &Op) -> String {
    // R05: show an inline env prefix shell-style in front of the op.
    let prefix: String = match op.args.get("env_prefix") {
        Some(Value::Object(m)) => m.iter().map(|(k, v)| format!("{k}={} ", quote(v.as_str().unwrap_or("")))).collect(),
        _ => String::new(),
    };
    format!("{prefix}{}", summarize_op_inner(op))
}

fn summarize_op_inner(op: &Op) -> String {
    let a = &op.args;
    match op.kind.as_str() {
        "_template_call" => return format!("call {}", string_arg(a, &["name"])),
        "if" => {
            return format!("if {} {} {}", string_arg(a, &["lhs"]), string_arg(a, &["op"]), string_arg(a, &["rhs"]))
        }
        "if_call" => return format!("if {} {}", string_arg(a, &["func"]), quote(string_arg(a, &["_0"]))),
        "run" => return format!("run {}", string_arg(a, &["target"])),
        "wasm_run" => return format!("wasm_run {}", quote(string_arg(a, &["path", "_0"]))),
        _ => {}
    }
    let preview = op_preview(a);
    if preview.is_empty() {
        op.kind.clone()
    } else {
        format!("{} {}", op.kind, preview)
    }
}

fn op_preview(args: &Map<String, Value>) -> String {
    for k in ["msg", "cmd", "url", "path", "dst", "_0", "key", "name"] {
        if let Some(Value::String(s)) = args.get(k) {
            if !s.is_empty() {
                // Byte-slice like Go (may split a UTF-8 sequence; %q then
                // renders the stray bytes as \xNN).
                if s.len() > 60 {
                    let mut b = s.as_bytes()[..57].to_vec();
                    b.extend_from_slice(b"...");
                    return gofmt::quote_bytes(&b);
                }
                return quote(s);
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::{Command, Modifiers};
    use serde_json::json;

    pub(crate) fn op(kind: &str, args: Value) -> Op {
        Op { kind: kind.into(), args: args.as_object().cloned().unwrap_or_default(), ..Default::default() }
    }

    pub(crate) fn block(kind: &str, args: Value, body: Vec<Op>) -> Op {
        Op { body, ..op(kind, args) }
    }

    pub(crate) fn prog(cmds: Vec<(&str, Vec<Op>)>) -> Program {
        let mut p = Program::default();
        for (n, ops) in cmds {
            p.commands.insert(n.into(), Command { name: n.into(), ops, ..Default::default() });
        }
        p
    }

    fn render(res: &SimResult, p: &Program, name: &str) -> String {
        let mut out = Vec::new();
        render_result(&mut out, res, p, name).unwrap();
        String::from_utf8(out).unwrap()
    }

    // R02: `finally` is modelled — it runs after a failing body and is never masked.
    #[test]
    fn try_finally_is_modelled() {
        let mut fail_then_clean = block(
            "try",
            json!({}),
            vec![
                op("fail", json!({"msg": "boom"})),
                op("_catch", json!({})),
                op("_finally", json!({})),
                op("print", json!({"msg": "cleanup"})),
            ],
        );
        fail_then_clean.line = 1;
        let p = prog(vec![("go", vec![fail_then_clean])]);
        let res = simulate_command(&p, "go", &SimEnv::default());
        // Body fails, no rescue: the try fails, but the finally op is still listed as running.
        assert_eq!(res.ops[0].outcome, Outcome::WillFail);
        let out = render(&res, &p, "go");
        assert!(out.contains("finally runs even though the body can fail"), "{out}");
        assert!(out.contains("✓ print \"cleanup\""), "{out}");
        // With a non-empty rescue arm the failure is caught.
        let caught = block(
            "try",
            json!({}),
            vec![
                op("fail", json!({"msg": "boom"})),
                op("_catch", json!({})),
                op("print", json!({"msg": "handled"})),
                op("_finally", json!({})),
            ],
        );
        let p = prog(vec![("go", vec![caught])]);
        let res = simulate_command(&p, "go", &SimEnv::default());
        assert_eq!(res.ops[0].outcome, Outcome::WillRun);
    }

    // R05: the prefix is shown, and an undeclared host env ref is flagged.
    #[test]
    fn env_prefix_is_shown_and_checked() {
        let mut p = prog(vec![(
            "go",
            vec![op("exec", json!({"bin": "kubectl", "_0": "get", "env_prefix": {"KUBECONFIG": "${CFG}", "T": "${SECRET}"}}))],
        )]);
        p.requirements.declared = true;
        p.globals.bindings.push(perch_domain::GlobalBinding { name: "CFG".into(), ..Default::default() });
        let res = simulate_command(&p, "go", &SimEnv::default());
        let out = render(&res, &p, "go");
        assert!(out.contains("KUBECONFIG=\"${CFG}\" T=\"${SECRET}\" exec"), "{out}");
        assert!(out.contains("env prefix applies to this process only: KUBECONFIG, T"), "{out}");
        assert!(out.contains("reads host env \"SECRET\""), "{out}");
        assert!(!out.contains("reads host env \"CFG\""), "{out}");
        assert_eq!(res.ops[0].outcome, Outcome::MightFail);
    }

    #[test]
    fn permissive_env_passes() {
        let p = prog(vec![("go", vec![op("print", json!({"msg": "hi"})), op("shell", json!({"cmd": "ls"}))])]);
        let res = simulate_command(&p, "go", &SimEnv::default());
        assert_eq!((res.will_run, res.will_fail, res.uncertain), (2, 0, 0));
        assert!(SimEnv::default().is_zero());
        assert_eq!(render(&res, &p, "go"), "── command go \n✓ print \"hi\"\n✓ shell \"ls\"\n\nsummary: 2 will-run · 0 will-fail · 0 uncertain\n");
    }

    #[test]
    fn capability_denials() {
        let p = prog(vec![(
            "go",
            vec![
                op("shell", json!({"cmd": "docker ps"})),
                op("http_get", json!({"url": "https://evil.com/x"})),
                op("write_file", json!({"path": "/etc/x"})),
                op("read_file", json!({"path": "/root/y"})),
                op("shell", json!({"cmd": "echo a | cat"})),
                op("fail", json!({})),
            ],
        )]);
        let env = SimEnv {
            bins: Some([("git".to_string(), true), ("echo".to_string(), true)].into()),
            network: Some(vec!["*.github.com".into()]),
            fs_write: Some(vec!["/tmp".into()]),
            fs_read: Some(vec!["/srv".into()]),
            ..Default::default()
        };
        let res = simulate_command(&p, "go", &env);
        assert_eq!((res.will_run, res.will_fail, res.uncertain), (0, 5, 1));
        let out = render(&res, &p, "go");
        assert!(out.contains("✗ shell \"docker ps\"\n   ↳ shell binary \"docker\" not in sim --allow-bin allowlist (have: echo, git)\n"));
        assert!(out.contains("   ↳ HTTP host \"evil.com\" not in sim --allow-host allowlist (have: *.github.com)\n"));
        assert!(out.contains("   ↳ write path \"/etc/x\" is outside sim --allow-write roots (allowed: /tmp)\n"));
        assert!(out.contains("? shell \"echo a | cat\"\n   • command contains shell metachars (pipes / substitution / chained)\n     subsequent processes are not bin-allowlist-checked\n"));
        assert!(out.contains("✗ fail\n   ↳ explicit fail: (no message)\n"));
    }

    #[test]
    fn os_gating_and_if() {
        let mut p = prog(vec![(
            "go",
            vec![
                block("os", json!({"target": "unix"}), vec![op("shell", json!({"cmd": "ls"}))]),
                block("if", json!({"lhs": "is_windows", "op": "truthy"}), vec![op("fail", json!({"msg": "x"}))]),
                block("if", json!({"lhs": "custom", "op": "eq", "rhs": "1"}), vec![op("print", json!({"msg": "m"}))]),
            ],
        )]);
        p.commands.get_mut("go").unwrap().modifiers = Modifiers { require_os: vec!["linux".into(), "darwin".into()], ..Default::default() };
        let env = SimEnv { os: "linux".into(), ..Default::default() };
        let res = simulate_command(&p, "go", &env);
        let out = render(&res, &p, "go");
        assert!(out.contains("   ↳ os \"unix\" matches sim-os — body will run\n"));
        assert!(out.contains("   ↳ condition is_windows truthy \"\" evaluates FALSE (sim is_windows=\"false\") — body skipped\n"));
        assert!(out.contains("   ↳ condition (custom eq) depends on runtime value — body presented as MIGHT-RUN\n"));
        assert_eq!(res.will_fail, 0);
        let bad = simulate_command(&p, "go", &SimEnv { os: "windows".into(), ..Default::default() });
        assert_eq!(bad.will_fail, 1);
        assert_eq!(bad.ops[0].reasons[0], "require_os: command needs [linux darwin]; sim env is OS=\"windows\"");
        assert_eq!(simulate_command(&p, "nope", &env).ops[0].reasons[0], "command \"nope\" not found in program");
    }

    #[test]
    fn helpers() {
        assert!(host_allowed("a.example.com", &["*.example.com".into()]));
        assert!(!host_allowed("a.b.example.com", &["*.example.com".into()]));
        assert_eq!(extract_host("https://h.io:80/p?q"), "h.io:80");
        assert!(path_under("/tmp/a/../b", &["/tmp".into()]));
        assert!(!path_under("/tmpx", &["/tmp".into()]));
        assert!(os_matches("unix", "freebsd") && !os_matches("unix", "windows"));
        assert_eq!(first_shell_token("  a\tb"), "a");
        let long = op("print", json!({"msg": "x".repeat(70)}));
        assert_eq!(summarize_op(&long), format!("print \"{}...\"", "x".repeat(57)));
    }

    #[test]
    fn env_interpolation_restriction() {
        let p = prog(vec![("go", vec![op("print", json!({"msg": "${TOKEN}"}))])]);
        let env = SimEnv {
            env: Some([("HOME".to_string(), "/h".to_string())].into()),
            env_restrict: true,
            ..Default::default()
        };
        let res = simulate_command(&p, "go", &env);
        assert_eq!(res.ops[0].reasons[0], "references ${TOKEN} but sim --env restricts host envs to HOME");
    }

    #[test]
    fn execute_reports_failures() {
        let p = prog(vec![("a", vec![op("fail", json!({"msg": "boom"}))]), ("b", vec![])]);
        let imp = Impl { load: Box::new(move |_| Ok(p.clone())) };
        let mut out = Vec::new();
        let err = imp.execute("f", "", SimEnv::default(), "", &mut out).unwrap_err();
        assert_eq!(err.to_string(), "1 op(s) would fail under the simulated environment");
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("── command a \n✗ fail \"boom\"\n   ↳ explicit fail: boom\n\nsummary: 0 will-run · 1 will-fail · 0 uncertain\n\n── command b \n"));
    }
}

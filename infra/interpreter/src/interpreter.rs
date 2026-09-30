use crate::bindings::{to_string_value, Bindings};
use crate::cliargs::{go_quote, parse_bool, parse_flags, parse_float, parse_int_base0, FlagDef, FlagKind};
use crate::interpolate::{interpolate, interpolate_args};
use crate::io::{SharedReader, SharedWriter};
use perch_domain::{ArgSpec, Command, Op, Program};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The error type every handler returns. Downcast to
/// [`perch_domain::OpError`] with [`find_op_error`]; [`ErrQuit`] / [`ErrTimeout`]
/// are recognised with [`is_quit`] / [`is_timeout`].
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type Result<T> = std::result::Result<T, Error>;

/// `fmt.Errorf` without `%w`: an error from a message.
pub fn err(msg: impl Into<String>) -> Error {
    Error::from(msg.into())
}

/// `fmt.Errorf("<prefix>: %w", source)`. The source stays reachable via
/// `source()` so [`find_op_error`] sees through it.
#[derive(Debug)]
pub struct Wrapped {
    pub msg: String,
    pub source: Error,
}

impl fmt::Display for Wrapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.msg, self.source)
    }
}

impl std::error::Error for Wrapped {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.source)
    }
}

/// Wraps `source` with a prefix, like `fmt.Errorf("%s: %w", prefix, source)`.
pub fn wrap(prefix: impl Into<String>, source: Error) -> Error {
    Box::new(Wrapped { msg: prefix.into(), source })
}

/// Returned by `run` / `run_ops` when the BeforeOp hook returns
/// [`OpAction::Quit`]. Callers treat it as a clean stop rather than a failure.
#[derive(Debug)]
pub struct ErrQuit;

impl fmt::Display for ErrQuit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("interpreter: quit requested")
    }
}
impl std::error::Error for ErrQuit {}

/// Returned when the interpreter exceeds its `deadline`.
#[derive(Debug)]
pub struct ErrTimeout;

impl fmt::Display for ErrTimeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("interpreter: --max-runtime exceeded")
    }
}
impl std::error::Error for ErrTimeout {}

/// `err == interpreter.ErrQuit` (exact, not through wrappers).
pub fn is_quit(e: &Error) -> bool {
    e.downcast_ref::<ErrQuit>().is_some()
}

/// `err == interpreter.ErrTimeout` (exact, not through wrappers).
pub fn is_timeout(e: &Error) -> bool {
    e.downcast_ref::<ErrTimeout>().is_some()
}

/// `errors.As(err, &opErr)`: the first [`perch_domain::OpError`] in the chain.
pub fn find_op_error<'a>(e: &'a (dyn std::error::Error + 'static)) -> Option<&'a perch_domain::OpError> {
    let mut cur: Option<&'a (dyn std::error::Error + 'static)> = Some(e);
    while let Some(c) = cur {
        if let Some(oe) = c.downcast_ref::<perch_domain::OpError>() {
            return Some(oe);
        }
        cur = c.source();
    }
    None
}

/// The interpolated args of one op call. Derefs to the arg map; a block op's
/// nested body arrives in `body` (Go's `_body` sentinel key), and `_capture`
/// (bool true) stays in the map when the result is bound.
pub struct Args<'a> {
    pub map: Map<String, Value>,
    pub body: &'a [Op],
}

impl Deref for Args<'_> {
    type Target = Map<String, Value>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

/// Runs one op against the runtime state. It receives interpolated args
/// (string values already substituted) and may capture into bindings. The
/// interpreter passes itself so block ops can recurse. Returning `Value::Null`
/// is Go's `nil`.
pub type Handler = Arc<dyn Fn(&Interpreter, &mut Bindings, &Args<'_>) -> Result<Value> + Send + Sync>;

/// Boxes a closure as a [`Handler`]; the registration idiom is
/// `handlers.insert("print".into(), handler(|i, b, args| { ... }))`.
pub fn handler<F>(f: F) -> Handler
where
    F: Fn(&Interpreter, &mut Bindings, &Args<'_>) -> Result<Value> + Send + Sync + 'static,
{
    Arc::new(f)
}

/// The verdict from a BeforeOp hook — run the op, skip it, run this op +
/// everything else without further asking, or stop the whole walk. Powers
/// --ask / --dry-run / --step previews.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpAction {
    /// Execute as normal.
    Run,
    /// Don't execute; if `let X = …` set X to "".
    Skip,
    /// Execute this op, then clear BeforeOp.
    RunAll,
    /// Stop the whole command immediately.
    Quit,
}

/// An optional pre-dispatch hook. When set, the interpreter calls it before
/// each op (with already-interpolated args) and respects the returned action.
pub type BeforeOp = Arc<dyn Fn(&Op, &Map<String, Value>, &mut Bindings) -> OpAction + Send + Sync>;

/// Receives each op's outcome after its handler returns. Used by the audit log
/// (infra/audit) to record a structured trace. `result` is `Null` on error.
pub type AfterOp =
    Arc<dyn Fn(&Op, &Map<String, Value>, &Bindings, &Value, Option<&Error>, Duration) + Send + Sync>;

/// Called once per command invocation BEFORE any op fires (after platform/args
/// checks). Used by the `requires` block enforcer to verify the host machine
/// satisfies the file's declared manifest. Wired up by the orchestrator to
/// avoid an ops → interpreter → ops cycle.
pub type PreflightHook = Arc<dyn Fn(&Interpreter, &Program) -> Result<()> + Send + Sync>;

/// Maps an op kind to the capability categories it belongs to (e.g.
/// "write_file" → ["write"]). Wired from `ops::hook_category_of` in the
/// orchestrator. Lets a `hooks before write …` line match every write op.
pub type HookCategory = Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>;

/// A paired Before/After hook independent of --ask / --dry-run. Block ops
/// naturally nest (their children's Before/After fire between the block's
/// Before and After), so a tracer that maintains a stack builds a span tree for
/// free. Used by --report to produce the post-run tree view. Implementations
/// use interior mutability (calls may come from `parallel` threads).
pub trait Tracer: Send + Sync {
    fn before(&self, op: &Op, args: &Map<String, Value>);
    fn after(&self, op: &Op, result: &Value, err: Option<&Error>, dur: Duration);
}

/// Gates which URLs perch's HTTP ops will dial and which redirects they'll
/// follow. See infra/ops http for the enforcement.
#[derive(Debug, Clone, Default)]
pub struct HTTPPolicy {
    /// Caps the redirect chain. 0 = refuse any redirect.
    pub max_redirects: i64,
    /// Disables the SSRF guard. With this off (default), perch refuses to dial
    /// any host that resolves to a private / loopback / link-local /
    /// unspecified IP — closes the AWS-metadata SSRF and the localhost-pivot.
    pub allow_private_ips: bool,
    /// Permits https → http redirects. Off by default — a 30x downgrade is
    /// almost always an attack signal.
    pub allow_scheme_downgrade: bool,
    /// When non-empty, restricts EVERY request URL (and every redirect
    /// destination) to the listed hosts. Patterns:
    ///   - exact:        api.github.com
    ///   - single-label: *.s3.amazonaws.com     (matches api.x.com NOT a.b.x.com)
    ///   - IP literal:   10.0.0.1
    ///   - host:port:    localhost:8080         (port must match exactly)
    ///
    /// Composes AND-wise with the SSRF guard — a host in the allowlist still
    /// has to pass the private-IP check unless allow_private_ips is also set.
    pub allowed_hosts: Vec<String>,
}

/// Walks a Program's ops, holding the handler registry and the IO sinks.
/// `Sync`, so block handlers (`parallel`) can share it across threads.
pub struct Interpreter {
    pub handlers: HashMap<String, Handler>,
    pub program: Arc<Program>,
    pub stdout: SharedWriter,
    pub stderr: SharedWriter,
    pub stdin: SharedReader,
    /// Consulted before each op dispatch. Used by --ask (step-through
    /// confirmation) and --dry-run (skip everything, just print). `None`
    /// means "run normally." Behind a mutex because `RunAll` clears it
    /// mid-walk; use [`Interpreter::set_before_op`].
    pub before_op: Mutex<Option<BeforeOp>>,
    /// When `Some`, restricts which host env vars resolve via ${NAME}
    /// fallthrough. Wired into every fresh Bindings made by `run`. `None`
    /// means "all host env vars visible" (legacy).
    pub env_allowlist: Option<HashMap<String, bool>>,
    /// When `Some`, restricts the first token of every `shell` command to the
    /// listed names. `None` = no restriction.
    pub allowed_shell_bins: Option<HashMap<String, bool>>,
    /// Rejects shell commands containing pipes / redirects /
    /// command-substitution / && / || / `;`. Combined with
    /// `allowed_shell_bins` this neutralizes most shell-injection vectors.
    pub no_shell_metachars: bool,
    /// Called AFTER each op's handler returns. Used by `--audit FILE.ndjson`.
    pub after_op: Option<AfterOp>,
    /// When set, caps the wall-clock budget for the invocation. Checked before
    /// each op dispatch — a long-running shell can't be interrupted mid-call,
    /// but the NEXT op after it returns [`ErrTimeout`]. Set by
    /// `--max-runtime SECS`.
    pub deadline: Option<Instant>,
    /// Governs http_get / http_post / download redirect and destination
    /// behaviour. `None` = secure defaults (no private IPs, no scheme
    /// downgrade, max 5 hops).
    pub http_policy: Option<HTTPPolicy>,
    /// Receives Before/After events for every op. Used to build the --report
    /// span tree.
    pub tracer: Option<Arc<dyn Tracer>>,
    pub preflight_hook: Option<PreflightHook>,
    pub hook_category: Option<HookCategory>,
}

impl Interpreter {
    /// Constructs an Interpreter with stdout/stderr/stdin defaulted to the
    /// process's.
    pub fn new(handlers: HashMap<String, Handler>, program: impl Into<Arc<Program>>) -> Interpreter {
        Interpreter {
            handlers,
            program: program.into(),
            stdout: SharedWriter::stdout(),
            stderr: SharedWriter::stderr(),
            stdin: SharedReader::stdin(),
            before_op: Mutex::new(None),
            env_allowlist: None,
            allowed_shell_bins: None,
            no_shell_metachars: false,
            after_op: None,
            deadline: None,
            http_policy: None,
            tracer: None,
            preflight_hook: None,
            hook_category: None,
        }
    }

    pub fn set_before_op(&self, hook: Option<BeforeOp>) {
        *self.before_op.lock().unwrap_or_else(|e| e.into_inner()) = hook;
    }

    /// `run` that bypasses the private/test gate. Used by the test runner to
    /// invoke `test`-marked commands directly. Otherwise identical to `run`.
    pub fn run_private(&self, command_name: &str, cli_args: &[String]) -> Result<()> {
        self.run_command(command_name, cli_args, true)
    }

    /// Dispatches to the named command (or the catch handler) with the
    /// supplied CLI args. Returns the process-style error.
    pub fn run(&self, command_name: &str, cli_args: &[String]) -> Result<()> {
        self.run_command(command_name, cli_args, false)
    }

    fn run_command(&self, command_name: &str, cli_args: &[String], allow_private: bool) -> Result<()> {
        let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let mut b = Bindings::new(&cwd);
        b.env_allowlist = self.env_allowlist.clone();
        self.seed_globals_and_env(&mut b);

        let cmd = self.program.commands.get(command_name);
        // Test-marked commands are only callable via `perch test`. From the
        // regular run path they look like `private` — treat them the same.
        // The test runner uses run_private (allow_private=true) to invoke them
        // anyway.
        let gated = cmd.is_some_and(|c| (c.modifiers.private || c.modifiers.test) && !allow_private);
        let cmd = match cmd {
            Some(c) if !gated => c,
            _ => {
                let catch = match &self.program.catch {
                    Some(c) => c,
                    None => return Err(err(format!("command not found: {}", go_quote(command_name)))),
                };
                // catch: bind the unknown name. ${proxy_args} (the full unknown
                // invocation joined with spaces) is bound ONLY if the catch
                // declared the `proxy_args` modifier — otherwise referencing it
                // fails with unresolved_var, which is the right behavior for
                // the "I don't intend to forward arbitrary input to shell"
                // case. Declaring `proxy_args` is the explicit opt-in to the
                // catch→shell forwarding pattern that --scan flags as HIGH.
                b.set(&catch.bind, command_name);
                if catch.proxy_args {
                    let mut full = vec![command_name.to_string()];
                    full.extend(cli_args.iter().cloned());
                    b.set("proxy_args", full.join(" "));
                }
                return self.run_ops(&catch.ops, &mut b);
            }
        };

        self.check_platform(cmd)?;

        // File-declared `requires` manifest preflight. Hook is wired by the
        // orchestrator to ops preflight; None when running in contexts where
        // requirements shouldn't be enforced (e.g. `perch --check`).
        if let Some(hook) = &self.preflight_hook {
            hook(self, &self.program)?;
        }

        let parsed = self.parse_args(cmd, cli_args)?;
        for (k, v) in parsed {
            b.vars.insert(k, v);
        }
        for (k, v) in &cmd.env {
            let rv = interpolate(v, &b)?;
            b.env.insert(k.clone(), rv);
        }
        if !cmd.modifiers.dir.is_empty() {
            b.cwd = interpolate(&cmd.modifiers.dir, &b)?;
        }

        let res = self.run_ops(&cmd.ops, &mut b);
        if let Err(e) = &res {
            if is_quit(e) {
                self.stdout.write_str("↪ stopped by user\n").ok();
                return Ok(());
            }
            if is_timeout(e) {
                self.stderr.write_str("↪ stopped: --max-runtime exceeded\n").ok();
            }
        }
        res
    }

    /// Walks a slice of ops in order.
    pub fn run_ops(&self, ops: &[Op], b: &mut Bindings) -> Result<()> {
        for op in ops {
            self.run_op(op, b)?;
        }
        Ok(())
    }

    /// Interpolates args and dispatches one op.
    pub fn run_op(&self, op: &Op, b: &mut Bindings) -> Result<()> {
        // Wall-clock budget: refuse to start a new op if we're past the
        // deadline. We can't interrupt a long-running op mid-call, but we can
        // prevent the next one from firing.
        if let Some(d) = self.deadline {
            if Instant::now() > d {
                return Err(Box::new(ErrTimeout));
            }
        }
        let mut args = interpolate_args(&op.args, b).map_err(|e| wrap(format!("op {}", op.kind), e))?;
        // Consult the optional preview hook BEFORE dispatch so previews see the
        // same interpolated args the handler would.
        let hook = self.before_op.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(hook) = hook {
            match hook(op, &args, b) {
                OpAction::Skip => {
                    // Skipped: capture an empty value so downstream ${X} still
                    // resolves (to ""), keeping interpolation alive in dry-run.
                    if !op.capture_into.is_empty() {
                        b.set(&op.capture_into, "");
                    }
                    return Ok(());
                }
                OpAction::Quit => return Err(Box::new(ErrQuit)),
                // Disable the hook for the rest of the walk.
                OpAction::RunAll => self.set_before_op(None),
                OpAction::Run => {}
            }
        }
        // Block ops receive their body via `Args::body`; the handler reads it
        // and recurses with run_ops.
        let mut exec_op: Option<Op> = None;
        let h = match self.handlers.get(&op.kind) {
            Some(h) => h.clone(),
            None => {
                // Unknown op kind → treat it as a bare declared-bin invocation
                // (exec): the kind IS the bin. This makes `let r = docker ps`
                // work without the `exec` keyword for the short-arg capture
                // forms that the grammar would otherwise resolve to an op-kind.
                // The exec handler applies the same requires/bin gate, so an
                // undeclared bin still fails bin_not_declared, and a genuine op
                // typo surfaces as that error too. Block ops never reach here
                // (their kinds are registered handlers).
                let exec_h = match self.handlers.get("exec") {
                    Some(h) if op.kind != "exec" => h.clone(),
                    _ => return Err(err(format!("unknown op: {}", go_quote(&op.kind)))),
                };
                args.insert("bin".to_string(), Value::String(op.kind.clone()));
                exec_op = Some(Op { kind: "exec".to_string(), ..op.clone() });
                exec_h
            }
        };
        let op: &Op = exec_op.as_ref().unwrap_or(op);
        // Signal capture intent so output-producing ops (exec, pipe) can stay
        // quiet when their result is bound (`let x = exec …`) but stream when
        // used as a bare statement — matching the old shell vs shell_output split.
        if !op.capture_into.is_empty() && !args.contains_key("_capture") {
            args.insert("_capture".to_string(), Value::Bool(true));
        }
        // before-hooks fire just ahead of the side effect. A handler that
        // errors VETOES the op (the error propagates, the op never runs).
        // Skipped while already inside a hook (re-entrancy guard) and on the
        // fast path with no hooks declared.
        let hooks_active = !self.program.hooks.is_empty() && !b.in_hook;
        if hooks_active {
            self.fire_hooks("before", op, &args, b, None)?;
        }
        if let Some(t) = &self.tracer {
            t.before(op, &args);
        }
        let start = Instant::now();
        let call_args = Args { map: args, body: &op.body };
        let res = h(self, b, &call_args);
        let dur = start.elapsed();
        let args = call_args.map;
        let (val, mut err_out) = match res {
            Ok(v) => (v, None),
            Err(e) => (Value::Null, Some(e)),
        };
        if let Some(after) = &self.after_op {
            after(op, &args, b, &val, err_out.as_ref(), dur);
        }
        if let Some(t) = &self.tracer {
            t.after(op, &val, err_out.as_ref(), dur);
        }
        if hooks_active {
            if let Some(e) = &err_out {
                // on_error handlers observe the failure; their own error (if
                // any) doesn't mask the op's.
                let _ = self.fire_hooks("on_error", op, &args, b, Some(e));
            }
            // after handlers always run; an after-handler error surfaces only
            // when the op itself succeeded (don't override the op's own error).
            if let Err(herr) = self.fire_hooks("after", op, &args, b, err_out.as_ref()) {
                if err_out.is_none() {
                    err_out = Some(herr);
                }
            }
        }
        if let Some(e) = err_out {
            return Err(e);
        }
        if !op.capture_into.is_empty() {
            b.vars.insert(op.capture_into.clone(), val);
        }
        Ok(())
    }

    /// Runs every declared hook of the given timing whose target matches
    /// `op.kind` — either an exact op-kind match, a capability-category match
    /// (via `hook_category`), or "any". Each handler runs as its named command
    /// under a re-entrancy-guarded child binding seeded with ${hook.*} context.
    /// A `before` handler's error is returned (vetoing the op); other timings'
    /// errors are returned for the caller to decide.
    fn fire_hooks(
        &self,
        timing: &str,
        op: &Op,
        args: &Map<String, Value>,
        b: &Bindings,
        op_err: Option<&Error>,
    ) -> Result<()> {
        let mut cats: Option<Vec<String>> = None;
        for h in &self.program.hooks {
            if h.timing != timing {
                continue;
            }
            if !self.hook_matches(&h.target, &op.kind, &mut cats) {
                continue;
            }
            let cmd = match self.program.commands.get(&h.handler) {
                Some(c) => c,
                None => {
                    return Err(err(format!(
                        "hook {} {}: no such handler command {}",
                        h.timing,
                        h.target,
                        go_quote(&h.handler)
                    )))
                }
            };
            let mut hb = b.child_for_hook();
            hb.set("hook.op", op.kind.as_str());
            hb.set("hook.timing", timing);
            hb.set("hook.target", hook_target(args));
            match op_err {
                Some(e) => hb.set("hook.error", e.to_string()),
                None => hb.set("hook.error", ""),
            }
            if let Err(e) = self.run_ops(&cmd.ops, &mut hb) {
                return Err(wrap(format!("{} hook {}", timing, go_quote(&h.handler)), e));
            }
        }
        Ok(())
    }

    /// Whether a hook target matches an op kind. `cats` caches the op's
    /// categories across the hooks loop (computed at most once).
    fn hook_matches(&self, target: &str, kind: &str, cats: &mut Option<Vec<String>>) -> bool {
        if target == kind || target == "any" {
            return true;
        }
        let Some(hc) = &self.hook_category else {
            return false;
        };
        let cats = cats.get_or_insert_with(|| hc(kind));
        cats.iter().any(|c| c == target)
    }

    fn seed_globals_and_env(&self, b: &mut Bindings) {
        // Auto-bindings: stable, host-derived values that conditionals can
        // reference without the user declaring them. These appear BEFORE
        // globals so a user-declared global of the same name takes priority.
        //
        // The full catalog is intentionally large because cross-platform
        // install / build / uninstall scripts otherwise paper over differences
        // with shell glue. Pre-binding ${home}, ${cache_dir}, ${exe_path},
        // ${path_sep}, ${exe_ext}, ${is_windows} etc. removes that need.
        let goos = go_os();
        let goarch = go_arch();
        b.set("os", goos);
        b.set("arch", goarch);
        b.set("is_windows", goos == "windows");
        b.set("is_macos", goos == "darwin");
        b.set("is_linux", goos == "linux");
        b.set("is_unix", goos != "windows");
        b.set("is_arm64", goarch == "arm64");
        b.set("is_amd64", goarch == "amd64");
        b.set("cpu_count", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1));
        b.set("pid", std::process::id());
        b.set("now_unix", SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));

        // Path / filesystem conventions that differ by OS.
        if goos == "windows" {
            b.set("path_sep", "\\");
            b.set("path_list_sep", ";");
            b.set("exe_ext", ".exe");
            b.set("null_device", "NUL");
            b.set("shell_name", "cmd");
        } else {
            b.set("path_sep", "/");
            b.set("path_list_sep", ":");
            b.set("exe_ext", "");
            b.set("null_device", "/dev/null");
            b.set("shell_name", "bash");
        }

        // Standard directories. Each is left unset rather than crashing if the
        // platform can't report it.
        let home = user_home_dir();
        if let Some(h) = &home {
            b.set("home", h.as_str());
            b.set("home_dir", h.as_str());
        }
        if let Some(d) = user_config_dir(goos, home.as_deref()) {
            b.set("config_dir", d);
        }
        if let Some(d) = user_cache_dir(goos, home.as_deref()) {
            b.set("cache_dir", d);
        }
        b.set("temp_dir", std::env::temp_dir().to_string_lossy().into_owned());
        match goos {
            "windows" => b.set("data_dir", std::env::var("APPDATA").unwrap_or_default()),
            "darwin" => {
                if let Some(h) = &home {
                    b.set("data_dir", format!("{}/Library/Application Support", h.trim_end_matches('/')));
                }
            }
            _ => {
                if let Some(h) = &home {
                    b.set("data_dir", format!("{}/.local/share", h.trim_end_matches('/')));
                }
            }
        }

        // The running binary itself.
        if let Ok(mut exe) = std::env::current_exe() {
            if let Ok(resolved) = exe.canonicalize() {
                exe = resolved;
            }
            b.set("exe_path", exe.to_string_lossy().into_owned());
            b.set("exe_dir", exe.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into()));
            b.set("exe_name", exe.file_name().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
        }

        // The source .perch file (empty when embedded inside a built binary).
        if !self.program.script_path.is_empty() {
            let sp = std::path::Path::new(&self.program.script_path);
            b.set("script_path", self.program.script_path.as_str());
            b.set(
                "script_dir",
                sp.parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| ".".into()),
            );
        } else {
            b.set("script_path", "");
            b.set("script_dir", "");
        }

        // Identity.
        match current_user() {
            Some((name, uid)) => {
                b.set("user", name);
                b.set("uid", uid);
            }
            None => b.set("user", std::env::var("USER").unwrap_or_default()),
        }
        if let Some(h) = hostname() {
            b.set("hostname", h);
        }
        // Globals are seeded in declared order. String values are interpolated
        // against the bindings built so far, so globals can reference earlier
        // globals and host env (e.g. ${HOME}) at seed time — no recursive
        // substitution at op-run time.
        for g in &self.program.globals.bindings {
            if let Value::String(s) = &g.value {
                if let Ok(rv) = interpolate(s, b) {
                    b.set(&g.name, rv);
                    continue;
                }
            }
            b.set(&g.name, g.value.clone());
        }
    }

    fn check_platform(&self, cmd: &Command) -> Result<()> {
        let goos = go_os();
        let goarch = go_arch();
        if !cmd.modifiers.require_os.is_empty() && !cmd.modifiers.require_os.iter().any(|o| o == goos) {
            return Err(err(format!(
                "command {} is restricted to OS in [{}]; running on {}",
                go_quote(&cmd.name),
                cmd.modifiers.require_os.join(", "),
                goos
            )));
        }
        if !cmd.modifiers.require_arch.is_empty() && !cmd.modifiers.require_arch.iter().any(|a| a == goarch) {
            return Err(err(format!(
                "command {} is restricted to arch in [{}]; running on {}",
                go_quote(&cmd.name),
                cmd.modifiers.require_arch.join(", "),
                goarch
            )));
        }
        Ok(())
    }

    /// The CLI argv → arg-name-map parsing used by `run`. Used by `run` op (in
    /// infra/ops) so `run NAME -arg=value` inside a body goes through the same
    /// parser as `perch NAME -arg=value` from a shell.
    pub fn parse_cli_args(&self, cmd: &Command, cli_args: &[String]) -> Result<Map<String, Value>> {
        self.parse_args(cmd, cli_args)
    }

    fn parse_args(&self, cmd: &Command, cli_args: &[String]) -> Result<Map<String, Value>> {
        let mut out = Map::new();

        if cmd.modifiers.proxy_args {
            out.insert("proxy_args".to_string(), Value::String(cli_args.join(" ")));
            return Ok(out);
        }

        // Register flags for non-positional args.
        let mut defs: Vec<FlagDef> = Vec::new();
        let mut refs: Vec<&ArgSpec> = Vec::new();
        for a in &cmd.args {
            if a.index.is_some() {
                continue;
            }
            let kind = match a.ty.as_str() {
                "string" => FlagKind::Str,
                "bool" => FlagKind::Bool,
                "int" => FlagKind::Int,
                "float" => FlagKind::Float,
                _ => return Err(err(format!("arg {}: unknown type {}", go_quote(&a.name), go_quote(&a.ty)))),
            };
            defs.push(FlagDef { name: a.name.clone(), kind });
            refs.push(a);
        }

        let parsed = parse_flags(&defs, cli_args).map_err(err)?;

        // Apply parsed flags + check required.
        for a in refs {
            let def = if a.has_default { Some(&a.default) } else { None };
            let val = match parsed.provided.get(&a.name) {
                Some(raw) => match a.ty.as_str() {
                    "string" => Value::String(raw.clone()),
                    "bool" => Value::Bool(parse_bool(raw).unwrap_or(false)),
                    "int" => Value::from(parse_int_base0(raw).unwrap_or(0)),
                    _ => float_value(parse_float(raw).unwrap_or(0.0)),
                },
                None => match a.ty.as_str() {
                    "string" => Value::String(def.and_then(|d| d.as_str()).unwrap_or("").to_string()),
                    "bool" => Value::Bool(def_bool(def)),
                    "int" => Value::from(def_int(def)),
                    _ => float_value(def_float(def)),
                },
            };
            out.insert(a.name.clone(), val);
            if !a.has_default && !a.optional && !parsed.provided.contains_key(&a.name) {
                return Err(err(format!("missing required argument -{}", a.name)));
            }
        }

        // Positional args.
        let rest = &parsed.rest;
        for a in &cmd.args {
            let Some(idx) = a.index else { continue };
            // Rest arg: gather every remaining positional from this index on
            // into a newline-joined string plus a count. The validator ensures
            // this is the last arg and type == "string".
            if a.rest {
                let start = idx.max(0) as usize;
                let values: Vec<&String> = rest.iter().skip(start).collect();
                let joined = values.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n");
                out.insert(a.name.clone(), Value::String(joined));
                out.insert(format!("{}_count", a.name), Value::from(values.len()));
                continue;
            }
            if idx < 0 || idx as usize >= rest.len() {
                if !a.has_default && !a.optional {
                    return Err(err(format!("missing positional argument #{} ({})", idx, a.name)));
                }
                out.insert(a.name.clone(), a.default.clone());
                continue;
            }
            let raw = &rest[idx as usize];
            match a.ty.as_str() {
                "string" => {
                    out.insert(a.name.clone(), Value::String(raw.clone()));
                }
                "int" => match raw.parse::<i64>() {
                    Ok(n) => {
                        out.insert(a.name.clone(), Value::from(n));
                    }
                    Err(_) => return Err(err(format!("arg {}: invalid int {}", a.name, go_quote(raw)))),
                },
                "float" => match parse_float(raw) {
                    Ok(f) => {
                        out.insert(a.name.clone(), float_value(f));
                    }
                    Err(_) => return Err(err(format!("arg {}: invalid float {}", a.name, go_quote(raw)))),
                },
                "bool" => match parse_bool(raw) {
                    Some(v) => {
                        out.insert(a.name.clone(), Value::Bool(v));
                    }
                    None => return Err(err(format!("arg {}: invalid bool {}", a.name, go_quote(raw)))),
                },
                _ => {}
            }
        }
        Ok(out)
    }
}

fn float_value(f: f64) -> Value {
    serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
}

/// def_{bool,int,float} coerce an arg's declared default into a typed value.
/// Quote-optional capture normalizes every default to a STRING (`default 6379`
/// → "6379"), so these parse the string form; they also accept the native JSON
/// types for defaults built directly in a Program (tests, embeds).
fn def_bool(def: Option<&Value>) -> bool {
    match def {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => parse_bool(s).unwrap_or(false),
        _ => false,
    }
}

fn def_int(def: Option<&Value>) -> i64 {
    match def {
        Some(Value::Number(n)) => n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn def_float(def: Option<&Value>) -> f64 {
    match def {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => parse_float(s).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// The resource a hook handler most likely cares about — the path / url / bin /
/// first positional — for the ${hook.target} binding.
fn hook_target(args: &Map<String, Value>) -> String {
    for k in ["path", "url", "host", "bin", "dst", "src", "name", "_0"] {
        if let Some(v) = args.get(k) {
            let s = to_string_value(v);
            if !s.is_empty() {
                return s;
            }
        }
    }
    String::new()
}

/// Go's `runtime.GOOS` spelling of the host OS.
pub fn go_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        o => o,
    }
}

/// Go's `runtime.GOARCH` spelling of the host architecture.
pub fn go_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        "powerpc64" => "ppc64",
        a => a,
    }
}

fn user_home_dir() -> Option<String> {
    let key = if go_os() == "windows" { "USERPROFILE" } else { "HOME" };
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn user_config_dir(goos: &str, home: Option<&str>) -> Option<String> {
    match goos {
        "windows" => std::env::var("AppData").or_else(|_| std::env::var("APPDATA")).ok().filter(|s| !s.is_empty()),
        "darwin" => home.map(|h| format!("{h}/Library/Application Support")),
        _ => match std::env::var("XDG_CONFIG_HOME").ok().filter(|s| s.starts_with('/')) {
            Some(x) => Some(x),
            None => home.map(|h| format!("{h}/.config")),
        },
    }
}

fn user_cache_dir(goos: &str, home: Option<&str>) -> Option<String> {
    match goos {
        "windows" => std::env::var("LocalAppData").or_else(|_| std::env::var("LOCALAPPDATA")).ok().filter(|s| !s.is_empty()),
        "darwin" => home.map(|h| format!("{h}/Library/Caches")),
        _ => match std::env::var("XDG_CACHE_HOME").ok().filter(|s| s.starts_with('/')) {
            Some(x) => Some(x),
            None => home.map(|h| format!("{h}/.cache")),
        },
    }
}

#[cfg(unix)]
fn current_user() -> Option<(String, String)> {
    // SAFETY: getuid has no preconditions; getpwuid_r writes into our buffer.
    unsafe {
        let uid = libc::getuid();
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut buf = vec![0u8; 4096];
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let rc = libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr() as *mut libc::c_char, buf.len(), &mut result);
        if rc != 0 || result.is_null() {
            return None;
        }
        let name = std::ffi::CStr::from_ptr(pwd.pw_name).to_string_lossy().into_owned();
        Some((name, uid.to_string()))
    }
}

#[cfg(not(unix))]
fn current_user() -> Option<(String, String)> {
    std::env::var("USERNAME").ok().map(|n| (n, String::new()))
}

#[cfg(unix)]
fn hostname() -> Option<String> {
    let mut buf = vec![0u8; 256];
    // SAFETY: buffer is valid for its length.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
}

#[cfg(not(unix))]
fn hostname() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::SharedBuf;
    use perch_domain::{Catch, GlobalBinding, Globals, Modifiers};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    /// A small handler set for testing without dragging in the full ops crate.
    fn make_handlers() -> HashMap<String, Handler> {
        let mut m: HashMap<String, Handler> = HashMap::new();
        m.insert(
            "print".into(),
            handler(|i, _b, a| {
                let msg = a.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                i.stdout.write_str(&format!("{msg}\n")).ok();
                Ok(Value::Null)
            }),
        );
        m.insert(
            "upper".into(),
            handler(|_i, _b, a| {
                let v = a.get("_0").and_then(|v| v.as_str()).unwrap_or("");
                Ok(Value::String(v.to_uppercase()))
            }),
        );
        // Minimal stub: matches "eq" via direct args.lhs/args.rhs.
        m.insert(
            "if".into(),
            handler(|i, b, a| {
                let lhs = a.get("lhs").and_then(|v| v.as_str()).unwrap_or("");
                let rhs = a.get("rhs").and_then(|v| v.as_str()).unwrap_or("");
                if b.lookup(lhs).as_deref() == Some(rhs) {
                    i.run_ops(a.body, b)?;
                }
                Ok(Value::Null)
            }),
        );
        m
    }

    fn cmds(list: Vec<Command>) -> BTreeMap<String, Command> {
        list.into_iter().map(|c| (c.name.clone(), c)).collect()
    }

    fn print_op(msg: &str) -> Op {
        Op { kind: "print".into(), args: args(json!({"msg": msg})), ..Default::default() }
    }

    fn greet_program() -> Program {
        Program {
            commands: cmds(vec![Command {
                name: "greet".into(),
                args: vec![ArgSpec {
                    name: "name".into(),
                    ty: "string".into(),
                    default: json!("world"),
                    has_default: true,
                    ..Default::default()
                }],
                ops: vec![print_op("Hello ${name}")],
                ..Default::default()
            }]),
            ..Default::default()
        }
    }

    fn interp(p: Program) -> (Interpreter, SharedBuf) {
        let out = SharedBuf::new();
        let mut i = Interpreter::new(make_handlers(), p);
        i.stdout = out.writer();
        (i, out)
    }

    fn sv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn run_command_with_default_arg() {
        let (i, out) = interp(greet_program());
        i.run("greet", &[]).unwrap();
        assert!(out.contents().contains("Hello world"), "{}", out.contents());
    }

    #[test]
    fn run_command_with_arg() {
        let (i, out) = interp(greet_program());
        i.run("greet", &sv(&["-name=Alice"])).unwrap();
        assert!(out.contents().contains("Hello Alice"), "{}", out.contents());
    }

    #[test]
    fn missing_required_arg() {
        let p = Program {
            commands: cmds(vec![Command {
                name: "x".into(),
                args: vec![ArgSpec { name: "must".into(), ty: "string".into(), ..Default::default() }],
                ..Default::default()
            }]),
            ..Default::default()
        };
        let (i, _) = interp(p);
        let e = i.run("x", &[]).unwrap_err();
        assert!(e.to_string().contains("missing required"), "{e}");
    }

    #[test]
    fn let_capture_flows_to_next_op() {
        let p = Program {
            commands: cmds(vec![Command {
                name: "x".into(),
                ops: vec![
                    Op {
                        kind: "upper".into(),
                        args: args(json!({"_0": "hi"})),
                        capture_into: "U".into(),
                        ..Default::default()
                    },
                    print_op("got ${U}"),
                ],
                ..Default::default()
            }]),
            ..Default::default()
        };
        let (i, out) = interp(p);
        i.run("x", &[]).unwrap();
        assert!(out.contents().contains("got HI"), "{}", out.contents());
    }

    #[test]
    fn catch() {
        let p = Program {
            catch: Some(Catch { bind: "name".into(), ops: vec![print_op("huh: ${name}")], ..Default::default() }),
            ..Default::default()
        };
        let (i, out) = interp(p);
        i.run("blorp", &[]).unwrap();
        assert!(out.contents().contains("huh: blorp"), "{}", out.contents());
    }

    #[test]
    fn catch_proxy_args() {
        // catch with the proxy_args modifier exposes the full unknown
        // invocation as ${proxy_args} so wrappers can forward to an underlying
        // tool. WITHOUT the modifier, ${proxy_args} is unbound (see the next
        // test).
        let p = Program {
            catch: Some(Catch {
                bind: "name".into(),
                proxy_args: true, // explicit opt-in to ${proxy_args} binding
                ops: vec![print_op("→ ${proxy_args}")],
                ..Default::default()
            }),
            ..Default::default()
        };
        let (i, out) = interp(p);
        i.run("log", &sv(&["--oneline", "-10"])).unwrap();
        assert!(out.contents().contains("→ log --oneline -10"), "{}", out.contents());
    }

    #[test]
    fn catch_proxy_args_unbound_without_modifier() {
        // Prevents the catch→shell forwarding pattern that --scan flags as
        // HIGH risk from happening implicitly.
        let p = Program {
            catch: Some(Catch { bind: "name".into(), ops: vec![print_op("→ ${proxy_args}")], ..Default::default() }),
            ..Default::default()
        };
        let (i, _) = interp(p);
        let e = i.run("log", &sv(&["--oneline"])).unwrap_err();
        assert!(e.to_string().contains("proxy_args"), "{e}");
    }

    #[test]
    fn private_command_falls_to_catch() {
        let p = Program {
            commands: cmds(vec![Command {
                name: "hidden".into(),
                modifiers: Modifiers { private: true, ..Default::default() },
                ..Default::default()
            }]),
            catch: Some(Catch { bind: "name".into(), ops: vec![print_op("caught: ${name}")], ..Default::default() }),
            ..Default::default()
        };
        let (i, out) = interp(p);
        i.run("hidden", &[]).unwrap();
        assert!(out.contents().contains("caught: hidden"), "{}", out.contents());
    }

    #[test]
    fn block_op_runs_body() {
        let p = Program {
            globals: Globals {
                bindings: vec![GlobalBinding { name: "mode".into(), ty: "string".into(), value: json!("yes") }],
            },
            commands: cmds(vec![Command {
                name: "x".into(),
                ops: vec![Op {
                    kind: "if".into(),
                    args: args(json!({"lhs": "mode", "rhs": "yes"})),
                    body: vec![print_op("matched")],
                    ..Default::default()
                }],
                ..Default::default()
            }]),
            ..Default::default()
        };
        let (i, out) = interp(p);
        i.run("x", &[]).unwrap();
        assert!(out.contents().contains("matched"), "{}", out.contents());
    }

    #[test]
    fn command_not_found_and_unknown_op() {
        let (i, _) = interp(Program::default());
        assert_eq!(i.run("nope", &[]).unwrap_err().to_string(), "command not found: \"nope\"");
        let mut b = Bindings::new("/");
        let op = Op { kind: "zzz".into(), ..Default::default() };
        assert_eq!(i.run_op(&op, &mut b).unwrap_err().to_string(), "unknown op: \"zzz\"");
    }

    #[test]
    fn before_op_skip_and_quit() {
        let p = Program {
            commands: cmds(vec![Command {
                name: "x".into(),
                ops: vec![
                    Op {
                        kind: "upper".into(),
                        args: args(json!({"_0": "hi"})),
                        capture_into: "U".into(),
                        ..Default::default()
                    },
                    print_op("[${U}]"),
                ],
                ..Default::default()
            }]),
            ..Default::default()
        };
        let (i, out) = interp(p);
        i.set_before_op(Some(Arc::new(|_, _, _| OpAction::Skip)));
        i.run("x", &[]).unwrap();
        assert_eq!(out.contents(), ""); // print skipped too

        i.set_before_op(Some(Arc::new(|_, _, _| OpAction::Quit)));
        i.run("x", &[]).unwrap();
        assert!(out.contents().contains("↪ stopped by user"));
    }

    #[test]
    fn positional_and_typed_args() {
        let cmd = Command {
            name: "c".into(),
            args: vec![
                ArgSpec { name: "n".into(), ty: "int".into(), default: json!("5"), has_default: true, ..Default::default() },
                ArgSpec { name: "p".into(), ty: "string".into(), index: Some(0), ..Default::default() },
                ArgSpec { name: "rest".into(), ty: "string".into(), index: Some(1), rest: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let (i, _) = interp(Program::default());
        let m = i.parse_cli_args(&cmd, &sv(&["-n=7", "a", "b", "c"])).unwrap();
        assert_eq!(m["n"], json!(7));
        assert_eq!(m["p"], json!("a"));
        assert_eq!(m["rest"], json!("b\nc"));
        assert_eq!(m["rest_count"], json!(2));
        let m = i.parse_cli_args(&cmd, &sv(&["a"])).unwrap();
        assert_eq!(m["n"], json!(5));
        assert_eq!(
            i.parse_cli_args(&cmd, &sv(&[])).unwrap_err().to_string(),
            "missing positional argument #0 (p)"
        );
    }
}

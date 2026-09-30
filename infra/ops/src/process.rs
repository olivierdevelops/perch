//! Process ops: print/println/eprintln, shell family, exec / exec_chain / pipe,
//! fail / exit / sleep, run, list_commands, and process management.
use crate::common::{arg_string, look_path, truthy_value, go_io_msg};
use crate::requires::{check_exec_bin, check_shell_bin_declared, check_subprocess_bin, resolve_exec_path};
use perch_domain::{ErrorKind, Op, OpError};
use perch_interpreter::{
    err, find_op_error, go_os, go_quote, handler, interpolate_args, to_string_value, wrap, Args, Bindings, Error,
    Handler, Interpreter, Result, SharedReader, SharedWriter, StdKind,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;

pub fn register_process(m: &mut HashMap<String, Handler>) {
    let mut reg = |k: &str, h: Handler| {
        m.insert(k.to_string(), h);
    };
    reg("print", handler(op_print));
    reg("println", handler(op_println));
    reg("eprintln", handler(op_eprintln));
    reg("shell", handler(op_shell));
    reg("shell_output", handler(op_shell_output));
    reg("shell_detached", handler(op_shell_detached));
    reg("exec", handler(op_exec));
    reg("exec_chain", handler(op_exec_chain));
    reg("pipe", handler(op_pipe));
    reg("fail", handler(op_fail));
    reg("exit", handler(op_exit));
    reg("sleep", handler(op_sleep));
    reg("run", handler(op_run));
    reg("list_commands", handler(op_list_commands));
    reg("try_shell", handler(op_try_shell));
    reg("shell_in", handler(op_shell_in));
    reg("process_running", handler(op_process_running));
    reg("kill_by_name", handler(op_kill_by_name));
}

// ── subprocess plumbing (Go's exec.Cmd, minus the goroutines) ─────────────

/// Where a child's stdout/stderr goes.
#[derive(Clone, Copy)]
pub(crate) enum Dest<'a> {
    /// Stream into a shared writer (the real fd is inherited when it is the
    /// matching process stream, like Go passing an `*os.File` through).
    Writer(&'a SharedWriter),
    /// Collect into a buffer returned by `Running::wait`.
    Capture,
    /// Keep the pipe open for the caller (`pipe` stages).
    Pipe,
    /// /dev/null.
    Null,
}

/// Where a child's stdin comes from.
pub(crate) enum Src<'a> {
    Reader(&'a SharedReader),
    Stdio(Stdio),
    Null,
}

pub(crate) struct Running {
    pub child: Child,
    copiers: Vec<(bool, JoinHandle<Vec<u8>>)>,
}

/// How a failed subprocess ended (Go: `*exec.ExitError` vs other errors).
pub(crate) enum RunErr {
    Exit { code: i32, text: String },
    Other(String),
}

fn signal_name(sig: i32) -> String {
    match sig {
        1 => "hangup".into(),
        2 => "interrupt".into(),
        3 => "quit".into(),
        4 => "illegal instruction".into(),
        5 => "trace/BPT trap".into(),
        6 => "aborted".into(),
        7 => "bus error".into(),
        8 => "floating point exception".into(),
        9 => "killed".into(),
        10 => "user defined signal 1".into(),
        11 => "segmentation fault".into(),
        12 => "user defined signal 2".into(),
        13 => "broken pipe".into(),
        14 => "alarm clock".into(),
        15 => "terminated".into(),
        n => format!("signal {n}"),
    }
}

/// `ProcessState.String()` for a non-success status.
fn status_error(st: ExitStatus) -> Option<RunErr> {
    if st.success() {
        return None;
    }
    if let Some(code) = st.code() {
        return Some(RunErr::Exit { code, text: format!("exit status {code}") });
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = st.signal() {
            let mut text = format!("signal: {}", signal_name(sig));
            if st.core_dumped() {
                text.push_str(" (core dumped)");
            }
            return Some(RunErr::Exit { code: -1, text });
        }
    }
    Some(RunErr::Exit { code: -1, text: "exit status -1".into() })
}

/// Go-flavoured text for a spawn failure of `name`.
fn spawn_err_text(name: &str, e: &std::io::Error) -> String {
    format!("fork/exec {}: {}", name, go_io_msg(e))
}

/// Builds the Command for `bin` + `argv`: a bare name is resolved against the
/// parent's PATH first (Go's `exec.LookPath`), keeping argv[0] as typed.
fn build_command(bin: &str, argv: &[String]) -> std::result::Result<Command, RunErr> {
    let target: String = if bin.contains('/') || (cfg!(windows) && bin.contains('\\')) {
        bin.to_string()
    } else {
        match look_path(bin) {
            Some(p) => p.to_string_lossy().into_owned(),
            None => {
                return Err(RunErr::Other(format!("exec: {}: executable file not found in $PATH", go_quote(bin))));
            }
        }
    };
    let mut c = Command::new(&target);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.arg0(bin);
    }
    c.args(argv);
    Ok(c)
}

fn copy_stream<R: Read + Send + 'static>(mut r: R, sink: Option<SharedWriter>, capture: bool) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut cap: Vec<u8> = Vec::new();
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if capture {
                        cap.extend_from_slice(&buf[..n]);
                    }
                    if let Some(w) = &sink {
                        let _ = w.write_all(&buf[..n]);
                    }
                }
            }
        }
        cap
    })
}

/// Spawns `cmd` with the given stdio wiring. `is_stdout_stream` tells whether a
/// `Writer` destination may be inherited for stdout (`true`) or stderr.
pub(crate) fn spawn(
    cmd: &mut Command,
    stdin: Src<'_>,
    stdout: Dest<'_>,
    stderr: Dest<'_>,
) -> std::result::Result<Running, RunErr> {
    let name = cmd.get_program().to_string_lossy().into_owned();
    let mut stdin_thread: Option<(SharedReader, bool)> = None;
    match stdin {
        Src::Null => {
            cmd.stdin(Stdio::null());
        }
        Src::Stdio(s) => {
            cmd.stdin(s);
        }
        Src::Reader(r) => {
            if r.is_process_stdin() {
                cmd.stdin(Stdio::inherit());
            } else {
                cmd.stdin(Stdio::piped());
                stdin_thread = Some((r.clone(), true));
            }
        }
    }
    let dest_stdio = |d: Dest<'_>, mine: StdKind| -> (Stdio, bool) {
        match d {
            Dest::Null => (Stdio::null(), false),
            Dest::Writer(w) if w.std_kind() == mine => (Stdio::inherit(), false),
            Dest::Writer(_) | Dest::Capture | Dest::Pipe => (Stdio::piped(), true),
        }
    };
    let (so, so_piped) = dest_stdio(stdout, StdKind::Stdout);
    let (se, se_piped) = dest_stdio(stderr, StdKind::Stderr);
    cmd.stdout(so).stderr(se);

    let mut child = cmd.spawn().map_err(|e| RunErr::Other(spawn_err_text(&name, &e)))?;
    let mut copiers = Vec::new();
    if let Some((mut r, _)) = stdin_thread {
        if let Some(mut w) = child.stdin.take() {
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match r.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if w.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    }
    if so_piped {
        match stdout {
            Dest::Writer(w) => {
                if let Some(o) = child.stdout.take() {
                    copiers.push((true, copy_stream(o, Some(w.clone()), false)));
                }
            }
            Dest::Capture => {
                if let Some(o) = child.stdout.take() {
                    copiers.push((true, copy_stream(o, None, true)));
                }
            }
            _ => {}
        }
    }
    if se_piped {
        match stderr {
            Dest::Writer(w) => {
                if let Some(o) = child.stderr.take() {
                    copiers.push((false, copy_stream(o, Some(w.clone()), false)));
                }
            }
            Dest::Capture => {
                if let Some(o) = child.stderr.take() {
                    copiers.push((false, copy_stream(o, None, true)));
                }
            }
            _ => {}
        }
    }
    Ok(Running { child, copiers })
}

impl Running {
    /// Waits for exit and the stream copiers; returns the exit error (if any)
    /// and the captured stdout / stderr bytes.
    pub fn wait(mut self) -> (Option<RunErr>, Vec<u8>, Vec<u8>) {
        let st = self.child.wait();
        let mut out = Vec::new();
        let mut errb = Vec::new();
        for (is_out, h) in self.copiers {
            let v = h.join().unwrap_or_default();
            if is_out {
                out = v;
            } else {
                errb = v;
            }
        }
        let e = match st {
            Ok(st) => status_error(st),
            Err(e) => Some(RunErr::Other(go_io_msg(&e))),
        };
        (e, out, errb)
    }
}

/// `cmd.Run()`.
pub(crate) fn run_cmd(
    cmd: &mut Command,
    stdin: Src<'_>,
    stdout: Dest<'_>,
    stderr: Dest<'_>,
) -> (Option<RunErr>, Vec<u8>) {
    match spawn(cmd, stdin, stdout, stderr) {
        Ok(r) => {
            let (e, out, _) = r.wait();
            (e, out)
        }
        Err(e) => (Some(e), Vec::new()),
    }
}

/// Wraps a shell exec error with an appropriate ErrorKind so `match err.kind`
/// can discriminate. Exit-code != 0 -> shell_exit_nonzero; signal-killed ->
/// shell_signal_killed.
pub(crate) fn tag_shell_err(e: RunErr, cmd: &str) -> Error {
    match e {
        RunErr::Exit { code, text } => {
            let mut oe = OpError::new("shell", ErrorKind::ShellExitNonzero, &text);
            oe.code = code.to_string();
            oe.detail = cmd.to_string();
            // Signal-killed has ExitCode -1 on most platforms.
            if code == -1 {
                oe.kind = ErrorKind::ShellSignalKilled;
            }
            Box::new(oe)
        }
        RunErr::Other(msg) => Box::new(OpError::new("shell", ErrorKind::ShellExitNonzero, &msg).with_detail(cmd)),
    }
}

fn run_err_to_error(e: RunErr) -> Error {
    match e {
        RunErr::Exit { text, .. } => err(text),
        RunErr::Other(m) => err(m),
    }
}

/// `cmd.Dir = dir` (an empty dir means the process cwd, as in Go).
fn set_dir(cmd: &mut Command, dir: &str) {
    if !dir.is_empty() {
        cmd.current_dir(dir);
    }
}

/// Creates the command that runs `s` via the host shell.
pub(crate) fn build_shell(s: &str) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(s);
        c
    } else {
        let mut c = Command::new("bash");
        c.arg("-c").arg(s);
        c
    }
}

// ── env ───────────────────────────────────────────────────────────────────

/// The baseline operational env passed to every declared subprocess without
/// needing a `requires env` line — the OS plumbing tools need to run but which
/// carries no application secrets. App-specific or sensitive vars (DOCKER_HOST,
/// AWS_*, *_TOKEN, *_KEY) are NOT here: declare them with `requires env "NAME"`.
pub fn default_bin_env() -> &'static [&'static str] {
    if cfg!(windows) {
        &[
            "PATH", "PATHEXT", "SystemRoot", "SystemDrive", "windir", "COMSPEC", "TEMP", "TMP", "USERPROFILE",
            "HOMEDRIVE", "HOMEPATH", "APPDATA", "LOCALAPPDATA", "ProgramData", "ProgramFiles", "ProgramFiles(x86)",
            "NUMBER_OF_PROCESSORS", "OS", "USERNAME", "COMPUTERNAME",
        ]
    } else {
        &[
            "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "SHELL", "USER", "LOGNAME", "LANG", "LANGUAGE", "LC_ALL",
            "LC_CTYPE", "TERM", "TZ",
        ]
    }
}

/// Builds the subprocess environment from the manifest — NOT the full host
/// environment.
///
/// SECURITY (the subprocess `vars` escape): perch can't parse a declared bin's
/// arguments, so it can't tell what env a subprocess reads. The defense is to
/// scrub: a subprocess sees ONLY
///
///  - the default operational set (PATH, HOME, TMPDIR, LANG, …);
///  - host env vars the file DECLARED via `requires env "NAME"`;
///  - names the operator explicitly allowed via `perch --env A,B`;
///  - the program's own bindings (uppercase globals) and per-command `env`.
///
/// Everything else is dropped. Legacy inherit-all only remains for a Program
/// with `requirements.declared == false` AND no --env (a hand-built test prog).
/// Later entries override earlier ones.
pub fn build_env(i: &Interpreter, b: &Bindings) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    let declared = i.program.requirements.declared;
    if !declared && b.env_allowlist.is_none() {
        for (k, v) in std::env::vars_os() {
            env.push((k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()));
        }
    } else {
        let mut allowed: Vec<String> = default_bin_env().iter().map(|s| s.to_string()).collect();
        for e in &i.program.requirements.envs {
            allowed.push(e.name.clone());
        }
        if let Some(al) = &b.env_allowlist {
            allowed.extend(al.keys().cloned());
        }
        allowed.sort();
        allowed.dedup();
        for name in allowed {
            if let Some(v) = std::env::var_os(&name) {
                env.push((name, v.to_string_lossy().into_owned()));
            }
        }
    }
    // Globals are visible as env vars too — convention: any binding with an
    // uppercase first letter becomes an env var. These are values the user
    // explicitly declared in the .perch file (or via CLI flag), so they're
    // considered "in scope" regardless of the allowlist.
    let mut vars: Vec<(&String, &Value)> = b.vars.iter().collect();
    vars.sort_by(|a, c| a.0.cmp(c.0));
    for (k, v) in vars {
        if k.as_bytes().first().is_some_and(|c| c.is_ascii_uppercase()) {
            env.push((k.clone(), to_string_value(v)));
        }
    }
    let mut envs: Vec<(&String, &String)> = b.env.iter().collect();
    envs.sort_by(|a, c| a.0.cmp(c.0));
    for (k, v) in envs {
        env.push((k.clone(), v.clone()));
    }
    env
}

/// Applies [`build_env`] to a Command (the environment is replaced, not merged).
pub fn apply_env(i: &Interpreter, cmd: &mut Command, b: &Bindings) {
    cmd.env_clear();
    for (k, v) in build_env(i, b) {
        cmd.env(k, v);
    }
}

// ── ops ───────────────────────────────────────────────────────────────────

fn op_try_shell(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let cmd = arg_string(args, &["cmd", "_0"]);
    // Same allowlist / metachar checks as `shell`. A blocked call returns the
    // error rather than being silently false — the user explicitly restricted
    // the surface, and a silent false would lie about why.
    if let Err(e) = check_shell(i, &cmd) {
        return Err(Box::new(e));
    }
    let mut c = build_shell(&cmd);
    set_dir(&mut c, &b.cwd);
    apply_env(i, &mut c, b);
    let (e, _) = run_cmd(&mut c, Src::Null, Dest::Null, Dest::Null);
    Ok(Value::Bool(e.is_none()))
}

fn op_shell_in(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let dir = arg_string(args, &["dir", "_0"]);
    let cmd = arg_string(args, &["cmd", "_1"]);
    check_shell(i, &cmd)?;
    let mut c = build_shell(&cmd);
    set_dir(&mut c, if dir.is_empty() { &b.cwd } else { &dir });
    apply_env(i, &mut c, b);
    let (e, _) = run_cmd(&mut c, Src::Reader(&i.stdin), Dest::Writer(&i.stdout), Dest::Writer(&i.stderr));
    match e {
        Some(e) => Err(run_err_to_error(e)),
        None => Ok(Value::Null),
    }
}

fn op_process_running(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    if name.is_empty() {
        return Ok(Value::Bool(false));
    }
    let tool = if go_os() == "windows" { "tasklist" } else { "pgrep" };
    check_subprocess_bin(i, tool)?;
    if go_os() == "windows" {
        let out = Command::new("tasklist").arg("/FI").arg(format!("IMAGENAME eq {name}")).output();
        let text = out
            .map(|o| {
                let mut v = o.stdout;
                v.extend(o.stderr);
                String::from_utf8_lossy(&v).to_lowercase()
            })
            .unwrap_or_default();
        return Ok(Value::Bool(text.contains(&name.to_lowercase())));
    }
    let ok = Command::new("pgrep")
        .arg("-f")
        .arg(&name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    Ok(Value::Bool(ok))
}

/// Kills processes matching NAME. Best-effort; errors are swallowed so
/// re-running an uninstall after a partial cleanup is safe.
fn op_kill_by_name(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let name = arg_string(args, &["name", "_0"]);
    if name.is_empty() {
        return Ok(Value::Null);
    }
    let tool = if go_os() == "windows" { "taskkill" } else { "pkill" };
    check_subprocess_bin(i, tool)?;
    let mut c = if go_os() == "windows" {
        let mut c = Command::new("taskkill");
        c.arg("/F").arg("/IM").arg(&name);
        c
    } else {
        let mut c = Command::new("pkill");
        c.arg("-f").arg(&name);
        c
    };
    let _ = c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    Ok(Value::Null)
}

fn op_print(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let _ = i.stdout.write_str(&format!("{}\n", arg_string(args, &["msg", "_0"])));
    Ok(Value::Null)
}

fn op_println(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    op_print(i, b, args)
}

fn op_eprintln(i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let _ = i.stderr.write_str(&format!("{}\n", arg_string(args, &["msg", "_0"])));
    Ok(Value::Null)
}

/// Enforces the two user-side defenses against subprocess escape when shell is
/// allowed: a binary allowlist (first token must be permitted) and a metachar
/// filter (no pipes / redirects / && / ; / $(...) / backticks). Returns Ok when
/// the command passes both checks (or when neither is active).
pub fn check_shell(i: &Interpreter, raw: &str) -> std::result::Result<(), OpError> {
    // File-declared manifest enforcement (no-op unless `requires` declared).
    check_shell_bin_declared(i, raw)?;
    if i.no_shell_metachars {
        for ch in ["|", ">", "<", "&", ";", "`", "$("] {
            if raw.contains(ch) {
                return Err(OpError::new(
                    "shell",
                    ErrorKind::ShellMetacharsDenied,
                    &format!("shell metachar {} rejected by --no-shell-metachars", go_quote(ch)),
                )
                .with_detail(raw));
            }
        }
    }
    if let Some(allowed) = &i.allowed_shell_bins {
        let mut first = "";
        for f in raw.split_whitespace() {
            if f.contains('=') && !f.contains([' ', '\t']) {
                continue;
            }
            first = f;
            break;
        }
        if first.is_empty() {
            return Err(OpError::new("shell", ErrorKind::ShellBinNotAllowed, "empty command rejected by --allow-bin"));
        }
        let base = match first.rfind(['/', '\\']) {
            Some(idx) => &first[idx + 1..],
            None => first,
        };
        if !allowed.get(base).copied().unwrap_or(false) {
            let names: Vec<&str> = allowed.keys().map(|s| s.as_str()).collect();
            return Err(OpError::new(
                "shell",
                ErrorKind::ShellBinNotAllowed,
                &format!("binary {} is not in --allow-bin (allowed: {})", go_quote(base), names.join(", ")),
            )
            .with_detail(base));
        }
    }
    Ok(())
}

fn op_shell(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let raw = arg_string(args, &["cmd", "_0"]);
    check_shell(i, &raw)?;
    let mut cmd = build_shell(&raw);
    apply_env(i, &mut cmd, b);
    set_dir(&mut cmd, &b.cwd);
    let (e, _) = run_cmd(&mut cmd, Src::Reader(&i.stdin), Dest::Writer(&i.stdout), Dest::Writer(&i.stderr));
    match e {
        Some(e) => Err(tag_shell_err(e, &raw)),
        None => Ok(Value::Null),
    }
}

fn trim_nl(out: &[u8]) -> String {
    String::from_utf8_lossy(out).trim_end_matches('\n').to_string()
}

fn op_shell_output(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let raw = arg_string(args, &["cmd", "_0"]);
    check_shell(i, &raw)?;
    let mut cmd = build_shell(&raw);
    apply_env(i, &mut cmd, b);
    set_dir(&mut cmd, &b.cwd);
    let (e, out) = run_cmd(&mut cmd, Src::Null, Dest::Capture, Dest::Writer(&i.stderr));
    match e {
        Some(e) => Err(tag_shell_err(e, &raw)),
        None => Ok(Value::String(trim_nl(&out))),
    }
}

/// Collects argv slots `_0.._N` in order until the first gap.
fn collect_argv(args: &Map<String, Value>) -> Vec<String> {
    let mut argv = Vec::new();
    let mut n = 0;
    while let Some(v) = args.get(&format!("_{n}")) {
        argv.push(to_string_value(v));
        n += 1;
    }
    argv
}

fn display_of(bin: &str, argv: &[String]) -> String {
    if argv.is_empty() {
        bin.to_string()
    } else {
        format!("{} {}", bin, argv.join(" "))
    }
}

/// Runs a DECLARED binary directly — never through a shell. The first arg
/// ("bin") is the binary; "_0".."_N" are argv slots, each passed untouched (no
/// word-splitting, no glob, no metachar surface). This is the shell-free
/// subprocess primitive from docs/sandboxed-by-design.md §3.2.
///
/// Gating: identical to `shell`'s first-token check — when a `requires` block is
/// present, the bin must be a declared `bin "…"` or the op fails
/// bin_not_declared. The capability mask (`sandbox no_subprocess`) and any hash
/// pin still apply through the same paths shell uses.
///
/// stdout is tee'd to the program's stdout AND captured as the op's return
/// value, so a bare `exec git status` streams while `let h = exec git
/// "rev-parse" "HEAD"` captures.
fn op_exec(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    exec_with(i, b, args)
}

fn exec_with(i: &Interpreter, b: &Bindings, args: &Map<String, Value>) -> Result<Value> {
    let bin = arg_string(args, &["bin"]);
    if bin.is_empty() {
        return Err(Box::new(OpError::new("exec", ErrorKind::Unclassified, "exec: missing binary")));
    }
    // Capability + manifest gate. Reuses the same declared-bin enforcement as
    // `shell` (and the CapMask no_subprocess / allow-bin checks).
    check_exec_bin(i, &bin)?;
    let argv = collect_argv(args);
    let display = display_of(&bin, &argv);
    let runerr;
    let out;
    match build_command(&resolve_exec_path(i, &bin), &argv) {
        Ok(mut cmd) => {
            apply_env(i, &mut cmd, b);
            set_dir(&mut cmd, &b.cwd);
            // Capture stdout into a buffer (so `let x = exec …` works), then tee
            // the captured bytes to the program's stdout so a bare `exec …` streams.
            let (e, o) = run_cmd(&mut cmd, Src::Reader(&i.stdin), Dest::Capture, Dest::Writer(&i.stderr));
            runerr = e;
            out = o;
        }
        Err(e) => {
            runerr = Some(e);
            out = Vec::new();
        }
    }
    // Tee stdout to the program's stdout for a bare `exec …`, but stay quiet when
    // the result is bound (`let x = exec …`) — matching the old shell (stream) vs
    // shell_output (silent capture) split.
    if !capture_requested(args) {
        let _ = i.stdout.write_all(&out);
    }
    match runerr {
        Some(e) => Err(tag_shell_err(e, &display)),
        None => Ok(Value::String(trim_nl(&out))),
    }
}

fn capture_requested(args: &Map<String, Value>) -> bool {
    truthy_value(args.get("_capture").unwrap_or(&Value::Null))
}

/// Runs `exec a && exec b || exec c ; exec d` — a chain of exec clauses joined
/// by perch-level operators (NOT shell metachars; they're literal source tokens
/// the loader folded into Body + Args["ops"], so an interpolated ${x} can never
/// become an operator — the §3.3 keystone).
///
/// Each clause is a child exec op in Body. Operators drive short-circuit
/// evaluation on the previous clause's exit status:
///
///   &&  run next only if the previous clause SUCCEEDED
///   ||  run next only if the previous clause FAILED
///   ;   always run next
///
/// The chain's result is the error of the LAST actually-run clause (so a bare
/// chain aborts on a real failure; wrap in `try` or use `||` to continue).
fn op_exec_chain(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let body = args.body;
    let ops = to_string_slice(args.get("ops"));
    if body.is_empty() {
        return Ok(Value::Null);
    }
    let run_clause = |op: &Op, b: &Bindings| -> Result<()> {
        let a = interpolate_args(&op.args, b)?;
        exec_with(i, b, &a).map(|_| ())
    };
    let mut last_err = run_clause(&body[0], b).err();
    for (k, clause) in body.iter().enumerate().skip(1) {
        let op = ops.get(k - 1).map(|s| s.as_str()).unwrap_or("");
        let run = match op {
            "&&" => last_err.is_none(),
            "||" => last_err.is_some(),
            _ => true, // ";"
        };
        if run {
            last_err = run_clause(clause, b).err();
        }
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(Value::Null),
    }
}

fn to_string_slice(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(xs)) => xs.iter().map(to_string_value).collect(),
        _ => Vec::new(),
    }
}

/// Runs a `pipe ... end` block: each body stage is an `exec BIN …`, and perch
/// wires stage N's stdout into stage N+1's stdin with in-process OS pipes — no
/// shell, no `sh -c`. The block's value is the final stage's stdout (captured
/// for `let out = pipe … end`) and is also streamed. docs/sandboxed-by-design.md
/// §3.5. Each stage's bin is gated exactly like a standalone `exec`.
fn op_pipe(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    struct Stage {
        cmd: Result<Command>,
        display: String,
    }
    let mut stages: Vec<Stage> = Vec::new();
    for op in args.body {
        if op.kind != "exec" {
            return Err(Box::new(OpError::new(
                "pipe",
                ErrorKind::Unclassified,
                &format!("pipe: every stage must be `exec`, got {}", go_quote(&op.kind)),
            )));
        }
        let a = interpolate_args(&op.args, b)?;
        let bin = arg_string(&a, &["bin"]);
        if bin.is_empty() {
            return Err(Box::new(OpError::new("pipe", ErrorKind::Unclassified, "pipe: exec stage missing binary")));
        }
        check_exec_bin(i, &bin)?;
        let argv = collect_argv(&a);
        let display = display_of(&bin, &argv);
        let cmd = build_command(&resolve_exec_path(i, &bin), &argv).map_err(run_err_to_error).map(|mut c| {
            apply_env(i, &mut c, b);
            set_dir(&mut c, &b.cwd);
            c
        });
        stages.push(Stage { cmd, display });
    }
    if stages.is_empty() {
        return Ok(Value::String(String::new()));
    }
    let all_display: Vec<String> = stages.iter().map(|s| s.display.clone()).collect();
    let n = stages.len();
    let mut running: Vec<Running> = Vec::new();
    let mut prev_out: Option<Stdio> = None;
    for (k, st) in stages.into_iter().enumerate() {
        let mut cmd = match st.cmd {
            Ok(c) => c,
            Err(e) => return Err(tag_shell_err(RunErr::Other(e.to_string()), &all_display.join(" | "))),
        };
        let src = match prev_out.take() {
            Some(s) => Src::Stdio(s),
            None => Src::Reader(&i.stdin),
        };
        let out_dest = if k == n - 1 { Dest::Capture } else { Dest::Pipe };
        // All stages share the program's stderr; the shared writer serializes
        // their copy threads.
        match spawn(&mut cmd, src, out_dest, Dest::Writer(&i.stderr)) {
            Ok(mut r) => {
                if k != n - 1 {
                    prev_out = r.child.stdout.take().map(Stdio::from);
                }
                running.push(r);
            }
            Err(e) => return Err(tag_shell_err(e, &all_display.join(" | "))),
        }
    }
    let mut first_err: Option<Error> = None;
    let mut out: Vec<u8> = Vec::new();
    let last = running.len() - 1;
    for (k, r) in running.into_iter().enumerate() {
        let (e, o, _) = r.wait();
        if k == last {
            out = o;
        }
        if let Some(e) = e {
            if first_err.is_none() {
                first_err = Some(tag_shell_err(e, &all_display[k]));
            }
        }
    }
    if !capture_requested(args) {
        let _ = i.stdout.write_all(&out);
    }
    let res = Value::String(trim_nl(&out));
    match first_err {
        Some(e) => Err(e),
        None => Ok(res),
    }
}

fn op_shell_detached(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let raw = arg_string(args, &["cmd", "_0"]);
    check_shell(i, &raw)?;
    let mut cmd = build_shell(&raw);
    apply_env(i, &mut cmd, b);
    set_dir(&mut cmd, &b.cwd);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.spawn() {
        Ok(_) => Ok(Value::Null),
        Err(e) => Err(err(spawn_err_text("bash", &e))),
    }
}

fn op_fail(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let mut msg = arg_string(args, &["msg", "_0"]);
    if msg.is_empty() {
        msg = "(no message)".to_string();
    }
    Err(Box::new(OpError::new("fail", ErrorKind::UserFail, &msg)))
}

fn op_exit(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let mut code: i64 = 0;
    if let Some(v) = args.get("code") {
        match v {
            Value::Number(n) => {
                code = n.as_i64().unwrap_or_else(|| n.as_f64().map(|f| f as i64).unwrap_or(0));
            }
            Value::String(s) => code = s.parse::<i64>().unwrap_or(0),
            _ => {}
        }
    }
    std::process::exit(code as i32);
}

fn op_sleep(_i: &Interpreter, _b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let mut secs = 0.0f64;
    if let Some(v) = args.get("seconds") {
        match v {
            Value::Number(n) => secs = n.as_f64().unwrap_or(0.0),
            Value::String(s) => secs = s.parse::<f64>().unwrap_or(0.0),
            _ => {}
        }
    }
    if secs.is_finite() && secs > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(secs));
    }
    Ok(Value::Null)
}

fn op_run(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let target = arg_string(args, &["target"]);
    if target.is_empty() {
        return Err(err("run: missing target"));
    }
    let Some(cmd) = i.program.commands.get(&target) else {
        return Err(err(format!("run: unknown command {}", go_quote(&target))));
    };
    // Collect trailing arg tokens (_0, _1, …) emitted by the run_Nargs grammar
    // overloads. These are CLI-style — `-name=value`, `-name value`,
    // `--name=value` — and feed the same parse_cli_args used by `perch NAME
    // -arg=value` from a shell. Up to 8 args.
    let mut cli_args: Vec<String> = Vec::new();
    for n in 0..8 {
        let Some(v) = args.get(&format!("_{n}")) else { break };
        let s = to_string_value(v);
        if s.is_empty() {
            break;
        }
        cli_args.push(s);
    }
    if !cli_args.is_empty() {
        let parsed = i.parse_cli_args(cmd, &cli_args).map_err(|e| wrap(format!("run {target}"), e))?;
        // Overlay parsed args onto current bindings — caller's `let` captures
        // stay visible, the target's declared args get their CLI-parsed values.
        for (k, v) in parsed {
            b.set(&k, v);
        }
    }
    i.run_ops(&cmd.ops, b)?;
    Ok(Value::Null)
}

fn op_list_commands(i: &Interpreter, _b: &mut Bindings, _args: &Args<'_>) -> Result<Value> {
    for (name, c) in &i.program.commands {
        if c.modifiers.private {
            continue;
        }
        if c.description.is_empty() {
            let _ = i.stdout.write_str(&format!("  {name}\n"));
        } else {
            let _ = i.stdout.write_str(&format!("  {:<20} {}\n", name, c.description));
        }
    }
    Ok(Value::Null)
}

/// Whether `e` wraps an OpError of the given kind (used by tests).
#[allow(dead_code)]
pub(crate) fn has_kind(e: &Error, k: ErrorKind) -> bool {
    find_op_error(&**e).is_some_and(|oe| oe.kind == k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::{EnvReq, Program, Requirements};
    use std::sync::Mutex;

    /// Serializes tests that mutate process env vars.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn interp(prog: Program) -> Interpreter {
        Interpreter::new(HashMap::new(), prog)
    }

    fn envs_string(env: &[(String, String)]) -> String {
        env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n")
    }

    // A declared subprocess must NOT inherit the full host environment: only the
    // default operational set (PATH, …), env vars declared via `requires env`,
    // and the program's own bindings reach it. An undeclared secret is scrubbed.
    #[test]
    fn apply_env_scrubs_undeclared_secrets() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("PERCH_TEST_SECRET", "shhh");
        std::env::set_var("PERCH_TEST_DECLARED", "okval");
        let prog = Program {
            requirements: Requirements {
                declared: true,
                envs: vec![EnvReq { name: "PERCH_TEST_DECLARED".into(), optional: false }],
                ..Default::default()
            },
            ..Default::default()
        };
        let i = interp(prog);
        let mut b = Bindings::new("");
        b.set("BUILD_DIR", "./out"); // uppercase binding -> exported
        let env = envs_string(&build_env(&i, &b));
        assert!(!env.contains("PERCH_TEST_SECRET"), "undeclared secret leaked:\n{env}");
        assert!(env.contains("PERCH_TEST_DECLARED=okval"), "declared var missing:\n{env}");
        assert!(env.contains("BUILD_DIR=./out"), "uppercase binding not exported:\n{env}");
        // The default operational set must still pass so tools can run.
        assert!(env.contains("PATH="), "PATH missing:\n{env}");

        // And the Command itself carries exactly that environment.
        let mut c = Command::new("true");
        apply_env(&i, &mut c, &b);
        let got: Vec<String> = c
            .get_envs()
            .map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.map(|v| v.to_string_lossy().into_owned()).unwrap_or_default()))
            .collect();
        assert!(got.iter().any(|e| e == "PERCH_TEST_DECLARED=okval"));
        assert!(!got.iter().any(|e| e.starts_with("PERCH_TEST_SECRET")));
    }

    // With no requires manifest AND no --env, legacy inherit-all is preserved.
    #[test]
    fn apply_env_legacy_inherit_when_undeclared() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("PERCH_TEST_AMBIENT", "visible");
        let prog = Program { requirements: Requirements { declared: false, ..Default::default() }, ..Default::default() };
        let i = interp(prog);
        let b = Bindings::new("");
        assert!(envs_string(&build_env(&i, &b)).contains("PERCH_TEST_AMBIENT=visible"));
    }
}

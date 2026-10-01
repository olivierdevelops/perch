//! [`Runtime`]: load perch programs and run their commands in-process under a
//! [`Policy`], getting a structured [`RunResult`] back.
//!
//! A `Runtime` owns its handler registry and policy and every `run` builds a
//! fresh interpreter, so runtimes never share mutable state; any number can
//! live in one process and run on different threads. The only process-wide
//! state underneath is immutable once initialised: the compiled capy library,
//! the op-vocabulary tables, the wasm engine, the TLS config and the
//! kernel-confinement support probe (all `OnceLock` caches). The embedded
//! bundle global (`perch_ops::set_bundle`) is only for fat binaries and is
//! never touched here.
use crate::policy::{Input, Output, Policy};
use perch_domain::Program;
use perch_interpreter::{
    find_op_error, is_quit, is_timeout, Handler, Interpreter, SharedBuf, SharedReader, SharedWriter,
};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use perch_scan::JsonReport;
pub use perch_validate::Issue;

/// Why a program could not be loaded (parse, import or static-enforcement
/// failure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    /// The loader's message.
    pub message: String,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for LoadError {}

/// One declared argument of a command.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgInfo {
    /// Argument name.
    pub name: String,
    /// `"string"`, `"int"`, `"float"` or `"bool"`.
    pub ty: String,
    /// Declared description.
    pub description: String,
    /// True when omitting it is fine (optional or has a default).
    pub optional: bool,
}

/// One runnable command (private and test commands are not listed).
#[derive(Debug, Clone, PartialEq)]
pub struct CommandInfo {
    /// Command name.
    pub name: String,
    /// Declared description.
    pub description: String,
    /// Declared arguments in order.
    pub args: Vec<ArgInfo>,
}

/// A loaded program, ready for [`Runtime::run`], [`Loaded::check`] and
/// [`Loaded::scan`]. Cheap to clone and safe to share between threads.
#[derive(Debug, Clone)]
pub struct Loaded {
    program: Arc<Program>,
    source: Option<PathBuf>,
    known_ops: Arc<HashSet<String>>,
}

impl Loaded {
    /// The parsed program.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// The file it was loaded from (`None` for [`Runtime::load_str`]).
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// The commands a host can run, sorted by name.
    pub fn commands(&self) -> Vec<CommandInfo> {
        self.program
            .commands
            .values()
            .filter(|c| !(c.modifiers.private || c.modifiers.test))
            .map(|c| CommandInfo {
                name: c.name.clone(),
                description: c.description.clone(),
                args: c
                    .args
                    .iter()
                    .map(|a| ArgInfo {
                        name: a.name.clone(),
                        ty: a.ty.clone(),
                        description: a.description.clone(),
                        optional: a.optional || a.has_default,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Static validation (what `perch --check` reports): every finding, errors
    /// and warnings. An empty list means clean.
    pub fn check(&self) -> Vec<Issue> {
        perch_validate::check(&self.program, &self.known_ops)
    }

    /// The structured capability scan (what `perch --scan --json` prints).
    pub fn scan(&self) -> JsonReport {
        let r = perch_scan::analyze(&self.program);
        let path = self.source.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "<string>".into());
        perch_scan::build_json_report(&self.program, &path, &r)
    }
}

/// A classified failure from [`Runtime::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunError {
    /// The `ErrorKind` identifier (`cap_shell_denied`, `timeout_exceeded`,
    /// `shell_exit_nonzero`, ...) or `"unclassified"`.
    pub kind: String,
    /// Human-readable message.
    pub message: String,
    /// The op that failed, when known.
    pub op: String,
    /// Exit code / HTTP status, when the op has one.
    pub code: String,
    /// Extra context (the shell command, URL, ...).
    pub detail: String,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for RunError {}

/// The outcome of one [`Runtime::run`].
#[derive(Debug, Clone)]
pub struct RunResult {
    /// True when the command finished without error.
    pub ok: bool,
    /// Captured stdout (empty unless the policy uses [`Output::Capture`]).
    pub stdout: String,
    /// Captured stderr (empty unless the policy uses [`Output::Capture`]).
    pub stderr: String,
    /// The failure, when `ok` is false.
    pub error: Option<RunError>,
    /// Wall-clock time of the run.
    pub duration: Duration,
}

/// A policy plus the op registry built from it. See the crate docs.
pub struct Runtime {
    policy: Policy,
    handlers: HashMap<String, Handler>,
    known_ops: Arc<HashSet<String>>,
}

fn load_err(e: perch_capyloader::Error) -> LoadError {
    LoadError { message: e.to_string() }
}

fn run_error(e: &perch_interpreter::Error) -> RunError {
    if let Some(oe) = find_op_error(e.as_ref()) {
        return RunError {
            kind: oe.kind.as_str().to_string(),
            message: oe.message.clone(),
            op: oe.op.clone(),
            code: oe.code.clone(),
            detail: oe.detail.clone(),
        };
    }
    // Capability denial: the restriction sentinel in perch-ops returns a plain
    // error ("op \"X\" is disabled by --no-Y"), not an OpError, because the CLI
    // output for it is a frozen contract. Recognise it here and report the
    // matching cap_* kind.
    if let Some(r) = capability_denial(&e.to_string()) {
        return r;
    }
    // The wall-clock timeout is a bare sentinel, not an OpError.
    let kind = if is_timeout(e) { "timeout_exceeded" } else { "unclassified" };
    RunError { kind: kind.into(), message: e.to_string(), op: String::new(), code: String::new(), detail: String::new() }
}

fn capability_denial(msg: &str) -> Option<RunError> {
    let at = msg.find(" is disabled by --no-")?;
    let flag = msg[at + " is disabled by --no-".len()..].split(|c: char| !c.is_ascii_lowercase()).next()?;
    let kind = match flag {
        "shell" => "cap_shell_denied",
        "subprocess" => "cap_subprocess_denied",
        "network" => "cap_network_denied",
        "write" => "cap_write_denied",
        _ => return None,
    };
    let op = msg[..at].rsplit('"').nth(1).unwrap_or("").to_string();
    Some(RunError { kind: kind.into(), message: msg.to_string(), op, code: String::new(), detail: String::new() })
}

impl Runtime {
    /// Builds a runtime that enforces `policy`.
    pub fn new(policy: Policy) -> Runtime {
        let handlers = policy.handlers();
        let known_ops = Arc::new(handlers.keys().cloned().collect());
        Runtime { policy, handlers, known_ops }
    }

    /// The policy this runtime enforces.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    fn wrap(&self, program: Program, source: Option<PathBuf>) -> Loaded {
        Loaded { program: Arc::new(program), source, known_ops: self.known_ops.clone() }
    }

    /// Loads a `.perch` file (imports resolved relative to it).
    pub fn load_path(&self, path: &Path) -> Result<Loaded, LoadError> {
        let program = perch_capyloader::load(&path.to_string_lossy()).map_err(load_err)?;
        Ok(self.wrap(program, Some(path.to_path_buf())))
    }

    /// Loads a program from source text. Any `import` in it resolves against
    /// the host process's cwd.
    pub fn load_str(&self, src: &str) -> Result<Loaded, LoadError> {
        let program = perch_capyloader::load_from_string(src).map_err(load_err)?;
        Ok(self.wrap(program, None))
    }

    /// Runs `command` with `args` under this runtime's policy. Never panics
    /// the host: an interpreter panic is reported as an `unclassified` error.
    pub fn run(&self, loaded: &Loaded, command: &str, args: &[String]) -> RunResult {
        let start = Instant::now();
        let out = SharedBuf::new();
        let err = SharedBuf::new();
        let sink = |o: Output, buf: &SharedBuf, real: fn() -> SharedWriter| match o {
            Output::Capture => buf.writer(),
            Output::Inherit => real(),
            Output::Discard => SharedWriter::discard(),
        };

        let p = &self.policy;
        let mut i = Interpreter::new(self.handlers.clone(), loaded.program.clone());
        p.configure(&mut i, &None);
        i.stdout = sink(p.stdout, &out, SharedWriter::stdout);
        i.stderr = sink(p.stderr, &err, SharedWriter::stderr);
        i.stdin = match &p.stdin {
            Input::Empty => SharedReader::empty(),
            Input::Inherit => SharedReader::stdin(),
            Input::Bytes(b) => SharedReader::new(Box::new(Cursor::new(b.clone()))),
        };
        if let Some(d) = p.max_runtime {
            i.set_deadline(Some(start + d));
        }

        let runnable = loaded.program.commands.get(command).is_some_and(|c| !(c.modifiers.private || c.modifiers.test));
        let outcome = if !runnable && loaded.program.catch.is_none() {
            Err(RunError {
                kind: "command_not_found".into(),
                message: format!("command not found: {command:?}"),
                op: String::new(),
                code: String::new(),
                detail: command.to_string(),
            })
        } else {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| i.run(command, args))) {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) if is_quit(&e) => Ok(()),
                Ok(Err(e)) => Err(run_error(&e)),
                Err(_) => Err(RunError {
                    kind: "unclassified".into(),
                    message: "interpreter panicked".into(),
                    op: String::new(),
                    code: String::new(),
                    detail: String::new(),
                }),
            }
        };
        RunResult {
            ok: outcome.is_ok(),
            stdout: out.contents(),
            stderr: err.contents(),
            error: outcome.err(),
            duration: start.elapsed(),
        }
    }
}

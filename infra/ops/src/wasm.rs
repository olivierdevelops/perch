//! wasm_run and the wasm_* marker ops (wasm.go / wasm_http.go): WebAssembly
//! execution via wasmtime — the constrained execution lane.
//!
//! Where `shell` is the universal escape hatch, `wasm_run` is hard isolation:
//! the module sees only what the body's `wasm_mount_*` / `wasm_env` /
//! `wasm_arg` / `wasm_allow_host` declarations grant. The marker ops have
//! handlers that only error ("outside a wasm_run block"); `wasm_run` reads
//! them straight from its body and never dispatches them.
//!
//! Module sources: a string path (host file) or a bare bundle alias
//! (`_alias: true`, resolved through `bundle ... include "x" as alias`).
//! Compiled modules are also cached on disk (see `wasm_cache.rs`, F02) and
//! cached in-process (keyed by content sha256 or by
//! `bundle:<hash>:<entry>`) so a `parallel` block reuses them.
//!
//! Mounts: read-only dirs at `/ro/<basename>`, read-write at `/rw/<basename>`.
#[path = "wasm_cache.rs"]
mod wasm_cache;
#[path = "wasm_http.rs"]
mod wasm_http;
#[path = "wasm_io.rs"]
mod wasm_io;

use crate::common::{arg_string, go_base, path_err, resolve};
use crate::group_b::{bundle_hash, bundle_read_file};
use crate::requires::{check_host_declared, check_path_declared};
use perch_domain::{ErrorKind, OpError, Program};
use perch_interpreter::{
    err, go_quote, handler, interpolate_args, Args, Bindings, CapMask, Error, Handler, Interpreter, Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use wasmtime::{Config, Engine, Linker, Module, Store, UpdateDeadline};
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::{DirPerms, FilePerms, I32Exit, WasiCtxBuilder};

/// Per-instantiation store data: the WASI context plus the HTTP bridge state.
pub struct HostState {
    wasi: WasiP1Ctx,
    http: wasm_http::CallState,
}

/// Shared runtime: engine, linker (WASI p1 + the `perch` host module) and the
/// compiled-module cache.
struct Runtime {
    engine: Engine,
    linker: Linker<HostState>,
    compiled: Mutex<HashMap<String, Module>>,
}

static RUNTIME: OnceLock<std::result::Result<Runtime, String>> = OnceLock::new();

fn runtime() -> std::result::Result<&'static Runtime, String> {
    RUNTIME
        .get_or_init(|| {
            let mut cfg = Config::new();
            cfg.epoch_interruption(true);
            let engine = Engine::new(&cfg).map_err(|e| format!("wasm engine: {e}"))?;
            let mut linker: Linker<HostState> = Linker::new(&engine);
            wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |s: &mut HostState| &mut s.wasi)
                .map_err(|e| format!("install wasi: {e}"))?;
            wasm_http::install_perch_host_module(&mut linker).map_err(|e| format!("install perch host module: {e}"))?;
            Ok(Runtime { engine, linker, compiled: Mutex::new(HashMap::new()) })
        })
        .as_ref()
        .map_err(|e| e.clone())
}

pub fn register_wasm(m: &mut HashMap<String, Handler>) {
    m.insert("wasm_run".into(), handler(op_wasm_run));
    for name in ["wasm_arg", "wasm_mount_read", "wasm_mount_write", "wasm_env", "wasm_allow_host"] {
        m.insert(name.into(), marker_err(name));
    }
}

/// Typed wasm failure (F04): `kind: message` when displayed.
fn wasm_err(op: &str, kind: ErrorKind, msg: impl Into<String>) -> Error {
    Box::new(OpError::new(op, kind, &msg.into()))
}

/// F03: refuse an operation with `wasm_capability_denied`, carrying the gate's
/// own message and the offending value as detail.
fn denied(op: &str, msg: impl Into<String>, detail: &str) -> Error {
    Box::new(OpError::new(op, ErrorKind::WasmCapabilityDenied, &msg.into()).with_detail(detail))
}

fn marker_err(name: &'static str) -> Handler {
    handler(move |_i, _b, _a| Err(err(format!("{name} is only valid inside a wasm_run or wasm_bundle block"))))
}

#[derive(Default)]
struct WasmConfig {
    argv: Vec<String>,
    mounts_ro: Vec<String>,
    mounts_rw: Vec<String>,
    env_allow: Vec<String>,
    allow_host: Vec<String>,
}

fn op_wasm_run(i: &Interpreter, b: &mut Bindings, args: &Args<'_>) -> Result<Value> {
    let mut raw_path = arg_string(args, &["path", "_0"]);
    if raw_path.is_empty() {
        return Err(err("wasm_run: missing module path"));
    }
    let mut module_path = String::new();
    let mut module_bytes: Option<Vec<u8>> = None;
    let mut cache_key = String::new();

    let is_alias = args.get("_alias").and_then(Value::as_bool).unwrap_or(false);
    if is_alias {
        let Some(entry) = lookup_bundle_alias(&i.program, &raw_path) else {
            return Err(wasm_err("wasm_run", ErrorKind::WasmCompileFailed, format!(
                "wasm_run: {} is not a declared bundle alias (add `include \"…\" as {raw_path}` to your `bundle ... end` section)",
                go_quote(&raw_path)
            )));
        };
        match bundle_read_file(&entry) {
            Some(Ok(buf)) => module_bytes = Some(buf),
            Some(Err(e)) => return Err(wasm_err("wasm_run", ErrorKind::WasmCompileFailed, format!("wasm_run {raw_path}: {e}"))),
            None => {
                return Err(wasm_err("wasm_run", ErrorKind::WasmCompileFailed, format!(
                    "wasm_run {raw_path}: alias resolves to {} but this binary has no embedded bundle (build with `perch --build`)",
                    go_quote(&entry)
                )))
            }
        }
        cache_key = format!("bundle:{}:{entry}", bundle_hash());
        raw_path = entry; // for argv0 / error messages
    } else {
        module_path = resolve(&raw_path, b);
        if let Err(e) = std::fs::metadata(&module_path) {
            return Err(wasm_err("wasm_run", ErrorKind::WasmCompileFailed, format!(
                "wasm_run: module {}: {}",
                go_quote(&raw_path),
                path_err("stat", &module_path, &e)
            )));
        }
    }
    run_wasm_module(i, b, args, &raw_path, "wasm_run", &module_path, module_bytes, &cache_key)
}

fn lookup_bundle_alias(p: &Program, name: &str) -> Option<String> {
    p.bundle.aliases.iter().find(|a| a.name == name).map(|a| a.entry.clone())
}

#[allow(clippy::too_many_arguments)]
fn run_wasm_module(
    i: &Interpreter,
    b: &mut Bindings,
    args: &Args<'_>,
    raw_path: &str,
    op_name: &str,
    module_path: &str,
    module_bytes: Option<Vec<u8>>,
    cache_key: &str,
) -> Result<Value> {
    // Walk the body for capability declarations. Nothing else may appear.
    let mut cfg = WasmConfig::default();
    for op in args.body {
        let op_args = interpolate_args(&op.args, b)
            .map_err(|e| err(format!("{op_name}: interpolating {}: {e}", op.kind)))?;
        match op.kind.as_str() {
            "wasm_arg" => cfg.argv.push(arg_string(&op_args, &["value", "_0"])),
            "wasm_mount_read" => {
                let p = resolve(&arg_string(&op_args, &["path", "_0"]), b);
                check_mount(i, b, op_name, "wasm_mount_read", &p, false)?;
                cfg.mounts_ro.push(p);
            }
            "wasm_mount_write" => {
                let p = resolve(&arg_string(&op_args, &["path", "_0"]), b);
                check_mount(i, b, op_name, "wasm_mount_write", &p, true)?;
                cfg.mounts_rw.push(p);
            }
            "wasm_env" => {
                for n in arg_string(&op_args, &["names", "_0"]).split(',') {
                    let n = n.trim();
                    if !n.is_empty() {
                        cfg.env_allow.push(n.to_string());
                    }
                }
            }
            // Per-module HTTP host allowlist; composes AND-wise with the outer
            // --allow-host policy.
            "wasm_allow_host" => {
                let h = arg_string(&op_args, &["host", "_0"]);
                check_allow_host(i, b, op_name, &h)?;
                cfg.allow_host.push(h);
            }
            other => {
                return Err(err(format!(
                    "{op_name}: {} is not valid inside a {op_name} block (only wasm_arg / wasm_mount_read / wasm_mount_write / wasm_env / wasm_allow_host)",
                    go_quote(other)
                )))
            }
        }
    }

    let rt = runtime().map_err(err)?;
    let compiled = match module_bytes {
        Some(bytes) => compile_bytes(rt, cache_key, &bytes),
        None => compile_file(rt, module_path),
    }
    .map_err(|e| wasm_err(op_name, ErrorKind::WasmCompileFailed, format!("{op_name}: compile {}: {e}", go_quote(raw_path))))?;

    let fail = |e: String| wasm_err(op_name, ErrorKind::WasmCompileFailed, format!("{op_name} {}: {e}", go_quote(raw_path)));
    let exited = |e: String| wasm_err(op_name, ErrorKind::WasmModuleExited, format!("{op_name} {}: {e}", go_quote(raw_path)));

    // WASI configured with EXACTLY the declared capabilities.
    let argv0 = go_base(if module_path.is_empty() { raw_path } else { module_path });
    let mut wb = WasiCtxBuilder::new();
    wb.stdout(wasm_io::Sink(i.stdout.clone()))
        .stderr(wasm_io::Sink(i.stderr.clone()))
        .stdin(wasm_io::Source(i.stdin.clone()));
    let mut argv = vec![argv0];
    argv.extend(cfg.argv.iter().cloned());
    wb.args(&argv);
    // Env allowlist: only declared names, only real values (b.lookup honors
    // --env restrictions, so the intersection reaches WASI).
    for name in &cfg.env_allow {
        if let Some(val) = b.lookup(name) {
            wb.env(name, val);
        }
    }
    for p in &cfg.mounts_ro {
        wb.preopened_dir(p, format!("/ro/{}", go_base(p)), DirPerms::READ, FilePerms::READ)
            .map_err(|e| fail(e.to_string()))?;
    }
    for p in &cfg.mounts_rw {
        wb.preopened_dir(p, format!("/rw/{}", go_base(p)), DirPerms::all(), FilePerms::all())
            .map_err(|e| fail(e.to_string()))?;
    }
    let host = HostState { wasi: wb.build_p1(), http: wasm_http::build_call_state(i, &cfg.allow_host) };
    let mut store = Store::new(&rt.engine, host);

    // Honor the interpreter's wall-clock deadline (a `timeout` block or
    // --max-runtime): a ticker bumps the shared engine epoch and this store's
    // callback traps once its own deadline has passed.
    store.set_epoch_deadline(u64::MAX / 2); // no deadline: effectively never
    let _ticker = i.deadline().map(|dl| {
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(move |_| {
            if Instant::now() >= dl {
                Err(wasmtime::Error::msg(DEADLINE_MSG))
            } else {
                Ok(UpdateDeadline::Continue(1))
            }
        });
        Ticker::start(rt.engine.clone())
    });

    let instance = match rt.linker.instantiate(&mut store, &compiled) {
        Ok(inst) => inst,
        Err(e) => return Err(fail(e.root_cause().to_string())),
    };
    let outcome = (|| -> wasmtime::Result<()> {
        if let Ok(start) = instance.get_typed_func::<(), ()>(&mut store, "_start") {
            start.call(&mut store, ())?;
        }
        Ok(())
    })();
    let refusal = store.data().http.refusal();
    // A failure after the host refused a module HTTP call is reported as
    // wasm_http_refused (the module itself only ever saw -1); the refusal
    // reason rides in `detail`.
    let exited = |msg: String| match &refusal {
        Some(reason) => {
            Box::new(OpError::new(op_name, ErrorKind::WasmHTTPRefused, &format!("{op_name} {}: {msg}", go_quote(raw_path))).with_detail(reason.clone()))
                as Error
        }
        None => exited(msg),
    };
    match outcome {
        Ok(()) => Ok(Value::Null),
        Err(e) => {
            // WASI exit(): code 0 is success.
            if let Some(x) = e.downcast_ref::<I32Exit>() {
                if x.0 == 0 {
                    return Ok(Value::Null);
                }
                return Err(exited(format!("module closed with exit_code({})", x.0)));
            }
            let root = e.root_cause().to_string();
            if root == DEADLINE_MSG {
                return Err(exited("module closed with context deadline exceeded".into()));
            }
            Err(exited(root))
        }
    }
}

/// F03: a mount path must sit inside a declared `read`/`write` root of the
/// program's `requires` block (same check the file ops use; no block = no
/// gate, like every other op), and `wasm_mount_write` also needs write
/// capability in the `sandbox` mask.
fn check_mount(i: &Interpreter, b: &Bindings, op: &str, marker: &str, path: &str, write: bool) -> Result<()> {
    if write && CapMask::any_no_write(b.cap_mask.as_deref()) {
        return Err(denied(
            op,
            format!("{marker} {} forbidden by sandbox (no_write scope)", go_quote(path)),
            path,
        ));
    }
    check_path_declared(i, b, path, write).map_err(|e| denied(op, format!("{marker}: {}", e.message), path))?;
    Ok(())
}

/// F03: `wasm_allow_host` must name a host declared in `requires` and needs
/// network capability in the `sandbox` mask.
fn check_allow_host(i: &Interpreter, b: &Bindings, op: &str, host: &str) -> Result<()> {
    if CapMask::any_no_network(b.cap_mask.as_deref()) {
        return Err(denied(op, format!("wasm_allow_host {} forbidden by sandbox (no_network scope)", go_quote(host)), host));
    }
    check_host_declared(i, host).map_err(|e| denied(op, format!("wasm_allow_host: {}", e.message), host))
}

const DEADLINE_MSG: &str = "perch: wasm deadline exceeded";

/// Bumps the engine epoch every 10ms until dropped.
struct Ticker(Option<mpsc::Sender<()>>, Option<std::thread::JoinHandle<()>>);

impl Ticker {
    fn start(engine: Engine) -> Ticker {
        let (tx, rx) = mpsc::channel::<()>();
        let h = std::thread::spawn(move || {
            while let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(Duration::from_millis(10)) {
                engine.increment_epoch();
            }
        });
        Ticker(Some(tx), Some(h))
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        drop(self.0.take());
        if let Some(h) = self.1.take() {
            let _ = h.join();
        }
    }
}

/// Reads a `.wasm` file, compiles (cached by content sha256).
fn compile_file(rt: &Runtime, path: &str) -> std::result::Result<Module, String> {
    let bytes = std::fs::read(path).map_err(|e| path_err("open", path, &e))?;
    let key = hex::encode(Sha256::digest(&bytes));
    compile_bytes(rt, &key, &bytes)
}

fn compile_bytes(rt: &Runtime, key: &str, bytes: &[u8]) -> std::result::Result<Module, String> {
    let mut cache = rt.compiled.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = cache.get(key) {
        return Ok(m.clone());
    }
    // Persistent cache (F02): PERCH_WASM_CACHE=off skips it entirely.
    let dir = if wasm_cache::enabled() { wasm_cache::cache_dir() } else { None };
    let m = wasm_cache::load_or_compile(&rt.engine, dir.as_deref(), bytes)?;
    cache.insert(key.to_string(), m.clone());
    Ok(m)
}

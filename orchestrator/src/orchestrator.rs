//! The wiring layer — the only crate that knows about every concrete
//! implementation. It builds each use-case `Impl` with closures over infra and
//! hands the assembled `Cli` the process argv.
use crate::adapters as a;
use crate::flags::{self, Set};
use perch_cli::{Cli, Config, UseCases};
use perch_domain::Program;
use perch_interpreter::{HTTPPolicy, Handler, Interpreter, SharedBuf, SharedWriter, Tracer};
use perch_ops::Restrictions;
use perch_runtests::TestSandbox;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Error = perch_cli::Error;

/// The perch version string. Bumped on each release tag.
pub const VERSION: &str = "0.1.1";

/// The default config the CLI looks for when no -f flag is given.
pub const DEFAULT_COMMANDS_FILE: &str = "commands.perch";

/// Everything the global flags decided; shared by the normal and embedded
/// wiring.
#[derive(Clone, Default)]
struct Settings {
    restrictions: Restrictions,
    env_allow: Option<Set>,
    allow_bins: Option<Set>,
    no_meta: bool,
    audit_path: String,
    report_path: String,
    report_on: bool,
    trace_path: String,
    trace_on: bool,
    max_runtime: Duration,
    http_policy: Option<HTTPPolicy>,
    preview_mode: String,
    stdin_untrusted: bool,
}

fn exit(code: i32) -> ! {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code)
}

/// The perch entry point: detects an embedded program, strips the global
/// flags, wires the CLI and exits with its code.
pub fn run() {
    let mut args: Vec<String> = std::env::args_os().map(|s| s.to_string_lossy().into_owned()).collect();

    // `perch help TOPIC` needs the literal flag tokens preserved (the topic may
    // BE a flag like --no-shell), so skip global-flag stripping.
    if args.len() >= 2 && args[1] == "help" {
        exit(build_cli(&Settings::default(), None).run_args(&args));
    }

    let mut s = Settings {
        restrictions: flags::extract_restrictions(&mut args),
        ..Default::default()
    };
    s.env_allow = flags::extract_env(&mut args);
    let (allow_bins, no_meta) = flags::extract_shell_guards(&mut args);
    s.allow_bins = allow_bins;
    s.no_meta = no_meta;
    let allow = flags::extract_allow(&mut args);
    s.audit_path = flags::extract_audit(&mut args);
    let (rp, ro) = flags::extract_report(&mut args);
    s.report_path = rp;
    s.report_on = ro;
    let (tp, to) = flags::extract_trace(&mut args);
    s.trace_path = tp;
    s.trace_on = to;
    s.max_runtime = flags::extract_max_runtime(&mut args);
    s.http_policy = flags::extract_http_policy(&mut args);
    s.preview_mode = flags::extract_preview(&mut args);

    // Stdin input (-f -) is untrusted by default: strictest restrictions unless
    // the user opts back in with --allow-X or --trust-stdin.
    if flags::is_stdin_invocation(&args) && !allow.trust_stdin {
        s.stdin_untrusted = true;
        if !allow.shell {
            s.restrictions.no_shell = true;
        }
        if !allow.subprocess {
            s.restrictions.no_subprocess = true;
        }
        if !allow.network {
            s.restrictions.no_network = true;
        }
        if !allow.write {
            s.restrictions.no_write = true;
        }
        if s.env_allow.is_none() {
            s.env_allow = Some(Set::new());
        }
    }

    if args[1..].iter().any(|x| x == "--restrictions") {
        show_restrictions();
        exit(0);
    }

    match perch_embed::load() {
        Err(e) => {
            eprintln!("embedded program: {e}");
            exit(1);
        }
        Ok(Some(bundle)) => exit(build_cli(&s, Some(bundle)).run_args(&args)),
        Ok(None) => exit(build_cli(&s, None).run_args(&args)),
    }
}

fn show_restrictions() {
    println!("Available restriction flags:");
    for name in perch_ops::restriction_list() {
        // Go's `%-15s` pads the name; `%v` of []string prints `[a b c]`.
        println!("  --{:<15} blocks: [{}]", name, perch_ops::blocked_by_restriction(&name).join(" "));
    }
    println!();
    println!("Compose freely:");
    println!("  perch --no-shell --no-network --no-write <cmd>");
    println!("  perch --env HOME,PATH,API_KEY <cmd>   # restrict host env-var visibility");
}

/// Prints a one-line banner naming the active restrictions (silent when
/// nothing's restricted).
fn announce_security_posture(s: &Settings) {
    let mut parts: Vec<String> = vec![];
    if s.restrictions.active() {
        parts.push(s.restrictions.as_flags().join(" "));
    }
    let names = |m: &Set| {
        let mut v: Vec<&str> = m.keys().map(|k| k.as_str()).collect();
        v.sort();
        v.join(",")
    };
    if let Some(env) = &s.env_allow {
        if env.is_empty() {
            parts.push("--env (empty)".into());
        } else {
            parts.push(format!("--env {}", names(env)));
        }
    }
    if let Some(b) = &s.allow_bins {
        parts.push(format!("--allow-bin {}", names(b)));
    }
    if s.no_meta {
        parts.push("--no-shell-metachars".into());
    }
    if !s.audit_path.is_empty() {
        parts.push(format!("--audit {}", s.audit_path));
    }
    if !s.max_runtime.is_zero() {
        parts.push(format!("--max-runtime {}s", s.max_runtime.as_secs()));
    }
    if s.stdin_untrusted {
        eprintln!("🔒 stdin (untrusted): {}", parts.join("  "));
        eprintln!("   → grant capabilities with --allow-shell / --allow-subprocess / --allow-network / --allow-write / --env A,B,C");
        eprintln!("   → or skip the deny-by-default posture with --trust-stdin");
    } else if !parts.is_empty() {
        eprintln!("🔒 security: {}", parts.join("  "));
    }
}

/// The BeforeOp matching the preview mode, or None for normal execution.
fn build_interpreter_hook(mode: &str) -> Option<perch_interpreter::BeforeOp> {
    match mode {
        "ask" => {
            println!("──── Step-through preview — y=run, n=skip, a=all, q=quit ────");
            Some(perch_preview::ask_hook(Box::new(std::io::stdin()), SharedWriter::stdout()))
        }
        "dry-run" => {
            println!("──── Dry-run — printing plan; no ops execute ────");
            Some(perch_preview::dry_run_hook(SharedWriter::stdout()))
        }
        _ => None,
    }
}

fn to_go_map(s: &Option<Set>) -> Option<HashMap<String, bool>> {
    s.clone()
}

/// A destination for --trace / --report output: stderr for ""/"-", otherwise a
/// freshly created file (falling back to stderr, with a notice, on failure).
fn open_sink(path: &str, flag: &str) -> Box<dyn Write + Send> {
    if path.is_empty() || path == "-" {
        return Box::new(std::io::stderr());
    }
    match std::fs::File::create(path) {
        Ok(f) => Box::new(f),
        Err(e) => {
            eprintln!("{flag}: cannot write to {path}: {} (falling back to stderr)", io_reason(path, &e));
            Box::new(std::io::stderr())
        }
    }
}

fn io_reason(path: &str, e: &std::io::Error) -> String {
    let why = match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => e.to_string(),
    };
    format!("open {path}: {why}")
}

/// Everything shared by a fresh interpreter: hooks, allowlists, policy.
fn configure(i: &mut Interpreter, s: &Settings, hook: &Option<perch_interpreter::BeforeOp>) {
    i.preflight_hook = Some(Arc::new(perch_ops::preflight));
    i.hook_category = Some(Arc::new(|k: &str| perch_ops::hook_category_of(k)));
    i.set_before_op(hook.clone());
    i.env_allowlist = to_go_map(&s.env_allow);
    i.allowed_shell_bins = to_go_map(&s.allow_bins);
    i.no_shell_metachars = s.no_meta;
    i.http_policy = s.http_policy.clone();
}

/// Builds the closure that runs one command with audit/report/trace wiring.
/// `fixed` is the embedded program (ignores the loaded one).
fn make_run_fn(
    handlers: HashMap<String, Handler>,
    s: Settings,
    hook: Option<perch_interpreter::BeforeOp>,
    fixed: Option<Program>,
) -> Box<dyn Fn(&Program, &str, &[String]) -> Result<(), Error>> {
    Box::new(move |p: &Program, name: &str, args: &[String]| {
        let prog = fixed.clone().unwrap_or_else(|| p.clone());
        let mut i = Interpreter::new(handlers.clone(), prog);
        configure(&mut i, &s, &hook);
        if !s.max_runtime.is_zero() {
            i.set_deadline(Some(Instant::now() + s.max_runtime));
        }
        let mut audit_done = None;
        if !s.audit_path.is_empty() {
            let sink = Arc::new(perch_audit::open(&s.audit_path)?);
            audit_done = Some(sink.wire_into(&mut i, name, args));
        }
        let mut rec: Option<Arc<perch_report::Recorder>> = None;
        if s.report_on {
            let r = Arc::new(perch_report::Recorder::new());
            r.set_root(name);
            i.tracer = Some(r.clone() as Arc<dyn Tracer>);
            rec = Some(r);
        } else if s.trace_on {
            // Live trace streams each op as it fires; shares the Tracer slot
            // with --report, so they are mutually exclusive.
            let w = open_sink(&s.trace_path, "--trace");
            i.tracer = Some(Arc::new(perch_report::LiveTracer::new(w)));
        }
        let err = i.run(name, args);
        if let Some(done) = audit_done {
            done(err.as_ref().err());
        }
        if let Some(r) = rec {
            r.finish(err.as_ref().err());
            let mut out = open_sink(&s.report_path, "--report");
            r.render(&mut *out);
        }
        err
    })
}

/// The per-test runner: each test gets its own handler map (sandbox masks layer
/// on top of the global restrictions for THIS test only), interpreter and
/// deadline; output goes to the supplied buffer.
fn run_test_fn(s: Settings) -> perch_runtests::RunTestFn {
    Box::new(move |p: &Program, name: &str, sb: &TestSandbox, out: &mut Vec<u8>| {
        let mut handlers = perch_ops::all_handlers();
        let r = &s.restrictions;
        let tr = Restrictions {
            no_shell: r.no_shell || sb.no_shell,
            no_subprocess: r.no_subprocess || sb.no_subprocess,
            no_network: r.no_network || sb.no_network,
            no_write: r.no_write || sb.no_write,
        };
        perch_ops::apply_restrictions(&mut handlers, &tr);
        perch_ops::apply_mask_gating(&mut handlers);
        let mut ti = Interpreter::new(handlers, p.clone());
        let buf = SharedBuf::new();
        ti.stdout = buf.writer();
        ti.stderr = buf.writer();
        configure(&mut ti, &s, &None);
        if !sb.timeout.is_zero() {
            ti.set_deadline(Some(Instant::now() + sb.timeout));
        }
        let prev_cwd = std::env::current_dir().ok();
        let mut restore = None;
        if !sb.cwd.is_empty() && prev_cwd.as_ref().map(|c| c.to_string_lossy() != sb.cwd.as_str()).unwrap_or(true) {
            if let Err(e) = std::env::set_current_dir(&sb.cwd) {
                return Err(format!("test sandbox: chdir {}: {}", sb.cwd, chdir_reason(&e)).into());
            }
            restore = prev_cwd;
        }
        let res = ti.run_private(name, &[]);
        if let Some(d) = restore {
            let _ = std::env::set_current_dir(d);
        }
        out.extend_from_slice(buf.contents().as_bytes());
        res
    })
}

fn chdir_reason(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".into(),
        std::io::ErrorKind::PermissionDenied => "permission denied".into(),
        _ => e.to_string(),
    }
}

/// Wires the CLI. With `bundle`, Run/List/Server/Shell/Validate/CommandHelp
/// serve the embedded program instead of reading a file.
fn build_cli(s: &Settings, bundle: Option<perch_embed::Bundle>) -> Cli {
    let mut handlers = perch_ops::all_handlers();
    perch_ops::apply_restrictions(&mut handlers, &s.restrictions);
    perch_ops::apply_mask_gating(&mut handlers);
    announce_security_posture(s);
    let hook = build_interpreter_hook(&s.preview_mode);

    let embedded: Option<Program> = bundle.as_ref().map(|b| b.program.clone());
    if let Some(b) = &bundle {
        // bundle_dir / bundle_hash / bundle_extract read this.
        perch_ops::set_bundle(if b.archive.is_empty() { None } else { Some(b.archive.clone()) }, &b.archive_hash);
    }

    // The loader every program-reading use case shares.
    let loader = |embedded: &Option<Program>| -> Box<dyn Fn(&str) -> Result<Program, Error>> {
        match embedded.clone() {
            Some(p) => Box::new(move |_| Ok(p.clone())),
            None => Box::new(perch_capyloader::load),
        }
    };
    let send_loader = |embedded: &Option<Program>| -> Box<dyn Fn(&str) -> Result<Program, Error> + Send + Sync> {
        match embedded.clone() {
            Some(p) => Box::new(move |_| Ok(p.clone())),
            None => Box::new(perch_capyloader::load),
        }
    };

    let known: HashSet<String> = handlers.keys().cloned().collect();
    let known_fn = |k: &HashSet<String>| -> Box<dyn Fn() -> HashSet<String> + Send + Sync> {
        let k = k.clone();
        Box::new(move || k.clone())
    };

    let server = RefCell::new(perch_httpserver::Server {
        handlers: handlers.clone(),
        config_path: String::new(),
        known_ops: Some(known_fn(&known)),
    });

    let build: Box<dyn perch_cli::RunBuildUseCase> = if embedded.is_some() {
        // Embedded binaries shouldn't re-embed; surface a friendly error.
        Box::new(a::DisabledBuild)
    } else {
        Box::new(a::Build(perch_runbuild::Impl {
            load: Box::new(perch_capyloader::load),
            embed: Box::new(|src, p, archive, out| perch_embed::embed(src, p, archive, out)),
        }))
    };

    let install_lsp_fn: perch_installvscode::InstallLSPFn =
        Box::new(|| perch_installlsp::Impl.execute(&mut std::io::stdout()));

    let uc = UseCases {
        run: Box::new(a::Run(perch_runcommand::Impl {
            load: loader(&embedded),
            run: make_run_fn(handlers.clone(), s.clone(), hook, embedded.clone()),
            suggest: Some(Box::new(perch_commandhelp::suggest)),
        })),
        list: Box::new(a::List(perch_listcommands::Impl { load: loader(&embedded) })),
        init: Box::new(a::Init(perch_initconfig::Impl)),
        build,
        server: Box::new(a::Server(perch_runserver::Impl {
            load: loader(&embedded),
            serve: Box::new(move |p, host, port, cfg| server.borrow_mut().serve(p, host, port, cfg)),
        })),
        shell: Box::new(a::Shell(perch_runshell::Impl {
            load_string: Box::new(perch_capyloader::load_from_string),
            load: send_loader(&embedded),
            handlers: handlers.clone(),
        })),
        validate: Box::new(a::Validate(perch_validate::Impl {
            load: send_loader(&embedded),
            known_ops: known_fn(&known),
        })),
        command_help: Box::new(a::CommandHelp(perch_commandhelp::Impl { load: loader(&embedded) })),
        install_lsp: Box::new(a::InstallLsp(perch_installlsp::Impl)),
        install_vscode: Box::new(a::InstallVscode(perch_installvscode::Impl { install_lsp: Some(install_lsp_fn) })),
        import_sh: Box::new(a::ImportSh(perch_importsh::Impl)),
        scan: Box::new(a::Scan(perch_scan::Impl { load: Box::new(perch_capyloader::load) })),
        help: Box::new(a::Help(perch_help::Impl { version: VERSION.to_string() })),
        test: Box::new(a::Test(perch_runtests::Impl {
            load: Box::new(perch_capyloader::load),
            run_test: run_test_fn(s.clone()),
            default_timeout: Duration::ZERO,
            keep_temp_dir: false,
        })),
        simulate: Box::new(a::Simulate(perch_simulate::Impl { load: Box::new(perch_capyloader::load) })),
        export_ops_catalog: Box::new(a::ExportOps(perch_exportopscatalog::Impl {
            kinds: Some(Box::new(perch_ops::builtin_kinds)),
        })),
    };

    let version = match &embedded {
        Some(p) if !p.version.is_empty() => p.version.clone(),
        _ => VERSION.to_string(),
    };
    Cli {
        use_cases: uc,
        config: Config { default_commands_file: DEFAULT_COMMANDS_FILE.to_string(), version },
    }
}

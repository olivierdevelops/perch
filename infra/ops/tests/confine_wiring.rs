//! T-12..T-19 at the perch level: the confinement module wired into every
//! spawn (PLAN-2026-0001 I-09, decision D3). These run real `.perch` source
//! through the gated handler set, so the declared `requires` scopes -> child
//! confinement path is exercised end to end.
use perch_domain::{ErrorKind, OpError};
use perch_interpreter::{Error, Interpreter, SharedBuf};
use perch_ops::{all_handlers, confine_probe, preflight, Support};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

fn tmpdir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!("perch-confwire-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

struct Knobs {
    allow_advisory: bool,
    unsupported: Option<&'static str>,
}

fn run(src: &str, k: &Knobs, cwd: &std::path::Path) -> (String, Option<Error>) {
    let prog = perch_capyloader::load_from_string(src).expect("load");
    let mut i = Interpreter::new(all_handlers(), prog);
    i.preflight_hook = Some(Arc::new(preflight));
    i.allow_advisory_scopes = k.allow_advisory;
    i.confine_unsupported = k.unsupported.map(String::from);
    let buf = SharedBuf::new();
    i.stdout = buf.writer();
    i.stderr = buf.writer();
    let prev = std::env::current_dir().unwrap();
    let _g = CWD.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_current_dir(cwd).unwrap();
    let res = i.run("go", &[]);
    std::env::set_current_dir(prev).unwrap();
    (buf.contents(), res.err())
}

static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn kind_of(e: &Option<Error>) -> Option<ErrorKind> {
    e.as_ref().and_then(|e| e.downcast_ref::<OpError>()).map(|o| o.kind)
}

fn enforced() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux")) && confine_probe() == Support::Enforced
}

const NONE: Knobs = Knobs { allow_advisory: false, unsupported: None };

fn shell_prog(root: &std::path::Path, script: &str) -> String {
    format!(
        "name \"x\"\nrequires\n    bin \"sh\"\n    write \"{r}/allowed\"\nend\ncommand go\n    do\n        shell \"{s}\"\n    end\nend\n",
        r = root.display(),
        s = script.replace('"', "\\\"")
    )
}

#[test]
fn shell_declared_write_ok_and_escape_refused() {
    if !enforced() {
        return;
    }
    let root = tmpdir();
    std::fs::create_dir_all(root.join("allowed")).unwrap();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    let ok = "sh -c 'echo in > allowed/ok.txt'";
    let (out, err) = run(&shell_prog(&root, ok), &NONE, &root);
    assert!(err.is_none(), "{out} {err:?}");
    assert_eq!(std::fs::read_to_string(root.join("allowed/ok.txt")).unwrap().trim(), "in");

    let esc = format!("sh -c 'echo out > {}/outside/x'", root.display());
    let (_, err) = run(&shell_prog(&root, &esc), &NONE, &root);
    assert!(err.is_some(), "escape must fail");
    assert!(!root.join("outside/x").exists(), "escape file must be absent");
}

#[test]
fn exec_and_pipe_and_chain_are_confined() {
    if !enforced() {
        return;
    }
    let root = tmpdir();
    std::fs::create_dir_all(root.join("allowed")).unwrap();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    let mk = |body: &str| {
        format!(
            "name \"x\"\nrequires\n    bin \"sh\"\n    bin \"cat\"\n    write \"{r}/allowed\"\nend\ncommand go\n    do\n{body}\n    end\nend\n",
            r = root.display()
        )
    };
    let esc = |n: &str| format!("        exec sh -c \"echo out > {}/outside/{n}\"", root.display());
    let (_, e1) = run(&mk(&esc("a")), &NONE, &root);
    assert!(e1.is_some() && !root.join("outside/a").exists(), "exec escape");
    let chain = format!("{}\n        exec sh -c \"echo out > {}/outside/b\" ;\n", "        exec sh -c \"true\"", root.display());
    let _ = run(&mk(&chain), &NONE, &root);
    assert!(!root.join("outside/b").exists(), "exec_chain escape");
    let pipe = format!(
        "        pipe\n            exec sh -c \"echo hi\"\n            exec sh -c \"cat > {}/outside/c\"\n        end",
        root.display()
    );
    let _ = run(&mk(&pipe), &NONE, &root);
    assert!(!root.join("outside/c").exists(), "pipe escape");
    // And the allowed root works through exec.
    let okp = "        exec sh -c \"echo in > allowed/e.txt\"";
    let (out, e) = run(&mk(okp), &NONE, &root);
    assert!(e.is_none(), "{out} {e:?}");
    assert!(root.join("allowed/e.txt").exists());
}

#[test]
fn env_scrub_survives_confinement() {
    if !enforced() {
        return;
    }
    std::env::set_var("PERCH_WIRE_SECRET", "s3cret");
    let root = tmpdir();
    std::fs::create_dir_all(root.join("allowed")).unwrap();
    let src = format!(
        "name \"x\"\nrequires\n    bin \"sh\"\n    write \"{r}/allowed\"\nend\ncommand go\n    do\n        exec sh -c \"echo secret=$PERCH_WIRE_SECRET > allowed/env.txt\"\n    end\nend\n",
        r = root.display()
    );
    let (out, e) = run(&src, &NONE, &root);
    assert!(e.is_none(), "{out} {e:?}");
    let got = std::fs::read_to_string(root.join("allowed/env.txt")).unwrap();
    assert_eq!(got.trim(), "secret=", "undeclared host env must not reach a confined child");
}

#[test]
fn no_scopes_declared_means_unconfined() {
    let root = tmpdir();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    // `requires` with only a bin: nothing to confine, even with an Unsupported
    // probe and no opt-out (must not refuse).
    let src = format!(
        "name \"x\"\nrequires\n    bin \"sh\"\nend\ncommand go\n    do\n        shell \"sh -c 'echo free > {}/outside/x'\"\n    end\nend\n",
        root.display()
    );
    let k = Knobs { allow_advisory: false, unsupported: Some("pretend unsupported") };
    let (out, err) = run(&src, &k, &root);
    assert!(err.is_none(), "{out} {err:?}");
    assert_eq!(std::fs::read_to_string(root.join("outside/x")).unwrap().trim(), "free");
}

#[test]
fn unsupported_platform_refuses_by_default() {
    let root = tmpdir();
    let k = Knobs { allow_advisory: false, unsupported: Some("no mechanism here") };
    let marker = root.join("allowed/ran");
    std::fs::create_dir_all(root.join("allowed")).unwrap();
    let script = format!("sh -c 'echo x > {}'", marker.display());
    for body in [
        format!("shell \"{script}\""),
        format!("exec sh -c \"echo x > {}\"", marker.display()),
        format!("pipe\n            exec sh -c \"echo x > {}\"\n        end", marker.display()),
    ] {
        let src = format!(
            "name \"x\"\nrequires\n    bin \"sh\"\n    write \"{r}/allowed\"\nend\ncommand go\n    do\n        {body}\n    end\nend\n",
            r = root.display()
        );
        let (_, err) = run(&src, &k, &root);
        assert_eq!(kind_of(&err), Some(ErrorKind::ConfinementUnavailable), "{err:?}");
        let msg = err.unwrap().to_string();
        assert!(msg.contains("no mechanism here") && msg.contains("--allow-advisory-scopes"), "{msg}");
        assert!(!marker.exists(), "refused spawn must not run");
    }
}

#[test]
fn advisory_flag_runs_unconfined_and_prints_banner_once() {
    let root = tmpdir();
    std::fs::create_dir_all(root.join("allowed")).unwrap();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    let k = Knobs { allow_advisory: true, unsupported: Some("no mechanism here") };
    let src = format!(
        "name \"x\"\nrequires\n    bin \"sh\"\n    write \"{r}/allowed\"\nend\ncommand go\n    do\n        shell \"sh -c 'echo a > {r}/outside/1'\"\n        shell \"sh -c 'echo b > {r}/outside/2'\"\n    end\nend\n",
        r = root.display()
    );
    let (out, err) = run(&src, &k, &root);
    assert!(err.is_none(), "{err:?}");
    assert!(root.join("outside/1").exists() && root.join("outside/2").exists());
    let banner = perch_ops::advisory_banner("no mechanism here");
    assert_eq!(out.matches(&banner).count(), 1, "banner once per process/interpreter: {out}");
}

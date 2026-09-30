//! Direct-dispatch tests for the group-A block/process/system ops, building op
//! trees by hand (no loader) against a legacy (undeclared) program.
use perch_domain::{Command, ErrorKind, Op, OpError, Program};
use perch_interpreter::{Bindings, Error, Interpreter, SharedBuf};
use perch_ops::{all_handlers, apply_mask_gating, apply_restrictions, Restrictions};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn tmpdir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!("perch-ops-blk-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn op(kind: &str, args: Value) -> Op {
    Op { kind: kind.into(), args: args.as_object().cloned().unwrap_or_default(), ..Default::default() }
}

fn block(kind: &str, args: Value, body: Vec<Op>) -> Op {
    Op { body, ..op(kind, args) }
}

fn capture(mut o: Op, name: &str) -> Op {
    o.capture_into = name.into();
    o
}

fn interp(prog: Program) -> (Interpreter, SharedBuf) {
    let mut i = Interpreter::new(all_handlers(), prog);
    let buf = SharedBuf::new();
    i.stdout = buf.writer();
    i.stderr = buf.writer();
    (i, buf)
}

fn run(ops: &[Op]) -> (String, Result<(), Error>, Bindings) {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(&std::env::current_dir().unwrap().to_string_lossy());
    let r = i.run_ops(ops, &mut b);
    (buf.contents(), r, b)
}

fn print(msg: &str) -> Op {
    op("print", json!({"msg": msg}))
}

#[test]
fn print_and_eprintln() {
    let (out, r, _) = run(&[print("hi"), op("eprintln", json!({"_0": "err"})), op("println", json!({"_0": "two"}))]);
    assert!(r.is_ok());
    assert_eq!(out, "hi\nerr\ntwo\n");
}

#[test]
fn fail_op_kind_and_default_message() {
    let (_, r, _) = run(&[op("fail", json!({"msg": "boom"}))]);
    let e = r.unwrap_err();
    assert_eq!(e.to_string(), "user_fail: boom");
    let (_, r, _) = run(&[op("fail", json!({}))]);
    assert_eq!(r.unwrap_err().to_string(), "user_fail: (no message)");
}

#[cfg(unix)]
#[test]
fn shell_nonzero_is_tagged() {
    let (_, r, _) = run(&[op("shell", json!({"cmd": "exit 3"}))]);
    let e = r.unwrap_err();
    let oe = e.downcast_ref::<OpError>().unwrap();
    assert_eq!(oe.kind, ErrorKind::ShellExitNonzero);
    assert_eq!(oe.code, "3");
    assert_eq!(oe.message, "exit status 3");
    assert_eq!(oe.detail, "exit 3");
}

#[cfg(unix)]
#[test]
fn shell_signal_is_tagged() {
    let (_, r, _) = run(&[op("shell", json!({"cmd": "kill -9 $$"}))]);
    let oe = r.unwrap_err().downcast_ref::<OpError>().cloned().unwrap();
    assert_eq!(oe.kind, ErrorKind::ShellSignalKilled);
    assert_eq!(oe.code, "-1");
    assert_eq!(oe.message, "signal: killed");
}

#[cfg(unix)]
#[test]
fn shell_output_trims_and_captures() {
    let (out, r, b) = run(&[capture(op("shell_output", json!({"cmd": "printf 'a\\nb\\n\\n'"})), "x"), op("shell", json!({"cmd": "echo streamed"}))]);
    assert!(r.is_ok());
    assert_eq!(b.vars["x"], json!("a\nb"));
    assert_eq!(out, "streamed\n");
}

#[cfg(unix)]
#[test]
fn shell_uses_cwd_and_shell_in_dir() {
    let dir = tmpdir();
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(&dir.to_string_lossy());
    i.run_ops(&[op("shell", json!({"cmd": "pwd"}))], &mut b).unwrap();
    assert_eq!(buf.contents().trim(), dir.to_string_lossy());
    let other = tmpdir();
    i.run_ops(&[op("shell_in", json!({"dir": other.to_string_lossy(), "cmd": "pwd"}))], &mut b).unwrap();
    assert!(buf.contents().contains(&*other.to_string_lossy()));
}

#[cfg(unix)]
#[test]
fn try_shell_returns_bool() {
    let (_, r, b) = run(&[capture(op("try_shell", json!({"cmd": "true"})), "a"), capture(op("try_shell", json!({"cmd": "false"})), "b")]);
    assert!(r.is_ok());
    assert_eq!(b.vars["a"], json!(true));
    assert_eq!(b.vars["b"], json!(false));
}

#[cfg(unix)]
#[test]
fn shell_metachar_and_allow_bin_checks() {
    let (mut i, _buf) = interp(Program::default());
    i.no_shell_metachars = true;
    let mut b = Bindings::new(".");
    let e = i.run_ops(&[op("shell", json!({"cmd": "echo a | cat"}))], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "shell_metachars_denied: shell metachar \"|\" rejected by --no-shell-metachars (echo a | cat)");

    let (mut i, _buf) = interp(Program::default());
    i.allowed_shell_bins = Some([("echo".to_string(), true)].into());
    let e = i.run_ops(&[op("shell", json!({"cmd": "FOO=1 /bin/ls"}))], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "shell_bin_not_allowed: binary \"ls\" is not in --allow-bin (allowed: echo) (ls)");
    i.run_ops(&[op("shell", json!({"cmd": "FOO=1 echo ok"}))], &mut b).unwrap();
}

#[cfg(unix)]
#[test]
fn exec_missing_binary_message() {
    let (_, r, _) = run(&[op("exec", json!({"bin": "definitely-not-a-bin-xyz"}))]);
    let oe = r.unwrap_err().downcast_ref::<OpError>().cloned().unwrap();
    assert_eq!(oe.kind, ErrorKind::ShellExitNonzero);
    assert_eq!(oe.message, "exec: \"definitely-not-a-bin-xyz\": executable file not found in $PATH");
    let (_, r, _) = run(&[op("exec", json!({}))]);
    assert_eq!(r.unwrap_err().to_string(), "unclassified: exec: missing binary");
}

#[cfg(unix)]
#[test]
fn exec_nonzero_display() {
    let (_, r, _) = run(&[op("exec", json!({"bin": "sh", "_0": "-c", "_1": "exit 2"}))]);
    let oe = r.unwrap_err().downcast_ref::<OpError>().cloned().unwrap();
    assert_eq!(oe.code, "2");
    assert_eq!(oe.detail, "sh -c exit 2");
}

#[test]
fn exit_and_sleep_do_not_panic_on_args() {
    let (_, r, _) = run(&[op("sleep", json!({"seconds": 0.01})), op("sleep", json!({"seconds": "0"}))]);
    assert!(r.is_ok());
}

// ── blocks ────────────────────────────────────────────────────────────────

#[test]
fn for_each_iterates_and_restores() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    b.set("f", "orig");
    let ops = [block("for_each", json!({"_0": "a\n\nb\n", "_1": "f"}), vec![print("got ${f}")]), print("after ${f}")];
    i.run_ops(&ops, &mut b).unwrap();
    assert_eq!(buf.contents(), "got a\ngot b\nafter orig\n");
    // default var name is `item`, removed afterwards.
    let ops = [block("for_each", json!({"value": "x"}), vec![print("i=${item}")])];
    i.run_ops(&ops, &mut b).unwrap();
    assert!(!b.vars.contains_key("item"));
}

#[test]
fn if_ops() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    b.set("v", "1.10.0");
    b.set("n", "10");
    b.set("e", "");
    let body = || vec![print("yes")];
    let mk = |o: &str, lhs: &str, rhs: Value| block("if", json!({"op": o, "lhs": lhs, "rhs": rhs}), body());
    let ops = [
        mk("gt", "v", json!("1.9.0")),   // semver-aware
        mk("ge", "n", json!(9)),         // numeric
        mk("lt", "n", json!(9)),         // no
        mk("eq", "v", json!("1.10.0")),
        mk("neq", "v", json!("x")),
        mk("truthy", "n", Value::Null),
        mk("falsy", "e", Value::Null),
        mk("truthy", "e", Value::Null), // no
    ];
    i.run_ops(&ops, &mut b).unwrap();
    assert_eq!(buf.contents().matches("yes").count(), 6);
}

#[test]
fn if_call_runs_body_when_truthy() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    let ops = [
        block("if_call", json!({"func": "has_bin", "_0": "sh"}), vec![print("has sh")]),
        block("if_call", json!({"func": "has_bin", "_0": "definitely-not-a-bin-xyz"}), vec![print("never")]),
        block("if_call", json!({"func": "no_such_op", "_0": "x"}), vec![print("never2")]),
    ];
    i.run_ops(&ops, &mut b).unwrap();
    let out = buf.contents();
    assert_eq!(out.contains("has sh"), cfg!(unix), "{out}");
    assert!(!out.contains("never"));
}

#[test]
fn os_and_arch_blocks() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    b.set("os", "linux");
    b.set("arch", "arm64");
    let ops = [
        block("os", json!({"target": "unix"}), vec![print("unix")]),
        block("os", json!({"target": "windows"}), vec![print("win")]),
        block("arch", json!({"target": "arm64"}), vec![print("arm")]),
        block("arch", json!({"target": "amd64"}), vec![print("amd")]),
    ];
    i.run_ops(&ops, &mut b).unwrap();
    assert_eq!(buf.contents(), "unix\narm\n");
}

#[test]
fn timeout_errors_and_restores_deadline() {
    let (i, _) = interp(Program::default());
    let mut b = Bindings::new(".");
    let e = i.run_ops(&[block("timeout", json!({"duration": "abc"}), vec![])], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "timeout: invalid duration \"abc\": time: invalid duration \"abc\"");
    // A tripped deadline makes the NEXT op fail with the timeout error; the
    // previous (absent) deadline is restored afterwards.
    let ops = [block("timeout", json!({"duration": "1ms"}), vec![op("sleep", json!({"seconds": 0.05})), print("late")])];
    let e = i.run_ops(&ops, &mut b).unwrap_err();
    assert!(perch_interpreter::is_timeout(&e), "{e}");
    assert!(i.deadline().is_none());
}

#[test]
fn retry_exhausts_with_wrapped_error() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    let ops = [block("retry", json!({"attempts": 3, "backoff": "fixed", "base": "1ms"}), vec![print("try"), op("fail", json!({"msg": "boom"}))])];
    let e = i.run_ops(&ops, &mut b).unwrap_err();
    assert_eq!(e.to_string(), "retry: 3 attempts failed; last error: user_fail: boom");
    assert_eq!(buf.contents().matches("try").count(), 3);
    // success on first attempt short-circuits.
    let ops = [block("retry", json!({"attempts": 5}), vec![print("once")])];
    i.run_ops(&ops, &mut b).unwrap();
    assert_eq!(buf.contents().matches("once").count(), 1);
}

#[test]
fn parallel_runs_all_and_aggregates_errors() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    i.run_ops(&[block("parallel", json!({}), vec![print("p1"), print("p2"), print("p3")])], &mut b).unwrap();
    let out = buf.contents();
    for p in ["p1", "p2", "p3"] {
        assert!(out.contains(p), "{out}");
    }
    let e = i
        .run_ops(&[block("parallel", json!({}), vec![op("fail", json!({"msg": "a"})), op("fail", json!({"msg": "b"}))])], &mut b)
        .unwrap_err();
    assert_eq!(e.to_string(), "parallel: 2 branches failed: user_fail: a | user_fail: b");
    let e = i
        .run_ops(&[block("parallel", json!({}), vec![op("fail", json!({"msg": "a"})), print("ok")])], &mut b)
        .unwrap_err();
    assert_eq!(e.to_string(), "user_fail: a");
}

#[test]
fn with_env_overlays_and_restores() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    b.env.insert("KEEP".into(), "orig".into());
    let body = if cfg!(unix) { vec![op("shell", json!({"cmd": "echo $A-$KEEP"}))] } else { vec![] };
    i.run_ops(&[block("with_env", json!({"env": "A=\"1\", KEEP=two"}), body)], &mut b).unwrap();
    if cfg!(unix) {
        assert_eq!(buf.contents(), "1-two\n");
    }
    assert_eq!(b.env.get("KEEP").map(String::as_str), Some("orig"));
    assert!(!b.env.contains_key("A"));
    let e = i.run_ops(&[block("with_env", json!({"env": "novalue"}), vec![])], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "with_env: expected KEY=value, got \"novalue\"");
}

#[test]
fn with_cwd_switches_and_restores() {
    let dir = tmpdir();
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("f"), "x").unwrap();
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(&dir.to_string_lossy());
    i.run_ops(&[block("with_cwd", json!({"path": "sub"}), vec![op("cwd", json!({}))]), print("done")], &mut b).unwrap();
    assert_eq!(b.cwd, dir.to_string_lossy());
    let e = i.run_ops(&[block("with_cwd", json!({"path": "nope"}), vec![])], &mut b).unwrap_err();
    assert_eq!(e.to_string(), format!("with_cwd \"{0}/nope\": stat {0}/nope: no such file or directory", dir.display()));
    let e = i.run_ops(&[block("with_cwd", json!({"path": "f"}), vec![])], &mut b).unwrap_err();
    assert_eq!(e.to_string(), format!("with_cwd \"{}/f\": not a directory", dir.display()));
    let e = i.run_ops(&[block("with_cwd", json!({}), vec![])], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "with_cwd: missing path");
    assert!(buf.contents().contains("done"));
}

#[test]
fn sandbox_masks_gate_ops() {
    let mut handlers = all_handlers();
    apply_mask_gating(&mut handlers);
    let mut i = Interpreter::new(handlers, Program::default());
    let buf = SharedBuf::new();
    i.stdout = buf.writer();
    i.stderr = buf.writer();
    let mut b = Bindings::new(".");
    let e = i
        .run_ops(&[block("sandbox", json!({"flags": "no_shell"}), vec![op("shell", json!({"cmd": "echo hi"}))])], &mut b)
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "op \"shell\" forbidden by sandbox (no-shell scope) — narrow the body or move the call outside the sandbox block"
    );
    assert!(b.cap_mask.is_none(), "mask must be popped on exit");
    let e = i.run_ops(&[block("sandbox", json!({"flags": "bogus"}), vec![])], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "sandbox: unknown flag \"bogus\"");
    if cfg!(unix) {
        let e = i
            .run_ops(&[block("sandbox", json!({"allow_bin": "echo"}), vec![op("shell", json!({"cmd": "ls /"}))])], &mut b)
            .unwrap_err();
        assert_eq!(e.to_string(), "shell binary \"ls\" forbidden by sandbox allow_bin");
        i.run_ops(&[block("sandbox", json!({"allow_bin": "echo"}), vec![op("shell", json!({"cmd": "echo fine"}))])], &mut b).unwrap();
    }
}

#[test]
fn restrictions_replace_handlers() {
    let mut handlers = all_handlers();
    apply_restrictions(&mut handlers, &Restrictions { no_shell: true, ..Default::default() });
    let i = Interpreter::new(handlers, Program::default());
    let mut b = Bindings::new(".");
    let e = i.run_ops(&[op("shell", json!({"cmd": "echo"}))], &mut b).unwrap_err();
    assert!(e.to_string().starts_with("op \"shell\" is disabled by --no-shell"));
}

#[test]
fn try_catch_populates_bindings() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    let mut catch = op("_catch", json!({"bind": "e"}));
    catch.args.insert("bind".into(), Value::String("e".into()));
    let body = vec![op("fail", json!({"msg": "boom"})), catch, print("${e.kind}|${e.message}|${e}")];
    i.run_ops(&[block("try", json!({}), body)], &mut b).unwrap();
    assert_eq!(buf.contents(), "user_fail|boom|boom\n");
    // Catch-body error wins; finally errors win over everything.
    let body = vec![op("fail", json!({"msg": "a"})), op("_catch", json!({})), op("fail", json!({"msg": "b"})), op("_finally", json!({})), print("fin")];
    let e = i.run_ops(&[block("try", json!({}), body)], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "user_fail: b");
    assert!(buf.contents().contains("fin"));
    let e = i.run_ops(&[op("_catch", json!({}))], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "unclassified: catch is only valid inside a try block");
}

#[test]
fn match_picks_case_or_else() {
    let (i, buf) = interp(Program::default());
    let mut b = Bindings::new(".");
    let mk = |t: &str| {
        block(
            "match",
            json!({"target": t}),
            vec![op("_case", json!({"value": "a"})), print("A"), op("_case", json!({"value": "b"})), print("B"), op("_else", json!({})), print("E")],
        )
    };
    i.run_ops(&[mk("b"), mk("zzz")], &mut b).unwrap();
    assert_eq!(buf.contents(), "B\nE\n");
}

#[test]
fn run_op_calls_command_with_cli_args() {
    let mut cmds = BTreeMap::new();
    cmds.insert(
        "greet".to_string(),
        Command {
            name: "greet".into(),
            args: vec![perch_domain::ArgSpec { name: "who".into(), ty: "string".into(), has_default: true, default: json!("w"), ..Default::default() }],
            ops: vec![print("hello ${who}")],
            ..Default::default()
        },
    );
    let (i, buf) = interp(Program { commands: cmds, ..Default::default() });
    let mut b = Bindings::new(".");
    b.set("who", "w");
    i.run_ops(&[op("run", json!({"target": "greet", "_0": "-who=team"}))], &mut b).unwrap();
    assert_eq!(buf.contents(), "hello team\n");
    let e = i.run_ops(&[op("run", json!({"target": "nope"}))], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "run: unknown command \"nope\"");
    let e = i.run_ops(&[op("run", json!({}))], &mut b).unwrap_err();
    assert_eq!(e.to_string(), "run: missing target");
}

#[test]
fn list_commands_skips_private() {
    let mut cmds = BTreeMap::new();
    let mut c = |name: &str, desc: &str, private: bool| {
        let mut cmd = Command { name: name.into(), description: desc.into(), ..Default::default() };
        cmd.modifiers.private = private;
        cmds.insert(name.to_string(), cmd);
    };
    c("build", "Build it", false);
    c("bare", "", false);
    c("hidden", "x", true);
    let (i, buf) = interp(Program { commands: cmds, ..Default::default() });
    let mut b = Bindings::new(".");
    i.run_ops(&[op("list_commands", json!({}))], &mut b).unwrap();
    assert_eq!(buf.contents(), "  bare\n  build                Build it\n");
}

// ── assertions ────────────────────────────────────────────────────────────

fn assert_err(o: Op) -> String {
    run(&[o]).1.unwrap_err().to_string()
}

#[test]
fn assertion_messages() {
    assert_eq!(assert_err(op("assert_eq", json!({"_0": "a", "_1": "b"}))), "assert_eq failed: expected \"b\", got \"a\"");
    assert_eq!(assert_err(op("assert_neq", json!({"_0": "a", "_1": "a"}))), "assert_neq failed: value should not be \"a\"");
    assert_eq!(assert_err(op("assert_contains", json!({"_0": "abc", "_1": "z"}))), "assert_contains failed: \"z\" not found in \"abc\"");
    assert_eq!(
        assert_err(op("assert_not_contains", json!({"_0": "abc", "_1": "b"}))),
        "assert_not_contains failed: \"b\" unexpectedly found in \"abc\""
    );
    assert_eq!(assert_err(op("assert_exists", json!({"_0": "/no/such/thing"}))), "assert_exists failed: \"/no/such/thing\" does not exist");
    assert_eq!(assert_err(op("assert_not_exists", json!({"_0": "/"}))), "assert_not_exists failed: \"/\" exists but shouldn't");
    assert_eq!(assert_err(op("assert_match", json!({"_0": "abc", "_1": "^z"}))), "assert_match failed: \"abc\" did not match /^z/");
    assert!(assert_err(op("assert_match", json!({"_0": "abc", "_1": "("}))).starts_with("assert_match: invalid regex \"(\": "));
    let (_, r, _) = run(&[
        op("assert_eq", json!({"_0": "a", "_1": "a"})),
        op("assert_match", json!({"_0": "v1.2.3", "_1": "^v[0-9]+\\.[0-9]+\\.[0-9]+$"})),
        op("assert_exists", json!({"_0": "/"})),
        op("assert_not_exists", json!({"_0": "/no/such/thing"})),
    ]);
    assert!(r.is_ok());
    let long = "x".repeat(300);
    let msg = assert_err(op("assert_contains", json!({"_0": long, "_1": "z"})));
    assert!(msg.contains(&format!("{}...\"", "x".repeat(117))), "{msg}");
}

// ── system ────────────────────────────────────────────────────────────────

fn value_of(o: Op) -> Value {
    let (_, r, b) = run(&[capture(o, "v")]);
    r.unwrap();
    b.vars["v"].clone()
}

#[test]
fn system_facts() {
    assert_eq!(value_of(op("get_os", json!({}))), json!(perch_interpreter::go_os()));
    assert_eq!(value_of(op("get_arch", json!({}))), json!(perch_interpreter::go_arch()));
    assert_eq!(value_of(op("pid", json!({}))), json!(std::process::id()));
    assert_eq!(value_of(op("path_sep", json!({}))), json!(if cfg!(windows) { "\\" } else { "/" }));
    assert_eq!(value_of(op("null_device", json!({}))), json!(if cfg!(windows) { "NUL" } else { "/dev/null" }));
    assert_eq!(value_of(op("exe_ext", json!({}))), json!(if cfg!(windows) { ".exe" } else { "" }));
    assert!(value_of(op("cpu_count", json!({}))).as_i64().unwrap() >= 1);
    assert_eq!(value_of(op("cwd", json!({}))), json!(std::env::current_dir().unwrap().to_string_lossy()));
    assert_eq!(value_of(op("script_path", json!({}))), json!(""));
    assert_eq!(value_of(op("script_dir", json!({}))), json!(""));
}

#[test]
fn env_ops_round_trip() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let name = "PERCH_OPS_TEST_ENV_ROUNDTRIP";
    std::env::remove_var(name);
    assert_eq!(value_of(op("env_has", json!({"_0": name}))), json!(false));
    assert_eq!(value_of(op("env_default", json!({"_0": name, "_1": "dflt"}))), json!("dflt"));
    run(&[op("set_env", json!({"_0": name, "_1": "val"}))]).1.unwrap();
    assert_eq!(value_of(op("get_env", json!({"_0": name}))), json!("val"));
    assert_eq!(value_of(op("env_has", json!({"_0": name}))), json!(true));
    assert_eq!(value_of(op("env_default", json!({"_0": name, "_1": "dflt"}))), json!("val"));
    run(&[op("unset_env", json!({"_0": name}))]).1.unwrap();
    assert_eq!(value_of(op("get_env", json!({"_0": name}))), json!(""));
    assert_eq!(run(&[op("set_env", json!({"_0": "A=B", "_1": "x"}))]).1.unwrap_err().to_string(), "setenv: invalid argument");
}

// ── install ───────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn install_probes() {
    assert_eq!(value_of(op("has_bin", json!({"_0": "sh"}))), json!(true));
    assert_eq!(value_of(op("has_bin", json!({"_0": "definitely-not-a-bin-xyz"}))), json!(false));
    assert_eq!(value_of(op("which", json!({"_0": "definitely-not-a-bin-xyz"}))), json!(""));
    assert!(value_of(op("which", json!({"_0": "sh"}))).as_str().unwrap().ends_with("/sh"));
    assert_eq!(value_of(op("path_contains", json!({"_0": "/definitely/not/on/path"}))), json!(false));
    assert_eq!(value_of(op("bin_version", json!({"_0": "definitely-not-a-bin-xyz"}))), json!(""));
    assert_eq!(run(&[op("add_to_path", json!({}))]).1.unwrap_err().to_string(), "add_to_path: dir is required");
    assert_eq!(run(&[op("pkg_install", json!({}))]).1.unwrap_err().to_string(), "pkg_install: name is required");
    assert_eq!(run(&[op("link_into_path", json!({"_0": "x"}))]).1.unwrap_err().to_string(), "link_into_path: src and dir are required");
}

#[cfg(unix)]
#[test]
fn link_into_path_symlinks() {
    let dir = tmpdir();
    let src = dir.join("tool");
    std::fs::write(&src, "x").unwrap();
    let dest_dir = dir.join("bin");
    let v = value_of(op("link_into_path", json!({"src": src.to_string_lossy(), "dir": dest_dir.to_string_lossy()})));
    let dst = dest_dir.join("tool");
    assert_eq!(v, json!(dst.to_string_lossy()));
    assert_eq!(std::fs::read_link(&dst).unwrap(), src);
    // Re-linking replaces the old link.
    run(&[op("link_into_path", json!({"_0": src.to_string_lossy(), "_1": dest_dir.to_string_lossy()}))]).1.unwrap();
}

// ── cache ─────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn cache_replays_bindings_on_hit() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tmpdir();
    let counter = home.join("count");
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CACHE_HOME", home.join("xdg"));
    let key = format!("k-{}", std::process::id());
    let ops = vec![block(
        "cache",
        json!({"key": key, "ttl": "1h"}),
        vec![
            op("shell", json!({"cmd": format!("echo run >> {}", counter.display())})),
            capture(op("shell_output", json!({"cmd": "echo computed"})), "result"),
        ],
    )];
    let (out1, r, b1) = run(&ops);
    r.unwrap();
    assert_eq!(b1.vars["result"], json!("computed"));
    assert!(!out1.contains("cache hit"));
    let (out2, r, b2) = run(&ops);
    r.unwrap();
    assert_eq!(b2.vars["result"], json!("computed"), "binding must be replayed");
    assert!(out2.contains(&format!("↪ cache hit: {key} (replayed 1 bindings, ")), "{out2}");
    assert_eq!(std::fs::read_to_string(&counter).unwrap().lines().count(), 1, "body must not re-run on a hit");

    // Missing key and bad TTL errors.
    assert_eq!(run(&[block("cache", json!({}), vec![])]).1.unwrap_err().to_string(), "cache: missing key (first positional arg)");
    let e = run(&[block("cache", json!({"key": "k", "ttl": "zz"}), vec![])]).1.unwrap_err();
    assert_eq!(e.to_string(), "cache: invalid ttl \"zz\": time: invalid duration \"zz\"");
    // Expired entries are misses.
    let ops = vec![block("cache", json!({"key": format!("{key}-exp"), "ttl": "0s"}), vec![op("shell", json!({"cmd": format!("echo run >> {}", counter.display())}))])];
    run(&ops).1.unwrap();
    run(&ops).1.unwrap();
    assert_eq!(std::fs::read_to_string(&counter).unwrap().lines().count(), 3);
}

// Group A and group B must not register the same op kind (Go's registration
// order would silently pick the later one).
#[test]
fn builtin_kinds_are_sorted_unique_and_public() {
    let kinds = perch_ops::builtin_kinds();
    let mut sorted = kinds.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(kinds, sorted);
    assert!(kinds.iter().all(|k| !k.starts_with('_')));
    for k in ["print", "shell", "exec", "if", "try", "cache", "assert_eq", "which", "write_file"] {
        assert!(kinds.iter().any(|x| x == k), "missing {k}");
    }
    assert!(!kinds.iter().any(|k| k == "_catch"));
}

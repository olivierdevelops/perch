//! wasm_run e2e tests. Uses the prebuilt fixtures under demos/ (built from Go
//! with GOOS=wasip1) and asserts the behavior the Go/wazero implementation
//! produces; hand-assembled modules cover exit codes and the deadline.
use perch_interpreter::{Error, HTTPPolicy, Interpreter, SharedBuf, SharedReader};
use perch_ops::all_handlers;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

mod support;

fn root() -> PathBuf {
    support::portable(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap())
}

fn tmpdir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!("perch-ops-wasm-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&d).unwrap();
    support::portable(d.canonicalize().unwrap())
}

fn run_with_stdin(src: &str, stdin: Option<&str>) -> (String, Option<Error>) {
    run_full(src, stdin, None)
}

fn run_full(src: &str, stdin: Option<&str>, policy: Option<HTTPPolicy>) -> (String, Option<Error>) {
    // Runs share process-global env (PERCH_WASM_CACHE*), so they serialize.
    let _g = env_lock();
    run_inner(src, stdin, policy)
}

fn run_inner(src: &str, stdin: Option<&str>, policy: Option<HTTPPolicy>) -> (String, Option<Error>) {
    let prog = match perch_capyloader::load_from_string(src) {
        Ok(p) => p,
        Err(e) => return (String::new(), Some(e)),
    };
    let mut i = Interpreter::new(all_handlers(), prog);
    let buf = SharedBuf::new();
    i.stdout = buf.writer();
    i.stderr = buf.writer();
    i.http_policy = policy;
    if let Some(s) = stdin {
        i.stdin = SharedReader::new(Box::new(std::io::Cursor::new(s.as_bytes().to_vec())));
    }
    let res = i.run("t", &[]);
    (buf.contents(), res.err())
}

fn run(src: &str) -> (String, Option<Error>) {
    run_with_stdin(src, None)
}

/// Wraps `body` in a command, re-indenting one op per line. Block openers are
/// `with_env` / `timeout` / `parallel`, and `wasm_run` when its next line is a
/// body op (a bodyless `wasm_run "m.wasm"` takes no `end`); `end` closes one.
fn cmd(body: &str) -> String {
    // Zero-ambient: wasm mounts/hosts must be declared. The default manifest
    // grants the repo, the temp dir and loopback; T-37/T-38 use `cmd_body`
    // with their own narrower blocks.
    let tmp = std::env::temp_dir();
    let mut reads = vec![root(), tmp.clone()];
    if let Ok(c) = tmp.canonicalize() {
        reads.push(c);
    }
    let mut req = String::from("requires\n    host \"127.0.0.1\"\n");
    for r in &reads {
        req.push_str(&format!("    read \"{0}\"\n    write \"{0}\"\n", r.display()));
    }
    req.push_str("end\n");
    format!("{req}{}", cmd_body(body))
}

fn cmd_body(body: &str) -> String {
    let lines: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut depth = 2;
    let mut out = String::from("name \"x\"\ncommand t\n    do\n");
    for (n, line) in lines.iter().enumerate() {
        if *line == "end" {
            depth -= 1;
        }
        out.push_str(&"    ".repeat(depth));
        out.push_str(line);
        out.push('\n');
        let kw = line.split_whitespace().next().unwrap_or("");
        let next_is_body = lines.get(n + 1).is_some_and(|l| *l != "end" && !l.starts_with("wasm_run"));
        if matches!(kw, "with_env" | "timeout" | "parallel") || (kw == "wasm_run" && next_is_body) {
            depth += 1;
        }
    }
    out.push_str("    end\nend\n");
    out
}

fn e(err: &Option<Error>) -> String {
    err.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "<nil>".into())
}

/// `_start` that calls proc_exit(code).
fn exit_module(code: u8) -> Vec<u8> {
    let mut m = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    m.extend([0x01, 0x08, 0x02, 0x60, 0x01, 0x7f, 0x00, 0x60, 0x00, 0x00]);
    m.extend([0x02, 0x24, 0x01, 0x16]);
    m.extend(b"wasi_snapshot_preview1");
    m.extend([0x09]);
    m.extend(b"proc_exit");
    m.extend([0x00, 0x00]);
    m.extend([0x03, 0x02, 0x01, 0x01]);
    m.extend([0x05, 0x03, 0x01, 0x00, 0x01]);
    m.extend([0x07, 0x13, 0x02, 0x06]);
    m.extend(b"_start");
    m.extend([0x00, 0x01, 0x06]);
    m.extend(b"memory");
    m.extend([0x02, 0x00]);
    m.extend([0x0a, 0x08, 0x01, 0x06, 0x00, 0x41, code, 0x10, 0x00, 0x0b]);
    m
}

/// `_start` that loops forever.
fn spin_module() -> Vec<u8> {
    let mut m = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    m.extend([0x01, 0x04, 0x01, 0x60, 0x00, 0x00]);
    m.extend([0x03, 0x02, 0x01, 0x00]);
    m.extend([0x07, 0x0a, 0x01, 0x06]);
    m.extend(b"_start");
    m.extend([0x00, 0x00]);
    m.extend([0x0a, 0x09, 0x01, 0x07, 0x00, 0x03, 0x40, 0x0c, 0x00, 0x0b, 0x0b]);
    m
}

const HELLO: &str = "demos/wasm-hello/hello.wasm";

#[test]
fn hello_minimal_sees_only_argv() {
    let src = cmd(&format!("wasm_run \"{}/{HELLO}\"\n wasm_arg \"no-caps\"\n end", root().display()));
    let (out, err) = run(&src);
    assert!(err.is_none(), "run: {}", e(&err));
    let want = "── hello.wasm ────────────────────────────────\n\
argv: [hello.wasm no-caps]\n\
env GREETING: (not visible — not in allowlist)\n\
env HOME: (not visible — not in allowlist)\n\
env SECRET: (not visible — not in allowlist)\n\
env PATH: (not visible — not in allowlist)\n\
fs /ro/src: (not visible — not mounted)\n\
fs /rw/bin: (not visible — not mounted)\n\
fs /etc/passwd: (not visible — not mounted)\n";
    assert_eq!(out, want);
}

#[test]
fn hello_env_allowlist_and_mounts() {
    let d = tmpdir();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::create_dir_all(d.join("bin")).unwrap();
    let src = cmd(&format!(
        "with_env \"GREETING=hi,SECRET=s\"\n wasm_run \"{}/{HELLO}\"\n wasm_arg \"alice\"\n wasm_env \"GREETING, HOME\"\n wasm_mount_read \"{d}/src\"\n wasm_mount_write \"{d}/bin\"\n end\n end",
        root().display(),
        d = d.display()
    ));
    let (out, err) = run(&src);
    assert!(err.is_none(), "run: {}", e(&err));
    assert!(out.contains("argv: [hello.wasm alice]"), "{out}");
    assert!(out.contains("env GREETING: hi"), "{out}");
    assert!(out.contains("env HOME: "), "{out}");
    assert!(!out.contains("env HOME: (not visible"), "{out}");
    assert!(out.contains("env SECRET: (not visible"), "{out}");
    assert!(out.contains("fs /ro/src: visible"), "{out}");
    assert!(out.contains("fs /rw/bin: visible"), "{out}");
    assert!(out.contains("fs /etc/passwd: (not visible"), "{out}");
}

#[test]
fn policy_check_good_and_bad() {
    let deploy = root().join("demos/wasm-policy-check/deploy");
    let wasm = root().join("demos/wasm-policy-check/policy-check.wasm");
    let good = cmd(&format!(
        "wasm_run \"{}\"\n wasm_arg \"/ro/deploy/api-good.yaml\"\n wasm_mount_read \"{}\"\n end",
        wasm.display(),
        deploy.display()
    ));
    let (out, err) = run(&good);
    assert!(err.is_none(), "good: {} out={out}", e(&err));
    let bad = cmd(&format!(
        "wasm_run \"{}\"\n wasm_arg \"/ro/deploy/api-bad.yaml\"\n wasm_mount_read \"{}\"\n end",
        wasm.display(),
        deploy.display()
    ));
    let (out, err) = run(&bad);
    let msg = e(&err);
    assert!(msg.starts_with("wasm_module_exited: wasm_run \""), "bad: {msg} out={out}");
    assert!(msg.ends_with("policy-check.wasm\": module closed with exit_code(1)"), "bad: {msg}");
}

#[test]
fn schema_validator_reads_mounts() {
    let base = root().join("demos/wasm-schema-validator");
    let src = cmd(&format!(
        "wasm_run \"{b}/schema-validator.wasm\"\n wasm_arg \"/ro/schemas/user.json\"\n wasm_arg \"/ro/fixtures/good-alice.json\"\n wasm_mount_read \"{b}/schemas\"\n wasm_mount_read \"{b}/fixtures\"\n end",
        b = base.display()
    ));
    let (out, err) = run(&src);
    assert!(err.is_none(), "{} out={out}", e(&err));
    assert!(out.contains("valid"), "{out}");
}

#[test]
fn plugin_runs_and_evil_is_contained() {
    let base = root().join("demos/wasm-plugin-host");
    let mk = |n: &str| {
        cmd(&format!(
            "wasm_run \"{b}/plugins/{n}.wasm\"\n wasm_mount_read \"{b}/data\"\n end",
            b = base.display()
        ))
    };
    let (out, err) = run(&mk("tax"));
    assert!(err.is_none(), "{} out={out}", e(&err));
    assert!(out.contains("\"plugin\": \"tax\"") && out.contains("\"total\": 54.989"), "{out}");
    for n in ["discount", "format", "shipping"] {
        let (out, err) = run(&mk(n));
        assert!(err.is_none(), "{n}: {} out={out}", e(&err));
    }
    let (out, err) = run(&mk("evil"));
    assert!(err.is_none(), "{} out={out}", e(&err));
    assert!(out.contains("open /etc/passwd: Bad file number"), "{out}");
    assert_eq!(out.matches("✓ DENIED").count(), 5, "{out}");
}

#[test]
fn stdout_and_stderr_use_interpreter_sinks_and_parallel_shares_cache() {
    let src = cmd(&format!(
        "parallel\n wasm_run \"{r}/{HELLO}\"\n wasm_arg \"a\"\n end\n wasm_run \"{r}/{HELLO}\"\n wasm_arg \"b\"\n end\n end",
        r = root().display()
    ));
    let (out, err) = run(&src);
    assert!(err.is_none(), "{}", e(&err));
    assert_eq!(out.matches("── hello.wasm").count(), 2, "{out}");
}

#[test]
fn exit_codes() {
    let d = tmpdir();
    std::fs::write(d.join("ok.wasm"), exit_module(0)).unwrap();
    std::fs::write(d.join("bad.wasm"), exit_module(3)).unwrap();
    let (_, err) = run(&cmd(&format!("wasm_run \"{}/ok.wasm\"", d.display())));
    assert!(err.is_none(), "{}", e(&err));
    let (_, err) = run(&cmd(&format!("wasm_run \"{}/bad.wasm\"", d.display())));
    assert_eq!(e(&err), format!("wasm_module_exited: wasm_run \"{}/bad.wasm\": module closed with exit_code(3)", d.display()));
}

#[test]
fn deadline_interrupts_a_spinning_module() {
    let d = tmpdir();
    std::fs::write(d.join("spin.wasm"), spin_module()).unwrap();
    let g = env_lock();
    let start = std::time::Instant::now();
    let (_, err) = run_inner(&cmd(&format!("timeout \"300ms\"\n wasm_run \"{}/spin.wasm\"\n end", d.display())), None, None);
    drop(g);
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
    assert!(e(&err).contains("deadline exceeded"), "{}", e(&err));
}

#[test]
fn error_paths() {
    let d = tmpdir();
    let (_, err) = run(&cmd(&format!("wasm_run \"{}/nope.wasm\"", d.display())));
    assert_eq!(
        e(&err),
        format!("wasm_compile_failed: wasm_run: module \"{d}/nope.wasm\": stat {d}/nope.wasm: no such file or directory", d = d.display())
    );

    std::fs::write(d.join("junk.wasm"), b"not wasm at all").unwrap();
    let (_, err) = run(&cmd(&format!("wasm_run \"{}/junk.wasm\"", d.display())));
    assert!(e(&err).starts_with(&format!("wasm_compile_failed: wasm_run: compile \"{}/junk.wasm\": ", d.display())), "{}", e(&err));

    let (_, err) = run(&cmd("wasm_arg \"x\""));
    assert_eq!(e(&err), "wasm_arg is only valid inside a wasm_run or wasm_bundle block");

    let (_, err) = run(&cmd(&format!("wasm_run \"{}/{HELLO}\"\n print \"hi\"\n end", root().display())));
    assert!(e(&err).contains("\"print\" is not valid inside a wasm_run block"), "{}", e(&err));

    // A bare identifier that isn't a declared bundle alias is a host path.
    let (_, err) = run(&cmd("wasm_run ghost"));
    assert!(e(&err).starts_with("wasm_compile_failed: wasm_run: module \"ghost\": stat "), "{}", e(&err));

    // Mount of a directory that doesn't exist fails the run.
    let (_, err) = run(&cmd(&format!(
        "wasm_run \"{}/{HELLO}\"\n wasm_mount_read \"{}/missing\"\n end",
        root().display(),
        d.display()
    )));
    assert!(e(&err).starts_with("wasm_compile_failed: wasm_run \""), "{}", e(&err));
}

#[test]
fn bundle_alias_resolution_errors() {
    use perch_interpreter::Bindings;
    let src = "name \"x\"\nbundle\n    include \"./hello.wasm\" as pol\nend\ncommand t\n    do\n        print \"hi\"\n    end\nend\n";
    let prog = perch_capyloader::load_from_string(src).expect("load");
    let i = Interpreter::new(all_handlers(), prog);
    let h = i.handlers["wasm_run"].clone();
    let call = |name: &str| {
        let mut map = serde_json::Map::new();
        map.insert("path".into(), name.into());
        map.insert("_alias".into(), true.into());
        let mut b = Bindings::default();
        h(&i, &mut b, &perch_interpreter::Args { map, body: &[] }).err().map(|e| e.to_string()).unwrap_or_default()
    };
    let m = call("nope");
    assert!(m.starts_with("wasm_compile_failed: wasm_run: \"nope\" is not a declared bundle alias (add `include"), "{m}");
    let m = call("pol");
    assert_eq!(
        m,
        "wasm_compile_failed: wasm_run pol: alias resolves to \"hello.wasm\" but this binary has no embedded bundle (build with `perch --build`)"
    );
}

/// `_start` calls `perch.http_get(url)` and exits 1 when the host refused
/// (handle < 0), 0 otherwise.
fn http_module(url: &str) -> Vec<u8> {
    let n = url.len() as u8;
    assert!(n < 128);
    let mut m = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    m.extend([0x01, 0x0e, 0x03, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, 0x60, 0x01, 0x7f, 0x00, 0x60, 0x00, 0x00]);
    m.extend([0x02, 0x35, 0x02, 0x05]);
    m.extend(b"perch");
    m.push(0x08);
    m.extend(b"http_get");
    m.extend([0x00, 0x00, 0x16]);
    m.extend(b"wasi_snapshot_preview1");
    m.push(0x09);
    m.extend(b"proc_exit");
    m.extend([0x00, 0x01]);
    m.extend([0x03, 0x02, 0x01, 0x02]);
    m.extend([0x05, 0x03, 0x01, 0x00, 0x01]);
    m.extend([0x07, 0x13, 0x02, 0x06]);
    m.extend(b"_start");
    m.extend([0x00, 0x02, 0x06]);
    m.extend(b"memory");
    m.extend([0x02, 0x00]);
    m.extend([0x0a, 0x0f, 0x01, 0x0d, 0x00, 0x41, 0x00, 0x41, n, 0x10, 0x00, 0x41, 0x00, 0x48, 0x10, 0x01, 0x0b]);
    m.extend([0x0b, 6 + n, 0x01, 0x00, 0x41, 0x00, 0x0b, n]);
    m.extend(url.as_bytes());
    m
}

/// One-shot local HTTP server; returns (port, requests-served counter).
fn serve() -> (u16, std::sync::Arc<std::sync::atomic::AtomicU32>) {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let hits = std::sync::Arc::new(AtomicU32::new(0));
    let h = hits.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { break };
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            h.fetch_add(1, Ordering::SeqCst);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        }
    });
    (port, hits)
}

fn permissive(allowed: &[&str]) -> Option<HTTPPolicy> {
    Some(HTTPPolicy {
        max_redirects: 5,
        allow_private_ips: true,
        allow_scheme_downgrade: false,
        allowed_hosts: allowed.iter().map(|s| s.to_string()).collect(),
    })
}

#[test]
fn http_bridge_policy() {
    let d = tmpdir();
    let (port, hits) = serve();
    std::fs::write(d.join("h.wasm"), http_module(&format!("http://127.0.0.1:{port}/x"))).unwrap();
    let plain = cmd(&format!("wasm_run \"{}/h.wasm\"", d.display()));
    let allowed = cmd(&format!("wasm_run \"{}/h.wasm\"\n wasm_allow_host \"127.0.0.1\"\n end", d.display()));
    let refused = |err: &Option<Error>| {
        e(err).starts_with("wasm_http_refused: wasm_run \"") && e(err).contains("module closed with exit_code(1)")
    };

    // No wasm_allow_host: no network, server never hit.
    let (_, err) = run_full(&plain, None, permissive(&[]));
    assert!(refused(&err), "{}", e(&err));
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    // Declared host but default outer policy: SSRF guard refuses loopback.
    let (_, err) = run_full(&allowed, None, None);
    assert!(refused(&err), "{}", e(&err));
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    // Declared host, outer --allow-host forbids it: empty intersection.
    let (_, err) = run_full(&allowed, None, permissive(&["other.example.com"]));
    assert!(refused(&err), "{}", e(&err));
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    // Declared host + private IPs permitted: request goes through.
    let (_, err) = run_full(&allowed, None, permissive(&[]));
    assert!(err.is_none(), "{}", e(&err));
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Intersection containing the host: also allowed.
    let (_, err) = run_full(&allowed, None, permissive(&["127.0.0.1"]));
    assert!(err.is_none(), "{}", e(&err));
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

// ---- F02: persistent compile cache (e2e; unit tests live in wasm_cache.rs) ----

/// Serializes tests that point PERCH_WASM_CACHE_DIR / PERCH_WASM_CACHE at a
/// scratch location (process-global env).
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

fn cwasm_files(d: &Path) -> usize {
    std::fs::read_dir(d).map(|r| r.filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "cwasm")).count()).unwrap_or(0)
}

#[test]
fn t35_t36_disk_cache_written_switchable_and_read_only_safe() {
    let _g = env_lock();
    let d = tmpdir();
    let cache = d.join("cache");
    std::env::set_var("PERCH_WASM_CACHE_DIR", &cache);
    // Distinct module bytes so the in-process cache can't have seen them.
    let run_exit = |code: u8, name: &str| {
        // Unique trailing custom section: bytes no other test has compiled.
        let mut m = exit_module(code);
        let cname = format!("u{code}{name}");
        let payload = std::process::id().to_le_bytes();
        m.extend([0x00, (1 + cname.len() + payload.len()) as u8, cname.len() as u8]);
        m.extend(cname.as_bytes());
        m.extend(payload);
        std::fs::write(d.join(name), m).unwrap();
        run_inner(&cmd(&format!("wasm_run \"{}/{name}\"", d.display())), None, None).1
    };
    // Written on first use.
    assert!(run_exit(0, "a.wasm").is_none());
    assert_eq!(cwasm_files(&cache), 1);
    // PERCH_WASM_CACHE=off: no entry written, run still correct.
    std::env::set_var("PERCH_WASM_CACHE", "off");
    let err = run_exit(7, "b.wasm");
    std::env::remove_var("PERCH_WASM_CACHE");
    assert!(e(&err).contains("exit_code(7)"), "{}", e(&err));
    assert_eq!(cwasm_files(&cache), 1);
    // Read-only cache dir: execution unaffected (POSIX permission bits only).
    #[cfg(unix)]
    {
        let ro = d.join("ro");
        std::fs::create_dir_all(&ro).unwrap();
        let mut perm = std::fs::metadata(&ro).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o555);
        std::fs::set_permissions(&ro, perm).unwrap();
        std::env::set_var("PERCH_WASM_CACHE_DIR", ro.join("sub"));
        let err = run_exit(9, "c.wasm");
        std::env::remove_var("PERCH_WASM_CACHE_DIR");
        assert!(e(&err).contains("exit_code(9)"), "{}", e(&err));
    }
}

// ---- F03 / F04: gating and typed kinds ----

fn with_requires(req: &str, body: &str) -> String {
    let c = cmd_body(body);
    format!("requires\n{req}\nend\n{c}")
}

#[test]
fn t37_mounts_gated_by_requires_roots() {
    let d = tmpdir();
    std::fs::create_dir_all(d.join("ok")).unwrap();
    std::fs::create_dir_all(d.join("other")).unwrap();
    let hello = format!("{}/{HELLO}", root().display());
    let req = format!("    read \"{d}/ok\"\n    write \"{d}/ok\"", d = d.display());
    // Inside the declared roots: allowed.
    let src = with_requires(&req, &format!("wasm_run \"{hello}\"\n wasm_mount_read \"{d}/ok\"\n wasm_mount_write \"{d}/ok\"\n end", d = d.display()));
    let (out, err) = run(&src);
    assert!(err.is_none(), "{} out={out}", e(&err));
    // Read mount outside every root: refused before the module runs.
    let src = with_requires(&req, &format!("wasm_run \"{hello}\"\n wasm_mount_read \"{d}/other\"\n end", d = d.display()));
    let (out, err) = run(&src);
    assert!(e(&err).starts_with("wasm_capability_denied: wasm_mount_read: read of "), "{}", e(&err));
    assert!(out.is_empty(), "module must not have run: {out}");
    // Write mount outside the write roots.
    let req_ro = format!("    read \"{d}/ok\"", d = d.display());
    let src = with_requires(&req_ro, &format!("wasm_run \"{hello}\"\n wasm_mount_write \"{d}/ok\"\n end", d = d.display()));
    let (_, err) = run(&src);
    assert!(e(&err).starts_with("wasm_capability_denied: wasm_mount_write: write to "), "{}", e(&err));
    // A read-only root still allows a read mount.
    let src = with_requires(&req_ro, &format!("wasm_run \"{hello}\"\n wasm_mount_read \"{d}/ok\"\n end", d = d.display()));
    let (_, err) = run(&src);
    assert!(err.is_none(), "{}", e(&err));
}

#[test]
fn t38_allow_host_gated_by_requires_hosts() {
    let d = tmpdir();
    std::fs::write(d.join("h.wasm"), http_module("http://127.0.0.1:1/x")).unwrap();
    let body = format!("wasm_run \"{}/h.wasm\"\n wasm_allow_host \"evil.example.com\"\n end", d.display());
    let (_, err) = run(&with_requires("    host \"api.example.com\"", &body));
    assert!(e(&err).starts_with("wasm_capability_denied: wasm_allow_host: host \"evil.example.com\" is not declared"), "{}", e(&err));
    // Declared host passes the gate (the module then exits 1: SSRF refusal).
    let body = format!("wasm_run \"{}/h.wasm\"\n wasm_allow_host \"api.example.com\"\n end", d.display());
    let (_, err) = run(&with_requires("    host \"api.example.com\"", &body));
    assert!(e(&err).starts_with("wasm_http_refused: "), "{}", e(&err));
    // Wildcard declaration covers a subdomain.
    let body = format!("wasm_run \"{}/h.wasm\"\n wasm_allow_host \"a.example.com\"\n end", d.display());
    let (_, err) = run(&with_requires("    host \"*.example.com\"", &body));
    assert!(!e(&err).starts_with("wasm_capability_denied"), "{}", e(&err));
    // No requires block = empty manifest (zero ambient): denied, like every other op.
    let (_, err) = run(&cmd_body(&body));
    assert!(e(&err).starts_with("wasm_capability_denied: wasm_allow_host:"), "{}", e(&err));
}

#[test]
fn t39_typed_kinds() {
    let d = tmpdir();
    std::fs::write(d.join("junk.wasm"), b"not wasm").unwrap();
    std::fs::write(d.join("bad.wasm"), exit_module(2)).unwrap();
    std::fs::write(d.join("spin.wasm"), spin_module()).unwrap();
    let kind = |src: String| e(&run(&src).1).split(':').next().unwrap().to_string();
    assert_eq!(kind(cmd(&format!("wasm_run \"{}/junk.wasm\"", d.display()))), "wasm_compile_failed");
    assert_eq!(kind(cmd(&format!("wasm_run \"{}/missing.wasm\"", d.display()))), "wasm_compile_failed");
    assert_eq!(kind(cmd(&format!("wasm_run \"{}/bad.wasm\"", d.display()))), "wasm_module_exited");
    assert_eq!(kind(cmd(&format!("timeout \"200ms\"\n wasm_run \"{}/spin.wasm\"\n end", d.display()))), "wasm_module_exited");
    let denied = with_requires("    read \"/nonexistent-root\"", &format!("wasm_run \"{}/bad.wasm\"\n wasm_mount_read \"{}\"\n end", d.display(), d.display()));
    assert_eq!(kind(denied), "wasm_capability_denied");
    // http refusal: SSRF guard blocks loopback, module exits 1.
    std::fs::write(d.join("h.wasm"), http_module("http://127.0.0.1:1/x")).unwrap();
    let src = cmd(&format!("wasm_run \"{}/h.wasm\"\n wasm_allow_host \"127.0.0.1\"\n end", d.display()));
    let (_, err) = run_full(&src, None, None);
    let m = e(&err);
    assert!(m.starts_with("wasm_http_refused: wasm_run \""), "{m}");
}

// ---- F03 leftover: CLI --no-write / --no-network reach the wasm marker ops ----

fn run_restricted(src: &str, no_write: bool, no_network: bool) -> (String, Option<Error>) {
    let _g = env_lock();
    let prog = perch_capyloader::load_from_string(src).expect("load");
    let mut i = Interpreter::new(all_handlers(), prog);
    let buf = SharedBuf::new();
    i.stdout = buf.writer();
    i.stderr = buf.writer();
    i.restrict_no_write = no_write;
    i.restrict_no_network = no_network;
    let res = i.run("t", &[]);
    (buf.contents(), res.err())
}

#[test]
fn f03_no_write_blocks_wasm_mount_write() {
    let d = tmpdir();
    std::fs::create_dir_all(d.join("ok")).unwrap();
    let hello = format!("{}/{HELLO}", root().display());
    let req = format!("    read \"{d}/ok\"\n    write \"{d}/ok\"", d = d.display());
    let rw = with_requires(&req, &format!("wasm_run \"{hello}\"\n wasm_mount_write \"{d}/ok\"\n end", d = d.display()));
    let (_, err) = run_restricted(&rw, false, false);
    assert!(err.is_none(), "{}", e(&err));
    let (out, err) = run_restricted(&rw, true, false);
    assert!(e(&err).starts_with("wasm_capability_denied: wasm_mount_write "), "{}", e(&err));
    assert!(e(&err).contains("--no-write"), "{}", e(&err));
    assert!(out.is_empty(), "module must not have run: {out}");
    // A read mount is unaffected by --no-write.
    let ro = with_requires(&req, &format!("wasm_run \"{hello}\"\n wasm_mount_read \"{d}/ok\"\n end", d = d.display()));
    let (_, err) = run_restricted(&ro, true, false);
    assert!(err.is_none(), "{}", e(&err));
}

#[test]
fn f03_no_network_blocks_wasm_allow_host() {
    let d = tmpdir();
    std::fs::write(d.join("h.wasm"), http_module("http://127.0.0.1:1/x")).unwrap();
    let body = format!("wasm_run \"{}/h.wasm\"\n wasm_allow_host \"api.example.com\"\n end", d.display());
    let src = with_requires("    host \"api.example.com\"", &body);
    let (_, err) = run_restricted(&src, false, true);
    assert!(e(&err).starts_with("wasm_capability_denied: wasm_allow_host "), "{}", e(&err));
    assert!(e(&err).contains("--no-network"), "{}", e(&err));
    let (_, err) = run_restricted(&src, false, false);
    assert!(!e(&err).starts_with("wasm_capability_denied"), "{}", e(&err));
}

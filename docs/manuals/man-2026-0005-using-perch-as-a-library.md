---
document_id: MAN-2026-0005
title: "Using perch as a library - Runtime, Policy and RunResult"
document_type: manual
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [orchestrator, runtime library, policy]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [Rust developers embedding perch, tool authors]
scope: "Task chapter for requirement R04 of PROP-2026-0002 - build a Runtime from a Policy, load a program, run a command, read a structured RunResult, with the real constraints (shared process state, cwd, timeouts, error-kind mapping)."
reason: "A host program should run `.perch` commands in-process under an explicit policy instead of shelling out and parsing output."
related_documents: [MAN-2026-0001, PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [manual, library, embedding, policy, rust]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Using perch as a library - Runtime, Policy and RunResult

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** the `perch` crate (orchestrator): `lib.rs`, `policy.rs`, `runtime.rs`
> **Verified:** the complete example below was compiled and run as a separate scratch crate depending on the `perch` crate by path, on the `rust-port` branch (0.1.1). The crate's doctest and `orchestrator/tests/runtime.rs` (8 tests) pass.

## Summary

The `perch` package ships a library next to its binary. The binary is now a one-line entry (`perch::run_cli()`); the same wiring is available to your program:

- `Policy`: plain data describing what a run may do (capability switches, env and bin allowlists, HTTP limits, wall-clock budget, working directory, where stdout/stderr/stdin go).
- `Runtime`: a policy plus the op registry built from it. `Runtime::new(policy)`.
- `Loaded`: a parsed program. `rt.load_path(path)` or `rt.load_str(src)`. It can list its commands, run `check` (what `--check` reports) and `scan` (what `--scan --json` reports).
- `RunResult`: `ok`, `stdout`, `stderr`, `error: Option<RunError>`, `duration`. `RunError` has `kind`, `message`, `op`, `code`, `detail`.

## When to use it

Use the library when you are a Rust program that wants to run perch commands under a fixed policy, in-process, with a structured result: an agent runtime, a launcher, a test harness. If you only need a one-off, shelling out to `perch --scan --json` ([MAN-2026-0002](man-2026-0002-structured-scan.md)) or `perch <cmd>` is simpler. The CLI behaviour is unchanged: the binary is built on this library.

## Setup

In the host crate's `Cargo.toml`:

```toml
[dependencies]
perch = { path = "../perch/orchestrator" }
# or, once the branch is on the default branch (not run for this manual - needs the network):
# perch = { git = "https://github.com/olivierdevelops/perch" }
```

The package is named `perch` and its library target is also `perch`. It is not published to crates.io (out of scope for 0.2.0). The package pulls in wasmtime, so the first build is slow.

## Journey overview

```text
Policy::default().no_network(true).max_runtime(..)      one value per posture
        |
        v
Runtime::new(policy) --load_str / load_path--> Loaded --check()/scan()/commands()
        |                                         |
        +------------ run(&loaded, "cmd", &args) -+--> RunResult { ok, stdout, stderr, error, duration }
```

## A complete runnable example

`Cargo.toml` (as above) and `src/main.rs`:

```rust
use perch::{Policy, Runtime};
use std::time::Duration;

const SRC: &str = r#"
requires
    bin "sh"
    host "example.com"
end

command greet
    description "Say hello"
    arg name
        type string
        default "world"
    end
    do
        print "hello ${name}"
    end
end

command fetch
    description "Fetch a page"
    do
        http_get "https://example.com/"
    end
end

command boom
    description "Always fails"
    do
        fail "something broke"
    end
end

command nap
    description "Sleeps, then prints"
    do
        sleep 1
        print "unreachable"
    end
end
"#;

fn main() {
    // A runtime carries its policy. Two runtimes, two postures.
    let strict = Runtime::new(Policy::default().no_network(true).max_runtime(Duration::from_millis(200)));
    let open = Runtime::new(Policy::default());

    let prog = strict.load_str(SRC).expect("program loads");
    println!("commands: {:?}", prog.commands().iter().map(|c| c.name.as_str()).collect::<Vec<_>>());
    for issue in prog.check() {
        println!("check: {issue}");
    }
    println!("scan risk: {}", prog.scan().risk);

    let r = strict.run(&prog, "greet", &["-name=perch".into()]);
    println!("greet: ok={} stdout={:?} stderr={:?}", r.ok, r.stdout, r.stderr);

    let r = strict.run(&prog, "fetch", &[]);
    let e = r.error.as_ref().unwrap();
    println!("fetch: ok={} kind={} op={:?} msg={:?}", r.ok, e.kind, e.op, e.message);

    let r = open.run(&prog, "boom", &[]);
    let e = r.error.as_ref().unwrap();
    println!("boom:  ok={} kind={} msg={:?}", r.ok, e.kind, e.message);

    let r = strict.run(&prog, "nap", &[]);
    let e = r.error.as_ref().unwrap();
    println!("nap:   ok={} kind={} elapsed>=1s: {}", r.ok, e.kind, r.duration >= Duration::from_secs(1));

    let r = open.run(&prog, "nope", &[]);
    println!("nope:  kind={}", r.error.unwrap().kind);

    match open.load_str("command x\n  do\n    sh -c 'echo'\n  end\nend\n") {
        Ok(_) => println!("load: ok"),
        Err(e) => println!("load error: {e}"),
    }
}
```

Output (`cargo run`, verified):

```text
commands: ["boom", "fetch", "greet", "nap"]
scan risk: low
greet: ok=true stdout="hello perch\n" stderr=""
fetch: ok=false kind=cap_network_denied op="http_get" msg="op \"http_get\" is disabled by --no-network — run `perch help --no-network` for details"
boom:  ok=false kind=user_fail msg="something broke"
nap:   ok=false kind=timeout_exceeded elapsed>=1s: true
nope:  kind=command_not_found
load error: bin_not_declared: command x line 0: `sh` is not a known op and not declared in `requires` (add `bin "sh"` to the requires block to run it) — did you mean the op `as`?
```

What it shows:

- `greet`: arguments are passed exactly as on the command line (`-name=perch`).
- `fetch`: the policy denied a network op before it ran; the error kind is `cap_network_denied`.
- `boom`: an ordinary failure has the op's kind (`user_fail`).
- `nap`: see [timeouts](#timeouts-do-not-interrupt-a-running-op).
- `nope`: an unknown command is `command_not_found` (unless the program has a `catch` block, which handles it).
- Loading never runs anything; load errors come back as `LoadError { message }`.

The doctest in `orchestrator/src/lib.rs` is the smallest version:

```rust
use perch::{Policy, Runtime};

let rt = Runtime::new(Policy::default().no_shell(true).no_network(true));
let prog = rt
    .load_str("command hello\n    do\n        print \"hi\"\n    end\nend\n")
    .unwrap();
let res = rt.run(&prog, "hello", &[]);
assert!(res.ok);
assert_eq!(res.stdout, "hi\n");
```

## API reference

### `Policy` (all fields are public; `Policy::default()` is the CLI's default posture)

| Field / builder | Type | Default | Effect |
|---|---|---|---|
| `no_shell(bool)` | bool | false | Deny `shell` ops (`cap_shell_denied`). |
| `no_subprocess(bool)` | bool | false | Deny process-spawning ops (`cap_subprocess_denied`). |
| `no_network(bool)` | bool | false | Deny network ops (`cap_network_denied`). |
| `no_write(bool)` | bool | false | Deny filesystem-mutating ops (`cap_write_denied`). |
| `deny_all()` | builder | | All four of the above. |
| `env_allow(iter)` | `Option<BTreeSet<String>>` | `None` (whole environment) | Only these host env names are visible; empty set hides all. |
| `allowed_bins(iter)` | `Option<BTreeSet<String>>` | `None` | `shell` may run only these binaries. |
| `no_shell_metachars(bool)` | bool | false | Refuse shell strings with pipes, `;`, `&&`, ... |
| `http(HttpPolicy)` | `Option<HttpPolicy>` | secure defaults | `max_redirects` (5), `allow_private_ips` (false), `allow_scheme_downgrade` (false), `allowed_hosts` (empty = any public host). |
| `max_runtime(Duration)` | `Option<Duration>` | `None` | Wall-clock budget for one `run`. |
| `allow_advisory_scopes(bool)` | bool | false | The library form of `--allow-advisory-scopes` ([MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md)). |
| `working_dir(path)` | `Option<PathBuf>` | process cwd | Directory relative paths and subprocesses start in, per run. The process cwd is not changed. |
| `stdout(Output)`, `stderr(Output)` | `Capture` / `Inherit` / `Discard` | `Capture` | Where output goes. With `Capture` it is returned in `RunResult`; with `Inherit` the result field stays empty. |
| `stdin(Input)` | `Empty` / `Inherit` / `Bytes(Vec<u8>)` | `Empty` | Where stdin comes from. |

`Policy` is plain data: `Clone`, `PartialEq`, `Debug`. The CLI builds its runs from the same type, so the two cannot drift.

### `Runtime` and `Loaded`

| Item | Meaning |
|---|---|
| `Runtime::new(Policy) -> Runtime` | Builds the op registry for that policy. |
| `rt.policy() -> &Policy` | The policy in force. |
| `rt.load_path(&Path) -> Result<Loaded, LoadError>` | Load a file; imports resolve relative to it. |
| `rt.load_str(&str) -> Result<Loaded, LoadError>` | Load source text. `import` lines resolve against the process cwd (verified: `import "./imp/lib.perch"` works from the directory that has `imp/`, and fails with `open ./imp/lib.perch: no such file or directory` from another). |
| `rt.run(&Loaded, &str, &[String]) -> RunResult` | Run one command. Never panics the host: an interpreter panic becomes an `unclassified` error. |
| `Loaded::commands() -> Vec<CommandInfo>` | Public commands (private and test commands are hidden), sorted, with `ArgInfo { name, ty, description, optional }`. |
| `Loaded::check() -> Vec<Issue>` | The `--check` findings (`severity`, `where_`, `message`). Empty means clean. |
| `Loaded::scan() -> JsonReport` | The structure behind `--scan --json` ([MAN-2026-0002](man-2026-0002-structured-scan.md)). |
| `Loaded::program()`, `Loaded::source()` | The parsed `perch_domain::Program`, and the file path (`None` for `load_str`). |

`Loaded` is cheap to clone and safe to share between threads.

### `RunResult` and `RunError`

`RunError.kind` is the identifier from [errors.md](../errors.md) (`user_fail`, `shell_exit_nonzero`, `cap_network_denied`, `timeout_exceeded`, ...), plus two library-level kinds: `command_not_found` and `unclassified`. `op` names the failing op when known; `code` and `detail` carry the exit code or extra context.

## Concurrency

Each `run` builds a fresh interpreter, so runtimes do not share interpreter state. Two runtimes with different policies can run on different threads. Verified with two threads and two working directories:

```text
("w1", true, "id=one", None)
("w2", true, "id=two", None)
process cwd unchanged: true
```

(Each thread builds `Runtime::new(Policy::default().working_dir(dir))` and runs a command that reads `id.txt`.)

## What is still process-wide

"No global state" holds for policies and interpreter state, not for the operating-system process. Know these before sharing one process between mutually distrustful callers:

- **Caches initialised once and then read-only:** the compiled capy grammar, op-vocabulary tables, the wasmtime engine, the TLS root store, and the kernel-confinement probe.
- **The wasm compile cache on disk** (`PERCH_WASM_CACHE_DIR`, [MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md)) is shared by every runtime in the process and by other perch processes.
- **The process environment is shared and mutable.** `set_env` / `export` / `unset_env` change the real process environment, so one runtime can change what another reads. Verified: runtime A runs `set_env "PERCH_LEAK" "from-runtime-A"`; runtime B (policy `env_allow(["PERCH_LEAK"])`) then reads `PERCH_LEAK=from-runtime-A`. `Policy::env_allow` limits what a runtime can see, not what another runtime has written.
- **Some ops resolve relative paths against the process cwd, not `Policy::working_dir`.** Verified with `working_dir` set to a temp directory: `cwd` returned that directory, while `path_abs "foo"` returned `<process cwd>/foo`. File ops, `cwd` and spawned binaries honour `working_dir` (the first two verified above; spawned binaries from the source). `path_abs` is the one verified exception; other ops built on the same absolute-path helper (`go_abs` in the ops crate) may behave the same. Use absolute paths if it matters.
- **Embedded bundles** (`perch_ops::set_bundle`) are only for fat binaries built with `--build` and are never touched by `Runtime`.

## Timeouts do not interrupt a running op

`max_runtime` is checked before each op starts. An op that is already running (a `sleep`, a long process, a download) runs to completion; the next op then fails with `timeout_exceeded`. In the example, `nap` sleeps its full second even though the budget is 200 ms, and `print "unreachable"` never runs. A host that needs a hard stop must run perch in a separate process or thread it can abandon.

## Capability-denial kinds are mapped by the library

The interpreter reports a `--no-shell` / `--no-network` / `--no-write` / `--no-subprocess` denial as a plain error whose text is `op "X" is disabled by --no-Y ...`, not as a tagged error. On the CLI, `${err.kind}` inside `rescue` is therefore `unclassified` (verified: `perch --no-shell ...` with `rescue` printing `kind=unclassified`). `Runtime::run` recognises that message and reports the matching `cap_shell_denied`, `cap_subprocess_denied`, `cap_network_denied` or `cap_write_denied`, with `op` taken from the message. This text mapping is a workaround for a known issue (F10 in the 0.2.0 validation notes): do not depend on the `message` wording, and expect the CLI's `unclassified` to change when the ops tag these errors themselves.

## Expected result and side effects

- `load_*` reads files and runs no ops.
- `run` executes ops with their real side effects (files, processes, network) subject to the policy and the program's own `requires`. A program with no `requires` block is an empty manifest: no bins, writes or hosts are allowed.

## Errors and recovery

| Symptom | Cause | Recovery |
|---|---|---|
| `LoadError` | Parse error, unknown op, undeclared bin. | Fix the source; run `Loaded::check` on a loadable program for the rest. |
| `kind == "command_not_found"` | No such public command and no `catch`. | List with `Loaded::commands()`. |
| `kind == "cap_*_denied"` | The policy forbids the op. | Widen the policy, or remove the op. |
| `kind == "timeout_exceeded"` | Budget exhausted at an op boundary. | Raise `max_runtime`; see above. |
| `kind == "unclassified"` | An error from an op that has no kind yet, or an interpreter panic. | Inspect `message`. |

## Limitations

- Not published on crates.io; the API is 0.x and may change between minor versions.
- Interpreter state is per run, but the process-wide items above are shared.
- No cancellation handle, no streaming of output while a run is in progress (use `Output::Inherit` to stream to the host's own stdout/stderr).
- C ABI or wasm build of the library is out of scope for 0.2.0.

## Version applicability

| Feature | Introduced | Notes |
|---|---|---|
| `perch` library target (`Runtime`, `Policy`, `Loaded`, `RunResult`, `RunError`) | 0.2.0 | The CLI binary is a thin entry over it. |
| `perch::run_cli`, `perch::VERSION`, `perch::DEFAULT_COMMANDS_FILE` | 0.2.0 | Used by the binary. |

## Related documents

[MAN-2026-0001](man-2026-0001-perch-manual-index.md) · [embedding.md](../embedding.md) · [errors.md](../errors.md) · [SYS-2026-0001](../system/sys-2026-0001-rust-workspace.md) · [ARCH-2026-0001](../architecture/arch-2026-0001-layer-boundaries.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · Verified demo: DEMO-2026-0001 (planned at `docs/demos/demo-2026-0001-perch-0-2-0-verification.md`).

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial chapter for R04. |

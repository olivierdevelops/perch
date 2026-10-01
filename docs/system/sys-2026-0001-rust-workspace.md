---
document_id: SYS-2026-0001
title: "Perch Rust workspace - the implemented system"
document_type: system
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [domain, infra, usecases, io, orchestrator, cmd]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [engineers, maintainers, reviewers]
scope: "The current implemented Rust workspace - crates, dependency graph, data flow of a command run, the wasmtime runtime, confinement, caches and process-global state, external dependencies, runtime configuration and version sources."
reason: "The Go-to-Rust port changed the implementation language and layout; the standard (section 31) requires system documentation of what is actually implemented."
related_documents: [PLAN-2026-0001, PROP-2026-0002, ARCH-2026-0001, MAN-2026-0001, REF-2026-0003]
supersedes: null
superseded_by: null
tags: [system, rust, workspace, crates, runtime]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Perch Rust workspace - the implemented system

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** every crate in the Cargo workspace
> **Verified against:** the `rust-port` branch source and build (`Cargo.toml` workspace version 0.1.1; the 0.2.0 bump happens at release). Crate dependencies come from `cargo metadata`; behaviour statements were checked by running the binary where noted.

## Summary

Perch is one Cargo workspace. Each VHCO folder is a set of crates, one crate per package. Cargo's dependency graph, not a convention, decides what may call what: the `perch` crate in `orchestrator/` is the only crate that depends on all the others. It builds three programs: `perch` (CLI plus a library target), `perch-lsp` and `perch-mcp`.

The product behaviour is documented in the manual ([MAN-2026-0001](../manuals/man-2026-0001-perch-manual-index.md)). This document describes how it is built. The layering rules and where the code departs from them are in [ARCH-2026-0001](../architecture/arch-2026-0001-layer-boundaries.md).

## Responsibilities

- Load a `.perch` file (or an embedded program) into a typed `Program`.
- Enforce the file's `requires` manifest and the operator's capability flags while running it.
- Run commands through an op-dispatching interpreter; expose them through a CLI, REPL, HTTP UI, MCP and a library.
- Inspect programs without running them (`--check`, `--scan`, `simulate`, `--dry-run`).
- Build fat binaries, run tests, serve an editor protocol.

## Boundaries and non-responsibilities

- No process supervision, scheduling, package hosting or multi-user authentication.
- Perch is not itself sandboxed; it confines the processes it spawns (macOS, Linux) and gates its own ops.
- No network service other than the optional local `--server` UI and the stdio protocols of `perch-lsp` and `perch-mcp`.

## Architecture

### Crates

Workspace members from `Cargo.toml` (31 crates). Descriptions are the crate-level documentation of each `lib.rs` / `main.rs`.

| Folder | Crate (package) | Role |
|---|---|---|
| `domain/` | `perch-domain` | Pure data: `Program`, `Command`, `Op`, `Requirements`, `ArgSpec`, `Modifiers`, `Hook`, `Bundle`, `ErrorKind`, `OpError`. Depends on nothing in the workspace. |
| `infra/capyloader` | `perch-capyloader` | Compiles a `.perch` source into a `Program`: runs the embedded `lib.capy` grammar through the `capy-core` engine, stream-parses its NDJSON events, folds block markers into nested ops, lowers command-level `finally` and env prefixes, enforces static `requires` checks. |
| `infra/interpreter` | `perch-interpreter` | Walks a `Program`; holds bindings, the handler registry, deadline, hooks, IO sinks, HTTP policy, confinement flags. Knows no concrete op. |
| `infra/ops` | `perch-ops` | Every built-in op handler (about 200), the `requires` preflight and gates, capability restrictions, process spawning, the `confine` module, the wasmtime runtime, HTTP, caches. |
| `infra/audit` | `perch-audit` | NDJSON trace of every dispatched op (`--audit`). |
| `infra/report` | `perch-report` | Span tree renderer (`--report`) and live tracer (`--trace`). |
| `infra/preview` | `perch-preview` | `--dry-run` and `--ask` before-op hooks. |
| `infra/embed` | `perch-embed` | Fat-binary footer: embed and load a program and file bundle. |
| `infra/httpserver` | `perch-httpserver` | The `--server` UI and its JSON/NDJSON endpoints. |
| `infra/opcatalog` | `perch-opcatalog` | The JSON op catalog behind `--export`. |
| `infra/vscodeext` | `perch-vscodeext` | The VS Code extension files embedded in the binary. |
| `usecases/*` | `perch-runcommand`, `-listcommands`, `-commandhelp`, `-help`, `-initconfig`, `-runbuild`, `-runserver`, `-runshell`, `-runtests`, `-validate`, `-scan`, `-simulate`, `-importsh`, `-installlsp`, `-installvscode`, `-exportopscatalog` | One crate per CLI operation. Each defines an `Impl` struct whose fields are closures (load, run, fetch, ...) supplied by the orchestrator. |
| `io/cli` | `perch-cli` | Parses argv into an intent, owns the help text and the use-case traits, dispatches. |
| `orchestrator/` | `perch` | Wiring and the library facade (`Runtime`, `Policy`); the `perch` binary is `perch::run_cli()`. |
| `cmd/perch-lsp` | `perch-lsp` | Language Server Protocol server (JSON-RPC over stdio): diagnostics, completion, hover, outline. |
| `cmd/perch-mcp` | `perch-mcp` | MCP server over stdio: tools `perch_list` and `perch_run`. |

Outside the workspace: `editors/` (VS Code extension, tree-sitter grammar), `wasm-sdk/` (module-side SDKs), `recipes/`, `demos/`, `docs/`, `scripts/`, `Formula/`, `skills/`.

### Dependency graph (from `cargo metadata`, normal dependencies)

```text
                         +---------------------------+
                         | perch  (orchestrator)     |  depends on every crate below
                         +-------------+-------------+
                                       |
        +------------------+-----------+-------------+------------------+
        v                  v                         v                  v
  io/cli            usecases/*                 infra/httpserver     cmd/perch-lsp, cmd/perch-mcp
  (domain)       (domain; validate and             |  (domain, interpreter,   (capyloader, domain, ops,
                  runshell also interpreter;       |   ops, scan, simulate,    validate | interpreter)
                  exportopscatalog -> opcatalog;   |   validate)
                  installvscode -> vscodeext)      |
                                                   v
  infra/ops ----------> infra/interpreter ----> domain <---- infra/capyloader
     |                                              ^              ^
     +-----------------------------------------------+-------------+  (ops -> capyloader is declared but used by tests only)
  infra/audit, infra/report, infra/preview  -> interpreter, domain
  infra/opcatalog -> capyloader, interpreter, domain
  infra/embed, infra/vscodeext -> domain
```

Every crate except `perch-domain` depends on `perch-domain`. Notable edges: `perch-ops` depends on `perch-interpreter` (and declares `perch-capyloader`, which only its tests use); `perch-httpserver` (an infra crate) depends on three use-case crates; `perch-validate` and `perch-runshell` (use cases) depend on `perch-interpreter`. These are discussed in ARCH-2026-0001.

### Data flow of a command run

```text
argv
 |  orchestrator::run
 |    extract global flags (--no-*, --env, --allow-bin, --http/redirect flags, --audit, --report/--trace,
 |    --max-runtime, --dry-run/--ask, --allow-advisory-scopes); stdin programs get the strictest defaults
 |    perch_embed::load()  -> embedded Program, or none
 v
 build_cli: Policy -> handler registry (all_handlers + restrictions + mask gating); UseCases wired with closures
 |
 v
 perch_cli::Cli::run_args -> classify argv (flag, subcommand, or command name) -> use case
 |
 v  runcommand::Impl::execute(file, command, args)
 |    load closure = perch_capyloader::load(path)
 |      lib.capy via capy-core -> NDJSON events -> Program
 |      imports resolved, templates inlined, requires normalised (no block = empty manifest),
 |      static checks (undeclared bin, env prefix placement), `finally` lowered to try markers
 v
 Interpreter::new(handlers, program)  + Policy::configure (preflight hook, allowlists, HTTP policy,
 |                                     advisory flag, restrictions, working dir) + deadline + audit/tracer
 v
 Interpreter::run(command, args)
 |    platform check -> preflight (bins exist / hash pins / required env / os / arch)
 |    parse args into bindings -> run_ops
 |      run_op: deadline check -> interpolate args -> before-op hook (dry-run / ask)
 |              -> handler (perch-ops) -> after-op hook (audit) / tracer
 v
 handler for a process-spawning op (perch-ops process.rs)
 |    check_exec_bin / check_shell (declared bin, --allow-bin, metachars)
 |    env_prefix_overlay (R05: bindings, then gated host env)
 |    confine_spawn: scopes from `requires`; macOS sandbox-exec rewrite | Linux Landlock pre_exec |
 |                   unsupported -> refuse (confinement_unavailable) or advisory banner
 |    scrubbed child env + prefix overlay -> spawn -> capture/stream output
 v
 result -> error text / exit code via perch-cli (exit 1 for failures, 2 for flag misuse)
```

The library path ([MAN-2026-0005](../manuals/man-2026-0005-using-perch-as-a-library.md)) enters at `Policy` and `Runtime::run`, which build the same interpreter configuration through `Policy::handlers` and `Policy::configure`, then call `Interpreter::run`; output goes to buffers instead of the process streams.

### The wasmtime runtime

`wasm_run` (in `infra/ops/src/wasm.rs`) runs WASI Preview 1 modules under wasmtime 40 (Cranelift, no async). One engine and linker are created lazily per process (`OnceLock`) with epoch interruption; a ticker thread advances the epoch every 10 ms so the interpreter's deadline can stop a module. A module sees only what the `wasm_run` body declares: argv, an env allowlist, `/ro/<name>` and `/rw/<name>` preopened directories, and (if `wasm_allow_host` is declared) host imports under the module name `perch` (`http_get` and friends, with the SSRF guard and redirect policy). Mounts and hosts are checked against `requires` and `--no-write` / `--no-network` before instantiation. Compiled modules are cached in memory per process and on disk across processes (see below). Failures are wrapped as `wasm_compile_failed`, `wasm_module_exited`, `wasm_capability_denied`, `wasm_http_refused`.

### Confinement

`infra/ops/src/confine/` exposes `probe()` and `confine(cmd, scopes)` with exactly one backend compiled in: `macos.rs` (rewrites the command to `/usr/bin/sandbox-exec -p <profile> program args`), `linux.rs` (a Landlock ruleset applied in a `pre_exec` hook; target-specific dependency `landlock`), `other.rs` (always unsupported). `process.rs::decide_confinement` turns scopes, the probe result and `--allow-advisory-scopes` into enforce, advisory or refuse. See [MAN-2026-0004](../manuals/man-2026-0004-confining-spawned-binaries.md).

## Interfaces

| Interface | Provided by | Notes |
|---|---|---|
| CLI | `perch` binary | Flags and subcommands in the next section. Exit 0 success, 1 failure, 2 for `--scan` flag errors. |
| Library | `perch` crate (`Runtime`, `Policy`, `Loaded`, `RunResult`, `RunError`, `run_cli`) | [MAN-2026-0005](../manuals/man-2026-0005-using-perch-as-a-library.md) |
| HTTP UI | `perch --server` (`infra/httpserver`) | `GET /`, `GET /api/program`, `POST /api/exec` (NDJSON), `POST /api/check`, `/api/scan`, `/api/simulate`; localhost by default. |
| MCP | `perch-mcp` | Tools `perch_list`, `perch_run`; stdout/stderr returned as the tool result. |
| LSP | `perch-lsp` | didOpen/didChange/didClose, publishDiagnostics, completion, hover, documentSymbol; `serverInfo` currently reports version 0.1.0. |
| Embedded program | fat binary footer (`infra/embed`) | Current footer is 24 bytes, magic `PRCHEMB2`: JSON program, optional gzipped tar bundle, archive length, JSON length, magic. Legacy 16-byte `PRCHEMB1` footers still load. |

## Configuration

### Runtime configuration

Output of `perch --help` is the commands of the file in the current directory; the flag summary is `perch/io/cli::help_text` and `perch help`. Flags this build recognises (from `orchestrator/src/flags.rs` and `io/cli/src/lib.rs`):

| Group | Flags |
|---|---|
| Run and inspect | `-f FILE`, `--help`/`-h`, `--version`, `--init`, `--check`/`--validate`, `--scan [--json \| --format text\|json]`, `simulate` (`--sim-*` flags), `test`/`--test` (`--filter`, `-v`/`--verbose`), `--dry-run`, `--ask`, `--shell`, `--server` (`--host`, `--port`), `--build -o OUT` (`--include`), `--import`, `--export[=PATH]`, `--completions SHELL`, `--install-lsp`, `--install-vscode`, `help [TOPIC] [--json]` |
| Capability policy | `--no-shell`, `--no-network`, `--no-write`, `--no-subprocess`, `--env A,B`, `--allow-bin`, `--no-shell-metachars`, `--allow-host`, `--allow-private-ips`, `--allow-scheme-downgrade`, `--max-redirects`, `--no-redirects`, `--max-runtime SECS`, `--allow-advisory-scopes`, `--allow-shell/-subprocess/-network/-write` and `--trust-stdin` (stdin programs only) |
| Observability | `--audit FILE`, `--report[=PATH]`, `--trace[=PATH]` |

Environment variables read by perch itself (verified list from the source):

| Variable | Used for |
|---|---|
| `PERCH_WASM_CACHE_DIR`, `PERCH_WASM_CACHE` | wasm compile cache location and switch ([MAN-2026-0007](../manuals/man-2026-0007-wasm-cache-and-gating.md)) |
| `HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, `LocalAppData`/`LOCALAPPDATA`, `APPDATA` | locating user cache/config directories |
| `PATH`, `PATHEXT`, `SHELL`, `USER`, `USERNAME`, `COMPUTERNAME` | binary lookup, shell detection, system helper ops |
| `UPDATE_GOLDEN` | test-only: regenerate HTTP server golden pages |
| `PERCH_INSTALL_DIR`, `PERCH_VERSION` | `scripts/install.sh` / `install.ps1` only |

Any variable named in a `.perch` file is read through the gates described in [requires.md](../requires.md); it is not part of perch's own configuration.

## Runtime behaviour

- A run builds a fresh `Interpreter`; commands run on the calling thread. `parallel` blocks use threads with per-branch binding copies.
- The wall-clock deadline is checked before each op, never inside one. A `finally` runs under a fresh 5 s grace deadline when the body timed out ([MAN-2026-0003](../manuals/man-2026-0003-cleanup-with-finally.md)).
- A file without a `requires` block is treated as an empty manifest after load (`normalize_requirements`), so gates apply to every file.
- Errors are `OpError` values with an `ErrorKind` (see [errors.md](../errors.md)); capability denials from `--no-*` are still plain strings (known issue), which the library maps to `cap_*` kinds by message text.

## Data and storage

| Item | Location | Lifetime |
|---|---|---|
| wasm compiled modules | `<user cache dir>/perch/wasm/<sha256>-wt40-epoch1<hash>.cwasm` (`PERCH_WASM_CACHE_DIR` overrides) | persistent, no eviction |
| `cache "KEY" "TTL"` block results | `<user cache dir>/perch/blocks/<sha256(key)>.json` | until TTL |
| `--audit` NDJSON | path given on the command line | operator-managed |
| fat binary | `perch --build -o OUT`: perch executable + program JSON + optional gzipped tar + 24-byte footer | the artifact |
| extracted bundles | a temp directory created on first `bundle_dir`/`bundle_extract` use | process |

No database. Perch writes nothing else on its own.

## Process-global state and caches

All are initialised once (`OnceLock`) and read-only afterwards, except where noted:

| State | Where | Notes |
|---|---|---|
| Compiled capy grammar engine | `capyloader/loader.rs` | |
| Op-vocabulary tables, regexes | `capyloader/enforce.rs`, `opcatalog`, `scan`, `simulate`, `importsh` | |
| wasmtime engine and linker | `ops/wasm.rs` (`RUNTIME`) | One per process. |
| TLS client configuration | `ops/group_b/http.rs` | System roots, webpki fallback. |
| Confinement probe result | `ops/process.rs`, `ops/confine/macos.rs` | One probe per process. |
| Embedded bundle | `ops/group_b/bundle.rs` (`BUNDLE`, a `Mutex`) | Set only by fat binaries via `set_bundle`. |
| Process environment and cwd | the OS | `set_env`/`export`/`unset_env` mutate the real environment; `perch test` changes the process cwd per test; a few path helpers (`path_abs`) use the process cwd. These are the reasons two runtimes in one process are not fully isolated ([MAN-2026-0005](../manuals/man-2026-0005-using-perch-as-a-library.md#what-is-still-process-wide)). |

## Dependencies

| Dependency | Where | Why |
|---|---|---|
| `capy-core` (git, pinned rev `4cc6d32`) | capyloader | The grammar engine that runs `lib.capy`. |
| `wasmtime` / `wasmtime-wasi` 40 | ops | wasm execution (features: cranelift, runtime, std; WASI p1). |
| `ureq` 2, `rustls` 0.23 (ring), `rustls-native-certs`, `webpki-roots` | ops | HTTP with system trust roots and bundled fallback. |
| `landlock` 0.4 | ops, Linux only | Linux confinement. |
| `tokio` (minimal), `async-trait`, `bytes` | ops | wasmtime-wasi plumbing. |
| `serde`, `serde_json` (preserve_order) | all | Program and report serialization. |
| `sha2`, `sha1`, `crc32fast`, `hex`, `base64`, `flate2`, `tar`, `zip`, `regex`, `url`, `percent-encoding`, `chrono`, `libc` | ops, embed, others | Ops (hashing, archives, encoding, time, paths). |

No dependency is fetched at run time. The release build needs network access to fetch crates and the pinned `capy-core` revision.

## Deployment

`cargo build --release -p perch -p perch-lsp -p perch-mcp` per target (`.github/workflows/release.yml`) for six targets: darwin, linux and windows on amd64 and arm64. Assets `perch-<os>-<arch>[.exe]`, `perch-lsp-...`, `perch-mcp-...`, per-file `.sha256`, `checksums.txt`. The dev profile compiles third-party crates at `opt-level = 2` (an unoptimised Cranelift makes a 1.5 MB wasm module take about 25 s to compile); workspace crates keep fast dev builds. The release profile strips symbols.

### Version sources (all must agree at release)

| Source | Current value |
|---|---|
| `Cargo.toml` `[workspace.package] version` | 0.1.1 |
| `orchestrator/src/orchestrator.rs` `VERSION` (printed by `--version`) | 0.1.1 |
| `cmd/perch-lsp/src/main.rs` `serverInfo.version` | 0.1.0 |
| `cmd/perch-mcp/src/main.rs` `SERVER_VERSION` | 0.1.0 |
| `usecases/initconfig/src/lib.rs` template `version` | 0.1.0 (a sample program's version, not perch's) |
| `Formula/perch.rb` `version` | 0.1.0 |
| `editors/vscode-perch/package.json`, `infra/vscodeext/template/package.json`, `editors/tree-sitter-perch/package.json` | 0.1.0 |

The table is a snapshot at the verification commit; the 0.2.0 bump updates it.

## Security boundaries

- **Declared manifest (`requires`)**: gates perch's own ops (bins, hosts, env, read/write paths) before each op, every time.
- **Environment scrubbing**: a spawned child gets the default operational set, declared env, `--env` names, bindings and per-command env, plus an inline prefix overlay; nothing else.
- **Kernel confinement** of spawned binaries to declared `read`/`write` (and best-effort `host`) on macOS and Linux; refuse by default elsewhere.
- **Capability flags** narrow what an invocation may do; they only narrow.
- **HTTP**: SSRF guard on every request and redirect hop, redirect limits, no https-to-http downgrade by default.
- **wasm**: the module boundary is the runtime; capabilities are exactly those declared and permitted by the manifest and flags.
- Not a boundary: perch process itself; `shell` once allowed; matching is by path prefix, not a chroot.

## Observability

`--audit FILE` (NDJSON: session start, one record per op with args, duration and error, session end), `--report` (span tree), `--trace` (live), `--dry-run` and `--ask` (preview), `↪` status lines on stderr for timeout and stop events, a `🔒 security:` banner on stderr when any restriction flag is active.

## Known limitations

- Linux confinement and the Windows code paths have not been run on real systems for this document; they are covered by CI as it exists.
- `--max-runtime` is checked between ops; a deadline that expires inside a body's last op prevents the following `finally` from running ([MAN-2026-0003](../manuals/man-2026-0003-cleanup-with-finally.md#limitations-and-known-issues)).
- Capability-denial errors are unclassified on the CLI.
- The `perch help` catalog (`usecases/help/src/catalog.json`) still describes `--install-lsp` as using `go install` and omits `--scan --json` and `--allow-advisory-scopes`.
- Three stray backup files are tracked in the source tree: `infra/ops/src/group_b/strings.rs-E`, `textlines.rs-E`, `time.rs-E`.
- `perch-lsp` and `perch-mcp` report version 0.1.0.

## Last verified version

Source of the `rust-port` branch with workspace version 0.1.1, on macOS arm64, 2026-10-01.

## Related documents

[ARCH-2026-0001](../architecture/arch-2026-0001-layer-boundaries.md) · [MAN-2026-0001](../manuals/man-2026-0001-perch-manual-index.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [vhco-architecture.md](../../vhco-architecture.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial system document for the Rust workspace as of PLAN-2026-0001. |

---
document_id: MAN-2026-0001
title: "Perch manual - root index and feature catalogue"
document_type: manual
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [CLI, language, interpreter, ops, scan, sandbox, wasm, runtime library, LSP, MCP, web UI, documentation]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [perch authors, operators, tool authors, reviewers]
scope: "Root index of the perch manual - goals, concepts, installation, a feature catalogue that links every product guide and the new 0.2.0 task chapters, configuration, troubleshooting, limitations and version applicability."
reason: "Standard section 30 requires a discoverable, current-state catalogue answering what can I do, why would I use it, and where are the instructions."
related_documents: [PLAN-2026-0001, PROP-2026-0002, MAN-2026-0002, MAN-2026-0003, MAN-2026-0004, MAN-2026-0005, MAN-2026-0006, MAN-2026-0007, SYS-2026-0001, ARCH-2026-0001, REF-2026-0003]
supersedes: null
superseded_by: null
tags: [manual, index, catalogue]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Perch manual - root index and feature catalogue

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later (pre-existing features are marked with the version family they come from)
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** the whole product
> **Verified:** commands in this index were run on the `rust-port` branch build that reports `perch --version` = 0.1.1 (the 0.2.0 bump happens at release). The existing product guides were not re-verified line by line for this index; where this index contradicts one of them, this index was checked against the binary.

## Purpose

This is the entry point to everything a user can do with perch. It orders the manual, states what perch is for and what it is not, and lists every feature with the reason to use it and the page that explains it. The task chapters for what is new in 0.2.0 are separate documents (MAN-2026-0002 to MAN-2026-0007); everything older lives in the existing product guides in `docs/`, which this index links rather than duplicates.

## Reading order

1. [Project goals and boundaries](#project-goals-and-boundaries) and [Concepts](#concepts) (this page).
2. [Installation](#installation-and-setup), then [getting-started.md](../getting-started.md).
3. [The complete guide](../guide.md) for the whole language, or the task pages in the [feature catalogue](#feature-catalogue).
4. [What's new in 0.2.0](#whats-new-in-020) for the task chapters.
5. [Troubleshooting](#troubleshooting), [Limitations](#limitations) and [Version applicability](#version-applicability) when something surprises you.

## What's new in 0.2.0

| Addition | What changes for you | Task chapter |
|---|---|---|
| Structured scan (R01) | `perch --scan --json` prints `declared`, `inferred` and `risk` as separate JSON fields; the text advice no longer tells you to add `--no-write` when you declared a write scope | [MAN-2026-0002](man-2026-0002-structured-scan.md) |
| Command-level `finally` (R02) | `do ... finally ... end`; a failing cleanup no longer hides the original error; cleanup runs when `--max-runtime`/`timeout` fires | [MAN-2026-0003](man-2026-0003-cleanup-with-finally.md) |
| Confinement of spawned binaries (R03) | Declared `read`/`write` (best-effort `host`) bind the binaries perch starts, on macOS and Linux; elsewhere perch refuses unless `--allow-advisory-scopes` | [MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md) |
| Runtime library (R04) | `perch::Runtime`, `Policy`, `Loaded`, `RunResult` for in-process use | [MAN-2026-0005](man-2026-0005-using-perch-as-a-library.md) |
| Inline env prefix (R05) | `KUBECONFIG=$CFG kubectl get pods`; one-process overlay, gated like any env read | [MAN-2026-0006](man-2026-0006-env-prefix.md) |
| wasm cache, gating, typed errors, TLS roots, serve page (F02-F04, F06, F07) | Faster repeat `wasm_run`; mounts and hosts follow `requires`; `wasm_*` error kinds; system certificate store; `perch --server` page no longer truncates | [MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md) |
| `perch --install-lsp` without Go (F05) | Downloads the release asset and verifies its sha256 | [lsp.md](../lsp.md) |

Release history, compatibility and rollback live in the release document, not here. The verified walkthrough of every item is DEMO-2026-0001 (planned at `docs/demos/demo-2026-0001-perch-0-2-0-verification.md`).

## Project goals and boundaries

**Goal.** Perch is a cross-platform command runtime: define, run and ship operational tools in one structured `.perch` file. The same file is a CLI, a REPL, a web UI, an MCP tool surface for agents, a `#!/usr/bin/env perch` script, and (with `--build`) a single portable binary. Perch is written in Rust; a release is a single executable per OS and architecture.

**Safety posture.** Controlled scripting with declared capabilities, not a promise of a perfect sandbox: a file declares what it needs in `requires`; perch gates its own ops against that declaration, scrubs the environment it hands to children, and (0.2.0) confines spawned binaries to declared `read`/`write` scopes where the operating system allows it.

**Non-goals** (unchanged; see the README for the full list): a general-purpose programming language, a CI system, a container or Kubernetes orchestrator, a package manager, a process supervisor (no restart policies, no health checks), a multi-user server, a polyglot runtime.

## Concepts

| Term | Meaning |
|---|---|
| `.perch` file | The program: metadata, top-level bindings, an optional `requires` block, `command` blocks, optional `catch`, `template` and `bundle`. Default name `commands.perch`. |
| command | A named, callable unit with typed arguments and a `do ... end` body. |
| op | A built-in operation used in a body (`print`, `mkdir`, `http_get`, ...). About 200 ops; see [op-reference.md](../op-reference.md) (`perch --export` prints the live catalog: 203 ops and 30 auto-bound variables on this build). |
| declared bin | A binary named in `requires`; a bare `NAME args` line runs it without a shell. |
| `requires` | The file's manifest: bins (with optional hash pins), env, hosts, read and write roots, os, arch. A file with no block is an empty manifest: nothing may be spawned, written or reached until declared. |
| capability flags | Per-invocation policy: `--no-shell`, `--no-network`, `--no-write`, `--no-subprocess`, `--env`, `--allow-bin`, `--allow-host`, `--max-runtime`. Declarations are promises about the program; flags are policy for the run. |
| scan / check / simulate / dry-run | Four ways to inspect a file without running its ops: `--scan` (capabilities and risk), `--check` (static validation), `simulate` (what-if against a hypothetical host), `--dry-run` (the op list). |
| fat binary | `perch --build` output: the perch executable with your program appended. |
| library | The `perch` crate's `Runtime` API (0.2.0), the same engine the CLI uses. |

Mental model:

```text
 .perch file --load--> Program --+--> CLI / REPL / web UI / MCP / script    (interfaces)
   (requires, commands)          |
                                 +--> interpreter --> ops --> perch's own gates
                                                          \--> spawned binaries --> (0.2.0) kernel confinement
```

The implementation view is in [SYS-2026-0001](../system/sys-2026-0001-rust-workspace.md) and [ARCH-2026-0001](../architecture/arch-2026-0001-layer-boundaries.md).

## Installation and setup

### Release binary (recommended)

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.sh | sh
curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.sh | sh -s -- --version v0.2.0
```

`scripts/install.sh` detects `uname -s` (Darwin or Linux) and `uname -m` (x86_64/amd64 or arm64/aarch64), downloads the release asset `perch-<os>-<arch>` and its `.sha256`, verifies the checksum when the file is present, and installs to `PERCH_INSTALL_DIR`, else `/usr/local/bin` (using `sudo` if needed), else `~/.local/bin`. It prints a PATH hint if the directory is not on `PATH`. `--help` prints its usage:

```text
$ sh scripts/install.sh --help
perch installer for macOS and Linux.

Usage:
  curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.sh | sh
  curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.sh | sh -s -- --version v0.1.0

Honors:
  PERCH_INSTALL_DIR    — install destination (default: /usr/local/bin or ~/.local/bin)
  PERCH_VERSION        — version tag to install (default: latest)
```

(Only `--help` and the unknown-argument error were run; the download path needs the network and was not run for this manual.) Windows (PowerShell): `irm https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.ps1 | iex` (honours `PERCH_VERSION`, `PERCH_INSTALL_DIR`; installs `perch-windows-<arch>.exe`; not run).

Release assets are `perch-`, `perch-lsp-` and `perch-mcp-` binaries for six targets (`darwin`, `linux`, `windows` by `amd64`, `arm64`) with `.sha256` files and a `checksums.txt`.

### From source

The workspace builds three programs: `perch` (the CLI and library), `perch-lsp` and `perch-mcp`.

```sh
cargo install --path orchestrator            # from a checkout; installs the `perch` binary
cargo install --path cmd/perch-lsp
cargo install --path cmd/perch-mcp
```

`cargo install --path orchestrator` was run (offline, debug profile) and installed `perch v0.1.1 (executable perch)`; `perch --version` then printed `0.1.1`. From a remote repository the equivalent is `cargo install --git https://github.com/olivierdevelops/perch perch` (the package is named `perch`); it was not run because it needs the network and the branch to be on the default branch. A Rust toolchain is required, and the first build compiles wasmtime, which is slow.

### Check the install

```text
$ perch --version
0.1.1            # prints 0.2.0 on a 0.2.0 build
$ perch --init   # writes a starter commands.perch
$ perch --check
✓ commands.perch: 2 commands, 0 catch, 1 binding — no issues
```

### Editors and agents

`perch --install-lsp` downloads `perch-lsp` for your OS from the latest release, verifies its sha256 against `checksums.txt`, and installs it next to the `perch` executable (or into `~/.local/bin` / `%LOCALAPPDATA%\perch`); no Go toolchain is needed. This download-and-verify path was exercised by the use case's unit tests with a fake server, not against the network. `perch --install-vscode` installs the LSP and the VS Code extension (needs `node`, `npm` and the `code` CLI). See [lsp.md](../lsp.md) and [mcp.md](../mcp.md).

## Feature catalogue

"Since" gives the version family the feature is documented from: `0.1.x` for features that already existed, `0.2.0` for additions in this release. "Demo" points to the release verification demo for new items; pre-existing items are not part of that demo.

| Feature | Why / when to use it | Surfaces | Since | Instructions | Demo |
|---|---|---|---|---|---|
| Scaffold, run, list commands | Start a project and see what a file offers | CLI (`--init`, `--help`, `<cmd> --help`) | 0.1.x | [getting-started.md](../getting-started.md), [guide.md](../guide.md) | n/a |
| The `.perch` language: commands, args, bindings, conditionals, templates, imports | Replace Makefiles and shell glue with a typed file | CLI, LSP | 0.1.x | [language.md](../language.md), [capy-limitations.md](../capy-limitations.md) | n/a |
| Ops catalog (about 200 built-ins) | Cross-platform file, hash, archive, HTTP, string, JSON, path, time ops instead of per-OS shell | CLI | 0.1.x | [op-reference.md](../op-reference.md) (`perch --export` for the live JSON) | n/a |
| Run external tools without a shell: bare declared bins, `exec`, `pipe`, `&&` `\|\|` `;` chains | Shell-like lines with no injection surface | CLI | 0.1.x | [language.md](../language.md#running-external-tools-a-bare-bin-name-exec-shell), [sandboxed-by-design.md](../sandboxed-by-design.md) | n/a |
| Inline `NAME=VALUE` env prefix | One-off env on one call, as in the shell | CLI | **0.2.0** | [MAN-2026-0006](man-2026-0006-env-prefix.md), [migrating-from-shell.md](../migrating-from-shell.md) | DEMO-2026-0001 |
| Execution contexts: `parallel`, `timeout`, `retry`, `with_env`, `with_cwd`, `sandbox`, `cache`, `--report` | Change how a body runs; see a span tree of what ran | CLI | 0.1.x | [execution-contexts.md](../execution-contexts.md) | n/a |
| Error handling: `try / rescue / finally`, `match`, error-kind enum | Handle, branch on, and clean up after failures | CLI | 0.1.x | [errors.md](../errors.md) | n/a |
| Command-level `do ... finally ... end`, non-masking cleanup errors, `finally` on timeout | Always stop what you started, and still see the real error | CLI | **0.2.0** | [MAN-2026-0003](man-2026-0003-cleanup-with-finally.md) | DEMO-2026-0001 |
| `requires` manifest: bins, hash pins, env, hosts, os, arch, read/write roots | Declare what a file needs; refuse everything else | CLI | 0.1.x | [requires.md](../requires.md), [capability-gating.md](../capability-gating.md) | n/a |
| Environment scrubbing for spawned binaries | A tool sees only declared env, never your other secrets | CLI | 0.1.x | [requires.md](../requires.md) | n/a |
| Kernel confinement of spawned binaries; `--allow-advisory-scopes`; `confinement_unavailable` | Hold a server or tool to its declared directories | CLI, library | **0.2.0** | [MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md) | DEMO-2026-0001 |
| Capability flags: `--no-shell`, `--no-network`, `--no-write`, `--no-subprocess`, `--env`, `--allow-bin`, `--allow-host`, `--max-runtime`, HTTP policy flags | Per-invocation policy, independent of the file | CLI, library | 0.1.x | [sandbox.md](../sandbox.md), [capability-gating.md](../capability-gating.md) | n/a |
| Hooks: `before`, `after`, `on_error` around built-in ops | Intercept operations (audit, deny, wrap) | CLI | 0.1.x | [hooks.md](../hooks.md) | n/a |
| Audit trail (`--audit FILE.ndjson`), `--trace`, `--report` | Review exactly which ops ran | CLI | 0.1.x | [execution-contexts.md](../execution-contexts.md), [sandbox.md](../sandbox.md) | n/a |
| `perch --check` | Static validation, including undeclared literal bins, hosts, env | CLI, web UI, LSP | 0.1.x | [requires.md](../requires.md#static-checking-catch-undeclared-use-before-running) | n/a |
| `perch --scan` (text) | Audit what a file needs and its risk; get the tightest invocation | CLI, web UI | 0.1.x | [sandbox.md](../sandbox.md) | n/a |
| `perch --scan --json` | Let a tool compare declared and inferred reach | CLI | **0.2.0** | [MAN-2026-0002](man-2026-0002-structured-scan.md), [trust-by-manifest.md](../trust-by-manifest.md) | DEMO-2026-0001 |
| `perch simulate`, `--dry-run`, `--ask` | Preview without running; what-if on another host | CLI, web UI | 0.1.x | [simulate.md](../simulate.md) | n/a |
| `perch test` and `assert_*` ops | Test commands in a sandboxed temp directory | CLI | 0.1.x | [testing.md](../testing.md) | n/a |
| REPL (`--shell`) | Try ops interactively | CLI | 0.1.x | [guide.md](../guide.md) | n/a |
| Web UI (`--server`) | Run commands from a browser; Simulate / Scan / Check tabs | HTTP | 0.1.x (page fix 0.2.0) | [web-ui.md](../web-ui.md), [MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md#f07-the-serve-page) | DEMO-2026-0001 |
| `--build` fat binary and `bundle` | Ship a tool as one executable with embedded files | CLI | 0.1.x | [embedding.md](../embedding.md) | n/a |
| Runtime library | Run commands in-process under a `Policy`, get a `RunResult` | Rust API | **0.2.0** | [MAN-2026-0005](man-2026-0005-using-perch-as-a-library.md), [embedding.md](../embedding.md) | DEMO-2026-0001 |
| `wasm_run`: constrained lane for untrusted logic | Run third-party or generated code with only the declared capabilities | CLI | 0.1.x | [wasm.md](../wasm.md), [wasm-walkthroughs.md](../wasm-walkthroughs.md) | n/a |
| wasm compile cache, mount/host gating, typed errors | Fast repeat runs; manifest-bound mounts and hosts; branchable failures | CLI | **0.2.0** | [MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md) | DEMO-2026-0001 |
| System TLS roots for `http_*` | Corporate and local CAs work | CLI | **0.2.0** | [MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md#f06-system-tls-roots) | not demonstrated (see chapter) |
| MCP server (`perch-mcp`) | Expose commands as tools to AI agents (`perch_list`, `perch_run`) | MCP | 0.1.x | [mcp.md](../mcp.md), [llm-control-plane.md](../llm-control-plane.md) | n/a |
| Language server (`perch-lsp`), VS Code extension, tree-sitter grammar | Diagnostics, completion, hover, outline; docs for `finally` and env prefix | LSP | 0.1.x (new keyword docs 0.2.0) | [lsp.md](../lsp.md) | n/a |
| Import a shell script (`--import`) | Start migrating `.sh` files | CLI | 0.1.x | [migrating-from-shell.md](../migrating-from-shell.md) | n/a |
| Shell completions, `perch help` | Discoverability | CLI | 0.1.x | [README](../../README.md) | n/a |
| Recipes and tutorials | Ready-to-run files and end-to-end walkthroughs | CLI | 0.1.x | [recipes.md](../recipes.md), [tutorials/](../tutorials/01-replace-your-makefile.md), [applications.md](../applications.md) | n/a |
| Design documents (roadmap, not behaviour) | Where the safety model is heading | n/a | n/a | [sandboxed-by-design.md](../sandboxed-by-design.md), [typed-bins.md](../typed-bins.md), [trust-by-manifest.md](../trust-by-manifest.md), [os-in-a-program.md](../os-in-a-program.md) | n/a |
| FAQ, current-status page | Quick answers | n/a | 0.1.x | [faq.md](../faq.md), [using-perch-today.md](../using-perch-today.md) | n/a |

## Configuration and environment variables

| Name | Kind | Type / values | Default | Scope | Effect | Security notes |
|---|---|---|---|---|---|---|
| `-f PATH` | CLI flag | path or `-` | `commands.perch` | invocation | Which file to load. `-f -` reads stdin and applies the strictest restrictions unless `--trust-stdin` or `--allow-*` | Stdin programs are untrusted by default. |
| `--allow-advisory-scopes` | CLI flag | switch | off | invocation | Run spawned binaries unconfined, with a stderr banner, where confinement is unavailable | Weakens R03; see [MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md). |
| `--scan --json` / `--format json` | CLI flag | switch / `json`, `text` | text | scan | Structured scan output | None. |
| `--env A,B` | CLI flag | name list | not set (declared names plus the default operational set are visible) | invocation | Allowlist of host variables visible to `${NAME}` and prefixes | Narrows, never widens. |
| `--max-runtime SECONDS` | CLI flag | integer | none | invocation | Wall-clock budget checked between ops | Does not interrupt a running op. |
| `PERCH_WASM_CACHE_DIR` | env var | directory | `<user cache dir>/perch/wasm` | process | wasm compile cache location | See [MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md). |
| `PERCH_WASM_CACHE` | env var | `off`, `0`, `false`, `no`, `disabled` | on | process | Disable the wasm compile cache | |
| `PERCH_INSTALL_DIR`, `PERCH_VERSION` | env var (installer) | path, tag | `/usr/local/bin` or `~/.local/bin`; latest | installer | Where and which version `install.sh` installs | The installer verifies the `.sha256` when present. |

The full flag list is in `perch --help` (see the output in [SYS-2026-0001](../system/sys-2026-0001-rust-workspace.md#runtime-configuration)) and in `perch help` (the generated catalog; its entries for `--scan --json`, `--allow-advisory-scopes` and `--install-lsp` have not been updated for 0.2.0, see Troubleshooting).

## Troubleshooting

| Symptom | Likely cause | What to do |
|---|---|---|
| `bin_not_declared` when loading a file that runs a tool | The file has no `requires` entry for it; a file without a `requires` block is an empty manifest | Add `bin "tool"` to `requires`. |
| `write_not_declared` / `read_not_declared` | Op path is outside every declared root, or none are declared | Declare `write "./dir"` / `read "./dir"` ([requires.md](../requires.md)). |
| `env_not_declared` | Host variable read (including in an env prefix) is not declared or not on `--env` | Add `env "NAME"` ([MAN-2026-0006](man-2026-0006-env-prefix.md)). |
| `confinement_unavailable` | The file has scopes and the platform cannot confine | [MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md). |
| A tool fails with `Operation not permitted` under declared scopes (macOS) | It touched a path outside its scopes or the default read allowances | Declare the path, [MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md#errors-and-recovery). |
| A script using `x=tool args` (no spaces) breaks after upgrading | The unspaced form is now an env prefix | Write `x = tool args` ([MAN-2026-0006](man-2026-0006-env-prefix.md#compatibility-note-xtool-args-without-spaces)). |
| `rescue err` fails to load with `bin_not_declared ... \`rescue\`` | The error is always bound as `err`; a name after `rescue` is not accepted | Write a bare `rescue` ([MAN-2026-0003](man-2026-0003-cleanup-with-finally.md#limitations-and-known-issues)). |
| `perch --help` shows a different program | `--help` lists the commands of `commands.perch` in the current directory, not perch's own usage | Run it in an empty directory, or use `perch help`. |
| `perch help --scan` does not mention `--json` | The generated help catalog was not updated | Use [MAN-2026-0002](man-2026-0002-structured-scan.md). |
| `finally` did not run after `--max-runtime` | The deadline expired inside the body's last op | [MAN-2026-0003](man-2026-0003-cleanup-with-finally.md#limitations-and-known-issues). |
| Slow first `wasm_run` | Compile cache cold or disabled | [MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md). |

## Limitations

- Perch gates perch's own ops and (0.2.0) confines spawned binaries to `read`/`write`; it is not a general sandbox. `host` scopes are best effort. Linux confinement was not run on a real kernel for this manual. Windows has no confinement and refuses by default.
- `--max-runtime` and `timeout` are checked between ops, never inside one.
- `shell` remains available and remains a broad capability; prefer declared bins and native ops.
- No cross-compile in `--build`; the output matches the host OS and architecture.
- The library shares some process state between runtimes (environment, a few path helpers); see [MAN-2026-0005](man-2026-0005-using-perch-as-a-library.md#what-is-still-process-wide).
- Several older product guides still contain pre-Rust wording (Go examples, `go install`, wazero). Where one disagrees with this manual or the binary, this manual and the binary win; known stale spots are listed in the release documentation.

## Version applicability

| Interface | Introduced | Changed | Notes |
|---|---|---|---|
| CLI, language, ops, `requires` | 0.1.0 | 0.2.0 | Additions listed in [What's new](#whats-new-in-020). |
| First Rust release | 0.1.1 | | Go tree removed afterwards. |
| `--scan --json`, `finally`, confinement, library, env prefix | 0.2.0 | | |

## Related documents

[PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [SYS-2026-0001](../system/sys-2026-0001-rust-workspace.md) · [ARCH-2026-0001](../architecture/arch-2026-0001-layer-boundaries.md) · [docs index](../index.md) · [document index](../index/document-index.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial manual index for 0.2.0. |

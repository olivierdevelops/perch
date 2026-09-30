---
document_id: PROP-2026-0002
title: Perch as a safe, embeddable layer — structured scan, finally, confinement, runtime library and post-port follow-ups
document_type: proposal
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 2
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [scan, interpreter, language, ops, sandbox, wasm, runtime library, packaging, CI]
affected_versions:
  from: "0.1.1"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [engineers, maintainers, reviewers, tool authors who wrap perch]
scope: Five capabilities requested (structured --scan, finally, confinement of spawned binaries, an embeddable runtime library, inline env prefixes on binary calls) plus the follow-up work deliberately left out of the Go-to-Rust port.
reason: Let a calling tool read what a file reaches, rely on cleanup running, rely on declared scopes being enforced, and drive perch in-process under an explicit policy.
related_documents: [PROP-2026-0001, STD-2026-0001, REF-2026-0004]
supersedes: null
superseded_by: null
tags: [proposal, security, sandbox, scan, language, library, windows, packaging]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Perch as a safe, embeddable layer

> **Status:** Draft · **Created:** 2026-10-01 · **Revision:** 2
> **Owner:** Perch maintainers (proposed; assignment pending)
> **Affected versions:** Written against 0.1.1, the first Rust release.
> **Components:** Scan, interpreter, language, ops, sandbox, wasm, a new runtime library crate, packaging and CI.

## Decision Requested

Adopt the requirements below as the next body of work after the Rust port. They fall in two groups:

- **Requested (R01–R05):** the three gaps and the library request in `.ignore/request.md`, plus inline env prefixes on binary calls (R05, raised in conversation). R01–R04 together let a wrapping tool read what a file can reach, start it, confine it and stop it without trusting it blindly.
- **Follow-ups (F01–F09):** items found during the port and deliberately left out, because the port's job was behavior parity with the Go build and each of these changes behavior or adds a feature.

This document proposes work. It does not claim that anything below is implemented, tested or released.

## Original User Request

| ID | What was asked | Source | Interpretation |
|---|---|---|---|
| UQ-01 | "write a proposal for all the new features you wanted to add and …/.ignore/request.md" | User conversation, 2026-10-01 | Cover the follow-ups raised during the port and every request in request.md. |
| UQ-02 | "also must be able to import perch as a library and use its functions — essentially create a runtime with its policies then exec with it" | Last lines of request.md | A library API where a caller builds a runtime carrying a policy and then executes commands with it. Becomes R04. |
| UQ-04 | "also i want to support ENV=$ENVVAR binary verb --args ..." and, in a follow-up, "so it look more unix shell like insted of with context everywhere" | User conversation, 2026-10-01 | Shell-style inline environment assignment before a declared binary call, with the value taken from a variable. The stated aim is that a perch line reads like the shell line it replaces, rather than needing a `with_env` context block around it. Becomes R05. |
| UQ-03 | "Tested on perch 0.1.0 (macOS, arm64). Each gap has a repro below." | request.md | Evidence for R01–R03 comes from the requester's repros against 0.1.0 (Go). They were not reproduced against 0.1.1. |

Not asked for, and stated as a non-goal in request.md: a supervisor, restart policies or health checks.

## Problem and Evidence

| ID | Problem | Consequence | Evidence and confidence |
|---|---|---|---|
| P-01 | `--scan` prints text only and infers from operations, not from the `requires` manifest | A wrapping tool cannot ask "what can this file reach?" without parsing prose, and cannot compare declared scopes with used ones | request.md: `--scan --json` and `--scan --format json` print the human text. A file declaring `write "./allowed"` whose `shell` call writes is reported as `✗ writes — add --no-write for free`. **Reported, not reproduced by me.** |
| P-02 | Cleanup after a failing command has no command-level form, and a failing `finally` hides the original error | A local service cannot be reliably stopped after a failed run without wrapping the whole body in `try`, and a failing cleanup loses the real cause | request.md asked for a `finally` block and showed `try … rescue` swallowing the failure. **Correction found while planning:** `try … finally … end` already exists (docs/errors.md) and, tested on 0.1.1, runs the cleanup and re-raises the original failure (exit status 1, original message). What is actually missing: (a) a command-level `do … finally … end` form, and (b) when the cleanup also fails, the cleanup error replaces the body error (tested: body `fail "body boom"` + cleanup `fail "cleanup boom"` reports only `cleanup boom`). Perch exits 1 for any failure, so "original exit code" means the original error and kind, not the child's status. **Tested on 0.1.1.** |
| P-03 | `requires read/write/host` gate perch's own ops but not a spawned binary | A declared `sh -c 'echo out > /private/tmp/…'` writes outside the declared scope. A native server is the program whose writes most need confining | request.md repro. The README already lists OS confinement as roadmap, and docs/typed-bins.md surveys the per-OS mechanisms. **Known gap; repro reported.** |
| P-04 | Perch is a binary, not a library | A runtime that wants to load a file under a fixed policy and run commands must shell out and parse output | request.md closing lines. The Rust port already has one crate per layer, so the pieces exist but have no public facade. |
| P-06 | No inline env prefix on a binary call. Env for a spawned binary is set only through the per-command `env` modifier or a `with_env … end` block (docs/language.md) | A one-off `KUBECONFIG=$CFG kubectl get pods` needs a `with_env … end` block or a command-wide setting, so a shell one-liner becomes several lines and the env is separated from the call it belongs to | docs/language.md `with_env` and `env` modifier; no prefix form in the grammar. **Read from docs, not tested against 0.1.1.** |
| P-05 | Items left out of the port (F01–F09 below) | Known gaps ship in 0.1.1 | See each F-item. |

## Goals and Non-Goals

**Goals**

- A caller can obtain a stable, machine-readable description of what a file declares and what it does.
- Cleanup declared by the author runs on every exit path and never changes the original outcome.
- Declared filesystem and (where the OS allows) network scopes bind spawned binaries. Where the OS cannot, perch says so instead of silently running with ambient authority.
- A Rust program can build a runtime with a policy and execute commands in-process.
- Windows reaches the same test bar as Linux and macOS.

**Non-goals**

- A process supervisor, restart policy or health checks (request.md and the README exclude these).
- Changing the `.perch` language beyond `finally`.
- Reopening behavior decisions from PROP-2026-0001; this proposal is additive.

## Requirements

### Requested

| ID | Requirement | Acceptance |
|---|---|---|
| R01 | `--scan --json` emits `declared` (the `requires` manifest: `bin` with hashes, `host`, `env`, `read`, `write`, `os`, `arch`) and `inferred` (capabilities observed in ops) as **separate** fields, plus `risk`. `--format json` is accepted as an alias. Scan's "add `--no-write` for free" advice must account for declared `write` roots. | Golden JSON for a file with declared and undeclared reach; the request.md example produces the documented shape; the human text no longer advises `--no-write` when a write scope is declared and used. |
| R02 | (a) A command-level `finally` section (`do … finally … end`) that is sugar for wrapping the whole body in `try … finally … end`, runs on success, on failure and on `fail`, and re-raises the original error. (b) If a `finally` (command-level or `try`) fails while the body also failed, both errors surface with the **original first**, and the error kind/`${err.*}` seen by an enclosing `rescue` is the original's. The existing `try … finally` behavior and docs/errors.md are otherwise unchanged. | Tests: body fails then finally runs and the original error is re-raised; body succeeds then finally runs; `fail "x"` runs finally; nested `try`/`finally`; both fail and the original is reported first; `--check` validates placement; `try … rescue … finally` unchanged. |
| R03 | Spawned binaries are confined to declared `read`/`write` (and `host` where feasible). macOS: `sandbox-exec` profile. Linux: Landlock. Elsewhere, or on a kernel without support: **fail closed** by refusing to run under a `requires` block that declares scopes, unless the operator passes an explicit opt-out that prints "scopes are advisory". | The request.md repro no longer writes `/private/tmp/perch-escape-test` on macOS and Linux; a Windows run refuses or prints the advisory notice, never silently proceeds; escape-attempt tests per platform. |
| R05 | A declared-bin call accepts leading `NAME=VALUE` assignments before the binary: `ENV=$ENVVAR binary verb --args …`. `VALUE` may be bare, quoted, or `$NAME` / `${NAME}`, resolved like any op arg. The assignments apply to that one spawned process only and do not leak into bindings or later ops. Works for bare calls, `exec`, and captures (`out = KEY=$V tool …`). | Tests: prefix reaches the child env (and only that child); `$ENVVAR` resolves from a binding and from a `requires env` name; an undeclared host variable is refused by the env gate; a prefix does not survive into the next op; `--check` rejects a malformed `NAME=`; `simulate` and `--dry-run` show the prefix; the scan reports it under `inferred`. |
| R04 | A library crate `perch-runtime` exposes: build a `Runtime` from a `Policy` (capability mask, env allowlist, HTTP policy, deadline, IO sinks), load a program from a path or string, run a command with arguments, and return a structured result (stdout, stderr, exit status, error kind). No global state: two runtimes with different policies coexist in one process. | A doc-tested example in the crate; a test runs two runtimes with different policies concurrently; the `perch` binary is reimplemented on top of the crate with no behavior change (existing parity matrix still passes). |

### Follow-ups left out of the port

| ID | Item | Why it was deferred |
|---|---|---|
| F01 | **Windows test parity.** The first Windows CI run found CRLF checkouts breaking a byte-for-byte golden (`template_matches_go_html_template`); `.gitattributes` now forces LF, unverified. More failures may sit behind it. Interface listing returns nothing on Windows, file modes are approximated, and the Windows code paths have never been exercised. | Release went out with Windows tests non-blocking. |
| F02 | **wasm compile cache.** Compiling a ~1.5 MB module with wasmtime takes several seconds per invocation. Add a persistent cache (wasmtime's cache config or serialized modules under the user cache dir), mirroring what the Go build did with wazero. | New behavior relative to the port's first cut. |
| F03 | **wasm capability gating.** `wasm_mount_*` and `wasm_allow_host` are not checked against `requires` roots or the capability mask. This is also true in Go. | Closing it changes behavior. |
| F04 | **Typed wasm error kinds.** `wasm_compile_failed`, `wasm_module_exited`, `wasm_capability_denied` and `wasm_http_refused` exist in the docs and domain but are never emitted (Go returned plain errors). | Emitting them changes error text to `kind: msg`. |
| F05 | **`installlsp` without Go.** It still shells out to `go install …perch-lsp@main`. Replace with a release download verified by sha256. | Same behavior as Go was the port's bar. |
| F06 | **TLS trust roots.** `http_*` uses bundled webpki roots, so corporate MITM CAs fail. Use the system trust store. | Differs from Go, which used the system store. |
| F07 | **`perch serve` `/` page bug.** In Go, any visible command with args truncates the page right after `<label for="` (template uses `{{$.Name}}` where `$` is the root). The port reproduced it on purpose. | Fixing diverges from the byte-identical bar. |
| F08 | **Unverified parity surface.** `--install-lsp`, `--install-vscode`, `--import`, shebang invocation and interactive `--ask` were not compared against Go. | Needed external tools or a terminal. |
| F09 | **Tidy and release plumbing.** Collapse duplicated helpers in `infra/ops/src/group_b` into `common.rs`; fill the Homebrew formula's sha256 placeholders from release checksums; update README, install docs and scripts that still say `go install` now that the Go tree is deleted (decision D4). | Cleanup, not parity. |

## Design Notes

**R01.** `perch-scan` already builds a `Report` with `BTreeMap` fields. Add a `declared` section sourced from `Program.requirements` (already parsed) and keep `inferred` as today. Serialize with serde and sorted keys so output is stable for golden tests. `--json` and `--format json` select the serializer; the text renderer is unchanged apart from the advice fix.

**R02.** `try … rescue … finally` already parses through capy's `block_sections` and is handled by `opTry`. (a) Extend the command body grammar so `do … finally … end` lowers to the same marker stream as a whole-body `try … finally`, so no interpreter semantics change. (b) In `opTry`, when the `finally` ops fail and a body error is pending, return the body error with the cleanup error appended (for example `; additionally, finally failed: …`), keeping the body error's kind. Execution contexts that unwind via `timeout` or signals must still run it; `--max-runtime` expiry runs `finally` with a short grace deadline (decision D1).

**R03.** Confinement sits in `infra/ops` process spawning, behind one function that takes the declared scopes and a command and returns a wrapped command. macOS: generate a `(deny default)` profile with allowed read/write subpaths plus the minimum system reads needed to exec, pass via `sandbox-exec -p`. Linux: apply a Landlock ruleset in a `pre_exec` hook before `exec`. Other platforms: return `Unsupported`, which the caller turns into refusal or advisory. `sandbox-exec` is deprecated by Apple but still the practical mechanism; docs/typed-bins.md already records this trade-off. Network (`host`) confinement is best-effort: Landlock's network rules where the kernel has them, otherwise advisory. The fail-closed rule is the important part and ships first.

**R05.** Extend the bare-dispatch grammar so a run of `NAME=value` tokens before the leading name is parsed as an env overlay on that op (the same shape as `with_env` for one op), then dispatched as today. Resolution order for `$ENVVAR`: bindings first, then host env, with host env subject to the existing gates (`requires env`, `--env`), so a prefix cannot smuggle a secret the file did not declare. Names follow the existing env-scrub rules: the overlay is added on top of the scrubbed child environment, not a way around it. A token is a prefix only if it matches `[A-Za-z_][A-Za-z0-9_]*=` before the first non-assignment token, which keeps `name=value` arguments after the binary untouched.

**R04.** New crate `perch-runtime` depending on the existing layer crates. The orchestrator's wiring (handler registry, preflight hook, restrictions, deadline, stdio) moves into the crate behind a builder; the `perch` binary becomes a thin CLI over it. The VHCO rule that only the orchestrator composes concrete pieces still holds: `perch-runtime` is the orchestrator's library half.

## Alternatives Considered

| Alternative | Why not chosen |
|---|---|
| Wrap subprocesses in a container instead of `sandbox-exec`/Landlock | Needs a daemon and images, and the point of perch is to stay a single binary. |
| Make `--scan --json` a separate command | A flag on the existing command keeps one place to learn. |
| `finally` as sugar over `try … rescue` plus rethrow | Cannot preserve the original error identity and exit code cleanly, which is the requirement. |
| Only `with_env` blocks (no inline form) | Already exists, but it wraps the call in a context. The stated goal is shell-like lines without contexts everywhere, which also eases migration from shell scripts (docs/migrating-from-shell.md). `with_env` stays for scoping env over several ops. |
| Expose perch to other languages only via the CLI and MCP | Already possible, but the request is for in-process use with per-runtime policy. A C ABI is deferred to O3. |

## Risks and Rollback

| Risk | Mitigation |
|---|---|
| Sandbox profile too tight or too loose, breaking legitimate tools | Ship behind a flag first, then default-on for files that declare scopes; keep the explicit advisory opt-out. |
| `sandbox-exec` removed or further restricted by Apple | Fail closed means perch refuses rather than degrading silently; revisit with Endpoint Security or a helper if needed. |
| `finally` semantics surprise authors (ordering with `try`, timeouts) | Specify in docs with tests first; D1 settles the timeout case. |
| Library API churn | Mark `perch-runtime` 0.x and keep the surface small (builder, run, result). |
| Windows work hides further failures | F01 adds Windows back to blocking CI only once green. |

Rollback for each requirement is to revert its commit; none changes stored data. R03's fail-closed default is the only change that can make a previously running file refuse to run, so it needs a changelog entry and a documented opt-out.

## Security Impact

R03 is a security improvement: it turns declared scopes from perch-level checks into kernel-enforced ones for spawned binaries, and removes the silent ambient-authority path. R01 increases the information a wrapper can rely on but must not leak more than today's scan (no secret values, only names). F03 closes an existing gap. F06 restores the system trust store, which is a security-relevant parity fix.

## Compatibility Impact

- R01, R04, R05 and F02 are additive. R05 only gives meaning to a line that today would be read as a binary named `NAME=value`.
- R02 adds a keyword; an existing program using `finally` as an identifier would need a rename. The registry already errors on keyword collisions, so this surfaces at `--check`.
- R03 changes behavior for files that declare scopes on unsupported platforms (refuse by default). F04 changes error text. F07 changes the `/` page for programs that hit the bug.

## Test and Validation Design

- **R01:** golden JSON tests; a round-trip test that `declared` equals the parsed manifest.
- **R02:** the matrix under R02 acceptance, run through the interpreter and through the CLI for exit codes.
- **R03:** per-platform escape tests (the request.md repro as a test), plus an unsupported-platform test asserting refusal.
- **R04:** doc-test, concurrent-policy test, and the existing ~960-invocation parity matrix against the Go build run with the binary rebuilt on the library.
- **F01:** Windows job green without `continue-on-error`.
- Tests that exercise real side effects (for example the Homebrew-installing demo) must use copied trees or `--dry-run`; the port's parity run once executed `brew install` on a developer machine.

## Sequencing and Estimates

Rough sizes, not commitments.

| Step | Items | Size |
|---|---|---|
| 1 | F01 (Windows green), F09 (formula checksums, helper cleanup) | small |
| 2 | R01, F06, F04 | small |
| 3 | R02, R05 | small–medium |
| 4 | R04 | medium |
| 5 | R03 (fail-closed guard, then macOS, then Linux), F03 | medium–large |
| 6 | F02, F05, F07, F08 | small each |

Step 5 is largest and most uncertain; its fail-closed guard can ship ahead of the platform backends.

## Decisions and Open Questions

Decided by the requester on 2026-10-01 (recorded here; still subject to maintainer review of this draft):

| ID | Question | Decision |
|---|---|---|
| D1 | Does `finally` run when `timeout` or `--max-runtime` fires? | **Yes.** It runs with a short, fixed grace deadline (proposed 5 s) so cleanup such as stopping a service is not skipped, but a hung cleanup cannot hold the process forever. On grace expiry the original timeout error is reported and the overrun is noted. |
| D2 | Env prefix on built-in ops, or binary calls only? | **Binary calls and `exec` only** (R05). |
| D3 | Confinement on Windows (R03): refuse by default, or advisory? | **Refuse by default** (the secure choice). A file that declares `read`/`write`/`host` scopes will not run on a platform with no enforcement mechanism unless the operator passes an explicit opt-out, which prints that scopes are advisory. |
| D4 | Delete the Go tree now that 0.1.1 is Rust? | **Yes.** Done on branch `rust-port` (see change history). Go callers of the old packages lose that import path; O3 covers a non-Rust host story. |

Still open:

| ID | Question | Owner |
|---|---|---|
| O1 | Should `--scan --json` carry a schema version field from the start? | Language maintainer |
| O3 | Is a C ABI or wasm build of `perch-runtime` wanted for non-Rust hosts, now that the Go packages are gone? | Maintainers |

## Approval

Pending. No reviewer has approved this draft.

## Related Documents

- [PROP-2026-0001](prop-2026-0001-perch-correctness-security-and-completeness.md): correctness, security and completeness remediation (a separate, earlier proposal).
- [docs/typed-bins.md](../typed-bins.md) and [docs/sandboxed-by-design.md](../sandboxed-by-design.md): confinement landscape and capability model.
- [docs/embedding.md](../embedding.md): the existing `--build` embedding format.
- `.ignore/request.md`: source of R01–R04 (local, not committed).

## Change History

| Rev | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial draft, including R05 (inline env prefix). |
| 2 | 2026-10-01 | Claude | Record decisions D1–D4; O2, O4, O5 and O6 resolved. |

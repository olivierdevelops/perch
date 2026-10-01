---
document_id: ARCH-2026-0001
title: "Perch layer boundaries - VHCO as realized in the Rust workspace, and where it drifts"
document_type: architecture
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [domain, infra, usecases, io, orchestrator, cmd, runtime library]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI]
audience: [engineers, maintainers, reviewers]
scope: "How the VHCO rules in vhco-architecture.md map onto the Rust workspace as it exists, the measured dependency edges that depart from them, the decision to place the runtime library in the orchestrator crate, and the drift recorded without claiming whole-repository compliance."
reason: "AGENTS.md says to follow vhco-architecture.md; the standard (section 31) asks for an architecture decision when boundaries or component ownership change, and R04 added a public library surface."
related_documents: [PLAN-2026-0001, PROP-2026-0002, SYS-2026-0001, MAN-2026-0005, REF-2026-0003]
supersedes: null
superseded_by: null
tags: [architecture, vhco, boundaries, library, drift]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Perch layer boundaries - VHCO as realized in the Rust workspace, and where it drifts

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** all crates; the `perch` library target in particular
> **Compliance statement:** this document does **not** claim that the repository complies with VHCO. It records what is true, what departs from the rules, and what was decided.

## Summary

The workspace follows the shape of VHCO (domain, usecases, io, infra, orchestrator, one crate per package, a single composition crate) and enforces part of it mechanically through Cargo. It departs from the written rules in several measurable ways: infrastructure crates depend on the domain and on each other and, in one case, on use cases; two use cases depend on infrastructure; two extra executables compose their own subsets outside the orchestrator; there is no `features/` layer; and an extra top-level folder (`cmd/`) exists. The runtime library added in 0.2.0 lives in the orchestrator crate, not in a new crate or folder, which is the choice that best fits the rules.

## Scope

In scope: the Cargo workspace members listed in [SYS-2026-0001](../system/sys-2026-0001-rust-workspace.md). Out of scope: the VHCO rules about views and view models (this is a CLI/library product; `io/cli` is the only io module), and non-Rust trees (`editors/`, `wasm-sdk/`).

## The rules, and what the workspace does

Rules are from [vhco-architecture.md](../../vhco-architecture.md) ("The Golden Rules").

| Rule | Realized how | Status |
|---|---|---|
| Exactly six top-level folders | Source folders present: `domain/`, `usecases/`, `io/`, `infra/`, `orchestrator/`. `features/` is absent. An additional source folder `cmd/` holds two executables. Non-source folders (`docs/`, `editors/`, `recipes/`, `demos/`, `scripts/`, `skills/`, `wasm-sdk/`, `assets/`, `Formula/`) are not code layers. | Departs (five of the six, plus `cmd/`). |
| 1. Modules do not import each other; only the orchestrator composes | Mechanically enforced by crate dependencies. Measured edges below. `usecases/*` mostly depend on `perch-domain` only and receive everything else as closures. | Partly met. |
| 2. No new top-level folders | `cmd/` exists. R04 deliberately did not add one (see the decision below). | Departs (pre-existing `cmd/`). |
| 5. Each layer declares the protocols it needs (consumer-owned) | `io/cli` declares the use-case traits (`RunCommandUseCase`, `ScanUseCase`, ...). Each use case declares closure types for what it needs (`LoadFn`, `FetchFn`, ...). | Met for `io` and `usecases`. |
| 6. Functions only have access to their parameters | Use cases: yes (closures in `Impl`). Ops: handlers receive an `&Interpreter` carrying program, bindings, hooks and policy, and reach the OS directly (files, processes, environment). Process-global caches exist. | Partly met (see drift). |
| 7. All `Impl`s live in the orchestrator | Each use case crate defines its own `Impl` struct (fields are closures) next to its logic; the orchestrator builds the values and adapters. | Departs in naming and placement; composition itself is in the orchestrator. |
| 8. Infra is replaceable; infra knows nothing of the system | Infra crates import `perch-domain` types and each other. | Departs (see below). |
| Domain has no dependencies | `perch-domain` depends on `serde` and `serde_json` only, nothing in the workspace. | Met. |

## Measured dependency edges

Produced by `cargo metadata --format-version 1 --no-deps`, normal (non-dev) workspace dependencies, grouped by top-level folder. Re-run the command to re-check; the counts are edges between crates.

| From | To | Edges | Rule it touches |
|---|---|---|---|
| orchestrator | domain, infra, io, usecases | 1, 8, 1, 16 | Allowed (composition). |
| io | domain | 1 | Allowed (`io/cli` imports only domain). |
| usecases | domain | 15 | Domain as shared vocabulary; accepted drift of the strict reading. |
| usecases | infra | 4 | `perch-validate` and `perch-runshell` -> `perch-interpreter`; `perch-exportopscatalog` -> `perch-opcatalog`; `perch-installvscode` -> `perch-vscodeext`. Departs from rules 1 and 8. |
| infra | domain | 10 | Every infra crate. Departs from "infra does not import domain". |
| infra | infra | 9 | `perch-ops` -> `perch-interpreter`, `perch-capyloader`; `perch-audit`, `perch-report`, `perch-preview` -> `perch-interpreter`; `perch-opcatalog` -> `perch-capyloader`, `perch-interpreter`; `perch-httpserver` -> `perch-interpreter`, `perch-ops`. Departs from rule 1. |
| infra | usecases | 3 | `perch-httpserver` -> `perch-scan`, `perch-simulate`, `perch-validate`. The reverse of the intended direction. |
| cmd | domain, infra, usecases | 2, 5, 1 | `perch-lsp` -> capyloader, ops, validate; `perch-mcp` -> capyloader, interpreter, ops. Each is a second composition point. |

```text
 intended (VHCO)                         realized (Cargo)
 -----------------------                 ---------------------------------------------
 io ----> usecases ----> infra           orchestrator -> everything
 (orchestrator wires)                    io/cli -> domain
                                         usecases -> domain  (+ 4 edges -> infra)
                                         infra -> domain, other infra  (+ 3 edges -> usecases)
                                         cmd/* -> domain, infra, (validate)
```

## Why these edges exist

- **Domain everywhere.** `Program`, `Op`, `ErrorKind` are the shared vocabulary of a Rust port that kept the Go data model. Treating domain as importable by every layer is the common reading in practice; the written rule says infra must not.
- **Infra on infra.** The interpreter is the extension point (handlers register into it), so everything that observes or extends execution (`ops`, `audit`, `report`, `preview`) depends on it. `perch-ops` -> `perch-capyloader` is declared as a normal dependency but the loader is used only by `infra/ops/tests/*.rs` (to load `.perch` source in end-to-end tests); no source file under `infra/ops/src` references it, so it is a dev-dependency in practice (D-09).
- **Use cases on infra (`validate`, `runshell`).** They use interpreter helpers (`go_quote`, `provided_var_names`, `Bindings`, `Interpreter`) rather than receiving them as parameters.
- **Infra on use cases (`httpserver`).** The web UI exposes `--check`, `--scan` and `simulate`, and calls the use-case functions directly instead of receiving closures from the orchestrator.
- **`cmd/` executables.** `perch-lsp` and `perch-mcp` are separate programs that compose loader, ops and interpreter themselves.

None of these were introduced by 0.2.0. 0.2.0 added edges only within existing patterns: the `confine` module inside `infra/ops`, `JsonReport` (serde) inside `usecases/scan`, and the library facade inside `orchestrator`.

## Decision: the runtime library lives in the orchestrator crate

**Context.** R04 asks for a library: build a `Runtime` from a `Policy`, load a program, run a command, get a structured result ([MAN-2026-0005](../manuals/man-2026-0005-using-perch-as-a-library.md)). PROP-2026-0002 first proposed a new crate `perch-runtime`.

**Decision.** The library is the library target of the existing `perch` package in `orchestrator/` (`src/lib.rs`, `policy.rs`, `runtime.rs`). The binary target is `src/main.rs`, a one-line call to `perch::run_cli()`.

**Reasons.**

1. A new top-level folder would break "no new top-level folders"; a new crate under an existing folder would need to depend on infra and use cases, which only the orchestrator is allowed to do.
2. `Runtime` is exactly a composition: it builds the handler registry from a policy, builds an interpreter, wires IO sinks and the deadline. That is the orchestrator's job by definition ("the library half of the orchestrator").
3. One construction path: `Policy::handlers` and `Policy::configure` are used by both `Runtime::run` and the CLI wiring (`policy_of(Settings)` in `orchestrator.rs`), so the CLI flags and the library cannot drift in how a policy becomes interpreter state.

**Alternatives considered.**

| Alternative | Why not |
|---|---|
| New crate `perch-runtime` under a new top-level folder | Violates the six-folder rule. |
| New crate `perch-runtime` under `orchestrator/` | Two composition crates; unclear ownership of the CLI wiring. Not rejected as impossible, only as unnecessary. |
| Expose `perch-interpreter` and `perch-ops` directly to embedders | Leaks the handler registry and policy wiring to every caller; no stable surface. |

**Consequences.**

- The public API of the `perch` crate includes types from lower layers (`JsonReport` from `perch-scan`, `Issue` from `perch-validate`, `Program` via `Loaded::program()`), so a change in those crates can change the library API.
- The package is named `perch`; it cannot be published under that name without a decision about the crates.io namespace (out of scope for 0.2.0).
- Honest gap: `Runtime::run` and the CLI's `make_run_fn` are two functions that both build an `Interpreter`. They share `Policy`, but the CLI path adds audit, report and trace wiring and does not call `Runtime::run`; `perch test` builds its own interpreters and changes the process cwd. "The CLI is built on the library" is true for policy and entry point, not for every run path.

## Drift recorded

| ID | Observation | Evidence | Risk | Proposed handling |
|---|---|---|---|---|
| D-01 | `infra/httpserver` depends on `usecases/{scan,simulate,validate}` | `cargo metadata` edges above | Layer inversion; the web UI cannot be built or tested without those crates | Inject the three functions as closures from the orchestrator (as `runserver` already does for serving). |
| D-02 | `usecases/validate`, `usecases/runshell` depend on `infra/interpreter`; two more use cases depend on infra crates | edges above | Use cases cannot be tested without infra | Move the helpers into `domain` or pass them as parameters. |
| D-03 | `cmd/perch-lsp` and `cmd/perch-mcp` compose infra themselves | `cmd/*/Cargo.toml` | Second wiring point that can diverge from the orchestrator's | Build them from library functions of the `perch` crate, or accept `cmd/` as a documented seventh folder. |
| D-04 | No `features/` layer | folder listing | Capabilities live as closures in use cases | Accept; document that the capability shapes are the closure types in each use case. |
| D-05 | Infra imports domain | all 10 infra crates | Contradicts "infra knows nothing of the system" | Accept as a reading of domain as shared vocabulary, or split an `infra`-neutral types crate. Needs a maintainer decision. |
| D-06 | Process-global state under the "no hidden globals" rule | `OnceLock` caches, the `BUNDLE` mutex, `set_env`, `perch test` cwd changes ([SYS-2026-0001](../system/sys-2026-0001-rust-workspace.md#process-global-state-and-caches)) | Two runtimes in one process are not fully isolated | Documented in MAN-2026-0005; fix `set_env` and `path_abs` to use per-interpreter state. |
| D-07 | `Impl` structs live in use case crates | `usecases/*/src/lib.rs` | Naming differs from the guide's "Impls in the orchestrator" | Accept: the structs are closure bags; the orchestrator performs the composition. |
| D-08 | Stray tracked files in infra: `infra/ops/src/group_b/{strings,textlines,time}.rs-E` | `git ls-files` | None functional; hygiene | Delete (not done here; outside the documentation scope). |
| D-09 | `perch-ops` declares `perch-capyloader` as a normal dependency; only tests use it | `grep -rn capyloader infra/ops` | One avoidable infra-to-infra edge | Move it to `[dev-dependencies]`. |

No annotation-based check was run: the repository carries no `vhco:` source annotations, so the `vhco` tool's generated model would be empty. The edge table above is the evidence.

## Security considerations

The layering matters to safety in one place: all gating (`requires`, capability flags, confinement) lives in `infra/ops` and the interpreter, and every entry point (CLI, library, `perch-mcp`, the web UI) reaches them through the handler registry. `perch-lsp` loads programs but never runs ops, so it does not gate anything. A new entry point must obtain handlers from `Policy::handlers` (or `perch_ops::all_handlers` plus `apply_restrictions` and `apply_mask_gating`) or it will skip operator restrictions. The web UI receives the restricted handler set from the orchestrator. `perch-mcp` builds `perch_ops::all_handlers()` itself, sets the preflight hook, and takes no capability flags (`-f` is its only flag), so an MCP run is bounded by the file's `requires` and nothing else; this follows from D-03 and is worth a maintainer decision.

## Open questions

1. Is `cmd/` accepted as a layer, or should the two executables be folded into the orchestrator crate as extra `[[bin]]` targets?
2. Should `perch-domain` be treated as importable by infra?
3. Should the CLI run path be re-expressed over `Runtime::run` (with audit/trace as policy fields) so that there is one run path?

## Related documents

[SYS-2026-0001](../system/sys-2026-0001-rust-workspace.md) · [MAN-2026-0005](../manuals/man-2026-0005-using-perch-as-a-library.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [vhco-architecture.md](../../vhco-architecture.md) · [AGENTS.md](../../AGENTS.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial architecture record for the Rust workspace and the R04 placement decision. |

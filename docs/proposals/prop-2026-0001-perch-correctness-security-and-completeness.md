---
document_id: PROP-2026-0001
title: Perch correctness, security and completeness remediation
document_type: proposal
status: draft
created_date: 2026-09-26
last_updated: 2026-09-27
document_revision: 2
authors: [Codex, Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [parser, interpreter, operations, validation, CLI, MCP, HTTP server, simulation, packaging, documentation]
affected_versions:
  from: "0.1.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [engineers, maintainers, reviewers]
scope: Resolve all 67 numbered issues (B01–B51 plus addendum B52–B67), every missing feature (M01–M11) and every documentation discrepancy in the supplied 27-project report and its addendum.
reason: Restore trustworthy execution, capability enforcement and agreement between documented and actual behavior.
related_documents: [REF-2026-0002, STD-2026-0001, REF-2026-0003]
supersedes: null
superseded_by: null
tags: [proposal, correctness, security, language, compatibility]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-10-26
---

# Perch correctness, security and completeness remediation

> **Status:** Draft · **Created:** 2026-09-26 · **Updated:** 2026-09-27 · **Revision:** 2
> **Owner:** Perch maintainers (proposed; assignment pending)
> **Affected versions:** Reported installed 0.1.0 / help 0.1.2; reviewed source `c60e402`. Fix release versions remain unassigned.
> **Components:** Parser, interpreter, ops, check, CLI, MCP, HTTP server, simulator, packaging and docs.

## Decision Requested

Adopt the contracts, complete coverage and implementation sequence below as the basis for remediation. Implementation should proceed in independently reviewable increments: immediate security corrections, language and policy foundations, tools and packaging, then missing capabilities and release conformance.

This document proposes work; it does not claim fixes, tests, releases or maintainer approval. The current task changes documentation only. A security patch must not wait for arithmetic, typed bins or a whole-repository architecture migration.

## Original User Request

| ID | What was asked | Source / date | Interpretation |
|---|---|---|---|
| UQ-01 | “write a proposal to fix everything in …/perch_projects/PERCH_BUGS.md follow documentation.md” | User conversation, 2026-09-26; full report preserved as [REF-2026-0002](../references/ref-2026-0002-perch-project-bug-report.md) | Everything includes numbered defects, missing features and docs drift. Write the proposal, not the implementation. |
| UQ-02 | “follow vhco-architecture.md” | Repository [AGENTS.md](../../AGENTS.md), supplied in this conversation | Apply the repository architecture rules to proposed implementation boundaries. |

No `documentation.md` or `DOCUMENTATION.md` was found in this repository during discovery. This draft provisionally follows `/Users/oliverlaleau/Documents/projects/vibe-documentation/DOCUMENTATION.md`, REF-2026-0001 revision 3, especially §§4.2, 12.1 and 21. A verbatim local reference copy is [REF-2026-0003](../references/ref-2026-0003-documentation-standard-source.md). Its selection is an explicit assumption pending the user's clarification; it is not represented as an already-adopted Perch policy.

## Problem and Evidence

The supplied report aggregates 27 projects. Frequency estimates belong to that report, not a new measurement. Original IDs remain stable throughout this proposal: `B01` means report issue #1. `M01–M10` enumerate the unnumbered missing capabilities; `D01–D05` enumerate documentation drift. `B52–B67` and `M11` come from [Addendum A](../references/ref-2026-0002-perch-project-bug-report.md#addendum-a--findings-omitted-from-the-original-consolidation). The addendum records findings from the same per-project reports that the original consolidation left out. It also lists reported details already folded into B01–B51, so every agent finding traces to a requirement.

| Problem | Affected users and consequence | Evidence and confidence | Request |
|---|---|---|---|
| P-01: parsing, effects and policy disagree | Operators can target the wrong namespace, miss policy checks or overwrite files | Source-confirmed key mismatch in `infra/ops/archive.go` and `infra/ops/requires.go`; `opSymlink` removes link destination; `handlePerchRun` combines private/unknown lookup before catch handling. Exploitability still requires isolated reproduction. | UQ-01 |
| P-02: values and invocation state lack consistent semantics | Authors get plausible but wrong predicates, lists, strings and results | Report B09–B30; `infra/interpreter/bindings.go` uses `map[string]any`; loader compiles through Capy/NDJSON. These facts locate the boundary but do not prove every reported failure. | UQ-01 |
| P-03: tools model a different program | Check, scan, simulate and dry-run cannot reliably predict execution | Report B31–B34, B50–B51; independent implementations exist in `usecases/validate`, `scan`, `simulate` and `infra/preview`. | UQ-01 |
| P-04: capability and source-location contracts are unclear | Tests need excessive permissions; bundles behave differently; host errors disappear | Source-confirmed `expandRoots` discards interpolation errors and roots resolve against `b.Cwd`; report B35–B49. | UQ-01 |
| P-05: missing operations and stale contracts drive shell workarounds | More subprocess trust and less portable automation | Report's missing-feature/docs sections; `docs/typed-bins.md` explicitly says design only; MCP only parses `-f`; HTTP client has a fixed 30-second timeout. | UQ-01 |

Reviewed source: `infra/capyloader/loader.go`, `registry.go`, `infra/interpreter/bindings.go`, selected portions of `interpreter.go`, `infra/ops/archive.go`, `requires.go`, `http.go`, `files.go`, `strings.go`, `infra/opcatalog/catalog.go`, `cmd/perch-mcp/main.go`, `usecases/runcommand/runcommand.go`, and `docs/typed-bins.md`. This is a targeted source review, not an exhaustive audit. No application tests or original projects were executed for this proposal. In particular B07 is a reported potential exposure, not a reproduced token leak; B01's bypass is a risk requiring proof.

### Current User Journey

```text
Author -> manual/example -> .perch source -> --check succeeds
                                         -> runtime reparses/coerces
                                         -> wrong value / target / policy outcome
                                         -> shell workaround -> larger trust boundary
```

## Goals and Non-Goals

| ID | Goal | Problems | Observable acceptance signal |
|---|---|---|---|
| G-01 | Enforce capabilities before effects and keep secrets out of diagnostics | P-01, P-04 | Denied actions create no side effect; secret sentinels absent from every output surface. |
| G-02 | Give values, calls and control flow one explicit contract | P-02 | Every B09–B30 regression produces the defined value or typed error. |
| G-03 | Align all execution and inspection surfaces | P-03, P-04 | Same normalized invocation and visibility policy across CLI/MCP/web/check/scan/simulate. |
| G-04 | Support the reported real workflows without mandatory shell workarounds | P-05 | Missing-feature acceptance cases work natively, including typed-bin validation. |
| G-05 | Make migration, documentation and release evidence traceable | All | Every report item maps to a test, manual section and release verification row. |

Non-goals: running the 27 projects against live production services; claiming host subprocesses are confined by manifest argument checks; adding a new UI, service, storage backend or authentication system; repairing unrelated pre-existing architecture drift. No existing security boundary is weakened to make an example pass.

## Proposed User Journey

```text
Author -> versioned manual -> parse once -> typed program + source locations
                                            |
                        +-------------------+-------------------+
                        |                                       |
                  check / scan / simulate                invoke with policy
                  known/unknown diagnostics               normalize + authorize
                        |                                       |
                  useful preflight                      execute -> typed result
                                                        or stable error + recovery
```

## Requirements

Each row is also a regression acceptance specification. Unless otherwise stated, its external source is the matching issue in [the supplied report](../references/ref-2026-0002-perch-project-bug-report.md#original-report). Source labels `B`, `M` and `D` refer exclusively to that external report. Goals, use cases, changes, files and tests are mapped later; proposed contracts are not misrepresented as existing syntax.

### Numbered defects: security and execution

| Requirement | Source | Required behavior / acceptance criterion | Goal |
|---|---|---|---|
| R1 | B01 | Normalize positional and named archive/compression args before policy and handlers. All six ops reject missing paths; denied source/destination causes zero filesystem effects. Extraction rejects traversal, absolute entries and symlink escapes. | G-01 |
| R2 | B02 | Parse command flags before or after positionals, honor `--`, reject unknown flags. `restart_service web -namespace=kube-system -confirm=web` uses the specified namespace and confirmation. | G-01, G-03 |
| R3 | B03 | `symlink` refuses an existing destination by default without removing it. An explicit replacement option can replace a file/link after authorization; never recursively delete a directory. | G-01 |
| R4 | B04 | MCP rejects an existing private command independently of catch. Catch handles unknown names only; external entry restrictions apply before any handler executes. | G-01 |
| R5 | B05 | MCP accepts capability restrictions equivalent to CLI: supported `--no-*`, `--allow-host`, `--env` and sandbox options. Unsupported options fail at startup; `--require-sandbox` fails closed if enforcement is unavailable. | G-01, G-03 |
| R6 | B06 | Scan classifies bare declared bins and explicit exec identically. It never recommends `--no-subprocess` when a reachable path requires a subprocess; unknown paths retain uncertainty. | G-03 |
| R7 | B07 | Redact URL userinfo and query values plus authorization/cookie credentials in errors, hooks, trace, audit, MCP and web output, including wrapped transport errors and redirects. Sentinel secrets never appear. | G-01 |
| R8 | B08 | Native HTTP accepts header maps and content types, exposes status/headers/body through a structured request operation, and treats unexpected 4xx/5xx as typed errors. Body-returning convenience ops retain their result shape. All HTTP variants share host/SSRF/redirect enforcement. | G-01, G-04 |

### Numbered defects: language and operations

| Requirement | Source | Required behavior / acceptance criterion | Goal |
|---|---|---|---|
| R9 | B09 | Literal and binding RHS assignments evaluate values; only a resolved call invokes a command/op/bin. Strings, numbers, empty strings and copied bindings all work inside `do`. | G-02 |
| R10 | B10 | Decode strings once. Plain single/double quoted and backtick strings preserve backslashes; explicit escaped strings decode documented escapes. Regex `\s`, `\d`, `\[` survives statements, capture, format and file operations unchanged. | G-02 |
| R11 | B11 | All three plain quoting forms allow multiline content with consistent bytes in assignment, captures, write and append. Unterminated strings report their opening source location. | G-02 |
| R12 | B12 | Both comparison operands are expressions; unresolved bare names error. Numeric ordering rejects nonnumeric values instead of coercing them into a plausible result. | G-02 |
| R13 | B13 | `not` negates a complete predicate expression, including `not exists "p"` and `not has_bin "x"`; it does not swallow errors. | G-02 |
| R14 | B14 | Predicates receive their full typed argument list; arity/type validation is identical in check and runtime. | G-02 |
| R15 | B15 | Unknown predicate is a check/runtime error with location and suggestion, never silent false. Remove nonexistent `not_exists` examples in favor of `not exists`. | G-02 |
| R16 | B16 | Template parameters are lexical typed bindings usable bare, in comparisons and in interpolation. Missing parameters fail at entry. | G-02 |
| R17 | B17 | Lists remain lists through split/regex/JSON/capture/calls. Empty means zero iterations; join accepts a list; `for_each` evaluates a binding, not its spelling. | G-02 |
| R18 | B18 | Commands return explicit typed values; absent return yields null. Callee locals and args cannot modify caller bindings. Globals are read-only inputs, not a communication channel. | G-02 |
| R19 | B19 | Every invocation creates fresh parameters and applies defaults even with zero args. Default interpolation uses globals and earlier bound parameters; unknown/cyclic/forward default references error deterministically. | G-02 |
| R20 | B20 | Only unquoted operator tokens create chains. Quoted `;` and `&&` remain literal argv, including captures and escaped-string forms. | G-02 |
| R21 | B21 | Both `rescue` and `rescue err` parse; the latter binds a structured error inside rescue only. Outside-scope use fails check. | G-02 |
| R22 | B22 | One reserved-name catalogue rejects collisions including `run`, `test`, `import` at load/check time across commands, templates, bin aliases and imports. | G-02, G-03 |
| R23 | B23 | Each uncaught error event invokes the applicable `on_error` handler once at its propagation boundary. Handled rescue errors do not also trigger global recovery. Handler-local state persists during that invocation without leaking to callers. | G-02 |
| R24 | B24 | `repeat "ab" 3` returns `ababab`; zero returns empty; negative, fractional or excessive counts error before allocation. Named/positional calls agree. | G-02 |
| R25 | B25 | Variadic format preserves numeric types, supports multiple values, and returns literal format strings with zero arguments. Invalid format/count/type produces a typed error, never `%!…` artifacts. | G-02 |
| R26 | B26 | Replace accepts separate old/new arguments so commas are representable. Reject empty old by default. Keep deprecated comma-pair form only when unambiguous; ambiguous calls receive migration diagnostics. | G-02 |
| R27 | B27 | JSON paths support keys and array indexes, optional leading dot, typed objects/lists and explicit missing/null distinctions. Malformed JSON or invalid paths error. Implement and document `json_count`. | G-02, G-04 |
| R28 | B28 | Exec supports explicit stdout/stderr capture, silence and stdin modes. Default stdin is closed; inheritance is opt-in. Nonzero exits retain structured output and status without forcing shell. | G-01, G-02 |
| R29 | B29 | Write/append can create missing parent directories after authorization of all created paths. Existing permissions are preserved; symlink escapes and uncreatable parents error without unintended writes. | G-01, G-02 |
| R30 | B30 | Glob preserves relative/absolute input shape, returns sorted typed lists, supports recursive `**` with bounded traversal and no implicit symlink following. Every traversed root must be readable. | G-01, G-02 |

### Numbered defects: checker, manifest, tests and tools

| Requirement | Source | Required behavior / acceptance criterion | Goal |
|---|---|---|---|
| R31 | B31 | Check uses runtime lexical scope rules. A variable assigned on every reaching branch is available after the merge; conditional-only assignment is possibly unbound. Callee locals remain inaccessible under R18, with migration guidance. | G-02, G-03 |
| R32 | B32 | Manifest bindings resolve consistently for read/write/host, from immutable declared configuration rather than mutable command locals. Unresolvable/dynamic values produce explicit diagnostics, never false static assurance. | G-01, G-03 |
| R33 | B33 | Check and runtime normalize roots identically; `.` and `./` are equivalent and write implies read. Runtime still validates dynamic paths. | G-01, G-03 |
| R34 | B34 | Tests may explicitly assert an expected policy denial. Check reports that expected diagnostic as satisfied only for the annotated expression; runtime still denies the effect. Catch/rescue alone does not silence arbitrary static failures. | G-01, G-03 |
| R35 | B35 | Keep zero ambient authority when requires is absent. Pure computation remains usable; external effects require declarations. Correct ambient-access claims and provide migration examples. | G-01, G-05 |
| R36 | B36 | Initialize `bundle_dir` before manifest expansion; unresolved roots cause an actionable error naming the declaration, never silent omission. | G-01, G-03 |
| R37 | B37 | Bundle preserves declared relative layout in source and built modes; `bundle_extract` works in both. Reject colliding entries/traversal; extraction obeys write roots. | G-03 |
| R38 | B38 | Permission errors distinguish attempted path, canonical path and declared effective roots with source locations. Do not label the attempted path as the declaration. | G-01, G-03 |
| R39 | B39 | Canonicalize existing path components, including macOS `/var` versus `/private/var`. Nonexistent leaves use canonical parents; symlink changes cannot grant out-of-root access. | G-01, G-03 |
| R40 | B40 | Resolve manifest relative roots against the declaring script's directory, including imports. Runtime working directory and test temp directory do not redefine authority. Stdin source requires an explicit captured base directory. | G-01, G-03 |
| R41 | B41 | Add test-local requirements and fixture lifecycle. They apply only inside the test invocation and remain intersected with operator restrictions; production calls never inherit test grants. | G-01, G-03 |
| R42 | B42 | Exclude tests/private commands from ordinary list, web metadata and MCP list; reject external execution consistently. Dedicated test runner can discover tests. | G-01, G-03 |
| R43 | B43 | Keep private-network access an explicit operator opt-in in addition to host declaration. CLI/MCP/docs explain both gates; declaring localhost never silently relaxes SSRF policy. | G-01, G-05 |
| R44 | B44 | `http_status` returns actual HTTP status only. Capability/TLS/DNS/timeout failures raise distinct errors, never synthetic status zero. | G-01, G-03 |
| R45 | B45 | Hooks receive structured operation context: executable plus ordered redacted argv, or method/host/redacted URL. Security enforcement uses full internal values; displayed `hook.target` is sanitized. | G-01, G-03 |
| R46 | B46 | `os_version` uses an injected platform adapter; no hidden user-declared `uname`/`sw_vers` requirement. Unsupported platforms return explicit unavailable data/error. | G-03 |
| R47 | B47 | HTTP timeout is configurable, positive and bounded by operator maximum; 30 seconds remains the default. Cancellation reaches transport and response reading. | G-03, G-04 |
| R48 | B48 | `port_free` defines address/family semantics and detects a loopback listener on macOS. Check IPv4/IPv6 as requested, report probe errors, and document that availability is advisory and racy. | G-03 |
| R49 | B49 | Pure top-level expressions such as `NL = hex_decode "0a"` evaluate once in declaration order. Effectful top-level execution remains rejected with a source diagnostic. | G-02 |
| R50 | B50 | Simulation tracks reachability and certainty: fail in an unknown branch is possible failure, not definite failure. Bare bins use executable+argv, fixtures match them, and `--sim-no-subprocess` prevents all real subprocesses. | G-03 |
| R51 | B51 | Dry-run traverses all match arms, displays full predicate arguments, and preserves numeric argv order beyond index 9. It labels conditional effects and redacts secrets without executing effects. | G-01, G-03 |

### Missing features and documentation

| Requirement | Source | Required behavior / acceptance criterion | Goal |
|---|---|---|---|
| R52 | M01: arithmetic | Add typed add/subtract/multiply/divide/modulo and rounding; checked integer overflow, finite floating results and divide-by-zero errors. Counters, averages and rates have executable examples. | G-04 |
| R53 | M02: date math | Add RFC3339 parsing, duration arithmetic, calendar-day addition and differences with explicit timezone. Inject clock; distinguish 24 hours from a calendar day across DST. | G-04 |
| R54 | M03: else | Add a single optional else branch to if with checker definite-assignment merging and dry-run/simulate support. | G-02, G-04 |
| R55 | M04: continue | Continue advances the nearest loop; reject outside loops; cleanup/finally behavior must still run. Parallel loop continue skips only its current item. | G-02, G-04 |
| R56 | M05: enum args | Add enum values to command/template/typed-bin schemas. Invalid input fails before effects across CLI/MCP/web; help and controls show legal choices. | G-03, G-04 |
| R57 | M06: random bytes | Cryptographic random bytes with explicit hex/base64 output and bounded length; entropy failure errors, never falls back to pseudo-randomness. | G-04 |
| R58 | M07: regex escape | Escape literal input using the active regex engine's quoting rules; metacharacter and Unicode round-trip tests. | G-04 |
| R59 | M08: JSON iteration/keys | Typed arrays iterate natively; object keys returns a sorted typed string list. Non-object keys requests type-error. | G-02, G-04 |
| R60 | M09: parallel over list | Bounded workers, isolated bindings, deterministic input-order results, cancellation, errors identified by item index. No implicit shared mutable counters. | G-01, G-04 |
| R61 | M10: typed bins | Implement the declared-call contract in [typed-bins design](../typed-bins.md): typed slots/defaults/aliases, argv arrays and scope checks before spawn; prevent bypass via bare/exec invocation of interface-only bins. Clearly state that this validates arguments, not child-process internals. | G-01, G-04 |
| R62 | D01: op reference gaps | Catalogue every reported missing op: has_bin, bin_version, port_free, http_status, wait_for_url, detect_pkg_mgr, pkg_install, version_*, ensure_line_in_file, path_join, count_lines, bundle_dir, with_cwd, with_env, match, try, grep/tail/cut. Every shipped signature/example is checked against registry and parser. | G-05 |
| R63 | D02: regex examples | Correct regex_replace arity and Walkthrough 8 regex_match argument order; execute those exact snippets in CI. | G-05 |
| R64 | D03: syntax/docs | Reconcile backticks, multiline strings, match syntax, rescue binding and zero-ambient rules across skill/language/guide/errors/op reference. Unsupported syntax is never presented as shipped. | G-05 |
| R65 | D04: stale MCP binary | Remove/rebuild stale root binary through reproducible packaging; distributed MCP parses apostrophes in comments and reports matching build identity. Never smoke-test only `go run`. | G-03, G-05 |
| R66 | D05: version mismatch | One authoritative product version feeds CLI version/help, MCP initialize, packaged artifacts and docs build. Distinguish product version from the user's program version. | G-03, G-05 |
| R67 | UQ-02; VHCO Golden Rules 1, 5–8 | New semantic contracts live in domain/consumer-owned interfaces; adapters accept explicit inputs; composition/Impls live in orchestrator. Record existing violations, migrate touched responsibilities, and add no new top-level folder. | G-05 |
| R68 | UQ-01; documentation standard §§4.2, 12.1, 21, 34 | Preserve evidence, map every requirement to user outcome/change/file/test, record compatibility decisions, and require test/demo/manual/version evidence before release claims. | G-05 |

The report's separate HTTP missing-feature request is fully covered by R8/R47; it is not omitted or counted twice.

### Addendum defects and features (B52–B67, M11)

| Requirement | Source | Required behavior / acceptance criterion | Goal |
|---|---|---|---|
| R69 | B52 | Resolve each declared bin to a canonical absolute path at preflight (declared path, or PATH lookup recorded once). Exec, bare-bin and typed-bin calls authorize the resolved canonical path, not the basename. A user-supplied path whose basename matches a declared bin (`-interpreter=/tmp/x/python3`) is denied unless that exact canonical path is declared. Symlinked executables are resolved before comparison. | G-01 |
| R70 | B53 | Every redirect hop is authorized against the effective `requires host` set and the operator `--allow-host` set, as well as the SSRF guard. An undeclared redirect target is a `host_not_declared` error naming the hop; credentials are stripped on cross-origin hops (R7). | G-01 |
| R71 | B54 | Regex capture groups are available: `regex_find_all` accepts a group selector (index or name) and returns a typed list, and `regex_match_groups` returns a typed list or object of groups for the first match. Whole-match default behavior is unchanged. | G-02, G-04 |
| R72 | B55 | Scan reports only true environment reads (`get_env`, `requires env`) under environment inputs, never local or top-level bindings. Write/read roots are shown as the declared effective roots (resolved, or explicitly marked dynamic), and test-only grants are listed separately from production roots. | G-03 |
| R73 | B56 | A missing non-optional `requires env` fails only the commands whose reachable body reads that variable, before any effect, naming the variable. Commands that never read it run normally. `--check` reports which commands depend on each required variable. | G-01, G-03 |
| R74 | B57 | Reserved-name checking (R22) also covers auto-bound names (`config_dir`, `data_dir`, `cache_dir`, `home`, `script_dir`, `temp_dir`, `cwd`, `os`, …). Shadowing them with an arg, local or binding is a check-time error with a rename suggestion. | G-02, G-03 |
| R75 | B58 | Every runtime failure carries a specific structured `err.kind`, never `unclassified` for known failure classes. Archive/compression failures and hook vetoes get dedicated kinds (e.g. `archive_error`, `hook_veto`). A test enumerates the op catalogue and fails if any op can surface `unclassified`. | G-02, G-03 |
| R76 | B59 | Positional (`index N`) args also accept named `-name=value`. Supplying one arg both ways is an `invalid_argument` error. Help shows both forms. | G-03 |
| R77 | B60 | Built binaries extract bundles into a per-process directory that is removed on normal exit, error exit and signal, or reuse a content-addressed cache with documented location and eviction. Tests assert that no `perch-bundle-*` directory remains after each exit path. | G-03 |
| R78 | B61 | Document `trim` precisely (leading/trailing whitespace including newlines). Once R9 lands, literal and binding assignment no longer needs `trim`; migration guidance replaces `x = trim "..."` workarounds where exact bytes matter. Add `trim_space`/`trim_newline` options only if the maintainers accept them; `trim` is not silently redefined. | G-02, G-05 |
| R79 | B62 | Auto-bind `cwd` alongside the other environment names, with its effective value (not source dir). It is covered by R74 reserved-name checks. | G-02 |
| R80 | B63 | `http_status` documents its method. It accepts a `method` option (default `HEAD`), and a documented fallback (`HEAD` → `GET` on 405/501) that can be disabled. The returned status always reflects the final request actually sent. | G-03, G-04 |
| R81 | B64 | Help and catalogue types come from declared or inferred binding types. A quoted literal is always `string`; numbers only from unquoted numeric literals. Help, `/api/program` and MCP schemas show the same type. | G-03, G-05 |
| R82 | B65 | Define one capture contract: a captured exec does **not** echo stdout to the terminal by default; stderr handling follows R28 modes; an explicit `tee`/echo option prints and captures. Docs, dry-run and trace describe the same behavior. | G-02, G-05 |
| R83 | B66 | `list_commands`, `--help`, `/api/program` and MCP `perch_list` return commands in one deterministic order (declaration order), consistently across runs and surfaces. | G-03 |
| R84 | M11 | Add a `requires host public` grant, or an equivalent spelling frozen in P0. It admits any host whose every resolved address is public, and it remains subject to the SSRF, private-address and redirect checks (R70). Scan and help flag it as a broad grant, and it can never admit loopback, link-local, private or metadata addresses. | G-01, G-04 |
| R85 | B67 | `--check` description warnings name the missing level (command vs arg) and the command, and suppress the command-level warning only if the maintainers adopt an explicit rule for it. | G-05 |

## Use Cases

### Catalogue and common procedure

For each row: (1) author supplies the stated source/input, (2) invokes the surface, (3) receives the expected result, (4) on error corrects the named input/declaration and retries. No failure branch should perform a denied effect. Tests run on macOS/Linux/Windows unless a platform-specific assertion is stated. Proposed fixture files are created by the tests listed in F12; they do not exist yet.

| ID | Actor / trigger / preconditions | Surface, input and expected result | Negative path and recovery | Requirements / goals / tests |
|---|---|---|---|---|
| UC-01 | Operator runs a declared file/archive/service command in a temporary workspace | `perch -f policy.perch archive`; permitted archive round-trips bytes. `perch -f policy.perch restart_service web -namespace=kube-system -confirm=web` sends kube-system to a fake service adapter. | Missing args, bad flags, denied root or occupied symlink errors before effects; correct args/explicit grant and retry. | R1–R3, R29–R30, R33, R38–R40, R76; G-01/G-03; T-01 |
| UC-02 | Agent connects to MCP with an operator restriction configuration and a program containing public/private/test/catch | `perch-mcp -f policy.perch --no-subprocess --allow-host=api.example.test --env=TOKEN`; initialize/list exposes only public commands; run respects identical caps. | Private/test invocation returns tool error even with catch; unsupported option or required unavailable sandbox prevents server start. | R4–R5, R42–R43, R83; G-01/G-03; T-02 |
| UC-03 | Automation author calls a controlled HTTP fixture with an injected test credential | `perch -f network.perch fetch`; header arrives at fixture, structured response has actual status/headers/body, timeout follows config. Hooks record sanitized method/host/argv. | 401 is an HTTP status error; TLS/DNS/timeout/policy remain distinct; fix credential or grant. No token in any output. | R7–R8, R43–R45, R47, R70, R80, R84; G-01/G-04; T-03 |
| UC-04 | Author assigns values, calls templates/commands, iterates lists and handles errors | `perch -f semantics.perch verify`; outputs captured typed values and one recovery event, preserving literal bytes and caller state. | Unknown predicate, wrong arity/type, missing return use, invalid default or collision yields a source diagnostic; fix source then rerun. | R9–R28, R31, R49, R71, R74–R75, R78–R79, R82; G-02; T-04 |
| UC-05 | Author checks a program with immutable manifest config and runs isolated tests | `perch -f checks.perch --check`; valid program succeeds; `perch -f checks.perch test` gives test-only fixture rights and verifies expected denial. | Invalid root expansion fails explicitly; possibly unbound value is diagnosed; add required binding or test declaration, not production authority. | R31–R36, R38–R41, R73, R85; G-01/G-03; T-05 |
| UC-06 | Operator inspects a program without running effects | `perch -f inspect.perch --scan`, `perch -f inspect.perch simulate --sim-no-subprocess`, `perch -f inspect.perch --dry-run`; reports correct subprocess use, uncertainty and argument order. | Unknown branch remains uncertain; invalid source is an error; supply fixtures to refine simulation. | R6, R50–R51, R72; G-03; T-06 |
| UC-07 | Distributor bundles files and installs CLI/MCP; author probes OS/ports | `perch -f bundle.perch --build -o ./app`; source and built `extract` produce the same tree. `perch --version` agrees with product help and MCP initialize. | Conflicting bundle paths, unavailable platform probe or occupied port return specific diagnostics; fix bundle/probe input. | R36–R37, R46, R48, R65–R66, R77, R81; G-03/G-05; T-07 |
| UC-08 | Author computes counters, dates, escaped patterns and JSON keys | `perch -f values.perch verify`; deterministic arithmetic/date results with frozen clock, sorted keys and correct random byte length/encoding. | Overflow, invalid date/timezone, entropy failure, invalid regex input or JSON type mismatch errors; correct input. | R52–R53, R57–R59; G-04; T-08 |
| UC-09 | Author branches, skips loop items, validates enums and maps in parallel | `perch -f flow.perch verify -mode=fast`; else/continue execute correct branches; bounded parallel results preserve input order. | Bad enum rejected at entry; item error includes index and cancels siblings under specified mode; correct input/retry. | R54–R56, R60; G-02/G-04; T-09 |
| UC-10 | Author invokes a typed binary interface with fake executable | `perch -f bins.perch verify`; typed path/host/default/enum validated and ordered argv reaches fixture executable. | Undeclared path/host, invalid enum or direct bypass attempt denied before spawn; correct interface/declarations. | R61, R69; G-01/G-04; T-10 |
| UC-11 | Reader follows docs; web user chooses a public command and enum | Read op reference/skill, run copied examples; GET `/api/program` excludes tests/private commands; dashboard enum selector supplies a legal value. | Invalid enum fails preflight with legal choices; user corrects control and retries; no hidden command is available by crafting a request. | R42, R56, R62–R66, R81, R83; G-03/G-05; T-11 |
| UC-12 | Maintainer implements and validates the scoped change | Inspect imports and explicit dependency signatures, execute regression/doc/release checks, record evidence by requirement. | Architecture drift, absent test evidence or stale binary blocks the applicable release; repair and rerun. | R67–R68; G-05; T-12 |

### Changed surface contracts

All command examples above are proposed fixture contracts, not commands asserted to exist in today's repository. Success exits 0; argument, policy or evaluation error exits nonzero with stable code and source span on stderr. Structured results are serialized only at CLI/MCP/web boundaries. Existing CLI command syntax should be retained except where the migration section names a deliberate change.

```text
CLI: command tokens -> interspersed flag parser -> bound args -> policy -> effect
                              |
                              +-> -- ends flag parsing; remaining tokens are literal
MCP: tools/call perch_run -> lookup -> visibility -> bind -> policy -> execute
                                  | denied -> isError:true, no catch execution
                                  | unknown -> optional catch (same policy)
HTTP op: typed request -> policy -> checked connect/redirect -> response
                                            | deny/transport -> typed error
                                            | HTTP 4xx/5xx -> status error + response
```

MCP uses the existing JSON-RPC transport. Example run parameters: `{"name":"perch_run","arguments":{"command":"private_task","args":{}}}`. Proposed tool result preserves `content` text and `isError: true`; its sanitized text identifies `command_not_public`. Invalid JSON-RPC parameters remain protocol errors, not command results. Catch must never turn this refusal into a successful run. `perch_list` returns public commands only; initialize reports the authoritative product version.

Web contracts preserve the current shapes inspected in `infra/httpserver/server.go` (`handleProgram`, `execRequest`, `handleExec`). Proposed examples use a server at `http://127.0.0.1:8080` and fixture command `verify`:

```sh
curl -sS http://127.0.0.1:8080/api/program
curl -sS -N -H 'Content-Type: application/json' \
  -d '{"command":"verify","args":{"mode":"fast"}}' \
  http://127.0.0.1:8080/api/exec
```

`GET /api/program` returns `200 application/json`, retaining name/description/version/path/commands. Example command fragment: `{"name":"verify","args":[{"name":"mode","type":"enum","values":["fast","safe"],"default":"safe","has_default":true}]}`. `values` is the new field; test/private commands are omitted. Loading invalid source must fail before serving a misleading partial program; retain the current startup/load error path.

`POST /api/exec` retains request `{command,args,env_only?,allow_bin?,allow_host?}`. Any supplied narrowing options intersect operator policy. For accepted requests, `200 application/x-ndjson` streams `{"kind":"status","msg":"started"}`, zero or more `out`/`err` rows, then `{"kind":"status","msg":"ok"}` or `{"kind":"status","msg":"error"}`. Runtime errors stay in that stream after headers have been committed; their text includes a stable sanitized error code. Wrong HTTP method is `405 text/plain` (`POST only`); malformed JSON is `400 text/plain`; unknown/private/test command is `404 text/plain` (`command not found`) before streaming. Invalid enum is proposed `400 text/plain` (`invalid_argument: mode must be one of fast, safe`) before effects or streaming. Do not introduce a second JSON error envelope.

```text
Dashboard command list (infra/httpserver/index.html)
  -> public commands only -> select command -> enum select (initial/default choice)
  -> Run -> pending output -> completed output
                   | invalid enum -> inline error + legal choices -> edit -> retry
                   | server failure -> error feedback -> retry
No public commands -> empty-state text; no test/private controls.
```

Retain keyboard navigation and focus on the failing field; announce validation/output changes using the existing accessible feedback mechanism, adding one if absent. Controls show legal values but the server remains authoritative. No new navigation screen is proposed.

## Project Standards Baseline

[Standards index](../standards/index.md), STD-2026-0001 revision 1, records the actual user-provided VHCO authority and the provisional documentation reference. No project rule has been silently approved by this proposal. The repository already contains directories outside the guide's six-module example and `Impl`s outside orchestrator; this proposal acknowledges that baseline.

| Rule / source | Applicability and proposal evidence | Initial result | Follow-up |
|---|---|---|---|
| VHCO Golden Rule 2 | All proposed files remain within existing top-level folders. | PASS | Colocate tests; do not create a new root `tests/` despite the guide's conflicting FAQ. |
| VHCO Golden Rules 1, 5, 7, 8 | C-01/C-02 move touched semantic contracts to domain and orchestration behind consumer interfaces; raw adapters have no domain imports. | PARTIAL | Existing interpreter/loader/ops imports violate strict isolation. Record exact touched migrations in first implementation plan. |
| VHCO Golden Rule 6 | C-02/C-03 inject policy, clock, resolver, filesystem, process and randomness; invocation state is explicit. | PASS | Verify no new ambient reads in changed functions. |
| VHCO Golden Rules 3–4 | C-05 keeps web rendering separate from validation and execution. | PASS | Inspect current frontend implementation before enum controls. |
| Documentation §§4.2, 12.1 | UQ/G/R/UC/C/F/T chain, evidence snapshot, source impact and acceptance inventory present. | PARTIAL | Standard selection and ownership unconfirmed; enum UI integration discovery remains open. Keep draft. |
| Documentation §34 | Proposed release gates only; no claimed release/test completion. | NOT APPLICABLE | Apply when implementation reaches release. |

`PARTIAL` uses the standard's general result vocabulary (§6.4) and denotes incomplete draft conformance, not approval. Its proposal-specific §21 vocabulary would classify the unresolved standard/architecture questions as NEEDS HUMAN REVIEW. No approved exception is invented.

## Implementation Design

### Environment and feasibility

Go `1.24.1` is the declared module baseline. The loader embeds a Capy grammar and uses `github.com/olivierdevelops/capy/rust/gobind v0.12.1` plus wazero. String/token fidelity must be tested through the actual Capy-to-NDJSON boundary, not only Go unit tests. If the embedded engine cannot preserve source tokens or implement the grammar, a pinned dependency change with upstream tests is a prerequisite, not a downstream string-repair workaround.

Existing handler wrappers, catalogue generation and interpreter dispatch provide integration points. Filesystem containment needs platform-specific implementations for existing links and missing leaves. HTTP tests need injected resolver/dialer and controlled fixtures; no external service credentials are necessary. Simulation, random bytes and dates must use supplied capabilities so tests remain deterministic and simulation creates no accidental host effects.

### Methods by use case

| Change | Use cases / requirements | Method, sequence and failure behavior | Feasibility |
|---|---|---|---|
| C-01 | UC-01/04/05/09/10; R1–R3, R9–R22, R24–R27, R31–R34, R49, R54–R56, R59, R61, R71, R74, R76, R78 | Define typed expressions, operation signatures and canonical call binding. Decode tokens once; resolve names and arity before dispatch; preserve values and source locations. Validation failures produce no effects. | Integration points exist; Capy lexer experiment required. |
| C-02 | UC-04/05/08/09; R16–R19, R21, R23, R31, R49, R52–R55, R57–R60, R67, R75, R79, R82 | Fresh lexical invocation frames; explicit return/error/continue signals; one error-event owner; bounded parallel workers. Pure operations accept typed values and injected clock/random source. | Binding map exists; typed scope migration is substantial. |
| C-03 | UC-01/02/03/05/07/10; R1, R3–R5, R7–R8, R28–R29, R32–R48, R61, R69–R70, R73, R77, R80, R84 | Normalize arguments, resolve immutable manifests, intersect policies, authorize immediately before adapter effects. Same policy request used by all entry points. Reject unresolved roots, private calls and invalid transport inputs early. | Current wrappers are reusable seams; safe path and dial adapters require platform work. |
| C-04 | UC-05/06; R6, R12–R15, R22, R31–R34, R50–R51, R72, R85 | Checker/scan/simulate/preview consume the same typed call model and effect metadata. Abstract execution uses known/unknown values and reachability; display alone serializes values. | Replace duplicated interpretation incrementally; retain fixtures as contract tests. |
| C-05 | UC-02/07/09/11; R2, R5, R37, R42, R46, R48, R56, R65–R66, R76–R77, R81, R83 | Wire CLI/MCP/web to common invocation/visibility use cases; preserve bundle manifest tree; derive version from one source; provide explicit platform probes. | Entrypoints exist; Existing HTTP envelope inspected; enum rendering integration still requires discovery. |
| C-06 | UC-08/09/10; R27, R30, R52–R61, R71, R84 | Implement missing operations/control forms on typed foundations, then typed-bin interfaces; update all inspection and help surfaces simultaneously. | Native Go adapters available; syntax and subprocess-boundary acceptance required. |
| C-07 | UC-11/12; R10–R11, R15, R21, R35, R43, R62–R68, R78, R80–R82, R85 | Generate/reference operation signatures and executable docs; pin artifact/version evidence; add migration and requirement-linked release verification. No unsupported capability described as shipped. | Catalogue/drift tests exist; extension is practical. |

### Added and changed contracts

All names below are proposed. Their final parser spellings must be frozen in the first plan and used identically in tests, catalogue and manuals.

| Contract | CRUD / shape / default | Lifetime and consumers | Requirements |
|---|---|---|---|
| `Value` | CREATE: null, bool, signed int64, finite float64, string, list, string-keyed object; preserve JSON number fidelity or reject out-of-range conversion | Domain data; invocation/capture/evaluation | R9–R19, R25, R27, R52, R59 |
| `Expr`, `BoundCall`, `OpSpec` | CREATE: typed expression tree; named bound arguments; signature/arity/default/result/effect/reserved-name metadata plus source span | Parsed program, check, execution, catalogue and tools | R1–R2, R12–R15, R20–R22, R24–R26, R31, R49–R51 |
| Plain and escaped string syntax | UPDATE: plain quotes/backticks raw and multiline; CREATE explicit `e"..."` escaped form with documented escapes | Lexer only; consumers receive decoded value | R10–R11 |
| `return VALUE`, `else`, `continue`, `rescue NAME` | CREATE/UPDATE control nodes, explicit return/error/continue signals; continue outside loop rejected | Current lexical invocation; rescue name scoped to rescue | R18, R21, R23, R54–R55 |
| `InvocationFrame` | CREATE: immutable parent/global view, fresh args/locals, explicit result; no mutable caller aliasing | One call; worker gets independent frame | R16–R19, R23, R31, R60 |
| Effective policy | UPDATE: intersect manifest, operator, block and invocation restrictions; roots carry declaring-source base; no silent interpolation drop | Per invocation and effect | R1, R4–R5, R32–R43 |
| `http_request` | CREATE: method/url/header object/body/timeout/expected-status options -> object `{status, headers, body}`; default accepted status 200–299 | One request; convenience ops wrap same adapter | R7–R8, R44, R47 |
| HTTP limits | CREATE request `timeout` duration and operator `--http-timeout-max`; default request 30s, operator maximum proposed 5m; no disabling by zero | CLI/MCP policy; per request cannot raise maximum | R5, R47 |
| `exec_result` and exec stream options | CREATE structured `{stdout, stderr, exit_code}`; UPDATE stdin mode closed/inherit/value; stderr inherit/capture/discard; existing exec capture remains stdout | One child; bounded capture; cancellation kills owned process tree where supported | R28 |
| Symlink replacement / parent creation | UPDATE `symlink` replace=false default; write/append create_parents=true default, optional false | One authorized operation; reject directory replacement | R3, R29 |
| `hook.operation`, `hook.argv`, `hook.method`, `hook.host` | CREATE typed sanitized metadata; UPDATE `hook.target` to safe display value | Hook frame; no policy decisions from redacted text | R7, R45 |
| Test requirements / expected denial | CREATE test-local `requires` and proposed `expect_error CODE` block; no blanket static-error suppression | Test runner only, fresh temp dir per test | R34, R41 |
| Enum schema | CREATE `type enum` plus nonempty distinct `values`; optional validated default | Arguments and catalogue; CLI/MCP/web | R56 |
| Arithmetic/date/random/regex/JSON ops | CREATE `add`, `subtract`, `multiply`, `divide`, `modulo`, `round`, `date_parse`, `date_add`, `date_add_days`, `date_diff`, `random_bytes`, `regex_escape`, `json_keys`, `json_count` | Pure evaluation except injected clock/entropy; explicit timezone and encoding | R27, R52–R53, R57–R59 |
| `parallel_each` | CREATE list, worker limit (default 4), failure mode fail-fast default; ordered list result; first failure records index, settled failures retained | One call; worker-local state; no unlimited goroutines | R60 |
| Typed bin calls | CREATE interface entries as described in `docs/typed-bins.md`; scope-bearing read/write/host types, defaults, enum values, direct argv assembly | Imported interface and each spawn; interface-only executable cannot use raw exec path | R61 |
| Structured errors | UPDATE stable code/message/span/details; proposed codes include invalid_argument, unknown_predicate, type_error, unbound_binding, invalid_default, manifest_resolution, command_not_public, http_status_error, transport_error; preserve existing read/write/host denial codes | Runtime/check to surface adapters; sanitized details only | R4, R7, R12–R15, R19, R27, R32–R44 |
| Product build identity | UPDATE single version + commit identity injected at composition; user's program version is separate | CLI/MCP/help/build artifacts | R65–R66 |
| Canonical bin identity | UPDATE each declared bin carries its resolved canonical absolute path; authorization compares canonical paths, never basenames | Preflight resolution per invocation; exec/bare/typed-bin calls | R69 |
| Redirect authorization | UPDATE each hop re-runs host policy (manifest ∩ operator) and SSRF checks | Per request hop | R70, R84 |
| `requires host public` | CREATE grant admitting only hosts whose resolved addresses are all public; still SSRF/redirect-checked; flagged broad by scan/help | Manifest; HTTP adapters | R84 |
| Regex groups | UPDATE `regex_find_all` optional `group` (index or name); CREATE `regex_match_groups` returning list/object | Pure evaluation | R71 |
| `http_status` method | UPDATE `method` option (default `HEAD`) plus documented disable-able `HEAD`→`GET` fallback on 405/501 | One probe | R80 |
| Capture echo | UPDATE captured exec does not echo stdout by default; explicit `tee` option prints and captures | One child; trace/dry-run describe same | R82 |
| Env requirement scope | UPDATE required env evaluated per reachable command, not file-wide | Invocation preflight; check reports dependents | R73 |
| Bundle extraction lifetime | UPDATE per-process extraction dir removed on every exit path, or documented content-addressed cache with eviction | Built binary process | R77 |
| Command ordering | UPDATE declaration order in every listing surface | CLI/help/web/MCP | R83 |

`os_version`, networking, filesystem and random adapters receive explicit capabilities. Pure top-level evaluation uses only operations marked pure; entropy and host queries are not implicitly allowed as globals.

### Architecture, data and state relationships

```text
Before: parser -> loose argument map -> handler-specific lookup -> independent gate keys
After:  source -> Expr / OpSpec -> BoundCall -> effective policy -> adapter -> Value/Error
                                   |             |
                                   +-> check     +-> deny before effect
                                   +-> scan/simulate/dry-run (no live effects by default)

Call state:
  immutable globals -> frame A(args, locals)
                            | call(args)
                            v
                       frame B(args, locals) -> explicit return -> frame A capture
                            | error -> rescue OR one on_error boundary
  parallel_each -> independent frame per item -> ordered result vector

VHCO target for touched responsibilities:
  io consumer contracts -> injected invocation capability
  usecases consumer contracts -> feature capability shapes
  domain = pure Value/Expr/Policy data
  infra = raw parser events, filesystem, process, HTTP, clock, entropy
  orchestrator = adapters + evaluator/validation composition + all new Impl wiring
```

The shared semantic model is data, not a new service locator. The orchestrator adapts module-local interfaces; `infra/ops` must not become a new shared domain-policy engine. Existing mixed packages are migrated as touched, with temporary boundaries explicitly recorded in the plan.

## Expected Code and Documentation Changes

The inventory below is the known source impact, not a claim that every implementation file has already been designed. Each group lists exact known paths; `READ+UPDATE` means inspect first, then edit. Proposed CREATE paths are intentional new files. For every row, the referenced C design and contract table provide the proposed schema/pseudocode; test/doc destinations are explicit. No unnamed “relevant files” are authorized.

| ID | Paths and CRUD | Region / precise proposed edit | Changes / requirements | Tests and documentation |
|---|---|---|---|---|
| F01 | READ+UPDATE `domain/program.go`, `domain/bin.go`, `domain/errors.go`; CREATE `domain/value.go`, `domain/expression.go`, `domain/operation.go` | Add Value/Expr/BoundCall/OpSpec/control/enum/test/bundle metadata and source spans; structured result/error contracts. | C-01/C-02/C-06; R1–R2, R9–R28, R31–R34, R41, R49–R61, R75 | T-01/04/05/08/09/10; language/op reference/errors |
| F02 | READ+UPDATE `infra/capyloader/lib.capy`, `loader.go`, `registry.go`, `opkinds.txt` | Preserve raw tokens and multiline bytes in emitted events; parse new expression/control declarations; derive reserved names; move event-to-domain assembly into orchestrator. | C-01; R9–R22, R31, R41, R49, R54–R56, R61, R67, R74, R76 | T-04/05/09/10/12; language/skill |
| F03 | READ+UPDATE `infra/interpreter/interpreter.go`, `bindings.go`, `interpolate.go`, `infra/ops/flow.go` | Replace string-reparse capture and shared call bindings with typed evaluation/frames; propagate signals once; adapt legacy entrypoints to orchestrator implementation. | C-01/C-02; R2, R9–R23, R28, R31, R49, R54–R55, R60, R67, R74–R75, R82 | T-01/04/09/12; guide/errors |
| F04 | READ+UPDATE `infra/ops/strings.go`, `regex.go`, `encoding.go`, `paths.go`, `time.go`; CREATE `infra/ops/random.go` | Typed repeat/format/replace/JSON contracts, glob path shape, regex escaping and explicit clock/entropy adapters. Move pure language evaluation into C-02 evaluator rather than adding new domain imports to infra. | C-02/C-06; R10–R11, R17, R24–R27, R30, R52–R53, R57–R59, R67, R71, R78 | T-04/08/12; op reference |
| F05 | READ+UPDATE `infra/ops/archive.go`, `compression.go`, `files.go`, `requires.go`, `restrict.go`, `infra/capyloader/enforce.go` | Canonical argument gate, explicit manifest errors and script-relative roots; no implicit symlink deletion; parent creation and path containment. Extract semantic policy to orchestrator with raw path adapter inputs. | C-03; R1, R3, R29, R32–R41, R67, R69, R73, R75, R84 | T-01/05/07/10/12; requires/sandbox/testing |
| F06 | READ+UPDATE `infra/ops/http.go`, `network.go`, `process.go`, `hookcat.go`, `system.go`, `infra/interpreter/autovars.go`, `infra/report/report.go`, `trace.go`, `infra/audit/audit.go` | HTTP/exec structured adapters and bounded timeouts/capture; sanitized diagnostics/hooks; platform/port probes; inject auto-var environment inputs. | C-03/C-05; R7–R8, R28, R36, R43–R48, R67, R70, R79–R80, R82, R84 | T-03/04/07/12; hooks/sandbox/op reference |
| F07 | READ+UPDATE `usecases/validate/validate.go`, `usecases/scan/scan.go`, `risk.go`, `usecases/simulate/simulate.go`, `state.go`, `stateful.go`, `fixture.go`, `infra/preview/preview.go` | Shared typed effects and scope analysis; certainty lattice, bin fixture identity and no-subprocess; render complete conditional branches and ordered args. Keep consumer contracts here, move touched Impl composition to F10. | C-04; R6, R12–R15, R22, R31–R34, R50–R51, R67, R72–R74, R85 | T-05/06/12; simulate/testing |
| F08 | READ+UPDATE `io/cli/cli.go`, `cmd/perch-mcp/main.go`, `usecases/listcommands/listcommands.go`, `infra/httpserver/server.go`, `infra/httpserver/index.html` | Interspersed flags, capability option parity, public visibility before catch, enum schema and selector; compose concrete adapters in orchestrator. | C-03/C-05; R2, R4–R5, R42–R43, R47, R56, R66–R67, R76, R81, R83 | T-01/02/03/11/12; mcp/web-ui |
| F09 | READ+UPDATE `usecases/runtests/runtests.go`, `usecases/runbuild/runbuild.go`, `infra/ops/bundle.go` | Test-local authority, expected-denial annotation, fixture cleanup; logical bundle manifest and source/build extraction parity. | C-03/C-05; R34, R36–R37, R40–R41, R65, R77 | T-05/07; testing/embedding |
| F10 | READ+UPDATE `orchestrator/orchestrator.go`; CREATE `orchestrator/evaluation.go`, `orchestrator/invocation.go`, `orchestrator/policy.go`, `orchestrator/validation.go`, `orchestrator/version.go`; CREATE `usecases/invoke/invoke.go` | Own all new composition; inject normalize/evaluate/authorize/execute functions through consumer contracts; unify external invocation and version. Split existing factories only as touched. | C-01–C-06; R1–R61, R65–R67, R69–R85 | T-01–T-12; architecture/migration |
| F11 | READ+UPDATE `infra/opcatalog/catalog.go`, `argdesc.go`, `docs.go`, `examples.go`, `requirements.go`, `usecases/help/help.go`, `catalog.go`, `usecases/commandhelp/commandhelp.go` | Export canonical signatures/types/defaults/effects; regenerate examples/help and remove stale product-version text; keep catalogue assembly in orchestrator under VHCO. | C-01/C-05/C-07; R22, R56, R62–R67, R74, R81, R83, R85 | T-11/12; op reference/guide |
| F12 | CREATE `orchestrator/remediation_test.go`, `orchestrator/remediation_security_test.go`, `orchestrator/remediation_tools_test.go`, `orchestrator/remediation_features_test.go`, `cmd/perch-mcp/remediation_test.go`, `infra/httpserver/remediation_test.go`, `infra/capyloader/remediation_test.go` | Requirement-keyed fixture tests T-01–T-12; construct fixture source/temp dirs in tests, use fake side-effect adapters and sentinel secrets. | C-01–C-07; R1–R85 | All tests; testing record described below |
| F13 | READ `go.mod`, `go.sum`, `main.go`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `scripts/install.sh`, `scripts/install.ps1`, `Formula`, `perch-mcp`, `infra/httpserver/server.go`, `docs/typed-bins.md` | Discovery: identify Capy lexer limitation, packaging/version sources, binary tracking, current HTTP request/response/error schemas and existing enum field rendering. Resolve changes to exact paths before plan approval. | C-01/C-05/C-07; R10–R11, R42, R56, R61, R65–R66 | T-07/10/11/12; packaging and interface inventory |
| F14 | READ+UPDATE `.github/workflows/ci.yml`, `.github/workflows/release.yml`; DELETE `perch-mcp` if tracked build output, otherwise replace only through release build | Run selected conformance suites/docs examples and package smoke tests; do not hand-edit binaries. If root binary is an intentional distribution input, document and automate rebuild instead of deletion. | C-05/C-07; R62–R68 | T-07/11/12; release verification |
| F15 | READ+UPDATE `docs/op-reference.md`, `guide.md`, `language.md`, `errors.md`, `sandbox.md`, `requires.md`, `testing.md`, `mcp.md`, `hooks.md`, `simulate.md`, `typed-bins.md`, `web-ui.md`, `embedding.md`, `skills/perch/SKILL.md`, `README.md`, `CHANGELOG.md`, `mkdocs.yml` | For each changed contract, document normal/error journey and capability requirements; correct reported snippets; clearly separate proposed from shipped; add navigation to migration/reference only when appropriate. | C-07; R1–R85 | T-11/12; every requirement maps to a manual section at release |
| F16 | CREATE `docs/migrations/mig-2026-0001-perch-semantics-and-policy.md`, `docs/testing/test-2026-0001-perch-remediation.md`, `docs/demos/demo-2026-0001-perch-remediation.md`, `docs/reports/rpt-2026-0001-perch-remediation-validation.md`, `docs/architecture/components/arch-2026-0001-perch-evaluation-and-policy.md` | Migration before/after cases; test executions; per-update runnable verification; per-requirement results; touched VHCO boundaries. These are future implementation artifacts, not created or claimed complete now. | C-07; R1–R85 | T-12 and release evidence |

F13 resolution rule: locate each actual writer/consumer using symbol/reference search; add exact UPDATE/CREATE paths and signatures to the owning plan before implementation. Possible dependency edits are not assumed necessary. `Formula` is a READ directory inventory, not permission to rewrite every formula. Generated completions, editor syntax and WASM bridge consumers must be inventoried for each new syntax/flag, then added by exact path; their discovery cannot be waived as “out of scope.”

Reviewable pseudocode for the inventory (illustrative Go, not a promised ABI):

```go
// F01/F02: syntax is converted once; no downstream string reparsing.
type BoundCall struct { Kind string; Args map[string]Value; Span SourceSpan }
// F03/F10: all state and capabilities are supplied, all outputs explicit.
func Evaluate(expr Expr, frame Frame, caps EvalCapabilities) (Value, Signal, error)
func Invoke(req Invocation, caps InvocationCapabilities) (Result, error)
// F05/F06: authorize the exact normalized target, then pass raw inputs to adapter.
call := Bind(spec, syntax, frame)
policy := ResolvePolicy(manifest, sourceBase, immutableConfig, operatorPolicy)
if err := Authorize(call, policy, pathResolver); err != nil { return err }
return Execute(call, adapters) // safe-open and checked-dial enforce at effect time too
// F07: absent evidence cannot become a definite outcome.
verdict := Analyze(call, KnownOrUnknownState, Reachability, fixtureCapabilities)
// F08/F09/F11: expose public metadata and canonical contracts only.
visible := FilterCommands(program.Commands, PublicInvocation)
// F12/F14: same source fixture -> check, runtime, tool output, packaged executable.
AssertRequirementCase(id, fixture, expectedValueOrError, expectedSideEffects)
```

F15's content transformation is `documented signature/example -> canonical OpSpec + executed fixture`; F16's schema is `requirement, source commit, environment, exact action, expected, actual, result, evidence`. F14's workflow is `build from one commit -> run actual CLI/MCP artifact -> compare version/behavior -> record checksums`.

Inventory summary: 16 source-impact groups, including exact CREATE test/semantic/document paths; existing implementation paths are READ+UPDATE, F13 is bounded READ/discovery, and the only conditional DELETE is the stale root executable. No deletion of user source or original report is proposed. The documentation created now is listed in the document index; the implementation inventory remains future work.

## Alternatives Considered

| Alternative | Benefit | Cost / reason rejected or selected | Requirements |
|---|---|---|---|
| Patch each handler only | Fast local repairs | Selected only for urgent security corrections; alone preserves parser/checker/policy drift and repeated bugs. | R1–R8, R24 |
| Change docs to describe every current behavior | Low implementation effort | Appropriate for zero-ambient and localhost dual gates; unacceptable for silent wrong values, deletions or private-command execution. | R3–R4, R9–R27, R35, R43 |
| Rewrite the language/runtime wholesale first | Clean final structure | Delays security relief and increases migration risk. Incremental typed contracts with explicit adapters selected. | R1–R85 |
| Enable ambient filesystem/network access | Fewer manifest errors | Rejected: removes safety rather than resolving roots/types. | R32–R43 |
| Treat typed bins as a full sandbox | Appealing product claim | Rejected: validating declared argv cannot constrain arbitrary behavior inside a host executable. | R61 |

## Risks and Rollback

| Risk / detection | Impact | Mitigation and rollback | Proposed owner |
|---|---|---|---|
| Raw strings and lexical scope change previously working scripts; corpus diff | Broken scripts | Publish diagnostic migration first; staged prerelease and known fixtures. Roll back feature release if needed, preserving security patch. | Language maintainer |
| Shared argument normalization omits an effect path; deny-effect tests | Policy bypass | Fail closed on absent required args/unknown effect metadata; adversarial tests through every surface. Never restore unsafe old authorization as rollback. | Security maintainer |
| TOCTOU paths or DNS rebinding; link/dial race tests | Out-of-root/network access | Safe path handles and checked dial per redirect, not only string prefix or preflight DNS lookup. Disable unsupported unsafe operation on a platform until implemented. | Platform maintainer |
| Capy dependency cannot preserve token semantics; round-trip experiment | Blocks language design | Pin compatible engine update with source-token tests; keep grammar migration out of emergency patch. | Language maintainer |
| Parallel execution leaks state or ignores cancellation; race tests | Wrong results/resource leaks | Isolated workers, bounded limits, deterministic order and cancellation; disable new parallel feature without reverting other fixes. | Runtime maintainer |
| Catalogue/docs overstate release support; snippet/artifact checks | Repeated drift | Generate contracts, execute examples, verify actual installed artifacts. Reissue artifacts/docs from fixed commit. | Release maintainer |

## Security Impact

Effective authority is the intersection of operator restrictions and the applicable manifest/block/test context. Test-only declarations never raise operator limits. Missing requirements retain zero ambient effects; unknown roots are errors, not silently removed grants. CLI/MCP/web all pass through external visibility and invocation policy before catch.

Path containment must protect actual filesystem operations, archive entries and newly created parents, not just normalized strings. HTTP revalidates redirect hosts, strips credentials on cross-origin redirects and constrains the actual dialed address. Every redirect hop is re-authorized against manifest and operator hosts, not only the initial URL (R70). A `public` host grant admits only hosts whose resolved addresses are all public, and it never widens the private-address guard (R84). Declared executables are authorized by canonical absolute path, so a same-named binary elsewhere is not admitted (R69). Diagnostics omit userinfo/query secrets and sensitive headers, including nested causes. Unknown executable arguments in hooks should be conservatively redacted unless a typed declaration identifies safe display fields.

Native HTTP eliminates a common need for curl but does not prevent an explicitly permitted arbitrary subprocess from networking. `--require-sandbox` needs a real enforceable backend; unsupported systems fail closed. Typed bins reduce argument ambiguity but are not process confinement. Preserve this distinction in scan, help and docs.

## Operational Impact

Keep HTTP's default timeout at 30 seconds; add a finite operator ceiling. Bound response/exec capture, archive expansion, repeat allocation, glob traversal and parallel workers, with configurable limits supplied through execution options. Establish concrete defaults and regression limits in the owning implementation plan before release; no unlimited resource mode is implied.

`port_free` is an advisory probe, never a lock or guarantee that a subsequent bind succeeds. Network/platform probes expose errors instead of false confidence. Program source base, effect cwd, fixture root and bundle root are separate values and remain visible in safe diagnostics.

## Compatibility Impact

Intentional breaking corrections: raw string escape interpretation, lexical command scope, explicit returns/null instead of empty-string results, unknown predicate/type errors, non-2xx HTTP failure, closed child stdin, script-relative manifest roots, hidden test commands, refusal to overwrite symlinks implicitly, canonical-path bin authorization (R69), redirect-hop host checks (R70), rejection of args that shadow auto-bound names (R74), and captured exec no longer echoing to the terminal (R82). Per-command evaluation of required env vars (R73), deterministic command order (R83) and named use of positional args (R76) are relaxations or additive. Parent directory creation and recursive glob are additive behavior with security-sensitive side effects.

Use a clearly documented compatibility release; maintainers choose its version after baseline identity is established. Do not pretend `0.1.0` and `0.1.2` identify reproducible builds. A compatibility mode may preserve safe display/default behavior temporarily, but must not re-enable gate bypasses, private access, credential exposure or destructive symlink defaults. Prefer source migration diagnostics over indefinite dual evaluators.

## Migration Requirements

1. Record installed CLI/MCP paths, hashes, versions/help and source commit before reproduction.
2. Turn report cases into minimal fixtures; run the original 27-project corpus only with fake/local dependencies and explicit sandboxing. Record unavailable projects rather than claiming coverage.
3. Convert strings relying on escapes to explicit escaped form; retain regex backslashes as raw text.
4. Replace caller access to callee locals with explicit return/capture; make defaults and template arguments lexical.
5. Replace ambiguous comma replacement with separate args; add required stdin mode and HTTP expected statuses/header options.
6. Anchor manifest roots to declaring source; use test-local fixture grants and explicit private-IP operator opt-in where necessary. Declare every redirect target host, or use the `public` grant where appropriate; declare bins by the path actually executed.
6a. Rename args or bindings that shadow auto-bound names; add an explicit `tee` where terminal echo of a capture was relied on; remove `trim` from literal-assignment workarounds where exact bytes matter.
7. Rebuild bundles and MCP artifacts, re-run source-versus-built parity and exact help/version checks.
8. Run migration diagnostics, check and runtime fixtures; publish before/after examples and known unsupported cases before marking the feature release ready.

## Test and Validation Design

All tests below are **planned, not executed**. F12 names their files; F16 names execution/validation records. Each requirement row supplies its specific expected outcome; tests must enumerate subcases by R ID so one passing broad test cannot hide an uncovered item.

| Test | Kind / use cases | Procedure, environment and purpose | Expected result / evidence |
|---|---|---|---|
| T-01 | Regression/security/E2E; UC-01 | In `orchestrator/remediation_security_test.go`, table-test all archive/compression named/positional/missing/denied paths, symlink occupied destinations, parent creation, traversal, platform aliases, flags around positionals/`--`, and `index N` args supplied positionally, by name, and both (R76). Use temp roots and effect-recording adapters; run `go test ./orchestrator -run TestRemediationPolicy`. | Correct bytes/argv; all denied paths untouched; existing destinations unchanged without explicit replacement. |
| T-02 | MCP protocol/security; UC-02 | `go test ./cmd/perch-mcp -run TestRemediationMCP`; stdio initialize/list/run public/private/test/unknown with and without catch and each restriction flag; attempt inner policy widening; list twice and compare order with CLI/web (R83). | Only public listed/callable, unknown-only catch, flags enforced before effects, unavailable required sandbox fails startup, identical deterministic order on every surface. |
| T-03 | HTTP/process/security; UC-03 | `go test ./orchestrator -run TestRemediationNetwork`; fake resolver/dialer and local HTTP/TLS fixtures: auth headers, 200/204/401/404/500, redirect host/downgrade/private address, rebinding, timeout/DNS/TLS/cancellation. Capture hook/audit/trace/web/MCP output with sentinel secrets. Test child stderr/stdin modes. Redirect from a declared to an undeclared host (R70); `http_status` method option and HEAD→GET fallback against a fixture rejecting HEAD (R80); `requires host public` against public, loopback, private, link-local and metadata addresses and via redirect (R84). | Actual status or typed failure; no forbidden dial, leaked sentinel, hung stdin or uncancelled child/request; undeclared redirect hop denied by name; `public` never reaches non-public addresses. |
| T-04 | Parser/value/scope regression; UC-04 | `go test ./infra/capyloader ./orchestrator -run TestRemediationSemantics`; parse real source through embedded Capy and evaluate every R9–R28 case, including Unicode, backslashes, multiline bytes, quoted chains, two sequential calls, nested recovery and defaults. Also: regex capture groups by index/name (R71); shadowing every auto-bound name including `cwd` (R74, R79); catalogue-wide assertion that no op can surface `err.kind = unclassified`, with archive and hook-veto kinds (R75); `trim` newline semantics and literal assignment without `trim` (R78); captured exec produces no terminal echo unless `tee` (R82). | Byte-preserved literals, typed results, isolated callers, one recovery event; explicit source errors on invalid input; specific error kinds; no unintended echo. |
| T-05 | Check/runtime parity and isolation; UC-05 | `go test ./orchestrator -run TestRemediationCheck`; paired checker/runtime fixtures for all branch merges, immutable roots, write-implies-read, source/cwd distinction, unresolved bundle root, test-only grants and expected denial. A file with a required env var used by one command: the other commands run when it is unset (R73). Description-warning text for command-level versus arg-level omissions (R85). | Check agrees on definite facts; dynamic facts marked uncertain; runtime always gates; production cannot inherit test grants. |
| T-06 | Tool conformance; UC-06 | `go test ./orchestrator -run TestRemediationTools`; compare scan/simulate/preview on same source; unknown/known if/match, 12 argv items, bare/explicit exec and oracle fixtures; replace real exec with a fail-if-called capability. Scan environment-input and root sections against a fixture with local bindings, placeholders and test-only grants (R72). | Honest uncertainty, correct executable/ordered args, no live subprocess under sim restriction or dry-run; recommendation does not break required paths. |
| T-07 | Build/platform/artifact; UC-07 | `go test ./orchestrator -run TestRemediationPackaging`; build temp CLI/MCP/app artifacts, execute them, compare source/built bundle trees and checksums; version/help/MCP initialize consistency; apostrophe-comment fixture. Bind real loopback IPv4/IPv6 listeners on each OS for port probes. Run built binary to normal exit, error exit and SIGTERM and assert no leftover `perch-bundle-*` (R77). Help/catalogue type of quoted `"3.12"` is `string` (R81). | Tree/layout parity and matching product identity; listening port unavailable; unsupported address family reported explicitly. |
| T-08 | Unit/property/failure; UC-08 | `go test ./orchestrator -run TestRemediationValues`; overflow/divide-zero/rounding, frozen RFC3339 instants and DST boundaries, entropy error/length/encoding, regex literal property and sorted JSON keys/type errors. | Defined values/errors without silent coercion; deterministic clock tests and injected entropy failure. |
| T-09 | Control/concurrency/UI schema; UC-09 | `go test -race ./orchestrator -run TestRemediationFlow`; else/continue nesting and cleanup, enum valid/invalid/default, empty list, worker limit, ordered results, item failure/cancel and caller state. | Correct branches, no races/state leakage, bounded workers, all workers stop on cancellation. |
| T-10 | Typed-bin E2E/security; UC-10 | `go test ./orchestrator -run TestRemediationTypedBins`; fake argv-recording binary, scope slots/default/enum/aliases, shell metacharacters as literal argv, bare/exec bypass attempts and inspect-tool parity. A same-basename executable at an undeclared path and a symlink to a declared bin (R69). | Exact argv without shell parsing; bad scope/default/enum fails before spawn; docs clearly limit guarantees. |
| T-11 | Docs/catalogue/web; UC-11 | `go test ./infra/opcatalog ./infra/httpserver -run 'TestRemediation|Test.*Catalog|Test.*Example'`; execute corrected/new documentation snippets. Exercise GET `/api/program` and POST `/api/exec`, invalid enum/private/test requests; manually use dashboard selector by keyboard and verify error recovery. | Catalogue coverage, no hidden commands, legal enum values, consistent error envelope, usable focus/feedback; packaged examples work; types and command order match across help, `/api/program` and MCP (R81, R83). |
| T-12 | Build/static/release/traceability; UC-12 | `go test ./...`, `go vet ./...`, and supported CI race jobs after targeted suites; inspect touched module imports/signatures, validate doc metadata/links and full R/UC/C/F/T coverage, smoke actual distribution artifacts. | Required checks pass; all 85 requirement outcomes have recorded evidence, no unapproved architecture exception or unverified release claim. |

Parser fuzz seeds: quoting/escaping/newlines/chain delimiters; property: serialized token values round-trip and no quoted chain executes. Policy fuzz seeds: path separators/dot segments/symlink paths/archive entries; property: denied effects never occur. Execute under CI time/resource limits and retain failing corpus inputs beside the owning test. These extend T-01/T-04 rather than replace deterministic regressions.

No speedup or throughput claim is made. Measurable acceptance is functional: zero denied effects, zero sentinel leaks, exact list length/order, no more active workers than configured, one handler event per propagated error, and cancellation completion under a predeclared CI timeout. Record environment, baseline failure, expected threshold, observed result and commit in TEST-2026-0001; do not label unexecuted cases PASS.

## Documentation, Demo and Release Impact

| Artifact | Destination / operation | Required content and gate |
|---|---|---|
| User manual and CLI/API reference | F15 UPDATE | Current behavior, exact normal/error examples, all new syntax/options and migrations; op list checked against registry. |
| Architecture/system | F16 CREATE architecture record; F15 UPDATE execution-related manuals | Typed evaluation/policy composition, source base/cwd distinction and explicit subprocess limitations. |
| Migration | F16 CREATE MIG-2026-0001 | Every intentional break above, diagnostic, replacement syntax and tested before/after behavior. |
| Demo | F16 CREATE DEMO-2026-0001 | One `U-NN` verification row per released change, linking its R IDs, exact invocation, expected output and execution evidence. |
| README | F15 UPDATE | Accurate supported features and compatibility notices; no claim typed bins are implemented before release. |
| Tests/report | F16 CREATE TEST/RPT | Per-requirement actual results and residual failures, environment and exact source identity. |
| Version/artifacts | F10/F13/F14 | Single product version, commit, built artifact hashes and actual artifact smoke results. |
| Release | Future `docs/releases/v<major>/rel-<version>-perch-remediation.md` | Allocate path only after version decision; exact tag/commit, plan/test/report/demo/manual links and remaining limitations. |
| Index | UPDATE `docs/index/document-index.md` | Register created records and revisions; never link a planned artifact as if it exists. |

Release owner is the proposed maintainer role until assigned. Part II of the provisional documentation standard supplies the implementation-to-release evidence chain; preparing this proposal neither tags nor publishes anything.

## Requirements Alignment

Every row below links back to UQ-01; R67 additionally links UQ-02. Tests expand their listed requirement IDs into individual positive, negative and boundary cases. All rows require F15/F16 manual/demo/release evidence in addition to the implementation files shown.

| Requirements | Goals | Use cases | Changes | Files | Tests |
|---|---|---|---|---|---|
| R1–R3 | G-01/G-03 | UC-01 | C-01/C-03/C-05 | F01/F05/F08/F10/F12 | T-01 |
| R4–R5 | G-01/G-03 | UC-02 | C-03/C-05 | F08/F10/F12 | T-02 |
| R6 | G-03 | UC-06 | C-04 | F07/F10/F12 | T-06 |
| R7–R8 | G-01/G-04 | UC-03 | C-03 | F06/F10/F12 | T-03 |
| R9–R28 | G-01/G-02/G-03/G-04 | UC-03/04 | C-01/C-02/C-03 | F01–F04/F06/F10/F12 | T-03/04 |
| R29–R30 | G-01/G-02 | UC-01 | C-03/C-06 | F04/F05/F10/F12 | T-01 |
| R31–R34 | G-01/G-02/G-03 | UC-04/05 | C-01–C-04 | F01–F03/F05/F07/F09/F10/F12 | T-04/05 |
| R35–R41 | G-01/G-03/G-05 | UC-05/07 | C-03/C-05/C-07 | F05/F06/F09/F10/F12 | T-05/07 |
| R42 | G-01/G-03 | UC-02/11 | C-03/C-05 | F08/F10/F12/F13 | T-02/11 |
| R43–R45, R47 | G-01/G-03/G-04/G-05 | UC-02/03 | C-03/C-07 | F06/F08/F10/F12 | T-02/03 |
| R46, R48 | G-03 | UC-07 | C-03/C-05 | F06/F10/F12 | T-07 |
| R49 | G-02 | UC-04 | C-01/C-02 | F01–F03/F10/F12 | T-04 |
| R50–R51 | G-01/G-03 | UC-06 | C-04 | F07/F10/F12 | T-06 |
| R52–R53, R57–R59 | G-02/G-04 | UC-08 | C-02/C-06 | F01/F04/F10/F12 | T-08 |
| R54–R56, R60 | G-01/G-02/G-03/G-04 | UC-09/11 | C-01/C-02/C-05/C-06 | F01–F03/F08/F10–F12 | T-09/11 |
| R61 | G-01/G-04 | UC-10 | C-01/C-03/C-06 | F01/F02/F06/F10/F12/F13 | T-10 |
| R62–R64 | G-05 | UC-11 | C-07 | F11/F12/F14/F15/F16 | T-11/12 |
| R65–R66 | G-03/G-05 | UC-07/11 | C-05/C-07 | F08/F10–F14 | T-07/11/12 |
| R67–R68 | G-05 | UC-12 | C-01–C-07 | F01–F16 | T-12 |
| R69 | G-01 | UC-10 | C-03 | F05/F06/F10/F12 | T-10 |
| R70, R80, R84 | G-01/G-03/G-04 | UC-03 | C-03/C-06/C-07 | F05/F06/F10/F12/F15 | T-03 |
| R71, R74–R75, R78–R79, R82 | G-02/G-03/G-05 | UC-04 | C-01/C-02/C-07 | F01–F04/F06/F07/F10/F12/F15 | T-04 |
| R72 | G-03 | UC-06 | C-04 | F07/F10/F12 | T-06 |
| R73, R85 | G-01/G-03/G-05 | UC-05 | C-03/C-04/C-07 | F05/F07/F10–F12 | T-05 |
| R76 | G-03 | UC-01 | C-01/C-05 | F02/F08/F10/F12 | T-01 |
| R77, R81 | G-03/G-05 | UC-07/11 | C-03/C-05/C-07 | F08/F09/F11/F12 | T-07/11 |
| R83 | G-03 | UC-02/11 | C-05 | F08/F11/F12 | T-02/11 |

Reverse alignment: C-01–C-07 each list their requirements in the methods table; every F row references a C and at least one R; T-01–T-12 derive from UC-01–UC-12. Coverage includes all B01–B67, M01–M11 and D01–D05 without treating source-reported behavior as reproduced evidence. Addendum A's folded findings are covered by the B item they fold into, and its single non-defect is excluded with a stated reason.

## Plan Strategy and Estimated Work

Use one implementation plan with phases initially, so cross-cutting scope/type/policy changes have a single live ledger. Proposed destination: `docs/plans/plan-2026-0001-perch-remediation.md` (not created by this task). Split later only if owners explicitly allocate every requirement and dependency; do not duplicate ownership.

| Phase | Owning scope | Dependencies / entry | Exit gate | Planning size, not a commitment |
|---|---|---|---|---|
| P0: baseline and contracts | R67–R68; baseline discovery for all rows, F13 and syntax/wire decisions | Proposal reviewed, maintainer roles assigned | Repro fixtures, confirmed standard, exact unresolved file inventory and compatibility decisions | Medium |
| P1: immediate security | R1–R8, R43–R45, R69–R70, R76; urgent parts of R3/R28/R38 | Baseline and isolated side-effect tests; no dependency on missing features | Private/catch, arg/gate, overwrite, secret/HTTP and MCP-policy tests pass; independently releasable patch | Large |
| P2: values/calls/check | R9–R28, R31, R49, R71, R74–R75, R78–R79, R82 | Token fidelity experiment and frozen typed/call contracts | String/value/control/scope/default/check parity regressions pass | Extra large |
| P3: manifests/platform/tools | R29–R30, R32–R42, R46–R48, R50–R51, R72–R73, R77, R80, R83 | Canonical calls plus policy foundations | Root/test/bundle/platform/tool conformance across supported OSes | Large |
| P4: missing capabilities | R52–R61, R84 | Typed values and effect metadata; R60 after isolated frames; R61 after argument/schema gates | New ops/control/typed bins work through all surfaces and tools | Extra large |
| P5: docs/distribution and closure | R62–R66, R81, R85, final R68 evidence | Earlier phase tests and migration artifacts | Verified manual/demo/artifacts, complete report coverage, selected version and release evidence | Medium |

Each R has one owning phase above; P1's urgent partial coverage does not mark P2/P3 requirements complete. C-01–C-07 span phases and remain owned by this single plan; UC-01–UC-12 follow their R ownership. Documentation and tests accompany each phase, not just P5. Calendar estimates require reproductions and assigned people; the size labels reflect risk and scope only.

The implementation plan must track owner, phase, R/UC/C/F/T IDs, dependencies, status and evidence per task, using the standard's live-ledger states. An unavailable corpus or platform is BLOCKED/PARTIAL evidence, not a passed gate. All required work remains NOT STARTED in this proposal.

## Open Questions

1. Confirm the intended `documentation.md`; this draft uses the clearly identified shared standard provisionally.
2. Assign maintainers/reviewers and decide the compatibility-release version after identifying installed artifacts.
3. Prove the Capy engine can implement the proposed raw/multiline/escaped token model; pin an engine update if necessary.
4. Confirm enum rendering/accessibility integration after F13 discovery; the inspected HTTP envelope and proposed enum field are specified above.
5. Resolve exact platform implementations and resource limits for safe-open, checked-dial, process cancellation and sandbox-required behavior before approving affected implementation tasks.
6. Determine whether the root MCP executable is tracked/generated or intentionally distributed, and select F14's deletion or reproducible rebuild branch.
7. Freeze the addendum spellings and defaults: `requires host public`, the `regex_find_all` group selector, the `http_status` HEAD→GET fallback default, any new `trim` variants (R78), and the description-warning rule (R85).

These are bounded implementation decisions. They do not remove any reported issue from scope or justify describing the proposal as implemented.

## Approval

No approval recorded. Status remains draft. Approving the remediation direction is distinct from accepting final language syntax, policy compatibility decisions, implementation results or a release. Initial source review is not test evidence or maintainer approval.

## Related Documents

- [Original bug report snapshot — REF-2026-0002](../references/ref-2026-0002-perch-project-bug-report.md)
- [Documentation standard source snapshot — REF-2026-0003](../references/ref-2026-0003-documentation-standard-source.md)
- [Standards baseline — STD-2026-0001](../standards/index.md)
- [Document index](../index/document-index.md)
- [VHCO architecture](../../vhco-architecture.md)
- [Typed bins design](../typed-bins.md)
- [Current capability-gating scope](../capability-gating.md)
- [Current sandbox contract](../sandbox.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-09-26 | Codex | Initial proposal covering all supplied findings, typed semantics and policy design, per-item acceptance, file impact, tests, migration and phased release evidence. |
| 2 | 2026-09-27 | Claude | Cover REF-2026-0002 Addendum A: add R69–R85 for B52–B67 and M11; extend use cases, changes, contracts, file impact, tests, alignment and phases; update coverage counts. |

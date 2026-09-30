---
document_id: REF-2026-0002
title: Perch 27-project bug report snapshot
document_type: reference
status: draft
created_date: 2026-09-26
last_updated: 2026-09-27
document_revision: 2
authors: [Codex, Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch maintainers]
systems: [Perch]
components: [language, runtime, sandbox, documentation]
affected_versions:
  from: "0.1.0"
  to: "0.1.2"
applicable_environments: [development]
audience: [engineers, reviewers]
scope: Preserve the supplied report and its original issue numbering, plus an addendum of findings omitted from its consolidation.
reason: The original scratchpad is temporary; remediation needs a durable source.
related_documents: [PROP-2026-0001]
supersedes: null
superseded_by: null
tags: [bugs, evidence]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-10-26
---

# Perch 27-project bug report snapshot

> **Status:** Draft · **Created:** 2026-09-26 · **Updated:** 2026-09-27 · **Revision:** 2 · **Owner:** Perch maintainers (proposed)
> **Affected versions:** Reported 0.1.0–0.1.2; actual installed build identity is unverified.

## Provenance

Source: `/private/tmp/claude-501/-Users-oliverlaleau-Documents-projects-executor/076f1efc-af26-47a1-b0a2-8bfe13b9dd96/scratchpad/perch_projects/PERCH_BUGS.md`.

SHA-256 of original bytes: `ec173b5090093f1b1a6e99d2da4b7c14dc88e9522069d47770601077a8efee15`.

Copied verbatim below. These are the supplied report's findings, not independently reproduced results. The source date is unknown; snapshot date is 2026-09-26.

## Original report

# perch bugs & gaps found while building 27 real-world projects

Deduplicated from 27 agent reports. `[NN]` = project folder(s) that hit it. Count = how many projects hit it independently.
Tested against the installed `perch` (reports `--version` 0.1.0, `--help` 0.1.2).

## P0 — security / data loss

1. **Archive ops ignore positional args.** `tar_create`, `zip_create`, `tar_extract`, `zip_extract`, `gzip`, `ungzip`: handlers in `infra/ops/archive.go` read `src`/`dst`, parser passes `_0`/`_1` → paths empty, op fails with "open <cwd>: is a directory". The gating table in `infra/ops/requires.go` uses the same keys, so these ops likely **bypass read/write root checks**. [10, 22]
2. **Flags after a positional are silently dropped.** `restart_service web -namespace=kube-system -confirm=web` ran against `default`. `--help` usage line shows exactly this order. [06, 04, 08, 11, 17, 18]
3. **`symlink` silently deletes what's at the link path** (`os.Remove` before `os.Symlink` in `opSymlink`, `infra/ops/files.go`). Undocumented. [23]
4. **perch-mcp can run private commands when a `catch` exists** — `handlePerchRun` skips the private-command refusal on that path. [13]
5. **perch-mcp accepts only `-f`.** No `--no-*`, `--allow-host`, `--env`; contradicts `sandbox.md` (which also mentions `--require-sandbox`). An MCP config can't narrow capabilities. [13]
6. **`--scan` recommends `--no-subprocess` for files that run bare bins** — the flag blocks `exec`, so following the advice breaks the file. [13, 21]
7. **Tokens may leak in HTTP error messages** — errors include the full URL, so `user:token@host` URLs leak (from source reading, not reproduced). [25]
8. **`http_*` ops can't send headers** (no `Authorization`, Content-Type fixed to JSON), don't expose status, and don't fail on 4xx/5xx. Forces `curl`, which bypasses `--allow-host`/SSRF guard and puts tokens in argv. [07, 16, 19, 20, 25, 27]

## P1 — language semantics (silent wrong results)

9. **`x = "literal"` inside `do` runs a program named `literal`** (`bin_not_declared`). Also `x = 5`, `x = ""`, `x = other_var`. Guide Walkthrough 8 uses this form. Workaround: `x = trim "..."`. **(~20 projects)**
10. **Strings aren't raw.** Backslashes are stripped/processed inconsistently (`\s`→`s`, `\d`→`d`, `\[`→`[`; `\n` is a newline in `print`/`write_file` but `n` in captures/`format`). Regexes silently change meaning. **(~15)**
11. **Multi-line `"..."`/`'...'` strings fail** ("unterminated string"); backticks work only at top level / in statement ops, and are mangled in captures (`` `a⏎b` `` → `` `anb` ``) and in `append_file`. SKILL.md contradicts itself on backticks. **(~18)**
12. **`if a < b` / `a != b` compares against the literal text of `b`.** `--check` silent. Non-numeric `<` returns true instead of erroring. **(~10)**
13. **`if not FUNC ARG` (e.g. `if not exists "p"`, `if not has_bin "x"`) is always true** — `not` takes only a binding name. **(7)**
14. **`if FUNC A B` passes only one arg** (`if has_prefix x "#"` → always true; `if regex_match P S` → always true; `if contains A B` → true). [06, 08, 20]
15. **Unknown predicate in `if` is silently false** and not flagged by `--check`. `not_exists` in `docs/language.md` is not a real op. [04, 18]
16. **Template args aren't bindings** — only `"${arg}"` substitution works; bare `if arg` / `if arg == "x"` silently false. **(7)**
17. **Lists become strings.** `split`/`regex_find_all`/`json_get` arrays capture as `[a b]`; `for_each` runs once on that or iterates the literal name; `join` no-ops; empty list is the text `[]`. **(~12)**
18. **Commands can't return values** (`x = cmd` → `""`), yet callee bindings leak back to the caller at runtime, and callee args overwrite caller bindings of the same name. **(~12)**
19. **Arg defaults skipped on zero-arg command-to-command calls** ("unknown placeholder"); previous call's values leak into later calls. Arg `default "${...}"` isn't interpolated. **(8)**
20. **Quoted `";"` / `"&&"` parsed as chain operators.** [05, 25, 26]
21. **`rescue err` (documented in errors.md/guide.md) doesn't parse**; bare `rescue` works. **(6)**
22. **`run`, `test`, `import` as command names collide** with built-ins; `--check` doesn't flag `run`. [04, 20, 25]
23. **`on_error any` fires once per enclosing block** (`if`, `run`, `match`, `for_each`, `try`) → duplicate logs / repeated restores; handler state doesn't persist. [03, 06, 12]
24. **`repeat STR N` always returns `""`** (reads `args["count"]`, gets `_1`, `infra/ops/strings.go`). [03, 06, 14, 17, 25]
25. **`format`**: one value only (`%!s(MISSING)`), no-verb appends `%!(EXTRA <nil>)`, ints arrive as strings (`%!d(string=5)`). **(6)**
26. **`replace "OLD,NEW"`** can't replace a comma; empty OLD inserts between every char. [03, 04, 16]
27. **`json_get`**: no array indexing, leading-dot form returns empty, nested objects print as Go maps, invalid JSON returns `""` silently; `json_count` documented but missing. **(8)**
28. **`exec` captures stdout only**; stderr can't be captured or silenced without `shell`; perch passes its stdin to children (s_client hangs). [08, 14]
29. **`write_file`/`append_line` don't create parent dirs.** [03, 10, 15, 23]
30. **`glob` returns absolute paths for relative patterns** (breaks `path_rel`); no `**`. [12, 21]

## P1 — `--check` vs runtime disagreements

31. Bindings set in a called command, `match` arm, `if` block, `try`, or `rescue` → "unknown placeholder" though they work at runtime. **(~15)**
32. `requires` roots given as bindings (`write OUT_DIR`) aren't resolved; `host API_HOST` passes check but fails at runtime with `host_not_declared`. **(8)**
33. No path normalization: `read "."` doesn't cover `go.mod`; `"sbom-out/x"` ≠ `"./sbom-out"`; write-implies-read not honored statically. **(6)**
34. Literal out-of-root writes are rejected even in negative tests that expect the error. [21]

## P2 — manifest / sandbox / testing

35. **File without `requires` has no access**, contradicting SKILL.md / op-reference / language.md ("ambient access"). **(8)**
36. `read "${bundle_dir}"` silently dropped from roots (`expandRoots`, `infra/ops/requires.go`). [24, 26]
37. `bundle include "./dir"` is flattened in built binaries; layout differs from running from source; `bundle_extract` fails from source. [24, 26]
38. `read_not_declared` message prints the attempted path where it should list declared roots. [21, 24, 26, 27]
39. `${temp_dir}` (`/var/...`) doesn't cover the macOS test cwd (`/private/var/...`) — no symlink resolution. [23]
40. Relative `read`/`write` roots resolve against cwd (the temp dir under `perch test`), not the script dir. [06, 27]
41. Tests can't declare their own `requires`; fixture writes force widening the production manifest. `test_allow_write` is reserved. [05, 08, 12]
42. `test` commands leak into `list_commands`, `/api/program`, and `perch_list` (MCP) — where `perch_run` then refuses them. [05, 06, 13, 17, 23]
43. HTTP to localhost needs `--allow-private-ips` on the CLI; `requires host "localhost"` doesn't allow it. [17, 22]
44. `http_status` returns 0 for undeclared host / TLS / DNS errors instead of raising. [08, 09, 24]
45. Hooks: `${hook.target}` for exec is only the program name (can't distinguish read vs delete); for HTTP it's the full URL (secrets). [06, 11, 16]
46. `os_version` needs undeclared `sw_vers`/`uname` bins. [01, 26]
47. Fixed 30s HTTP timeout, not configurable. [17]
48. `port_free` returns true while a port is listening on 127.0.0.1 (macOS). [17]
49. A `NL = hex_decode "0a"` top-level binding fails ("exec outside a do block"). [25]
50. `perch simulate`: `fail` in unknown branch counted as WILL_FAIL (unusable as CI gate); bare-bin calls labeled by first arg; fixture oracles don't apply to bare bins; `--sim-no-subprocess` ignored. [03, 11, 21]
51. `--dry-run` hides `match` case bodies, prints `exists` without arg, sorts exec args lexically. [05, 20]

## Missing features (requested repeatedly)

- **Arithmetic** (counters, totals, tok/s, averages) — every project worked around it. **(~15)**
- **Date math** ("N days ago", days until expiry). [08, 11, 19]
- **HTTP headers / status / timeout** (see #8, #47).
- **`else`**, **`continue`**, enum arg type (only in typed-bins design doc), random bytes, regex escape, JSON array iteration/key listing, `parallel` over a list.
- **Typed bins** (`docs/typed-bins.md`) are design-only.

## Docs drift

- `op-reference.md` missing: `has_bin`, `bin_version`, `port_free`, `http_status`, `wait_for_url`, `detect_pkg_mgr`, `pkg_install`, `version_*`, `ensure_line_in_file`, `path_join`, `count_lines`, `bundle_dir`, `with_cwd`, `with_env`, `match`, `try`, `grep`/`tail`/`cut`.
- `regex_replace` arg count wrong; guide Walkthrough 8 has `regex_match` args reversed.
- `match` syntax undocumented in SKILL.md; errors.md uses `case`.
- Stale `perch-mcp` binary at repo root fails to parse an apostrophe in a comment (source build is fine). [13]
- `perch --version` 0.1.0 vs `--help` 0.1.2.

## Addendum A — findings omitted from the original consolidation

Added in revision 2 (2026-09-27). The consolidated report above was compiled from 27 per-project agent reports. When it was reconciled against those reports, the findings below were missing, or appeared only inside a broader item. They come from the same agent reports and, like the rest of this document, are **reported, not independently reproduced**. The original report above is unchanged, and its SHA-256 still describes only that section.

IDs continue the original numbering. `[NN]` = reporting project folder(s).

### New numbered findings

| ID | Finding | Projects |
|---|---|---|
| B52 | **Declared bins are matched by basename.** `bin "python3"` admits any executable named `python3` at any path (e.g. a user-supplied `-interpreter=/tmp/x/python3`). | [26] |
| B53 | **HTTP redirect targets are not checked against `requires host`.** `rdap.org` → registry redirect succeeded without the registry host being declared; only the initial URL is gated. | [08] |
| B54 | **`regex_find_all` returns whole matches only**, with no way to get capture groups. | [17] |
| B55 | **`--scan` misreports inputs and roots.** It lists local bindings (`NL`, `REPO`, `LOG_DIR`) under "ENV VARS REFERENCED" and suggests them for `--env`; its "writes" line shows unresolved placeholders (`${target}`) and test-only paths instead of the declared roots. | [13, 21] |
| B56 | **A non-optional `requires env` blocks every command** in the file when that variable is unset, including commands that never read it. | [07] |
| B57 | **Command args silently shadow auto-bound names.** An arg named `config_dir` overrides `${config_dir}` with no `--check` warning. | [10] |
| B58 | **`err.kind` is `unclassified`** for archive-op failures and for effects vetoed by a `before` hook. | [07, 10] |
| B59 | **An arg declared with `index N` cannot also be passed as `-name=value`** ("flag provided but not defined"). | [06] |
| B60 | **Built binaries may leave extracted bundle directories** (`perch-bundle-*`) in the system temp dir. Observed leftover; ownership unconfirmed. | [24] |
| B61 | **`trim` also strips trailing newlines**, but it is the only practical way to assign a literal (B09), so newline-terminated values cannot be copied faithfully. | [14] |
| B62 | **`${cwd}` is not auto-bound**; `here = cwd` is required. | [10] |
| B63 | **`http_status` sends `HEAD`**, which some servers reject or answer differently from `GET`; the method is undocumented. | [09] |
| B64 | **`--help` shows a quoted top-level string such as `"3.12"` as `(float)`.** The runtime value is correct. | [26] |
| B65 | **Capture echo behavior is inconsistent and contradicts the docs.** `x = git rev-parse HEAD` also prints to the terminal (03), while the docs' claim that `x = BIN …` prints its output did not hold elsewhere (06). | [03, 06] |
| B66 | **`list_commands` returns commands in random order.** | [05] |
| B67 | **`--check` warns "no description"** for commands whose args have descriptions but the command has none; the message doesn't say which level is missing. | [14] |

### New missing feature

| ID | Feature | Projects |
|---|---|---|
| M11 | **A `requires` form for "any public host".** Hosts read from a file at runtime can only be admitted with broad TLD wildcards (`*.com`). A public-only grant would still be subject to the SSRF/private-address guard. | [08] |

### Findings folded into existing items

These were reported but are already part of an original item. They are listed so that every agent finding can be traced.

| Reported detail | Folded into | Projects |
|---|---|---|
| `for_each` splits strings only on newlines; no inline list literal | B17 | [09, 13] |
| `json_stringify` does not turn a captured Go map back into JSON; drops `\ ` | B27, B10 | [07, 14] |
| Backtick strings drop embedded `"` and keep outer backticks as op arguments | B10, B11 | [15, 24] |
| `hook.target` for a `for_each` error is the loop's first item | B45 | [12] |
| Handler-local state set inside `on_error` is lost | B23 | [12] |
| `simulate` marks `if apply` uncertain although the arg defaults to false; cannot reason about interpolated paths | B50 | [21] |
| Test commands shown in `perch_list` cannot be run via `perch_run` | B42 | [13] |
| `http_status` returns 0 on TLS/DNS failure | B44 | [08, 09] |
| `--check` flags `get_env` in files without `requires` | B35 | [07] |
| Calling a command inside `try` with zero args skips defaults | B19 | [11] |
| Bool flags passed command-to-command as `"-flag=true"` only work before positionals | B02 | [11] |

### Not a defect

| Reported detail | Reason | Projects |
|---|---|---|
| `perch --build` only targets the host platform | Documented as roadmap; not a behavior contradiction | [14] |

## Related Documents

- [Remediation proposal](../proposals/prop-2026-0001-perch-correctness-security-and-completeness.md)

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-09-26 | Codex | Preserve supplied report, provenance and original IDs. |
| 2 | 2026-09-27 | Claude | Add Addendum A: B52–B67 and M11, which were omitted from the consolidation, plus traceability for folded and non-defect findings. Original report unchanged. |

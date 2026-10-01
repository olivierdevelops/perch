---
document_id: MAN-2026-0004
title: "Confining spawned binaries to declared scopes"
document_type: manual
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [ops, sandbox, process spawning, CLI]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [macOS, Linux, Windows]
audience: [perch authors, operators, security reviewers]
scope: "Task chapter for requirement R03 of PROP-2026-0002 - kernel confinement of spawned binaries to the declared `read` and `write` roots (and best-effort `host`), the per-OS support table, fail-closed behaviour and its opt-out."
reason: "`requires read/write/host` used to bound only perch's own ops; a spawned binary could write anywhere the user could."
related_documents: [MAN-2026-0001, PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [manual, sandbox, confinement, requires, security, macos, linux]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Confining spawned binaries to declared scopes

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** process spawning in the ops crate, the confinement module, CLI flag
> **Verified:** macOS sections were run on macOS (Darwin 25.4, arm64) with the `rust-port` build that reports 0.1.1. **The Linux backend has not been run on a real Linux kernel**; its rows are derived from the source and are marked as such.

## Summary

When a file's `requires` block declares `read`, `write` or `host`, perch now confines every binary it spawns to those scopes with an operating-system mechanism, instead of only checking perch's own ops:

- macOS: a generated `sandbox-exec` profile.
- Linux: a Landlock ruleset applied just before `exec` (kernel 5.13 or later).
- Anywhere else, or when the mechanism is unavailable: perch **refuses to spawn** (error `confinement_unavailable`), unless the operator passes `--allow-advisory-scopes`, in which case it runs unconfined and prints that the scopes are advisory.

A file that declares no `read`, `write` or `host` is not confined (nothing to enforce).

## When to use it

Declare the directories a server or tool may touch, then let perch hold it to them. The motivating case is a native server (a database) that should write only to its data directory:

```perch
requires
    bin "mysqld"
    write "./data"
end
```

Prerequisites: a `requires` block with at least one `read`, `write` or `host` line; macOS (any recent version with `/usr/bin/sandbox-exec`) or Linux 5.13+ with Landlock enabled.

## Journey overview

```text
perch <cmd>  -> load file -> run ops ... -> spawn a binary (exec / bare bin / shell / pipe stage ...)
                                              |
                       no read/write/host declared? ---------> spawn unconfined (as before)
                                              |
                          platform can confine? -- yes -----> spawn under profile / Landlock
                                              |
                                              no
                               --allow-advisory-scopes? -- no -> error confinement_unavailable (nothing runs)
                                              |
                                             yes -> one stderr banner, spawn unconfined
```

## What is enforced

| Scope | Meaning for a spawned binary |
|---|---|
| `write "PATH"` | May create, modify and delete files under PATH. Implies read of the same tree. |
| `read "PATH"` | May read under PATH. |
| everything else on disk | Reads allowed only from the system locations needed to start an ordinary program (see [macOS default read allowances](#macos-default-read-allowances)); writes refused. |
| `host "NAME"` | Best effort. On macOS declaring any host switches network on for the whole process (the profile cannot filter by name); declaring none leaves network off. Not enforced on Linux. |

Relative roots resolve against the working directory in effect when the binary is spawned, the same rule perch's own file-op gate uses ([requires.md](../requires.md)).

Confinement applies to: bare declared-bin calls, `exec`, `pipe ... end` stages, `shell`, `shell_in`, `shell_output`, `shell_detached`, `try_shell`, and `bin_version`.

### Per-OS support

| OS | Mechanism | `read` / `write` | `host` | Status of this chapter's claims |
|---|---|---|---|---|
| macOS | `/usr/bin/sandbox-exec -p <profile>`, profile generated per spawn, `(deny default)` | Enforced | Best effort: network allowed for the whole process if any host is declared, otherwise denied | Verified on macOS arm64 (below). |
| Linux | Landlock, ABI V1 (kernel 5.13+), ruleset applied in a `pre_exec` hook | Enforced when the kernel supports Landlock | Not enforced (V1 has no network rules); network is unrestricted | **Not run on a real Linux kernel for this chapter.** Behaviour taken from the source (`infra/ops/src/confine/linux.rs`). The integration tests in `infra/ops/tests/confine_wiring.rs` are written to run on Linux CI; no result is recorded here. |
| Windows | none | Refuses | Refuses | Refusal path verified by simulation (below); Windows itself not run. |
| Other Unix (BSD, ...) | none | Refuses | Refuses | Same code path as Windows. |

A Linux kernel without Landlock, or a macOS process that is already inside a sandbox where the probe fails, counts as "cannot confine" and takes the refuse-or-advisory path.

## CLI procedure

### The repro: a declared write works, an escape is refused

`t.perch` (the request that motivated this feature; the escape target is a relative path outside the declared root):

```perch
requires
    bin "sh"
    write "./allowed"
end
command go
    do
        mkdir "./allowed"
        shell "sh -c 'echo in > allowed/ok.txt; echo out > ./escape-test.txt'"
    end
end
```

```text
$ perch -f t.perch go </dev/null ; echo rc=$?
sh: ./escape-test.txt: Operation not permitted
shell_exit_nonzero: exit status 1 (sh -c 'echo in > allowed/ok.txt; echo out > ./escape-test.txt')
rc=1
$ cat allowed/ok.txt
in
$ ls escape-test.txt
ls: escape-test.txt: No such file or directory
```

The declared write (`allowed/ok.txt`) succeeded; the write outside it was refused by the kernel. Perch reports the child's failure as an ordinary `shell_exit_nonzero`; the "Operation not permitted" line is the child's own message.

(`</dev/null` is only to keep an interactive harness's stdin out of `bash`; it is not needed in normal use.)

### Reads, network and host-only files on macOS

```perch
requires
    bin "sh"
    read "./data"
end
command ok
    do
        sh -c 'cat ./data/in.txt'
    end
end
command go
    do
        sh -c 'cat ./h2.txt'
    end
end
```

```text
$ perch -f r.perch ok
indata
$ perch -f r.perch go
cat: ./h2.txt: Operation not permitted
shell_exit_nonzero: exit status 1 (sh -c cat ./h2.txt)
```

Network, with a local listener on loopback (no external traffic):

```text
$ perch -f n.perch net     # requires: bin "sh", write "./allowed"           (no host)
blocked
$ perch -f n2.perch net    # same plus: host "127.0.0.1"
connected
$ perch -f n3.perch net    # requires: bin "sh" only (no scopes)
connected
```

A file that declares only `host` (no `read`, no `write`) is still confined: the child can reach the network but can neither write nor read the project directory:

```text
$ perch -f h.perch go      # requires: bin "sh", host "example.com"; body: sh -c 'echo x > ./h.txt'
/bin/sh: ./h.txt: Operation not permitted
shell_exit_nonzero: exit status 1 (sh -c echo x > ./h.txt)
```

Declare the directories the tool needs, even if you only care about the network scope.

### macOS default read allowances

To let ordinary programs start, the profile always allows reading these (never writing): `/usr/lib`, `/usr/bin`, `/usr/sbin`, `/usr/libexec`, `/usr/share`, `/bin`, `/sbin`, `/System`, `/opt/homebrew`, `/usr/local`, `/Library/Apple`, `/Library/Preferences`, `/Library/Developer/CommandLineTools`, `/private/etc`, `/etc`, timezone and dyld cache directories, and a few device nodes (`/dev/null`, `/dev/urandom`, ...). In addition:

- The directory the program lives in (the literal path and its symlink target) is readable and executable, so a bin installed anywhere, or a path-form bin with sibling files, can load itself. Verified: a `bin "./tools/tool" as tool` script reading `./tools/sidecar.txt` printed `sidecar-data`.
- The child's working directory is readable as a directory literal only, so `getcwd(3)` works (names are listable, file contents are not). Verified: a confined `sh -c 'pwd; ls .'` lists the directory, while `cat` of a file in it is refused.
- `/opt/homebrew` and `/usr/local` are readable by every confined child. Do not keep secrets there.

### The refusal path and the opt-out

On a platform that cannot confine, a file with scopes does not run its binaries:

```text
$ perch -f t.perch go          # Windows, or any host where confinement is unavailable
confinement_unavailable: this file declares read/write/host scopes but they cannot be enforced on spawned binaries here (no confinement mechanism on this platform); refusing to spawn — pass --allow-advisory-scopes to run unconfined with the scopes advisory (no confinement mechanism on this platform)
```

With the opt-out the binaries run unconfined and stderr gets one banner per run:

```text
$ perch --allow-advisory-scopes -f t.perch go
🔒 security: --allow-advisory-scopes
perch: declared read/write scopes are advisory on this platform (no confinement mechanism on this platform); spawned binaries are NOT confined
ran
```

> Verification note. The two blocks above were produced by running the real interpreter and op handlers with the platform probe forced to "unsupported" (a scratch program driving `Interpreter.confine_unsupported`), because the CLI has no switch to fake an unsupported platform and this chapter was verified on macOS, which is supported. The `🔒 security:` line is the CLI's standard flag banner, observed separately on macOS with `--allow-advisory-scopes`. The `ran` line is the child's output.

`--allow-advisory-scopes` is global (like `--no-shell`) and has no effect where confinement works. It does not weaken perch's own gates (`write_not_declared` and friends still apply to perch's own ops).

## What is NOT confined

- Helper operations that spawn their own processes: `pkg_install`, `pkg_uninstall`, `pkg_installed` (package manager), `process_running` (`pgrep` / `tasklist`), `kill_by_name` (`pkill` / `taskkill`), the OS-version helper (`sw_vers` / `uname`) and the Windows admin check. They run as before under perch's own capability flags (`--no-subprocess`).
- `host` scopes are not name-filtered on any platform: advisory on macOS (network on or off as a whole), not enforced on Linux.
- A binary that a confined binary launches inherits the same confinement (verified on macOS: in the repro, `bash` launches `sh`, and the `sh` write was refused), but anything the binary asks an already-running daemon to do (for example `docker` talking to the Docker daemon) happens outside the sandbox.
- Perch itself is not sandboxed; only its children are.
- `wasm_run` modules are confined by construction, not by this mechanism ([MAN-2026-0007](man-2026-0007-wasm-cache-and-gating.md)).

## Expected result and side effects

- Confinement is per spawn and invisible when the child stays inside its scopes.
- On macOS the child is started through `sandbox-exec`, so its `argv[0]` is the resolved program path rather than the name as written in the file (taken from the source; not separately demonstrated).
- The probe (one `sandbox-exec` run on macOS) happens at most once per perch process.

## Errors and recovery

| Symptom | Cause | Recovery |
|---|---|---|
| `confinement_unavailable: ... refusing to spawn ...` | Platform cannot confine and no opt-out. | Run on macOS or Linux 5.13+, remove the scopes you do not need, or pass `--allow-advisory-scopes` knowingly. |
| `Operation not permitted` from the child, exit status non-zero | The child tried to touch a path outside its scopes. | Add the path to `read` or `write` if it is legitimate. |
| A tool fails to start, or complains it cannot create a cache file | The tool needs a location outside the default allowances. Example (verified): macOS `/usr/bin/python3` is a shim that reads `/Applications/Xcode.app` and writes under `$TMPDIR`. With `write "./allowed"` only it fails with `file system sandbox blocked open()`; adding `read "/Applications/Xcode.app"` lets it run, and the remaining `couldn't create cache file ... Operation not permitted` warnings come from the refused `$TMPDIR` writes. | Declare the extra `read`/`write` roots the tool needs, or use a tool that does not depend on them. |
| `sandbox-exec: sandbox_apply: Operation not permitted`, exit 71 | Perch itself runs inside another macOS sandbox that forbids nesting. See limitations. | Run perch outside the outer sandbox. |

## Limitations and known issues

- **Linux is untested on a real kernel.** Treat the Linux rows as unverified until CI or a manual run records a result. From the source: a declared `write` root must exist when the binary is spawned (a missing one fails the spawn); a missing `read` root is skipped; `/usr`, `/lib*`, `/bin`, `/sbin`, `/etc`, `/proc/self` and four device files are always readable.
- **Probe in a nested sandbox.** The macOS probe asks `sandbox-exec` to apply a permissive profile; inside an outer sandbox that is allowed, but the strict profile the real spawn needs is not. Perch then reports no problem at probe time and the spawn fails with `sandbox-exec: sandbox_apply: Operation not permitted` and exit status 71, surfaced as `shell_exit_nonzero` rather than `confinement_unavailable`. It still fails closed (the child does not run), but with the wrong error kind, and `--allow-advisory-scopes` does not help. Verified by running perch under `sandbox-exec -p '(version 1)(allow default)'`:

  ```text
  $ sandbox-exec -p '(version 1)(allow default)' perch -f t.perch go
  sandbox-exec: sandbox_apply: Operation not permitted
  shell_exit_nonzero: exit status 71 (sh -c 'echo in > allowed/ok.txt; echo out > ./escape-test.txt')
  ```

- `sandbox-exec` is deprecated by Apple but still the practical mechanism ([typed-bins.md](../typed-bins.md) records the landscape). If Apple removes it, perch refuses rather than degrading.
- Path matching is by path prefix; on macOS the profile includes both the literal path and its resolved symlink target. It is not a chroot.
- No supervisor, restart policy or health check is part of this feature.

## Version applicability

| Feature | Introduced | Notes |
|---|---|---|
| `requires read/write/host` bound perch's own ops | 0.1.0 | |
| Kernel confinement of spawned binaries (macOS, Linux) | 0.2.0 | Linux untested on a real kernel at the time of writing. |
| Refuse by default where unavailable; `--allow-advisory-scopes` | 0.2.0 | The only change that can make a previously running file refuse to run. |
| `confinement_unavailable` error kind | 0.2.0 | Listed in [errors.md](../errors.md). |

## Related documents

[MAN-2026-0001](man-2026-0001-perch-manual-index.md) · [requires.md](../requires.md) · [sandboxed-by-design.md](../sandboxed-by-design.md) · [typed-bins.md](../typed-bins.md) · [errors.md](../errors.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · Verified demo: DEMO-2026-0001 (planned at `docs/demos/demo-2026-0001-perch-0-2-0-verification.md`).

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial chapter for R03. |

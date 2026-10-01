---
document_id: MAN-2026-0003
title: "Cleanup with finally - command-level finally and non-masking errors"
document_type: manual
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [language, interpreter, ops, validate, LSP]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [perch authors, operators]
scope: "Task chapter for requirement R02 of PROP-2026-0002 - the command-level `do ... finally ... end` section, the error-merging rule, and `finally` on `--max-runtime` and `timeout`."
reason: "A local service must be stopped after a failed run, and a failing cleanup must not hide the real cause."
related_documents: [MAN-2026-0001, PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [manual, finally, errors, cleanup, timeout]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Cleanup with finally - command-level finally and non-masking errors

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** language grammar, interpreter, error handling ops, validator, LSP
> **Verified:** commands and outputs below were run on the `rust-port` build that reports 0.1.1 (the 0.2.0 bump happens at release).

## Summary

Perch has had `try ... rescue ... finally ... end` since 0.1.0. 0.2.0 adds:

1. A command-level form: `do ... finally ... end`, sugar for wrapping the whole body in `try ... finally ... end`.
2. A non-masking rule: when the body fails and the cleanup also fails, the body error is reported first, with the cleanup error appended.
3. `finally` runs when `--max-runtime` or a `timeout` block fires, under a 5 second grace period.

`try ... rescue ... finally` is unchanged. See [errors.md](../errors.md) for the full error model.

## When to use it

Starting something in the body (a container, a local database, a temp directory) and stopping it afterwards no matter what:

```perch
# Illustration only (needs docker; not executed for this manual).
command run_with_db
    description "start a db, do the work, always stop it"
    do
        docker run -d --name db -p 5432:5432 postgres:16
        run_the_work
    finally
        docker stop db
    end
end
```

Without `finally`, a failing `run_the_work` skips `docker stop db`. With `try ... rescue` the cleanup runs but the failure is swallowed and the exit code is 0. `finally` runs the cleanup and still fails the command.

## Journey overview

```text
        +--------+  ok    +---------+                 +----------------+
 start->|  body  |------->| finally |--- ok --------->| exit 0         |
        +--------+        +---------+                 +----------------+
            | error            |
            v                  +-- fails -> error of finally   (only if the body succeeded)
        +---------+
        | finally |--- ok ----> original error re-raised (exit 1)
        +---------+
            +-- fails ------> original error + "; additionally, finally failed: ..." (exit 1)
```

## Syntax

```perch
command NAME
    do
        BODY OPS
    finally
        CLEANUP OPS
    end
end
```

- `finally` is optional; a `do` without it behaves exactly as before. An empty `finally` section is allowed and does nothing.
- It works in `command`, `catch` and `template` bodies.
- Variables captured in the body are visible in the cleanup (see the `bind` example below).
- `try ... rescue ... finally ... end` blocks can nest inside either part.
- There is no command-level `rescue`; use `try ... rescue` inside the body for that.

## CLI procedure

All files below start with `requires` / `bin "sh"` only where they spawn a binary; these do not, so no manifest is needed.

### Success, failure, and both failing

```perch
command ok
    do
        print "body"
    finally
        print "cleanup"
    end
end

command bad
    do
        print "body"
        fail "body boom"
    finally
        print "cleanup"
    end
end

command both
    do
        fail "body boom"
    finally
        fail "cleanup boom"
    end
end

command fonly
    do
        print "x"
    finally
        fail "cleanup boom"
    end
end
```

```text
$ perch -f f1.perch ok ; echo rc=$?
body
cleanup
rc=0
$ perch -f f1.perch bad ; echo rc=$?
body
cleanup
user_fail: body boom
rc=1
$ perch -f f1.perch both ; echo rc=$?
user_fail: body boom; additionally, finally failed: user_fail: cleanup boom
rc=1
$ perch -f f1.perch fonly ; echo rc=$?
x
user_fail: cleanup boom
rc=1
```

Perch exits 1 for any failure, so "the original exit code" means the original error and kind, not a child process's status.

### The merge rule

| Body | Cleanup | Result |
|---|---|---|
| ok | ok | success |
| ok | fails | the cleanup error |
| fails | ok | the body error, re-raised |
| fails | fails | the body error, same kind, message extended with `; additionally, finally failed: <cleanup error>` |

The kind is the body's, so an enclosing `rescue` sees the original `${err.kind}`:

```perch
command both
    do
        fail "body boom"
    finally
        fail "cleanup boom"
    end
end

command bothrescued
    do
        try
            both
        rescue
            print "rescued kind=${err.kind} msg=${err.message}"
        end
    end
end
```

```text
$ perch -f f5.perch bothrescued
rescued kind=user_fail msg=body boom; additionally, finally failed: user_fail: cleanup boom
```

`${err.message}` includes the appended note; match on `${err.kind}`, not on the message text.

### Bindings, nesting and the caller

```perch
command bind
    do
        tmp = upper "workdir"
        print "using ${tmp}"
        fail "boom"
    finally
        print "removing ${tmp}"
    end
end

command nested
    do
        try
            fail "inner"
        finally
            print "inner cleanup"
        end
    finally
        print "outer cleanup"
    end
end
```

```text
$ perch -f f5.perch bind
using WORKDIR
removing WORKDIR
user_fail: boom
$ perch -f f5.perch nested
inner cleanup
outer cleanup
user_fail: inner
```

A command that calls `bind` inside `try ... rescue` sees `user_fail: boom` after the cleanup output.

### `finally` on `--max-runtime` and `timeout`

Perch checks the wall-clock deadline before starting each op. It cannot interrupt an op that is already running, so a deadline fires at the next op boundary. When it fires inside a body that has a `finally`:

- the `rescue` arm is skipped (the deadline has already expired, so a handler could not run anything);
- the `finally` ops run under a fresh 5 second grace deadline;
- the original timeout error is kept and reported afterwards.

```perch
command mid
    description "timeout detected between body ops"
    do
        print "start"
        sleep 2
        print "after-sleep"
    finally
        print "cleanup ran"
    end
end
```

```text
$ perch -f to.perch --max-runtime 1 mid
🔒 security: --max-runtime 1s
start
cleanup ran
↪ stopped: --max-runtime exceeded
interpreter: --max-runtime exceeded
```

`sleep 2` is not interrupted: it ends at 2 s, the next body op (`print "after-sleep"`) is refused because the 1 s budget is gone, and the cleanup then runs.

`↪` lines go to stderr; the last line (`interpreter: --max-runtime exceeded`) goes to stdout. With `2>/dev/null` only `start` and the final line remain.

If the cleanup itself overruns the grace period or fails, perch keeps the timeout error and adds a `↪` note on stderr:

```text
$ perch -f to.perch --max-runtime 1 slowclean      # cleanup: print, sleep 6, print
🔒 security: --max-runtime 1s
start
cleanup begins
↪ finally did not finish within its 5s grace period
↪ stopped: --max-runtime exceeded
interpreter: --max-runtime exceeded
```

```text
$ perch -f f2.perch --max-runtime 1 slowbad        # cleanup: fail "cleanup boom"
🔒 security: --max-runtime 1s
start
↪ finally failed: user_fail: cleanup boom
↪ stopped: --max-runtime exceeded
interpreter: --max-runtime exceeded
```

`try ... rescue ... finally` behaves the same way: with a 1 s limit and a `sleep 2` in the `try` body the output is `inner cleanup`, then the timeout error; the `rescue` arm never runs. A `try ... finally` placed inside a `timeout "1s" ... end` block also runs its cleanup when the block's deadline fires.

## Expected result and side effects

- Cleanup ops run exactly once per execution of the guarded body.
- Nothing about success changes: a command without `finally` lowers exactly as in 0.1.x.
- `perch simulate` models both forms (`✗ try` with "finally runs even though the body can fail ..." and the cleanup ops listed); see [simulate.md](../simulate.md).
- The LSP hover for `do` and `finally`, and completion, mention the section (see [lsp.md](../lsp.md)).

## Errors and recovery

| Symptom | Cause | Recovery |
|---|---|---|
| `bin_not_declared: ... \`finally\` is not a known op ...` at load | `finally` in a place the grammar does not take it (for example inside `with_env ... end`). Only `do` bodies and `try` blocks take a `finally` section. | Wrap the region in `try ... finally ... end`, or move `finally` to the command's `do`. |
| `line N: op 'exec' outside a do block` at load | A second `finally`, a `rescue` directly inside `do` (there is no command-level `rescue`), or a `finally` after the command's `do ... end` has closed. | Keep one `finally` before the `end` of `do`; use `try ... rescue` for handlers. |
| Merged message `...; additionally, finally failed: ...` | Both parts failed. | Fix the body error first; it is the real cause. |

## Limitations and known issues

- **The deadline can expire inside the last body op.** The deadline is only checked before an op starts. If `--max-runtime` expires while the body's final op is still running, the body counts as successful, no grace deadline is applied, and the first cleanup op is refused with the same timeout error. Verified:

  ```text
  $ perch -f to.perch --max-runtime 1 last      # body: print "start"; sleep 2   finally: print "cleanup ran"
  🔒 security: --max-runtime 1s
  start
  ↪ stopped: --max-runtime exceeded
  interpreter: --max-runtime exceeded
  ```

  `cleanup ran` is not printed. Workaround: end the body with a cheap op (for example `print`) so the deadline is detected inside the guarded body. This is recorded for the 0.2.0 validation report as a defect against D1.
- Misplaced `finally` produces generic load errors (see the table above), not the clearer "`finally` divider outside a `try` block" message the validator can produce for hand-built programs.
- A `rescue` handler binds the error as `err` only; `rescue err` (a name after `rescue`) does not parse in this build, which is why [errors.md](../errors.md) examples use a bare `rescue`.
- Cleanup ops are not interrupted either: a long `sleep` or process in `finally` runs to completion and the grace deadline is checked at the next op.

## Version applicability

| Feature | Introduced | Notes |
|---|---|---|
| `try ... rescue ... finally ... end` | 0.1.0 | Unchanged. |
| `do ... finally ... end` | 0.2.0 | |
| Merge rule (body error first, cleanup appended) | 0.2.0 | Before: the cleanup error replaced the body error. |
| `finally` on `--max-runtime` / `timeout` with 5 s grace, `↪` notes | 0.2.0 | |

## Related documents

[MAN-2026-0001](man-2026-0001-perch-manual-index.md) · [errors.md](../errors.md) · [language.md](../language.md) · [execution-contexts.md](../execution-contexts.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · Verified demo: DEMO-2026-0001 (planned at `docs/demos/demo-2026-0001-perch-0-2-0-verification.md`).

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial chapter for R02. |

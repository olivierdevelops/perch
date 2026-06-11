# Hooks — intercept built-in ops with `before` / `after` / `on_error`

A `hooks … end` block declares **author-controlled interceptors** that fire a
handler command around every built-in op matching a target. Use them to **audit**
(log every subprocess), **gate** (refuse a write outside a directory), or
**react** (notify on any failure) — all declared in the file and visible in the
trace.

```perch
name "deploy"

hooks
    before write   guard_writes      # runs ahead of any filesystem-write op
    before exec     audit_subprocess  # ahead of any subprocess spawn
    after  net      log_http          # after any network op returns
    on_error any     notify_failure    # whenever any op errors
end

requires
    bin   "kubectl"
    write "./build"
end

command guard_writes
    do
        if not path_contains ${hook.target} "./build"
            fail "blocked write outside ./build: ${hook.target}"   # ← veto
        end
    end
end

command audit_subprocess
    do
        append_line "./build/audit.log" "exec: ${hook.target}"
    end
end
```

## Syntax

Inside `hooks … end`, each line is `TIMING TARGET HANDLER` — three bare words,
the same shape as the rest of perch:

| Part | Values |
|---|---|
| **TIMING** | `before` · `after` · `on_error` |
| **TARGET** | a capability **category** (`read` · `write` · `net` · `exec` · `env`), a specific **op kind** (`write_file`, `http_get`), or `any` |
| **HANDLER** | the name of a command to run when the hook fires |

`HANDLER` is just a command — no special form. It runs with the current bindings
plus injected context.

## Timing

- **`before`** runs *ahead of* the op. **If the handler errors (`fail …`), the op
  is vetoed** — it never executes and the error propagates. This is the gate.
- **`after`** runs once the op returns (whether it succeeded or failed). An
  `after` handler error surfaces only if the op itself succeeded.
- **`on_error`** runs *only* when the op errored — for cleanup / alerting. Its own
  error never masks the op's.

## Context — what the handler sees

The dispatcher seeds these bindings before running the handler:

| Binding | Value |
|---|---|
| `${hook.op}` | the op kind that fired the hook (`write_file`, `exec`, …) |
| `${hook.target}` | the resource the op acts on — path / url / bin / first positional arg |
| `${hook.timing}` | `before` / `after` / `on_error` |
| `${hook.error}` | the op's error message (on `after`/`on_error`; empty otherwise) |

## How it works — and what it is (and isn't)

perch funnels **every** op through one dispatch point, so a hook is just a check
wrapped around that call — no new execution model. Two guarantees:

- **Re-entrancy guard.** A handler's own ops do **not** re-fire hooks, so a
  `before write` hook that itself writes runs once, not forever.
- **Fast path.** With no `hooks` block, dispatch is unchanged (zero overhead).

!!! warning "Hooks are policy + observability — not a sandbox"
    A hook is in-process perch logic. It **genuinely enforces perch's own ops**: a
    `before write_file` veto really stops the write. But a `before exec` hook sees
    and can veto the **command line**, *not* the spawned binary's internal
    syscalls — once `docker` runs it does what it does. And a hostile `.perch`
    author can simply not declare a hook. So hooks are **author-side policy +
    audit**, defense-in-depth that composes with OS-level confinement (see [the
    subprocess trust boundary](sandboxed-by-design.md#the-subprocess-trust-boundary-honest-scope))
    — never a substitute for it.

## Legibility

Hooks are cross-cutting, so perch keeps them visible: every hook is declared in
the file (not bolted on at runtime), `perch --check` validates that each handler
is a real command and warns on a target that can never match, and `perch --report`
shows each hook run as a span around the op it fired on. A reader can always see
the interception.

## Validation (`--check`)

- the handler must be a declared command (else **error**);
- the timing must be `before`/`after`/`on_error` (else **error**);
- a target that's neither a category, a known op kind, nor `any` **warns** ("the
  hook may never fire").

---
document_id: MAN-2026-0006
title: "Inline env prefix - NAME=VALUE binary verb --args"
document_type: manual
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [language, loader, interpreter, ops, validate, scan, simulate, preview]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [perch authors, people migrating shell scripts]
scope: "Task chapter for requirement R05 of PROP-2026-0002 - shell-style `NAME=VALUE` assignments before a declared-bin call, `exec`, a capture or a pipe stage, with resolution, gating, rejection rules and migration examples."
reason: "A shell one-liner such as `KUBECONFIG=$CFG kubectl get pods` should read the same in perch instead of needing a `with_env` block around it."
related_documents: [MAN-2026-0001, PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [manual, env, language, shell-migration, gating]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Inline env prefix - NAME=VALUE binary verb --args

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** grammar and loader, interpreter checks, process spawning, `--check`, `--scan`, `simulate`, `--dry-run`
> **Verified:** every example below was run on the `rust-port` build that reports 0.1.1 (the 0.2.0 bump happens at release), with `sh` as the binary so the output is deterministic.

## Summary

A call to a declared binary may start with `NAME=VALUE` assignments, exactly like a shell line:

```perch
KUBECONFIG=$CFG_PATH kubectl get pods
```

The assignments apply to that one spawned process. They do not change bindings, do not persist to the next op, and pass through the same environment gates as every other read of the host environment.

## When to use it

Use it for a one-off environment variable on one call: `KUBECONFIG`, `AWS_REGION`, `GOOS`/`GOARCH`, `NO_COLOR`. Use `with_env "K=v" ... end` when the same overlay should cover several ops, and the command's `env` modifier when it should cover the whole command ([language.md](../language.md)).

## Syntax

```text
[NAME=VALUE ...] BIN [tokens ...]            bare declared-bin call
exec [NAME=VALUE ...] BIN [tokens ...]       explicit exec
out = [NAME=VALUE ...] BIN [tokens ...]      capture (note the spaces around the first =)
A=1 BIN ... && B=2 BIN ...                   chained clauses, each with its own prefix
pipe ... end                                 a stage may carry a prefix
```

- `NAME` matches `[A-Za-z_][A-Za-z0-9_]*`.
- `VALUE` is bare (`debug`), quoted (`"two words"`, `'x y'`), `$NAME`, `${NAME}`, or a mix such as `prefix-${X}-suffix`. An empty value (`V=`) sets the variable to the empty string. `$NAME` is rewritten to `${NAME}` at load.
- The binary is the first token that is not an assignment. It must be a declared `bin` (or follow `exec`).
- Tokens after the binary are never read as assignments: `sh -c '...' _ A=1` passes `A=1` as an ordinary argument.

## How a value is resolved

For each `${REF}` in a value, in this order:

1. A binding in the running command: an argument or a captured variable.
2. The command's `env` modifier.
3. The host environment, **under the same gates as any other host-env read**:
   - If an operator `--env A,B` allowlist is active, the name must be on it.
   - Otherwise the name must be declared with `requires env "NAME"` or be in the default operational set (`PATH`, `HOME`, `TMPDIR`, `TMP`, `TEMP`, `SHELL`, `USER`, `LOGNAME`, `LANG`, `LANGUAGE`, `LC_ALL`, `LC_CTYPE`, `TERM`, `TZ`; the Windows set is longer).

A prefix therefore cannot read a secret the file did not declare. The overlay is added on top of the scrubbed child environment (see [requires.md](../requires.md)); it adds variables, it does not turn scrubbing off.

The overlay also wins over the other environment layers for that process:

```perch
requires
    bin "sh"
end
command a
    description "d"
    env GREETING "cmd-env"
    do
        with_env "GREETING=with-env"
            GREETING=prefix sh -c 'echo "inside with_env, prefix wins: $GREETING"'
            sh -c 'echo "inside with_env, no prefix: $GREETING"'
        end
    end
end
```

```text
$ perch -f w.perch a
inside with_env, prefix wins: prefix
inside with_env, no prefix: with-env
```

## CLI procedure

All examples use `requires` with `bin "sh"` and, where stated, `env "CFG_PATH"`.

### Literal values, bindings, declared env

```perch
requires
    bin "sh"
    env "CFG_PATH"
end

command literal
    description "literals"
    do
        A=1 B="two words" C='x y' sh -c 'echo "$A|$B|$C"'
    end
end

command basic
    description "binding value, two assignments, no leak"
    do
        cfg = upper "/etc/app.conf"
        APP_CONFIG=$cfg LOG_LEVEL=debug sh -c 'echo "cfg=$APP_CONFIG level=$LOG_LEVEL"'
        sh -c 'echo "next op: [$APP_CONFIG]"'
    end
end

command declared
    description "declared host env"
    do
        KUBECONFIG=$CFG_PATH sh -c 'echo "kube=$KUBECONFIG"'
    end
end
```

```text
$ CFG_PATH=/tmp/kube.yaml perch -f p.perch literal
1|two words|x y
$ CFG_PATH=/tmp/kube.yaml perch -f p.perch basic
cfg=/ETC/APP.CONF level=debug
next op: []
$ CFG_PATH=/tmp/kube.yaml perch -f p.perch declared
kube=/tmp/kube.yaml
```

The second `sh` sees an empty `APP_CONFIG`: the prefix did not leak.

### Resolution order

```perch
command fromhost
    description "host env"
    do
        K=$CFG_PATH sh -c 'echo "K=$K"'
    end
end

command binding
    description "binding shadows host env"
    do
        CFG_PATH = upper "from-binding"
        K=$CFG_PATH sh -c 'echo "K=$K"'
    end
end

command argv
    description "value comes from an arg"
    arg region
        type string
        default "eu-west-1"
    end
    do
        AWS_REGION=$region sh -c 'echo "AWS_REGION=$AWS_REGION"'
    end
end
```

```text
$ CFG_PATH=/etc/host.conf perch -f o.perch fromhost
K=/etc/host.conf
$ CFG_PATH=/etc/host.conf perch -f o.perch binding
K=FROM-BINDING
$ perch -f o.perch argv -region=us-east-2
AWS_REGION=us-east-2
```

### Capture, `exec`, chain, pipe

```perch
command capture
    do
        out = GREETING=hello sh -c 'echo "$GREETING world"'
        print "captured: ${out}"
    end
end
command viaexec
    do
        exec NAME=exec1 sh -c 'echo "name=$NAME"'
    end
end
command chain
    do
        A=1 sh -c 'echo "first A=$A B=$B"' && B=2 sh -c 'echo "second A=$A B=$B"'
    end
end
command pipeit
    description "prefix on a pipe stage"
    do
        out = pipe
            A=1 sh -c 'echo "stage A=$A"'
            sh -c 'cat'
        end
        print "${out}"
    end
end
```

```text
$ perch -f p.perch capture
captured: hello world
$ perch -f p.perch viaexec
name=exec1
$ perch -f p.perch chain
first A=1 B=
second A= B=2
$ perch -f w.perch pipeit
stage A=1
```

In a chain each clause has its own prefix and none carries over to the next.

### Gating: what is refused

An undeclared host variable (`SECRET_TOKEN` is set in the host but not in `requires`):

```text
$ SECRET_TOKEN=s3cr3t perch -f p.perch undeclared
env_not_declared: env prefix TOKEN=${SECRET_TOKEN}: env var "SECRET_TOKEN" is not declared in `requires` (SECRET_TOKEN)
```

`--check` warns in advance:

```text
$ perch -f p.perch --check
warning: undeclared: env prefix TOKEN=${SECRET_TOKEN} reads host env "SECRET_TOKEN" which is not declared in `requires` (add `env "SECRET_TOKEN"`) — the run is refused with env_not_declared unless `--env SECRET_TOKEN` allows it
```

The operator can allow a name at run time with `--env`, and an active `--env` list also restricts declared names:

```text
$ CFG_PATH=x SECRET_TOKEN=s3 perch -f p.perch --env SECRET_TOKEN undeclared
🔒 security: --env SECRET_TOKEN
token=s3
$ CFG_PATH=x SECRET_TOKEN=s3 perch -f p.perch --env SECRET_TOKEN declared
🔒 security: --env SECRET_TOKEN
env_not_declared: env prefix KUBECONFIG=${CFG_PATH}: env var "CFG_PATH" is not in the --env allowlist (CFG_PATH)
```

A name that is declared but optional and unset:

```text
$ perch -f p3.perch optunset         # requires: env "OPT_VAR" optional ; body: V=$OPT_VAR sh -c 'echo "v=$V"'
unresolved_var: env prefix V=${OPT_VAR}: "OPT_VAR" is not set
```

A required declared name that is unset fails earlier, in preflight:

```text
$ perch -f p.perch declared          # CFG_PATH not set
requirement_unmet: required env var "CFG_PATH" is not set (CFG_PATH)
```

### Rejected at load time

A prefix belongs to a declared-bin call or `exec` only (decision D2). On built-in ops, commands and templates it is refused, because those do not spawn a process of their own and the environment they would affect is perch's, not a child's. Use `with_env` or the command's `env` modifier there.

```text
$ perch -f bad.perch --check         # body: A=1 print "x"
✗ bad.perch: command t: env prefix (NAME=value before the call) is only valid on a declared-bin call or `exec`; `print` is a built-in op — use `with_env` or the command's `env` modifier instead

# body: A=1 basic2        (a command in the same file)
✗ bad.perch: command t: env prefix (NAME=value before the call) is only valid on a declared-bin call or `exec`; `basic2` is a command — use `with_env` or the command's `env` modifier instead

# body: 1A=x sh -c "echo hi"
✗ bad.perch: command t: malformed env assignment "1A=x": the name before `=` must match [A-Za-z_][A-Za-z0-9_]*
```

The same refusal applies to `mkdir`, `write_file` and every other built-in op. A line that is only an assignment (`A=1`) is not a prefix; it is read as a capture of a command named `1` and fails as `bin_not_declared`.

### Seen by the tools

`perch --scan --json` lists each prefixed call under `inferred.env_prefix` (names only, never values):

```text
$ perch -f p.perch --scan --json | jq -c '.inferred.env_prefix[0]'
{"bin":"sh","names":["APP_CONFIG","LOG_LEVEL"],"op":"exec"}
```

`--dry-run` and `simulate` show the prefix on the op:

```text
$ perch -f p.perch --dry-run basic
──── Dry-run — printing plan; no ops execute ────
  [1] upper "/etc/app.conf"   → ${cfg}
  [2] APP_CONFIG="${cfg}" LOG_LEVEL="debug" exec "-c" "echo \"cfg=$APP_CONFIG level=$LOG_LEVEL\"" bin="sh"
  [3] exec "-c" "echo \"next op: [$APP_CONFIG]\"" bin="sh" implicit="true"
$ perch -f p.perch simulate basic
── command basic — binding value, two assignments, no leak
✓ upper "/etc/app.conf"
✓ APP_CONFIG="${cfg}" LOG_LEVEL="debug" exec "-c"
   ↳ env prefix applies to this process only: APP_CONFIG, LOG_LEVEL
✓ exec "-c"
```

(The `exec "-c"` rendering, with the binary shown as `bin="sh"` or omitted, is how these two tools already print every bare declared-bin call in 0.1.x; the new part is the prefix and its note.)

## Compatibility note: `x=tool args` without spaces

Before 0.2.0 the loader read a statement-initial `x=tool args` as the capture `x = tool args` (spacing made no difference; taken from the loader's own source comment, the 0.1.1 binary was not re-run). From 0.2.0 an unspaced `NAME=VALUE` at the start of a statement is an env prefix. A script that relied on the unspaced capture breaks:

```text
# body: x=upper "abc"
bin_not_declared: command a line 0: `abc` is not a known op and not declared in `requires` (add `bin "abc"` to the requires block to run it) — did you mean the op `arch`?

# body: x=sh -c 'echo hi'
bin_not_declared: command unspaced line 0: `-c` is not a known op and not declared in `requires` (add `bin "-c"` to the requires block to run it) — did you mean the op `as`?
```

Fix: put spaces around the first `=`: `x = upper "abc"` (prints `ABC`), `x = sh -c 'echo hi'` (prints `got hi`). All 23 recipe files and all 11 demo `commands.perch` files in this repository pass `perch --check` on this build, so none of them relied on the unspaced form.

## Comparison with the other environment mechanisms

| Mechanism | Scope | Reads like shell? | Use it for |
|---|---|---|---|
| `NAME=VALUE bin ...` (this chapter) | one spawned process | yes | one-off variable on one call |
| `with_env "K=v,..." ... end` | the ops inside the block | no (a block) | the same overlay over several ops |
| command `env KEY "value"` | the whole command body | no | a command-wide default |
| `export NAME value` / `set_env` | the rest of the run, and the process | no | a deliberate persistent change (note: changes the process environment) |
| `requires env "NAME"` | declares a host variable the file may read | | gating, not setting |

## Migration from shell

| Shell | perch |
|---|---|
| `KUBECONFIG=$CFG kubectl get pods` | `KUBECONFIG=$CFG kubectl get pods` (declare `bin "kubectl"` and `env "CFG"`) |
| `GOOS=linux GOARCH=arm64 go build -o out ./cmd` | `GOOS=linux GOARCH=arm64 go build -o out ./cmd` (illustration; not run, `go` was not required for this manual; the same form with `sh` is verified above) |
| `NO_COLOR=1 ls` | `NO_COLOR=1 ls` (declare `bin "ls"`) |
| `A=1 cmd1 && B=2 cmd2` | identical; each clause keeps its own prefix |
| `export TOKEN=...; tool` | `TOKEN=$TOKEN tool` (declare `env "TOKEN"`) or `with_env` for several calls |
| `VAR=$(cmd) tool` | `v = cmd` then `VAR=$v tool` |

See [migrating-from-shell.md](../migrating-from-shell.md).

## Expected result and side effects

- The child process gets the overlay on top of its scrubbed environment; nothing else changes.
- Nothing is written to bindings or to the host process environment.
- Values appear in the child's environment (and so possibly in its process listing); names, not values, appear in `--scan --json`.

## Errors and recovery

| Error | Cause | Recovery |
|---|---|---|
| `env_not_declared: env prefix ...` | Host variable not declared in `requires`, or not on the `--env` allowlist. | Add `env "NAME"`, or have the operator pass `--env NAME`. |
| `unresolved_var: env prefix ...` | Declared optional variable is unset. | Set it, or give the value a binding. |
| `requirement_unmet: required env var ...` | Declared, not optional, and unset (preflight). | Set it, or mark it `optional`. |
| `env prefix ... is only valid on a declared-bin call or exec` | Prefix on a built-in op, command or template. | Use `with_env` or the command `env` modifier. |
| `malformed env assignment "..."` | Name before `=` is not an identifier. | Rename it. |
| `bin_not_declared` right after adding `=` | Unspaced capture read as a prefix. | Add spaces around the first `=`. |

## Limitations

- Values can only be strings: no arithmetic or defaults (`${VAR:-x}` is not supported).
- A prefix cannot make a variable visible that the file never declared; that is the point.
- The `--dry-run` and `simulate` renderings keep their existing format for the call itself.

## Version applicability

| Feature | Introduced | Notes |
|---|---|---|
| `with_env`, command `env` | 0.1.0 | Unchanged. |
| Inline `NAME=VALUE` prefix on bins, `exec`, captures, chained clauses, pipe stages | 0.2.0 | |
| Unspaced `x=tool args` reads as a prefix | 0.2.0 | Compatibility note above. |

## Related documents

[MAN-2026-0001](man-2026-0001-perch-manual-index.md) · [MAN-2026-0002](man-2026-0002-structured-scan.md) · [language.md](../language.md) · [requires.md](../requires.md) · [migrating-from-shell.md](../migrating-from-shell.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · Verified demo: DEMO-2026-0001 (planned at `docs/demos/demo-2026-0001-perch-0-2-0-verification.md`).

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial chapter for R05. |

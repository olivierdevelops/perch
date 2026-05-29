# Error handling — `try / rescue / finally` + `match` + the error-kind enum

> **What this enables.** Catch op failures, branch on what *kind* of error happened, run cleanup unconditionally, and re-raise after you've decided to. Composes with `retry`, `parallel`, `timeout`, and every other block op — errors propagate UP through blocks until something catches them.

!!! warning "Current status — the `try` block doesn't parse yet"
    The error model below (the error-kind enum, the `${err.*}` bindings, and `match`) is shipped and works. **The `try / rescue / finally` block itself currently fails to parse** in the released build (a known grammar bug, tracked). Until it's fixed: an op that fails propagates the error up and halts the command (the normal model); use `perch --check` / `perch simulate` to catch failure paths ahead of time, and use `match "${...}"` for value dispatch. The `match`/enum sections on this page are accurate today; the `try`-wrapped examples are the *intended* design and will work once the parse bug is resolved. See [using-perch-today.md](using-perch-today.md#gotchas-in-the-current-build).

---

## TL;DR

```perch
try
    let body = http_get "${url}"
rescue err
    match "${err.kind}"
        case http_5xx
            throw "${err.message}"               # let an outer retry handle it
        case http_4xx
            print "bad request: ${err.code}"
        case http_ssrf_blocked
            run alert "-msg=security: ${err.detail}"
        else
            throw "${err.message}"               # unknown — re-raise
    end
finally
    rm "${tmpfile}"
end
```

---

## The block shape

```perch
try
    OP_THAT_MIGHT_FAIL
    OP_AGAIN
rescue ERR_NAME
    # runs only if the try body errored
    # ${ERR_NAME.kind}, ${ERR_NAME.message}, ${ERR_NAME.code}, ${ERR_NAME.op}, ${ERR_NAME.detail}
finally
    # runs always (success or fail), AFTER rescue, BEFORE propagation
end
```

Both `rescue` and `finally` are **optional**, but at least one must be present (a bare `try ... end` is a parse error — you wrote a block but didn't say what to do with it).

### Why `rescue` and not `catch`?

perch already has a top-level `catch unknown ... end` for declaring catch-all CLI commands ("forward unknown verbs to git"). That's structurally different from exception handling. To keep both clear, perch uses Ruby-style `rescue` for the try-arm; `catch` stays the catch-all-command keyword.

### What you can do inside `rescue`

| | Behavior |
|---|---|
| **Run normally** (no `throw`, no `fail`) | Error is considered **handled**; execution continues after the `try` block (after `finally` runs). |
| **`throw "msg"`** | Re-raise. Error propagates after `finally` runs. Spelled `throw` (alias for `fail`) to read clearly. |
| **`fail "msg"`** | Same as `throw` — produces `user_fail`. |
| **Any other op errors** | That error replaces the original and propagates. |

### What `finally` does

- Runs **unconditionally** — after a successful `try` body, after a successful `rescue`, after a failing `rescue`.
- Errors in `finally` **override** the original error (otherwise nobody could see a cleanup failure).
- Common use: cleanup of resources allocated in the try (temp dirs, kube context switches, sentinel files).

---

## The error-kind enum

`${err.kind}` is one of these. `match` against the bare identifier (no quotes — the grammar accepts both `case user_fail` and `case "user_fail"`).

### Shell (4)

| Kind | When it fires |
|---|---|
| `shell_exit_nonzero` | A `shell` / `shell_output` / `try_shell` op's process exited with a non-zero status. `${err.code}` is the exit code. |
| `shell_metachars_denied` | `--no-shell-metachars` was set and the command contained one of `|`, `>`, `<`, `&`, `;`, `` ` ``, `$(`. |
| `shell_bin_not_allowed` | `--allow-bin` was set and the first token of the command isn't in the allowlist. |
| `shell_signal_killed` | The shell process was killed by a signal (e.g. SIGKILL by OOM, SIGTERM by timeout). |

### HTTP (6)

| Kind | When it fires |
|---|---|
| `http_4xx` | (Reserved — currently `http_get` returns body+status without raising on 4xx. A future `http_get_strict` op will use this.) |
| `http_5xx` | Same as above. |
| `http_redirect_refused` | A 3xx response would have redirected to a host outside `--allow-host`, or downgraded https→http without `--allow-scheme-downgrade`. |
| `http_ssrf_blocked` | Destination hostname resolves to a private / loopback / link-local / unspecified IP. The SSRF guard runs on every request AND every redirect hop. |
| `http_dns_failed` | Hostname couldn't be resolved. |
| `http_timeout` | Request exceeded the client timeout (30s default; overridable via `timeout` block). |

### File (4)

| Kind | When it fires |
|---|---|
| `file_not_found` | `read_file` / `stat` / similar on a path that doesn't exist. |
| `file_permission_denied` | OS refused read or write. |
| `file_path_disallowed` | A write hit a path outside the active sandbox's allowed write roots (or `--allow-fs-write`). |
| `file_already_exists` | An op that refuses to clobber found an existing file. |

### Capability (4)

These fire when the runtime restriction layer refuses an op outright (before the op runs).

| Kind | When it fires |
|---|---|
| `cap_shell_denied` | `--no-shell` was set; a shell op was attempted. |
| `cap_network_denied` | `--no-network` was set; an HTTP / DNS op was attempted. |
| `cap_subprocess_denied` | `--no-subprocess` was set; `pkg_install` / `kill_by_name` / etc. |
| `cap_write_denied` | `--no-write` was set; any FS-write op was attempted. |

### WASM (4)

| Kind | When it fires |
|---|---|
| `wasm_compile_failed` | The module bytes aren't a valid WASM module. |
| `wasm_module_exited` | The module called `os.Exit(N)` with `N != 0`. `${err.code}` is the exit code the module produced. |
| `wasm_capability_denied` | The module tried something not declared in the `wasm_run` body (file outside mounts, env var outside allowlist, etc.). |
| `wasm_http_refused` | The module called `perch.http_get` but had no matching `wasm_allow_host`, or the host wasn't in the intersection of allowlists. |

### Interpolation (2)

| Kind | When it fires |
|---|---|
| `unresolved_var` | `${name}` had no binding (no arg, no global, no let, not in the env allowlist). |
| `unresolved_template` | `call NAME args:val` where `NAME` isn't a declared template. |

### Runtime (4)

| Kind | When it fires |
|---|---|
| `timeout_exceeded` | Wall-clock deadline (`--max-runtime` or `timeout` block) was hit. |
| `signal_received` | SIGINT or SIGTERM received while running. Usually you handle this via `on_signal HANDLER` on the command, not via `rescue`. |
| `user_fail` | Explicit `fail "msg"` or `throw "msg"` in the .perch source. |
| `assert_failed` | An `assert_*` op didn't match. `${err.detail}` carries the actual vs expected. |

### Lookup (2)

| Kind | When it fires |
|---|---|
| `command_not_found` | `run X` where `X` isn't a declared command. |
| `bin_not_found` | `has_bin` returned false in a context that required the binary (e.g. inside `require_bin`). |

### Requires-manifest (4)

These fire only in files that declared a `requires ... end` block. See [docs/requires.md](requires.md).

| Kind | When it fires |
|---|---|
| `bin_not_declared` | A `shell` op's first token isn't listed under `bin` in the `requires` block. |
| `env_not_declared` | `get_env "X"` where `X` isn't listed under `env`. |
| `host_not_declared` | An HTTP op targets a host not listed under `host` (or its `*.suffix` form). |
| `read_not_declared` | A filesystem read op touches a path outside every declared `read` (and `write`) root. |
| `write_not_declared` | A filesystem write op touches a path outside every declared `write` root. |
| `requirement_unmet` | Preflight failure — required bin missing, version doesn't satisfy comparator, required env not set, or host OS/arch not in declared list. |

### Catch-all (1)

| Kind | When it fires |
|---|---|
| `unclassified` | An error from an op whose handler hasn't yet been migrated to tagged errors. Treat this as a "real bug we should fix" signal — open an issue with the audit log. |

---

## The `${err.*}` bindings

Inside a `rescue ERR_NAME` arm, five bindings are populated:

| Binding | Type | Example |
|---|---|---|
| `${err.kind}` | enum (string) | `http_5xx` |
| `${err.message}` | string | `"500 Internal Server Error"` |
| `${err.code}` | string | `"500"` (exit code, status code, or "" if N/A) |
| `${err.op}` | string | `"http_get"` (op kind that failed) |
| `${err.detail}` | string | structured extra info (e.g. blocked URL, denied binary name) |
| `${err}` | string | shorthand for `${err.message}` — useful for `throw "${err}"` |

(`ERR_NAME` is conventionally `err`; you can use any identifier.)

---

## `match` — value-driven dispatch

```perch
# Bare ident (no quotes, no ${...}) — works for plain binding names
match os
    case darwin
        ...
    case linux
        ...
end

# String form — required for dotted bindings (err.kind, err.message, etc.)
# because capy's tokenizer treats `.` as a separator.
match "${err.kind}"
    case literal_1
        OP_A
    case literal_2
        OP_B
    case "with quotes"      # both forms accepted
        OP_C
    else
        OP_D                # fallback
end
```

**When to use which form:**

| Target binding | Form              |
|----------------|-------------------|
| `os`, `arch`, `status`, `result` | `match os`     |
| `err.kind`, `err.code`, `err.message` | `match "${err.kind}"` (dotted — string form required) |

Semantics:

- **First matching case wins.** No fall-through to subsequent cases.
- **Exact string match** against the post-interpolation value of `VALUE`. No regex, no prefix matching (yet).
- **`else`** is the fallback. If no case matches and no `else` is present, the block is a no-op.
- **Case values can be bare identifiers OR quoted strings.** Bare idents are captured as their string spelling (so `case user_fail` matches the string `"user_fail"`).

`match` works on any string, not just `err.kind`:

```perch
match "${os}"
    case darwin
        shell "brew install jq"
    case linux
        shell "apt-get install -y jq"
    case windows
        shell "choco install jq -y"
    else
        fail "unsupported os: ${os}"
end
```

---

## Composition with other blocks

Errors propagate UP through blocks until something catches them.

### With `retry`

The classic "retry on transient errors, fail loudly on permanent ones":

```perch
retry max=5 delay=2s
    try
        let body = http_get "${url}"
    rescue err
        match "${err.kind}"
            case http_5xx
                throw "transient"            # retry will catch + re-run
            case http_timeout
                throw "transient"
            case http_dns_failed
                throw "transient"
            case http_4xx
                # 4xx isn't transient — fail loudly
                fail "client error ${err.code}: ${err.message}"
            else
                throw "${err.message}"
        end
    end
end
```

### With `parallel`

Each branch's error propagates to the surrounding `parallel`. To prevent one branch's failure from cancelling the others, wrap each in its own `try`:

```perch
parallel max=3
    try
        run deploy_region "-region=a"
    rescue err
        run alert "-msg=a failed: ${err.message}"
    end
    try
        run deploy_region "-region=b"
    rescue err
        run alert "-msg=b failed: ${err.message}"
    end
end
```

### With `timeout`

```perch
try
    timeout secs=10
        let body = http_get "${slow_url}"
    end
rescue err
    match "${err.kind}"
        case timeout_exceeded
            print "skipped — too slow"
        else
            throw "${err.message}"
    end
end
```

---

## Common patterns

### 1. Cleanup with `finally`

```perch
let tmp = mktemp_dir
try
    cp "./src/big-file" "${tmp}/staged"
    shell "process ${tmp}/staged"
    mv "${tmp}/staged" "./out/"
finally
    rm "${tmp}"        # always cleans up, even if process failed
end
```

### 2. Optional dependency with graceful degrade

```perch
try
    let cached = http_get "http://redis:6379/value"
    print "from cache: ${cached}"
rescue err
    # Cache down — proceed without
    print "cache miss / unreachable: ${err.kind}"
    let cached = "${default_value}"
end
# ${cached} is set either way
```

### 3. Discriminate on multiple cases

```perch
try
    shell "deploy.sh ${target}"
rescue err
    match "${err.kind}"
        case shell_exit_nonzero
            # Distinguish by exit code
            match "${err.code}"
                case "1"
                    fail "generic deploy failure"
                case "2"
                    print "deploy: target unreachable — retrying"
                    throw "transient"
                case "127"
                    fail "deploy script not found"
                else
                    fail "deploy failed with code ${err.code}"
            end
        case shell_bin_not_allowed
            fail "deploy.sh isn't in --allow-bin (sandbox misconfigured)"
        else
            throw "${err.message}"
    end
end
```

### 4. Re-raise from inside `rescue`

```perch
try
    OP_THAT_MIGHT_FAIL
rescue err
    # Log + alert, then re-raise
    print "[ERROR] ${err.kind}: ${err.message}"
    run alert "-msg=${err.message}"
    throw "${err.message}"      # propagates after finally runs
finally
    rm "${tmp}"
end
```

---

## Honest limits

- **No exception hierarchies.** Error kinds are flat — there's no "all `http_*` matches `case http_error`." If you want grouping, list each kind explicitly or use `else`.
- **`match` is exact-match only.** No regex, no prefix matching, no guards. Future work.
- **`try` requires either `rescue` or `finally`** (or both). Bare `try ... end` is a parse error.
- **Capability denials currently produce `unclassified`.** Tagging in progress; existing CLI flag combinations still work, just without the precise kind name.
- **`http_4xx` and `http_5xx`** are reserved kinds — they exist in the enum but the current `http_get` op returns the body without raising on non-2xx status. A future `http_get_strict` op will surface them.
- **Errors in `finally` override the original error.** If you want the original to take precedence, don't let finally throw — wrap it in its own `try`.

---

## See also

- [The complete guide → Error handling](guide.md#error-handling-and-recovery) — the broader pattern context
- [docs/language.md](language.md) — full grammar reference
- [docs/sandbox.md](sandbox.md) — capability-denial kinds in context

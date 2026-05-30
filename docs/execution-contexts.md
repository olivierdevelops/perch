# Templates, execution contexts, and `--report`

> **TL;DR.** Six new block ops wrap a body to change *how* it runs:
> `parallel`, `timeout`, `retry`, `with_env`, `with_cwd`, `sandbox`, and
> `cache`. Plus a parse-time **template** mechanism that stamps out
> parameterized op-sequences, and `--report` that renders the whole
> run as a span tree.
>
> This page is the practical guide — when to reach for each, what they
> do at runtime, and how they compose.

These additions sit on top of the existing perch surface. They don't
change what a `command` is, don't introduce closures or higher-order
functions, and don't break any existing file. They extend the **op
catalog** (where perch is supposed to grow) rather than the language
(where perch is supposed to stay thin). See [language.md](language.md)
for the canonical reference; this page is the worked-examples tour.

---

## The mental model

There are two kinds of new vocabulary, doing two different jobs:

| Mechanism | Job | Expansion time | Example |
|---|---|---|---|
| **Template** | Eliminate repetition | Parse time (inline splice) | `call check_bin "docker"` |
| **Execution context** | Wrap a body to modify how it runs | Run time (block op) | `retry 3 ... end`, `sandbox "no_shell" ... end` |

**Templates stamp out boilerplate. Execution contexts wrap execution.**
They are not interchangeable, and trying to use one for the other's job
produces awkward code. The line:

- If you're saying *"I keep writing the same 4 ops with different
  names"* → reach for a **template**.
- If you're saying *"wrap this body in setup/teardown / parallelism /
  retry / a deadline / a capability gate"* → reach for an **execution
  context**.

---

## Templates

A `template NAME … end` block has the **same arg-block syntax as
`command`** and the same `do … end` body. The only difference is
*when* the body's ops materialize:

- A `command` is invoked at run time, with its body executed.
- A `template` is invoked at parse time, with its body spliced into
  the call site.

> **A template is a command that expands at parse time instead of
> running at execution time.**

### Declaring a template

```capy
template check_bin
    description "Fail unless the named binary is on PATH"
    arg name
        type string
        description "Binary to check"
    end
    do
        if not_exists "${name}"
            fail "${name} is required but not installed"
        end
    end
end
```

This declares a template that takes one string parameter `name` and
expands to a 4-op body wherever it's called.

### Calling a template

```capy
command setup
    do
        call check_bin "docker"
        call check_bin "kubectl"
        call check_bin "jq"
    end
end
```

After parsing, `setup`'s body is identical to what you'd get by
inlining `check_bin`'s body three times with `${name}` substituted:

```capy
# What the interpreter actually sees:
command setup
    do
        if not_exists "docker"
            fail "docker is required but not installed"
        end
        if not_exists "kubectl"
            fail "kubectl is required but not installed"
        end
        if not_exists "jq"
            fail "jq is required but not installed"
        end
    end
end
```

That's the entire mental model. **Find-and-replace with named
arguments, at parse time, before the interpreter ever sees the
program.**

### Default and optional args

Template args are real perch args — they support `default`, `optional`,
and everything else `command` arg blocks support:

```capy
template install_pkg
    arg pkg
        type string
    end
    arg version
        type string
        default "latest"
    end
    do
        exec brew install ${pkg}@${version}
    end
end

command setup
    do
        call install_pkg "jq"            # version defaults to "latest"
        call install_pkg "ripgrep" "13.0"
    end
end
```

### Templates calling templates

Templates can compose. The expansion pass resolves them in a single
walk, with recursion explicitly rejected:

```capy
template log_step
    arg label
        type string
    end
    do
        print "==> ${label}"
    end
end

template install_pkg
    arg pkg
        type string
    end
    do
        call log_step "Installing ${pkg}"
        exec brew install ${pkg}
    end
end

command setup
    do
        call install_pkg "jq"
    end
end
```

The validator rejects `template_A` calling `template_A` (directly or
transitively) with a clear error pointing at both the use site and
the template definition.

### What templates can't do

These are not bugs, they're the line:

- **No recursion.** Validator rejects.
- **No closures, no return values.** A template is pure parameter
  substitution. Use `let` capture and ordinary bindings for state.
- **No declaration-emitting templates.** A template can't contain
  `command`, `import`, or `globals`. Templates expand inside a body,
  not at file scope.
- **Templates don't appear in `--help`, MCP, or `--list`.** They're
  invisible at runtime. Only the post-expansion ops exist.

The framing for documentation, in one sentence:

> **perch has templates, not functions.** A template is a parse-time
> rewrite — no closures, no return values, no recursion. If you reach
> for one and find yourself wanting any of those, you've outgrown
> perch and should call out to a real language via `shell`.

---

## Execution contexts

Six block ops that wrap a body to change *how* it runs. Each is
**purely about execution semantics** — none of them introduce new data
types, new control flow, or new abstraction units. They compose by
nesting.

### `parallel … end`

Run each direct child concurrently:

```capy
command release
    description "Build all three platforms in parallel"
    do
        parallel
            build_darwin
            build_linux
            build_windows
        end
        print "all three done"
    end
end
```

- Each child runs in its own goroutine against a **copy of Bindings**.
- The block exits when ALL children complete.
- If any child errored, the block's error is the first one (siblings
  finish regardless).
- `let X = …` captures inside `parallel` are local to the branch and
  do not survive the block — use the `cache` block or a file if you
  need cross-branch state.
- Nesting is permitted: `parallel { … parallel { … } }`.

### `timeout "DURATION" … end`

Cap wall-clock for the body:

```capy
timeout "30s"
    exec kubectl apply -f manifest.yaml
    exec wait-for-rollout deploy/api
end
```

- `DURATION` is any Go-style duration string: `"500ms"`, `"30s"`,
  `"5m"`, `"1h"`.
- A long-running op can't be interrupted mid-call (Go's `exec.Cmd`
  doesn't support that without explicit context wiring). The *next*
  op after the deadline trips returns `ErrTimeout`.
- Inner `timeout` blocks can only **narrow** the active deadline,
  never extend it. The outer `--max-runtime` CLI flag is the
  hardest bound.

### `retry N … end`

Retry the body on error:

```capy
retry 3
    exec curl -fsSL https://flaky.example.com/api
end
```

- Default sleep schedule is exponential: 1s, 2s, 4s, 8s, …, capped at
  5 minutes per sleep.
- Returns the LAST error if every attempt failed, wrapped with
  attempt count.
- Never retries past the outer command's deadline (if a `timeout` or
  `--max-runtime` is active).
- `retry` with no argument defaults to 3 attempts.
- The validator surfaces a warning when the body contains an obviously
  non-idempotent op (`rm -rf`, `mv`, `http_post`) — not blocked, just
  flagged at `--check` time.

### `with_env "K1=v,K2=v" … end`

Overlay env vars for the duration of the body:

```capy
with_env "GOOS=linux,CGO_ENABLED=0"
    exec go build -o bin/linux/app ./cmd
end
```

- Comma-joined `KEY=value` pairs.
- Restores prior values on exit (even if the body errored).
- More readable than the per-command `env` modifier when the override
  is scoped to a few ops, not the whole command.

### `with_cwd "./path" … end`

Bracketed `cd` that auto-restores:

```capy
with_cwd "./subproject"
    exec npm install
    exec npm run build
end
# back to the previous cwd here, even if the body errored
```

- Relative paths resolve against the current cwd.
- Errors if the path doesn't exist or isn't a directory.
- Unlike the standalone `cd` op (which persists for the rest of the
  command), `with_cwd` is bracketed.

### `sandbox "FLAGS" … end`

Narrow the active capability mask for the body:

```capy
sandbox "no_shell,no_network"
    vendor.update_check
end
```

- `FLAGS` is a comma-joined list. Supported: `no_shell`,
  `no_subprocess`, `no_network`, `no_write`.
- **Intersection rule:** masks can only be **narrowed**, never
  widened. There is no `allow_shell` — you cannot re-enable what an
  outer mask (or the CLI flags) blocked.
- The runtime enforces the intersection of every active mask:
  - The outermost CLI flags (`--no-shell`, `--no-network`, …)
  - Plus every `sandbox` block currently on the stack
- Op handlers consult the mask at dispatch time. A blocked op errors
  with `op "shell" forbidden by sandbox (no-shell scope)` — pointing
  at the **block** scope so the user knows it's the file, not the
  CLI, that denied it.
- Static enforcement at `--check` time (walking the call graph from
  the sandbox block) is on the roadmap; runtime enforcement is
  shipped today.

The killer use case is **trusting third-party imports**:

```capy
import "./vendor/third-party.perch" as tp

command safe_lookup
    do
        sandbox "no_shell,no_network,no_write"
            tp.do_thing
        end
    end
end
```

The imported file's ops run only with the capabilities the sandbox
permits — you cannot be tricked into running `shell` from imported
code you didn't write.

### `cache "KEY" "TTL" … end`

User-keyed body cache:

```capy
cache "build-${target}-${sha256_file('go.sum')}" "24h"
    exec go build -o bin/${target} ./cmd
    let size = file_size "bin/${target}"
end
```

- **First arg** = cache key. Interpolation happens before hashing,
  so `${target}` and `${sha256_file('go.sum')}` materialize at run
  time.
- **Second arg** = TTL duration (`"24h"`, `"5m"`, `"1h30m"`, etc.).
- **On miss:** runs the body and persists every `let X = …` binding
  newly produced. Auto-bindings (`os`, `home`, `cache_dir`, …) are
  excluded so the cache file stays small.
- **On hit (within TTL):** skips the body entirely and replays the
  captured bindings into scope. You see a one-line message:

  ```
  ↪ cache hit: build-linux-3f9a2b1c (replayed 1 bindings, 23h45m left)
  ```

- Stored at `~/.cache/perch/blocks/<sha256(key)>.json`. Safe to
  delete that directory at any time — next miss rebuilds.

#### Honest framing: this is NOT Bazel

perch does **not** hash the body's transitive inputs. The user picks
the key, and the key is the contract. If you leave a stale input out
of the key, you get stale cache.

This is intentional. perch's `shell` op is a black box the runtime
can't see through — it can't know which files `go build` reads. A
real content-addressed cache (Bazel, Nix) requires hermeticity that
perch deliberately doesn't have (see [ideas/05](https://github.com/luowensheng/perch/blob/main/ideas/05-build-system-direction.md)).

The user-keyed model matches how every practical caching layer
actually works — GitHub Actions `cache@v3`, Earthly's `--cache-id`,
even Docker build-layer hashing in practice. Build the key right;
get reliable cache. Build the key wrong; that's a key bug, not a
cache bug.

---

## `--report` — see what ran, in what order, for how long

When any of these contexts are in play, `--report` renders the
execution as a tree at the end of the run:

```sh
$ perch --report release
```

```
── perch trace ─────────────────────────────────
✓ release (4.21s)
└─ ✓ sandbox "no_network,env=KUBECONFIG" (4.20s)
   ├─ ✓ with_lock "prod-deploy" (4.18s) [from template with_lock]
   │  ├─ ✓ acquire_lock "prod-deploy" (12ms)
   │  ├─ ✓ retry attempts=3 (4.10s)
   │  │  └─ ✗ exec kubectl apply ... (5.00s)
   │  │     ↳ error: timeout after 5m
   │  └─ ✓ release_lock "prod-deploy" (8ms)
   └─ ✓ swap_traffic (4ms)
```

### What you get for free

- **Errors carry their full path.** "shell failed" becomes
  `release > sandbox > with_lock(prod-deploy) > retry attempt 1 > shell …`.
- **Durations roll up.** The sandbox span's duration includes its body.
- **Template provenance is shown.** Ops expanded from a template have
  `[from template NAME]` so you can tell which call site produced
  which leaf.
- **`parallel` shows real concurrent wall-clock.** A 30s `parallel`
  block of three 30s children appears as 30s on the wall, not 90s.

### Variants

- `--report` — render to stderr (default).
- `--report=PATH` — write the tree to a file.
- `--report=-` — write to stdout.

### `--trace` — same tree, but LIVE (streamed while running)

`--report` renders the tree **after** the run. `--trace` streams it
**as the run happens**:

```sh
$ perch --trace -f release.perch deploy
▸ sandbox              flags="no_network"
  ▸ retry                attempts=3
    ▸ shell                "kubectl apply -f manifest.yaml"
configmap/api-config configured
deployment.apps/api configured
    ✓                            (1.20s)
  ✓                              (1.20s)
✓                                (1.21s)
```

- Each op prints `▸ kind args…` the moment it fires.
- The op's own stdout/stderr (the `kubectl` output above) appears in line.
- `✓ (dur)` closes the op on success; `✗ error` for failures.
- Block ops nest their children by indent.

Variants:

- `--trace` — stream to stderr (default).
- `--trace=PATH` — write to a file.
- `--trace=-` — write to stdout.

### Trace vs. report vs. audit

Three sinks, same hook order — they always agree on what ran:

| Flag | When | Form | Use it for |
|---|---|---|---|
| **`--trace`** | While running (live) | Human-readable, indented | Watching a long-running command's progress in real time |
| **`--report`** | After running | Human-readable tree | Reviewing what happened after the fact |
| **`--audit FILE.ndjson`** | While running (live) | JSON-per-line | Machine ingest (Loki / Datadog / CI structured logs) |

`--trace` and `--report` share the Tracer slot — pick one. `--audit` is
independent and composes with either. **All three** can show errors with
their full block-path context (parent context names appear above the
failing op).

---

## Composing them — a complete example

```capy
template with_log
    description "Prefix a body's print output with a section header"
    arg label
        type string
    end
    do
        print "==> ${label}"
    end
end

command release
    description "Build, test, sign and publish for all targets"
    do
        sandbox "no_network"
            call with_log "Setting up"
            with_env "GOFLAGS=-trimpath,CGO_ENABLED=0"
                # Three parallel builds with shared env
                parallel
                    cache "build-darwin-${sha256_file('go.sum')}" "24h"
                        timeout "5m"
                            with_env "GOOS=darwin"
                                exec go build -o bin/darwin/app ./cmd
                            end
                        end
                    end
                    cache "build-linux-${sha256_file('go.sum')}" "24h"
                        timeout "5m"
                            with_env "GOOS=linux"
                                exec go build -o bin/linux/app ./cmd
                            end
                        end
                    end
                    cache "build-windows-${sha256_file('go.sum')}" "24h"
                        timeout "5m"
                            with_env "GOOS=windows"
                                exec go build -o bin/windows/app.exe ./cmd
                            end
                        end
                    end
                end
            end
        end

        # Network needed for signing + publish; sandbox above doesn't apply here
        call with_log "Signing"
        retry 3
            exec cosign sign --key cosign.key bin/*/app
        end

        call with_log "Publishing"
        retry 3
            exec scp -r bin/ releases-server:/srv/releases/v${version}/
        end
    end
end
```

Run it with the tree view:

```sh
$ perch --report release
```

Output (truncated):

```
── perch trace ─────────────────────────────────
✓ release (1m42s)
├─ ✓ sandbox "no_network" (45s)
│  ├─ ✓ print "==> Setting up" (5µs) [from template with_log]
│  └─ ✓ with_env env=GOFLAGS=-trimpath,CGO_ENABLED=0 (45s)
│     └─ ✓ parallel (45s)
│        ├─ ✓ cache "build-darwin-..." (45s)
│        │  └─ ✓ timeout "5m" (45s)
│        │     └─ ✓ exec go build ... (45s)
│        ├─ ✓ cache "build-linux-..." (38s)
│        │  └─ ✓ timeout "5m" (38s)
│        │     └─ ✓ exec go build ... (38s)
│        └─ ✓ cache "build-windows-..." (52s)
│           └─ ✓ timeout "5m" (52s)
│              └─ ✓ exec go build ... (52s)
├─ ✓ print "==> Signing" (4µs) [from template with_log]
├─ ✓ retry attempts=3 (8s)
│  └─ ✓ exec cosign sign ... (8s)
├─ ✓ print "==> Publishing" (4µs) [from template with_log]
└─ ✓ retry attempts=3 (49s)
   └─ ✓ exec scp -r bin/ ... (49s)
```

Note: the three `parallel` build children show their own wall-clock
durations (45s / 38s / 52s) but the `parallel` block as a whole only
took 52s — the slowest branch, not the sum.

Run it again:

```
✓ release (15s)
├─ ✓ sandbox "no_network" (1s)
│  └─ ✓ with_env (1s)
│     └─ ✓ parallel (1s)
│        ├─ ↪ cache hit: build-darwin-... (replayed 0 bindings, 23h59m left)
│        ├─ ↪ cache hit: build-linux-...
│        └─ ↪ cache hit: build-windows-...
…
```

Three cache hits, three builds skipped, total run time drops by 90%.

---

## Decision flowchart — which one do I want?

```
Q: I want to wrap a body in setup/teardown.
A: → Use an execution context. Pick the one that matches:
    - run children concurrently       → parallel
    - cap wall-clock                  → timeout
    - retry on failure                → retry
    - per-block env overlay           → with_env
    - temporary cwd switch            → with_cwd
    - restrict what the body can do   → sandbox
    - memoize the body                → cache

Q: I keep writing the same N ops with different names.
A: → Use a template.

Q: I want to capture a value, mutate it, and return it.
A: → That's a function. perch doesn't have those. You've outgrown
   the DSL — call into Go / Python / Bash via `shell` for that piece.

Q: I want a body cache that invalidates when files change.
A: → Use cache with the file's hash in the key:
        cache "build-${target}-${sha256_file('go.sum')}" "24h"
   perch does NOT auto-detect file changes (see "Honest framing"
   above for why).

Q: I want to enforce that an imported file can't do shell or network.
A: → Wrap the call in sandbox:
        sandbox "no_shell,no_network"
            run vendor.do_thing
        end

Q: How do I see what actually ran?
A: → Add --report to the command line. Tree view at the end.
     Or --audit FILE.ndjson for machine-readable.
```

---

## What's NOT changed

These additions are purely additive:

- **No keywords removed.** Existing `.perch` files run unchanged.
- **No semantic changes to `command`, `do`, `if`, `for_each`, `let`,
  `run`, or any other existing form.** They behave exactly as before.
- **No new data types.** No lists, no maps, no closures. Bindings
  remain string / int / float / bool.
- **No MCP schema changes.** Templates are invisible to agents
  (expanded before MCP sees the program); execution contexts are
  invisible because they live inside a command's body, not in its
  arg spec.
- **No `--check` regressions.** The validator catches new failure
  modes (template recursion, unknown template call, unknown sandbox
  flag, malformed retry/timeout duration) without breaking anything
  it already caught.

What this is in one sentence:

> Perch gained six wrapping primitives and a parameterised stamping
> mechanism, all expressed in the existing block-op shape. The
> language model didn't change; the op catalog grew.

See [language.md](language.md) for the canonical syntax reference and
[ideas/](https://github.com/luowensheng/perch/blob/main/ideas/) for
the design rationale.

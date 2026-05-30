# Language reference

The complete surface of the `commands.perch` DSL. Two firm rules to keep in mind everywhere:

1. **Config vs body is syntactic.** Between `command NAME` and `do` is *declarative configuration*. Inside `do … end` is the *executable body*. They never mix.
2. **`${name}` interpolates at runtime.** Capy parses `${name}` inside `"..."` captures as literal characters (it only interpolates inside its own template/backtick contexts), so the placeholder round-trips through parsing into the program JSON unchanged. The Go runtime substitutes from the bindings table (args → globals → host env) just before each op runs. To pass a literal `${VAR}` through to a `shell` call (e.g. an actual shell variable), prefix with a backslash: `\${VAR}`.

## File structure

```capy
name    "..."           # top-level metadata
about   "..."
version "..."

KEY = VALUE             # bindings shared by every command (bare, top-level)
...

command NAME            # one or more commands
    ...config...
    do
        ...ops...
    end
end

catch NAME              # optional fallback for unknown commands
    do
        ...ops...
    end
end
```

## Top-level metadata

| Surface | Effect |
|---|---|
| `name "x"`    | Program name. Shown in `--help`. |
| `about "x"`   | Top-level description. Shown in `--help`. |
| `version "x"` | Program version. Returned by `--version`. |

## Top-level bindings

Bindings shared by every command invocation are declared **bare at the top level** — `NAME = value`, no wrapping block. (The old `globals … end` block was removed; a leftover one is a clear load error.) The literal's type — `bool`, `int`, `float`, `string` — is preserved.

```capy
verbose    = true             # bool
PORT       = 8080             # int
RATE       = 0.5              # float
BUILD_DIR  = "./builds"       # string
```

Bindings are visible inside every command body as `${verbose}`, `${BUILD_DIR}`, etc., and can be referenced by name in the `requires` block — `write BUILD_DIR` is sugar for `write "${BUILD_DIR}"`. By convention, UPPER_SNAKE_CASE bindings are also exported as environment variables to every spawned `shell`/`exec` call. A bare `NAME = value` is only legal at file scope; inside a command body use `let NAME = …`.

## `requires … end` — the file-declared manifest

Declares everything the file needs from the host: bins, env vars, hosts, OS, arch. When present, perch enforces strictly — undeclared shell bins / HTTP hosts / `get_env` reads error (`bin_not_declared`, `host_not_declared`, `env_not_declared`), and preflight verifies bins exist and (optionally) match a pinned SHA-256 hash. There is **no version checking** — that would require executing the binary before the sandbox exists (and a trojaned binary lies about its version); pin the artifact's hash instead.

A `bin` may be a bare command resolved on `PATH` (`go`, `docker`) **or a path
to an executable** (`./bins/tool.exe`, `${script_dir}/bin/tool`) — path-form
bins are checked for existence on disk (relative to the `.perch` file) rather
than a PATH lookup. Add `as NAME` to give a path a clean handle you can invoke
by name; perch resolves the alias to the real path before spawning:

```perch
requires
    bin "./bins/binary.exe" as binary      # path bin + handle
end
command run
    do
        binary --serve                     # runs ./bins/binary.exe (resolved to the script dir)
    end
end
```

```perch
requires
    bin "kubectl"
        hash "sha256:abc123…"              # pin the exact build you trust (read-only, no exec)
    end
    bin "docker"  optional
    bin "go"
    bin "internal-tool"
        hash_file "bundle:checksums/tool.sha256"   # pin from an embedded file
    end

    env  "KUBECONFIG"
    env  "DEBUG" optional
    host "api.github.com"
    host "*.amazonaws.com"
    os   "linux"
    arch "amd64"
end
```

`perch --check` statically flags undeclared *literal* usage before you ever run it. Full reference: [requires.md](requires.md).

## Commands

A `command NAME ... end` declares one named, callable unit. Inside it the **config region** runs from the opening line to the `do` keyword; the **body region** runs from `do` to its matching `end`.

```capy
command build
    # ── config ──
    description "Compile myapp"
    arg target
        type string
        default "darwin"
        description "Target OS"
    end
    require_os  "darwin" "linux"
    env         GO111MODULE "on"

    # ── body ──
    do
        print "Building for ${target}"
        exec go build -o ./bin/${target}/myapp
    end
end
```

### Config

| Surface | Effect |
|---|---|
| `description "x"`                 | Help text shown by `--help`. |
| `arg NAME ... end`                | Declares a typed CLI argument. Each property is its own labelled inner line (see below). |
| `private`                         | Hide from CLI; only callable via `run` from another command. |
| `detached`                        | Don't wait on processes spawned by `shell_detached`. |
| `proxy_args`                      | Skip arg parsing; argv comes through as `${proxy_args}`. Required on both `command` and `catch` blocks — without it, `${proxy_args}` is unbound. |
| `require_os "darwin" ...`         | Refuse to run on other OSes. Repeatable. |
| `require_arch "arm64" ...`        | Refuse to run on other architectures. |
| `dir "./subdir"`                  | Set the cwd for the body. |
| `on_signal HANDLER`               | Run `HANDLER` (another command) on SIGINT/SIGTERM. |
| `env KEY "value"`                 | Set an env var for the body's `shell` calls. |

### Arg blocks

Each argument is its own `arg NAME ... end` block inside the config region. The body holds labelled fields; nothing is positional.

```capy
arg target
    type string                # required: string / int / float / bool
    default "darwin"           # optional: literal value (string/int/float/bool)
    description "Target OS"    # optional: shown in --help
    optional                   # optional: arg may be omitted even with no default
    index 0                    # optional: bind to positional index N
end
```

- **`type`** is the only required field.
- **`default`** must match `type`. Presence of `default` makes the arg optional.
- **`description`** uses the same `description` keyword as the command's own description — context inside an `arg` block routes it to the arg.
- **`optional`** marks an arg that has no default but can be omitted; ops that read it should `if_empty` guard.
- **`index N`** binds the arg to a positional slot. Without it, the arg is a `-name=value` flag.
- **`rest`** (variadic) — collects every remaining positional argument into a newline-joined string. Must be the **last** declared arg, type `string`, no default, must carry `index N`. A companion `${NAME_count}` int binding tells you how many values arrived. Equivalent to Go's `args ...string`. Iterate with `for_each "${NAME}" item ... end`.

```capy
command pack
    description "Archive files into a tarball"
    arg out
        type string
        index 0
    end
    arg files
        type string
        index 1
        rest                      # captures every remaining arg
    end
    do
        print "got ${files_count} files"
        for_each "${files}" f
            print "  → ${f}"
        end
        tar_create "${files}" "${out}"
    end
end
```

```sh
$ perch pack out.tar.gz a.txt b.txt c.txt
got 3 files
  → a.txt
  → b.txt
  → c.txt
```

For the older "forward every arg as a single space-joined string" pattern, see the `proxy_args` command modifier instead — it bypasses arg declarations entirely.

Multiple args just sit next to each other:

```capy
command release
    description "Cross-compile and publish"
    arg target
        type string
        default "darwin"
        description "Target OS"
    end
    arg version
        type string
        description "Release tag (required)"
    end
    arg dry_run
        type bool
        default false
        description "Skip the actual upload"
    end
    do ...
end
```

### Body

```capy
do
    OP ARGS...
    let NAME = OP ARGS...        # capture an op's return value
    if os == "darwin"
        OP ARGS...               # nested ops, only run on macOS
    end
    other_command            # dispatch into another command
    fail "explicit error"        # exit non-zero with a message
end
```

Inside the body:

- Every line is an **op call**. The op kind (`print`, `shell`, `mkdir`, …) is the first identifier; args follow.
- **`let NAME = OP ARGS`** runs the op and stores the return value under `NAME`. Subsequent strings interpolate via `${NAME}`.
- **Block ops** — the unified `if EXPR ... end` wraps a nested body that runs only when the condition holds. EXPR may be a comparison (`os == "linux"`, `size > 1000000`), a truthy/falsy check (`has_bin`, `not has_bin`), or a predicate call (`exists "./bin"`). See "Conditionals" in the [op catalog](op-reference.md).
- **`run NAME`** calls another command in the same file (private commands are callable here).

See the [op catalog](op-reference.md) for every built-in op.

### Running external tools — `shell` vs `exec`

Two ways to run a subprocess:

```perch
shell "docker compose up -d"          # via the host shell (bash/cmd.exe)
exec docker compose up -d             # shell-free: BIN + structured argv
```

- **`shell "…"`** hands the string to bash (POSIX) / `cmd.exe` (Windows). Pipes/globs/`&&` work because the shell expands them — at the cost of per-OS quoting differences and an injection surface.
- **`exec BIN tok…`** runs `BIN` directly (`os/exec`, never a shell). Each token is exactly one argv slot — bare flags/paths work unquoted (`exec git log --oneline -10`); quote a token only to keep embedded spaces (`exec git commit -m "fix the bug"`); `${x}` always fills exactly one slot, even if its value has spaces or metacharacters (no injection). Captures stdout *and* streams it. When a `requires` block is present, `BIN` must be a declared `bin`.
- **`pipe … end`** wires `stdout → stdin` between `exec` stages with in-process pipes — no shell:

  ```perch
  let n = pipe
      exec docker ps -q
      exec wc -l
  end
  ```

- **Chaining** — `exec a && exec b`, `exec a || exec b`, `exec a ; exec b` join clauses by exit status (perch operators, not shell metachars): `&&` runs the next clause only on success, `||` only on failure, `;` always. They're literal source tokens, so an interpolated `${x}` can never *become* an operator.

  ```perch
  exec git pull && exec go build && exec go test   # stop at first failure
  exec which gh || exec brew install gh            # fallback
  ```

This is the shell-free model from [sandboxed-by-design.md](sandboxed-by-design.md) §3.2/§3.5, shipping today. The line-toolbox (`grep` / `cut` / `head` / `sort_lines` / …) composes with captured output to replace a pipeline's middle stages. **`shell` is deprecated** in favor of `exec` — keep it only for genuine shell needs (a value that must word-split, e.g. `${proxy_args}`, or a gnarly one-off `awk`/`sed` chain).

### Error handling — `try / rescue / finally`

```perch
try
    exec flaky-deploy
rescue
    print "deploy failed: ${err.kind} / ${err.message}"
finally
    exec cleanup-temp
end
```

`rescue` runs only if the body raised (with `${err.kind}`, `${err.message}`, … bound); `finally` always runs. Both are optional — a `finally`-only `try` re-raises after cleanup; only a non-empty `rescue` swallows. Discriminate kinds with `match err.kind` (bare dotted ident) or `match "${err.kind}"`. Full model: [errors.md](errors.md).

## `catch NAME … end`

A fallback dispatched when the user types a command we don't have. The unknown name is bound to `NAME` inside the body.

```capy
catch unknown
    description "Help users who typo"
    do
        print "Unknown command: ${unknown}"
        print "Try one of:"
        list_commands
        exit 1
    end
end
```

**Passthrough pattern** — extend an existing tool with team conventions. Requires the `proxy_args` modifier to opt in to receiving the full unknown invocation; without it, `${proxy_args}` is unbound and referencing it errors (prevents the "any unknown verb silently forwards to shell" footgun):

```capy
catch passthrough
    description "Forward unknown commands to real git"
    proxy_args                        # ← required to bind ${proxy_args}
    do
        shell "git ${proxy_args}"
    end
end
```

Without the `proxy_args` modifier, `${proxy_args}` is unbound; a catch that doesn't declare it but references `${proxy_args}` halts with `unresolved_var`. Aligns catch with commands (where the `proxy_args` modifier was already required).

With that catch in place, `./mywrapper status` calls `git status`, `./mywrapper log --oneline -10` calls `git log --oneline -10`, and any custom commands you declare above the catch still take precedence over the underlying tool.

## Templates — parse-time stamps

A `template NAME … end` block is a **parse-time stamp** with the same `arg NAME … end` block syntax as `command`. Every `call NAME args…` site is expanded inline before the program ever reaches the interpreter, with positional args substituted as `${argname}` bindings in the spliced body.

```capy
template check_bin
    description "Fail unless the named binary is on PATH"
    arg name
        type string
    end
    do
        if not_exists "${name}"
            fail "${name} is required but not installed"
        end
    end
end

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
        check_bin "docker"
        check_bin "kubectl"
        install_pkg "jq"
        install_pkg "ripgrep" "13.0"
    end
end
```

**A template is a command that expands at parse time instead of running at execution time.** Same arg-block syntax, same call by positional arguments, same `--check` validation. The only difference is *when* the body's ops materialize — at parse time, inline at every call site (template), or at run time, when the command is invoked (command).

**Guardrails the validator enforces:**

- No recursion. A template cannot call itself (directly or via another template).
- Templates may only emit ops, never declarations. No `command`, `import`, or top-level bindings inside a template.
- Templates do not appear in `--help`, are not callable from the CLI, and do not show up in MCP.
- Positional args only. Optional / default values are honored from the arg-block spec.

**When to use a template vs. an execution context** — see [the section below](#execution-contexts-block-ops-that-wrap-a-body): templates eliminate *repetition*; execution contexts wrap a body to change *how* it runs. They do different jobs. Don't conflate them.

## Execution contexts (block ops that wrap a body)

Six block-shaped ops modify *how* the inner body executes without changing *what* it can express. They compose by nesting and read top-to-bottom.

### `parallel`

```capy
parallel
    build_darwin
    build_linux
    build_windows
end
```

Each direct child of `parallel` runs in its own goroutine; the block exits when ALL goroutines have completed. The first error becomes the block's error; siblings finish regardless. Each branch sees its own `Bindings` copy — `let X = …` captures inside parallel are local to the branch and do not survive the block.

### `timeout`

```capy
timeout "30s"
    exec kubectl apply -f manifest.yaml
end
```

Caps wall-clock for the body. A long-running op can't be interrupted mid-call; the *next* op after the deadline trips returns `ErrTimeout`. The interpreter's outer `--max-runtime` is the upper bound that any inner `timeout` block can only narrow.

### `retry`

```capy
retry 3
    exec curl -fsSL https://flaky.example.com/
end
```

Runs the body up to N times. On non-nil error, sleeps with exponential backoff (base 1s, capped at 5m) and retries. Default attempts is 3 when not specified. Never retries past the outer command's deadline.

### `with_env`

```capy
with_env "GOOS=linux,CGO_ENABLED=0"
    exec go build ./cmd
end
```

Overlays per-block environment variables onto the bindings for the body, then restores prior values on exit. Comma-joined `KEY=value` pairs. More readable than the per-command `env` modifier when the override is scoped to a few ops.

The three env-management forms, by lifetime:

| Form | Lifetime |
|---|---|
| `with_env "K=v" ... end` | scoped — auto-restored when the block exits |
| `export NAME "value"` (alias `set_env`) | process — persists for the rest of the run |
| `unset NAME` (alias `unset_env`) | removes a var from the process + binding overlay |

### `with_cwd`

```capy
with_cwd "./subproject"
    exec npm install
    exec npm run build
end
```

Temporarily switches `cwd` for the body, restoring even on error. Unlike `cd` (which persists for the rest of the command), `with_cwd` is bracketed.

### `sandbox`

```capy
sandbox "no_shell,no_network"
    vendor.update_check
end
```

Narrows the active capability mask for the body. Available flags inside the string: `no_shell`, `no_subprocess`, `no_network`, `no_write`. **Intersection rule:** masks can only be narrowed, never widened — an inner block can't re-enable what an outer mask (or the CLI flags) blocked. Same Android-style trust model perch's process-level flags use, with finer granularity. Runtime enforcement is shipped today; full static enforcement walking the call graph at `--check` time is on the roadmap.

### `cache`

```capy
cache "build-${target}-${sha256_file('go.sum')}" "24h"
    exec go build -o bin/${target} ./cmd
    let size = file_size "bin/${target}"
end
```

User-keyed body cache. First arg = cache key. Second = TTL duration. On miss: runs the body and persists every `let X = …` binding produced. On hit within TTL: skips the body entirely and replays the captured bindings into scope. Stored at `~/.cache/perch/blocks/<sha256(key)>.json`.

**Honest framing:** perch does NOT hash the body's transitive inputs. The user picks the key, and the key is the contract. If a stale input is left out of the key, you get stale cache. This is intentional — perch lacks the hermeticity needed for content-addressed caching (see [ideas/05](https://github.com/luowensheng/perch/blob/main/ideas/05-build-system-direction.md)). The user-keyed model matches how every practical caching layer (GitHub Actions cache, Earthly `--cache-id`, etc.) actually works.

### `--report` — see what ran, in what order, for how long

When any of these contexts are in play, `--report` renders the execution as a tree. Each block produces a span containing its children; durations, errors, and template provenance are shown inline:

```sh
$ perch --report release
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

`--report=PATH` writes the tree to a file (`--report=-` for stdout). The audit NDJSON (`--audit FILE.ndjson`) remains the canonical machine-readable artifact; `--report` is the human-readable renderer derived from the same hook order.

## String literals

Three interchangeable delimiters: **`"..."`**, **`'...'`**, **`` `...` ``**. All three are *raw* — no backslash escapes are interpreted — and `${name}` interpolation is active in all three. Pick whichever delimiter doesn't appear in your content.

This matters for JSON, SQL, and shell-with-quotes — content that would otherwise require painful `\"` escape sequences:

```capy
# JSON — content has " but no '. Use single quotes.
let body = format '{"order_id":"${order_id}","amount":${amount},"reason":"${reason}"}'

# SQL with quoted literals — content has both " and '. Use backticks.
shell_output `psql -h db -c "SELECT * FROM users WHERE name='${name}'"`

# Plain text with no quotes. Any delimiter works; "..." is the convention.
print "hello ${user}"
```

What this is NOT: there are no `\n` / `\t` / `\"` escape sequences. A backslash before any character (including the delimiter) is just a literal backslash followed by that character. If you need a literal newline in a string, write a multi-line string in your source (a real newline byte). If you need to embed a quote, switch to a delimiter that doesn't appear in your content.

The one substitution `${name}` *is* processed — that's the only special syntax inside strings.

## Interpolation

`${NAME}` inside any string-valued op argument is resolved at runtime. Resolution order:

1. Command-local bindings: parsed arg values and `let` captures.
2. Top-level `NAME = value` bindings.
3. Per-command `env` declarations.
4. Host process environment (so `${HOME}`, `${USER}`, `${PATH}` just work).

Unknown names produce an error at op-run time. To pass a literal `${VAR}` through to a child process (e.g. a real shell variable inside a `shell` op), escape with a backslash: `\${VAR}`.

## Comments

`# ...` line comments parse and are ignored — both whole-line and trailing:

```perch
# Build the release artifacts
command build
    do
        exec go build -o ./bin/app ./cmd/app   # cross-platform, no shell
    end
end
```

## Reserved words

The DSL has *no reserved words*. `name`, `command`, `do`, `end`, `if`, `let`, etc. are just library-defined functions. You could rebind them by editing perch's [lib.capy](https://github.com/luowensheng/perch/blob/main/infra/capyloader/lib.capy) — and yes, that's the point of building on [capy](https://luowensheng.github.io/capy).

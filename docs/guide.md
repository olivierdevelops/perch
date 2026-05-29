# The complete perch guide

> **Everything you need to get started AND ship a serious project with perch.** One document, end-to-end. Read in 30–45 minutes; skim the index to jump.

---

## Contents

**Onboarding**
1. [TL;DR — first run in 60 seconds](#tldr-first-run-in-60-seconds)
2. [Install](#install)
3. [The mental model](#the-mental-model)

**Reference**

4. [Anatomy of a `.perch` file](#anatomy-of-a-perch-file)
5. [Commands — args, modifiers, defaults](#commands-args-modifiers-defaults)
6. [Ops — the language vocabulary](#ops-the-language-vocabulary)
7. [State and data flow](#state-and-data-flow)
8. [Block ops — `if`, `parallel`, `retry`, `timeout`, `with_env`, `with_cwd`, `cache`, `sandbox`, `for_each`](#block-ops)
9. [Error handling and recovery](#error-handling-and-recovery)
10. [Templates — code reuse](#templates-code-reuse)
11. [Imports — multi-file projects](#imports-multi-file-projects)
12. [The capability model — restrictions, allowlists, audit](#the-capability-model)
13. [Cross-platform patterns](#cross-platform-patterns)
14. [Bundles — ship one binary](#bundles-ship-one-binary)
15. [WASM modules — sandbox by construction](#wasm-modules-sandbox-by-construction)
16. [Testing — `perch test`](#testing-perch-test)
17. [Pre-flight — `--check`, `--scan`, `simulate`](#pre-flight-check-scan-simulate)
18. [Observability — `--trace`, `--audit`, `--report`](#observability-trace-audit-report)
19. [Debugging your perch programs](#debugging-your-perch-programs)
20. [The five surfaces — CLI, web UI, REPL, MCP, binary](#the-five-surfaces)

**Patterns**

21. [Patterns & idioms — the 18 things you'll reach for](#patterns-and-idioms)

**Walkthroughs (10 end-to-end examples)**

22. [Walkthrough 1 — Replace your Makefile](#walkthrough-1-replace-your-makefile)
23. [Walkthrough 2 — Ship a self-installing tool](#walkthrough-2-ship-a-self-installing-tool)
24. [Walkthrough 3 — Internal ops backend (web UI + MCP)](#walkthrough-3-internal-ops-backend-web-ui-mcp)
25. [Walkthrough 4 — Plugin host with WASM](#walkthrough-4-plugin-host-with-wasm)
26. [Walkthrough 5 — CI gate with `simulate`](#walkthrough-5-ci-gate-with-simulate)
27. [Walkthrough 6 — Database backup + restore tool](#walkthrough-6-database-backup-restore-tool)
28. [Walkthrough 7 — SSL certificate rotation](#walkthrough-7-ssl-certificate-rotation)
29. [Walkthrough 8 — Multi-environment deployer (dev / staging / prod)](#walkthrough-8-multi-environment-deployer)
30. [Walkthrough 9 — File-processing pipeline (ETL-style)](#walkthrough-9-file-processing-pipeline)
31. [Walkthrough 10 — Git workflow wrapper](#walkthrough-10-git-workflow-wrapper)

**Production**

32. [Distribution — getting your binary to users](#distribution-getting-your-binary-to-users)
33. [Production deployment — logs, monitoring, hardening](#production-deployment)
34. [Common pitfalls](#common-pitfalls)
35. [FAQ](#faq)
36. [Honest limits](#honest-limits)
37. [Quick reference card](#quick-reference-card)

---

## TL;DR — first run in 60 seconds

```sh
# 1. Install
go install github.com/luowensheng/perch@latest

# 2. Make a file
cat > commands.perch <<'EOF'
name "myapp"

command hello
    description "Greet someone"
    arg who
        type string
        default "world"
        description "Who to greet"
    end
    do
        print "hello ${who}"
    end
end
EOF

# 3. Run it
perch hello              # → hello world
perch hello -who=perch   # → hello perch
perch --help             # → lists every command
perch hello --help       # → per-command help (args, defaults, examples)
```

That's perch. Everything else in this doc is depth on top of these three commands.

---

## Install

| Platform | Command |
|---|---|
| **Go users (any OS)** | `go install github.com/luowensheng/perch@latest` |
| **macOS / Linux (binary)** | `curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh \| sh` |
| **Windows (PowerShell)** | `irm https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.ps1 \| iex` |
| **Homebrew (macOS)** | See repo for tap status |
| **Manual** | Download from the [releases page](https://github.com/luowensheng/perch/releases) |

**Optional companions (also installed via Go):**

```sh
go install github.com/luowensheng/perch/cmd/perch-mcp@latest  # MCP server for AI agents
go install github.com/luowensheng/perch/cmd/perch-lsp@latest  # LSP for editors
```

Or use the built-in installers:

```sh
perch --install-lsp        # installs perch-lsp
perch --install-vscode     # installs perch-lsp + the VS Code extension
```

---

## The mental model

> **A `.perch` file is a typed CLI representation of an operational toolset.** Once you have one, every other surface — web UI, REPL, MCP, binary — is a different rendering of the same declarations. The file is the abstraction; the surfaces are downstream.

Concretely:

- A `.perch` file declares **commands** (typed verbs with named args).
- Each command's body is a list of **ops** (`shell`, `mkdir`, `http_get`, `assert_eq`, …) — ~140 built-ins, identical on macOS / Linux / Windows.
- Capability declarations (`--no-shell`, `wasm_run`'s body declarations) say what the program is allowed to do.
- Auto-bound variables (`${os}`, `${home_dir}`, `${script_dir}`, ~30 of them) give cross-platform context without `if uname`.
- The whole thing compiles to a single Go binary you can `scp`.

A `.perch` file with N commands gives you, for free:

```
perch CMD [args]                # CLI for command CMD
perch --help                    # list of every command
perch CMD --help                # per-command typed help
perch --server                  # web UI with a form per command
perch --shell                   # REPL
perch-mcp -f file.perch         # MCP server exposing every command as a tool
perch --build -o myapp          # portable binary with the program embedded
```

Add a new command → all six surfaces get it. No code generation, no schemas to update, no integration glue.

---

## Anatomy of a `.perch` file

Every `.perch` file has the same shape. Most projects use 3–5 of these sections; the rest are optional.

```perch
# 1. Identity & metadata
name        "myapp"
about       "Build, test, ship myapp"
version     "1.2.0"

# 2. Globals — values shared by every command
globals
    BUILD_DIR = "./builds"
    GIT_OK    = true
end

# 3. Requires — what this file needs from the host (declared + enforced)
requires
    bin "go"
    bin "git"
end

# 4. Bundle — what gets embedded into the fat binary at `perch --build` time
bundle
    include "./modules"             # whole dir
    include "./policy.wasm" as policy_wasm   # one file, alias for wasm_run
end

# 5. Templates — reusable parameter-substitution stamps (optional)
template ensure_dir
    arg path
        type string
    end
    do
        if not exists "${path}"
            mkdir "${path}"
        end
    end
end

# 6. Commands — the typed verbs
command build
    description "Compile myapp"
    arg target
        type string
        default "${os}"
        description "Target OS"
    end
    do
        call ensure_dir "${BUILD_DIR}/${target}"
        exec go build -o ${BUILD_DIR}/${target}/myapp ./cmd/myapp
    end
end

# 7. Catch — runs when the user types an unknown command (optional)
catch unknown
    description "Forward unknown verbs to git, like `gh foo` would"
    proxy_args                     # required to bind ${proxy_args} in the catch body
    do
        shell "git ${proxy_args}"
    end
end
```

**Top-level sections, ranked by how often you'll need them:**

| Section | When you need it |
|---|---|
| `name`, `version` | Always. Identifies the program. |
| `command NAME ... end` | Always. The thing you're declaring. |
| `globals ... end` | Often. Shared paths / flags / defaults. |
| `about "..."` | Usually. One-sentence description for `--help`. |
| `requires ... end` | When you want the file to declare (and enforce) its external needs — bins (+ SHA-256 hash pins), env vars, hosts, filesystem read/write scopes, OS/arch. Every external op then verifies the manifest before it runs; `perch --check` proves feasibility without running. See [capability-gating.md](capability-gating.md). |
| `bundle ... end` | When you want a portable binary with files embedded. |
| `template NAME ... end` | When you have repeated body shapes. |
| `import "PATH"` | Multi-file projects. |
| `catch NAME ... end` | When you want a fallback / proxy verb. |

---

## Commands — args, modifiers, defaults

A command is the unit of execution. Every command has:

- A **name** (the CLI verb)
- A **description** (shown in `--help`)
- Zero or more **args** (typed CLI flags or positional)
- Optional **modifiers** (`private`, `detached`, `test`, etc.)
- A **`do ... end` body** (the ops that run)

### Arg block — full shape

```perch
arg NAME
    type string                # string | int | float | bool
    default "fallback"          # used when the user omits the flag
    description "Human help"   # shown in --help
    index 0                     # positional: 0 = first positional arg
    optional                    # may be empty without a default
    rest                        # absorb every remaining positional arg
end
```

| Field | Required | Effect |
|---|---|---|
| `type` | yes | One of `string`, `int`, `float`, `bool`. Determines CLI parsing + the value type when interpolated. |
| `default` | no | Value when the user doesn't pass the flag. Same type as `type`. |
| `description` | no (but helpful) | Shown in `perch CMD --help` and the web UI's input placeholder. |
| `index N` | no | Bind to positional argument N (0-indexed). Without `index`, the arg is a flag (`-name=value`). |
| `optional` | no | Permit the arg to be absent even without a default. The body sees the empty value `""` / `0` / `false`. |
| `rest` | no | Absorb every remaining positional arg. Must be the LAST declared arg, `type string`, no default. The value is newline-joined; a sibling `${NAME_count}` int binding gives the count. |

### Modifiers (between `command NAME` and `do`)

```perch
command stop_serve
    description "Stop the dev server"
    private                              # hidden from --help / MCP / suggestions
    detached                             # spawned processes don't block the command
    proxy_args                           # bind ${proxy_args} = full argv after the verb
    require_os "darwin" "linux"          # refuse to run on other OSes
    require_arch "arm64" "amd64"         # refuse to run on other archs
    dir "./subproject"                   # cwd for the body
    on_signal handler_name               # run HANDLER on SIGINT/SIGTERM
    env DEPLOY_TARGET "staging"          # per-command env var
    test                                 # marks this as a `perch test` test
    test_allow_shell                     # opt out of test-mode shell denial
    test_allow_network                   # opt out of test-mode network denial
    test_keep_cwd                        # opt out of the test-mode temp-cwd
    test_timeout 30                      # max seconds for `perch test`
    do
        exec pkill -f cmd/server
    end
end
```

### Calling commands

```sh
# Flag form (any arg)
perch build -target=linux

# Positional form (args declared with `index`)
perch deploy us-east-1                  # if -region has index 0
perch deploy us-east-1 -bake=15

# Per-command help (rendered from the arg specs)
perch build --help

# Cross-command call (inside a body)
run other_command "-arg1=x"  "-arg2=y"
```

---

## Ops — the language vocabulary

There are ~140 built-in ops, every one of them cross-platform. Catalog by category:

| Category | Examples |
|---|---|
| **Output** | `print`, `println`, `eprintln` |
| **Shell** | `shell`, `shell_output`, `try_shell`, `shell_detached`, `pkg_install` |
| **Files** | `mkdir`, `cp`, `mv`, `rm`, `touch`, `chmod`, `exists`, `read_file`, `write_file`, `append_file`, `file_size`, `file_mtime`, `list_dir`, `walk_dir`, `mktemp`, `mktemp_dir`, `ensure_dir`, `symlink`, `copy_dir` |
| **Strings** | `trim`, `lower`, `upper`, `replace`, `split`, `join`, `contains`, `has_prefix`, `has_suffix`, `slice`, `capitalize`, `length`, `format`, `pad_left`, `pad_right`, `repeat` |
| **Hash** | `md5`, `sha1`, `sha256`, `md5_file`, `sha256_file`, `crc32`, `adler32`, `fnv32` |
| **Encoding** | `base64_encode/decode`, `hex_encode/decode`, `url_encode/decode`, `json_parse`, `json_stringify`, `json_get`, `csv_parse`, `xml_parse` |
| **HTTP** | `http_get`, `http_post`, `http_put`, `http_delete`, `http_status`, `download` |
| **Network** | `dns_lookup`, `port_check`, `get_ip`, `get_hostname` |
| **Compression** | `gzip`, `ungzip`, `tar_create`, `tar_extract`, `zip_create`, `zip_extract` |
| **Time** | `now`, `format_time`, `parse_time`, `unix_to_iso`, `iso_to_unix`, `time_diff`, `sleep` |
| **Regex** | `regex_match`, `regex_replace`, `regex_find_all` |
| **System** | `get_os`, `get_arch`, `get_env`, `set_env`, `app_data_dir`, `cache_dir`, `temp_dir`, `home_dir`, `cwd`, `pid`, `hostname`, `user`, `has_bin`, `bin_version`, `os_version` |
| **Bundle** | `bundle_hash`, `bundle_dir`, `bundle_extract`, `link_into_path` |
| **Assert (for tests)** | `assert_eq`, `assert_neq`, `assert_contains`, `assert_not_contains`, `assert_exists`, `assert_not_exists`, `assert_match` |
| **Block ops** | `if`, `parallel`, `retry`, `timeout`, `with_env`, `with_cwd`, `sandbox`, `cache`, `for_each`, `wasm_run` |

Full reference: [op-reference.md](op-reference.md).

### Op shape

Most ops are one of:

```perch
print "hi"                               # bare op with positional string
mkdir "${BUILD_DIR}/${target}"          # ditto
let n = file_size "./build/out"         # let-capture: result bound to ${n}
let body = http_get "https://api.x/health"
print "got ${body}"
```

Some block ops take a named arg + a body:

```perch
parallel max=3                           # block op with kwarg
    exec go test ./a
    exec go test ./b
    exec go test ./c
end
```

### Interpolation — `${NAME}`

Every string arg can reference variables:

```perch
print "host=${hostname} cwd=${cwd}"
```

Sources, in resolution order:

1. **`let X = ...` captures** in the current body
2. **Command args** (declared via `arg NAME ...`)
3. **Globals** (declared in `globals ... end`)
4. **Auto-bound vars** (always present: `${os}`, `${arch}`, `${home_dir}`, `${script_dir}`, `${cwd}`, etc.)
5. **Host env vars** (subject to `--env` allowlist if set)

Unknown names cause an error at op-dispatch time (and a warning at `perch --check` time).

### Auto-bound variables

These ~30 names are always available — no `let`, no `if uname`:

| Group | Examples |
|---|---|
| **OS / arch** | `${os}` (`darwin`/`linux`/`windows`), `${arch}` (`amd64`/`arm64`), `${is_macos}`, `${is_linux}`, `${is_windows}`, `${is_unix}`, `${is_arm64}`, `${is_amd64}` |
| **Paths** | `${script_dir}`, `${script_path}`, `${cwd}`, `${home_dir}`, `${temp_dir}`, `${cache_dir}`, `${app_data_dir}` |
| **Identity** | `${user}`, `${hostname}`, `${pid}` |
| **Bundle** | `${bundle_dir}` (auto-extract on first reference; falls back to `${script_dir}` when not running from a built binary) |

---

## State and data flow

perch is **stringly-typed by design.** Every value an op produces is a string; types are recovered at op boundaries (e.g. `if size > 1000` parses the string as int). This keeps interpolation uniform: `${anything}` always works.

### Passing data between ops

The four mechanisms, ranked by how often you'll use them:

```perch
# 1. `let X = OP …` — capture an op's return value into a local binding
let rev = exec git rev-parse HEAD
print "deploying ${rev}"

# 2. Globals — read-only after parse, visible everywhere
globals
    APP_DIR = "/opt/myapp"
end

# 3. Args — values from the caller (CLI flag, MCP arg, web UI form)
arg target type string default "darwin" end

# 4. Files — write at one step, read at another (persists across commands)
write_file "${temp_dir}/state.json" "${rev}"
let prior = read_file "${temp_dir}/state.json"
```

**Things `let` does NOT do:**

- It doesn't leak across commands. Each command's body has its own binding scope. To pass state between commands, write to a file or use `cache`.
- It doesn't survive across `parallel` siblings. Each `parallel` child runs in its own clone; capturing `let X = ...` inside one branch doesn't make `${X}` visible in another.
- It doesn't survive `run other_command`. The callee runs with a fresh binding table.

### Working with JSON

The most common shape — fetch a JSON API, extract a field:

```perch
command status
    do
        let body = http_get "https://api.example.com/status"
        let value = json_get "${body}" ".version"
        let svc = json_get "${body}" ".service.name"
        print "${svc} is at version ${value}"
    end
end
```

`json_get` uses dot-path syntax: `.foo.bar.baz`, `.items.0.name`, `.tags` (the whole list, comma-joined). For more complex shapes, pipe through a WASM module that uses your language's real JSON library.

### Working with shell output

```perch
let lines = exec find . -name *.go -type f
let count = length "${lines}"      # newline-counting
print "${count} files"
```

`shell_output` captures stdout (stderr still goes to your screen). For exit-code-sensitive flows, use `try_shell` which doesn't fail the command on non-zero:

```perch
let result = try_shell "ping -c 1 ${host}"
if "${result}" == ""
    print "host ${host} unreachable"
end
```

### File-backed state (the only durable kind)

For state that needs to survive across `perch` invocations:

```perch
let state_file = format "${cache_dir}/myapp/lastrun"

command deploy
    do
        let now = now
        write_file "${state_file}" "${now}"
        # ... do deploy ...
    end
end

command since_lastrun
    do
        if not exists "${state_file}"
            print "never deployed"
            exit 0
        end
        let then = read_file "${state_file}"
        let diff = time_diff "${then}" "${now}"
        print "last deploy was ${diff} ago"
    end
end
```

This pattern is enough for: deployment markers, idempotency tokens, cache invalidation keys, "are we set up?" flags.

---

## Block ops

### `if EXPR ... end` — unified conditional

```perch
if os == "linux"
    exec apt-get install -y jq
end

if has_bin "kubectl"
    exec kubectl get pods
end

if not has_bin "docker"
    fail "docker is required"
end

if exists "./Cargo.toml"
    exec cargo build --release
end

if size > 1000000
    print "file is large"
end
```

Forms supported:

- **Comparisons**: `NAME == "x"`, `NAME != "x"`, `NAME > N`, `NAME < N`
- **Truthy / falsy**: `if has_bin`, `if not has_bin`
- **Predicate calls**: `if exists "PATH"`, `if has_bin "NAME"`, `if not exists "PATH"`

### `parallel max=N ... end`

Run children concurrently up to N at a time:

```perch
parallel max=4
    exec go test ./a
    exec go test ./b
    exec go test ./c
    exec go test ./d
end
```

### `retry max=N delay=Ns ... end`

Re-run children on failure with exponential backoff:

```perch
retry max=3 delay=2s
    let body = http_get "https://api.example.com/health"
end
```

### `timeout secs=N ... end`

Kill children if they exceed N seconds:

```perch
timeout secs=300
    exec make integration-test
end
```

### `with_env KEY1=val1,KEY2=val2 ... end`

Overlay env vars for the body:

```perch
with_env DEBUG=1,GOOS=linux
    exec go build ./cmd/myapp
end
```

### `with_cwd "PATH" ... end`

Run body in another directory:

```perch
with_cwd "./subproject"
    exec go test ./...
end
```

### `sandbox flags="no_shell,no_network" ... end`

Tighten capabilities for the body:

```perch
sandbox flags="no_shell,no_network"
    let body = read_file "./input.json"
    let parsed = json_parse "${body}"
    write_file "./output.json" "${parsed}"
end
```

### `cache key="KEY" ttl="1h" ... end`

Skip the body if a recent identical run cached its `let` bindings:

```perch
cache key="${file_hash}" ttl="24h"
    let cost = exec ./expensive-script.sh
end
```

### `for_each NAME in LIST ... end`

Loop body N times:

```perch
for_each region in "us-east-1,us-west-2,eu-west-1"
    exec deploy --region=${region}
end
```

### `wasm_run` — WebAssembly under WASI

See [WASM modules](#wasm-modules-sandbox-by-construction) below.

---

## Error handling and recovery

By default, **any op that errors halts the command** and the process exits non-zero. This is the right default for CI gates and deploy scripts. The ladder of "I want different behavior":

### General error handling — `try / rescue / finally` + `match`

> **Status:** the `match` op (including bare `match err.kind`), the error-kind enum, **and** the `try / rescue / finally` block all work today — the block is built on capy `block_sections`. A `finally`-only `try` re-raises after cleanup; only a non-empty `rescue` arm swallows the error.

```perch
try
    let body = http_get "${url}"
rescue err
    match "${err.kind}"
        case http_5xx
            throw "${err.message}"     # let an outer retry handle it
        case http_4xx
            print "bad request: ${err.code}"
        case http_ssrf_blocked
            run alert "-msg=security: ${err.detail}"
        else
            throw "${err.message}"     # unknown — re-raise
    end
finally
    rm "${tmpfile}"
end
```

Inside `rescue err`, five bindings are populated: `${err.kind}` (enum), `${err.message}`, `${err.code}`, `${err.op}`, `${err.detail}`. The full enum (30 kinds — `shell_exit_nonzero`, `http_5xx`, `http_ssrf_blocked`, `wasm_module_exited`, `file_not_found`, …) lives in **[docs/errors.md](errors.md)**, which is also the reference for every composition rule and pattern.

`finally` runs **unconditionally** — both on success and failure. Errors in `finally` override the original (so cleanup failures aren't silently swallowed).

Use `throw "msg"` inside `rescue` to re-raise — semantically the same as `fail "msg"` but spelled to read clearly as "I caught this and decided to re-raise."

> **Why `rescue` instead of `catch`?** perch already uses `catch unknown ... end` at file scope for declaring catch-all CLI commands. That's a different concept from exception handling. To keep both clear, perch borrows Ruby's `try / rescue / finally` naming.

### Recoverable shell calls — `try_shell`

```perch
let result = try_shell "ping -c 1 ${host}"
if "${result}" == ""
    print "${host} unreachable, falling back"
end
```

`try_shell` captures stdout AND swallows non-zero exit codes. Use when you want to **branch on success/failure** rather than abort.

### Retry with backoff — `retry` block

```perch
retry max=5 delay=2s
    let body = http_get "https://api.example.com/health"
    assert_contains "${body}" "ok"
end
```

Exponential backoff (2s, 4s, 8s, 16s, 32s in this example). The whole body re-runs; partial side effects persist. For idempotent ops only.

### Explicit failure — `fail`

```perch
if not has_bin "kubectl"
    fail "kubectl required — install from https://kubernetes.io/docs/tasks/tools/"
end
```

Halt immediately with a readable message. The message goes to stderr; exit code is 1.

### Cleanup with `on_signal`

For long-running commands (servers, watch loops) that need to clean up on Ctrl-C:

```perch
command serve
    description "Dev server with auto-cleanup"
    detached
    on_signal stop_serve
    do
        exec go run ./cmd/server
    end
end

command stop_serve
    private
    do
        exec pkill -f cmd/server
        rm "${temp_dir}/server.pid"
    end
end
```

When `serve` receives SIGINT/SIGTERM, perch dispatches to `stop_serve` before exiting.

### Detect-and-degrade pattern

```perch
command full_setup
    do
        if has_bin "brew"
            exec brew install jq
        end
        if not has_bin "brew"
            if has_bin "apt-get"
                exec sudo apt-get install -y jq
            end
        end
        if not has_bin "jq"
            fail "couldn't install jq via brew or apt-get — install manually"
        end
    end
end
```

Three escalating attempts; only fails if none worked. Useful for cross-platform tool installation.

### Cleanup-after-error idiom

perch doesn't have try/finally. Use a wrapper command that always runs the cleanup, even after fail:

```perch
command release
    do
        run _release_inner
        run _release_cleanup       # always runs (release halts → cleanup doesn't)
    end
end

# Better pattern: do cleanup OUTSIDE the failing region.
command release_with_cleanup
    do
        let tmp = mktemp_dir
        # Set up state...
        exec build-and-test ${tmp}      # might fail
        # If we get here, build succeeded:
        run _release_publish "-target_dir=${tmp}"
        rm "${tmp}"
    end
end
```

For truly defensive cleanup (delete the temp dir even on build failure), wrap in a `with_cwd` block or move the cleanup into a wrapper command and use a separate `--report` audit log to detect failures post-hoc.

### Exit codes

- **0** — success
- **1** — `fail` op, or any op error
- **non-zero from shell** — propagates by default; `try_shell` to suppress
- **128 + N** — caught signal N (rare; perch usually shuts down cleanly via `on_signal`)

---

## Templates — code reuse

When the same body shape appears in two commands, lift it into a template. Templates are **parse-time stamps** — they expand inline at every `call NAME ...` site.

```perch
template require_bin
    arg name string
    do
        if not has_bin "${name}"
            fail "${name} is required — install it first"
        end
    end
end

command up
    description "Start the dev stack"
    do
        call require_bin "docker"
        call require_bin "docker-compose"
        exec docker-compose up -d
    end
end

command down
    do
        call require_bin "docker-compose"
        exec docker-compose down
    end
end
```

Templates can't recurse, can't declare commands, and don't appear in `--help` / MCP.

---

## Imports — multi-file projects

Big projects span multiple `.perch` files. Imports merge:

```perch
# main.perch
import "recipes/_lib.perch"
import "recipes/redis.perch" as redis    # namespaced; verbs become `redis.up`, etc.

command full_stack
    do
        call require_docker
        run redis.up
        exec ./scripts/seed-data.sh
    end
end
```

- **Flat import** (`import "PATH"`) — imported commands + templates merge into the host's namespace. Last definition wins.
- **Namespaced import** (`import "PATH" as NAME`) — imported verbs become `NAME.verb`. Useful for libraries you want to keep distinct.

Imports are resolved at parse time relative to the importing file's directory.

---

## The capability model

perch programs declare what they need; restrict at run time what they can do. Two layers:

### 1. CLI restriction flags (outer policy)

```sh
perch --no-shell                 # refuse every shell call
perch --no-network               # refuse every HTTP / network call
perch --no-subprocess            # refuse spawn (pkg_install, kill_by_name, etc.)
perch --no-write                 # filesystem is read-only

perch --env HOME,PATH            # only listed env vars resolve via ${NAME}
perch --allow-bin docker,git     # only listed binaries can be the first token of `shell`
perch --allow-host api.gh.com    # network allowlist (composes with default SSRF guard)
perch --no-shell-metachars       # reject shell commands with pipes / && / `;` etc.
perch --max-runtime 600          # wall-clock cap, in seconds
perch --no-redirects             # HTTP redirects refused
perch --allow-private-ips        # opt out of the default SSRF guard
```

Compose freely:

```sh
perch --no-shell --no-network --env HOME,PATH --max-runtime 300 deploy
```

### 2. In-language `sandbox` blocks (inner policy)

```perch
command parse_user_input
    do
        sandbox flags="no_shell,no_network,no_subprocess"
            let raw = read_file "./untrusted.json"
            let parsed = json_parse "${raw}"
            write_file "./safe.json" "${parsed}"
        end
    end
end
```

Inner sandboxes can ONLY tighten. They can't grant capability the outer policy denied.

### Always-on defaults

- **SSRF guard** — every HTTP call refuses private / loopback / link-local IPs unless `--allow-private-ips`. Defense includes redirect-hop checks and DNS-rebinding (multi-A) handling.
- **Scheme downgrade guard** — `https → http` redirects refused unless `--allow-scheme-downgrade`.
- **`private` commands** — never callable from the CLI (only via `run`); excluded from `--help` and MCP.

### Audit log

```sh
perch --audit /var/log/perch.ndjson deploy
```

Writes one NDJSON line per op (kind, args after interpolation, duration, result/error). Useful for: forensics, debugging, feeding into observability pipelines.

---

## Cross-platform patterns

perch's ~140 built-in ops behave identically on macOS / Linux / Windows. The places you still need OS-aware logic are:

### Path separators

perch's path-shaped ops normalize separators internally — `mkdir "./a/b/c"` works on Windows. But if you're passing paths into `shell "..."`, the shell sees what you typed:

```perch
# WRONG on Windows: cmd.exe doesn't understand forward slashes for cd
shell "cd ./subproject && go test ./..."

# RIGHT: use with_cwd (cross-platform)
with_cwd "./subproject"
    exec go test ./...
end

# OR use the platform-correct sep
let sep = "/"
if os == "windows"
    let sep = "\\"
end
```

Most of the time `with_cwd` is the cleaner answer.

### Line endings

`read_file` returns the file's content as-is. If you're reading on Windows and got CRLF, downstream ops may produce different output than on Unix. Normalise explicitly when comparing:

```perch
let raw = read_file "./config.txt"
let normalized = replace "${raw}" "\r\n" "\n"
```

### Shell vs OS — use `os "PLATFORM" ... end` for explicit OS context

The first token of `shell "X"` is the binary you're invoking. Different OSes have different binaries. **Use the `os "PLATFORM" ... end` block** to declare which body is meant for which OS — explicit beats hidden:

```perch
command setup
    do
        os "darwin"
            exec brew install jq
        end
        os "linux"
            exec apt-get install -y jq
        end
        os "windows"
            exec choco install jq -y
        end
    end
end
```

### Version checks

`shell_output` + `version_extract` + `version_*` comparators give you typed version gating without a per-binary parser table to maintain. The pattern:

```perch
command deploy
    do
        let raw = exec kubectl version --client -o json
        let v   = version_extract "${raw}"
        let ok  = version_ge "${v}" "1.28.0"
        if not ok
            fail "kubectl ${v} < 1.28.0 — upgrade and retry"
        end
        exec kubectl rollout restart deployment/api
    end
end
```

**`version_extract STRING [PATTERN]`** pulls a version out of arbitrary text. With no PATTERN it uses a default heuristic (`v?\d+(\.\d+)+(...optional pre-release tail...)`) that matches most CLI version output. Supply a pattern with one capture group for unusual formats:

```perch
let v = version_extract "${raw}" `"gitVersion":"v(\d+\.\d+\.\d+)`
```

**Comparators** return `"true"` / `"false"`:

| Op | Semantics |
|---|---|
| `version_eq A B` | A == B |
| `version_ne A B` | A != B |
| `version_gt A B`, `version_ge A B` | strict / inclusive greater |
| `version_lt A B`, `version_le A B` | strict / inclusive less |
| `version_compat A B` | same major version (`~=`-style compatibility) |

### Infix assertion (recommended)

For halt-on-failure version gates, **`assert_version "X" OP "Y"`** reads like the math:

```perch
let raw = exec kubectl version --client -o json
let v   = version_extract "${raw}"

assert_version "${v}" >= "1.28.0"   # halt with assert_failed if too old
assert_version "${v}" <  "2.0.0"    # halt if too new
assert_version "${v}" ~  "1.0.0"    # halt unless same major (PEP 440 ~= / Cargo caret)
```

Supported operators: `>=`, `>`, `<=`, `<`, `==`, `!=`, `~` (same-major). Halts with `err.kind = assert_failed`, composes with `try / rescue`:

```perch
try
    assert_version "${v}" >= "1.28.0"
rescue err
    match "${err.kind}"
        case assert_failed
            print "kubectl too old — using legacy path"
            run deploy_legacy
        else
            throw "${err.message}"
    end
end
```

### Inside an `if`

The standard `if X OP Y ... end` block does **semver-aware comparison** when both sides look like version strings (optional `v` prefix, dot-separated digit segments). No special keyword required:

```perch
let v = version_extract "${raw}"

assert_version v >= "1.28.0"   # hard gate
if v >= "1.28.0"                # soft branch
    exec kubectl rollout restart deployment/api
end
```

Crucially `1.29.3 > 1.9.0` is now true (semver order), where float comparison would say false (`1.29 < 1.9` numerically). Plain numeric comparisons (file sizes, counts, etc.) still use float — the auto-detection only flips when both operands look like versions.

For programmatic boolean checks where you want to capture the result, the prefix `version_ge` / `version_gt` / etc. ops also work and return `"true"` / `"false"` strings.

**Ordering rules** — best-effort, no library dependency:

- Optional `v` prefix is stripped on both sides
- Numeric segments compared element-wise (`1.10.0 > 1.9.0` — numeric, not lex)
- Missing segments treated as zero (`1.2 == 1.2.0`)
- Pre-release suffix (`-rc.1`) sorts BEFORE the unsuffixed version (semver spec)
- Build metadata after `+` is ignored
- Falls back to string compare if either side is unparseable (so `if ok ... end` still reaches a decision)

**`arch "ARCH" ... end`** is the architecture sibling — runs the body only when `${arch}` matches. Standard targets: `"amd64"`, `"arm64"`, `"386"`, `"arm"`, `"riscv64"` (Go GOARCH values). Compose with `os` for matrix builds:

```perch
command release
    do
        os "linux"
            arch "amd64"
                with_env "GOOS=linux,GOARCH=amd64"
                    exec go build -o app-linux-x64 ./cmd/app
                end
            end
            arch "arm64"
                with_env "GOOS=linux,GOARCH=arm64"
                    exec go build -o app-linux-arm64 ./cmd/app
                end
            end
        end
        os "darwin"
            arch "amd64"
                with_env "GOOS=darwin,GOARCH=amd64"
                    exec go build -o app-darwin-x64 ./cmd/app
                end
            end
            arch "arm64"
                with_env "GOOS=darwin,GOARCH=arm64"
                    exec go build -o app-darwin-arm64 ./cmd/app
                end
            end
        end
    end
end
```

`perch simulate release --sim-os=linux --sim-arch=arm64` prunes everything except the matching leaf. No umbrellas for arch (matrix builds want exact pinning).

**`os "unix"`** is an umbrella that matches darwin / linux / freebsd / openbsd / netbsd — handy for "any Unix; Windows needs its own":

```perch
command rm_build
    do
        os "unix"
            exec rm -rf ./build
        end
        os "windows"
            exec rmdir /S /Q .\\build
        end
    end
end
```

Supported targets: `"darwin"`, `"linux"`, `"windows"`, `"freebsd"`, `"openbsd"`, `"netbsd"`, and the umbrella `"unix"`. Mirror of the existing `${is_unix}` auto-bound var (true on anything that isn't Windows).

Why prefer `os "X"` over `if os == "X"`?

- **Same runtime semantics** (body runs only when `${os}` matches the declared platform).
- **Stronger static analysis.** `simulate --sim-os=linux` knows the `os "darwin"` branch is dead code; the **web UI** can flag "incompatible with your current OS"; **`--scan`** can give per-OS capability summaries.
- **Reads cleaner.** "This body is for Linux" is the structural intent; an `if` reads as runtime branching.

If you genuinely want runtime branching (one body that needs to react to `${os}` mid-flight), `if os == "X"` is still the right tool. For declaring "this command's body is OS-specific work," `os "X"` is the new shape.

Older cross-platform style still works (no behavior change):

```perch
if is_macos
    exec brew install jq
end
```

For common cases, factor into a template:

```perch
template install_pkg
    arg name string
    do
        if is_macos
            exec brew install ${name}
        end
        if is_linux
            exec sudo apt-get install -y ${name}
        end
        if is_windows
            exec choco install ${name} -y
        end
    end
end

command setup
    do
        call install_pkg "jq"
        call install_pkg "ripgrep"
    end
end
```

### Environment quirks

| Quirk | Solution |
|---|---|
| Windows env vars are case-insensitive | perch normalizes to uppercase internally |
| `${HOME}` doesn't exist on Windows | Use `${home_dir}` (auto-bound, cross-platform) |
| `${PATH}` separator is `;` on Windows, `:` elsewhere | `link_into_path` op handles this; for manual PATH manipulation, branch on `${os}` |
| Executable extension on Windows is `.exe` | `${exe_ext}` is auto-bound ("`.exe`" on Windows, "" elsewhere) |
| Default shell differs (sh vs cmd) | If you need bash specifically: `shell "bash -c '...'"` and require_bin bash up front |

### What auto-bound vars give you

You almost never need to write OS-detection code from scratch. The big six:

```perch
${os}            # darwin | linux | windows
${arch}          # amd64 | arm64
${is_macos}      # "true" | "false"
${is_linux}      # "true" | "false"
${is_windows}    # "true" | "false"
${is_unix}       # "true" on macOS+Linux, "false" on Windows
```

Use them in `if` conditions:

```perch
if is_unix
    exec chmod +x ${OUT_DIR}/myapp
end
```

---

## Bundles — ship one binary

`perch --build` produces a single self-contained executable. The `.perch` file gets compiled into Go, embedded into the runtime; an optional `bundle ... end` section embeds arbitrary files alongside.

```perch
name "myapp"
version "1.0.0"

bundle
    include "./modules"             # whole directory
    include "./policy.wasm" as policy_wasm    # one file, with alias
    include "./templates/email.txt"
end

command run_plugin
    do
        wasm_run policy_wasm        # bare ident — resolved via bundle alias
            wasm_arg "/ro/deploy"
        end
    end
end
```

Build:

```sh
perch --build -f myapp.perch -o myapp
# → ./myapp is a single executable with .wasm + templates inside
```

Distribute:

```sh
scp myapp ops@host:/usr/local/bin/
ssh ops@host 'myapp run_plugin'      # zero install, zero disk reads
```

**Bundle behavior:**

- Paths in `include` resolve relative to the `.perch` file's directory.
- `include "PATH" as NAME` registers `NAME` as a bare-identifier alias usable by `wasm_run NAME` and friends.
- CLI `--include PATH` at `--build` time is **additive** (use for CI steps injecting generated files).
- Recipients of the binary need **only** what your install commands themselves require — no Go, no perch, no python (unless YOU shell out to python).

Worked example: [demos/05-python-installer](https://github.com/luowensheng/perch/tree/main/demos/05-python-installer) — a single binary that drops a Python project into `~/.cache/perch/<hash>/`, sets up a venv, and links into `$PATH`.

---

## WASM modules — sandbox by construction

`shell` is best-effort isolation (a sandbox is a fence around an open field). `wasm_run` is **strict isolation by construction**: the module sees ONLY the argv, env vars, mounts, and hosts you declared. Anything else **does not exist** in its execution environment.

```perch
command validate_manifest
    do
        wasm_run "./validate.wasm"
            wasm_arg "/ro/manifest"             # argv passed to module
            wasm_env "GREETING,HOME"            # only these env vars
            wasm_mount_read "./manifest.yaml"   # read-only mount → /ro/manifest.yaml
            wasm_mount_write "./out"            # read-write     → /rw/out
            wasm_allow_host "api.github.com"    # HTTP allowlist (for the host-provided perch.http_get import)
        end
    end
end
```

Build a module (any language targeting `wasm32-wasip1` works):

```sh
# Go
GOOS=wasip1 GOARCH=wasm go build -o validate.wasm .

# Rust
cargo build --target wasm32-wasip1 --release

# TinyGo
tinygo build -o validate.wasm -target=wasi .
```

### Capability declarations

| Declaration | Effect |
|---|---|
| `wasm_arg "VALUE"` | Append to argv. Multiple OK. |
| `wasm_env "K1,K2"` | Env allowlist. Only listed names pass through. |
| `wasm_mount_read "PATH"` | Mount host PATH at `/ro/<basename>` (read-only). |
| `wasm_mount_write "PATH"` | Same but read-write at `/rw/<basename>`. |
| `wasm_allow_host "HOST"` | Per-module HTTP allowlist (needed for `perch.http_get` host import). |

### HTTP from inside a module

WASI Preview 1 has no sockets. perch exposes HTTP via host imports under the module name `perch`:

```go
// fetch.go — compiled with GOOS=wasip1 GOARCH=wasm
import "github.com/luowensheng/perch/wasm-sdk/perchhttp"

body, status, err := perchhttp.Get("https://api.github.com/zen")
```

```perch
command zen
    do
        wasm_run "./fetch.wasm"
            wasm_allow_host "api.github.com"
        end
    end
end
```

What's enforced:
- No `wasm_allow_host` declarations → every `http_get` returns -1 (fail-closed).
- Module dials a host outside its allowlist → refused at the host-function boundary.
- Outer `--allow-host` policy still applies (intersection wins).
- SSRF + redirect guards still run on every call AND every redirect hop.

**Honest limits:** only `GET` (no POST/PUT/DELETE yet), no custom headers, 32 MB response cap, no streaming. Roadmap: POST + headers next; sockets only if WASI Preview 2 stabilises in wazero.

The killer demo for `wasm_run`: [demos/wasm-plugin-host](https://github.com/luowensheng/perch/tree/main/demos/wasm-plugin-host) — a plugin runtime that runs 4 legitimate plugins + 1 deliberately malicious plugin trying 5 escape attempts; every escape fails because the runtime doesn't *provide* those operations.

---

## Testing — `perch test`

Mark a command with the `test` modifier and `perch test` discovers + runs it. Tests are sandboxed by default (`--no-shell`, `--no-network`, `--no-subprocess`, fresh temp cwd).

```perch
template suite_assert
    arg got string
    arg want string
    do
        assert_eq "${got}" "${want}"
    end
end

command test_lower
    description "lower op should match Unicode case folding"
    test                                # marks this as a test
    do
        let result = lower "HÉLLO"
        call suite_assert "${result}" "héllo"
    end
end

command test_http_get
    test
    test_allow_network                  # opt out of network denial
    do
        let body = http_get "https://api.github.com/zen"
        assert_contains "${body}" "."   # at least produced text
    end
end
```

Run:

```sh
perch test                          # all tests
perch test --filter lower           # name substring filter
perch test -v                       # verbose: show captured output even on pass
```

Exit code is non-zero if any test failed. Wire into pre-commit and CI.

Seven `assert_*` ops: `assert_eq`, `assert_neq`, `assert_contains`, `assert_not_contains`, `assert_exists`, `assert_not_exists`, `assert_match`. Each produces a readable failure message.

---

## Pre-flight — `--check`, `--scan`, `simulate`

Three tools that catch problems before execution. Wire any of them into pre-commit / CI.

### `--check` (validate)

Syntactic + reference validation:

```sh
perch --check -f commands.perch
```

Catches: unknown op kinds, missing `run TARGET` references, typo'd arg types, duplicate args, colliding positional indexes, unresolved `${name}` placeholders.

### `--scan` (capability audit + risk score)

Static analysis: what capabilities does this program need? What risks are there?

```sh
perch --scan -f deploy.perch
```

Output now leads with a **one-glance risk score**:

```
RISK: 🟢 SAFE   — pure ops only
RISK: 🟡 LOW    — limited surface; review the capabilities below
RISK: 🟠 MED    — multiple capabilities or shell metacharacters; review carefully
RISK: 🔴 HIGH   — sudo / proxy_args / privileged ops; read every command first
```

Plus the bullet list of WHY (`uses sudo`, `executes shell`, `network access (3 hosts)`, `writes the filesystem (2 roots)`, etc.). Then the full capability table, risk findings (with severity HIGH / MED / LOW / INFO), and a recommended hardened invocation.

**Always run before executing a `.perch` file you didn't write.** A 🔴 HIGH score isn't necessarily bad (production deploy scripts are usually HIGH) — it's the *signal that you should actually read the file before running it*.

The web UI's Scan tab surfaces the score as a colored pill; the JSON `/api/scan` endpoint returns `risk: {score: "HIGH", reasons: [...]}` for downstream tools.

### `simulate` (hypothetical-env analyzer)

Walk the program against a hypothetical host — without running anything. Reports per-op outcomes: **WILL_RUN ✓** / **WILL_FAIL ✗** / **MIGHT_FAIL ?**.

```sh
perch simulate deploy --sim-os=linux --sim-have-bin=kubectl,docker \
                      --sim-allow-host=api.example.com
```

For multi-scenario "what if X / what if Y" analysis, supply a JSON fixture:

```sh
perch simulate deploy --sim-file fixture.json
```

```json
{
  "os": "linux",
  "bins": ["docker", "kubectl"],
  "oracles": {
    "shell_output": { "git rev-parse HEAD": "abc123" },
    "http": { "https://api.example.com/health": {"status": 200} }
  },
  "scenarios": [
    {"name": "happy", "overrides": {}},
    {"name": "api-down", "overrides": {
      "http": {"https://api.example.com/health": {"status": 500}}
    }}
  ]
}
```

Each scenario runs as an independent walk with its own report. Drop into CI as a multi-environment gate. See [docs/simulate.md](simulate.md) for the full fixture format.

---

## Observability — `--trace`, `--audit`, `--report`

Same hook order, three audiences:

| Flag | Audience | Output |
|---|---|---|
| `--trace` | Human, live | Streams `▸ kind args…` to stderr as each op fires; indents block-op children. |
| `--audit FILE.ndjson` | Machines | One JSON line per op (kind, args, duration, result/error). Feeds into observability pipelines. |
| `--report` | Human, post-run | Renders the span tree after the run with full error context. |

All three derive from the same `Tracer` interface, so timing + nesting + ordering are consistent across them.

```sh
perch --trace deploy                              # stream live
perch --trace=trace.log deploy                    # → file
perch --audit /var/log/perch.ndjson deploy        # JSON for downstream
perch --report deploy                             # post-run tree
```

`--trace` and `--report` are mutually exclusive (they share a slot); `--audit` composes with either.

---

## Debugging your perch programs

Tools to figure out **why** a command did what it did:

### `--dry-run` — walk the plan, skip execution

```sh
perch --dry-run deploy -target=staging
```

Prints every op with its arguments **post-interpolation**, in the order it would execute, without running anything. Catches: wrong `${var}` resolution, surprise control-flow path, missing arg. The dry-run output is also legitimate documentation — if you can't read what `--dry-run` prints, you have a code-clarity problem in the .perch file.

### `--ask` — interactive op-by-op confirmation

```sh
perch --ask deploy
# > Run `shell "kubectl apply -f manifest.yaml"`? [y/n/a/q]
```

`y` = run this op, `n` = skip, `a` = run everything from here without asking, `q` = quit. Best for first-time runs of an unfamiliar `.perch` file (especially community recipes).

### `--trace` — live stream of every op

```sh
perch --trace deploy 2>trace.log
# stderr now has timestamped "▸ kind args… ✓ (dur)" lines for every op
```

Shows execution as it happens, properly indented for block-op nesting. Useful when you suspect the program is "stuck" somewhere — `--trace` will show you the last op that fired.

### `--audit FILE.ndjson` — structured event log

One JSON line per op:

```json
{"ts":"2026-…","kind":"shell","args":{"cmd":"…"},"dur_ms":42,"result":null,"error":null}
```

Pipe into `jq` to inspect:

```sh
perch --audit /tmp/run.ndjson deploy
jq 'select(.error)' < /tmp/run.ndjson         # every failing op
jq 'select(.dur_ms > 1000) | .kind' < /tmp/run.ndjson  # slow ops
jq -r '.kind' < /tmp/run.ndjson | sort | uniq -c   # op-kind histogram
```

### `--report` — post-run span tree

```sh
perch --report deploy
```

After the run, prints a tree of every op call, indented by block nesting, annotated with timings and error context. Perfect for "the command failed — what was the exact path?"

### Reading common errors

| Error | What it means | Fix |
|---|---|---|
| `unknown op kind "foo"` | The op isn't a built-in and isn't a `template` you imported. | `perch --check` lists all unknown kinds. Check spelling. |
| `unresolved ${name}` | You referenced `${name}` but nothing binds it. | Add an `arg`, `let`, or `globals` entry. |
| `command "deploy" not found` (with "did you mean…?") | Typo. | The suggestion is fuzzy-matched; usually right. |
| `host "X" not in --allow-host allowlist` | Tighter outer policy than the command needs. | Either widen the launch flag or add the host to a per-command allowlist. |
| `module closed with exit_code(N)` | A `wasm_run` module called `os.Exit(N)`. | The exit code is the module's own decision — check the module's source. |
| `redirect refused: host "X" not in allowlist` | HTTP got 3xx to a host you didn't allow. | Add the redirect destination to `--allow-host`, OR investigate (could be SSRF/phishing). |
| `interpreter: --max-runtime exceeded` | Wall-clock cap hit. | Either raise `--max-runtime` or look at the audit log for the slow op. |

### Reading the audit log practically

The audit log is the single most useful artifact for production triage. Standard pipeline:

```sh
# CI step: always emit audit
perch --audit /tmp/perch-$$.ndjson deploy

# On failure, archive the audit log alongside the test reports
if [ $? -ne 0 ]; then
    cp /tmp/perch-$$.ndjson "${CI_ARTIFACT_DIR}/audit.ndjson"
fi

# Later: investigate
jq -r '"\(.ts) \(.kind) \(.args.cmd // .args)" + (if .error then " — ERROR: \(.error)" else "" end)' \
   < audit.ndjson | tail -30
```

You're aiming for: the last ~30 ops before the failure, in order, with their post-interpolation arguments. That's almost always enough to reproduce.

---

## The five surfaces

Same `.perch` file, five front-ends:

### 1. CLI (default)

```sh
perch <cmd> [args]
perch --help
perch <cmd> --help
```

### 2. Web UI

```sh
perch -f commands.perch --server --port 8080
# → open http://127.0.0.1:8080
```

Five tabs (▶ Run / 🧪 Simulate / 🔍 Scan / ✓ Check / ℹ About). Type-aware form inputs, live output streaming, dark mode. Single-tenant + localhost-bound by default — put behind a reverse proxy for shared access. See [docs/web-ui.md](web-ui.md).

### 3. REPL

```sh
perch -f commands.perch --shell
> build target:linux
> ...
```

Each input wrapped as an ad-hoc command body and run. Bindings persist across lines.

### 4. MCP server (for AI agents)

```sh
# Claude Desktop / Claude Code / Cursor / Zed:
{
  "perch": {
    "command": "perch-mcp",
    "args": ["-f", "/abs/path/commands.perch"]
  }
}
```

Every visible command becomes an MCP tool with a JSON-schema arg surface. Capability flags inherit from the launch (`perch-mcp --no-shell --no-network --env KUBECONFIG -f ops.perch`). Live progress streaming via `notifications/progress`. See [docs/mcp.md](mcp.md).

### 5. Portable binary

```sh
perch --build -f commands.perch -o myapp
./myapp <cmd>                              # the binary contains the program
```

Recipients run one file. No Go, no perch, no source clone.

---

## Patterns and idioms

Eighteen things you'll reach for on every serious perch project. Most are 5-line idioms — copy-paste them, adapt the variable names.

### 1. Require a binary (with a useful error)

```perch
template require_bin
    arg name string
    do
        if not has_bin "${name}"
            fail "${name} is required — install it first"
        end
    end
end

# Use:
call require_bin "kubectl"
call require_bin "docker"
```

### 2. Idempotent setup ("install if missing")

```perch
let marker = format "${cache_dir}/myapp/.setup-${version}"
if not exists "${marker}"
    # ... do expensive one-time setup ...
    touch "${marker}"
end
```

### 3. Wait for a port

```perch
template wait_for_port
    arg host string
    arg port string
    arg timeout_s string
    do
        retry max=30 delay=1s
            if not port_check "${host}" "${port}"
                fail "${host}:${port} not yet ready"
            end
        end
    end
end

# Use:
call wait_for_port "localhost" "5432" "30"
```

### 4. Capture and reuse stdin / piped input

```perch
command count
    description "Count lines on stdin"
    do
        let raw = read_file "/dev/stdin"
        let n = length "${raw}"
        print "${n}"
    end
end
# usage: cat file.txt | perch count
```

### 5. Cache an expensive shell call

```perch
cache key="${file_hash}" ttl="24h"
    let cost = exec ./expensive-analyzer ${file}
end
print "${cost}"   # populated whether cache hit or miss
```

The cache key should incorporate every input that affects the output (file hashes, args, tool version).

### 6. Validate args with regex

```perch
arg host
    type string
    description "Target host"
end

do
    if not regex_match "${host}" "^[a-z0-9.-]+\.internal$"
        fail "invalid host: ${host}"
    end
    # ... safe to use ${host} downstream ...
end
```

Always validate user-supplied identifiers BEFORE interpolating into `shell` strings. This is the #1 injection-vector pattern to know.

### 7. Forward unknown verbs (catch-all)

```perch
catch unknown
    description "Forward to gh, like a typo-correcting alias"
    proxy_args                       # required to bind ${proxy_args}
    do
        shell "gh ${proxy_args}"
    end
end
```

Now `perch repo view` → `gh repo view`, etc. Useful for wrapping an existing CLI. The `proxy_args` modifier is **required** — without it, `${proxy_args}` is unbound in the catch body and referencing it errors. This makes the catch→shell forwarding pattern explicit instead of accidental.

### 8. Parallel fan-out with limit

```perch
parallel max=4
    for_each region in "us-east-1,us-west-2,eu-west-1,ap-south-1"
        exec deploy-region.sh ${region}
    end
end
```

`max=4` caps concurrency. Without it the loop runs all branches at once, which is rarely what you want.

### 9. Build then verify

```perch
command release
    do
        run build
        run test           # tests run after a successful build
        run sign
        run publish        # only if everything above passed
    end
end
```

`run NAME` halts the parent on a non-zero from the callee. Chain commands as transactions.

### 10. Time-bounded operation

```perch
timeout secs=300
    exec make integration-test
end
```

Wraps in a `context.WithDeadline`-style cap. The `shell` op can't be interrupted mid-syscall, but anything still pending in the next op after timeout returns `ErrTimeout`.

### 11. Atomic file write

```perch
let tmp = mktemp_dir
write_file "${tmp}/out.json" "${data}"
mv "${tmp}/out.json" "${target}"
rm "${tmp}"
```

The `mv` is atomic on the same filesystem — readers never see a partial file.

### 12. Detect-and-degrade tool fallback

```perch
if has_bin "rg"
    let matches = exec rg -l ${pattern} ./src
end
if not has_bin "rg"
    let matches = exec grep -r -l ${pattern} ./src
end
```

Prefer ripgrep; fall back to grep. Same pattern for fd/find, bat/cat, eza/ls.

### 13. Bundled assets pattern

```perch
bundle
    include "./templates" as templates
    include "./schemas" as schemas
end

command render
    do
        let tmpl = read_file "${bundle_dir}/templates/email.html"
        # ...
    end
end
```

`${bundle_dir}` lazily extracts the embedded archive on first reference; falls back to `${script_dir}` when running from a `.perch` file (not a built binary). Same code path in dev + production.

### 14. Secrets from environment, never the file

```perch
# WRONG: secret in source
globals
    API_KEY = "sk-abc123"
end

# RIGHT: read from env at runtime
command call_api
    do
        let key = get_env "API_KEY"
        if "${key}" == ""
            fail "API_KEY env var not set"
        end
        # ... use ${key} ...
    end
end
```

Combine with `--env API_KEY` to restrict which env vars resolve via `${NAME}`. Audit logs redact env vars whose names contain `KEY`, `TOKEN`, `SECRET`, `PASSWORD` (case-insensitive).

### 15. Test mode for destructive ops

```perch
command delete_old_backups
    arg dry_run
        type bool
        default true                          # SAFE DEFAULT
    end
    do
        let old = exec find /backup -mtime +30
        if "${dry_run}" == "true"
            print "would delete:"
            print "${old}"
        end
        if "${dry_run}" == "false"
            for_each f in "${old}"
                rm "${f}"
            end
        end
    end
end
# Default-safe: `perch delete_old_backups` is a dry run.
# Real run requires `perch delete_old_backups -dry_run=false`.
```

### 16. Reproducible "what version are we on"

```perch
command version
    do
        let rev = exec git rev-parse --short HEAD
        let dirty = exec git status --porcelain
        let suffix = ""
        if "${dirty}" != ""
            let suffix = "-dirty"
        end
        print "${rev}${suffix}"
    end
end
```

Use as a build-id seed: `let buildid = run version` → bake into binary.

### 17. JSON pipeline (no `jq` shell-out)

```perch
let body = http_get "https://api.example.com/users"
let count = json_count "${body}" ".items"
let first_name = json_get "${body}" ".items.0.name"
print "got ${count} users; first: ${first_name}"
```

Pure perch — no `jq` dependency, works on Windows, no quoting issues.

### 18. Service-style command with cleanup

```perch
command serve
    detached
    on_signal stop_serve
    do
        write_file "${temp_dir}/myapp.pid" "${pid}"
        exec go run ./cmd/server
    end
end

command stop_serve
    private
    do
        if exists "${temp_dir}/myapp.pid"
            let pid = read_file "${temp_dir}/myapp.pid"
            exec kill ${pid}
            rm "${temp_dir}/myapp.pid"
        end
    end
end
```

Detached + on_signal + PID-file. The standard "Ctrl-C cleans up gracefully" shape.

---

## Walkthrough 1 — Replace your Makefile

**Goal:** Replace a 200-line Makefile that breaks on Windows / Apple Silicon / the new intern's laptop. One file, three surfaces (CLI for local dev, CI for builds, `--server` for support).

```perch
# build.perch
name        "myapp"
version     "1.0.0"
about       "Build, test, release myapp"

globals
    APP_NAME = "myapp"
    OUT_DIR  = "./dist"
end

template ensure_clean
    arg dir string
    do
        rm "${dir}"
        mkdir "${dir}"
    end
end

command build
    description "Compile for the current platform"
    do
        call ensure_clean "${OUT_DIR}/${os}-${arch}"
        exec go build -o ${OUT_DIR}/${os}-${arch}/${APP_NAME} ./cmd/${APP_NAME}
        let size = file_size "${OUT_DIR}/${os}-${arch}/${APP_NAME}"
        print "✓ built ${size} bytes"
    end
end

command release
    description "Cross-compile for darwin/linux/windows × amd64/arm64"
    do
        parallel max=4
            with_env GOOS=darwin,GOARCH=arm64
                exec go build -o ${OUT_DIR}/darwin-arm64/${APP_NAME} ./cmd/${APP_NAME}
            end
            with_env GOOS=darwin,GOARCH=amd64
                exec go build -o ${OUT_DIR}/darwin-amd64/${APP_NAME} ./cmd/${APP_NAME}
            end
            with_env GOOS=linux,GOARCH=amd64
                exec go build -o ${OUT_DIR}/linux-amd64/${APP_NAME} ./cmd/${APP_NAME}
            end
            with_env GOOS=windows,GOARCH=amd64
                exec go build -o ${OUT_DIR}/windows-amd64/${APP_NAME}.exe ./cmd/${APP_NAME}
            end
        end
        print "✓ all 4 targets built"
    end
end

command test
    description "Run the test suite"
    do
        exec go test -race ./...
        if exists "./integration"
            exec go test -tags=integration ./integration/...
        end
    end
end

command clean
    description "Remove build artifacts"
    do
        rm "${OUT_DIR}"
    end
end

command ci
    description "What CI runs"
    do
        run test
        run release
    end
end
```

Wire it up:

```sh
# Local dev
perch test
perch build
perch release

# Pre-commit
perch --check        # statically validate
perch test           # unit tests sandboxed

# CI
perch ci             # full pipeline

# Support (non-devs)
perch --server --port 8080
# Support engineers click verbs in a browser instead of typing kubectl
```

Same file. Five entry points. Zero CSS, zero Cobra/Click boilerplate, zero CI YAML duplication.

→ Full tutorial: [tutorials/01-replace-your-makefile.md](tutorials/01-replace-your-makefile.md).

---

## Walkthrough 2 — Ship a self-installing tool

**Goal:** Distribute an internal CLI (Python project, or Node, or anything) as one binary. Recipients run one file. No `pip install`, no `npm install`, no toolchain on the target.

```perch
# stt.perch  — Speech-to-Text CLI, distributed as one binary
name "stt"
version "1.0.0"

bundle
    include "./src"                # the Python source tree
    include "./requirements.txt"
end

template ensure_dir
    arg path string
    do
        if not exists "${path}"
            mkdir "${path}"
        end
    end
end

command install
    description "Set up stt on this machine (idempotent)"
    do
        if not has_bin "python3"
            fail "stt needs python3 (3.10+) — install via your OS package manager"
        end

        let install_dir = format "${cache_dir}/stt/${bundle_hash}"
        call ensure_dir "${install_dir}"

        if not exists "${install_dir}/.installed"
            print "→ extracting source to ${install_dir}"
            bundle_extract "${install_dir}"

            print "→ creating venv"
            exec python3 -m venv ${install_dir}/.venv

            print "→ installing dependencies"
            exec ${install_dir}/.venv/bin/pip install -r ${install_dir}/requirements.txt

            touch "${install_dir}/.installed"
        end

        # Drop a launcher into ~/.local/bin
        call ensure_dir "${home_dir}/.local/bin"
        write_file "${home_dir}/.local/bin/stt" "#!/bin/sh\nexec ${install_dir}/.venv/bin/python ${install_dir}/src/main.py \"$@\"\n"
        chmod "${home_dir}/.local/bin/stt" "755"

        print "✓ installed. Run: stt --help"
        print "  (make sure ~/.local/bin is on your PATH)"
    end
end

command uninstall
    description "Remove stt"
    do
        rm "${cache_dir}/stt"
        rm "${home_dir}/.local/bin/stt"
        print "✓ removed."
    end
end
```

Build + ship:

```sh
perch --build -f stt.perch -o stt
scp stt user@host:/tmp/
ssh user@host '/tmp/stt install && stt example.wav'
```

The recipient needs **only** `python3` on PATH. No pip, no virtualenv setup, no internet at install time, no version skew.

→ Full demo: [demos/05-python-installer](https://github.com/luowensheng/perch/tree/main/demos/05-python-installer).

---

## Walkthrough 3 — Internal ops backend (web UI + MCP)

**Goal:** Give your team a self-service ops surface. Support engineers click verbs in a browser; engineers run them from a terminal; AI agents (Claude, Cursor) execute them through MCP. One source of truth for everything.

```perch
# ops.perch — internal ops + runbooks
name "ops"
about "Internal ops surface for the platform team"
version "1.3.0"

globals
    APPROVED_HOSTS = "api.example.com,deploy.example.com,*.s3.amazonaws.com"
end

command restart_service
    description "Restart a service on a remote host (validated)"
    arg host
        type string
        description "Target host (must match /^[a-z0-9.-]+\.internal$/)"
    end
    arg service
        type string
        description "Service name (must be one of: web, worker, scheduler)"
    end
    do
        if not regex_match "${host}" "^[a-z0-9.-]+\.internal$"
            fail "invalid host: ${host}"
        end
        if not regex_match "${service}" "^(web|worker|scheduler)$"
            fail "invalid service: ${service}"
        end
        exec ssh ops@${host} "systemctl restart ${service}"
    end
end

command health_check
    description "Check the API's /health endpoint"
    do
        let body = http_get "https://api.example.com/health"
        assert_contains "${body}" "ok"
        print "✓ healthy"
    end
end

command list_pods
    description "List pods in the prod namespace"
    do
        exec kubectl get pods -n prod
    end
end

command tail_logs
    description "Stream logs for a service"
    arg service
        type string
        description "Service name"
    end
    do
        exec kubectl logs -f deployment/${service} -n prod
    end
end

# Hidden helper — not visible to CLI / MCP / web UI
command _kube_context
    private
    do
        exec kubectl config use-context prod
    end
end
```

### Launch surfaces

```sh
# CLI for engineers
perch -f ops.perch list_pods

# Web UI for support, locked down
perch -f ops.perch --server --port 8080 \
      --no-shell-metachars \
      --allow-bin ssh,kubectl \
      --allow-host api.example.com,*.example.com \
      --env KUBECONFIG \
      --audit /var/log/perch-ops.ndjson

# MCP for AI agents, the strictest gates
perch-mcp -f /etc/perch/ops.perch \
      --no-shell-metachars \
      --allow-bin kubectl \
      --env KUBECONFIG
```

The MCP server now lets Claude / Cursor / Zed call verbs like `list_pods` and `tail_logs` directly — typed args, capability-gated, audit-logged. The agent can't shell out to anything other than `kubectl`; can't dial any host outside the allowlist; can't read env vars outside `KUBECONFIG`. All while the support team has a web UI for the same verbs.

→ Web UI deep dive: [docs/web-ui.md](web-ui.md).
→ MCP deep dive: [docs/mcp.md](mcp.md).

---

## Walkthrough 4 — Plugin host with WASM

**Goal:** Let third parties (or AI agents) write plugins that run inside your system, without granting them shell / network / fs access.

```perch
# plugin-host.perch
name "plugin-host"
version "1.0.0"

bundle
    include "./plugins/tax.wasm"      as tax
    include "./plugins/discount.wasm" as discount
    include "./plugins/shipping.wasm" as shipping
end

command run_plugin
    description "Apply one declared plugin to an order"
    arg plugin
        type string
        description "Plugin name (one of: tax, discount, shipping)"
    end
    do
        if not exists "/tmp/order.json"
            fail "no /tmp/order.json — provide input first"
        end

        if "${plugin}" == "tax"
            wasm_run tax
                wasm_arg "/ro/input/order.json"
                wasm_mount_read "/tmp"
            end
        end
        if "${plugin}" == "discount"
            wasm_run discount
                wasm_arg "/ro/input/order.json"
                wasm_mount_read "/tmp"
            end
        end
        if "${plugin}" == "shipping"
            wasm_run shipping
                wasm_arg "/ro/input/order.json"
                wasm_mount_read "/tmp"
                wasm_allow_host "rates.example.com"   # this one needs HTTP
            end
        end
    end
end

command run_all
    description "Apply every plugin in parallel"
    do
        parallel max=3
            run run_plugin "-plugin=tax"
            run run_plugin "-plugin=discount"
            run run_plugin "-plugin=shipping"
        end
    end
end
```

**What's enforced** (by the WASM runtime, not by perch's policy layer):

- A plugin trying to read `/etc/passwd` → file doesn't exist in its world
- A plugin trying to spawn `curl` → no such syscall in WASI
- A plugin trying to open a TCP socket → not implemented
- A plugin trying to read `$AWS_KEY` → env var not in its allowlist
- A plugin trying to write outside `/rw` → ENOENT

Plugins can be written in **any language that targets WASI**: Go, Rust, TinyGo, Zig, C++ via wasi-sdk, AssemblyScript. The contract is "read `/ro/input/X`, write JSON to stdout."

→ Killer demo with malicious plugin: [demos/wasm-plugin-host](https://github.com/luowensheng/perch/tree/main/demos/wasm-plugin-host).
→ Five real-world WASM walkthroughs: [docs/wasm-walkthroughs.md](wasm-walkthroughs.md).

---

## Walkthrough 5 — CI gate with `simulate`

**Goal:** Before merging a change to your `.perch` file, prove it won't break on the production host AND won't break on the staging host AND won't be vulnerable to an API outage.

```sh
# .github/workflows/perch-gate.yml
- name: Validate perch program
  run: perch --check -f deploy.perch

- name: Scan for risks
  run: perch --scan -f deploy.perch

- name: Simulate against all production scenarios
  run: perch -f deploy.perch simulate --sim-file ci/fixture.json
```

`ci/fixture.json` declares the scenarios CI cares about:

```json
{
  "os": "linux", "arch": "amd64",
  "bins": ["docker", "kubectl", "git"],
  "env": {"HOME": "/home/runner", "KUBECONFIG": "/etc/kube/config"},
  "fs_write": ["/tmp", "/var/log/deploy"],
  "network": ["api.github.com", "*.s3.amazonaws.com"],

  "oracles": {
    "shell_output": {
      "git rev-parse HEAD": "abc123",
      "kubectl get nodes -o name": "node/prod-1\nnode/prod-2"
    },
    "http": {
      "https://api.github.com/health": {"status": 200, "body": "OK"}
    },
    "has_bin": {"docker": true, "kubectl": true}
  },

  "scenarios": [
    {"name": "happy-path",        "overrides": {}},
    {"name": "github-down",       "overrides": {
      "http": {"https://api.github.com/health": {"status": 500}}
    }},
    {"name": "kubectl-missing",   "overrides": {
      "has_bin": {"kubectl": false}
    }},
    {"name": "github-redirect-to-evil", "overrides": {
      "http": {"https://api.github.com/health":
        {"status": 302, "redirect": "https://evil.com/payload"}}
    }}
  ]
}
```

Each scenario runs as an independent walk. Exit code is non-zero if **any** scenario has a `WILL_FAIL`. So you can prove things like:

- "If the deploy verb fires while api.github.com returns 500, every downstream op that doesn't degrade gracefully will fail."
- "If a future change introduces a `--allow-host` for *.evil.com via a malicious redirect, simulate catches it before merge."
- "If `kubectl` isn't on the host, our verb fails fast instead of silently skipping the deploy."

→ Simulate deep dive: [docs/simulate.md](simulate.md).

---

## Walkthrough 6 — Database backup + restore tool

**Goal:** A single `db.perch` file that backs up Postgres, MySQL, or SQLite, encrypts the dump, ships it to S3, and can restore from any backup. Schedulable from cron, callable from CI, drivable by AI agent through MCP.

```perch
name "db"
version "1.0.0"
about "Database backup + restore — Postgres / MySQL / SQLite"

globals
    BACKUP_DIR = "${home_dir}/.cache/db-backups"
    S3_BUCKET  = "my-backups"
end

template ensure_dir
    arg path string
    do
        if not exists "${path}"
            mkdir "${path}"
        end
    end
end

command backup
    description "Snapshot a database to local file + ship to S3"
    arg engine
        type string
        description "Engine: postgres | mysql | sqlite"
    end
    arg conn
        type string
        description "Connection string OR file path (sqlite)"
    end
    arg label
        type string
        default "manual"
        description "Tag for the backup filename"
    end
    do
        call ensure_dir "${BACKUP_DIR}"
        let stamp = format_time "now" "2006-01-02_150405"
        let out = format "${BACKUP_DIR}/${engine}-${label}-${stamp}.sql"

        if "${engine}" == "postgres"
            shell "pg_dump ${conn} > ${out}"
        end
        if "${engine}" == "mysql"
            shell "mysqldump --single-transaction ${conn} > ${out}"
        end
        if "${engine}" == "sqlite"
            shell "sqlite3 ${conn} .dump > ${out}"
        end

        let size = file_size "${out}"
        print "✓ dumped ${size} bytes → ${out}"

        # Encrypt + compress
        exec gzip ${out}
        let key = get_env "BACKUP_GPG_KEY"
        if "${key}" != ""
            exec gpg --batch --yes --recipient ${key} --encrypt ${out}.gz
            rm "${out}.gz"
            let final = format "${out}.gz.gpg"
        end
        if "${key}" == ""
            let final = format "${out}.gz"
        end

        # Ship to S3 (if aws cli is present)
        if has_bin "aws"
            exec aws s3 cp ${final} s3://${S3_BUCKET}/
            print "✓ uploaded to s3://${S3_BUCKET}/"
        end
        if not has_bin "aws"
            print "ℹ aws CLI not found — backup stays local at ${final}"
        end

        # Local retention: keep 14 most recent
        run prune_local "-engine=${engine}" "-keep=14"
    end
end

command restore
    description "Restore from a local or S3 backup file"
    arg engine type string description "Engine" end
    arg conn   type string description "Connection string (target db)" end
    arg path   type string description "Backup file path (.sql, .sql.gz, .sql.gz.gpg)" end
    do
        if not exists "${path}"
            fail "backup file not found: ${path}"
        end

        let tmp = mktemp_dir
        let stage = format "${tmp}/restore.sql"

        # Decrypt if needed
        if has_suffix "${path}" ".gpg"
            shell "gpg --batch --decrypt ${path} > ${stage}.gz"
            exec gunzip ${stage}.gz
        end
        if has_suffix "${path}" ".gz"
            shell "gunzip -c ${path} > ${stage}"
        end
        if not has_suffix "${path}" ".gz"
            cp "${path}" "${stage}"
        end

        # Restore
        if "${engine}" == "postgres"
            shell "psql ${conn} < ${stage}"
        end
        if "${engine}" == "mysql"
            shell "mysql ${conn} < ${stage}"
        end
        if "${engine}" == "sqlite"
            shell "sqlite3 ${conn} < ${stage}"
        end
        rm "${tmp}"
        print "✓ restored"
    end
end

command prune_local
    description "Remove old local backups, keeping the N most recent"
    arg engine type string end
    arg keep   type int default 14 end
    do
        # Sort by mtime, drop oldest. Implementation per OS:
        if is_unix
            shell "ls -1t ${BACKUP_DIR}/${engine}-*.sql.gz* 2>/dev/null | tail -n +$((${keep}+1)) | xargs -r rm"
        end
        if is_windows
            shell "Get-ChildItem ${BACKUP_DIR}\\${engine}-*.sql.gz* | Sort-Object LastWriteTime -Desc | Select-Object -Skip ${keep} | Remove-Item"
        end
    end
end

command list_backups
    description "Show backups available locally and in S3"
    do
        print "── Local ──"
        if exists "${BACKUP_DIR}"
            exec ls -lh ${BACKUP_DIR}/
        end
        if has_bin "aws"
            print "── S3 ──"
            exec aws s3 ls s3://${S3_BUCKET}/
        end
    end
end

# Test mode
command test_roundtrip
    test
    test_allow_shell
    description "Backup + restore + verify integrity on a tiny SQLite DB"
    do
        let tmp = mktemp_dir
        shell "sqlite3 ${tmp}/src.db 'CREATE TABLE t(x); INSERT INTO t VALUES(42);'"
        run backup "-engine=sqlite" "-conn=${tmp}/src.db" "-label=test"
        # Most recent backup wins:
        let latest = shell_output "ls -1t ${BACKUP_DIR}/sqlite-test-*.sql.gz* | head -1"
        run restore "-engine=sqlite" "-conn=${tmp}/dst.db" "-path=${latest}"
        let got = shell_output "sqlite3 ${tmp}/dst.db 'SELECT x FROM t;'"
        assert_eq "${got}" "42"
        rm "${tmp}"
    end
end
```

Schedule from cron:

```cron
# /etc/cron.d/db-backup
0 3 * * * appuser /usr/local/bin/perch -f /etc/db.perch backup engine:postgres conn:'host=db.internal dbname=prod' label:nightly
```

Hand to an agent via MCP:

```json
{"perch_db": {"command": "perch-mcp", "args": ["-f", "/etc/db.perch", "--env", "BACKUP_GPG_KEY,AWS_ACCESS_KEY_ID,AWS_SECRET_ACCESS_KEY"]}}
```

The agent now has typed `backup`, `restore`, `list_backups`, `prune_local` verbs. It can't shell out to anything other than the database tools and aws (constrain further with `--allow-bin`).

---

## Walkthrough 7 — SSL certificate rotation

**Goal:** Automate the cert lifecycle — request from Let's Encrypt, install to nginx, reload, monitor expiry. One file replaces an Ansible playbook + a cron job + a Slack-alerting script.

```perch
name "cert"
version "1.0.0"

globals
    CERT_DIR   = "/etc/letsencrypt/live"
    NGINX_CONF = "/etc/nginx/conf.d"
    ALERT_WEBHOOK = "https://hooks.slack.com/services/REPLACEME"
end

command issue
    description "Get a new cert from Let's Encrypt for a domain"
    arg domain type string description "Domain (e.g. example.com)" end
    arg email  type string description "Contact email for LE registration" end
    do
        call require_bin "certbot"
        exec certbot certonly --non-interactive --agree-tos --email ${email} --webroot -w /var/www/html -d ${domain}
        run install_to_nginx "-domain=${domain}"
        run alert "-msg=issued cert for ${domain}"
    end
end

command renew
    description "Try to renew every cert; reload nginx only if anything changed"
    do
        let before = shell_output "ls -lT ${CERT_DIR}/*/fullchain.pem 2>/dev/null | md5sum"
        exec certbot renew --quiet
        let after = shell_output "ls -lT ${CERT_DIR}/*/fullchain.pem 2>/dev/null | md5sum"
        if "${before}" != "${after}"
            exec nginx -s reload
            run alert "-msg=renewed cert(s); reloaded nginx"
        end
        if "${before}" == "${after}"
            print "nothing to renew"
        end
    end
end

command install_to_nginx
    description "Generate nginx conf for a domain; symlink cert paths; reload"
    arg domain type string end
    do
        let conf = format "${NGINX_CONF}/${domain}.conf"
        write_file "${conf}" "server {
    listen 443 ssl http2;
    server_name ${domain};
    ssl_certificate     ${CERT_DIR}/${domain}/fullchain.pem;
    ssl_certificate_key ${CERT_DIR}/${domain}/privkey.pem;
    location / { proxy_pass http://127.0.0.1:8080; }
}
"
        exec nginx -t
        exec nginx -s reload
    end
end

command check_expiry
    description "Find certs expiring within N days; alert if any"
    arg threshold_days type int default 14 end
    do
        let found = ""
        for_each cert in "${CERT_DIR}/*/cert.pem"
            let days = shell_output "openssl x509 -in ${cert} -enddate -noout | awk -F= '{print $2}' | xargs -I{} date -d '{}' +%s | awk -v now=$(date +%s) '{print int(($1-now)/86400)}'"
            if days < threshold_days
                let domain = shell_output "basename $(dirname ${cert})"
                let found = format "${found}${domain} (${days}d)\n"
            end
        end
        if "${found}" != ""
            run alert "-msg=certs expiring soon:\n${found}"
        end
    end
end

command alert
    description "Post a message to the Slack webhook"
    private
    arg msg type string end
    do
        let body = format '{"text":"[cert.perch] ${msg}"}'
        exec curl -sS -X POST -H "Content-Type: application/json" -d ${body} ${ALERT_WEBHOOK}
    end
end

template require_bin
    arg name string
    do
        if not has_bin "${name}"
            fail "${name} is required"
        end
    end
end
```

Schedule:

```cron
# /etc/cron.d/cert
17 3 * * * root /usr/local/bin/perch -f /etc/cert.perch renew
0 9 * * 1  root /usr/local/bin/perch -f /etc/cert.perch check_expiry threshold_days:14
```

One file. Replaces three tools (the Ansible cert role, the cron entry, the alert script). Operators run `perch -f /etc/cert.perch issue domain:foo.com email:ops@foo.com` for new domains. CI smoke-tests it with simulate before merge.

---

## Walkthrough 8 — Multi-environment deployer

**Goal:** Same `deploy.perch` deploys to dev / staging / prod with environment-appropriate guards. Prod requires confirmation. All envs use the same verbs.

```perch
name "deploy"
version "2.0.0"

globals
    PROD_CONFIRM_TOKEN = "yes-deploy-prod"
end

command deploy
    description "Deploy to a target environment"
    arg env
        type string
        description "Environment: dev | staging | prod"
    end
    arg image
        type string
        description "Container image (full ref with tag)"
    end
    arg confirm
        type string
        default ""
        description "(prod only) Type 'yes-deploy-prod' to authorise"
    end
    do
        # 1. Validate env name
        if not regex_match "${env}" "^(dev|staging|prod)$"
            fail "env must be one of: dev, staging, prod (got ${env})"
        end

        # 2. Validate image ref
        if not regex_match "${image}" "^[a-z0-9./_-]+:[a-z0-9._-]+$"
            fail "image must be in the form name:tag — got ${image}"
        end

        # 3. Prod confirmation gate
        if "${env}" == "prod"
            if "${confirm}" != "${PROD_CONFIRM_TOKEN}"
                fail "prod deploys require -confirm=${PROD_CONFIRM_TOKEN}"
            end
        end

        # 4. Capability gate by env
        run _set_kube_context "-env=${env}"

        # 5. Diff before apply
        print "── Pending changes for ${env} ──"
        exec helm diff upgrade myapp ./chart --set image=${image} -n ${env}

        # 6. Apply
        if "${env}" == "prod"
            run _alert "-msg=deploying ${image} to PROD by ${user}"
        end
        exec helm upgrade --install myapp ./chart --set image=${image} -n ${env} --wait

        # 7. Smoke test
        run smoke "-env=${env}"

        # 8. Record the deploy
        let stamp = format_time "now" "2006-01-02T15:04:05Z"
        append_file "${cache_dir}/deploys.ndjson" "{\"env\":\"${env}\",\"image\":\"${image}\",\"by\":\"${user}\",\"at\":\"${stamp}\"}\n"
    end
end

command rollback
    description "Rollback an environment to the previous release"
    arg env type string end
    arg confirm type string default "" end
    do
        if "${env}" == "prod"
            if "${confirm}" != "${PROD_CONFIRM_TOKEN}"
                fail "prod rollbacks require -confirm=${PROD_CONFIRM_TOKEN}"
            end
        end
        run _set_kube_context "-env=${env}"
        exec helm rollback myapp -n ${env}
        run smoke "-env=${env}"
        run _alert "-msg=rolled back ${env} (by ${user})"
    end
end

command smoke
    description "Quick health check against an env's API"
    arg env type string end
    do
        let host = format "api-${env}.example.com"
        if "${env}" == "prod"
            let host = "api.example.com"
        end
        retry max=10 delay=3s
            let body = http_get "https://${host}/health"
            assert_contains "${body}" "ok"
        end
        print "✓ ${env} healthy"
    end
end

command _set_kube_context
    private
    arg env type string end
    do
        exec kubectl config use-context ${env}-cluster
    end
end

command _alert
    private
    arg msg type string end
    do
        let url = get_env "SLACK_WEBHOOK"
        if "${url}" != ""
            shell "curl -sS -X POST -H 'Content-Type: application/json' -d '{\"text\":\"${msg}\"}' ${url}"
        end
    end
end

# Test that the env validator rejects garbage
command test_env_validator
    test
    description "deploy with bogus env must fail"
    do
        # We can't actually call `deploy` here without side effects.
        # Test the regex in isolation instead:
        if regex_match "not-an-env" "^(dev|staging|prod)$"
            fail "regex accepted invalid env"
        end
    end
end
```

Invocations:

```sh
# Dev — no confirmation required
perch deploy env:dev image:myapp:abc123

# Prod — explicit confirmation
perch deploy env:prod image:myapp:abc123 confirm:yes-deploy-prod

# CI pipeline (staging on every push to main)
perch deploy env:staging image:myapp:${GITHUB_SHA}
```

Same file. Three environments. Confirmation gate that's IMPOSSIBLE to bypass via typo (you have to type the exact token). Audit log of every deploy in `~/.cache/perch/deploys.ndjson`.

---

## Walkthrough 9 — File-processing pipeline

**Goal:** Read a CSV of customer records, normalize + enrich + validate, emit cleaned JSON. Classic ETL shape, scriptable from CI / cron / agent.

```perch
name "etl"
version "1.0.0"

globals
    SCHEMA_HASH = "v1"
end

command process
    description "Run the full pipeline: extract → transform → load"
    arg input  type string description "Input CSV path" end
    arg output type string description "Output JSON path" end
    do
        if not exists "${input}"
            fail "input file not found: ${input}"
        end

        # 1. Hash inputs for cache invalidation
        let in_hash = sha256_file "${input}"
        let cache_key = format "${SCHEMA_HASH}:${in_hash}"

        # 2. Skip if cached output matches
        cache key:"${cache_key}" ttl:"24h"
            run _extract "-input=${input}"
            run _transform
            run _validate
            run _load "-output=${output}"
        end
        print "✓ pipeline done; output at ${output}"
    end
end

command _extract
    private
    arg input type string end
    do
        let raw = read_file "${input}"
        let rows = csv_parse "${raw}"
        let n = json_count "${rows}" "."
        print "extracted ${n} rows from ${input}"
        write_file "${temp_dir}/etl-rows.json" "${rows}"
    end
end

command _transform
    private
    do
        let rows = read_file "${temp_dir}/etl-rows.json"
        # Hand off to a WASM module that does the real lift in Go/Rust:
        wasm_run "${script_dir}/transform.wasm"
            wasm_arg "/ro/etl-rows.json"
            wasm_arg "/rw/etl-rows-clean.json"
            wasm_mount_read "${temp_dir}"
            wasm_mount_write "${temp_dir}"
        end
    end
end

command _validate
    private
    do
        # In-perch sanity checks before declaring "ok":
        let rows = read_file "${temp_dir}/etl-rows-clean.json"
        let n = json_count "${rows}" "."
        if n < 1
            fail "transform produced 0 rows"
        end
        # Every row must have an `email` field:
        for_each row in "${rows}"
            let email = json_get "${row}" ".email"
            if "${email}" == ""
                fail "row missing email field"
            end
            if not regex_match "${email}" "^[^@]+@[^@]+\.[^@]+$"
                fail "invalid email: ${email}"
            end
        end
        print "✓ validated ${n} rows"
    end
end

command _load
    private
    arg output type string end
    do
        mv "${temp_dir}/etl-rows-clean.json" "${output}"
    end
end

command serve_pipeline
    description "Watch a dir; process new CSVs as they arrive"
    do
        retry max=99999 delay=5s
            let in = shell_output "ls ./inbox/*.csv 2>/dev/null | head -1"
            if "${in}" == ""
                fail "no new files"   # forces retry to wait
            end
            let out = format "./outbox/${user}-$(basename ${in} .csv).json"
            run process "-input=${in}" "-output=${out}"
            mv "${in}" "./done/"
        end
    end
end
```

Notes:

- **`wasm_run` for the real work.** The transform step is in Go/Rust compiled to WASM — `.wasm` files travel with the binary via `bundle`. Bytewise-deterministic across OSes; no Python/Node toolchain on the target.
- **`cache` key includes the schema version.** Any rev to the WASM module changes `SCHEMA_HASH`, invalidating every prior result.
- **`serve_pipeline` is a "poll" pattern.** Retry with a long delay + a `fail` on "nothing to do" creates a soft daemon. For real production, run under systemd or supervisord and use `detached` + `on_signal` for clean shutdown.

---

## Walkthrough 10 — Git workflow wrapper

**Goal:** Replace your team's three different "how do I open a PR" wikis with one `pr.perch` file. Same verbs locally, in CI, and via the web UI for non-devs reviewing branches.

```perch
name "pr"
version "1.0.0"
about "Standardised git workflow — branch, commit, push, PR"

globals
    DEFAULT_BASE = "main"
end

command branch
    description "Create a branch from the latest base"
    arg name
        type string
        description "Branch name (will be prefixed with username)"
    end
    arg base
        type string
        default "${DEFAULT_BASE}"
        description "Base branch (default: main)"
    end
    do
        # Validate name (no slashes; no spaces)
        if not regex_match "${name}" "^[a-z0-9-]+$"
            fail "branch name must be lowercase alphanumeric with hyphens"
        end
        exec git fetch origin
        exec git switch ${base}
        exec git pull --ff-only
        exec git switch -c ${user}/${name}
        print "✓ on ${user}/${name}"
    end
end

command commit
    description "Stage and commit (conventional-commits format)"
    arg type
        type string
        description "Commit type: feat | fix | refactor | docs | test | chore"
    end
    arg msg
        type string
        description "Commit message (without the type prefix)"
    end
    do
        if not regex_match "${type}" "^(feat|fix|refactor|docs|test|chore)$"
            fail "type must be one of: feat fix refactor docs test chore"
        end
        exec git add -A
        exec git commit -m "${type}: ${msg}"
    end
end

command pr
    description "Open a PR for the current branch"
    arg title
        type string
        default ""
        description "PR title (defaults to last commit message)"
    end
    arg base
        type string
        default "${DEFAULT_BASE}"
    end
    do
        let branch = exec git symbolic-ref --short HEAD
        if "${branch}" == "${base}"
            fail "you're on ${base} — make a feature branch first (`perch branch name:my-feature`)"
        end

        # Push
        exec git push -u origin ${branch}

        # Title fallback
        if "${title}" == ""
            let title = exec git log -1 --pretty=%s
        end

        # Open the PR via gh
        call require_bin "gh"
        let url = exec gh pr create --base ${base} --title ${title} --body "Opened via perch pr."
        print "✓ PR opened: ${url}"

        # Try to copy to clipboard
        if has_bin "pbcopy"
            shell "echo '${url}' | pbcopy"
            print "  (copied to clipboard)"
        end
        if has_bin "xclip"
            shell "echo '${url}' | xclip -selection clipboard"
        end
    end
end

command land
    description "Squash-merge the current PR + delete the branch + sync local"
    do
        call require_bin "gh"
        let branch = exec git symbolic-ref --short HEAD
        let base = exec gh pr view --json baseRefName -q .baseRefName
        exec gh pr merge --squash --auto --delete-branch
        exec git switch ${base}
        exec git pull --ff-only
        print "✓ landed; back on ${base}"
    end
end

command sync
    description "Pull the latest base; rebase the current branch"
    do
        let branch = exec git symbolic-ref --short HEAD
        exec git fetch origin ${DEFAULT_BASE}
        exec git rebase origin/${DEFAULT_BASE}
        print "✓ rebased ${branch} on ${DEFAULT_BASE}"
    end
end

command cleanup
    description "Remove local branches whose remotes are gone"
    do
        exec git fetch --prune
        shell "git branch -vv | grep ': gone]' | awk '{print $1}' | xargs -r git branch -D"
    end
end

template require_bin
    arg name string
    do
        if not has_bin "${name}"
            fail "${name} is required — install it first"
        end
    end
end
```

Now your team's workflow is:

```sh
perch branch  name:fix-auth-bug
# ... edit code ...
perch commit  type:fix  msg:"validate auth header before decoding"
perch pr      title:"Fix: validate auth header before JWT decode"
perch land
```

vs. the old way (three different shell aliases on three different laptops, none of which work on the new intern's Windows machine). And the same file works in CI:

```yaml
# .github/workflows/sync.yml
- run: perch -f pr.perch cleanup
```

---

## Distribution — getting your binary to users

Three patterns by audience:

### a) Internal team, you control the hosts

```sh
perch --build -f ops.perch -o ops
ansible all -m copy -a "src=ops dest=/usr/local/bin/ mode=0755"
```

### b) External team / customers

Ship via GitHub Releases + a `curl | sh` install script:

```sh
# install.sh in your repo
#!/bin/sh
VERSION="${1:-latest}"
curl -fsSL "https://github.com/yourorg/myapp/releases/${VERSION}/download/myapp-$(uname -s)-$(uname -m)" \
  -o /usr/local/bin/myapp
chmod +x /usr/local/bin/myapp
echo "installed myapp $(myapp --version)"
```

End users:

```sh
curl -fsSL https://yourorg.com/install.sh | sh
```

### c) AI agents

Set up `perch-mcp` in the agent's config:

```json
{
  "mcpServers": {
    "yourapp": {
      "command": "perch-mcp",
      "args": ["-f", "/etc/yourapp/commands.perch", "--no-shell-metachars", "--allow-bin", "kubectl"]
    }
  }
}
```

The agent now has typed access to every verb. See [docs/mcp.md](mcp.md).

---

## Production deployment

Running perch in production — your binary is on the boxes, agents are calling it, CI is wiring it into pipelines. The operational concerns:

### Log management

Every production invocation should emit an audit log. Rotate it:

```sh
# /etc/cron.daily/perch-audit-rotate
#!/bin/sh
cd /var/log/perch
gzip *.ndjson 2>/dev/null
find . -name '*.ndjson.gz' -mtime +30 -delete
```

Or use logrotate:

```
# /etc/logrotate.d/perch
/var/log/perch/*.ndjson {
    daily
    rotate 30
    compress
    missingok
    notifempty
}
```

### Monitoring perch itself

The audit log IS your monitoring surface. Useful queries:

```sh
# Failure rate (last hour)
find /var/log/perch -name '*.ndjson' -mmin -60 -exec cat {} \; \
  | jq 'select(.error)' | wc -l

# Slowest 10 ops in the last day
find /var/log/perch -name '*.ndjson' -mtime -1 -exec cat {} \; \
  | jq -s 'sort_by(-.dur_ms) | .[0:10] | .[] | {kind, dur_ms, cmd: .args.cmd}'

# Verbs called by AI agents (via perch-mcp)
grep -h '"mcp"' /var/log/perch/*.ndjson | jq -r '.args | keys[0]'
```

Ship to a real log aggregator (Loki, Datadog, ELK) by tailing the NDJSON into your existing pipeline.

### Hardening the launch

Defense-in-depth: every perch invocation in production should layer restrictions:

```sh
# Strict shape — no shell, no network, only the env vars the program needs
perch -f /etc/myapp.perch \
    --env KUBECONFIG,HOME,PATH \
    --no-shell-metachars \
    --allow-bin kubectl,curl \
    --allow-host api.example.com \
    --max-runtime 300 \
    --audit /var/log/perch/$(date +%Y-%m-%d).ndjson \
    deploy
```

Each flag closes one class of misuse:

| Flag | Closes |
|---|---|
| `--no-shell` | Removes the `shell` op entirely (use when your program is all native ops) |
| `--no-network` | Refuses HTTP / DNS / socket ops |
| `--no-subprocess` | Refuses `pkg_install`, `kill_by_name`, etc. |
| `--no-write` | Filesystem is read-only |
| `--no-shell-metachars` | Refuses shell strings with `|`, `&&`, `;`, `` ` ``, `$()` (forces simple `cmd args` form) |
| `--env K,K,…` | Restricts which env vars resolve via `${NAME}` |
| `--allow-bin N,…` | Whitelists the first token of every shell call |
| `--allow-host H,…` | HTTP allowlist (composes with the always-on SSRF guard) |
| `--max-runtime SECS` | Wall-clock cap |

Pre-flight any production launch with `perch --scan -f myapp.perch` — it prints the **recommended invocation** for your file based on the ops it uses.

### Systemd service template

```ini
# /etc/systemd/system/myapp.service
[Unit]
Description=myapp (perch-driven)
After=network.target

[Service]
ExecStart=/usr/local/bin/perch -f /etc/myapp.perch \
    --no-shell-metachars \
    --allow-bin docker,kubectl \
    --allow-host api.example.com \
    --max-runtime 0 \
    --audit /var/log/perch/myapp.ndjson \
    serve

Environment=KUBECONFIG=/etc/kube/config
User=appuser
Group=appgroup
Restart=on-failure
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

### Secrets

Never put secrets in the `.perch` file. Read from env:

```perch
do
    let token = get_env "API_TOKEN"
    if "${token}" == ""
        fail "API_TOKEN env var required"
    end
    # ... use ${token} ...
end
```

Inject via systemd's `Environment=` (read from a chmod-600 file), or via your secret manager (Vault / AWS Secrets Manager / 1Password CLI) at launch:

```sh
op run --env-file=.env -- perch -f myapp.perch deploy
```

The audit log redacts env vars whose names match `*KEY*`, `*TOKEN*`, `*SECRET*`, `*PASSWORD*` (case-insensitive) — but **don't rely on that** as your primary defense; use `--env` to restrict what perch can see in the first place.

### Distribution checksums

When shipping a binary via `--build`, publish + check sha256:

```sh
sha256sum myapp > myapp.sha256
# Recipient:
sha256sum -c myapp.sha256
```

For GitHub Releases the CI workflow at `.github/workflows/release.yml` (in this repo) already does this. The `bundle_hash` op gives the same hash from inside the running binary, so you can audit "did the bundle I built match the bundle currently running?"

### Versioning + compatibility

A `.perch` file's surface IS its commands. To evolve safely:

- **Add args with `optional` or `default`** — never break existing callers.
- **Add new commands freely** — they're additive.
- **Rename / remove commands** — bump major version, document migration in CHANGELOG.
- **Globals are read-only** — changing their value is a breaking change (the binary picks them up at startup; users who pin to `--version=1.0.0` get the old value).

Bake `version "1.2.0"` at the top; surface via `perch --version`. For complex programs, expose `command version` that prints both the program version AND `bundle_hash`:

```perch
command version
    do
        print "myapp ${version}"      # parse-time bound
        print "bundle ${bundle_hash}" # runtime
        print "perch  ${perch_version}"
    end
end
```

---

## Common pitfalls

### `${var}` inside double-double quotes

Variables interpolate INSIDE strings, even literal-looking ones:

```perch
shell "echo \"${var}\""
# perch substitutes ${var} first; the shell then sees the value
# If ${var} contains a quote, the shell receives a malformed string
```

**Fix:** validate `${var}` with `regex_match` before interpolating, OR use ops that don't pass through the shell (`print`, `write_file`).

### `let X = OP` doesn't survive `parallel`

```perch
parallel
    let result = http_get "https://a.com/health"  # local to this branch
end
print "${result}"   # ERROR: ${result} is unbound here
```

**Fix:** write to a file inside the branch and read it after; or restructure so each branch is a separate `run` to a command whose return value you don't need to consume.

### Forgetting to validate `${proxy_args}`

```perch
catch unknown
    proxy_args                       # explicit opt-in; without this, ${proxy_args} is unbound
    do
        shell "git ${proxy_args}"    # ← user-controlled string into shell!
    end
end
```

The `proxy_args` modifier is now **required** on both `command` and `catch` blocks to bind `${proxy_args}`. Without it, referencing `${proxy_args}` errors with `unresolved_var` — the forwarding intent has to be visible in the source. But even with the modifier declared, you still need to harden the launch: if the user runs `perch '$(rm -rf /)'`, that string lands in `${proxy_args}` verbatim. **Always pair `proxy_args` with `--no-shell-metachars`** at launch, or validate each segment with `regex_match` before shelling.

### `if exists "${path}"` race

`exists` is a snapshot. If another process creates/deletes the file between your `if exists` and the op that uses it, you have a TOCTOU race. For high-frequency ops, prefer "do the thing, catch the error" (e.g. `mkdir` is idempotent — perch's version doesn't fail if the dir exists).

### `cache` on non-deterministic body

```perch
cache key:"static-key" ttl:"1h"
    let now = now      # ← changes every call
    write_file "./out" "${now}"
end
```

The cache will hit; `${now}` won't update. **Cache only deterministic body shapes** — same key + same inputs → same outputs.

### Shadowing globals with args

```perch
globals
    region = "us-east-1"
end

command deploy
    arg region type string end
    do
        print "${region}"   # which one wins?
    end
end
```

Args win over globals within their command body. If you want the global, rename one of them (`g_region`, or `target_region` for the arg).

### `for_each` over unparsed string

```perch
for_each f in "a.txt b.txt c.txt"
    print "${f}"
end
# Prints once: "a.txt b.txt c.txt"
```

`for_each` splits on commas (and newlines), not whitespace.

**Fix:**

```perch
for_each f in "a.txt,b.txt,c.txt"   # or use \n separators from shell output
    print "${f}"
end
```

### Forgetting `--allow-private-ips` for localhost dev

```sh
perch -f dev.perch --allow-host localhost up
# Fails: localhost resolves to 127.0.0.1 which the SSRF guard blocks.
```

**Fix:** add `--allow-private-ips` to your dev invocation. (Production should NOT use this flag.)

### Single-quotes inside shell args

```perch
exec psql -c "SELECT * FROM t WHERE name = bob"
# Tricky to read; very tricky to maintain.
```

**Fix:** write the query to a tempfile, then `psql -f tmp.sql`. Or use a database op via WASM if it exists for your engine.

### `--build` with no `name`

```perch
# missing: name "..."
```

`perch --build` will fall back to a generic name; users get `perch-app` instead of `myapp`. **Always declare `name` and `version`** at the top of files you intend to build.

### Running an unknown `.perch` file

```sh
# DON'T:
curl http://example.com/random.perch | perch -f /dev/stdin do_stuff

# DO:
curl http://example.com/random.perch > /tmp/script.perch
perch --scan -f /tmp/script.perch        # audit FIRST
# If the scan looks fine, then:
perch --no-shell-metachars --max-runtime 60 -f /tmp/script.perch do_stuff
```

Treat external `.perch` files the same as you'd treat external shell scripts. `--scan` is your friend.

---

## FAQ

### Why not just use bash / Make / Just / Task?

| | bash | Make | Just | Task | perch |
|---|---|---|---|---|---|
| Cross-platform (without Cygwin/WSL) | ✗ | ✗ | partial | partial | ✓ |
| Typed args + per-command help | ✗ | ✗ | partial | partial | ✓ |
| Capability restrictions | ✗ | ✗ | ✗ | ✗ | ✓ |
| Web UI without writing one | ✗ | ✗ | ✗ | ✗ | ✓ |
| MCP / AI-agent surface | ✗ | ✗ | ✗ | ✗ | ✓ |
| Single-binary distribution with embedded project | ✗ | ✗ | ✗ | ✗ | ✓ |
| Static `--check` / `--scan` / `simulate` | ✗ | ✗ | ✗ | ✗ | ✓ |
| Sandboxed test framework built in | ✗ | ✗ | ✗ | ✗ | ✓ |

When you only need a cross-platform runner without the other surfaces, Just or Task are fine. perch shines when you want any combination of: capability gating, agent integration, single-binary distribution, web UI, static analysis, or strict cross-platform behavior.

### Is perch ready for production?

Yes, for the use cases documented here. Limits are listed in [Honest limits](#honest-limits). The DSL surface is still evolving — major releases will document breaking changes in `CHANGELOG.md`.

### How do I keep my `.perch` file maintainable as it grows?

Three patterns:

1. **Split by domain into multiple files**, then `import "ops/redis.perch"` etc.
2. **Lift repeated bodies into `template`s** in a shared `_lib.perch`.
3. **Use `private` aggressively** to hide internal helpers (`_set_kube_context`, `_alert`) from the visible CLI surface.

Aim for ≤ 200 lines per top-level `.perch`, ≤ 30 lines per command body. Anything beyond that — refactor or push the heavy lifting into a WASM module.

### How do I handle long-running services?

Use the `detached` + `on_signal` pattern (see [Patterns & idioms](#patterns-and-idioms) #18). For real production daemons, run perch under systemd / supervisord with `--max-runtime=0` (no cap) and the audit log going to a real log aggregator.

### Can I extend perch with my own ops?

For now: no, not without forking. The op set is fixed at compile time of the perch binary. **The intended extension point is `wasm_run`** — write your custom logic in any WASM-targeting language, embed it via `bundle`, invoke from a `wasm_run` block. This gives you full language flexibility without modifying perch.

### Can perch call itself / other perch programs?

Yes, via `shell "perch -f other.perch other_command"`. For an in-process equivalent, use `import` + `run`. The fat-binary case is more interesting: a built binary can `shell "${script_path} other_command"` to re-invoke itself (since `${script_path}` is the binary's own location when running embedded).

### How do I version-pin perch in CI?

```yaml
- uses: actions/setup-go@v5
- run: go install github.com/luowensheng/perch@v0.5.0
```

Pin to a tagged release. Major-version bumps may break the DSL surface; minor + patch are non-breaking by policy.

### What's the largest `.perch` file you'd recommend?

Past ~500 lines, split into multiple files via `import`. Past ~3000 lines total across imports, you're probably building something that should be a real Go application calling perch via its embedded program (the `infra/interpreter` package is consumable as a library). Most users never approach those limits.

### Can I use perch as a library from another Go program?

Yes — `infra/interpreter` is a public Go package. Load a `domain.Program` via `infra/capyloader.Load`, build an interpreter, dispatch commands. The whole CLI is a thin shell over this layer. No stable API guarantee yet — pin a tag if you embed.

### Are MCP tools generated automatically?

Yes. `perch-mcp -f file.perch` introspects every visible command and exposes it as an MCP tool with a JSON-schema arg surface derived from the `arg ... end` blocks. No code generation, no schema file to maintain. Add a command → it's instantly callable by Claude / Cursor / Zed.

### How do I roll back a deployed binary?

`perch --build` is deterministic: same `.perch` + same bundle → same binary bytes (modulo timestamps in the underlying Go binary). Keep the prior binary version available (e.g. `myapp-1.2.0`, `myapp-1.3.0`); rollback = `ln -sf myapp-1.2.0 /usr/local/bin/myapp`. The audit log in `~/.cache/perch/deploys.ndjson` (if you used the [multi-env deployer pattern](#walkthrough-8-multi-environment-deployer)) tells you which version was running when.

---

## Honest limits

What perch does **not** do (be precise about scope so you can decide):

| Not supported | What to use instead / when it lands |
|---|---|
| Kernel-level process isolation (namespaces, cgroups) | Use Docker if you need that. perch's `wasm_run` is by-construction isolation; `shell` is plain host process. |
| Memory / CPU resource limits | Roadmap. Today: `--max-runtime` for wall-clock; per-op deadlines via `timeout` block. |
| Streaming HTTP from WASM modules | Roadmap. Today: 32 MB buffered response, GET only, no custom headers. |
| Sockets from WASM modules | Blocked on WASI Preview 2 in wazero. |
| Multi-stage builds | Bundle is one tar. Pre-process artifacts before `--build` if you need staging. |
| Container registry / image pull | None. Distribute the binary itself (scp / GitHub Release / curl). |
| Plugins implemented in non-WASM languages | All WASM-targeting languages work (Go, Rust, TinyGo, Zig, C, AssemblyScript). Native plugins via `shell` work but have no isolation. |
| Cross-compile via `perch --build` (target ≠ host) | Roadmap. Today: `--build` produces a binary for the host's OS/arch; for cross-targeting, build perch itself with `GOOS`/`GOARCH` set. |
| External contributions to this repo | See [CONTRIBUTING.md](https://github.com/luowensheng/perch/blob/main/CONTRIBUTING.md). Forking is encouraged. |

---

## Quick reference card

### File structure

```perch
name "..." about "..." version "..."     # identity
globals NAME = VALUE ... end             # shared bindings
bundle include "PATH" [as NAME] ... end  # embed at --build time
template NAME arg X string … do … end    # reusable body
import "PATH" [as NAMESPACE]             # multi-file projects
command NAME description "..." [modifiers]
    arg NAME type T default V end
    do
        OP ARGS
        let X = OP ARGS                  # capture result
        if EXPR ... end                  # control flow
        BLOCK_OP ARGS ... end            # parallel / retry / etc.
        call TEMPLATE "value"
        run OTHER_COMMAND "-arg=value"
        fail "msg"
    end
end
catch unknown ... end                    # fallback for unknown verbs
```

### Common CLI flags

```
-f FILE                          load this .perch file
--check                          validate (no execution)
--scan                           capability + risk audit
simulate [CMD] [--sim-*]         what-if analyzer
--server [--port N]              web UI
--shell                          REPL
--build [-o OUT] [--include PATH] portable binary
test [--filter PAT] [-v]         run tests
--dry-run                        walk the plan, skip execution
--trace[=PATH]                   live op stream
--audit PATH.ndjson              JSON event log
--report                         post-run span tree
--no-shell                       refuse shell ops
--no-network                     refuse network ops
--no-subprocess                  refuse subprocess ops
--no-write                       fs is read-only
--no-shell-metachars             refuse | && ; etc in shell
--env K,K,...                    env allowlist
--allow-bin N,N,...              shell argv[0] allowlist
--allow-host H,H,...             network host allowlist
--max-runtime SECS               wall-clock cap
```

### Mental shortcuts

- **A perch program is a CLI by default.** Everything else (web UI, MCP, binary) renders from the same file.
- **Sandboxes nest.** Outer `--no-shell` + inner `sandbox` blocks compose; inner can only tighten.
- **`wasm_run` is the strict lane.** `shell` is best-effort.
- **State doesn't survive across commands** unless you write to a file or use `cache`. Commands are stateless transactions.
- **`let` is the only assignment.** Globals are read-only after parse.

---

## Where to next

- [Recipes (22 ready-to-run)](recipes.md) — copy-paste solutions for Postgres / Redis / observe / aistack / etc.
- [Language reference](language.md) — exhaustive grammar
- [Op catalog](op-reference.md) — every built-in op
- [Tutorials](tutorials/01-replace-your-makefile.md) — three step-by-steps
- [Simulate guide](simulate.md) — multi-scenario what-if analysis
- [Web UI guide](web-ui.md) — non-dev surface
- [WASM walkthroughs](wasm-walkthroughs.md) — five real-world workflows
- [The OS analogy](os-in-a-program.md) — the deeper claim

Bug reports → [open an issue](https://github.com/luowensheng/perch/issues/new?template=bug.yml). Ideas / show-and-tell → [Discussions](https://github.com/luowensheng/perch/discussions).

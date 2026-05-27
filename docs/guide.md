# The complete perch guide

> **Everything you need to get started AND ship a serious project with perch.** One document, end-to-end. Read in 30–45 minutes; skim the index to jump.

---

## Contents

1. [TL;DR — first run in 60 seconds](#tldr-first-run-in-60-seconds)
2. [Install](#install)
3. [The mental model](#the-mental-model)
4. [Anatomy of a `.perch` file](#anatomy-of-a-perch-file)
5. [Commands — args, modifiers, defaults](#commands-args-modifiers-defaults)
6. [Ops — the language vocabulary](#ops-the-language-vocabulary)
7. [Block ops — `if`, `parallel`, `retry`, `timeout`, `with_env`, `with_cwd`, `cache`, `sandbox`, `for_each`](#block-ops)
8. [Templates — code reuse](#templates-code-reuse)
9. [Imports — multi-file projects](#imports-multi-file-projects)
10. [The capability model — restrictions, allowlists, audit](#the-capability-model)
11. [Bundles — ship one binary](#bundles-ship-one-binary)
12. [WASM modules — sandbox by construction](#wasm-modules-sandbox-by-construction)
13. [Testing — `perch test`](#testing-perch-test)
14. [Pre-flight — `--check`, `--scan`, `simulate`](#pre-flight-check-scan-simulate)
15. [Observability — `--trace`, `--audit`, `--report`](#observability-trace-audit-report)
16. [The five surfaces — CLI, web UI, REPL, MCP, binary](#the-five-surfaces)
17. [Walkthrough 1 — Replace your Makefile](#walkthrough-1-replace-your-makefile)
18. [Walkthrough 2 — Ship a self-installing tool](#walkthrough-2-ship-a-self-installing-tool)
19. [Walkthrough 3 — Internal ops backend (web UI + MCP)](#walkthrough-3-internal-ops-backend-web-ui-mcp)
20. [Walkthrough 4 — Plugin host with WASM](#walkthrough-4-plugin-host-with-wasm)
21. [Walkthrough 5 — CI gate with `simulate`](#walkthrough-5-ci-gate-with-simulate)
22. [Distribution — getting your binary to users](#distribution-getting-your-binary-to-users)
23. [Honest limits](#honest-limits)
24. [Quick reference card](#quick-reference-card)

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

# 3. Bundle — what gets embedded into the fat binary at `perch --build` time
bundle
    include "./modules"             # whole dir
    include "./policy.wasm" as policy_wasm   # one file, alias for wasm_run
end

# 4. Templates — reusable parameter-substitution stamps (optional)
template ensure_dir
    arg path string
    do
        if not exists "${path}"
            mkdir "${path}"
        end
    end
end

# 5. Commands — the typed verbs
command build
    description "Compile myapp"
    arg target
        type string
        default "${os}"
        description "Target OS"
    end
    do
        call ensure_dir path:"${BUILD_DIR}/${target}"
        shell "go build -o ${BUILD_DIR}/${target}/myapp ./cmd/myapp"
    end
end

# 6. Catch — runs when the user types an unknown command (optional)
catch unknown
    description "Forward unknown verbs to git, like `gh foo` would"
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
        shell "pkill -f 'cmd/server'"
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
run other_command  arg1:"x"  arg2:"y"
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
shell "echo hi"                          # bare op with positional string
mkdir "${BUILD_DIR}/${target}"          # ditto
let n = file_size "./build/out"         # let-capture: result bound to ${n}
let body = http_get "https://api.x/health"
print "got ${body}"
```

Some block ops take a named arg + a body:

```perch
parallel max=3                           # block op with kwarg
    shell "go test ./a"
    shell "go test ./b"
    shell "go test ./c"
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

## Block ops

### `if EXPR ... end` — unified conditional

```perch
if os == "linux"
    shell "apt-get install -y jq"
end

if has_bin "kubectl"
    shell "kubectl get pods"
end

if not has_bin "docker"
    fail "docker is required"
end

if exists "./Cargo.toml"
    shell "cargo build --release"
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
    shell "go test ./a"
    shell "go test ./b"
    shell "go test ./c"
    shell "go test ./d"
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
    shell "make integration-test"
end
```

### `with_env KEY1=val1,KEY2=val2 ... end`

Overlay env vars for the body:

```perch
with_env DEBUG=1,GOOS=linux
    shell "go build ./cmd/myapp"
end
```

### `with_cwd "PATH" ... end`

Run body in another directory:

```perch
with_cwd "./subproject"
    shell "go test ./..."
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
    let cost = shell_output "./expensive-script.sh"
end
```

### `for_each NAME in LIST ... end`

Loop body N times:

```perch
for_each region in "us-east-1,us-west-2,eu-west-1"
    shell "deploy --region=${region}"
end
```

### `wasm_run` — WebAssembly under WASI

See [WASM modules](#wasm-modules-sandbox-by-construction) below.

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
        call require_bin name:"docker"
        call require_bin name:"docker-compose"
        shell "docker-compose up -d"
    end
end

command down
    do
        call require_bin name:"docker-compose"
        shell "docker-compose down"
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
        shell "./scripts/seed-data.sh"
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
        call suite_assert got:"${result}" want:"héllo"
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

### `--scan` (capability audit)

Static analysis: what capabilities does this program need? What risks are there?

```sh
perch --scan -f deploy.perch
```

Output: needed capabilities (shell? subprocess? network hosts? write roots? env vars?), risk findings (`sudo` usage, `${var}` interpolated into shell without validation, `${proxy_args}` forwarded to shell), and a recommended hardened invocation.

Always run before executing a `.perch` file you didn't write.

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
        call ensure_clean dir:"${OUT_DIR}/${os}-${arch}"
        shell "go build -o ${OUT_DIR}/${os}-${arch}/${APP_NAME} ./cmd/${APP_NAME}"
        let size = file_size "${OUT_DIR}/${os}-${arch}/${APP_NAME}"
        print "✓ built ${size} bytes"
    end
end

command release
    description "Cross-compile for darwin/linux/windows × amd64/arm64"
    do
        parallel max=4
            with_env GOOS=darwin,GOARCH=arm64
                shell "go build -o ${OUT_DIR}/darwin-arm64/${APP_NAME} ./cmd/${APP_NAME}"
            end
            with_env GOOS=darwin,GOARCH=amd64
                shell "go build -o ${OUT_DIR}/darwin-amd64/${APP_NAME} ./cmd/${APP_NAME}"
            end
            with_env GOOS=linux,GOARCH=amd64
                shell "go build -o ${OUT_DIR}/linux-amd64/${APP_NAME} ./cmd/${APP_NAME}"
            end
            with_env GOOS=windows,GOARCH=amd64
                shell "go build -o ${OUT_DIR}/windows-amd64/${APP_NAME}.exe ./cmd/${APP_NAME}"
            end
        end
        print "✓ all 4 targets built"
    end
end

command test
    description "Run the test suite"
    do
        shell "go test -race ./..."
        if exists "./integration"
            shell "go test -tags=integration ./integration/..."
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
        call ensure_dir path:"${install_dir}"

        if not exists "${install_dir}/.installed"
            print "→ extracting source to ${install_dir}"
            bundle_extract "${install_dir}"

            print "→ creating venv"
            shell "python3 -m venv ${install_dir}/.venv"

            print "→ installing dependencies"
            shell "${install_dir}/.venv/bin/pip install -r ${install_dir}/requirements.txt"

            touch "${install_dir}/.installed"
        end

        # Drop a launcher into ~/.local/bin
        call ensure_dir path:"${home_dir}/.local/bin"
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
        shell "ssh ops@${host} 'systemctl restart ${service}'"
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
        shell "kubectl get pods -n prod"
    end
end

command tail_logs
    description "Stream logs for a service"
    arg service
        type string
        description "Service name"
    end
    do
        shell "kubectl logs -f deployment/${service} -n prod"
    end
end

# Hidden helper — not visible to CLI / MCP / web UI
command _kube_context
    private
    do
        shell "kubectl config use-context prod"
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
            run run_plugin plugin:"tax"
            run run_plugin plugin:"discount"
            run run_plugin plugin:"shipping"
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
        call TEMPLATE arg:value
        run OTHER_COMMAND arg:value
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

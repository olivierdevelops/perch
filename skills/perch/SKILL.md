---
name: perch
description: Use this skill when the user wants to create, edit, or scaffold a `commands.perch` file for the perch task runner (https://github.com/olivierdevelops/perch). Trigger on: any mention of `perch`, `.perch`, "commands.perch", or requests to convert a Makefile / Justfile / shell scripts / bin folder / CI workflow into perch; also when the user asks how to ship a CLI tool as a single binary via `perch --build`. The deliverable is correct perch syntax — do not improvise keywords or ops.
---

# perch authoring guide

You are writing or modifying a `commands.perch` file. perch is a cross-platform task runner whose DSL is defined by [capy](https://olivierdevelops.github.io/capy). The grammar is small and rigid; do not invent new keywords or ops. Stick to what's documented below.

## File skeleton

Every `commands.perch` follows the same shape:

```capy
#!/usr/bin/env perch            # optional but recommended — lets the file be `./run` as a script

name    "PROGRAM_NAME"          # required
about   "PROGRAM_DESCRIPTION"   # optional but recommended
version "X.Y.Z"                 # optional but recommended

import "./shared.perch"         # optional — pulls in another file's commands (flat)
import "./aws.perch" as aws     # optional — namespaced: callable as aws.command_name

KEY = "value"                    # bindings shared by every command — declared bare at
NUMERIC = 42                     #   top level (there is NO `globals` block). Reference
FLAG = true                      #   them in `requires` by name, e.g. `write BUILD_DIR`.

requires                         # the manifest — declares every external resource the
    bin   "git"                  #   file touches: bins (+ optional hash pins, or a path
    bin   "./bin/tool" as tool   #   with a handle: `bin "PATH" as NAME`), env vars,
    env   "HOME"                 #   hosts, filesystem read/write scopes, OS/arch. Every
    host  "api.github.com"       #   external op verifies it before running; undeclared
    read  "./config"             #   access errors. A missing block = empty manifest
    write "./build"              #   (pure ops only). No version checking — pin a hash.
end

command NAME
    # config region — declarative metadata
    description "what it does"

    arg NAME
        type string                # required: string / int / float / bool
        default "value"            # optional: literal value (omit to make required)
        description "..."          # optional: shown in --help
        optional                   # optional: no default but may be omitted
        index 0                    # optional: bind to positional index
    end

    # other config modifiers as needed (see below)

    do
        # body region — ops run when the command is invoked
        OP ARGS...
    end
end

catch unknown                    # optional fallback
    description "..."
    do
        print "no command named ${unknown}"
        exit 1
    end
end
```

## Hard rules — do not break these

1. **Config statements must appear before `do`. Ops must appear inside `do … end`.** They never mix. Putting `arg foo ... end` inside a `do` block is a hard syntax error.
2. **Three string delimiters: `"..."`, `'...'`, `` `...` `` — all interchangeable, all raw, all interpolate `${name}`. Pick whichever delimiter doesn't appear in your content; no backslash escapes are interpreted.** This is the key insight for JSON / SQL / shell-with-quotes: use a delimiter that doesn't clash. Examples:

```capy
# JSON — use single quotes (content has " but no ')
let body = format '{"order_id":"${order_id}","amount":${amount}}'

# SQL with quoted literals — use backticks (content has both " and ')
shell `psql -c "SELECT * FROM t WHERE name='${name}'"`

# Plain text — double quotes are fine
print "hello ${user}"
```

There are no `\n` / `\t` / `\"` escape sequences. If you need a literal newline in a string, write a multi-line string (a real `\n` byte in the source — see `write_file` examples). If you need to embed a quote that matches your delimiter, switch delimiters.
3. **Op arguments are positional, not named.** `cp "src" "dst"` is right; `cp src:"a" dst:"b"` is wrong.
4. **`${name}` is the only runtime interpolation form.** Don't use Go's `{{.name}}` or shell's `$name`. perch parses `${name}` after capy is done.
5. **One op per line. Block ops — the unified `if EXPR ... end` form (comparisons, predicates, truthy/falsy) — wrap a nested body terminated by `end`. See the dedicated section below.**
6. **`let NAME = OP ARG` form is for capturing return values. `let` is NOT a standalone op — it must precede an op-call expression with 0, 1, or 2 args.**

## The full config vocabulary (config region, before `do`)

| Form | Effect |
|---|---|
| `description "x"`                 | Help text |
| `arg NAME ... end`                | Typed CLI argument; properties are labelled inner lines (see below) |
| `private`                         | Hide from CLI; only callable by name from another command |
| `detached`                        | Don't wait on detached spawns |
| `proxy_args`                      | Skip arg parsing; argv → `${proxy_args}` |
| `require_os "darwin" "linux"`     | Refuse to run on other OSes (one call per OS or multiple values) |
| `require_arch "arm64"`            | Same idea for arch |
| `dir "./subdir"`                  | cwd for the body |
| `on_signal HANDLER`               | Another command name; runs on SIGINT/SIGTERM |
| `env KEY "value"`                 | Per-command env var |

### `arg NAME ... end` inner fields

| Form              | Effect |
|-------------------|--------|
| `type TYPE`       | **Required.** TYPE ∈ `string` / `int` / `float` / `bool` |
| `default VALUE`   | Default literal; presence makes the arg optional |
| `description "x"` | Help text shown in `--help` (uses the same `description` keyword as the command) |
| `optional`        | Arg may be omitted even without a default |
| `index N`         | Bind to positional index N (instead of a `-NAME` flag) |
| `rest`            | **Variadic.** Collects every remaining positional arg into a newline-joined string. Must be the last arg, type `string`, no default, requires `index N`. Companion `${NAME_count}` int binding. Iterate with `for_each "${NAME}" item ... end`. |

## The op catalog (body region, inside `do`)

### Process / I/O
- `print MSG`, `println MSG`, `eprintln MSG`
- `BIN arg…` (bare) — **the normal way to run a subprocess.** A bare declared bin runs shell-free with structured argv (`git commit -m "fix it"`, `docker ps`). Each token is one argv slot. `BIN` must be a declared `bin`. Chain with `&&`/`||`/`;`; wire stages with `pipe … end`.
- `exec BIN arg…` — explicit form; needed only when the bin name collides with a built-in op (`exec rm`, `exec mkdir`, `exec chmod`). Captures work bare: `let h = git rev-parse HEAD`.
- `shell CMD` — runs in bash / cmd.exe. Deprecated; keep only for genuine shell needs (pipes, `${proxy_args}` word-splitting, `awk`/`sed` one-liners)
- `shell_detached CMD` — fire-and-forget
- `fail MSG` — exits non-zero
- `exit N`
- `sleep SECONDS`
- `NAME args…` — invoke another command (or expand a template) by its bare name. No `run`/`call` keyword; names are globally unique so it's unambiguous (`exec NAME` forces the subprocess reading)
- `list_commands` — prints the visible command list

### File system
- `mkdir PATH`, `cp SRC DST`, `mv SRC DST`, `rm PATH`, `cd PATH`
- `chmod PATH "0755"`, `touch PATH`
- `write_file PATH CONTENT`

### Compression / archives
- `gzip SRC DST`, `ungzip SRC DST`
- `tar_create SRC_DIR DST`, `tar_extract SRC DST`
- `zip_create SRC_DIR DST`, `zip_extract SRC DST`

### HTTP
- `download URL DST`

### Iteration — `for_each VALUE NAME ... end`

Iterates a **newline-separated** string, binding each non-empty line to `NAME` for the body. Pairs naturally with `rest` args and ops that return newline-joined lists (`glob`, `list_dir`, `interfaces`, `dns_lookup`, `read_file` piped through `split` etc.).

```capy
command install_all
    arg packages
        type string
        index 0
        rest
    end
    do
        for_each "${packages}" pkg
            pkg_install "${pkg}"
        end
    end
end
```

Empty input is a no-op (body never runs). The previous binding for `NAME` (if any) is restored after the loop, so `for_each` blocks compose cleanly.

### Conditionals — the unified `if EXPR ... end`

Every conditional is the same `if … end` block. The expression takes one of these shapes:

| Shape | Example | When it runs |
|---|---|---|
| `NAME == LITERAL` | `if os == "linux"` | bindings[NAME] equals the literal |
| `NAME != LITERAL` | `if mode != "prod"` | bindings[NAME] differs |
| `NAME > NUMBER` | `if COUNT > 5` | numeric > |
| `NAME < NUMBER` | `if size < 0` | numeric < |
| `NAME >= NUMBER` | `if size >= 1024` | numeric >= |
| `NAME <= NUMBER` | `if retries <= 3` | numeric <= |
| `NAME` | `if has_bin` | bindings[NAME] is truthy (non-empty / non-zero / non-"false") |
| `not NAME` | `if not has_bin` | bindings[NAME] is falsy |
| `FUNC ARG` | `if exists "./bin"` | calls FUNC(ARG); body runs if return value is truthy |

`os` and `arch` are **auto-bound** at command start — but they're not the only ones. See the **Auto-bound variables** section below.

For complex predicates — e.g. comparing a captured value — capture first then compare:

```capy
let size = file_size "./bin"
if size > 1000000
    print "big"
end
```

## Auto-bound variables (always available as `${name}` — no declaration)

Every command starts with these bound. They're the difference between a cross-platform script and a script that "happens to also work on macOS." Reach for them before resorting to `shell` tricks.

**OS / arch / convenience flags**
- `os` — `"darwin"` / `"linux"` / `"windows"`
- `arch` — `"amd64"` / `"arm64"` / …
- `is_windows`, `is_macos`, `is_linux`, `is_unix` — booleans (`if is_windows`)
- `is_arm64`, `is_amd64` — booleans
- `cpu_count`, `pid`, `now_unix`

**Path conventions** (the things that differ per OS)
- `path_sep` — `/` or `\`
- `path_list_sep` — `:` or `;` (the PATH separator)
- `exe_ext` — `.exe` on Windows, `""` elsewhere
- `null_device` — `/dev/null` or `NUL`
- `shell_name` — `bash` or `cmd`

**Standard directories** (OS-correct, never hardcode these)
- `home` / `home_dir` — user's home dir
- `config_dir` — `~/.config` / `%APPDATA%` / `~/Library/Application Support`
- `cache_dir` — OS user cache dir
- `data_dir` — OS user data dir
- `temp_dir` — OS temp dir

**The binary and the script itself**
- `exe_path` — absolute path to the running perch binary (or built binary)
- `exe_dir` — directory of that binary
- `exe_name` — just the file name
- `script_path` — absolute path of the loaded `.perch` file (empty when embedded)
- `script_dir` — directory containing it

**Identity**
- `user` — current username
- `uid` — user id (unix)
- `hostname` — host name

### Cross-platform install / build / uninstall ops

These cover the things every install script otherwise re-implements with shell glue. They work identically on macOS / Linux / Windows.

**Binary discovery**
- `which BIN` (string) — full path on PATH or `""`
- `has_bin BIN` (bool) — `if has_bin "python3" ... end`
- `bin_version BIN [FLAG]` — runs `BIN --version` (or `BIN FLAG`); empty on failure

**Path manipulation** (cross-platform — never hand-concat with `/`)
- `path_join A B [C ...]`, `path_dir P`, `path_base P`, `path_ext P`
- `path_abs P`, `path_clean P`, `path_rel BASE TARGET`
- `path_with_ext P EXT` (rewrites extension), `is_abs P`
- `to_slash P` / `from_slash P` (swap `\` ↔ `/`)
- `expand_path "~/x"` — expands `~` and env vars

**PATH and shell rc**
- `path_contains DIR` (bool) — is DIR on $PATH
- `shell_rc_path` — best-guess `~/.zshrc` / `~/.bashrc` / fish config
- `add_to_path DIR` — idempotent; appends to shell rc if missing; prints `setx` instructions on Windows
- `link_into_path SRC DIR` — symlinks SRC into DIR (copies on Windows)

**Package manager** (auto-detects brew/apt/dnf/pacman/apk/zypper/winget/choco/scoop)
- `detect_pkg_mgr` — returns the name, or `""`
- `pkg_install NAME` / `pkg_uninstall NAME` / `pkg_installed NAME`

**System probes**
- `is_admin` (bool) — euid 0 / `net session` on Windows
- `is_ci` (bool) — checks CI, GITHUB_ACTIONS, GITLAB_CI, etc.
- `is_tty` (bool)
- `os_version` — best-effort version string (`sw_vers` / `uname -r` / `ver`)

**Network probes**
- `port_free PORT` (bool), `find_free_port` (int)
- `wait_for_port HOST PORT TIMEOUT_SEC` (bool)
- `wait_for_url URL TIMEOUT_SEC` (bool)
- `http_status URL` (int) — HEAD probe
- `local_ip`, `public_ip`, `mac_address`, `interfaces`

**Filesystem helpers for install scripts**
- `ensure_dir PATH` — mkdir -p, returns abs path
- `make_executable PATH` — chmod +x (no-op on Windows)
- `copy_dir SRC DST` — recursive copy
- `append_file PATH CONTENT`, `append_line PATH LINE`
- `ensure_line_in_file PATH LINE` — idempotent; returns true if added
- `replace_in_file PATH OLD NEW`
- `backup_file PATH` — copies to PATH.bak
- `glob PATTERN`, `list_dir PATH`
- `symlink TARGET LINK`, `read_link PATH`
- `mktemp_dir [PREFIX]`, `mktemp_file [PREFIX]`

**Process helpers**
- `try_shell CMD` (bool) — like `shell` but returns true/false instead of erroring
- `shell_in DIR CMD` — like `shell` but with explicit cwd
- `process_running NAME` (bool), `kill_by_name NAME`

**Integrity**
- `verify_sha256 PATH HASH` (bool)

### Capturable ops (use via `let X = OP ARG`)

**0-arg:** `hostname`, `cwd`, `get_os`, `get_arch`, `home_dir`, `temp_dir`, `cache_dir`, `app_data_dir`, `pid`, `user`

**1-arg, return string:** `upper`, `lower`, `trim`, `capitalize`, `md5`, `sha1`, `sha256`, `base64_encode`, `base64_decode`, `hex_encode`, `hex_decode`, `url_encode`, `url_decode`, `crc32`, `read_file`, `shell_output`, `http_get`, `http_delete`, `md5_file`, `sha1_file`, `sha256_file`, `json_parse`, `json_stringify`, `unix_to_iso`

**1-arg, return int/bool:** `length` (int), `file_size` (int), `exists` (bool), `is_dir` (bool), `is_file` (bool)

**2-arg:** `contains`, `has_prefix`, `has_suffix`, `replace`, `split`, `join`, `repeat`, `format`, `regex_match`, `regex_replace`, `http_post`, `http_put`, `port_check`, `dns_lookup`, `json_get`, `now` (1-arg)

## Interpolation rules

- `${name}` resolves in this order: command args → `let` captures → top-level bindings → per-command `env` → host process environment.
- Unknown names produce a runtime error. Giving the arg a `default` is the cleanest fix.
- UPPER_SNAKE_CASE top-level bindings are also exported as environment variables to `shell` calls automatically.
- `${HOME}`, `${USER}`, `${PATH}` work out of the box (they fall through to host env).

## When the user asks for something

Map the request to perch concepts:

| User says | Use |
|---|---|
| "build for multiple platforms" | a `release` command that invokes `build "-target=..."` three times (a bare command name; args are quoted, CLI-style) |
| "install dev tools cross-platform" | `if os == "darwin"` / `if os == "linux"` blocks running brew / apt / choco |
| "I want a help text on this" | `description "..."` in the config region |
| "make this command not show up in --help" | `private` modifier |
| "compute a checksum" | `let h = sha256_file "..."` |
| "fetch a URL and save it" | `download "url" "dst"` (no `let` needed) |
| "fetch JSON, read a field" | `let body = http_get "url"` then `let v = json_get "${body}" "a.b.c"` |
| "compress this folder" | `tar_create "src" "out.tar.gz"` |
| "skip a step if a file already exists" | `if exists "PATH" ... end` — or `let e = exists "PATH"; if not e ... end` for the inverse |
| "wrap / extend an existing tool, forwarding unknown commands" | `catch passthrough ... shell "REAL_TOOL ${proxy_args}"` — see Language Reference for the passthrough pattern |
| "ship this as a binary" | `perch --build -f commands.perch -o NAME` |
| "serve a web UI" | `perch --server` |
| "play interactively" | `perch --shell` |

## Anti-patterns — DO NOT write these

```capy
# ❌ Don't use Go-template syntax — perch uses ${name} not {{.name}}
print "hello {{.name}}"

# ❌ Don't put ops in the config region
command x
    print "wrong"        # this is an op; must be inside `do`
    do
        ...
    end
end

# ❌ Don't put config in the body region
command x
    do
        description "wrong"   # config; must be before `do`
        ...
    end
end

# ❌ Don't invent ops. If the catalog doesn't have it, use `shell "..."`.
do
    fetch_api "https://..."           # not an op
    http_get "https://..." > file     # ops don't pipe
end

# ❌ Don't omit description
command x
    do
        ...
    end
end
# (it works, but `--help` will be empty and the user won't know what x does)

# ❌ Don't use backticks for multi-line strings in op args
do
    shell `multi
line
script`               # wrong
    shell "multi\nline\nscript"  # right
end
```

## Worked example — Go project

```capy
name    "myapp"
about   "Build, test, release myapp"
version "0.1.0"

BIN_DIR   = "./bin"
APP_NAME  = "myapp"
MAIN_PKG  = "./cmd/myapp"

requires
    bin   "go"
    write BIN_DIR
end

command build
    description "Compile for one target"

    arg target
        type string
        default "darwin"
        description "Target OS (darwin/linux/windows)"
    end

    do
        mkdir "${BIN_DIR}/${target}"
        shell "GOOS=${target} go build -ldflags='-s -w' -o ${BIN_DIR}/${target}/${APP_NAME} ${MAIN_PKG}"
        let size = file_size "${BIN_DIR}/${target}/${APP_NAME}"
        print "✓ built (${size} bytes)"
    end
end

command release
    description "Cross-compile all three"
    do
        build "-target=darwin"
        build "-target=linux"
        build "-target=windows"
    end
end

command test
    description "Run tests"
    do
        shell "go test -race ./..."
    end
end

command ci
    description "What CI runs"
    do
        test
        release
    end
end

catch unknown
    do
        print "Unknown command: ${unknown}"
        list_commands
        exit 1
    end
end
```

## Piping a `.perch` file from stdin

`perch -f -` reads the perch source from stdin instead of from a file:

```sh
cat script.perch | perch -f - hello
curl -fsSL https://.../script.perch | perch -f - run
git show HEAD:commands.perch | perch -f - --check
```

**Stdin input is untrusted by default** — shell, subprocess, network, write, and host env vars are all disabled. Grant capabilities with `--allow-shell`, `--allow-subprocess`, `--allow-network`, `--allow-write`, and `--env A,B,C`. Pass `--trust-stdin` to skip the default entirely (when piping a `.perch` file you wrote yourself). File input (`-f file.perch`) is unchanged — the deny-by-default applies only to stdin since that's where untrusted scripts arrive. `${script_path}` is `"-"` in this mode.

## Preview before running

- **`perch --dry-run <cmd>`** — walks every op, prints kind + interpolated args, skips the handler. Capture vars become `""` so `${x}` still resolves downstream.
- **`perch --ask <cmd>`** — interactive step-through. For each op: `y` runs it, `n` skips, `a` runs this op then everything else without further asking, `q` quits the command.

Both work for any command and stack with the restriction flags: `perch --no-shell --ask deploy` lets you preview ops AND have the runtime refuse shell calls.

## `requires` — declare what the file needs from outside (the philosophy)

perch's security model is **declare every external resource**. Add a top-level `requires` block listing the bins, env vars, hosts, and filesystem paths the file touches. When present, **every external op verifies the manifest immediately before it runs** — an undeclared shell bin, HTTP host, `get_env` name, or filesystem path *errors*. Files without a `requires` block keep ambient access (the block is opt-in today).

```capy
requires
    bin   "git"                          # bins used by shell / subprocess ops
    bin   "docker" optional              # absence is not fatal
    bin   "kubectl"
        hash "sha256:abc123…"            # OPTIONAL: pin exact bytes (read-only, never executed)
    end
    env   "HOME"                         # env vars get_env / set_env may touch
    host  "api.github.com"               # hosts http_* / download / dns_lookup may reach
    host  "*.amazonaws.com"
    read  "./config"                     # filesystem read scope
    write "./build"                      # filesystem write scope (write implies read)
    os    "linux"                        # host OS allowlist (one value per line)
    os    "darwin"
    arch  "amd64"                        # host arch allowlist (one value per line)
    arch  "arm64"
end
```

Rules to follow when authoring:

- **Declare a `bin` for every program a `shell` op runs** (its first token) and for subprocess ops (`pkg_install`, `bin_version`, …). Undeclared → `bin_not_declared`.
- **Declare a `host` for every URL** an `http_*` / `download` / `dns_lookup` / `wait_for_url` op targets. Undeclared → `host_not_declared`.
- **Declare an `env`** for every name `get_env` / `set_env` / `env_has` uses. Undeclared → `env_not_declared`.
- **Declare `read` / `write` roots** for every path a filesystem op touches (`mkdir`, `write_file`, `read_file`, `cp`, …). Outside the roots → `read_not_declared` / `write_not_declared`.
- **There is NO version checking** in `requires` — it would require executing the binary (which a trojaned binary can lie to). Pin a `hash` instead, or check a version *inside* a command with `shell_output` + `version_extract` / `assert_version` under the declared `shell`/`bin` capability.
- `perch --check` statically flags undeclared *literal* usage; the runtime gate catches interpolated usage. Both compose with the operator `--no-*` flags (intersection — neither side can grant more than the other allows).

When you write a file with a `requires` block, make sure every external op you emit has a matching declaration, or `perch --check` will reject it.

## Restriction flags

`perch --no-*` flags disable groups of ops globally. Each flag names what it disables — composable, additive.

| Flag | Blocks |
|---|---|
| `--no-shell` | `shell`, `shell_output`, `shell_detached`, `shell_in`, `try_shell` |
| `--no-subprocess` | `pkg_install`, `pkg_uninstall`, `kill_by_name`, `process_running` |
| `--no-network` | every `http_*`, `download`, `dns_lookup`, `port_*`, `wait_for_*`, `public_ip`, `local_ip`, `mac_address`, `interfaces` |
| `--no-write` | every FS-mutation op (`write_file`, `append_*`, `cp`, `mv`, `rm`, `mkdir`, `chmod`, `touch`, archive create/extract, `symlink`, …) |

```sh
perch --no-shell --no-network --no-write deploy
perch --restrictions                # discover the full lists
```

A blocked call returns `op "X" is disabled by --no-Y`. When any restriction is active, perch prints a `🔒 security: ...` banner.

## HTTP destination control (SSRF + host allowlist)

Every HTTP op (`http_get`, `http_post`, `download`, `http_status`) is gated. Default-on, no flag required:

- Refuses requests + redirects to loopback / link-local / RFC 1918 / IPv6 ULA / unspecified IPs (closes the AWS-metadata SSRF, localhost pivot, internal-network pivot).
- Refuses https → http redirect downgrade.
- Caps at 5 redirect hops.
- Validates every A/AAAA record on multi-A responses (DNS-rebinding defense).

Layer **`--allow-host HOST[,HOST...]`** for a strict allowlist. Initial URL AND every redirect destination must match. Wildcards: `*.example.com` matches single-label prefix (`api.example.com` ✓, `a.b.example.com` ✗).

```sh
perch --allow-host api.github.com,*.docker.io,registry.npmjs.org deploy
```

Opt-out flags for the genuine cases: `--allow-private-ips`, `--allow-scheme-downgrade`, `--max-redirects N`, `--no-redirects`.

## Closing the subprocess escape hatch

Restrictions only fence perch's *own* op dispatch. A `shell` subprocess can ignore `--no-network`/`--env` by calling `curl` / reading `$SECRET` directly. Three layers of mitigation, compose as needed:

- **`--no-shell` (+ `--no-subprocess`)** — bulletproof, but you lose shell.
- **`--env A,B,C`** — automatically also scrubs the subprocess environment, so `shell "echo $X"` returns empty for any `X` not on the allowlist. Closes the most common leak even when shell is on.
- **`--allow-bin git,docker`** — when shell IS allowed, restrict the first token. Basename match (so `/usr/local/bin/git` matches `git`), skips leading `FOO=bar` assignments.
- **`--no-shell-metachars`** — rejects `|`, `>`, `<`, `&`, `;`, `` ` ``, `$(` in shell args. Stops shell-injection style escapes inside an otherwise-allowed call.

Recommended posture for AI-agent surfaces / untrusted files: stack all four `--no-*` flags + `--env <minimal>`. The escape hatch is fully closed only at Layer 1.

## Host env-var allowlist

`perch --env A,B,C` restricts which host env vars resolve via `${NAME}` fallthrough. By default every env var is reachable; with `--env` only the named ones are. Auto-bound names (`home`, `cache_dir`, `exe_path`, `is_macos`, …) are NOT env vars and are unaffected.

```sh
perch --env HOME,PATH,API_KEY deploy
perch --env --no-shell deploy      # bare --env = no env vars visible at all
```

Blocked lookups produce: `env var ${X} is not in --env allowlist (declare with --env X)`.

Full design + the upcoming capability sandbox is at [sandbox.md](https://olivierdevelops.github.io/perch/sandbox/).

## Multi-file composition

`import "./PATH.perch"` pulls another file's commands into the current program. Use it to share command definitions across projects or to split a large file by concern.

| Form | Effect |
|---|---|
| `import "./lib.perch"` | flat — commands callable by their bare names (`deploy`) |
| `import "./aws.perch" as aws` | namespaced — commands callable as `aws.deploy` |
| `import "${file_dir}/shared/k8s.perch"` | same as relative — `${file_dir}` is the directory of THIS file |
| `import "${HOME}/.perch/team-ops.perch"` | absolute via env / auto-bound var (see below) |

**Path expansion.** `${name}` placeholders are substituted in import paths before the file is opened. Recognised names (same vocabulary as runtime auto-bound vars):

- `${file_dir}` / `${script_dir}` — directory of THIS file (the importing file)
- `${home}` / `${HOME}` — user's home dir
- `${cache_dir}` / `${config_dir}` / `${temp_dir}` — OS dirs
- `${exe_dir}` — directory of the running perch binary
- `${user}` / `${USER}` — username
- any other `${NAME}` — falls through to the host env

Unknown names error at load time (not silently expanded to empty), so a typo in the import path surfaces immediately.

**Semantics:**

- **Conflicts** (two flat imports both defining `deploy`, or a flat import defining a name the importer also declares) → static error from `--check` and from `Load`. No silent override.
- **Top-level bindings merge with parent-wins precedence.** Imported bindings fill in defaults; the importer can override by declaring the same NAME.
- **`private` commands** in an imported file are hidden from flat import (keeps them as internal helpers) but accessible via aliased import (`alias.privatename`).
- **Catch handlers** don't propagate — only the root file's catch is active.
- **Cycles** are detected and reported, not followed.
- **Paths** are resolved relative to the *importing* file, not the cwd. `import "./x"` in `/a/b/main.perch` looks for `/a/b/x.perch`.

When to reach for it:

- A team-wide `ops-lib.perch` with shared `notify`, `audit_log`, `acquire_lock` commands → each project's `commands.perch` does `import "/team/ops-lib.perch"` and uses them.
- Splitting a large file: pull `command build_*` into `build.perch`, `command deploy_*` into `deploy.perch`, then `import` both from the top-level file.

When NOT:

- A 200-command monorepo orchestrator. perch's imports are flat (no nested namespaces, no `import * as deep.path`). Past the small-to-medium range, the tool starts to fight you.

## When in doubt

- The op catalog is the source of truth. If you're unsure whether something is an op, look at [op-reference.md](https://github.com/olivierdevelops/perch/blob/main/docs/op-reference.md). If it's not there, fall back to `shell "..."`.
- The language reference is [language.md](https://github.com/olivierdevelops/perch/blob/main/docs/language.md).
- The demos folder ([demos/](https://github.com/olivierdevelops/perch/tree/main/demos)) has canonical patterns for common shapes.
- Ask the user about their target OS / arch when writing a build pipeline — it changes whether you need `if os == "..."` branches or not.

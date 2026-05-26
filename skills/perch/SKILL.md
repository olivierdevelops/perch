---
name: perch
description: Use this skill when the user wants to create, edit, or scaffold a `commands.perch` file for the perch task runner (https://github.com/luowensheng/perch). Trigger on: any mention of `perch`, `.perch`, "commands.perch", or requests to convert a Makefile / Justfile / shell scripts / bin folder / CI workflow into perch; also when the user asks how to ship a CLI tool as a single binary via `perch --build`. The deliverable is correct perch syntax — do not improvise keywords or ops.
---

# perch authoring guide

You are writing or modifying a `commands.perch` file. perch is a cross-platform task runner whose DSL is defined by [capy](https://github.com/luowensheng/capy). The grammar is small and rigid; do not invent new keywords or ops. Stick to what's documented below.

## File skeleton

Every `commands.perch` follows the same shape:

```capy
name    "PROGRAM_NAME"          # required
about   "PROGRAM_DESCRIPTION"   # optional but recommended
version "X.Y.Z"                 # optional but recommended

globals                          # optional; bindings shared by every command
    KEY = "value"
    NUMERIC = 42
    FLAG = true
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
2. **String literals always use `"..."` (or `'...'`). Multi-line strings use backslash-n escapes: `"line one\nline two"`.** Backticks are not valid in user-written `.perch` for op arguments — they're a library-internal token.
3. **Op arguments are positional, not named.** `cp "src" "dst"` is right; `cp src:"a" dst:"b"` is wrong.
4. **`${name}` is the only runtime interpolation form.** Don't use Go's `{{.name}}` or shell's `$name`. perch parses `${name}` after capy is done.
5. **One op per line. Block ops (`if_os`, `if_arch`, `if_eq`, `if_gt`, `if_lt`, `if_exists`, `if_neq`, `if_empty`, `if_not_empty`) wrap a nested body terminated by `end`.**
6. **`let NAME = OP ARG` form is for capturing return values. `let` is NOT a standalone op — it must precede an op-call expression with 0, 1, or 2 args.**

## The full config vocabulary (config region, before `do`)

| Form | Effect |
|---|---|
| `description "x"`                 | Help text |
| `arg NAME ... end`                | Typed CLI argument; properties are labelled inner lines (see below) |
| `private`                         | Hide from CLI; only callable via `run` |
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

## The op catalog (body region, inside `do`)

### Process / I/O
- `print MSG`, `println MSG`, `eprintln MSG`
- `shell CMD` — runs in bash / cmd.exe
- `shell_detached CMD` — fire-and-forget
- `fail MSG` — exits non-zero
- `exit N`
- `sleep SECONDS`
- `run TARGET` — call another command
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

### Block ops (each wraps `... end`)
- `if_os "darwin" | "linux" | "windows"`
- `if_arch "amd64" | "arm64"`
- `if_exists "path"`
- `if_eq A B`, `if_neq A B`, `if_gt A B`, `if_lt A B`
- `if_empty X`, `if_not_empty X`

### Capturable ops (use via `let X = OP ARG`)

**0-arg:** `hostname`, `cwd`, `get_os`, `get_arch`, `home_dir`, `temp_dir`, `cache_dir`, `app_data_dir`, `pid`, `user`

**1-arg, return string:** `upper`, `lower`, `trim`, `capitalize`, `md5`, `sha1`, `sha256`, `base64_encode`, `base64_decode`, `hex_encode`, `hex_decode`, `url_encode`, `url_decode`, `crc32`, `read_file`, `shell_output`, `http_get`, `http_delete`, `md5_file`, `sha1_file`, `sha256_file`, `json_parse`, `json_stringify`, `unix_to_iso`

**1-arg, return int/bool:** `length` (int), `file_size` (int), `exists` (bool), `is_dir` (bool), `is_file` (bool)

**2-arg:** `contains`, `has_prefix`, `has_suffix`, `replace`, `split`, `join`, `repeat`, `format`, `regex_match`, `regex_replace`, `http_post`, `http_put`, `port_check`, `dns_lookup`, `json_get`, `now` (1-arg)

## Interpolation rules

- `${name}` resolves in this order: command args → `let` captures → `globals` → per-command `env` → host process environment.
- Unknown names produce a runtime error. Giving the arg a `default` is the cleanest fix.
- UPPER_SNAKE_CASE globals are also exported as environment variables to `shell` calls automatically.
- `${HOME}`, `${USER}`, `${PATH}` work out of the box (they fall through to host env).

## When the user asks for something

Map the request to perch concepts:

| User says | Use |
|---|---|
| "build for multiple platforms" | a `release` command that calls `run build target:"..."` three times |
| "install dev tools cross-platform" | `if_os` blocks running brew / apt / choco |
| "I want a help text on this" | `description "..."` in the config region |
| "make this command not show up in --help" | `private` modifier |
| "compute a checksum" | `let h = sha256_file "..."` |
| "fetch a URL and save it" | `download "url" "dst"` (no `let` needed) |
| "fetch JSON, read a field" | `let body = http_get "url"` then `let v = json_get body "a.b.c"` |
| "compress this folder" | `tar_create "src" "out.tar.gz"` |
| "skip a step if a file already exists" | `if_exists "PATH" ... end` (run the SKIP body) — or invert with `if_not_empty` for absence |
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

globals
    BIN_DIR   = "./bin"
    APP_NAME  = "myapp"
    MAIN_PKG  = "./cmd/myapp"
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
        run build target:"darwin"
        run build target:"linux"
        run build target:"windows"
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
        run test
        run release
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

## When in doubt

- The op catalog is the source of truth. If you're unsure whether something is an op, look at [op-reference.md](https://github.com/luowensheng/perch/blob/main/docs/op-reference.md). If it's not there, fall back to `shell "..."`.
- The language reference is [language.md](https://github.com/luowensheng/perch/blob/main/docs/language.md).
- The demos folder ([demos/](https://github.com/luowensheng/perch/tree/main/demos)) has canonical patterns for common shapes.
- Ask the user about their target OS / arch when writing a build pipeline — it changes whether you need `if_os` branches or not.

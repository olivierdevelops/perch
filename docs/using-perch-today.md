# Using perch today

> **Read this if you want to *use* perch right now.** A lot of recent design docs ([sandboxed-by-design](sandboxed-by-design.md), [trust-by-manifest](trust-by-manifest.md)) describe where perch is *heading* — default-deny capabilities, named capability handles, inline `|`/`&&`/`||`, WASM manifests. **That part is roadmap, not shipped.** But several pieces have landed: the **`exec`** verb (shell-free subprocess), the **`pipe … end`** block, **`glob`**, the line toolbox (`grep`/`head`/…), `read`/`write` path scopes, and `#` comments all work today. This page documents what actually works in the current build, with syntax that parses today.

---

## The model that's live today

perch runs a `.perch` file. The same file is a CLI, a web UI (`--server`), a REPL (`--shell`), an MCP tool surface (`perch-mcp`), and a portable binary (`--build`).

Security today is **ambient-by-default, restricted two ways**:

1. **The operator** restricts at launch with flags: `--no-shell`, `--no-network`, `--no-write`, `--no-subprocess`, `--allow-bin`, `--allow-host`, `--env`, `--untrusted`.
2. **The author** can add a `requires` block to the file (opt-in). When present, it enforces strictly — undeclared shell bins / hosts / env reads error.

> The planned inversion to **zero ambient authority** (nothing works unless declared) is roadmap, not current. Today a file with no `requires` block and no CLI flags has full ambient access.

---

## A complete, working file

Everything below parses and runs in the current build:

```perch
name    "myapp"
about   "Build and ship myapp"
version "0.3.0"

BUILD_DIR = "./builds"

requires
    bin "go"
    bin "git"
    env "HOME"
    host "api.github.com"
    write "./builds"           # the filesystem is external too — declare what you write
    read  "./cmd"              # ...and what you read
end

command build
    description "Compile myapp for one target"
    arg target
        type string
        default "darwin"
        description "Target OS"
    end
    do
        print "Building for ${target}"
        mkdir "${BUILD_DIR}/${target}"
        with_env "GOOS=${target}"
            exec go build -o ${BUILD_DIR}/${target}/myapp ./cmd/myapp
        end
        let size = file_size "${BUILD_DIR}/${target}/myapp"
        print "built ${size} bytes"
    end
end

command setup
    description "Install dev deps, per OS"
    do
        if os == "darwin"
            exec brew install jq ripgrep
        end
        if os == "linux"
            exec sudo apt-get install -y jq ripgrep
        end
    end
end

catch passthrough
    description "Forward unknown verbs to git"
    proxy_args
    do
        shell "git ${proxy_args}"
    end
end
```

Run it:

```sh
perch -f myapp.perch build            # or just `perch build` if the file is commands.perch
perch -f myapp.perch build -target=linux
perch -f myapp.perch --help
perch -f myapp.perch --check          # static validation
perch -f myapp.perch status           # falls through catch → `git status`
```

---

## Top-level sections (all shipped)

| Section | Purpose |
|---|---|
| `name` / `about` / `version` | Metadata for `--help` / `--version`. |
| `NAME = value` (top level) | Bindings shared by every command; UPPER_SNAKE ones also reach `shell`/`exec` as env. |
| `requires … end` | The file-declared manifest — see below. Opt-in; enforces strictly when present. |
| `bundle … end` | Files to embed in the `--build` fat binary (`include "./x" as alias`). |
| `template NAME … end` | Reusable op-sequence stamps, expanded at each `call`. |
| `command NAME … end` | A typed verb. |
| `catch NAME … end` | Fallback for unknown verbs. Add `proxy_args` to bind `${proxy_args}`. |
| `import "./other.perch"` | Pull in another file's commands/templates. |

---

## `requires` — the manifest, as it works now

```perch
requires
    bin "docker"                       # must exist on PATH
    bin "kubectl"
        hash "sha256:abc123…"          # optional: pin exact bytes (read-only, no exec)
    end
    bin "jq" optional                  # absence is not fatal
    env "KUBECONFIG"                   # must be set (non-empty)
    env "DEBUG" optional
    host "api.github.com"              # allowed HTTP destination
    host "*.amazonaws.com"             # wildcard suffix
    read  "./src"                      # filesystem read scope (paths/dirs)
    write "./dist"                     # filesystem write scope (a write root implies read)
    os   "linux"                       # host OS must match (one per line)
    arch "amd64"                       # host arch must match (one per line)
end
```

When this block is present:

- **Preflight** (once, before any op): every required `bin` must exist on PATH; every `hash`/`hash_file`-pinned bin is byte-compared (no execution); every required `env` must be set; host `os`/`arch` must match.
- **Runtime**: `shell "X …"` where `X` isn't a declared bin → `bin_not_declared`; `http_*` to an undeclared host → `host_not_declared`; `get_env "Y"` where `Y` isn't declared → `env_not_declared`; a filesystem op (`mkdir`, `write_file`, `read_file`, `cp`, …) whose path falls outside every declared `read`/`write` root → `read_not_declared` / `write_not_declared`. Preflight failures → `requirement_unmet`.
- **`perch --check`** flags undeclared *literal* usage statically, before running. Interpolated args (`shell "${cmd}"`) are deferred to the runtime guard.

**There is no version checking.** Verifying a version means executing the binary before the sandbox exists, and a trojaned binary lies. Pin a `hash` instead, or check a version inside a command with `shell_output` + `regex_match`. Full reference: [requires.md](requires.md).

Declaring a requirement is **not** a capability grant — sandbox flags (`--allow-bin`, etc.) still gate the invocation. The manifest describes the program; flags are the policy for the run.

---

## Running things — `shell` and the op catalog

Today you run external tools with the **`shell`** op (a string command):

```perch
exec docker compose up -d
let out = exec git rev-parse HEAD
shell_detached "my-server --port 8080"
```

For most data work you don't need a shell at all — perch ships ~140 cross-platform ops. Prefer them over shelling out:

```perch
let body = http_get "https://api.github.com/repos/me/app"
let stars = json_get body ".stargazers_count"
let h = sha256_file "./dist/app"
mkdir "./out"
write_file "./out/manifest.json" "${body}"
```

Capture an op's result with `let`. See the full list in [op-reference.md](op-reference.md).

### Argument forms (shipped)

```perch
let url = get_env "API_URL"
print "${url}"        # string form
print url             # bare ident — resolves the binding directly

let u = upper url     # let-captured op with a bare-ident arg
```

Bare idents work for plain binding names **and dotted member paths** — `match err.kind` and `match os` both work (capy `dotted_ident`). The string form `match "${err.kind}"` still works too.

### Environment

```perch
with_env "FOO=bar"          # scoped — auto-restored when the block exits
    exec printenv FOO       # the subprocess sees FOO in its environment
end
export "TOKEN" "abc"        # process-lifetime (alias: set_env)
unset "TOKEN"               # remove (alias: unset_env)
```

### Control flow

```perch
if os == "darwin"
    print "mac"
end

for_each items it
    print it
end

match status              # bare ident OK for plain names
    case ok
        print "good"
    else
        print "?"
end
```

### Calling other commands / templates

```perch
run build "-target=linux"      # invoke another command; args are quoted, CLI-style
call ensure_dir "./out"        # expand a template with positional args
```

---

## The four tools you'll actually use

| Command | What it does |
|---|---|
| `perch --check` | Static validation: arg types, unknown ops, unresolved `${…}`, undeclared `requires` usage. Exit non-zero on error. Wire into CI. |
| `perch --scan FILE` | Static security audit (no execution): capabilities needed, env vars touched, risk score, recommended hardened invocation. Run before adopting a file you didn't write. |
| `perch simulate` | Walk the program against a hypothetical host; per-op WILL_RUN / WILL_FAIL / MIGHT_FAIL verdicts. |
| `perch test` | Run commands marked `test` in a sandbox, with `assert_*` ops. |

---

## Sandboxing a run (operator side, shipped)

```sh
# Let an agent call your ops with no shell and no network, only KUBECONFIG visible:
perch-mcp --no-network --no-shell --env KUBECONFIG -f ops.perch

# Pin which bins and hosts a script may touch:
perch --allow-bin git,docker --allow-host api.github.com -f deploy.perch deploy

# Treat all input as hostile (strictest preset):
perch --untrusted -f thirdparty.perch
```

These compose AND-wise with any `requires`/`sandbox` declarations in the file — neither side can grant more than the other allows.

---

## Gotchas in the current build

- **Comments work.** `# …` (leading or trailing) parses and is ignored.
- **`try / rescue / finally` works.** Built on capy `block_sections`; a `finally`-only `try` re-raises after cleanup (only a non-empty `rescue` swallows).
- **`match` takes bare idents and dotted paths.** `match os` and `match err.kind` both work; the string form `match "${err.kind}"` still works too.
- **No version checking in `requires`.** Removed deliberately (it required executing the binary). Use `hash` pins, or check versions inside a command.
- **Indentation is significant** — 4 spaces or 1 tab per level.

---

## What's roadmap, NOT shipped

So you don't mistake design docs for current features. The following are **proposals only**:

- **Zero ambient authority / default-deny** — today a file without `requires` has full access. ([sandboxed-by-design.md](sandboxed-by-design.md))
- **`exec BIN args` verb, `shell { bin … }` two-level capability, capability handles (`as NAME`), `|` / `pipe` / `glob`, `with_exec`** — none exist yet. Use `shell "…"` + `requires` today. (Filesystem `read`/`write` path scopes in `requires` **are** shipped — see above.)
- **Removing the `shell` op** — `shell` is the current way to run subprocesses and isn't going anywhere soon.
- **In-module WASM manifests + signing + `wasm diff`** — `wasm_run` works today with capabilities declared in the `.perch` file; the in-module manifest is roadmap. ([trust-by-manifest.md](trust-by-manifest.md))

When in doubt: if it's in a doc titled with "(roadmap)" or "design", it's not shipped. This page only describes what runs.

---

## See also

- [The complete guide](guide.md) — zero-to-production walkthroughs
- [op-reference.md](op-reference.md) — the ~140 built-in ops
- [language.md](language.md) — full grammar reference
- [requires.md](requires.md) — the manifest in depth
- [sandbox.md](sandbox.md) — the capability flags

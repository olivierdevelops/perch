# perch

> **perch is a cross-platform task runner.** Declare your project's commands in one small file; run them on macOS / Linux / Windows; or `perch --build` once to ship them as a single portable binary.

[![CI](https://github.com/luowensheng/perch/actions/workflows/ci.yml/badge.svg)](https://github.com/luowensheng/perch/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/luowensheng/perch?include_prereleases)](https://github.com/luowensheng/perch/releases)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Powered by capy](https://img.shields.io/badge/grammar-capy-orange)](https://luowensheng.github.io/capy)

That's the one-sentence answer. The longer one: `perch` collapses what would otherwise be a Makefile *and* a `bin/` of bash scripts *and* the helper CLI you keep meaning to write into **one declarative file** — and the same file *also* serves as a web UI (`--server`), a REPL (`--shell`), an MCP tool surface for AI agents (`perch-mcp`), and a `#!/usr/bin/env perch` script. Those extra frontends are downstream consequences of having a typed-CLI representation in one file, not separate systems. **The primary abstraction is the file; everything else is rendering.**

```capy
name    "myapp"
about   "Build, test and ship myapp"
version "0.3.0"

globals
    BUILD_DIR = "./builds"
end

command build
    description "Compile myapp for one target"

    arg target
        type string
        default "darwin"
        description "Target OS"
    end

    do
        print "Building for ${target}…"
        mkdir "${BUILD_DIR}/${target}"
        shell "GOOS=${target} go build -o ${BUILD_DIR}/${target}/myapp ./cmd/myapp"
        let size = file_size "${BUILD_DIR}/${target}/myapp"
        print "Built ${size} bytes."
    end
end

command setup
    description "Install dev dependencies, cross-platform"
    do
        if os == "darwin"
            shell "brew install jq ripgrep"
        end
        if os == "linux"
            shell "sudo apt-get install -y jq ripgrep"
        end
        if os == "windows"
            shell "choco install jq ripgrep -y"
        end
    end
end
```

> `os` and `arch` are auto-bound at command start; reference them anywhere with `${os}` / `${arch}`. The unified `if EXPR ... end` form supersedes the old `if_os` / `if_eq` / `if_gt` keywords — see [docs/language.md](docs/language.md#conditionals).

```sh
perch build                    # → run from CLI
perch build -target=linux      # → with args
perch build --help             # → per-command help (args, defaults, examples)
perch --check                  # → statically validate commands.perch
perch --server                 # → same file, web UI
perch --shell                  # → same file, REPL
perch --build -o myapp         # → same file, portable binary
```

---

## Install

```sh
# Go users — CLI
go install github.com/luowensheng/perch@latest

# macOS / Linux (binary, no Go required)
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.ps1 | iex

# Or download a binary from the releases page:
# https://github.com/luowensheng/perch/releases
```

### AI-agent integration

`perch-mcp` is a Model Context Protocol server that lets Claude Desktop / Claude Code / Cursor / Zed call your commands as tools:

```sh
go install github.com/luowensheng/perch/cmd/perch-mcp@latest
```

See [docs/mcp.md](docs/mcp.md) for client setup. **The "why" lives in [docs/llm-control-plane.md](docs/llm-control-plane.md) — why a `.perch` file + `perch-mcp` + a few `--no-*` flags replaces the FastAPI service you'd otherwise stand up to give an agent typed, restricted actions.** There's also a [Claude Code skill](skills/perch/SKILL.md) that teaches Claude to *write* perch files correctly.

### Editor support (LSP + syntax)

`perch-lsp` provides diagnostics (parse + static `--check`), context-aware completion, hover, and document outline. **`perch` itself installs both**:

```sh
perch --install-lsp        # installs perch-lsp via `go install`
perch --install-vscode     # installs perch-lsp + the VS Code extension (auto-spawns the LSP)
```

- **VS Code:** `perch --install-vscode` does the whole flow (extracts the embedded extension, packages, installs). Requires `node`/`npm` and the VS Code `code` CLI on `$PATH`.
- **Neovim / Helix / Zed:** see [docs/lsp.md](docs/lsp.md) for one-screen setup snippets after `perch --install-lsp`.
- **Tree-sitter** grammar (for syntax highlighting beyond what the LSP gives you): [`editors/tree-sitter-perch`](editors/tree-sitter-perch).

### Shell completions

```sh
perch --completions bash > ~/.local/share/bash-completion/completions/perch
perch --completions zsh  > "${fpath[1]}/_perch"
perch --completions fish > ~/.config/fish/completions/perch.fish
```

---

## What perch gives you

1. **One language at the surface.** No more YAML-for-structure plus templates-for-logic. perch's DSL is defined by [capy](https://luowensheng.github.io/capy), so the grammar is itself data.
2. **Cross-platform built-ins.** `cp`, `mkdir`, `gzip`, `sha256_file`, `http_get`, plus `if os == "linux"` / `if arch == "arm64"` branching — first-class to the runtime, not bash one-liners you re-write per OS.
3. **Five frontends from one source.** The same `commands.perch` is callable as a CLI, served as a web UI (`--server`), steppable in a REPL (`--shell`), exposed to AI agents via MCP (`perch-mcp`), and runnable as an executable script (`#!/usr/bin/env perch` shebang).
4. **One `--build` away from shippable.** `perch --build -o myapp` produces a single executable for the *current OS/arch* — typically 10–15 MB. Format: the perch binary itself with your program JSON appended in a fat-binary footer; on startup, perch detects the footer and loads the embedded program instead of reading a `.perch` file. `--include <path>` additionally embeds a gzipped tarball — useful for shipping a Python / Node / monorepo project alongside the CLI.

    What this is, honestly:
    - **Cross-compile:** not yet — to ship a Linux binary, run `perch --build` on a Linux host (or under `docker run --rm -v $PWD:/src golang:alpine`). Native cross-compile is on the roadmap.
    - **Reproducibility:** not byte-identical across builds (Go's default link includes a build ID). The embedded program JSON IS deterministic — `sha256` of the appended footer is reproducible from the `.perch` source.
    - **Verification:** the built binary doesn't carry a signed manifest. If you need build provenance, run `perch --build` inside a reproducible-builds pipeline and sign the output with your existing tooling.
    - **Limits:** ~50 MB for the embedded archive (`--include`) before performance noticeably degrades; the binary loads everything into memory at startup.

    Full spec: [docs/embedding.md](docs/embedding.md).
5. **Controlled scripting** — not sandboxing. perch lets you declare what an invocation may do: `--no-shell`, `--no-network`, `--no-write`, `--no-subprocess`, `--env A,B,C`, `--allow-bin git,docker`, `--allow-host api.github.com`, `--max-runtime 300`, `--audit FILE.ndjson`. With `--no-shell` the boundary is airtight (perch never spawns a subprocess). With `shell` allowed, perch enforces *its own* op dispatch — the subprocess can still talk to the kernel, so adversarial input still needs an OS-level sandbox (`firejail` / `sandbox-exec` / `AppContainer`) layered underneath. HTTP ops have additional default-on protections: no private-IP destinations, no https→http downgrade, max 5 redirect hops, DNS-rebinding defense via multi-A validation.

> **Sweet spot.** A structured task runner for small-to-medium projects, plus an MCP tool surface for AI agents over the same file. Outside that range — a 200-command monorepo task system, a CI orchestrator, a public multi-tenant service, anything needing functions/modules — you'll outgrow perch's composability primitives. See "Where it breaks down" below.
>
> Honest framing of the agent side: perch gives an agent a **controlled execution surface** with declared restrictions, an audit log, and op-level dispatch. It is *not* a kernel-level sandbox — if the agent's input could be genuinely adversarial, layer perch under `firejail` / `sandbox-exec` / `AppContainer`.

---

## What perch is **not**

Perch is deliberately narrow. It is not:

- **A general-purpose programming language.** No closures, objects, modules, or user-defined functions beyond `command`. If your script needs those, it should call out to a language that has them.
- **A CI system.** It can be *invoked from* one (`perch ci` in a GitHub-Actions step) and replace shell glue *inside* a job. It doesn't schedule, queue, retry, or manage resources across machines.
- **A Kubernetes / container orchestrator.** It drives `kubectl` / `docker` / `helm` from the host side. It doesn't reimplement them.
- **A package manager.** `pkg_install` wraps `brew` / `apt` / `winget` etc. — perch doesn't host a registry or resolve dependencies.
- **An init system or service supervisor.** No restart policies, no health checks, no PID files. Use `systemd` / `launchd` / a process manager for that.
- **A polyglot runtime.** No Python / JS / Rust embedding. Call them via `shell` or ship them via `--build --include`.
- **A multi-user auth system.** `--server` is single-tenant, localhost-bound by default. For multi-tenant or public access, put it behind a reverse proxy with the auth story you already use.
- **A kernel-level sandbox.** Frame it as **controlled scripting, not sandboxing.** The op set is the boundary perch enforces; with `--no-shell` it's airtight (no subprocess can fire); with `shell` allowed, the spawned process can talk to the kernel directly and only perch's *own* HTTP / FS / env ops are fenced. For genuinely adversarial input, layer with `firejail` / `sandbox-exec` / `AppContainer` underneath. See [docs/sandbox.md §0d](docs/sandbox.md#0d-the-subprocess-escape-hatch--and-the-layered-defense) for the full discussion.

If you need one of the above, perch is the wrong layer — but it composes with whichever one you pick.

---

## A 30-second tour

After `go install github.com/luowensheng/perch@latest`:

```sh
mkdir hello-perch && cd hello-perch
perch --init                          # writes a starter commands.perch (with shebang, +x)
perch --help                          # lists the commands in it
perch hello                           # runs one via the perch binary
./commands.perch hello                # … or run the file directly as a script
./commands.perch                      # … or run the `main` default
perch --build -o ./greet              # bundles commands.perch into ./greet
./greet hello                         # ./greet works anywhere, no perch needed
```

`.perch` files double as standalone executable scripts — `perch --init` writes `#!/usr/bin/env perch` at the top and sets the file executable. Same shape as a bash script.

---

## A real example: perch builds itself

The repo's own [`commands.perch`](commands.perch) is what we use to build, clean, and tidy perch. It's 30 lines, four commands, and is the canonical "small portable task runner" shape:

```capy
#!/usr/bin/env perch
name "perch"
about "perch — a cross-platform command runner driven by capy"

globals
    BUILD_DIR = "${script_dir}/.ignore"
    BUILD_OUT = "${script_dir}/.ignore/perch"
end

command build
    description "Build the perch binary"
    do
        mkdir "${BUILD_DIR}"
        shell "go build -o ${BUILD_OUT} ."
        let size = file_size "${BUILD_OUT}"
        print "Built ${size} bytes."
    end
end

command clean
    description "Remove the built binary"
    do
        if exists "${BUILD_OUT}"
            rm "${BUILD_OUT}"
        end
    end
end

command tidy
    do
        shell "go mod tidy"
    end
end
```

Run with `perch build` or `./commands.perch build`. `${script_dir}` is one of the auto-bound variables — the directory containing the `.perch` file, so the build works from any cwd and contains no hardcoded paths.

Four more worked examples live under [demos/](demos): a Docker wrapper, a cross-platform installer, a Python project shipped as one binary, and a Go-project task runner. They're complete `commands.perch` files you can read end-to-end in two minutes each.

**Adoption is small.** Perch is young (v0.x). We use it ourselves; we'd like to see what other shapes it grows into.

---

## Where it breaks down

Honest about the limits — useful for deciding whether perch fits your problem:

**Composability.** This is where perch outgrows itself fastest:

- **No user-defined functions, closures, or higher-order ops.** `command NAME ... end` is the only abstraction unit. `run other_command` calls another command (with `let` bindings flowing through), but you can't pass a command as an argument, return one from another, or write a one-line helper inside a body. Repeated patterns get repeated literally — no macros, no `define`, no `defn`.
- **No imports or modules.** `-f` picks one file at a time. A second file can be referenced via `shell "perch -f other.perch sub-cmd"`, but that's a subprocess boundary, not a language one — no shared bindings, no static cross-file checks. **Multi-file composition is the most concrete missing feature.**
- **Scale ceiling per file.** ~10 commands reads well; ~30 starts to feel cramped; ~50+ probably wants splitting (which today means subprocess fan-out, not a language solution). If your problem looks like a 200-command monorepo orchestrator, perch is the wrong tool today.

**Other limits worth knowing:**

- **Streaming captures.** `shell "X"` streams output to stdout; `let s = shell_output "X"` waits for X to finish. Long real-time log views work via plain `shell`, not via captures.
- **No real list type.** Variadic args, `glob`, and `list_dir` return newline-joined strings; `for_each` iterates them. Nested data structures (maps, lists-of-lists) don't exist in the binding system. Use JSON ops + `json_get` for nested reads.
- **Single-process, sequential.** No coroutines, no event loop, no daemons. `shell_detached` is the only parallel escape; everything else runs in order.
- **State is per-invocation.** Persistent state lives in files / databases / whatever you choose. The REPL keeps bindings across lines within one session — that's the extent of in-memory persistence.
- **Cross-platform parity has an asterisk.** The ~140 ops are identical across macOS / Linux / Windows. `shell` invocations are inherently OS-specific. `--no-shell` is the only way to *guarantee* parity; with `shell` allowed, you write per-OS branches.
- **Adversarial input.** Restriction flags close the easy cases. For genuinely hostile `.perch` files you can't trust, layer perch under a kernel-level sandbox (`firejail`, `sandbox-exec`, `AppContainer`) — perch is *controlled scripting*, not a sandbox.
- **Hot reload.** Parsed once at start. Edit, re-run. No file-watcher mode.

If two or three of these are deal-breakers, perch is the wrong tool. If they're acceptable trade-offs, perch is probably saving you a Cobra app + a Makefile + a CI YAML + an MCP server.

---

## Why "perch"?

Capybaras famously let other animals — birds, monkeys, turtles — sit on their back. Your commands perch on perch the same way: declared once, then run wherever they need to (CLI, web, REPL, embedded binary). The DSL is also built on [capy](https://luowensheng.github.io/capy), which is short for capybara. So the name nods both ways.

---

## Documentation

| Reading order | What it covers |
|---|---|
| [docs/getting-started.md](docs/getting-started.md) | Five-minute tour |
| [docs/tutorials/01-replace-your-makefile.md](docs/tutorials/01-replace-your-makefile.md) | Convert a Makefile to perch |
| [docs/tutorials/02-ship-a-tool.md](docs/tutorials/02-ship-a-tool.md) | Bundle a commands.perch into a portable binary |
| [docs/tutorials/03-cross-platform-installer.md](docs/tutorials/03-cross-platform-installer.md) | One installer for macOS/Linux/Windows |
| [docs/language.md](docs/language.md) | Every keyword and modifier |
| [docs/op-reference.md](docs/op-reference.md) | The built-in op catalog (the "stdlib") |
| [docs/embedding.md](docs/embedding.md) | The `--build` fat-binary format |
| [docs/mcp.md](docs/mcp.md) | MCP server for AI agents (reference) |
| [docs/llm-control-plane.md](docs/llm-control-plane.md) | **Replace your LLM-tool backend with a `.perch` file** |
| [docs/applications.md](docs/applications.md) | **What perch is *for* — 23 real applications with worked examples** |
| [docs/faq.md](docs/faq.md) | vs Make / Just / Task / etc. |

Four worked examples live under [demos/](demos) — each a complete `commands.perch` you can run.

---

## Status

**Pre-1.0.** The DSL surface is stable; the op catalog will continue to grow. SemVer applies once we tag v1.0. See [CHANGELOG.md](CHANGELOG.md) for what's landed.

## License

[Apache License 2.0](LICENSE) — © 2026 The perch Authors.

## Acknowledgments

perch is built on [capy](https://luowensheng.github.io/capy) — the configurable transpiler engine that defines the entire DSL grammar.

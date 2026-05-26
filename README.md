# perch

> A cross-platform command runner. One `.perch` file → CLI, web UI, REPL, **or a single portable binary you can ship**.

[![CI](https://github.com/luowensheng/perch/actions/workflows/ci.yml/badge.svg)](https://github.com/luowensheng/perch/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/luowensheng/perch?include_prereleases)](https://github.com/luowensheng/perch/releases)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Powered by capy](https://img.shields.io/badge/grammar-capy-orange)](https://luowensheng.github.io/capy)

`perch` is what happens when a Makefile, a CI workflow file, a `bin/` of bash scripts, and the helper CLI you keep meaning to write all collapse into **one declarative file** — and that file works on macOS, Linux, and Windows from day one.

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
```

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
3. **Three frontends from one source.** The same `commands.perch` is callable as a CLI, served as a web UI (`--server`), and steppable in a REPL (`--shell`).
4. **One `--build` away from shippable.** Bundle your `commands.perch` into a single portable binary your users can run on a fresh machine with no Go toolchain, no perch install, no nothing.

---

## A 30-second tour

After `go install github.com/luowensheng/perch@latest`:

```sh
mkdir hello-perch && cd hello-perch
perch --init                          # writes a starter commands.perch
perch --help                          # lists the commands in it
perch hello                           # runs one
perch --build -o ./greet              # bundles commands.perch into ./greet
./greet hello                         # ./greet works anywhere, no perch needed
```

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

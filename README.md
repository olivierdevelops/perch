# perch

> **perch is a cross-platform command runtime for defining, running, and shipping operational tools in a single structured file.** Declare your commands once; run them consistently on macOS / Linux / Windows; expose them through a CLI, REPL, web UI, or MCP agent surface; and `perch --build` once to ship them as a single portable binary.

**Replaces the usual combination of:** shell scripts · Makefiles · ad-hoc CLI wrappers · internal ops tools · "one-off automation repos" — with one declarative file that humans, CI, and agents all execute.

📦 **[Browse the recipes →](recipes/)** — 22 ready-to-run `.perch` files for Redis, Postgres, MongoDB, the whole AI / observability / Kafka stacks, cross-platform tool installers, and daily Docker/kubectl wrappers. One `curl` + one `perch` invocation away from a working local environment.

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
perch test                     # → run every command marked `test` (sandboxed)
perch --report build           # → execute + render the span tree of what ran
perch --server                 # → same file, web UI (no terminal required ✨)
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

### 🪟 Web UI — no terminal required

For teammates who don't live in a terminal — support, ops, QA, the new hire on their first day — `perch --server` turns the same `.perch` file into a friendly localhost web app:

```sh
perch -f commands.perch --server --port 8080
# → open http://127.0.0.1:8080
```

**What you get out of the box:**

| Tab | What it does |
|---|---|
| **▶ Run** | Searchable command list with type-aware form inputs (checkbox for bools, number spinner for ints, multi-line for `rest` args). Click **Run** → output streams live in a dark panel. **Copy as CLI** button mirrors the form back to a shell command for handoff. |
| **🧪 Simulate** | Every `--sim-*` flag becomes a form field. Paste a v2 fixture JSON (with **oracles** + **scenarios**) and click Simulate → per-op outcomes (WILL_RUN ✓ / WILL_FAIL ✗ / MIGHT_FAIL ?) for each scenario side by side. |
| **🔍 Scan** | One click → the full capability + risk audit. Same report `perch --scan` prints, plus the recommended hardened invocation. |
| **✓ Check** | One click → syntactic validation. Issue list with severity counts. |
| **ℹ About** | Program metadata + links to docs. |

Plus: live **search/filter** across commands, **dark mode** (auto-detects system theme, persists per browser), **globals panel** showing every binding, **mod badges** for `test` / `detached` / `proxy_args` commands. Single-tenant + localhost-bound by default — put it behind your existing reverse proxy / SSO for shared access.

The web UI sits on the same interpreter as the CLI — anything you'd type as `perch -f file.perch CMD -arg=val` works in the UI, and vice versa.

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

## Mental model

> A structured way to **define and ship operational commands** that can run everywhere, with optional safety controls and multiple interfaces.

A `commands.perch` file is the single source of truth for an operational tool. It is **structured** (typed args, declared verbs, no string templating), **portable** (one runtime, identical built-ins across OSes), **optionally constrained** (capability flags + audit), **easy to distribute** (one binary), and **usable by both humans and agents** (CLI, web UI, REPL, MCP — same file). This enables a small but real pattern:

> **Operational workflows as distributable products.** What used to be a folder of scripts plus a wiki page becomes one binary you can `scp` and run.

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
5. **Composable execution + testable behavior.** Wrap any body in `parallel`, `timeout "30s"`, `retry 3`, `with_env`, `with_cwd`, `sandbox "no_shell,no_network"`, or `cache "KEY" "1h"` — block ops that change *how* the body runs without changing what it can express. Lift repeated op-sequences into `template NAME ... end` parameterized stamps, expanded inline at every `call NAME ...` site. Verify behavior with `perch test` — commands marked `test` run in a sandboxed temp cwd, fail-on-error semantics, seven `assert_*` ops for readable failures. Visualize a run with `perch --report` for a span tree of every op that fired. See [docs/execution-contexts.md](docs/execution-contexts.md) and [docs/testing.md](docs/testing.md).

6. **Controlled scripting** — not sandboxing. perch lets you declare what an invocation may do: `--no-shell`, `--no-network`, `--no-write`, `--no-subprocess`, `--env A,B,C`, `--allow-bin git,docker`, `--allow-host api.github.com`, `--max-runtime 300`, `--audit FILE.ndjson`. With `--no-shell` the boundary is airtight (perch never spawns a subprocess). With `shell` allowed, perch enforces *its own* op dispatch — the subprocess can still talk to the kernel, so adversarial input still needs an OS-level sandbox (`firejail` / `sandbox-exec` / `AppContainer`) layered underneath. HTTP ops have additional default-on protections: no private-IP destinations, no https→http downgrade, max 5 redirect hops, DNS-rebinding defense via multi-A validation.

> **Sweet spot.** A structured task runner for small-to-medium projects, plus an MCP tool surface for AI agents over the same file. Outside that range — a 200-command monorepo task system, a CI orchestrator, a public multi-tenant service, anything needing functions/modules — you'll outgrow perch's composability primitives. See "Where it breaks down" below.
>
> Honest framing of the agent side: perch gives an agent a **controlled execution surface** with declared restrictions, an audit log, and op-level dispatch. It is *not* a kernel-level sandbox — if the agent's input could be genuinely adversarial, layer perch under `firejail` / `sandbox-exec` / `AppContainer`.

---

## What perch is **not**

Perch is deliberately narrow. It is not:

- **A general-purpose programming language.** No closures, objects, modules, or user-defined runtime functions. (`template` blocks give you parse-time parameter substitution — paste-with-args, not closures — and that's the line.) If your script needs more, call out to a language that has it.
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

**Composability.** Improved in v0.2; still bounded:

- **Multi-file imports work.** `import "./shared.perch"` (flat) or `import "./aws.perch" as aws` (namespaced — `run aws.cmd`) pull in another file's commands. Cycles detected, conflicts erroring statically. Globals merge parent-wins; `private` commands hidden from flat import. Good enough to split a 100-command program into a few files of related concerns, or share a team-wide `ops-lib.perch` across projects.
- **No user-defined runtime functions, closures, or higher-order ops.** Two abstraction units exist: `command NAME ... end` (runtime verb) and `template NAME ... end` (parse-time stamp — paste-with-args, expanded inline at every `call NAME ...` site). You can't pass a command as an argument, return one from another, or close over state. If you reach for closures or return values, you've outgrown perch — call out to a real language via `shell`. See [docs/execution-contexts.md](docs/execution-contexts.md) for what templates can and can't do.
- **Scale ceiling.** ~50 commands across a few imported files reads well; ~200+ across a deeper graph starts to feel like the tool's fighting you. If your problem looks like a true monorepo task orchestrator with hundreds of commands and rich nesting, perch is the wrong layer.

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

Grouped by what you're trying to do.

**🚀 Get started in 30 minutes**

- [**recipes/**](recipes/) — **22 ready-to-run `.perch` files** for Redis, Postgres, MongoDB, the AI / observability / Kafka stacks, tool installers, and daily Docker/kubectl wrappers
- [docs/getting-started.md](docs/getting-started.md) — Five-minute tour
- [docs/migrating-from-shell.md](docs/migrating-from-shell.md) — Wrap your existing `.sh` files; three migration strategies
- [docs/tutorials/01-replace-your-makefile.md](docs/tutorials/01-replace-your-makefile.md) — Convert a Makefile to perch
- [docs/faq.md](docs/faq.md) — vs Make / Just / Task / Cobra; common questions

**🛠️ Author commands** (developers)

- [docs/language.md](docs/language.md) — Every keyword and modifier
- [docs/op-reference.md](docs/op-reference.md) — The built-in op catalog (~140 ops)
- [docs/execution-contexts.md](docs/execution-contexts.md) — **Templates + `parallel` / `retry` / `timeout` / `sandbox` / `cache` blocks + `--report`**
- [docs/testing.md](docs/testing.md) — **`perch test` — sandboxed behavior tests with `assert_*` ops**
- [**docs/wasm.md**](docs/wasm.md) — **`wasm_run` — load WebAssembly modules with capability gating by construction** (reference)
- [**docs/wasm-walkthroughs.md**](docs/wasm-walkthroughs.md) — **5 end-to-end real-world workflows: markdown validator, JSON Schema + caching, AI-agent surface via MCP, polyglot pipeline, CI hot loops**
- [**demos/wasm-plugin-host**](demos/wasm-plugin-host/) — **The killer demo: zero-trust runtime for AI-generated plugins — 4 legit + 1 deliberately malicious proving every escape attempt fails by construction**
- [docs/lsp.md](docs/lsp.md) — VS Code / Neovim / Helix / Zed integration
- [docs/applications.md](docs/applications.md) — **22 real applications worth copying**

**📦 Ship as a product**

- [docs/tutorials/02-ship-a-tool.md](docs/tutorials/02-ship-a-tool.md) — `perch --build` deep-dive
- [docs/tutorials/03-cross-platform-installer.md](docs/tutorials/03-cross-platform-installer.md) — One installer for three OSes
- [docs/embedding.md](docs/embedding.md) — Fat-binary format spec

**🛡️ Adopt at scale** (platform / SRE / security)

- [docs/sandbox.md](docs/sandbox.md) — **Capability model: env / FS / net / shell scopes, `--untrusted`, file-side `sandbox` blocks**
- [docs/llm-control-plane.md](docs/llm-control-plane.md) — **Replace your LLM-tool backend with a `.perch` file**
- [docs/mcp.md](docs/mcp.md) — MCP server reference (JSON-RPC over stdio)

Four worked examples live under [demos/](demos) — each a complete `commands.perch` you can run.

---

## Status

**Pre-1.0.** The DSL surface is stable; the op catalog will continue to grow. SemVer applies once we tag v1.0. See [CHANGELOG.md](CHANGELOG.md) for what's landed.

## License

[Apache License 2.0](LICENSE) — © 2026 The perch Authors.

## Acknowledgments

perch is built on [capy](https://luowensheng.github.io/capy) — the configurable transpiler engine that defines the entire DSL grammar.

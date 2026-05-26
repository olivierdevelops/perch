# perch

> A cross-platform command runner. One `.capy` file → CLI, web UI, REPL, **or a single portable binary you can ship**.

[![CI](https://github.com/luowensheng/perch/actions/workflows/ci.yml/badge.svg)](https://github.com/luowensheng/perch/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/luowensheng/perch?include_prereleases)](https://github.com/luowensheng/perch/releases)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Powered by capy](https://img.shields.io/badge/grammar-capy-orange)](https://github.com/luowensheng/capy)

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
    arg         target string "Target OS"
    arg_default target "darwin"
    do
        print "Building for {{target}}…"
        mkdir "{{BUILD_DIR}}/{{target}}"
        shell "GOOS={{target}} go build -o {{BUILD_DIR}}/{{target}}/myapp ./cmd/myapp"
        let size = file_size "{{BUILD_DIR}}/{{target}}/myapp"
        print "Built {{size}} bytes."
    end
end

command setup
    description "Install dev dependencies, cross-platform"
    do
        if_os "darwin"
            shell "brew install jq ripgrep"
        end
        if_os "linux"
            shell "sudo apt-get install -y jq ripgrep"
        end
        if_os "windows"
            shell "choco install jq ripgrep -y"
        end
    end
end
```

```sh
perch build                    # → run from CLI
perch build -target=linux      # → with args
perch --server                 # → same file, web UI
perch --shell                  # → same file, REPL
perch --build -o myapp         # → same file, portable binary
```

---

## Install

```sh
# Go users — CLI
go install github.com/luowensheng/perch/cmd/perch@latest

# macOS / Linux (binary, no Go required)
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh | sh

# Or download a binary from the releases page:
# https://github.com/luowensheng/perch/releases
```

---

## What perch gives you

1. **One language at the surface.** No more YAML-for-structure plus templates-for-logic. perch's DSL is defined by [capy](https://github.com/luowensheng/capy), so the grammar is itself data.
2. **Cross-platform built-ins.** `cp`, `mkdir`, `gzip`, `sha256_file`, `http_get`, `if_os` are first-class ops the runtime knows — not bash one-liners you re-write per OS.
3. **Three frontends from one source.** The same `commands.capy` is callable as a CLI, served as a web UI (`--server`), and steppable in a REPL (`--shell`).
4. **One `--build` away from shippable.** Bundle your `commands.capy` into a single portable binary your users can run on a fresh machine with no Go toolchain, no perch install, no nothing.

---

## A 30-second tour

After `go install …/cmd/perch@latest`:

```sh
mkdir hello-perch && cd hello-perch
perch --init                          # writes a starter commands.capy
perch --help                          # lists the commands in it
perch hello                           # runs one
perch --build -o ./greet              # bundles commands.capy into ./greet
./greet hello                         # ./greet works anywhere, no perch needed
```

---

## Why "perch"?

Capybaras famously let other animals — birds, monkeys, turtles — sit on their back. Your commands perch on perch the same way: declared once, then run wherever they need to (CLI, web, REPL, embedded binary). The DSL is also built on [capy](https://github.com/luowensheng/capy), which is short for capybara. So the name nods both ways.

---

## Documentation

| Reading order | What it covers |
|---|---|
| [docs/getting-started.md](docs/getting-started.md) | Five-minute tour |
| [docs/language.md](docs/language.md) | Every keyword and modifier |
| [docs/op-reference.md](docs/op-reference.md) | The built-in op catalog (the "stdlib") |
| [docs/embedding.md](docs/embedding.md) | The `--build` fat-binary format |
| [docs/faq.md](docs/faq.md) | vs Make / Just / Task / etc. |

Six worked examples live under [demos/](demos) — each a complete `commands.capy` you can run.

---

## Status

**Pre-1.0.** The DSL surface is stable; the op catalog will continue to grow. SemVer applies once we tag v1.0. See [CHANGELOG.md](CHANGELOG.md) for what's landed.

## License

[Apache License 2.0](LICENSE) — © 2026 The perch Authors.

## Acknowledgments

perch is built on [capy](https://github.com/luowensheng/capy) — the configurable transpiler engine that defines the entire DSL grammar.

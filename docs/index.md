---
hide:
  - toc
---

# perch

> One `.perch` file → CLI, web UI, REPL, MCP server, or a portable binary you can ship.
> Built on [capy](https://github.com/luowensheng/capy). Apache-2.0.

<div id="perch-demo" class="perch-demo"></div>

The animated demo above cycles through the same `redis.perch` file driving five different frontends. **You write one file. perch produces the CLI, the web UI, the REPL, the MCP tool surface, and a self-contained binary.**

---

## Three commands to start

```sh
# 1) install
go install github.com/luowensheng/perch@latest

# 2) scaffold
perch --init

# 3) explore
perch --help          # list commands
perch <cmd> --help    # per-command help (args, defaults, examples)
perch --check         # static validation
```

---

## What's in the box

<div class="perch-features">

<div class="card">
  <h4>One DSL, four frontends</h4>
  <p>The same <code>commands.perch</code> is callable as a CLI, served as a web UI (<code>--server</code>), steppable in a REPL (<code>--shell</code>), and exposed as MCP tools for AI agents (<code>perch-mcp</code>).</p>
</div>

<div class="card">
  <h4>Single-binary distribution</h4>
  <p><code>perch --build -o myapp</code> bundles the parsed program into a self-extracting binary. The recipient needs no Go, no perch install, no source. Same OS/arch as host; cross-compile on the roadmap.</p>
</div>

<div class="card">
  <h4>~70 cross-platform ops</h4>
  <p>First-class <code>shell</code>, <code>mkdir</code>, <code>cp</code>, <code>gzip</code>, <code>tar_create</code>, <code>http_get</code>, <code>download</code>, <code>sha256_file</code>, <code>regex_replace</code>, <code>now</code>, <code>json_get</code>, … plus <code>os</code>/<code>arch</code> auto-bound for branching.</p>
</div>

<div class="card">
  <h4>Unified <code>if EXPR ... end</code></h4>
  <p>Comparisons (<code>if os == "linux"</code>), truthy/falsy (<code>if has_bin</code>, <code>if not has_bin</code>), predicate calls (<code>if exists "./bin"</code>), numeric (<code>if size &gt; 1000000</code>). One block, every conditional shape.</p>
</div>

<div class="card">
  <h4>Static <code>--check</code> validator</h4>
  <p>Catches typo'd arg types, mismatched default values, duplicate args, colliding positional indexes, missing <code>run TARGET</code>, unknown op kinds, unresolved <code>${name}</code> placeholders — before the command runs.</p>
</div>

<div class="card">
  <h4>Per-command <code>--help</code></h4>
  <p><code>perch &lt;cmd&gt; --help</code> shows usage, an arguments table with types + defaults + required/optional flags, env vars, modifiers, examples, and the source file path.</p>
</div>

<div class="card">
  <h4>Fuzzy "Did you mean…?"</h4>
  <p>Levenshtein-based suggestions when an unknown command is typed and there's no <code>catch</code>. Up to three candidates ranked by distance.</p>
</div>

<div class="card">
  <h4>Catch passthrough</h4>
  <p>Inside <code>catch</code>, the full unknown invocation is bound as <code>${proxy_args}</code>. <code>shell "git ${proxy_args}"</code> turns perch into a drop-in superset of any tool you'd like to extend.</p>
</div>

<div class="card">
  <h4>LSP server (<code>perch-lsp</code>)</h4>
  <p>Diagnostics, context-aware completion, hover docs, document outline. Auto-spawned by the VS Code extension; Neovim/Helix/Zed setup is a one-screen snippet.</p>
</div>

<div class="card">
  <h4>MCP server (<code>perch-mcp</code>)</h4>
  <p>JSON-RPC over stdio. AI agents call <code>perch_list</code> / <code>perch_run</code> with typed args. The schema is the security boundary — no shell escape, ever.</p>
</div>

<div class="card">
  <h4>One-command install</h4>
  <p><code>perch --install-lsp</code> installs the LSP via <code>go install</code>. <code>perch --install-vscode</code> bundles the extension (embedded in the binary) and installs it via <code>code --install-extension</code>.</p>
</div>

<div class="card">
  <h4>Shell completions</h4>
  <p><code>perch --completions bash|zsh|fish</code> emits the completion script. Includes command-name completion from your current <code>commands.perch</code>.</p>
</div>

<div class="card">
  <h4>Web UI streams NDJSON</h4>
  <p><code>perch --server</code> renders a button-per-command UI with typed arg forms. Stdout streams as NDJSON over <code>/api/exec</code> — pipe it to your observability stack for a free audit log.</p>
</div>

<div class="card">
  <h4>Auto-bound <code>os</code> / <code>arch</code></h4>
  <p>Every command starts with <code>os</code> and <code>arch</code> already bound to <code>runtime.GOOS</code> / <code>runtime.GOARCH</code> — no declaration. <code>if os == "darwin"</code> just works.</p>
</div>

<div class="card">
  <h4>Block-shaped args</h4>
  <p><code>arg NAME ... end</code> with labelled inner fields (<code>type</code>, <code>default</code>, <code>description</code>, <code>optional</code>, <code>index</code>) — readable, no positional gotchas, validated by <code>--check</code>.</p>
</div>

<div class="card">
  <h4>VHCO architecture</h4>
  <p>Six top-level folders, one job each (<code>domain</code> / <code>features</code> / <code>usecases</code> / <code>io</code> / <code>infra</code> / <code>orchestrator</code>). New contributors land their first PR in 30 minutes.</p>
</div>

</div>

---

## The 30-second tour

```sh
mkdir hello-perch && cd hello-perch
perch --init                          # writes a starter commands.perch
perch --help                          # lists the commands in it
perch hello                           # runs one
perch --build -o ./greet              # bundles commands.perch into ./greet
./greet hello                         # ./greet works anywhere, no perch needed
perch --server                        # → http://127.0.0.1:10032 in a browser
perch --shell                         # interactive REPL
perch --install-vscode                # installs perch-lsp + VS Code extension
```

---

## Common applications

These are explored in depth in **[applications.md](applications.md)** — 22 real categories of work perch is good at.

- **Wrap a clunky CLI** — give Docker / kubectl / ffmpeg / openssl / rsync a sane verb-driven UX
- **Extend an existing tool** — `catch passthrough` so unknown commands fall through to the real binary
- **LLM-safe operations surface** — declared ops + typed args + confirmation tokens; no shell escape for agents
- **Unify multiple binaries** — one "meta-tool" for install / status / update / uninstall of your dev toolkit
- **Ship an internal team CLI** — replace your `bin/` folder of bash scripts with one shippable binary
- **Replace your Makefile** — same file drives local dev and CI; `if os ==` covers Windows for free
- **Cross-platform machine setup** — one `setup.perch` for darwin/linux/windows, shipped as one binary
- **Safe ops surface for non-engineers** — web UI with forms; support team clicks buttons instead of SSHing

→ Full catalog with worked code: **[applications.md](applications.md)**.

---

## Documentation

| Reading order | What it covers |
|---|---|
| [getting-started.md](getting-started.md) | Five-minute hands-on tour |
| [tutorials/01-replace-your-makefile.md](tutorials/01-replace-your-makefile.md) | Convert a real Makefile to perch |
| [tutorials/02-ship-a-tool.md](tutorials/02-ship-a-tool.md) | `perch --build` deep-dive |
| [tutorials/03-cross-platform-installer.md](tutorials/03-cross-platform-installer.md) | One installer for three OSes |
| [language.md](language.md) | Every keyword, modifier, and operator |
| [op-reference.md](op-reference.md) | The built-in op catalog (~70 ops) |
| [embedding.md](embedding.md) | Fat-binary format spec |
| [mcp.md](mcp.md) | AI agent integration |
| [lsp.md](lsp.md) | Editor integration |
| [applications.md](applications.md) | **What perch is for — 22 real applications** |
| [faq.md](faq.md) | vs Make / Just / Task / etc. |

Source on GitHub: [luowensheng/perch](https://github.com/luowensheng/perch). Apache-2.0.

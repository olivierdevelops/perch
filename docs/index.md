---
hide:
  - toc
---

# perch

> **One `.perch` file becomes a CLI, a web UI, a REPL, an MCP tool surface for AI agents, and a portable single-file binary.**
> Built on [capy](https://luowensheng.github.io/capy). Apache-2.0.

<div id="perch-demo" class="perch-demo"></div>

The animated demo above cycles through the same `redis.perch` file driving five different frontends. **You write one file. perch produces every surface.**

<div class="perch-cta">
  <a class="perch-cta__primary" href="getting-started/">Get started in 5 minutes →</a>
  <a class="perch-cta__secondary" href="applications/">See 22 real applications</a>
  <a class="perch-cta__secondary" href="https://github.com/luowensheng/perch">GitHub</a>
</div>

---

## Who this is for

<div class="perch-personas">

<div class="persona">
<h4>Platform &amp; DevEx teams</h4>
<p>You maintain the <code>bin/</code> folder of bash scripts, the Makefile no one trusts on Windows, and the README that tells new hires which steps to skip. Replace all three with one shippable binary.</p>
</div>

<div class="persona">
<h4>SREs &amp; on-call</h4>
<p>Wrap kubectl / docker / rsync / openssl behind named verbs with typed args. Give the support team a web UI for safe runbooks. Stream every action as NDJSON straight to your audit pipeline.</p>
</div>

<div class="persona">
<h4>Tool authors shipping internal CLIs</h4>
<p>Embed a Python or Node project inside a single binary that installs itself into a hash-addressed cache, sets up its own venv, and drops a launcher in <code>$PATH</code>. Recipients need no Go, no pip, no clone.</p>
</div>

<div class="persona">
<h4>Teams building with AI agents</h4>
<p>Expose only the verbs an agent should call. Typed args. No shell escape. <code>perch-mcp</code> serves your <code>.perch</code> file as a Model Context Protocol tool surface — the schema is the security boundary.</p>
</div>

</div>

---

## Sound familiar?

<div class="perch-pain">

- 🧱 Your Makefile silently breaks on Windows / on Apple Silicon / on the new intern's machine.
- 📜 Your `bin/` folder has 40 bash scripts and exactly one person who knows which to run.
- 🧪 You ship a tool internally and spend a week answering install questions instead of building.
- 🤖 An LLM agent needs to drive your infra and you're not handing it a shell. Ever.
- 🧰 Your "internal CLI" is six Python packages, a Node helper, and a stern Slack message.
- 🪟 "Windows is a stretch goal" has been on the roadmap for three quarters.

</div>

**perch is one DSL, one runtime, and one binary that solves all of these together.**

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

## What you get (outcomes, not features)

<div class="perch-features">

<div class="card">
  <h4>One source of truth</h4>
  <p>Local dev, CI, production runbook, and the support team's UI all execute the same <code>commands.perch</code>. No more "works on the build server, fails on Karen's laptop."</p>
</div>

<div class="card">
  <h4>Drop-in distribution</h4>
  <p><code>perch --build -o myapp</code> produces a self-extracting binary. <code>--include ./src</code> embeds an entire project. Recipients run one file — no Go, no perch, no source clone.</p>
</div>

<div class="card">
  <h4>Cross-platform without bash tax</h4>
  <p>~70 first-class ops — <code>cp</code>, <code>mkdir</code>, <code>gzip</code>, <code>tar_create</code>, <code>http_get</code>, <code>download</code>, <code>sha256_file</code>, <code>regex_replace</code>, <code>json_get</code>, … — implemented in Go, identical on macOS / Linux / Windows.</p>
</div>

<div class="card">
  <h4>Unified <code>if EXPR ... end</code></h4>
  <p>Comparisons (<code>if os == "linux"</code>), truthy/falsy (<code>if has_bin</code>, <code>if not has_bin</code>), predicate calls (<code>if exists "./bin"</code>), numeric (<code>if size &gt; 1000000</code>). One block, every shape.</p>
</div>

<div class="card">
  <h4>Static <code>--check</code> validator</h4>
  <p>Catches typo'd arg types, mismatched defaults, duplicate args, colliding positional indexes, missing <code>run TARGET</code>, unknown op kinds, unresolved <code>${name}</code> placeholders — before the command runs. Wire it into pre-commit.</p>
</div>

<div class="card">
  <h4>Per-command <code>--help</code></h4>
  <p><code>perch &lt;cmd&gt; --help</code> shows usage, an arguments table with types + defaults + required/optional flags, env vars, modifiers, examples, and the source file path. Help is free.</p>
</div>

<div class="card">
  <h4>Fuzzy "Did you mean…?"</h4>
  <p>Levenshtein-based suggestions when an unknown command is typed. Up to three candidates ranked by distance. Newcomers stop staring at "unknown command" errors.</p>
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
  <h4>Bundle ops for installers</h4>
  <p><code>bundle_hash</code> / <code>bundle_extract</code> / <code>bundle_dir</code> let your binary install itself into a content-addressable cache. Multiple versions coexist; pruning is <code>rm -rf</code> by hash.</p>
</div>

</div>

---

## Four use cases enterprise teams recognise

### 1. Replace the Makefile that everyone is afraid of

<div class="perch-usecase">

**Before:** a 400-line `Makefile` with shell variations behind every target; Windows users on WSL; the CI YAML reimplements half of it; nobody touches `test-integration` because the last person who did is now in a different company.

**After:** one `commands.perch` shared by local dev and CI. `perch --check` runs in pre-commit. `perch --help` is the README.

```capy
command test
    description "Run unit + integration tests"
    do
        shell "go test -race ./..."
        if exists "./integration"
            shell "go test -tags=integration ./integration/..."
        end
    end
end

command release
    description "Cross-compile for darwin/linux/windows"
    do
        run build target:"darwin"
        run build target:"linux"
        run build target:"windows"
    end
end
```

→ Walkthrough: **[tutorials/01-replace-your-makefile.md](tutorials/01-replace-your-makefile.md)**

</div>

### 2. Ship a Python / Node / monorepo project as one self-installing binary

<div class="perch-usecase">

**Before:** "first install pyenv, then python 3.11, then a venv, then pip install -r requirements.txt, then…" Three pages of README and a Slack channel for install help.

**After:** you hand them `stt_bin`. They run `./stt_bin install`. The binary extracts an embedded archive into `~/.cache/perch/<hash>/`, creates a venv, runs `pip install`, drops a launcher in `~/.local/bin/stt`. Done.

```sh
perch --build -f commands.perch --include ./src -o stt_bin
scp stt_bin user@server:/usr/local/bin/
ssh user@server 'stt_bin install && stt example.wav'
```

The recipient needs **only** what your install command requires (here: `python3`). No package manager. No registry. No internet at install time.

→ Worked example: **[demos/05-python-installer](https://github.com/luowensheng/perch/tree/main/demos/05-python-installer)**

</div>

### 3. Give AI agents a safe operations surface

<div class="perch-usecase">

**Before:** the agent gets a shell. You hope. You write a long system prompt about what it should and shouldn't do. You audit logs after the fact.

**After:** the agent gets `perch-mcp` pointed at `ops.perch`. It can call exactly the verbs you declared, with exactly the arg types you declared. Anything else returns a typed error. No shell escape ever.

```capy
command restart_service
    description "Restart a service on a host"
    arg host
        type string
        description "Hostname (must match /^[a-z0-9.-]+$/)"
    end
    arg service
        type string
        description "Service name (one of: web, worker, scheduler)"
    end
    do
        if not regex_match "${host}" "^[a-z0-9.-]+$"
            fail "invalid hostname"
        end
        shell "ssh ${host} systemctl restart ${service}"
    end
end
```

The agent never sees `ssh`. It sees `restart_service(host, service)` with typed args. The schema is the security boundary.

→ Details: **[mcp.md](mcp.md)**

</div>

### 4. Wrap a clunky CLI behind sane verbs

<div class="perch-usecase">

**Before:** every team member memorises 12 docker flags. Mistakes cost an afternoon. The Slack channel has the same three questions every week.

**After:** ship `dev` (a perch binary). The team types `dev up`, `dev logs`, `dev shell`, `dev reset`. Unknown verbs fall through to docker via <code>catch passthrough</code>, so power users lose nothing.

```capy
command up
    description "Start the dev stack"
    do
        shell "docker compose up -d"
        shell "docker compose exec api migrate up"
        print "✓ Stack running at http://localhost:8080"
    end
end

catch passthrough
    description "Forward unknown commands to docker"
    do
        shell "docker ${proxy_args}"
    end
end
```

One binary. Onboarding goes from "read this 6-page doc" to "run `dev up`."

→ More patterns: **[applications.md](applications.md)**

</div>

---

## How it compares

<div class="perch-compare-wrap">

| | perch | Make | Just | Task | bash scripts | Cobra/Click |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| Cross-platform without `if uname` | ✅ | ⚠️ | ⚠️ | ✅ | ❌ | ✅ |
| Typed args + per-command `--help` | ✅ | ❌ | ❌ | ⚠️ | ❌ | ✅ |
| Static validator (`--check`) | ✅ | ❌ | ❌ | ❌ | ❌ | ⚠️ |
| Built-in web UI | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| MCP server for AI agents | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Single-binary distribution | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Embed source/data inside binary | ✅ | ❌ | ❌ | ❌ | ❌ | ⚠️ |
| LSP + VS Code extension | ✅ | ❌ | ⚠️ | ⚠️ | ❌ | n/a |
| ~70 portable ops (no bash) | ✅ | ❌ | ❌ | ⚠️ | ❌ | n/a |
| Zero-build authoring (no Go required) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |

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

## Common questions

**Is it a build tool or a CLI framework?** Both. Same file becomes a Make-style task runner *and* a Cobra-style typed CLI. Pick the surface (CLI / web / REPL / MCP / binary) that fits the caller.

**Do recipients need to install perch?** No. `perch --build` produces a standalone binary. They run that.

**Does it work on Windows?** Yes. The ~70 built-in ops are Go implementations, identical on macOS / Linux / Windows. Only `shell` invocations are inherently OS-specific.

**What about secrets?** perch reads env vars at runtime (`${HOME}`, `${API_KEY}`, …). Don't bake secrets into the file. The static `--check` doesn't store anything.

**Can I extend it with my own ops?** Yes — perch is a Go library too. Drop a handler into `infra/ops/` and register it. But the ~70 built-ins cover most needs.

**How is this different from capy?** [capy](https://luowensheng.github.io/capy) is a small transpiler engine. perch *uses* capy to define its grammar (`lib.perch`), so the perch DSL is what capy ingests. capy isn't a language; perch is.

→ More: **[faq.md](faq.md)**

---

## Where to next

<div class="perch-next">

<div class="next">
<h4>For platform / DevEx</h4>
<ul>
<li><a href="tutorials/01-replace-your-makefile/">Replace your Makefile</a></li>
<li><a href="applications/">22 real applications</a></li>
<li><a href="language/">Language reference</a></li>
</ul>
</div>

<div class="next">
<h4>For tool authors</h4>
<ul>
<li><a href="tutorials/02-ship-a-tool/">Ship a tool as a binary</a></li>
<li><a href="tutorials/03-cross-platform-installer/">Cross-platform installer</a></li>
<li><a href="embedding/">Fat-binary format spec</a></li>
</ul>
</div>

<div class="next">
<h4>For AI / agent teams</h4>
<ul>
<li><a href="mcp/">MCP server integration</a></li>
<li><a href="op-reference/">Op catalog (~70 ops)</a></li>
<li><a href="language/">Language reference</a></li>
</ul>
</div>

<div class="next">
<h4>For editor users</h4>
<ul>
<li><a href="lsp/">LSP setup (VS Code / Neovim / Helix / Zed)</a></li>
<li><a href="getting-started/">5-minute hands-on</a></li>
<li><a href="faq/">FAQ &amp; comparisons</a></li>
</ul>
</div>

</div>

---

## Full documentation

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

---
hide:
  - toc
---

# perch

> **One `.perch` file becomes a CLI, a web UI, a REPL, an MCP tool surface for AI agents, and a portable single-file binary.**
> Built on [capy](https://luowensheng.github.io/capy). Apache-2.0.

<div id="perch-demo" class="perch-demo"></div>

The animated demo above cycles through the same `redis.perch` file driving five different frontends. **You write one file. perch produces every surface:**

<div id="perch-fanout"></div>

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
<p>Replace your LLM-tool backend with a <code>.perch</code> file. Typed args, declared verbs, composable restrictions — <code>perch-mcp --no-shell --no-network --env KUBECONFIG -f ops.perch</code> is the whole backend. <a href="llm-control-plane/">See how →</a></p>
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

## Cross-platform without thinking about it

Every command starts with ~30 variables already bound. **No declaration, no `let`, no `if uname`.** Hover any row — that's what perch sees on the running machine.

<div id="perch-bindings"></div>

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
  <h4>LLM control plane — no backend needed</h4>
  <p>One <code>.perch</code> file + <code>perch-mcp</code> replaces the FastAPI service you'd otherwise build to give an agent a fixed set of typed actions. The grammar is the security boundary; <code>--no-shell --no-network --env A,B</code> is the policy. <a href="llm-control-plane/">Deep dive →</a></p>
</div>

<div class="card">
  <h4>MCP server (<code>perch-mcp</code>)</h4>
  <p>JSON-RPC over stdio. Agents call <code>perch_list</code> / <code>perch_run</code> with typed args. Schema auto-derived from your file. Reference: <a href="mcp/">mcp.md</a>.</p>
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
  <h4>Preview before running</h4>
  <p><code>perch --dry-run cmd</code> prints every op (with interpolated args) and skips execution. <code>perch --ask cmd</code> prompts y/n/a/q per op — accept, skip, run-all, or quit. The args you see are what the handler receives; no surprises.</p>
</div>

<div class="card">
  <h4>Composable restrictions</h4>
  <p><code>perch --no-shell --no-network --no-write deploy</code> — each flag says exactly what it disables, and they compose. <code>perch --env HOME,PATH</code> restricts which host env vars resolve via <code>${…}</code>. The banner <code>🔒 security: --no-shell --env HOME,PATH</code> shows posture at a glance. See <a href="sandbox/">sandbox.md</a>.</p>
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

<div class="pterm-pair">
<div class="pterm" id="t-make-before" data-title="make (the bad old days)"></div>
<div class="pterm" id="t-make-after"  data-title="perch (one file, every OS)"></div>
</div>
<script type="application/json" data-pterm="t-make-before">
[
  {"k":"in",  "t":"make test"},
  {"k":"out", "t":"sed: -i requires an argument on macOS"},
  {"k":"err", "t":"make: *** [test] Error 1"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"make release"},
  {"k":"err", "t":"GOOS=windows: command not found"},
  {"k":"dim", "t":"# Karen's laptop only. Don't ask why."}
]
</script>
<script type="application/json" data-pterm="t-make-after">
[
  {"k":"in",  "t":"perch test"},
  {"k":"out", "t":"==> Running unit tests"},
  {"k":"ok",  "t":"✓ 142 passed (3.1s)"},
  {"k":"out", "t":"==> Running integration tests"},
  {"k":"ok",  "t":"✓ 14 passed (8.4s)"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"perch release"},
  {"k":"ok",  "t":"✓ built darwin/arm64  (12 MB)"},
  {"k":"ok",  "t":"✓ built linux/amd64   (12 MB)"},
  {"k":"ok",  "t":"✓ built windows/amd64 (12 MB)"}
]
</script>

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
```

→ Walkthrough: **[tutorials/01-replace-your-makefile.md](tutorials/01-replace-your-makefile.md)**

</div>

### 2. Ship a Python / Node / monorepo project as one self-installing binary

<div class="perch-usecase">

**Before:** "first install pyenv, then python 3.11, then a venv, then pip install -r requirements.txt, then…" Three pages of README and a Slack channel for install help.

**After:** you hand them `stt_bin`. They run `./stt_bin install`. The binary extracts an embedded archive into `~/.cache/perch/<hash>/`, creates a venv, runs `pip install`, drops a launcher in `~/.local/bin/stt`. Done.

<div class="pterm" id="t-stt" data-title="stt_bin — a self-installing Python project"></div>
<script type="application/json" data-pterm="t-stt">
[
  {"k":"in",  "t":"perch --build -f commands.perch --include ./src -o stt_bin"},
  {"k":"out", "t":"Bundling ./src …"},
  {"k":"ok",  "t":"✓ embedded 487 KB"},
  {"k":"ok",  "t":"✓ Built binary: /abs/stt_bin"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"scp stt_bin ops@host:/usr/local/bin/"},
  {"k":"out", "t":"stt_bin                                100%  12MB"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"ssh ops@host 'stt_bin install'"},
  {"k":"dim", "t":"→ ensure_dir ~/.cache/perch/3f9a2b…"},
  {"k":"dim", "t":"→ bundle_extract → 47 files"},
  {"k":"dim", "t":"→ python3 -m venv .venv"},
  {"k":"dim", "t":"→ pip install -r requirements.txt"},
  {"k":"dim", "t":"→ link_into_path ~/.local/bin/stt"},
  {"k":"ok",  "t":"✓ installed. stt is on PATH."},
  {"k":"blank","t":""},
  {"k":"in",  "t":"ssh ops@host 'stt example.wav'"},
  {"k":"ok",  "t":"✓ transcribed (3.4s)  →  example.txt"}
]
</script>

```sh
perch --build -f commands.perch --include ./src -o stt_bin
scp stt_bin user@server:/usr/local/bin/
ssh user@server 'stt_bin install && stt example.wav'
```

The recipient needs **only** what your install command requires (here: `python3`). No package manager. No registry. No internet at install time.

→ Worked example: **[demos/05-python-installer](https://github.com/luowensheng/perch/tree/main/demos/05-python-installer)**

</div>

### 3. Give AI agents a safe operations surface — without standing up a backend

> **Deep dive: [LLM control plane](llm-control-plane.md) — why a `.perch` file + `perch-mcp` + a few CLI flags replaces 2,000 lines of FastAPI scaffolding.**


<div class="perch-usecase">

**Before:** the agent gets a shell. You hope. You write a long system prompt about what it should and shouldn't do. You audit logs after the fact.

**After:** the agent gets `perch-mcp` pointed at `ops.perch`. It can call exactly the verbs you declared, with exactly the arg types you declared. Anything else returns a typed error. No shell escape ever.

<div class="pterm" id="t-mcp" data-title="perch-mcp — the entire backend"></div>
<script type="application/json" data-pterm="t-mcp">
[
  {"k":"dim", "t":"# 1. Start the MCP server with restrictions"},
  {"k":"in",  "t":"perch-mcp --no-network --env KUBECONFIG,HOME -f ops.perch"},
  {"k":"dim", "t":"🔒 security: --no-network  --env KUBECONFIG,HOME"},
  {"k":"ok",  "t":"perch-mcp listening on stdio (3 commands declared)"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# 2. Agent calls a declared verb — works"},
  {"k":"in",  "t":"perch_run name=\"restart_pod\" ns=\"prod\" pod=\"api-3\""},
  {"k":"ok",  "t":"✓ pod \"api-3\" deleted"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# 3. Agent tries an undeclared verb — rejected"},
  {"k":"in",  "t":"perch_run name=\"drop_database\" db=\"prod\""},
  {"k":"err", "t":"error: command \"drop_database\" not declared in ops.perch"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# 4. Agent crafts a shell-injection arg — caught by your regex"},
  {"k":"in",  "t":"perch_run name=\"restart_pod\" ns=\"; rm -rf /\" pod=\"x\""},
  {"k":"err", "t":"error: invalid namespace"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# 5. Agent tries to read a secret env var"},
  {"k":"in",  "t":"perch_run name=\"leak_envs\""},
  {"k":"err", "t":"env var ${AWS_SECRET_KEY} is not in --env allowlist"},
  {"k":"accent","t":"→ one .perch file. zero backend code."}
]
</script>

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

<div class="pterm" id="t-dev" data-title="dev — your team's CLI"></div>
<script type="application/json" data-pterm="t-dev">
[
  {"k":"in",  "t":"dev up"},
  {"k":"out", "t":"==> docker compose up -d"},
  {"k":"out", "t":"==> docker compose exec api migrate up"},
  {"k":"ok",  "t":"✓ Stack running at http://localhost:8080"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"dev --help"},
  {"k":"out", "t":"USAGE: dev <command> [flags]"},
  {"k":"out", "t":"  up        Start the dev stack"},
  {"k":"out", "t":"  logs      Tail container logs"},
  {"k":"out", "t":"  shell     Open a shell in the api container"},
  {"k":"out", "t":"  reset     Wipe volumes + restart"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# unknown verbs fall through to docker"},
  {"k":"in",  "t":"dev ps"},
  {"k":"out", "t":"CONTAINER ID   IMAGE        STATUS"},
  {"k":"out", "t":"4f2a9b1c       myapp/api    Up 12 minutes"}
]
</script>

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

<div class="pterm" id="t-tour" data-title="from zero to shipping binary"></div>
<script type="application/json" data-pterm="t-tour">
[
  {"k":"in",  "t":"mkdir hello-perch && cd hello-perch"},
  {"k":"in",  "t":"perch --init"},
  {"k":"ok",  "t":"✓ wrote commands.perch"},
  {"k":"in",  "t":"perch --help"},
  {"k":"out", "t":"  hello     Say hello"},
  {"k":"in",  "t":"perch hello"},
  {"k":"ok",  "t":"Hello, world!"},
  {"k":"in",  "t":"perch --check"},
  {"k":"ok",  "t":"✓ commands.perch: 1 command — no issues"},
  {"k":"in",  "t":"perch --build -o ./greet"},
  {"k":"ok",  "t":"✓ Built binary: ./greet  (12 MB)"},
  {"k":"in",  "t":"./greet hello"},
  {"k":"ok",  "t":"Hello, world!"},
  {"k":"dim", "t":"# same file, four more frontends:"},
  {"k":"dim", "t":"#   perch --server   →  web UI"},
  {"k":"dim", "t":"#   perch --shell    →  REPL"},
  {"k":"dim", "t":"#   perch-mcp        →  MCP tool surface"}
]
</script>

---

## Common questions

**Is it a build tool or a CLI framework?** Both. Same file becomes a Make-style task runner *and* a Cobra-style typed CLI. Pick the surface (CLI / web / REPL / MCP / binary) that fits the caller.

**Is it a cross-platform shell?** Yes — and that's the point. With ~110 built-in ops (cp, mkdir, gzip, tar_create, http_get, sha256_file, regex_replace, …) you can write a script that runs identically on macOS / Linux / Windows without falling back to bash or cmd. Disable the `shell` op and you have a *pure* portable script. See [sandbox.md](sandbox.md) for the "pure" mode design.

**Can I see what a command will do before running it?** Yes — `perch --dry-run cmd` prints every op with its interpolated args and skips execution; `perch --ask cmd` is the same plan interactively (`y` = run, `n` = skip, `a` = run all remaining, `q` = quit). See it in the terminal below.

<div class="pterm" id="t-restrict" data-title="composable security flags"></div>
<script type="application/json" data-pterm="t-restrict">
[
  {"k":"in",  "t":"perch --no-shell --no-network -f deploy.perch run"},
  {"k":"dim", "t":"🔒 security: --no-shell --no-network"},
  {"k":"ok",  "t":"Starting deploy to staging"},
  {"k":"err", "t":"op \"shell\" is disabled by --no-shell"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"perch --env HOME,PATH -f deploy.perch run"},
  {"k":"dim", "t":"🔒 security: --env HOME,PATH"},
  {"k":"ok",  "t":"home=/Users/you"},
  {"k":"err", "t":"env var ${OPENAI_API_KEY} is not in --env allowlist"},
  {"k":"accent","t":"→ each flag names what it disables. they compose."}
]
</script>

<div class="pterm" id="t-ask" data-title="perch --ask deploy"></div>
<script type="application/json" data-pterm="t-ask">
[
  {"k":"in",  "t":"perch --ask deploy -target=staging"},
  {"k":"dim", "t":"──── Step-through preview — y=run, n=skip, a=all, q=quit ────"},
  {"k":"out", "t":"  [1] print msg=\"Starting deploy to staging\""},
  {"k":"accent","t":"       run? [y/n/a/q] > y"},
  {"k":"ok",  "t":"Starting deploy to staging"},
  {"k":"out", "t":"  [2] http_status \"https://api.example.com/healthz\"   → ${s}"},
  {"k":"accent","t":"       run? [y/n/a/q] > y"},
  {"k":"out", "t":"  [3] shell cmd=\"kubectl apply -f manifest.yaml\""},
  {"k":"accent","t":"       run? [y/n/a/q] > a"},
  {"k":"dim", "t":"       (running all remaining)"},
  {"k":"ok",  "t":"✓ deploy complete"}
]
</script>

**Can I lock down what a `.perch` file is allowed to do?** Yes — composable flags, each naming what it disables:

```sh
perch --no-shell --no-subprocess --no-network --no-write deploy
perch --env HOME,PATH,API_KEY deploy        # ${OTHER_SECRET} now errors
```

`--no-shell` blocks `shell`/`shell_output`/`shell_detached`/`try_shell`. `--no-subprocess` blocks `pkg_install`/`kill_by_name`/etc. `--no-network` blocks every `http_*`, `download`, `port_*`, etc. `--no-write` blocks every FS-mutation op. `--env` restricts which host env vars resolve via `${…}`. `perch --restrictions` lists exactly what each flag blocks. The full capability sandbox (FS roots, network host allowlists, `--untrusted` with permission previews, file-side `sandbox` block) is designed in [sandbox.md](sandbox.md).

**Who writes the sandbox — the author or the user?** Both. The **author** writes a `sandbox` block in the `.perch` file as a *manifest of intent* — "this is what I need to do my job." Reviewers audit it; `perch --check` enforces it statically. The **user** layers further restrictions at run time (`--mode`, `--allow-*`, `--untrusted`). The runtime enforces the *intersection* — neither side can grant more than the other allows. Same model as Android permissions: app declares, user grants, OS enforces the overlap. Details in [sandbox.md §2.5](sandbox.md#25-who-writes-the-sandbox-the-trust-model).

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
| [mcp.md](mcp.md) | AI agent integration (reference) |
| [llm-control-plane.md](llm-control-plane.md) | **Replace your LLM-tool backend with a `.perch` file** |
| [sandbox.md](sandbox.md) | **Sandboxing design — env / FS / net / shell scopes, `--untrusted`** |
| [lsp.md](lsp.md) | Editor integration |
| [applications.md](applications.md) | **What perch is for — 22 real applications** |
| [faq.md](faq.md) | vs Make / Just / Task / etc. |

Source on GitHub: [luowensheng/perch](https://github.com/luowensheng/perch). Apache-2.0.

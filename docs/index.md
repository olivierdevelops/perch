---
hide:
  - toc
---

# perch

> **perch is a cross-platform command runtime for defining, running, and shipping operational tools in a single structured file.**

Declare your commands once; run them consistently on macOS / Linux / Windows; expose them through a CLI, REPL, web UI, or AI-agent surface; and `perch --build` once to ship them as a single portable binary. **Replaces shell scripts + Makefiles + ad-hoc CLI wrappers + internal ops tools + "one-off automation repos"** with one declarative file that humans, CI, and agents all execute.

That's the one-sentence answer. The longer one: a `.perch` file *also* serves as a web UI (`--server`), a REPL (`--shell`), an MCP tool surface for AI agents (`perch-mcp`), and a `#!/usr/bin/env perch` script. Those extra frontends are **downstream consequences** of having a typed-CLI representation in one file, not separate systems. The primary abstraction is the file; everything else is rendering. Built on [capy](https://luowensheng.github.io/capy). Apache-2.0.

**Mental model:** *A structured way to define and ship operational commands that can run everywhere, with optional safety controls and multiple interfaces.* This enables a small but real pattern — **operational workflows as distributable products**: what used to be a folder of scripts plus a wiki page becomes one binary you can `scp` and run.

The deeper claim — that perch is "the operating system you can `scp`" — has its own page: **[Read the OS analogy](os-in-a-program.md)**.

<div id="perch-demo" class="perch-demo"></div>

The animated demo above cycles through the same `redis.perch` file in five rendering modes. **You write one file. perch renders every surface:**

<div id="perch-fanout"></div>

<div class="pstats">
  <div class="pstats-item"><div class="pstats-num" data-count="146">0</div><div class="pstats-label">built-in ops</div></div>
  <div class="pstats-item"><div class="pstats-num" data-count="30" data-suffix="+">0</div><div class="pstats-label">auto-bound vars</div></div>
  <div class="pstats-item"><div class="pstats-num" data-count="5">0</div><div class="pstats-label">frontends</div></div>
  <div class="pstats-item"><div class="pstats-num" data-count="4">0</div><div class="pstats-label">restriction flags</div></div>
  <div class="pstats-item"><div class="pstats-num" data-count="1">0</div><div class="pstats-label">binary to ship</div></div>
</div>

<div class="perch-cta">
  <a class="perch-cta__primary" href="getting-started/">Get started in 5 minutes →</a>
  <a class="perch-cta__secondary" href="applications/">See 22 real applications</a>
  <a class="perch-cta__secondary" href="https://github.com/luowensheng/perch">GitHub</a>
</div>

---

## What perch replaces

| Today | Tomorrow with perch |
|---|---|
| 📜 `bin/` of bash scripts + a Makefile + a CI YAML duplicating both | 🪶 **One `commands.perch` file** that local dev, CI, and on-call all execute |
| 🛠️ A bespoke Cobra / Click / Typer CLI with hand-rolled arg parsing | 🪶 **Typed args in declared verbs** with per-command `--help` for free |
| 🤖 A FastAPI service exposing safe ops to an LLM agent | 🪶 **`perch-mcp` reads the same file** — typed tools, capability gates, audit |
| 📋 A wiki page telling new hires which scripts to run | 🪶 **`perch --help`** + an optional **`perch --server`** web UI |
| 📦 "First install Python 3.11, then a venv, then pip install …" | 🪶 **`perch --build`** ships one binary with the project embedded |
| 🪵 ad-hoc `echo`-style logging + manual screenshot of CI output | 🪶 **`--audit FILE.ndjson`** structured trace + **`--report`** span tree |
| 🧪 "Run it and see" as the only test strategy | 🪶 **`perch test`** sandboxed behavior tests with `assert_*` ops |

**Adoption is incremental.** Wrap your existing `.sh` files in a `shell` op; gain typed args + `--help` + audit + MCP in minutes. Promote to native ops over time. [Migration guide →](migrating-from-shell.md)

---

## 📦 Ready-made recipes — install in one curl

22 curated `.perch` files that solve real problems. Local Redis, the
whole AI/observability/Kafka stack, cross-platform tool installers, daily
Docker/kubectl wrappers. Download one, audit with `perch --scan`, run it.

```sh
# Pick one. Run it.
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/recipes/redis.perch -o redis.perch
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/recipes/_lib.perch -o _lib.perch
perch --scan -f redis.perch        # audit before running
perch -f redis.perch up             # 8 verbs ready: up / down / cli / flush / monitor / logs / backup / status
```

| Pain | Recipe | One command |
|---|---|---|
| "I need Postgres + Redis + S3 locally for my web app" | **devstack** | `perch -f devstack.perch up` (Postgres + Redis + MinIO in parallel) |
| "I want to play with local LLMs" | **aistack** | `perch -f aistack.perch up` (Ollama + ChromaDB + Open WebUI) |
| "I need local metrics + logs + dashboards" | **observe** | `perch -f observe.perch up` (Prometheus + Grafana + Loki) |
| "I keep typing 12 docker flags wrong" | **docker-mgr** | `perch -f docker-mgr.perch prune_safe` |
| "Three teammates wrote three different `git pr` aliases" | **gh-flow** | `perch -f gh-flow.perch pr / land / sync / cleanup` |
| "I want every modern CLI tool installed cross-platform" | **modern-unix** | `perch -f modern-unix.perch install` (ripgrep, fd, bat, fzf, jq, yq, eza, zoxide) |
| "Set up local HTTPS for dev" | **mkcert-local** | `perch -f mkcert-local.perch install_ca && perch -f mkcert-local.perch cert localhost dev.local` |
| "Encrypted backups, no SaaS" | **backup** | `perch -f backup.perch snapshot ~/Documents` (restic wrapper) |

[**Browse all 22 recipes →**](recipes.md)

The full catalog: single services (redis, postgres, mongodb, mysql, mailpit,
minio, rabbitmq, localstack), stacks (devstack, aistack, observe,
kafka-stack), tool installers (modern-unix, clouds, node-stack,
python-stack), CLI wrappers (gh-flow, docker-mgr, kube-helpers),
ops/security (mkcert-local, backup, scan-secrets).

---

## Why teams adopt perch

<div class="perch-features">

<div class="card">
  <h4>🎯 One file, every surface</h4>
  <p>The same <code>commands.perch</code> is callable as a CLI, served as a web UI, steppable in a REPL, exposed to AI agents over MCP, and bundled into a portable binary. Everything else is rendering.</p>
</div>

<div class="card">
  <h4>🛡️ Safe by composition</h4>
  <p>Restriction flags compose — <code>--no-shell --no-network --env HOME,PATH</code>. Default-on SSRF and redirect guards. <code>--scan</code> audits a file before you run it. The grammar is the security boundary.</p>
</div>

<div class="card">
  <h4>🔒 <code>wasm_run</code> — the constrained execution lane</h4>
  <p>For when "capability-gated shell" isn't enough. Load a WebAssembly module under WASI; the module sees ONLY the argv, env vars, and filesystem mounts you declared — anything else is invisible <em>by construction</em>, not policy. Pure Go (wazero); no Docker, no daemon, no native sandbox setup. <a href="wasm/">Reference →</a> · <a href="wasm-walkthroughs/">5 walkthroughs →</a> · <a href="https://github.com/luowensheng/perch/tree/main/demos/wasm-plugin-host"><strong>The killer demo: AI-generated plugins running zero-trust →</strong></a></p>
</div>

<div class="card">
  <h4>🤖 Agent-native, no backend</h4>
  <p>Replace your LLM-tool backend with a <code>.perch</code> file. <code>perch-mcp --no-shell --no-network -f ops.perch</code> is the whole stack. Typed verbs, declared schemas, capability gates, audit log — what you'd otherwise build from scratch.</p>
</div>

<div class="card">
  <h4>🚀 Zero-install recipients</h4>
  <p><code>perch --build -o myapp</code> produces a single executable. Recipients run one file — no Go, no perch, no source clone. <code>--include ./src</code> embeds an entire Python / Node project alongside.</p>
</div>

<div class="card">
  <h4>🧪 Behavior tests built in</h4>
  <p>Mark a command <code>test</code>. <code>perch test</code> runs it in a sandboxed temp cwd with <code>--no-shell</code> / <code>--no-network</code> / <code>--no-subprocess</code> on by default. Seven <code>assert_*</code> ops. Drop into pre-commit + CI. <a href="testing/">Details →</a></p>
</div>

<div class="card">
  <h4>📊 Full visibility — <code>--trace</code> · <code>--audit</code> · <code>--report</code></h4>
  <p><code>--trace</code> streams every op to stderr <strong>as it fires</strong> (live, indented for block nesting). <code>--audit FILE.ndjson</code> writes the same events as JSON for downstream ingest. <code>--report</code> renders the span tree after the run with full error context. Same hook order, three audiences.</p>
</div>

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
perch test            # run every command marked `test` (sandboxed)
perch --report cmd    # execute + render the span tree
```

---

## Cross-platform without thinking about it

Every command starts with ~30 variables already bound. **No declaration, no `let`, no `if uname`.** Hover any row — that's what perch sees on the running machine.

<div id="perch-bindings"></div>

## What's in the box

The technical details that don't fit on the marketing panel above:

<div class="perch-features">

<div class="card">
  <h4>~140 cross-platform ops</h4>
  <p><code>cp</code>, <code>mkdir</code>, <code>gzip</code>, <code>tar_create</code>, <code>http_get</code>, <code>download</code>, <code>sha256_file</code>, <code>regex_replace</code>, <code>json_get</code>, <code>bundle_extract</code>, … all implemented in Go, identical on macOS / Linux / Windows. Skip the bash tax. <a href="op-reference/">Catalog →</a></p>
</div>

<div class="card">
  <h4>Static <code>--check</code> validator</h4>
  <p>Catches typo'd arg types, mismatched defaults, duplicate args, colliding positional indexes, missing <code>run TARGET</code>, unknown ops, unresolved <code>${name}</code> placeholders — before any command runs. Wire it into pre-commit.</p>
</div>

<div class="card">
  <h4>Templates &amp; execution contexts</h4>
  <p>Wrap any body in <code>parallel</code>, <code>timeout</code>, <code>retry</code>, <code>with_env</code>, <code>with_cwd</code>, <code>sandbox</code>, or <code>cache</code>. Lift repetition into <code>template NAME ... end</code> parameter-substitution stamps. <a href="execution-contexts/">Details →</a></p>
</div>

<div class="card">
  <h4>Unified <code>if EXPR ... end</code></h4>
  <p>Comparisons (<code>if os == "linux"</code>), truthy/falsy (<code>if has_bin</code>, <code>if not has_bin</code>), predicate calls (<code>if exists "./bin"</code>), numeric (<code>if size &gt; 1000000</code>). One block, every shape.</p>
</div>

<div class="card">
  <h4>Preview before running</h4>
  <p><code>--dry-run</code> prints every op with interpolated args and skips execution. <code>--ask</code> prompts y/n/a/q per op. <code>--scan</code> walks the program statically and reports needed capabilities + risk findings. No surprises in CI.</p>
</div>

<div class="card">
  <h4>Editor integration</h4>
  <p><code>perch-lsp</code> provides diagnostics, completion, hover, document outline. <code>perch --install-vscode</code> bundles the VS Code extension; <code>--install-lsp</code> alone for Neovim / Helix / Zed. Tree-sitter grammar for syntax beyond LSP. <a href="lsp/">Setup →</a></p>
</div>

<div class="card">
  <h4>Catch passthrough &amp; fuzzy suggestions</h4>
  <p>Levenshtein-based "Did you mean…?" for typo'd command names. Inside <code>catch</code>, <code>${proxy_args}</code> holds the full unknown invocation — <code>shell "git ${proxy_args}"</code> makes perch a drop-in superset of any tool.</p>
</div>

<div class="card">
  <h4>Block-shaped args + per-command <code>--help</code></h4>
  <p><code>arg NAME ... end</code> with labelled inner fields (<code>type</code>, <code>default</code>, <code>description</code>, <code>optional</code>, <code>index</code>, <code>rest</code>). <code>perch &lt;cmd&gt; --help</code> renders usage + table from the spec. No manual doc-strings.</p>
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

## For platform / SRE / security teams

The questions enterprise teams ask up-front, answered in one place:

<div class="perch-features">

<div class="card">
  <h4>🛡️ Security model</h4>
  <p><strong>Capability gating, not kernel sandboxing.</strong> Composable <code>--no-shell</code> / <code>--no-network</code> / <code>--no-write</code> / <code>--no-subprocess</code> flags. <code>--env A,B,C</code> restricts host-env visibility. <code>--allow-bin git,docker</code> narrows shell to argv[0]. <code>--allow-host api.github.com</code> restricts network. Layer with <code>firejail</code> / <code>sandbox-exec</code> / <code>AppContainer</code> for genuinely adversarial input. <a href="sandbox/">sandbox.md →</a></p>
</div>

<div class="card">
  <h4>🧾 Audit + replay</h4>
  <p><code>--audit FILE.ndjson</code> records every op call with timestamp, args, duration, error, exit code, and bound-variable state. Same shape as Linux auditd but at the op level. Pipe to Loki / Datadog / CloudWatch. <code>--report</code> renders the same stream as a human-readable span tree after the run.</p>
</div>

<div class="card">
  <h4>🤖 AI agent safety</h4>
  <p>The MCP boundary is the file's grammar. Agents call <em>declared verbs</em> with <em>typed args</em> — anything else is a typed error. <code>perch-mcp --no-shell --no-network --env KUBECONFIG -f ops.perch</code> is the full policy. No FastAPI scaffolding, no manual JSON-Schema, no agent-readable shell escape. <a href="llm-control-plane/">LLM control plane →</a></p>
</div>

<div class="card">
  <h4>🔒 SSRF + redirect protection (default-on)</h4>
  <p>HTTP ops refuse loopback / link-local / RFC 1918 / IPv6 ULA destinations by default — closes the AWS metadata SSRF (<code>169.254.169.254</code>). https→http downgrades refused. Max 5 redirect hops, each re-validated (DNS-rebinding defense via multi-A check). Layer <code>--allow-host</code> for a strict allowlist.</p>
</div>

<div class="card">
  <h4>🪟 Cross-platform parity</h4>
  <p>The ~140 ops are identical Go implementations across macOS / Linux / Windows. With <code>--no-shell</code> the boundary is airtight (no subprocess can fire). Real "works on my machine" elimination, not aspiration.</p>
</div>

<div class="card">
  <h4>📜 License + dependencies</h4>
  <p>Apache-2.0. <strong>One Go binary, no SaaS, no telemetry, no phone-home.</strong> Self-host or `go install`. Bundle into your own distribution. No license fees, no per-seat costs, no cloud account required. Source: <a href="https://github.com/luowensheng/perch">github.com/luowensheng/perch</a>.</p>
</div>

<div class="card">
  <h4>🧪 Pre-commit + CI integration</h4>
  <p><code>perch --check</code> for static validation, <code>perch test</code> for behavior. Both exit non-zero on failure — wire into any CI. Per-test sandboxes prevent state leakage. The same <code>.perch</code> drives local dev, CI, and production. <a href="testing/">testing.md →</a></p>
</div>

<div class="card">
  <h4>🔍 Static audit of unknown scripts</h4>
  <p><code>perch --scan FILE</code> walks a program WITHOUT executing it and reports: capabilities needed, env vars referenced, risk findings (sudo, catch passthrough to shell, unvalidated <code>${var}</code> in shell args), and the tightest CLI invocation that should still let it run. Review third-party <code>.perch</code> files before adopting them.</p>
</div>

<div class="card">
  <h4>📊 Status &amp; maturity</h4>
  <p><strong>Pre-1.0 (v0.x).</strong> DSL surface is stable; op catalog continues to grow. SemVer applies once v1.0 is tagged. CI runs the full test suite on every commit. The repo eats its own dog food — <a href="https://github.com/luowensheng/perch/blob/main/commands.perch"><code>commands.perch</code></a> is what builds / tests / cleans perch itself.</p>
</div>

</div>

---

## How it compares

<div class="perch-compare-wrap" markdown="1">

| | perch | Make | Just | Task | bash scripts | Cobra/Click |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| Cross-platform without `if uname` | ✅ | ⚠️ | ⚠️ | ✅ | ❌ | ✅ |
| Typed args + per-command `--help` | ✅ | ❌ | ❌ | ⚠️ | ❌ | ✅ |
| Static validator (`--check`) | ✅ | ❌ | ❌ | ❌ | ❌ | ⚠️ |
| Sandboxed behavior tests (`perch test`) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `parallel` / `retry` / `timeout` / `cache` blocks | ✅ | ⚠️ | ❌ | ⚠️ | ❌ | ❌ |
| Span-tree execution report (`--report`) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
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
  {"k":"in",  "t":"perch test"},
  {"k":"ok",  "t":"✓ test_hello_says_hello                     (412µs)"},
  {"k":"ok",  "t":"1 passed, 0 failed in 412µs."},
  {"k":"in",  "t":"perch --trace hello"},
  {"k":"dim", "t":"▸ print                \"Hello, world!\""},
  {"k":"ok",  "t":"Hello, world!"},
  {"k":"dim", "t":"✓                                (45µs)"},
  {"k":"in",  "t":"perch --report hello"},
  {"k":"ok",  "t":"Hello, world!"},
  {"k":"dim", "t":"── perch trace ─────────────────────────────────"},
  {"k":"dim", "t":"✓ hello (2ms)"},
  {"k":"dim", "t":"└─ ✓ print \"Hello, world!\" (45µs)"},
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

**How do I audit a `.perch` file I didn't write?** `perch --scan FILE` walks the program statically — no execution — and reports:

- **Capabilities needed.** Does it need shell? Which binaries? Network? Which hosts? Writes? Which paths?
- **Env vars referenced.** Every `${UPPERCASE_NAME}` it touches.
- **Risk findings.** Sudo use, catch-passthrough to shell, unvalidated `${var}` in shell args, downloads followed by `make_executable`, etc. Each rated `HIGH` / `MED` / `LOW` with a concrete fix suggestion.
- **Recommended invocation.** The tightest CLI flag combination that should still let the script run — assembled automatically. Hands you the safe command to copy-paste.

Try it: <code>perch --scan -f deploy.perch</code>. See the animated demo above.

**I already have bash scripts — what's the migration story?** Three options, ranked by effort: **(1) Wrap** — write a thin `.perch` that calls your existing `.sh` files via the `shell` op; gain typed args + `--help` + MCP + web UI + audit log in minutes. **(2) Translate** — `perch --import deploy.sh` produces a `.perch` scaffold preserving semantics line-for-line (mostly `shell` ops to start), reviewable + statically checkable. **(3) Rewrite** — promote each `shell` op to native ops over time; once nothing needs `shell`, `--no-shell` becomes a real fence. Full guide: **[migrating-from-shell.md](migrating-from-shell.md)**.

**Is it a build tool or a CLI framework?** Both. Same file becomes a Make-style task runner *and* a Cobra-style typed CLI. Pick the surface (CLI / web / REPL / MCP / binary) that fits the caller.

**Are HTTP redirects and SSRF handled?** Yes — four layered protections, all default-on. Plus a strict host allowlist when you want to pin which domains the script can reach.

| Default | What it stops |
|---|---|
| Block private-IP requests + redirects | AWS metadata (`169.254.169.254`), localhost pivot, RFC 1918 pivot |
| Block https → http redirects | scheme downgrade |
| Cap at 5 redirect hops | redirect bombing |
| DNS-rebinding defense | multi-A responses get ALL records checked |

**`--allow-host HOST[,HOST...]`** (additive, repeatable) layers a strict allowlist on top. Every initial URL AND every redirect destination must match. Patterns: exact (`api.github.com`), single-label wildcard (`*.s3.amazonaws.com`), host:port (`localhost:8080`), IP literal. Composes AND-wise with the SSRF guard.

```sh
# Only api.github.com and the docker registry are reachable —
# anything else returns "host not in --allow-host allowlist".
perch --allow-host api.github.com,registry.docker.io,*.docker.io deploy
```

Opt-out flags for the genuine cases: `--allow-private-ips`, `--allow-scheme-downgrade`, `--max-redirects N`, `--no-redirects`. Run `perch help --allow-host` for the full story.

<div class="pterm" id="t-ssrf" data-title="SSRF / allowlist protection"></div>
<script type="application/json" data-pterm="t-ssrf">
[
  {"k":"dim", "t":"# default: refuses private IPs / metadata SSRF"},
  {"k":"in",  "t":"perch -f script.perch fetch_metadata"},
  {"k":"err", "t":"169.254.169.254 is a link-local address"},
  {"k":"err", "t":"(use --allow-private-ips to permit)"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# host allowlist — only github + docker reachable"},
  {"k":"in",  "t":"perch --allow-host api.github.com,*.docker.io -f script.perch run"},
  {"k":"out", "t":"github status = 200"},
  {"k":"err", "t":"host \"evil.com\" is not in --allow-host allowlist"},
  {"k":"err", "t":"(allowed: api.github.com, *.docker.io)"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# redirect to off-list host — also refused"},
  {"k":"in",  "t":"perch --allow-host api.github.com -f script.perch via_redirect"},
  {"k":"err", "t":"redirect refused: host \"attacker.com\" is not in --allow-host"},
  {"k":"accent","t":"→ EVERY redirect target is re-validated, including DNS-rebinding multi-A"}
]
</script>

**Where do I look up what a flag or concept means?** `perch help` — auto-generated reference. Three surfaces share the same catalog:

```sh
perch help                    # top-level index, grouped by Execution / Authoring / Security / …
perch help --no-shell         # detail on one flag
perch help shebang            # or one concept
perch help shell              # fuzzy match (4 results in this case)
perch help --json             # full machine-readable dump — for agents and tooling
```

Every error message includes a `perch help <topic>` hint pointing exactly at the right entry: `op "shell" is disabled by --no-shell — run perch help --no-shell for details`. Both humans and AI agents land on the same canonical reference.

**How do I install it / how do I run a remote `.perch` file?** Two one-liners.

Install (macOS / Linux / WSL — picks the right binary for your platform):

```sh
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh | sh
```

Run a `.perch` file straight from a URL (no save-to-disk step), with restrictions you choose:

```sh
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/sample.perch \
  | perch --no-shell --no-network -f - hello
```

`-f -` means "read the perch source from stdin." **Stdin input is treated as untrusted by default** — shell, subprocess, network, write, and host env-var visibility are all disabled. Grant capabilities explicitly:

```sh
# default: nothing dangerous can fire
curl URL | perch -f - run

# I'm okay with this script using shell:
curl URL | perch -f - --allow-shell run

# I'm okay with shell + network + 2 env vars:
curl URL | perch -f - --allow-shell --allow-network --env HOME,API_KEY run

# I trust this pipe completely (it's my own .perch):
cat my.perch | perch -f - --trust-stdin run
```

Same model Deno uses: deny-by-default, opt-in with `--allow-*`. Banner shows "🔒 stdin (untrusted): ..." with the exact flags blocking each capability. **File input (`-f file.perch`) is unchanged** — the deny-by-default only applies to stdin since that's where untrusted scripts arrive.

**Can I run `.perch` files as scripts (shebang)?** Yes. `perch --init` writes a `#!/usr/bin/env perch` line at the top and sets the file executable. Then `./commands.perch` runs the `main` command; `./commands.perch hello` runs `hello`; everything between just works. Conceptually a `.perch` file is a script *and* a structured CLI surface — both at once.

```sh
$ perch --init
$ chmod +x commands.perch
$ ./commands.perch           # runs the `main` command
$ ./commands.perch hello     # runs `hello`
$ ./commands.perch --help    # lists commands
```

The shebang line is just a `#` comment to perch's parser, so it has no effect on parsing.

**Is it a cross-platform shell?** Yes — and that's the point. With ~140 built-in ops (cp, mkdir, gzip, tar_create, http_get, sha256_file, regex_replace, …) you can write a script that runs identically on macOS / Linux / Windows without falling back to bash or cmd. Disable the `shell` op and you have a *pure* portable script. See [sandbox.md](sandbox.md) for the "pure" mode design.

**Can I see what a command will do before running it?** Yes — `perch --dry-run cmd` prints every op with its interpolated args and skips execution; `perch --ask cmd` is the same plan interactively (`y` = run, `n` = skip, `a` = run all remaining, `q` = quit). See it in the terminal below.

<div class="pterm" id="t-pipe" data-title="install + pipe — one-liners"></div>
<script type="application/json" data-pterm="t-pipe">
[
  {"k":"dim", "t":"# install in one line:"},
  {"k":"in",  "t":"curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh | sh"},
  {"k":"ok",  "t":"✓ perch v0.6.0 installed to /usr/local/bin/perch"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"# run a remote .perch file straight from a URL — never lands on disk:"},
  {"k":"in",  "t":"curl -fsSL https://example.com/scripts/setup.perch | perch -f - --no-shell run"},
  {"k":"dim", "t":"🔒 security: --no-shell"},
  {"k":"ok",  "t":"setting up project..."},
  {"k":"err", "t":"op \"shell\" is disabled by --no-shell"},
  {"k":"accent","t":"→ pipe untrusted scripts through restrictions before they touch your machine"}
]
</script>

<div class="pterm" id="t-shebang" data-title=".perch files are scripts too"></div>
<script type="application/json" data-pterm="t-shebang">
[
  {"k":"in",  "t":"perch --init"},
  {"k":"ok",  "t":"✓ wrote commands.perch"},
  {"k":"dim", "t":"Or run it as a script (perch must be on $PATH):"},
  {"k":"dim", "t":"  chmod +x commands.perch"},
  {"k":"dim", "t":"  ./commands.perch          # runs the `main` command"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"head -1 commands.perch"},
  {"k":"out", "t":"#!/usr/bin/env perch"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"chmod +x commands.perch"},
  {"k":"in",  "t":"./commands.perch"},
  {"k":"ok",  "t":"Hello from /Users/you"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"./commands.perch hello"},
  {"k":"ok",  "t":"Hello from /Users/you"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"./commands.perch --help"},
  {"k":"out", "t":"  hello   Say hello"},
  {"k":"out", "t":"  main    Default action when the file runs as a script"}
]
</script>

<div class="pterm" id="t-scan" data-title="perch --scan — static security audit"></div>
<script type="application/json" data-pterm="t-scan">
[
  {"k":"in",  "t":"perch --scan -f deploy.perch"},
  {"k":"dim", "t":"── deploy.perch ─────────────────────────────────"},
  {"k":"out", "t":"3 commands, 1 catch, 2 globals"},
  {"k":"blank","t":""},
  {"k":"out", "t":"CAPABILITIES NEEDED"},
  {"k":"out", "t":"  ✓ shell       (5 calls: docker, kubectl, rm, sudo)  ⚠ uses sudo"},
  {"k":"out", "t":"  ✗ subprocess  — add --no-subprocess for free"},
  {"k":"out", "t":"  ✓ network     (1 host: api.github.com)"},
  {"k":"out", "t":"  ✓ writes      (roots: ${APP_DIR}/last-deploy.txt)"},
  {"k":"blank","t":""},
  {"k":"out", "t":"ENV VARS REFERENCED"},
  {"k":"out", "t":"  APP_DIR, HOME"},
  {"k":"blank","t":""},
  {"k":"out", "t":"RISK FINDINGS"},
  {"k":"err", "t":"  [HIGH] admin op #1: shell uses `sudo` — privilege escalation"},
  {"k":"err", "t":"  [MED]  catch forwards ${proxy_args} to shell — any input → shell"},
  {"k":"err", "t":"  [LOW]  remove op #1: shell interpolates ${path} with no validation"},
  {"k":"blank","t":""},
  {"k":"accent","t":"RECOMMENDED INVOCATION"},
  {"k":"ok",  "t":"  perch --no-subprocess --allow-bin docker,kubectl,rm,sudo \\"},
  {"k":"ok",  "t":"        --no-shell-metachars --env APP_DIR,HOME \\"},
  {"k":"ok",  "t":"        --max-runtime 600 --audit /var/log/perch-deploy.ndjson \\"},
  {"k":"ok",  "t":"        -f deploy.perch"}
]
</script>

<div class="pterm" id="t-trace" data-title="perch --trace deploy — live op stream as it runs"></div>
<script type="application/json" data-pterm="t-trace">
[
  {"k":"in",  "t":"perch --trace -f release.perch deploy"},
  {"k":"dim", "t":"▸ sandbox              flags=\"no_network\""},
  {"k":"dim", "t":"  ▸ with_lock            \"prod-deploy\""},
  {"k":"dim", "t":"    ▸ acquire_lock         \"prod-deploy\""},
  {"k":"dim", "t":"    ✓                            (12ms)"},
  {"k":"dim", "t":"    ▸ retry                attempts=3"},
  {"k":"dim", "t":"      ▸ shell                \"kubectl apply -f manifest.yaml\""},
  {"k":"out", "t":"configmap/api-config configured"},
  {"k":"out", "t":"deployment.apps/api configured"},
  {"k":"dim", "t":"      ✓                            (1.20s)"},
  {"k":"dim", "t":"    ✓                              (1.20s)"},
  {"k":"dim", "t":"    ▸ release_lock         \"prod-deploy\""},
  {"k":"dim", "t":"    ✓                            (8ms)"},
  {"k":"dim", "t":"  ✓                              (1.22s)"},
  {"k":"dim", "t":"✓                                (1.22s)"},
  {"k":"accent","t":"→ each op prints the moment it fires; kubectl output appears in line"},
  {"k":"accent","t":"  → no waiting for the whole run to finish to see what's happening"}
]
</script>

<div class="pterm" id="t-wasm" data-title="wasm_run — capability gating by construction"></div>
<script type="application/json" data-pterm="t-wasm">
[
  {"k":"dim", "t":"# A .perch file that runs a WASM module:"},
  {"k":"dim", "t":"wasm_run \"./hello.wasm\""},
  {"k":"dim", "t":"    wasm_arg  \"alice\""},
  {"k":"dim", "t":"    wasm_env  \"GREETING,HOME\"     # only these env vars pass"},
  {"k":"dim", "t":"    wasm_mount_read  \"./src\"      # mounted /ro/src"},
  {"k":"dim", "t":"    wasm_mount_write \"./bin\"      # mounted /rw/bin"},
  {"k":"dim", "t":"end"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"SECRET=leak GREETING=hi perch -f demo demo"},
  {"k":"out", "t":"── hello.wasm ────────────────────────────────"},
  {"k":"out", "t":"argv: [hello.wasm alice bob]"},
  {"k":"ok",  "t":"env GREETING: hi"},
  {"k":"ok",  "t":"env HOME: /Users/you"},
  {"k":"err", "t":"env SECRET: (not visible — not in allowlist)"},
  {"k":"err", "t":"env PATH:   (not visible — not in allowlist)"},
  {"k":"ok",  "t":"fs /ro/src:    visible"},
  {"k":"ok",  "t":"fs /rw/bin:    visible"},
  {"k":"err", "t":"fs /etc/passwd: (not visible — not mounted)"},
  {"k":"accent","t":"→ host SECRET set but invisible to module — enforced by the WASM runtime"},
  {"k":"accent","t":"  → no syscall escape; nothing not declared exists"}
]
</script>

<div class="pterm" id="t-audit" data-title="--audit FILE.ndjson — structured trace"></div>
<script type="application/json" data-pterm="t-audit">
[
  {"k":"in",  "t":"perch --no-shell --audit /var/log/agent.ndjson -f ops.perch deploy"},
  {"k":"dim", "t":"🔒 security: --no-shell  --audit /var/log/agent.ndjson"},
  {"k":"ok",  "t":"Deploying…"},
  {"k":"err", "t":"op \"shell\" is disabled by --no-shell"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"tail -3 /var/log/agent.ndjson"},
  {"k":"dim", "t":"{\"event\":\"op\",\"kind\":\"print\",\"args\":{\"msg\":\"Deploying…\"},\"dur_ms\":0,\"ok\":true}"},
  {"k":"dim", "t":"{\"event\":\"op\",\"kind\":\"shell\",\"args\":{\"cmd\":\"kubectl …\"},\"dur_ms\":0,\"ok\":false,"},
  {"k":"dim", "t":"  \"error\":\"op \\\"shell\\\" is disabled by --no-shell\"}"},
  {"k":"dim", "t":"{\"event\":\"session_end\",\"dur_ms\":42,\"ok\":false,\"error\":\"…\"}"},
  {"k":"accent","t":"→ blocked calls are recorded too — exactly what audit wants."}
]
</script>

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

**Who writes the sandbox — the author or the user?** Both. The **author** writes a `sandbox` block in the `.perch` file as a *manifest of intent* — "this is what I need to do my job." Reviewers audit it; `perch --check` enforces it statically. The **user** layers further restrictions at run time (`--no-shell`, `--no-network`, `--no-write`, `--env`, `--allow-*`, `--untrusted`). The runtime enforces the *intersection* — neither side can grant more than the other allows. Same model as Android permissions: app declares, user grants, OS enforces the overlap. Details in [sandbox.md §2.5](sandbox.md#25-who-writes-the-sandbox-the-trust-model).

**Do recipients need to install perch?** No. `perch --build` produces a standalone binary. They run that.

**Does it work on Windows?** Yes. The ~140 built-in ops are Go implementations, identical on macOS / Linux / Windows. Only `shell` invocations are inherently OS-specific.

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
<li><a href="op-reference/">Op catalog (~140 ops)</a></li>
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

Grouped by what you're trying to do. Each row is one page.

### 🚀 Get started in 30 minutes

| | |
|---|---|
| [**recipes.md**](recipes.md) | **22 ready-to-run `.perch` files — Redis, Postgres, devstack, aistack, observe, kubectl helpers, …** |
| [getting-started.md](getting-started.md) | Five-minute hands-on tour |
| [migrating-from-shell.md](migrating-from-shell.md) | Wrap your existing `.sh` files — three migration strategies |
| [tutorials/01-replace-your-makefile.md](tutorials/01-replace-your-makefile.md) | Convert a real Makefile to perch |
| [faq.md](faq.md) | vs Make / Just / Task / Cobra; the common questions |

### 🛠️ Author commands (developers)

| | |
|---|---|
| [language.md](language.md) | Every keyword, modifier, and operator |
| [op-reference.md](op-reference.md) | The built-in op catalog (~140 ops) |
| [execution-contexts.md](execution-contexts.md) | **`parallel` / `retry` / `timeout` / `sandbox` / `cache` blocks + templates + `--report`** |
| [testing.md](testing.md) | **`perch test` — sandboxed behavior tests with `assert_*` ops** |
| [lsp.md](lsp.md) | VS Code / Neovim / Helix / Zed integration |
| [applications.md](applications.md) | **22 real applications worth copying** |

### 📦 Ship as a product

| | |
|---|---|
| [tutorials/02-ship-a-tool.md](tutorials/02-ship-a-tool.md) | `perch --build` deep-dive |
| [tutorials/03-cross-platform-installer.md](tutorials/03-cross-platform-installer.md) | One installer for three OSes |
| [embedding.md](embedding.md) | Fat-binary format spec — what's inside, how to verify it |

### 🛡️ Adopt at scale (platform / SRE / security)

| | |
|---|---|
| [sandbox.md](sandbox.md) | **Capability model — env / FS / net / shell scopes, `--untrusted`, file-side `sandbox` blocks** |
| [llm-control-plane.md](llm-control-plane.md) | **Replace your LLM-tool backend with a `.perch` file** |
| [mcp.md](mcp.md) | MCP server reference (JSON-RPC over stdio) |
| [applications.md](applications.md) | 22 patterns; many are SRE / platform-team shaped |

### 🪞 Background reading (design intent)

| | |
|---|---|
| [os-in-a-program.md](os-in-a-program.md) | The "operating system you can `scp`" framing |
| [user-experience.md](user-experience.md) | UX roadmap |
| [ai-assisted-authoring.md](ai-assisted-authoring.md) | Notes on agent-authored `.perch` files |

Source on GitHub: [**luowensheng/perch**](https://github.com/luowensheng/perch). Apache-2.0. One Go binary, no SaaS, no telemetry.

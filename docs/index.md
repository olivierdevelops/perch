---
hide:
  - toc
---

# perch

<div style="background:rgba(220,38,38,.08);border:1px solid rgba(220,38,38,.35);border-radius:10px;padding:12px 16px;margin:14px 0 20px;font-size:13px;line-height:1.5">
<strong>🚫 perch is NOT accepting external contributions at this time.</strong><br>
Pull requests will be closed unread. Feature ideas → <a href="https://github.com/olivierdevelops/perch/discussions">GitHub Discussions</a>. Bug reports for shipped behavior → <a href="https://github.com/olivierdevelops/perch/issues/new?template=bug.yml">issues</a> (welcome). The code is Apache-2.0 — fork freely. Full policy in <a href="https://github.com/olivierdevelops/perch/blob/main/CONTRIBUTING.md">CONTRIBUTING.md</a>. This stance is "for now" and will change once the grammar / op catalog stabilises.
</div>

> **One file. Every surface.** Define your commands once in a `.perch` file — run them as a CLI, a **web UI** 🪟, a REPL, an **AI-agent tool** 🤖, or a portable binary. macOS · Linux · Windows.

<style>
.qbits{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin:20px 0 8px}
.qbit{background:var(--md-default-bg-color);border:1px solid var(--md-default-fg-color--lightest);border-radius:10px;padding:14px 12px;text-align:center;transition:transform .15s}
.qbit:hover{transform:translateY(-2px);border-color:var(--md-accent-fg-color)}
.qbit .ico{font-size:28px;line-height:1;margin-bottom:6px}
.qbit .ttl{font-size:13px;font-weight:600;color:var(--md-default-fg-color);margin-bottom:2px}
.qbit .sub{font-size:11px;color:var(--md-default-fg-color--light);line-height:1.35}
.replaces{display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:10px;margin:14px 0}
.rep{background:var(--md-default-bg-color);border:1px solid var(--md-default-fg-color--lightest);border-radius:8px;padding:10px 12px;display:flex;gap:10px;align-items:center;font-size:13px}
.rep .ico{font-size:22px;line-height:1}
.rep .txt{color:var(--md-default-fg-color);font-weight:500}
.rep .txt .strike{color:var(--md-default-fg-color--lighter);text-decoration:line-through;font-weight:400;font-size:11px;display:block;margin-top:2px}
</style>

<div class="qbits">
  <div class="qbit"><div class="ico">📝</div><div class="ttl">One file</div><div class="sub">Replaces bash + Make + a Cobra/Click/Typer CLI</div></div>
  <div class="qbit"><div class="ico">🪟</div><div class="ttl">Web UI</div><div class="sub">No frontend to write — <code>--server</code> is the dashboard</div></div>
  <div class="qbit"><div class="ico">🤖</div><div class="ttl">AI agent tool</div><div class="sub">Same file as MCP server — typed verbs, capability gates</div></div>
  <div class="qbit"><div class="ico">📦</div><div class="ttl">One binary</div><div class="sub"><code>--build</code> ships a portable executable, no Go needed</div></div>
  <div class="qbit"><div class="ico">🛡️</div><div class="ttl">Safe by default</div><div class="sub">SSRF/redirect guards · capability flags · audit log</div></div>
  <div class="qbit"><div class="ico">⚡</div><div class="ttl">Cross-platform</div><div class="sub">macOS · Linux · Windows · same 146 built-in ops</div></div>
</div>

<div id="perch-demo" class="perch-demo"></div>

*Same `redis.perch` file, five rendering modes — animation cycles through each.*

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
  <a class="perch-cta__secondary" href="https://github.com/olivierdevelops/perch">GitHub</a>
</div>

---

## The three things perch does that nothing else does

<div class="perch-features">

<div class="card">
  <h4>🎯 One file → five surfaces</h4>
  <p>One <code>commands.perch</code> → <strong>CLI · web UI · REPL · MCP tool · portable binary</strong>. Not five integrations — one abstraction, five renderings.</p>
  <p style="font-size:11px;color:var(--md-default-fg-color--light);margin:6px 0 0">Nothing in bash / Make / Click / Cobra / Just / Task does this.</p>
</div>

<div class="card">
  <h4>🔒 <code>wasm_run</code> — sandbox by construction</h4>
  <p>Load a WebAssembly module. It sees ONLY the argv, env vars, and mounts you <em>declared</em> — anything else <strong>doesn't exist</strong>. Not policy. Construction.</p>
  <p style="font-size:11px;color:var(--md-default-fg-color--light);margin:6px 0 0">🎯 <a href="https://github.com/olivierdevelops/perch/tree/main/demos/wasm-plugin-host"><strong>Killer demo: zero-trust AI plugin runtime</strong></a> · <a href="wasm/">reference</a> · <a href="wasm-walkthroughs/">5 walkthroughs</a></p>
</div>

<div class="card">
  <h4>🧪 <code>perch simulate</code> — "what would happen on THAT host?" <span style="background:rgba(124,131,248,.15);color:#7c83f8;padding:1px 6px;border-radius:3px;font-size:.7em;vertical-align:middle;font-weight:700">v2</span></h4>
  <p>Walk the program against a <strong>hypothetical env</strong>. Per-op verdicts: <span style="color:#16a34a">WILL_RUN ✓</span> · <span style="color:#dc2626">WILL_FAIL ✗</span> · <span style="color:#d97706">MIGHT_FAIL ?</span>. <strong>v2</strong>: state threading, oracles, multi-scenario.</p>
  <p style="font-size:11px;color:var(--md-default-fg-color--light);margin:6px 0 0"><a href="simulate/"><strong>Details →</strong></a></p>
</div>

</div>

<style>
.shipped-head{font-size:.95em;font-weight:700;margin:1.2em 0 .5em}
.shipped-head .bk{font-size:.75em;font-weight:600;color:var(--md-default-fg-color--light)}
ul.shipped{list-style:none;padding:0;margin:0;display:grid;grid-template-columns:repeat(2,1fr);gap:6px 22px}
@media (max-width:720px){ul.shipped{grid-template-columns:1fr}}
ul.shipped li{font-size:.86em;line-height:1.5;color:var(--md-default-fg-color--light);padding:5px 0;border-top:1px solid var(--md-default-fg-color--lightest)}
ul.shipped li .lead{color:var(--md-default-fg-color);font-weight:700}
ul.shipped .brk{font-size:.72em;font-weight:700;color:#d97706;border:1px solid #fcd34d;border-radius:3px;padding:0 4px;margin-left:4px;vertical-align:middle}
</style>

<p class="shipped-head">Also recently shipped <span class="bk">— everything else that landed</span></p>

<ul class="shipped">
<li>📜 <a href="requires/"><span class="lead"><code>requires</code> manifest</span></a> — declare every external resource (<code>bin</code> · <code>env</code> · <code>host</code> · <code>read</code>/<code>write</code> · OS · arch). Every external op verifies it before running; undeclared access errors. <a href="capability-gating/">Gated, every time →</a></li>
<li>🔐 <span class="lead">SHA-256 bin pinning</span> — <code>hash "sha256:…"</code> / <code>hash_file "bundle:…"</code> compares bytes on disk; never executes the binary.</li>
<li>🖥️ <span class="lead">OS + arch context blocks</span> — <code>os "…" … end</code> · <code>arch "…" … end</code> declare cross-platform/arch branches structurally; compose for matrix builds. <code>simulate</code> prunes mismatched leaves as dead code.</li>
<li>🪢 <span class="lead">Keyword-free dispatch</span><span class="brk">breaking</span> — no <code>run</code>/<code>call</code>: a bare name invokes a command, expands a template, or runs a declared bin (<code>deploy -target=linux</code>). Names are globally unique, so it's unambiguous; <code>--check</code> errors on any collision.</li>
<li>🏷️ <span class="lead">Bare top-level bindings + <code>bin … as</code></span><span class="brk">breaking</span> — declare shared values bare (no <code>globals</code> block); reference them in <code>requires</code> by name (<code>write BUILD_DIR</code>); give a path-bin a handle (<code>bin "./bins/x" as x</code>).</li>
<li>🔒 <span class="lead">Catch needs <code>proxy_args</code></span><span class="brk">breaking</span> — the catch→shell forwarding <code>--scan</code> flagged HIGH can't happen implicitly; <code>${proxy_args}</code> is unbound without the modifier.</li>
<li>🔢 <span class="lead">Version checks like math</span> — <code>assert_version "${v}" >= "1.28.0"</code> (infix) + <code>version_extract</code> to pull a version from any tool's output. Semver-aware.</li>
<li>🎯 <span class="lead">Risk score in <code>--scan</code></span> — one glance: 🟢 SAFE / 🟡 LOW / 🟠 MED / 🔴 HIGH with concrete reasons; surfaced as a colored pill in the web UI.</li>
<li>🛟 <a href="errors/"><span class="lead"><code>try/rescue/finally</code> + <code>match</code></span></a> — 30-kind error enum (<code>shell_exit_nonzero</code>, <code>http_ssrf_blocked</code>, …) for structured recovery + cleanup.</li>
<li>🪟 <a href="web-ui/"><span class="lead">Web UI for non-devs</span></a> — the same <code>.perch</code> file served as a tabbed localhost app (Run / Simulate / Scan / Check / About).</li>
<li>📡 <a href="mcp/#streaming-progress"><span class="lead">MCP live streaming</span></a> — per-line stdout/stderr as <code>notifications/progress</code> events; no silent waits on long verbs.</li>
<li>📦 <a href="recipes/"><span class="lead">22 ready-made recipes</span></a> — Redis · Postgres · devstack · aistack · kafka · modern-unix · gh-flow · docker-mgr · mkcert · backup · scan-secrets.</li>
<li>🧪 <a href="wasm-walkthroughs/"><span class="lead">3 runnable <code>wasm_run</code> demos</span></a> — schema validator, K8s policy check, agent-safe diff summarizer.</li>
<li>📦 <a href="wasm/#embedded-modules-declare-once-with-as-name"><span class="lead">Declarative <code>bundle</code> + aliases</span></a> — <code>include "./mod.wasm" as mod</code> → <code>wasm_run mod</code> (bare ident, zero disk reads).</li>
</ul>

---

## The philosophy — what perch is trying to do

Every team has an *operational layer*: the scripts that build the project, spin up the dev stack, deploy the service, wrap the clunky CLI, run the runbook. It's the least-loved code you own — and you usually write it **five times in five incompatible languages**. A bash script for the terminal. A Makefile for CI. A Cobra/Click/Typer program when the bash gets too hairy. A little Flask dashboard so non-devs can click a button. An MCP/JSON-tool backend now that an agent needs to run it too. **Five descriptions of the same handful of verbs**, drifting apart the moment they're written.

perch's bet is that this is one program, not five — and that you should describe it **once**.

<style>
.philo{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:14px;margin:20px 0}
.philo .p{background:var(--md-default-bg-color);border:1px solid var(--md-default-fg-color--lightest);border-left:3px solid var(--md-accent-fg-color);border-radius:8px;padding:14px 16px}
.philo .p h4{margin:0 0 4px;font-size:14px;color:var(--md-default-fg-color)}
.philo .p p{margin:0;font-size:13px;line-height:1.55;color:var(--md-default-fg-color--light)}
</style>

<div class="philo">
  <div class="p">
    <h4>① Describe once, render many</h4>
    <p>You declare typed verbs in one <code>.perch</code> file. Being a CLI, a web form, a REPL command, an MCP tool, and a binary entry point is the <em>tool's</em> job — not yours. No schema written five times, nothing to keep in sync.</p>
  </div>
  <div class="p">
    <h4>② The grammar is the security boundary</h4>
    <p>Capabilities are <strong>declared and gated</strong>, not bolted on after. A file states what it touches (<code>requires</code>); the operator narrows what's allowed (<code>--no-*</code>); neither side can exceed the other. You can know what a file does <em>before</em> you run it.</p>
  </div>
  <div class="p">
    <h4>③ Cross-platform is the runtime's problem</h4>
    <p>Common operations — copy, mkdir, hash, http, gzip — are <strong>first-class ops</strong>, not per-OS shell incantations. <code>exec</code> runs declared binaries without a shell. The same file behaves the same on macOS, Linux, and Windows.</p>
  </div>
  <div class="p">
    <h4>④ Legible by construction</h4>
    <p>Typed args, <code>--check</code>, <code>--scan</code>, <code>simulate</code>, a structured error enum, and an audit log come standard. Operational code is the code most often run by someone who didn't write it — so it has to be <em>readable and inspectable</em>, not clever.</p>
  </div>
  <div class="p">
    <h4>⑤ Ship one artifact</h4>
    <p>Hand someone a file, or <code>--build</code> a single portable binary that needs no Go, no perch, no clone. The install tax on the recipient should be one download — and embedding can carry an entire Python/Node project along.</p>
  </div>
  <div class="p">
    <h4>⑥ Stay small on purpose</h4>
    <p>perch is a <strong>control plane, not a programming language</strong>. It orchestrates tools and glues steps together; it is deliberately not where you write your application's business logic. A small, closed vocabulary is what makes the other five promises keepable.</p>
  </div>
</div>

The throughline: the operational layer should be something you **write once, read easily, hand to a teammate — or an AI agent, or CI — with confidence, and run anywhere.** Everything else perch ships is in service of that one goal.

> Want the longer argument? [sandboxed-by-design](sandboxed-by-design/) and [trust-by-manifest](trust-by-manifest/) lay out the security worldview; [os-in-a-program](os-in-a-program/) covers the cross-platform stance.

---

## 🪟 The web UI — two products in one binary

`perch --server` is a single feature with **two distinct value props** depending on who you are:

<div class="perch-features" style="margin-bottom:8px">

<div class="card">
  <h4>🤖 "I'm using AI agents and want to <em>see</em> what they're doing"</h4>
  <p>As more teams give AI agents access to their infra ("deploy my app", "restart that pod"), <strong>non-devs are increasingly downstream of automated work they can't see</strong>. <code>perch --server</code> is the <strong>"shows your work"</strong> companion to <code>perch-mcp</code>: same <code>.perch</code> file, agent uses the MCP surface, you watch every op stream live in the <strong>Run</strong> tab. Open <strong>🧪 Simulate</strong> to ask "what would happen if the agent retried?" or <strong>🔍 Scan</strong> to see what a verb actually does before granting it.</p>
</div>

<div class="card">
  <h4>🎛️ "I just want to control my system from a UI"</h4>
  <p>You operate stuff — Docker, K8s, your home server, a small fleet — and you'd rather <strong>click than type the same <code>kubectl … | jq … | grep …</code> for the 400th time</strong>. You don't want to write a frontend, stand up Retool, or maintain Backstage. <strong>Declare your verbs in <code>commands.perch</code>; run <code>perch --server</code>; that's the UI.</strong> Add a <code>command restart_pod</code>, refresh the page, the form is there. No app to deploy. No CSS to write. The file <em>is</em> the dashboard.</p>
</div>

</div>

**Same engine for both audiences.** Same `.perch` file, same interpreter, same capability gates. Add a verb → it's a CLI command, a web UI form, an MCP tool, a REPL command, and a binary entry point. <strong>Four consumers, zero duplicate schemas.</strong>

```sh
perch -f commands.perch --server --port 8080
# → open http://127.0.0.1:8080
```

### Here's what they see

<style>
.ui-mockup{font:13px/1.5 -apple-system,ui-sans-serif,system-ui,sans-serif;background:#fafafa;border:1px solid #d4d4d8;border-radius:10px;overflow:hidden;margin:18px 0;box-shadow:0 4px 14px rgba(0,0,0,.06);max-width:920px}
.ui-mockup .chrome{display:flex;align-items:center;gap:8px;padding:8px 12px;background:#ececef;border-bottom:1px solid #d4d4d8}
.ui-mockup .dot{width:11px;height:11px;border-radius:50%;display:inline-block}
.ui-mockup .dot.r{background:#ff5f57}.ui-mockup .dot.y{background:#febc2e}.ui-mockup .dot.g{background:#28c840}
.ui-mockup .url{flex:1;background:#fff;border:1px solid #d4d4d8;border-radius:5px;padding:3px 10px;font:11px/1.4 ui-monospace,monospace;color:#555;margin-left:6px}
.ui-mockup .body{padding:16px 18px}
.ui-mockup .hdr{display:flex;align-items:center;gap:10px;border-bottom:1px solid #e4e4e7;padding-bottom:10px;margin-bottom:14px}
.ui-mockup .hdr h3{margin:0;font-size:17px;color:#1c1f26}
.ui-mockup .hdr .meta{color:#666;font-size:11px}
.ui-mockup .hdr .meta code{background:#fafafa;padding:1px 5px;border-radius:3px;font-size:10px}
.ui-mockup .hdr .toggle{margin-left:auto;background:#fff;border:1px solid #d4d4d8;border-radius:5px;padding:3px 8px;font-size:12px}
.ui-mockup .tabs{display:flex;gap:2px;border-bottom:1px solid #e4e4e7;margin-bottom:14px}
.ui-mockup .tab{padding:6px 12px;color:#666;font-size:12px;border:1px solid transparent;border-bottom:none;border-radius:5px 5px 0 0}
.ui-mockup .tab.on{color:#1c1f26;background:#fff;border-color:#e4e4e7;margin-bottom:-1px;padding-bottom:7px}
.ui-mockup .search{display:block;width:100%;padding:6px 10px;border:1px solid #d4d4d8;background:#fff;border-radius:6px;font:12px/1.4 -apple-system,system-ui,sans-serif;color:#666;margin-bottom:10px}
.ui-mockup .cmd{border:1px solid #e4e4e7;background:#fff;border-radius:8px;padding:10px 12px;margin-bottom:8px}
.ui-mockup .cmd h4{margin:0 0 2px;font:13px ui-monospace,monospace;color:#1c1f26;display:flex;align-items:center;gap:6px}
.ui-mockup .cmd .badge{font-size:9px;padding:1px 5px;border-radius:8px;background:#fafafa;border:1px solid #e4e4e7;color:#666;font-family:-apple-system,system-ui,sans-serif}
.ui-mockup .cmd .badge.t{color:#d97706;border-color:#fcd34d}
.ui-mockup .cmd .desc{color:#666;font-size:11px;margin-bottom:6px}
.ui-mockup .cmd .args{display:grid;grid-template-columns:max-content 1fr max-content;gap:4px 10px;font-size:11px;align-items:center;margin:6px 0}
.ui-mockup .cmd .args label{font-family:ui-monospace,monospace;color:#1c1f26}
.ui-mockup .cmd .args input{padding:3px 6px;border:1px solid #d4d4d8;border-radius:4px;font:11px/1.3 ui-monospace,monospace;background:#fff;color:#333}
.ui-mockup .cmd .args .t{color:#888;font-size:10px}
.ui-mockup .cmd .actions{display:flex;gap:6px;margin-top:6px}
.ui-mockup .cmd .btn{padding:3px 12px;font-size:11px;border-radius:5px;border:1px solid;cursor:default}
.ui-mockup .cmd .btn.run{background:#4f46e5;border-color:#4f46e5;color:#fff}
.ui-mockup .cmd .btn.ghost{background:transparent;border-color:#d4d4d8;color:#666}
.ui-mockup .out{background:#0d1117;color:#c9d1d9;padding:8px 10px;border-radius:6px;font:11px/1.45 ui-monospace,monospace;margin-top:8px}
.ui-mockup .out .ok{color:#3fb950}.ui-mockup .out .er{color:#f85149}.ui-mockup .out .di{color:#8b949e}.ui-mockup .out .hi{color:#7c83f8}
.ui-mockup .caption{text-align:center;color:#666;font-size:12px;margin-top:-10px;margin-bottom:22px;font-style:italic}
@media (prefers-color-scheme:dark){
.ui-mockup{background:#161b22;border-color:#21262d;box-shadow:0 4px 14px rgba(0,0,0,.4)}
.ui-mockup .chrome{background:#0d1117;border-color:#21262d}
.ui-mockup .url{background:#0d1117;border-color:#21262d;color:#8b949e}
.ui-mockup .hdr{border-color:#21262d} .ui-mockup .hdr h3{color:#c9d1d9} .ui-mockup .hdr .meta{color:#8b949e}
.ui-mockup .hdr .meta code,.ui-mockup .hdr .toggle{background:#0d1117;border-color:#21262d;color:#c9d1d9}
.ui-mockup .tabs{border-color:#21262d} .ui-mockup .tab{color:#8b949e}
.ui-mockup .tab.on{color:#c9d1d9;background:#161b22;border-color:#21262d}
.ui-mockup .search{background:#0d1117;border-color:#21262d;color:#8b949e}
.ui-mockup .cmd{background:#0d1117;border-color:#21262d}
.ui-mockup .cmd h4{color:#c9d1d9} .ui-mockup .cmd .desc{color:#8b949e}
.ui-mockup .cmd .badge{background:#161b22;border-color:#21262d;color:#8b949e}
.ui-mockup .cmd .args label{color:#c9d1d9}
.ui-mockup .cmd .args input{background:#161b22;border-color:#21262d;color:#c9d1d9}
.ui-mockup .cmd .btn.ghost{border-color:#21262d;color:#8b949e}
.ui-mockup .caption{color:#8b949e}
}
</style>

<div class="ui-mockup">
  <div class="chrome">
    <span class="dot r"></span><span class="dot y"></span><span class="dot g"></span>
    <span class="url">http://127.0.0.1:8080</span>
  </div>
  <div class="body">
    <div class="hdr">
      <h3>🪶 deploy</h3>
      <span class="meta">Production deploy + rollback workflows · v1.2.0 · <code>deploy.perch</code> · 6 commands</span>
      <span class="toggle">🌓</span>
    </div>
    <div class="tabs">
      <span class="tab on">▶ Run</span>
      <span class="tab">🧪 Simulate</span>
      <span class="tab">🔍 Scan</span>
      <span class="tab">✓ Check</span>
      <span class="tab">ℹ About</span>
    </div>
    <input class="search" value="🔎 Filter commands by name or description…" readonly>
    <div class="cmd">
      <h4>deploy_canary <span class="badge">proxy_args</span></h4>
      <div class="desc">Deploy the current build to one canary region; pin a baking window before promoting.</div>
      <div class="args">
        <label>-region</label><input value="us-east-1" readonly><span class="t">string</span>
        <label>-bake_minutes</label><input type="number" value="15" readonly><span class="t">int</span>
        <label>-dry_run</label><input type="checkbox" readonly><span class="t">bool</span>
      </div>
      <div class="actions">
        <span class="btn run">Run</span>
        <span class="btn ghost">Copy as CLI</span>
      </div>
      <div class="out">
<span class="di">[started]</span>
▸ <span class="hi">if</span> has_bin "kubectl"  → true
  ✓ <span class="ok">exec kubectl apply -f canary.yaml -n prod</span>
  ✓ <span class="ok">exec kubectl rollout status deploy/api-canary</span>
▸ <span class="hi">retry max=3</span>
  ✓ <span class="ok">http_get "https://api.example.com/health"</span> → 200
▸ wait 15m
<span class="di">[ok] deploy_canary completed in 16m 02s</span>
      </div>
    </div>
    <div class="cmd">
      <h4>rollback_release <span class="badge t">test-allowed</span></h4>
      <div class="desc">Revert to the previous tagged release across all regions.</div>
      <div class="args">
        <label>-to_tag</label><input value="" readonly><span class="t">string</span>
      </div>
      <div class="actions">
        <span class="btn run">Run</span>
        <span class="btn ghost">Copy as CLI</span>
      </div>
    </div>
  </div>
</div>
<p class="caption">▶ Run tab — type-aware form per command, live NDJSON output, Copy-as-CLI hand-off</p>

<div class="ui-mockup">
  <div class="chrome">
    <span class="dot r"></span><span class="dot y"></span><span class="dot g"></span>
    <span class="url">http://127.0.0.1:8080#simulate</span>
  </div>
  <div class="body">
    <div class="hdr">
      <h3>🪶 deploy</h3>
      <span class="meta">Production deploy + rollback workflows · v1.2.0</span>
      <span class="toggle">🌓</span>
    </div>
    <div class="tabs">
      <span class="tab">▶ Run</span>
      <span class="tab on">🧪 Simulate</span>
      <span class="tab">🔍 Scan</span>
      <span class="tab">✓ Check</span>
      <span class="tab">ℹ About</span>
    </div>
    <div class="cmd">
      <div class="args">
        <label>command</label><input value="deploy_canary" readonly><span class="t"></span>
        <label>--sim-os</label><input value="linux" readonly><span class="t">select</span>
        <label>--sim-have-bin</label><input value="kubectl,docker,curl" readonly><span class="t">CSV</span>
        <label>--sim-allow-host</label><input value="api.example.com" readonly><span class="t">CSV</span>
        <label>fixture JSON</label><input value="{ … oracles + scenarios … }" readonly><span class="t">textarea</span>
      </div>
      <div class="actions"><span class="btn run">Simulate</span><span class="badge t" style="margin-left:8px">2 scenarios · 1 will-fail</span></div>
      <div class="out">
<span class="hi">═══ Scenario: happy-path ═══</span>
✓ <span class="ok">if has_bin "kubectl"</span>  oracle: true → body runs
  ✓ <span class="ok">exec kubectl apply -f canary.yaml -n prod</span>
  ✓ <span class="ok">http_get "https://api.example.com/health"</span>  oracle: 200 OK
<span class="di">summary: 4 will-run · 0 will-fail · 0 uncertain</span>

<span class="hi">═══ Scenario: kubectl-missing ═══</span>
✗ <span class="er">if has_bin "kubectl"</span>  oracle: false → body SKIPPED
✗ <span class="er">fail "deploy requires kubectl"</span>
<span class="di">1 op(s) would fail across simulated scenarios — CI blocks the PR</span>
      </div>
    </div>
  </div>
</div>
<p class="caption">🧪 Simulate tab — "what would happen on the prod host if I ran this?" answered without a terminal</p>

**Five tabs, one file, zero config:**

<div class="perch-features">

<div class="card">
  <h4>▶ Run</h4>
  <p>Searchable list of every command. <strong>Type-aware form inputs</strong> — checkbox for bools, number spinner for ints, multi-line textarea for <code>rest</code> args. Click Run → output streams live. <strong>Copy as CLI</strong> button hands the form back as a shell command. Globals panel + mod badges (<code>test</code> · <code>detached</code> · <code>proxy_args</code>).</p>
</div>

<div class="card">
  <h4>🧪 Simulate</h4>
  <p>Every <code>--sim-*</code> flag becomes a form field. Paste a <strong>v2 fixture JSON</strong> (capabilities + oracles + scenarios) and Simulate → per-op outcomes for each scenario side by side. *"What would this do on the prod host?"* answered without a terminal.</p>
</div>

<div class="card">
  <h4>🔍 Scan</h4>
  <p>One click → the full capability + risk audit. Leads with a <strong>one-glance risk score</strong> (🟢 SAFE / 🟡 LOW / 🟠 MED / 🔴 HIGH) and concrete reasons (uses sudo, executes shell, catch forwards proxy_args, …). Severity pills, the recommended hardened invocation, every host / write root / env var the program touches. <strong>Run this before executing anything you didn't write yourself.</strong></p>
</div>

<div class="card">
  <h4>✓ Check</h4>
  <p>One click → syntactic validation (same engine as <code>perch --check</code> in pre-commit). Issue list with severity counts.</p>
</div>

<div class="card">
  <h4>ℹ About + Theme</h4>
  <p>Program metadata + doc links. <strong>Dark mode</strong> auto-respects <code>prefers-color-scheme</code> and persists per browser. Hash-routed tabs (<code>#run</code>, <code>#simulate</code>, …) so any tab is bookmarkable.</p>
</div>

<div class="card">
  <h4>🔌 JSON API</h4>
  <p>Every panel is a JSON endpoint you can drive from another internal tool: <code>GET /api/program</code>, <code>POST /api/check</code> / <code>/api/scan</code> / <code>/api/simulate</code>, NDJSON-streaming <code>/api/exec</code>. Embed perch in your dashboard, Slack bot, or Backstage plugin.</p>
</div>

</div>

**Security**: single-tenant + localhost-bound by default; pair with your reverse proxy + SSO for shared access. Capability restrictions inherit from launch — `perch --no-shell --no-network --server` produces a UI where shell ops error and HTTP is denied. Commands marked `private` are hidden + rejected.

[**Web UI guide →**](web-ui.md)

### See them in action

<div class="pterm-pair">
<div class="pterm" id="t-sim-happy"  data-title="perch simulate — scenario: happy-path"></div>
<div class="pterm" id="t-sim-broken" data-title="perch simulate — scenario: github-down + kubectl-missing"></div>
</div>
<script type="application/json" data-pterm="t-sim-happy">
[
  {"k":"in",  "t":"perch simulate release --sim-file fixture.json"},
  {"k":"dim", "t":"# capabilities + oracles + scenarios from one JSON file"},
  {"k":"blank","t":""},
  {"k":"hi",  "t":"═══ Scenario: happy-path ═══"},
  {"k":"out", "t":"── command release"},
  {"k":"ok",  "t":"✓ shell_output \"git rev-parse HEAD\""},
  {"k":"dim", "t":"   ↳ oracle: shell_output(\"git rev-parse HEAD\") = \"1f1db7b\" → ${rev}"},
  {"k":"ok",  "t":"✓ http_get \"https://api.github.com/health\""},
  {"k":"dim", "t":"   ↳ oracle: api.github.com/health → 200 OK"},
  {"k":"ok",  "t":"✓ if has_bin \"kubectl\"  (oracle: true → body runs)"},
  {"k":"ok",  "t":"  ✓ shell \"kubectl apply -f manifest.yaml\""},
  {"k":"ok",  "t":"✓ write_file \"/srv/data/release-1f1db7b.json\""},
  {"k":"dim", "t":"   ↳ state: /srv/data/release-1f1db7b.json now exists"},
  {"k":"ok",  "t":"✓ if exists \"/srv/data/release-1f1db7b.json\"  (state: true)"},
  {"k":"ok",  "t":"  ✓ shell \"curl -X POST notify.example.com -d ...\""},
  {"k":"blank","t":""},
  {"k":"ok",  "t":"summary: 7 will-run · 0 will-fail · 0 uncertain"},
  {"k":"dim", "t":"# exit 0 — ship it"}
]
</script>
<script type="application/json" data-pterm="t-sim-broken">
[
  {"k":"in",  "t":"perch simulate release --sim-file fixture.json"},
  {"k":"dim", "t":"# same file, two more scenarios — what if things go wrong?"},
  {"k":"blank","t":""},
  {"k":"hi",  "t":"═══ Scenario: github-down ═══"},
  {"k":"ok",  "t":"✓ shell_output \"git rev-parse HEAD\""},
  {"k":"err", "t":"✗ http_get \"https://api.github.com/health\""},
  {"k":"dim", "t":"   ↳ oracle: api.github.com/health → 500 internal error"},
  {"k":"blank","t":""},
  {"k":"hi",  "t":"═══ Scenario: kubectl-missing ═══"},
  {"k":"ok",  "t":"✓ if has_bin \"kubectl\"  (oracle: false → body SKIPPED)"},
  {"k":"err", "t":"✗ assert_eq deploy_target \"k8s\""},
  {"k":"dim", "t":"   ↳ no kubectl path, no deploy"},
  {"k":"blank","t":""},
  {"k":"hi",  "t":"═══ Scenario: github-redirects-to-evil ═══"},
  {"k":"err", "t":"✗ http_get \"https://api.github.com/health\""},
  {"k":"dim", "t":"   ↳ oracle: 302 → https://evil.com/payload"},
  {"k":"dim", "t":"   ↳ redirect destination NOT in --allow-host allowlist"},
  {"k":"blank","t":""},
  {"k":"err", "t":"3 op(s) would fail across simulated scenarios"},
  {"k":"dim", "t":"# exit 1 — CI rejects the PR"}
]
</script>

<div class="pterm-pair">
<div class="pterm" id="t-wasm-vis"   data-title="wasm_run — the runtime literally doesn't provide escape routes"></div>
<div class="pterm" id="t-mcp-stream" data-title="perch-mcp — stdout streams live via notifications/progress"></div>
</div>
<script type="application/json" data-pterm="t-wasm-vis">
[
  {"k":"in",  "t":"perch -f plugins.perch run_plugin --name=malicious"},
  {"k":"dim", "t":"# plugin tries 5 escape routes — runtime says 'those operations don't exist'"},
  {"k":"blank","t":""},
  {"k":"out", "t":"── plugin: malicious.wasm  (under wasm_run, WASI sandbox)"},
  {"k":"err", "t":"✗ attempt 1: read /etc/passwd        → ENOENT (no host fs access)"},
  {"k":"err", "t":"✗ attempt 2: read env $AWS_KEY        → \"\" (env not in allowlist)"},
  {"k":"err", "t":"✗ attempt 3: open TCP socket          → not implemented in WASI P1"},
  {"k":"err", "t":"✗ attempt 4: write /home/user/.ssh    → ENOENT (not mounted)"},
  {"k":"err", "t":"✗ attempt 5: exec curl                → no syscall surface"},
  {"k":"blank","t":""},
  {"k":"ok",  "t":"✓ plugin produced JSON on stdout (the ONLY thing it can do)"},
  {"k":"dim", "t":"# Not blocked by policy. Invisible by construction."},
  {"k":"hi",  "t":"  This is how you let an AI write plugins for your system."}
]
</script>
<script type="application/json" data-pterm="t-mcp-stream">
[
  {"k":"in",  "t":"# agent → MCP: tools/call perch_run name=deploy _meta.progressToken=42"},
  {"k":"blank","t":""},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"▸ build artifact\" }"},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"  compiled in 4.2s\" }"},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"▸ push to registry\" }"},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"  layer 1/4 ↑ 12 MB\" }"},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"  layer 2/4 ↑ 28 MB\" }"},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"  layer 3/4 ↑ 8 MB\" }"},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"  layer 4/4 ↑ 2 MB\" }"},
  {"k":"out", "t":"← notifications/progress  { token: 42, msg: \"▸ apply k8s manifest\" }"},
  {"k":"ok",  "t":"← tools/call result      { content: ✓ deployed in 47s }"},
  {"k":"dim", "t":"# the agent narrates the deploy in real time. no silent waits."}
]
</script>

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
curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/recipes/redis.perch -o redis.perch
curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/recipes/_lib.perch -o _lib.perch
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
  <h4>🛡️ Safe by composition</h4>
  <p>Restriction flags compose — <code>--no-shell --no-network --env HOME,PATH</code>. Default-on SSRF and redirect guards. <code>--scan</code> audits a file before you run it. The grammar is the security boundary.</p>
</div>

<div class="card">
  <h4>🤖 Agent-native, no backend</h4>
  <p>Replace your LLM-tool backend with a <code>.perch</code> file. <code>perch-mcp --no-shell --no-network -f ops.perch</code> is the whole stack. Typed verbs, declared schemas, capability gates, audit log. <strong>Live streaming over MCP progress notifications</strong> — long-running verbs emit per-line stdout/stderr events as they run instead of returning a silent blob. <a href="mcp/#streaming-progress">Streaming →</a></p>
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
go install github.com/olivierdevelops/perch@latest

# 2) scaffold
perch --init

# 3) explore
perch --help          # list commands
perch <cmd> --help    # per-command help (args, defaults, examples)
perch --check         # static validation
perch test            # run every command marked `test` (sandboxed)
perch simulate cmd --sim-os=linux --sim-have-bin=kubectl  # what would happen on THAT host?
perch simulate cmd --sim-file fixture.json   # multi-scenario: happy / github-down / kubectl-missing
perch --report cmd    # execute + render the span tree
```

---

## Cross-platform without thinking about it

**~30 variables auto-bound at every command start.** No declaration, no `let`, no `if uname`. Hover any row below — that's what perch sees on the running machine.

<div id="perch-bindings"></div>

## What's in the box

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

<div class="perch-usecase" markdown="1">

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

```perch
command test
    description "Run unit + integration tests"
    do
        go test -race ./...
        if exists "./integration"
            go test -tags=integration ./integration/...
        end
    end
end
```

→ Walkthrough: **[tutorials/01-replace-your-makefile.md](tutorials/01-replace-your-makefile.md)**

</div>

### 2. Ship a Python / Node / monorepo project as one self-installing binary

<div class="perch-usecase" markdown="1">

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

→ Worked example: **[demos/05-python-installer](https://github.com/olivierdevelops/perch/tree/main/demos/05-python-installer)**

</div>

### 3. Give AI agents a safe operations surface — without standing up a backend

> **Deep dive: [LLM control plane](llm-control-plane.md) — why a `.perch` file + `perch-mcp` + a few CLI flags replaces 2,000 lines of FastAPI scaffolding.**


<div class="perch-usecase" markdown="1">

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

```perch
requires
    bin "ssh"          # the file declares the one tool it shells out to
end

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
        ssh "${host}" systemctl restart "${service}"
    end
end
```

The agent never sees `ssh`. It sees `restart_service(host, service)` with typed args. The schema is the security boundary.

→ Details: **[mcp.md](mcp.md)**

</div>

### 4. Wrap a clunky CLI behind sane verbs

<div class="perch-usecase" markdown="1">

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

```perch
requires
    bin "docker"       # everything this file runs is declared
end

command up
    description "Start the dev stack"
    do
        docker compose up -d                 # shell-free: structured argv, no metachar surface
        docker compose exec api migrate up
        print "✓ Stack running at http://localhost:8080"
    end
end

catch passthrough
    description "Forward unknown commands to docker"
    proxy_args                       # explicit opt-in to bind ${proxy_args}
    do
        shell "docker ${proxy_args}"             # proxy_args must word-split → shell (the escape case)
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
  <p>Apache-2.0. <strong>One Go binary, no SaaS, no telemetry, no phone-home.</strong> Self-host or `go install`. Bundle into your own distribution. No license fees, no per-seat costs, no cloud account required. Source: <a href="https://github.com/olivierdevelops/perch">github.com/olivierdevelops/perch</a>.</p>
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
  <h4>📜 Declared requirements + supply-chain pinning</h4>
  <p>A <code>requires</code> block makes the file <strong>declare every external resource it touches</strong> — bins (with <strong>SHA-256 hash pins</strong>), env vars, hosts, filesystem <code>read</code>/<code>write</code> scopes, OS, arch. When present, <strong>every external op verifies the manifest immediately before executing, on every call</strong> (stateless — no allow-cache): undeclared shell bins, hosts, env reads, or filesystem paths all <strong>error</strong>. A regression test fails if any external op stops refusing undeclared access. <code>perch --check</code> additionally flags literal undeclared use at lint time — proving a file is feasible on a target host <em>without running it</em>. Hash pins (inline or <code>hash_file "bundle:..."</code> embedded in the fat binary) defend against PATH-shadow + trojaned mirrors, read-only. <a href="capability-gating/">capability-gating.md →</a> · <a href="requires/">requires.md →</a>. <strong>Where this is heading:</strong> <a href="sandboxed-by-design/">zero ambient authority</a> — programs start with NO external access at all; today the manifest is opt-in (a file without it keeps ambient access).</p>
</div>

<div class="card">
  <h4>📊 Status &amp; maturity</h4>
  <p><strong>Pre-1.0 (v0.x).</strong> DSL surface is stable; op catalog continues to grow. SemVer applies once v1.0 is tagged. CI runs the full test suite on every commit. The repo eats its own dog food — <a href="https://github.com/olivierdevelops/perch/blob/main/commands.perch"><code>commands.perch</code></a> is what builds / tests / cleans perch itself.</p>
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
curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.sh | sh
```

Run a `.perch` file straight from a URL (no save-to-disk step), with restrictions you choose:

```sh
curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/sample.perch \
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
  {"k":"in",  "t":"curl -fsSL https://raw.githubusercontent.com/olivierdevelops/perch/main/scripts/install.sh | sh"},
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
  {"k":"out", "t":"3 command(s), 1 catch, 2 binding(s)"},
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

<div class="pterm-pair">
<div class="pterm" id="t-bundle" data-title="bundle ... as NAME — declare once, reference by name"></div>
<div class="pterm" id="t-wasm" data-title="wasm_run — capability gating by construction"></div>
</div>
<script type="application/json" data-pterm="t-bundle">
[
  {"k":"dim", "t":"# myapp.perch"},
  {"k":"hi",  "t":"bundle"},
  {"k":"dim", "t":"    include \"./policy.wasm\"   as policy_wasm"},
  {"k":"dim", "t":"    include \"./schema.wasm\"   as schema"},
  {"k":"dim", "t":"end"},
  {"k":"blank","t":""},
  {"k":"dim", "t":"command run_plugin do"},
  {"k":"hi",  "t":"    wasm_run policy_wasm        # ← bare ident, no quotes"},
  {"k":"dim", "t":"        wasm_arg \"/ro/deploy\""},
  {"k":"dim", "t":"    end"},
  {"k":"dim", "t":"end"},
  {"k":"blank","t":""},
  {"k":"in",  "t":"perch --build -f myapp.perch -o myapp"},
  {"k":"ok",  "t":"✓ embedded 1.2 MB from 2 sources"},
  {"k":"in",  "t":"./myapp run_plugin"},
  {"k":"out", "t":"── policy.wasm (from in-memory bundle bytes) ──"},
  {"k":"ok",  "t":"✓ ran under WASI with zero disk reads at runtime"},
  {"k":"accent","t":"→ one file in, one binary out, zero CLI flags"}
]
</script>
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

**How is this different from capy?** [capy](https://olivierdevelops.github.io/capy) is a small transpiler engine. perch *uses* capy to define its grammar (`lib.perch`), so the perch DSL is what capy ingests. capy isn't a language; perch is.

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
| [requires.md](requires.md) | **`requires` — file-declared manifest (bins, env, hosts, FS read/write scopes, OS, hash pins)** |
| [capability-gating.md](capability-gating.md) | **Every external op verifies `requires` before executing — full per-op coverage table** |
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
| [requires.md](requires.md) | **File-declared manifest — bins / env / hosts / FS read+write scopes / OS / arch + SHA-256 hash pins; supply-chain provenance built in** |
| [capability-gating.md](capability-gating.md) | **The enforcement guarantee — every external op checks the manifest before it runs, every time; per-op coverage table + regression test** |
| [llm-control-plane.md](llm-control-plane.md) | **Replace your LLM-tool backend with a `.perch` file** |
| [mcp.md](mcp.md) | MCP server reference (JSON-RPC over stdio) |
| [applications.md](applications.md) | 22 patterns; many are SRE / platform-team shaped |

### 🪞 Background reading (design intent)

| | |
|---|---|
| [os-in-a-program.md](os-in-a-program.md) | The "operating system you can `scp`" framing |
| [user-experience.md](user-experience.md) | UX roadmap |
| [ai-assisted-authoring.md](ai-assisted-authoring.md) | Notes on agent-authored `.perch` files |

Source on GitHub: [**olivierdevelops/perch**](https://github.com/olivierdevelops/perch). Apache-2.0. One Go binary, no SaaS, no telemetry.

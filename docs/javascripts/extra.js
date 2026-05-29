// Animated demo on docs/index.md.
// Cycles through Docker-management scenes with a typewriter effect.
//
// Self-contained, no deps. Initializes against #perch-demo on DOMContentLoaded.

(function () {
    const SCENES = [
        {
            caption: "① Write redis.perch — manage a Docker container as commands",
            html: `<span class="kw">name</span>    <span class="str">"redis"</span>
<span class="kw">about</span>   <span class="str">"Manage a local Redis Docker container"</span>

<span class="kw">requires</span>
    <span class="kw">bin</span> <span class="str">"docker"</span>
<span class="kw">end</span>

<span class="kw">command</span> run
    <span class="kw">arg</span> port
        <span class="kw">type</span> int
        <span class="kw">default</span> <span class="num">6379</span>
    <span class="kw">end</span>
    <span class="kw">do</span>
        <span class="kw">exec</span> docker run -d --name redis -p <span class="str">\${port}</span>:6379 redis:7-alpine
        <span class="kw">print</span> <span class="str">"Redis running on port \${port}"</span>
    <span class="kw">end</span>
<span class="kw">end</span>

<span class="kw">command</span> stop
    <span class="kw">do</span>
        <span class="kw">exec</span> docker stop redis
    <span class="kw">end</span>
<span class="kw">end</span>`,
            type: "type",
            speed: 7,
        },
        {
            caption: "② Run from the CLI — args + typed defaults + --help",
            html: `<span class="prompt">$</span> perch run
<span class="ok">✓</span> Redis running on port <span class="num">6379</span>

<span class="prompt">$</span> perch run -port=6380
<span class="ok">✓</span> Redis running on port <span class="num">6380</span>

<span class="prompt">$</span> perch run --help
<span class="heading">run</span> — Manage a local Redis Docker container

<span class="dim">USAGE</span>
  perch run [-port=…]

<span class="dim">ARGUMENTS</span>
  -port int   <span class="dim">(default 6379)</span>`,
            type: "type",
            speed: 18,
        },
        {
            caption: "③ Bundle into a portable binary — no Go, no perch needed on the target",
            html: `<span class="prompt">$</span> perch --build -o redis
<span class="ok">✓</span> Built binary: ./redis

<span class="prompt">$</span> file ./redis
./redis: Mach-O 64-bit executable arm64

<span class="prompt">$</span> ./redis run -port=7000
<span class="ok">✓</span> Redis running on port <span class="num">7000</span>

<span class="prompt">$</span> scp redis ops@server:~
redis    <span class="info">100%</span>   12MB`,
            type: "type",
            speed: 22,
        },
        {
            caption: "④ Same file, web UI — designers + ops click buttons",
            html: ``,
            type: "browser",
            commands: [
                { name: "run",  desc: "Start Redis in the background", arg: "port=6380" },
                { name: "stop", desc: "Stop the running container",    arg: null       },
                { name: "logs", desc: "Stream container logs",          arg: null       },
            ],
            hold: 5200,
        },
        {
            caption: "⑤ Same file, AI agent — perch-mcp exposes commands as tools",
            html: `<span class="dim">// claude_desktop_config.json</span>
{
  <span class="str">"mcpServers"</span>: {
    <span class="str">"redis"</span>: {
      <span class="str">"command"</span>: <span class="str">"perch-mcp"</span>,
      <span class="str">"args"</span>: [<span class="str">"-f"</span>, <span class="str">"/abs/redis.perch"</span>]
    }
  }
}

<span class="dim">// agent's view of the tools</span>
• redis_list  — list every callable command
• redis_run   — invoke run / stop / logs with typed args

<span class="info">→ The grammar IS the security boundary.</span>
<span class="info">  Only what you declared is reachable.</span>`,
            type: "type",
            speed: 18,
        },
    ];

    function escapeText(s) {
        return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }

    // Type out an HTML string char-by-char, preserving spans.
    // Strategy: parse the HTML into "tokens" (visible text + tag wrappers),
    // then render them with delays between visible characters.
    function renderTyped(target, html, speed, onDone) {
        // Tokenize into chunks: each chunk is either a raw HTML tag or one char of text.
        const chunks = [];
        let i = 0;
        while (i < html.length) {
            if (html[i] === "<") {
                const end = html.indexOf(">", i);
                if (end < 0) break;
                chunks.push(html.substring(i, end + 1));
                i = end + 1;
            } else {
                chunks.push(html[i]);
                i++;
            }
        }
        let acc = "";
        let idx = 0;
        target.innerHTML = "";
        target.classList.add("perch-demo__cursor");
        function tick() {
            if (idx >= chunks.length) {
                target.classList.remove("perch-demo__cursor");
                if (onDone) onDone();
                return;
            }
            const c = chunks[idx++];
            acc += c;
            target.innerHTML = acc;
            // Tags get no delay; visible chars use `speed`.
            const isTag = c.length > 1 && c.startsWith("<");
            setTimeout(tick, isTag ? 0 : speed);
        }
        tick();
    }

    function renderBrowser(target, scene, onDone) {
        const url = "http://127.0.0.1:10032";
        const rows = scene.commands.map(c => `
            <div class="row">
                <span class="btn">Run</span>
                <strong>${c.name}</strong>
                <span class="label">— ${c.desc}</span>
                ${c.arg ? `<span class="dim" style="margin-left:auto">${c.arg}</span>` : ""}
            </div>`).join("");
        target.innerHTML = `
            <span class="prompt">$</span> perch --server --port 10032
            <span class="ok">→</span> perch UI on <span class="url">${url}</span>

            <div class="perch-demo__browser">
                <div class="perch-demo__browser-bar">
                    <span class="b-dot"></span><span class="b-dot"></span><span class="b-dot"></span>
                    <span style="margin-left:8px">${url}</span>
                </div>
                <div class="perch-demo__browser-body">
                    <div class="h1">redis · Manage a local Redis Docker container</div>
                    ${rows}
                </div>
            </div>`;
        if (onDone) onDone();
    }

    document.addEventListener("DOMContentLoaded", function () {
        const root = document.getElementById("perch-demo");
        if (!root) return;

        // Build the chrome + stage.
        root.innerHTML = `
            <div class="perch-demo__chrome">
                <span class="dot red"></span>
                <span class="dot yellow"></span>
                <span class="dot green"></span>
                <span class="perch-demo__title">perch — one file, every frontend</span>
            </div>
            <div class="perch-demo__stage" id="perch-demo-stage"></div>
            <div class="perch-demo__dots" id="perch-demo-dots"></div>`;

        const stage = root.querySelector("#perch-demo-stage");
        const dots  = root.querySelector("#perch-demo-dots");

        // Pre-build slides + dots.
        const slides = SCENES.map((scene, i) => {
            const slide = document.createElement("div");
            slide.className = "perch-demo__slide" + (i === 0 ? " is-active" : "");
            slide.dataset.idx = i;
            slide.innerHTML = `
                <div class="perch-demo__caption">${scene.caption}</div>
                <pre class="perch-demo__body" data-body></pre>`;
            stage.appendChild(slide);

            const dot = document.createElement("span");
            dot.className = "perch-demo__dots-dot" + (i === 0 ? " is-active" : "");
            if (i === 0) dot.classList.add("is-active");
            dot.dataset.idx = i;
            dot.onclick = () => goTo(i);
            dots.appendChild(dot);
            return slide;
        });

        let active = 0;
        let timer = null;

        function renderActive() {
            const scene = SCENES[active];
            const body  = slides[active].querySelector("[data-body]");
            const hold  = scene.hold || 4500;

            const advance = () => {
                clearTimeout(timer);
                timer = setTimeout(() => goTo((active + 1) % SCENES.length), hold);
            };

            if (scene.type === "browser") {
                renderBrowser(body, scene, advance);
            } else {
                renderTyped(body, scene.html, scene.speed || 14, advance);
            }
        }

        function goTo(i) {
            slides[active].classList.remove("is-active");
            dots.children[active].classList.remove("is-active");
            active = i;
            slides[active].classList.add("is-active");
            dots.children[active].classList.add("is-active");
            renderActive();
        }

        renderActive();
    });
})();

// ─── Inline mini-terminals ──────────────────────────────────────────
//
// Usage in markdown:
//
//   <div class="pterm" id="t-foo"></div>
//   <script type="application/json" data-pterm="t-foo">
//   [
//     {"k":"in",  "t":"perch test"},
//     {"k":"out", "t":"==> Running unit tests"},
//     {"k":"ok",  "t":"✓ 142 passed"}
//   ]
//   </script>
//
// k = "in" | "out" | "ok" | "err" | "dim" | "hi" | "blank" | "wait"
//
// Plays once when scrolled into view. Click ↻ to replay. Multiple
// terminals on one page are independent. Cheap: pure DOM, no deps.
(function () {
    const COLOR = {
        in:    "fg",
        out:   "fg",
        ok:    "ok",
        err:   "err",
        dim:   "dim",
        hi:    "accent",
    };

    function buildShell(target, lines) {
        target.classList.add("pterm-shell");
        target.innerHTML = `
            <div class="pterm-chrome">
                <span class="pterm-dot pterm-red"></span>
                <span class="pterm-dot pterm-yellow"></span>
                <span class="pterm-dot pterm-green"></span>
                <span class="pterm-title">${target.dataset.title || "terminal"}</span>
                <button class="pterm-replay" type="button" title="Replay">↻</button>
            </div>
            <pre class="pterm-body" data-pterm-body></pre>`;
        const body = target.querySelector("[data-pterm-body]");
        const btn  = target.querySelector(".pterm-replay");
        btn.onclick = () => play(body, lines);
        return body;
    }

    function play(body, lines) {
        body.innerHTML = "";
        let lineIdx = 0;
        function nextLine() {
            if (lineIdx >= lines.length) return;
            const line = lines[lineIdx++];
            if (line.k === "blank") {
                body.appendChild(document.createElement("br"));
                setTimeout(nextLine, 80);
                return;
            }
            if (line.k === "wait") {
                setTimeout(nextLine, line.t || 400);
                return;
            }
            const row = document.createElement("div");
            row.className = "pterm-row pterm-" + (COLOR[line.k] || "fg");
            if (line.k === "in") {
                const prompt = document.createElement("span");
                prompt.className = "pterm-prompt";
                prompt.textContent = "$ ";
                row.appendChild(prompt);
            }
            const text = document.createElement("span");
            row.appendChild(text);
            body.appendChild(row);

            // Typewriter for "in" lines (the user "typing"), instant for output.
            if (line.k === "in") {
                let i = 0;
                const speed = 28;
                (function tick() {
                    if (i >= line.t.length) {
                        setTimeout(nextLine, 220);
                        return;
                    }
                    text.textContent += line.t[i++];
                    setTimeout(tick, speed);
                })();
            } else {
                text.textContent = line.t;
                setTimeout(nextLine, 110);
            }
        }
        nextLine();
    }

    function init() {
        const scripts = document.querySelectorAll('script[type="application/json"][data-pterm]');
        const seen = new WeakSet();

        scripts.forEach(s => {
            const id = s.dataset.pterm;
            const target = document.getElementById(id);
            if (!target) return;
            let lines;
            try { lines = JSON.parse(s.textContent); } catch (e) { return; }
            const body = buildShell(target, lines);

            // Play when scrolled into view (first time only).
            const obs = new IntersectionObserver((entries) => {
                entries.forEach(e => {
                    if (e.isIntersecting && !seen.has(target)) {
                        seen.add(target);
                        play(body, lines);
                        obs.disconnect();
                    }
                });
            }, { threshold: 0.35 });
            obs.observe(target);
        });
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();

// ─── Auto-bindings inspector ───────────────────────────────────────
//
// A live panel rendering ${var} = expanded-value rows so readers see
// concretely what each auto-binding resolves to on the docs server's
// host. Values are baked at build-time (the JS just renders them) so
// no requests fire. Each row reveals on a stagger.
(function () {
    const ROWS = [
        ["os",          "darwin"],
        ["arch",        "arm64"],
        ["is_macos",    "true"],
        ["home",        "/Users/you"],
        ["cache_dir",   "/Users/you/Library/Caches"],
        ["config_dir",  "/Users/you/Library/Application Support"],
        ["temp_dir",    "/var/folders/.../T/"],
        ["exe_path",    "/usr/local/bin/perch"],
        ["exe_dir",     "/usr/local/bin"],
        ["script_path", "/abs/path/to/commands.perch"],
        ["path_sep",    "/"],
        ["exe_ext",     "(empty on unix; .exe on Windows)"],
        ["null_device", "/dev/null"],
        ["cpu_count",   "10"],
        ["user",        "you"],
        ["hostname",    "your-laptop"],
    ];

    function init() {
        const root = document.getElementById("perch-bindings");
        if (!root) return;
        root.classList.add("pbind");
        root.innerHTML = `
            <div class="pbind-chrome">
                <span class="pbind-dot"></span>
                <span class="pbind-title">auto-bound variables — no declaration, always available</span>
            </div>
            <div class="pbind-body" id="pbind-body"></div>`;
        const body = root.querySelector("#pbind-body");

        ROWS.forEach((r, idx) => {
            const row = document.createElement("div");
            row.className = "pbind-row";
            row.innerHTML = `
                <span class="pbind-name">\${${r[0]}}</span>
                <span class="pbind-arrow">→</span>
                <span class="pbind-val">${r[1]}</span>`;
            body.appendChild(row);
        });

        // Stagger-reveal on scroll into view.
        const obs = new IntersectionObserver((entries) => {
            entries.forEach(e => {
                if (e.isIntersecting) {
                    body.querySelectorAll(".pbind-row").forEach((row, idx) => {
                        setTimeout(() => row.classList.add("is-shown"), idx * 70);
                    });
                    obs.disconnect();
                }
            });
        }, { threshold: 0.25 });
        obs.observe(root);
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();

// ─── "One file, five outputs" flow diagram ─────────────────────────
//
// CSS-only branching tree, but we light up each branch on a stagger
// when the diagram scrolls into view so it feels alive.
(function () {
    function init() {
        const root = document.getElementById("perch-fanout");
        if (!root) return;
        root.classList.add("pfan");
        root.innerHTML = `
            <div class="pfan-source">
                <div class="pfan-file">commands.perch</div>
            </div>
            <div class="pfan-arrows">
                <div class="pfan-arrow"></div>
                <div class="pfan-arrow"></div>
                <div class="pfan-arrow"></div>
                <div class="pfan-arrow"></div>
                <div class="pfan-arrow"></div>
            </div>
            <div class="pfan-targets">
                <div class="pfan-target" data-d="0">
                    <div class="pfan-icon">⌨</div>
                    <div class="pfan-label">CLI</div>
                    <div class="pfan-sub">perch &lt;cmd&gt;</div>
                </div>
                <div class="pfan-target" data-d="1">
                    <div class="pfan-icon">🌐</div>
                    <div class="pfan-label">Web UI</div>
                    <div class="pfan-sub">perch --server</div>
                </div>
                <div class="pfan-target" data-d="2">
                    <div class="pfan-icon">›_</div>
                    <div class="pfan-label">REPL</div>
                    <div class="pfan-sub">perch --shell</div>
                </div>
                <div class="pfan-target" data-d="3">
                    <div class="pfan-icon">🤖</div>
                    <div class="pfan-label">MCP</div>
                    <div class="pfan-sub">perch-mcp</div>
                </div>
                <div class="pfan-target" data-d="4">
                    <div class="pfan-icon">📦</div>
                    <div class="pfan-label">Binary</div>
                    <div class="pfan-sub">perch --build</div>
                </div>
            </div>`;
        const obs = new IntersectionObserver((entries) => {
            entries.forEach(e => {
                if (e.isIntersecting) {
                    root.querySelectorAll(".pfan-arrow").forEach((a, idx) => {
                        setTimeout(() => a.classList.add("is-lit"), 150 + idx * 120);
                    });
                    root.querySelectorAll(".pfan-target").forEach((t, idx) => {
                        setTimeout(() => t.classList.add("is-lit"), 250 + idx * 120);
                    });
                    obs.disconnect();
                }
            });
        }, { threshold: 0.3 });
        obs.observe(root);
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();

// ─── Capy / perch syntax highlighter ───────────────────────────────
//
// Pygments doesn't know the capy/perch language, so mkdocs ships
// `language-capy` / `language-perch` code blocks as plain text. We
// post-process them in the browser: tokenise into comment / string /
// interpolation / keyword / number / boolean / identifier / op-call /
// punct and wrap each token in a span with our own class. Colors are
// defined in extra.css and match the existing dark code-block palette
// for both light and slate themes.
//
// Single linear scanner — no library dependencies, ~3 KB minified.
(function () {
    const KW = new Set([
        // structural
        "name", "about", "version", "globals", "command", "catch",
        "description", "do", "end", "arg", "type", "default", "optional", "index",
        // modifiers
        "private", "detached", "proxy_args", "require_os", "require_arch",
        "dir", "on_signal", "env",
        // control flow
        "if", "not", "let", "run", "for_each", "fail", "exit",
        // sandbox (designed)
        "sandbox", "read", "write", "net", "shell_bins", "no_shell",
        "no_subprocess", "no_network", "no_write", "no_sudo",
        "no_shell_metachars", "private_ops", "require_sandbox",
        "pure", "offline", "read_only",
        "max_runtime", "max_download", "max_file_size", "max_processes",
    ]);
    const BOOL = new Set(["true", "false"]);

    function isIdStart(c) { return /[A-Za-z_]/.test(c); }
    function isIdCont(c) { return /[A-Za-z0-9_]/.test(c); }
    function isDigit(c) { return /[0-9]/.test(c); }

    // tokenise(src) → array of {kind, text} tokens.
    function tokenise(src) {
        const tokens = [];
        let i = 0;
        while (i < src.length) {
            const c = src[i];

            // line comment
            if (c === "#") {
                const end = src.indexOf("\n", i);
                const j = end < 0 ? src.length : end;
                tokens.push({ kind: "com", text: src.slice(i, j) });
                i = j;
                continue;
            }
            // string with interpolation
            if (c === '"' || c === "'") {
                const quote = c;
                let j = i + 1;
                while (j < src.length && src[j] !== quote) {
                    if (src[j] === "\\" && j + 1 < src.length) { j += 2; continue; }
                    j++;
                }
                const raw = src.slice(i, Math.min(j + 1, src.length));
                tokens.push({ kind: "str", text: raw });
                i = j + 1;
                continue;
            }
            // number (integer or float)
            if (isDigit(c)) {
                let j = i + 1;
                while (j < src.length && (isDigit(src[j]) || src[j] === ".")) j++;
                tokens.push({ kind: "num", text: src.slice(i, j) });
                i = j;
                continue;
            }
            // identifier / keyword / boolean
            if (isIdStart(c)) {
                let j = i + 1;
                while (j < src.length && isIdCont(src[j])) j++;
                const word = src.slice(i, j);
                let kind = "id";
                if (KW.has(word))   kind = "kw";
                else if (BOOL.has(word)) kind = "bool";
                tokens.push({ kind, text: word });
                i = j;
                continue;
            }
            // multi-char operators
            if (i + 1 < src.length) {
                const two = src.substr(i, 2);
                if (two === "==" || two === "!=" || two === ">=" || two === "<=") {
                    tokens.push({ kind: "op", text: two });
                    i += 2;
                    continue;
                }
            }
            // single-char operators / punctuation
            if ("=<>+-*/().,:;".includes(c)) {
                tokens.push({ kind: "op", text: c });
                i++;
                continue;
            }
            // whitespace / fallthrough
            tokens.push({ kind: "ws", text: c });
            i++;
        }
        return tokens;
    }

    // Re-tokenise strings to highlight ${name} interpolation runs.
    function splitStringInterp(text) {
        const out = [];
        let i = 0;
        while (i < text.length) {
            const dollar = text.indexOf("${", i);
            if (dollar < 0) {
                out.push({ kind: "str", text: text.slice(i) });
                break;
            }
            if (dollar > i) out.push({ kind: "str", text: text.slice(i, dollar) });
            const close = text.indexOf("}", dollar);
            if (close < 0) {
                out.push({ kind: "str", text: text.slice(dollar) });
                break;
            }
            out.push({ kind: "interp", text: text.slice(dollar, close + 1) });
            i = close + 1;
        }
        return out;
    }

    function escapeHTML(s) {
        return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }

    function render(tokens) {
        const parts = [];
        for (const t of tokens) {
            if (t.kind === "str") {
                // Split out ${interp} runs so they get their own color.
                for (const sub of splitStringInterp(t.text)) {
                    parts.push(`<span class="pcode-${sub.kind}">${escapeHTML(sub.text)}</span>`);
                }
                continue;
            }
            if (t.kind === "ws") {
                parts.push(escapeHTML(t.text));
                continue;
            }
            parts.push(`<span class="pcode-${t.kind}">${escapeHTML(t.text)}</span>`);
        }
        return parts.join("");
    }

    function highlightBlock(code) {
        const src = code.textContent;
        code.innerHTML = render(tokenise(src));
        code.classList.add("pcode-hl");
    }

    // Pymdownx remaps the unknown "capy" lexer to "text" and the
    // resulting <div class="language-text highlight"> drops any hint of
    // the original language. Content-sniff for distinctive perch tokens.
    function looksLikeCapy(text) {
        const lines = text.split("\n");
        for (const raw of lines) {
            const line = raw.trim();
            if (!line || line.startsWith("#")) continue;
            // Top-level / block-opener forms.
            if (/^(name|about|version|globals|command|catch|sandbox|arg)\s/.test(line)) return true;
            // Body-form openers (for standalone snippets).
            if (/^(let\s+\w+\s*=|run\s+\w|if\s+\w|do$|end$|do\b|end\b)/.test(line)) return true;
            return false;
        }
        return false;
    }

    function init() {
        // 1) Anything explicitly tagged with the language class.
        document.querySelectorAll(
            "code.language-capy, code.language-perch, pre code.language-capy, pre code.language-perch"
        ).forEach(highlightBlock);

        // 2) For pymdownx-remapped blocks (lexer unknown → "text"),
        //    scan every fenced code block, skip non-text languages,
        //    and content-sniff the rest. Idempotent: pcode-hl class
        //    guards against re-processing.
        document.querySelectorAll("pre code").forEach(code => {
            if (code.classList.contains("pcode-hl")) return;
            const wrapper = code.closest("[class*='language-']");
            if (wrapper) {
                const cls = wrapper.className;
                // Only consider blocks where Pygments fell back to text.
                if (!/\blanguage-text\b/.test(cls)) return;
            }
            if (looksLikeCapy(code.textContent)) highlightBlock(code);
        });
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();

// ─── Stats bar counter — animates 0 → N when scrolled into view ────
(function () {
    function animateCounter(el) {
        const target = parseInt(el.dataset.count || el.textContent, 10);
        if (!isFinite(target)) return;
        const dur = 700;
        const start = performance.now();
        const suffix = el.dataset.suffix || "";
        el.classList.add("is-counting");
        function step(now) {
            const t = Math.min(1, (now - start) / dur);
            // ease-out cubic
            const eased = 1 - Math.pow(1 - t, 3);
            el.textContent = Math.round(target * eased) + suffix;
            if (t < 1) requestAnimationFrame(step);
            else el.textContent = target + suffix;
        }
        requestAnimationFrame(step);
    }

    function isInView(el) {
        const r = el.getBoundingClientRect();
        return r.top < (window.innerHeight || document.documentElement.clientHeight) && r.bottom > 0;
    }

    function init() {
        const root = document.querySelector(".pstats");
        if (!root) return;
        // Guard against double-init (we listen to multiple ready signals
        // to survive both vanilla loads and mkdocs-material's instant nav).
        if (root.dataset.pstatsInited) return;
        root.dataset.pstatsInited = "1";

        const fire = () => root.querySelectorAll(".pstats-num").forEach(animateCounter);

        // If the stats bar is already on-screen at init (the common case
        // — it's near the top of the page), fire immediately. No need to
        // wait for an intersection event that may never come.
        if (isInView(root)) {
            fire();
            return;
        }
        // Otherwise, wait for any intersection. Threshold 0 fires the
        // moment any pixel of the bar enters the viewport.
        const obs = new IntersectionObserver((entries) => {
            entries.forEach(e => {
                if (e.isIntersecting) {
                    fire();
                    obs.disconnect();
                }
            });
        }, { threshold: 0 });
        obs.observe(root);
    }

    // Run on DOMContentLoaded and again after window.load — covers both
    // first-paint and any late-arriving DOM.
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
    window.addEventListener("load", init);
})();

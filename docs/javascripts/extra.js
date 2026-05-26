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

<span class="kw">command</span> run
    <span class="kw">arg</span> port
        <span class="kw">type</span> int
        <span class="kw">default</span> <span class="num">6379</span>
    <span class="kw">end</span>
    <span class="kw">do</span>
        <span class="kw">shell</span> <span class="str">"docker run -d --name redis -p \${port}:6379 redis:7-alpine"</span>
        <span class="kw">print</span> <span class="str">"Redis running on port \${port}"</span>
    <span class="kw">end</span>
<span class="kw">end</span>

<span class="kw">command</span> stop
    <span class="kw">do</span>
        <span class="kw">shell</span> <span class="str">"docker stop redis"</span>
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

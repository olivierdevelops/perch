# 🪟 Web UI — `perch --server`

> **For teammates who don't live in a terminal.** Same `.perch` file, same interpreter, same restrictions — rendered as a friendly localhost web app.

---

## TL;DR

```sh
perch -f commands.perch --server --port 8080
# → open http://127.0.0.1:8080
```

That's it. The UI auto-renders **every** declared command as a form, exposes the same pre-flight tools the CLI does (`--check`, `--scan`, `simulate`), and streams output live as the interpreter walks ops.

**Who this is for:** support engineers running runbooks; QA running canned test sequences; product / ops folks who'd rather click than `perch -f deploy.perch deploy_canary -region=us-east-1 -bake_minutes=15`; new hires on day one before they've set up a terminal.

**Who this is NOT for:** multi-tenant SaaS hosting. `--server` is single-tenant + localhost-bound by default; put it behind your existing reverse proxy + SSO for shared access.

---

## Five tabs, one file

The UI is hash-routed (`http://host/#run`, `#simulate`, `#scan`, `#check`, `#about`) — bookmark the tab you use most.

### ▶ Run

The default view. Lists every visible command from the loaded `.perch` file:

- **Live search/filter** across command names + descriptions. (Essential when you `--include recipes/` and have 22 commands in one file.)
- **Type-aware form inputs** — `bool` args get a checkbox; `int` / `float` get number spinners; `rest` args get a multi-line textarea (one value per line); strings get text inputs.
- **Defaults render as placeholders**, not pre-filled values. Submitting an empty field uses the runtime default; this matches the CLI's behavior exactly.
- **Mod badges** show which commands are `test`, `detached`, or `proxy_args`.
- **Globals panel** (collapsible) at the top — every binding from the program's `globals` block, with type and value.
- Click **Run** → output streams live in a dark output panel (separated by `out` / `err` / `status` channels). Hit **Clear** between runs.
- **Copy as CLI** button — generates a shell-escaped `perch -f file.perch CMD -arg=val …` string mirroring the form so you can paste it back to a terminal for automation.

### 🧪 Simulate

The whole `perch simulate` (v2) surface, in a form:

- One field per CLI flag — `--sim-os` / `--sim-arch` (dropdowns), `--sim-env`, `--sim-fs-read`, `--sim-fs-write`, `--sim-have-bin`, `--sim-allow-host`.
- Checkboxes for `--sim-env-only`, `--sim-no-shell`, `--sim-no-network`, `--sim-no-subprocess`, `--sim-no-write`.
- A **JSON fixture textarea** — paste a v2 fixture (capabilities + **oracles** + **scenarios**) and one Simulate click runs every scenario; each gets its own banner + per-op report (WILL_RUN ✓ / WILL_FAIL ✗ / MIGHT_FAIL ?) in the output panel.
- Status pill summarises the run: green "all clear" or red "simulated failures present."

This is the killer feature for non-devs: *"what would happen on the production host if I ran this?"* — no terminal, no fixture file on disk, no `--sim-…` flag memorisation.

### 🔍 Scan

One click → the full **`perch --scan`** output:

- Capability summary (shell? subprocess? network hosts? write roots? env vars?)
- Per-finding severity ribbon (HIGH / MED / LOW)
- The recommended hardened invocation (`--no-subprocess --allow-bin docker,kubectl …`)
- A status pill: red if any HIGH finding, yellow on MED, green on none.

Useful before running anything you didn't write yourself — including the [22 recipes](recipes.md).

### ✓ Check

One click → **syntactic validation** (the same `perch --check` runs in pre-commit). Issue list with per-severity counts. Wire this tab into your CI dashboard via the `/api/check` JSON endpoint.

### ℹ About

Program metadata (name, version, source file path, command count) + direct links into the docs site for help.

---

## Theming

- **Dark mode toggle** in the header (🌓). Auto-respects `prefers-color-scheme`; choice persists per browser via `localStorage`.
- Mobile-friendly responsive layout — works on a tablet or phone for one-handed runbook execution.

---

## JSON API

Every panel is backed by an endpoint you can drive directly — handy for embedding perch in another internal tool or a Slack bot:

| Method | Path | Body | Returns |
|---|---|---|---|
| GET  | `/api/program`  | — | Program metadata + arg specs + globals + catch |
| POST | `/api/exec`     | `{command, args, allow_bin?, allow_host?, env_only?}` | NDJSON stream of `{kind:out\|err\|status, msg}` |
| POST | `/api/check`    | — | `{ok, errors, warnings, issues[]}` |
| POST | `/api/scan`     | — | `{report, capabilities, findings, recommended}` |
| POST | `/api/simulate` | `{command, env, fixture?}` | `{ok, report}` |

All endpoints return JSON (except `/api/exec` which streams NDJSON for live output). Pair with your existing dashboard, observability tool, or `curl | jq` workflow.

---

## What's NOT in the UI (yet)

Honest list — see [the parity TODO](https://github.com/luowensheng/perch/issues) for status:

- **Per-request `--no-shell` / `--no-network` / `--no-subprocess` / `--no-write` toggles** — today these have to be set when you launch `perch --server`; per-request overrides will land in a follow-up.
- **Live span-tree view** of running ops (the `--report` shape rendered in real time). Today the Run panel streams the same NDJSON your CLI would print.
- **`perch test` panel** with per-test pass/fail tree.
- **`perch --build` panel** for non-devs to produce a portable binary.
- **Audit log download** button after a run.
- **Run history** in localStorage with replay.
- **REPL** equivalent (`perch --shell` in a textarea).
- **`--ask` interactive prompts** (needs WebSocket).
- **Authentication** — single-tenant by design; pair with a reverse proxy.

---

## Security posture

Same model as `perch --server` always had:

1. **localhost-bound by default** (`--host 127.0.0.1`).
2. **Single-tenant** — no auth layer, no user model. Put it behind your existing auth boundary (SSO via reverse proxy is the typical shape).
3. **No file upload** — the program is loaded from a path at launch time, not from the browser. No `curl POST a .perch` attack surface.
4. **Capability restrictions inherit from launch** — `perch --no-shell --no-network --env KUBECONFIG --server` produces a UI where shell ops error and HTTP is denied. The grammar is still the security boundary.
5. **Private command filter** — commands marked `private` are hidden from the UI and `/api/exec` rejects them.

---

## See also

- [Getting started](getting-started.md) — the CLI walkthrough first
- [Simulate guide](simulate.md) — what the 🧪 Simulate panel exposes
- [MCP server](mcp.md) — the same file as an AI-agent surface
- [The OS analogy](os-in-a-program.md) — why one file makes sense as five surfaces

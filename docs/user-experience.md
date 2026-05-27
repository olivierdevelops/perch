# Making perch user-friendly — a UX report

> **Goal:** broaden perch's audience from "developers fluent in CLIs" to "anyone who needs to safely run a set of declared commands." Without sacrificing what already works.

This page is the design report for that effort. It enumerates current friction honestly, proposes concrete remedies grouped by phase of use (first-run → authoring → running → errors → sharing), and ends with a prioritized roadmap. Each proposal has a one-line effort estimate so the team can pick what fits.

The framing matters: **perch already has the surfaces** (CLI, web UI, REPL, MCP, built binaries, LSP, scan, audit). What's missing is the polish that makes those surfaces work for users who aren't already shell power-users.

---

## 1. Who is a "regular user"?

Four audiences, each with different needs:

| Audience | What they know | What they need |
|---|---|---|
| **Power users** (shell fluent, not CLI authors) | bash / fish / zsh, basic scripting | a more pleasant scripting language; gentle on-ramp |
| **Junior engineers** | a programming language, maybe one shell | templates, examples, IDE help, clear errors |
| **Non-engineers** (support, ops, business) | knows the *task*, not the tooling | a button or a typed verb, plain-English previews, no shell |
| **Domain experts** (data, ML, finance) | their domain + Python/R; weak on infra | recipes, "just run my script in a sandbox," visible progress |

The fifth audience — **first-time engineer visitors** — overlaps with all of these. Their needs collapse to: "find the right doc fast, get to a working script in five minutes, don't feel stupid."

The recommendations below are weighted toward the middle two columns (junior engineers + non-engineers), because that's where the largest gap is today.

---

## 2. Where the friction is today (honest assessment)

Five concrete walls a new user hits, ranked by frequency:

### 2.1 First-run blankness
`perch --init` writes a one-command starter file. After that, the user is alone with a docs site that assumes they know what `command` / `do` / `arg` / `${name}` mean. There's no wizard, no template picker, no in-binary "tutorial" mode.

### 2.2 Discovery problem (~140 ops, ~30 auto-vars, ~10 flags)
The op catalog exists but is a flat reference page. New users don't know `if exists "X"` is a thing; they reach for `shell "[ -f X ]"` because that's what they know. Nothing surfaces relevant ops as the user types or makes mistakes.

### 2.3 Reading problem
A `.perch` file written by someone else is dense. There's no "explain this command in plain English" view. `perch --check` validates structure but doesn't summarise intent. `perch --scan` audits security but doesn't translate "shell `kubectl rollout restart deploy/api`" into "Restart the api deployment via kubectl."

### 2.4 Running problem
Lots of flags, useful but daunting. Non-engineers using `perch --server` see typed argument fields but no plain-English description, no example invocation, no preview of what the command will do.

### 2.5 Failure mode tone
Most errors are diagnostic, not solution-focused. "unknown placeholder ${X}" tells you *what* failed but not *which line typo'd `${X}` for `${x}`*. We already do "did you mean…?" for command names; that pattern needs to extend everywhere.

---

## 3. Recommendations, grouped by phase

### Phase A — First 30 seconds (onboarding)

#### A.1 `perch --init --interactive` (wizard) — small effort

Today's `--init` writes a static template. Make `--interactive` ask:

1. What kind of project? (Go service / Python project / Node app / CLI wrapper / Docker stack / generic task runner)
2. What's the program name?
3. Should it ship as a binary?

Output: a scaffold *with the right ops and arg shapes pre-filled* for the chosen project type. The user edits values, not structure.

Effort: ~200 LOC + ~6 template files. Half a day.

#### A.2 First-run greeting

When perch is launched with no `.perch` file in scope:

```
👋 Welcome to perch. A few starting points:

   perch --init                     scaffold a starter commands.perch
   perch --init --interactive       walk you through the choice
   perch tutorial                   guided in-terminal tutorial
   perch --scan ./somefile.perch    audit a script someone gave you
   open https://luowensheng.github.io/perch/getting-started/
```

Don't show this if a `commands.perch` exists in cwd (the user is past onboarding). One screen, no longer.

Effort: tiny. ~30 LOC in `usecases/listcommands` for the empty-program case.

#### A.3 `perch tutorial` — in-terminal guided walkthrough

Five-minute interactive tour: type-along examples that build up a working file. Each step explains what happened and waits for the user to press Enter. Doesn't require internet access.

Effort: medium. ~600 LOC + content authoring. Two days for v1 with five steps.

---

### Phase B — Authoring (writing a `.perch`)

#### B.1 Template library

```
perch new --template python-installer
perch new --template docker-wrapper
perch new --template cli-shipping
perch new --template agent-surface       # for MCP / LLM control plane
perch new --template makefile-replacement
perch new --list                          # discover templates
```

Each template is a `.perch` file with comments explaining what to fill in. Lives in `infra/templates/` embedded into the binary.

Effort: small once 5 templates are written. Each template is ~50-80 lines of well-commented `.perch`.

#### B.2 AI assistance (`perch ai`) — design-only for now

`perch ai "I want a command that backs up my postgres database to S3 weekly"` shells out to Claude / OpenAI (whichever the user configured), seeded with `skills/perch/SKILL.md` as system context. Streams back a generated `.perch` snippet, which then drops into the user's file.

This is a separate command and is **opt-in only** — requires the user's API key, never bundled with the binary. The same SKILL.md that helps Claude Code write perch is reused.

Effort: small if API-only. The interesting part is the prompt template, not the integration.

#### B.3 Snippet expansion (extend the LSP)

`perch-lsp` already provides completion. Extend it with snippet templates:
- typing `command<TAB>` expands to a full command stub
- typing `arg<TAB>` expands to a full arg block
- typing `let<TAB>` offers a list of common capture ops (sha256_file, http_get, glob, …) with sample args

Effort: medium. `cmd/perch-lsp/snippets.go` + a few hundred lines.

#### B.4 Inline doc on hover (already exists)

`perch-lsp` already has hover; just verify it shows op descriptions + arg types + an example for every op in the catalog. Worth auditing the coverage.

Effort: tiny — audit pass on the existing hover handler.

---

### Phase C — Running (day-to-day use)

#### C.1 `perch <cmd> --explain`

Before running, print a plain-English summary of what the command will do:

```
$ perch deploy --explain

  Command: deploy
  Description: Roll out a release

  Will:
    1. Print "starting deploy"
    2. Run shell: docker compose up -d
    3. Run shell: kubectl apply -f manifest.yaml
    4. Check https://api.github.com/repos/x/y for a 2xx response
    5. Write to ${APP_DIR}/last-deploy.txt

  Needs:
    • shell access (binaries: docker, kubectl)
    • network access (api.github.com)
    • filesystem write to ${APP_DIR}

  Run with `--ask` to step through, or `--scan` for the security audit.
```

Builds on the existing scan analyzer. ~150 LOC, half a day.

#### C.2 Web UI improvements

The current `--server` shows command names + typed arg fields. Add:

- **Description prominently** for each command (already in the file)
- **Example invocation** rendered above each command's form
- **"Preview"** button next to "Run" — invokes the same code as `--dry-run`, shows result in a side panel
- **Recent runs** — last 10 invocations with stdout/stderr panes
- **Per-command audit log link** if `--audit FILE` is enabled

The UI is in `infra/httpserver/server.html`. ~500 LOC of HTML/JS additions.

#### C.3 Output formatting

Progress indicators for long-running ops + colored success/failure markers:

```
$ perch deploy
🛫 deploy
  ⏳ docker compose up -d        ─────────  (2.3s)
  ⏳ kubectl apply -f manifest    ─────────  (0.8s)
  ✓ deploy.sh                                (3.1s total)
```

`--quiet` suppresses everything but errors. `--json` emits the audit-log shape on stdout so the output is machine-readable.

Effort: medium. Touches the interpreter's stdout sinks. ~300 LOC.

#### C.4 `perch doctor`

Diagnose common setup issues:

```
$ perch doctor

  ✓ perch binary at /usr/local/bin/perch (v0.6.0)
  ✓ perch-lsp installed (v0.6.0)
  ✓ VS Code extension installed
  ⚠ commands.perch in cwd has 2 warnings — run `perch --check` to see them
  ✗ ~/.cache/perch directory permissions (0700, expected 0755)
       → fix: chmod 0755 ~/.cache/perch

  System:
    macOS 14.5, arm64, 10 CPU cores
    shell: zsh
    package manager: brew
```

Effort: small. ~200 LOC + a checklist.

#### C.5 Long-running notifications

When a `--server` or detached invocation completes (success or failure), fire a desktop notification:
- macOS: `osascript -e 'display notification ...'`
- Linux: `notify-send`
- Windows: PowerShell `New-BurntToastNotification`

Useful for builds and deploys that take minutes. Opt-in via `--notify`.

Effort: small. ~100 LOC + per-OS fallback.

---

### Phase D — Errors & recovery

#### D.1 Suggestions on every error

We already do this for unknown command names ("did you mean: deploy?"). Extend:

- **Unknown op kind** → "did you mean `http_get`? closest match by Levenshtein"
- **Unknown placeholder `${X}`** → "no binding named X. Available: target, host. Did you mean ${target}?"
- **Arg type mismatch** → "arg `port` is type int; got 'abc'. Quote a number, e.g. `-port=8080`."
- **File-not-found in `cp`/`rm`** → "no file at /tmp/foo. Did you mean: /tmp/food? (sibling in same dir)"

Each is a small addition at the error site. The hard part is having the *available bindings list* visible at the moment of error, which the interpreter already has.

Effort: medium overall, each fix small. ~500 LOC across the interpreter.

#### D.2 Every error links to a doc

```
$ perch deploy
op print: env var ${SECRET_KEY} is not in --env allowlist
  → see https://luowensheng.github.io/perch/sandbox/#env-allowlist
```

Several errors already do this (`--no-shell` links to sandbox.md). Extend uniformly.

Effort: small. Mostly content.

#### D.3 `perch why <error-token>`

A reverse-lookup for any error message:

```
$ perch why "unknown placeholder"
  This means perch saw `${name}` in a string but couldn't find `name`
  in any of the following:
    - command arguments
    - globals (top-level block)
    - per-command env vars
    - host environment variables
    - auto-bound names (home, cache_dir, etc.)

  Fix:
    1. Declare an arg with `arg name type string end` in the command's config
    2. Set a global: globals NAME = "value" end
    3. Pass via host env: NAME=value perch <cmd>
    4. Allow it with `--env NAME` if it's a host env var
```

Effort: medium. A static table of error tokens → explanations. ~400 LOC + content.

---

### Phase E — Sharing / handing off

#### E.1 `perch present FILE`

A read-only "explainer" view of a `.perch` file:

```
$ perch present deploy.perch

  ═══ deploy.perch ═══
  About: Deploy the app to staging or prod
  Version: 0.5.0

  Commands you can run:

  ┃ deploy            Roll out a release
  ┃   Arguments:
  ┃     -target=…    target (default: "staging")
  ┃   Will:
  ┃     · print "starting deploy"
  ┃     · run shell: docker compose up -d
  ┃     · …

  ┃ rollback          Revert to previous version
  ┃   …

  Security posture (run `perch --scan` for the full audit):
    Needs: shell (docker, kubectl), network (api.github.com), writes to ${APP_DIR}

  Run safely:
    perch --no-subprocess --allow-bin docker,kubectl --no-shell-metachars \
          --env APP_DIR,HOME --max-runtime 600 -f deploy.perch <command>
```

Combines `--check`, `--scan`, and per-command help into one rendered view aimed at a reader who isn't going to modify the file.

Effort: small once `--scan` is shipped (which it is). ~150 LOC reusing existing analyzers.

#### E.2 Web UI as the default for non-engineers

For the support-team-clicks-a-button use case, the answer should be: ship the binary with `perch --server` baked in as the default. A non-engineer doesn't `cd` to a directory and type `perch deploy`; they double-click an icon and click a button.

The path: `perch --build --server-only -o my-deploy-ui` produces a binary that, when run, opens `--server` on a free port and pops up the URL in the default browser. No CLI knowledge required.

Effort: medium. ~200 LOC + a new build flag. Plus a polish pass on the web UI itself (Phase C.2).

---

### Phase F — Discoverability

#### F.1 `perch ops`

Searchable op catalog right in the CLI:

```
$ perch ops
(150 ops; ↑↓ to navigate, / to filter, ↵ to see details)

  bundle_dir
  bundle_extract
  bundle_hash
  cp
  download
  …

$ perch ops download
  download URL DST
    Downloads URL to file DST. Creates parent dirs as needed.
    Example: download "https://example.com/x.tar.gz" "/tmp/x.tar.gz"
```

A tiny TUI built on `bubbletea` (or a flat list if we want to avoid the dep).

Effort: small (~250 LOC) without a TUI; medium with one.

#### F.2 `perch docs <topic>`

Opens the right doc page in a browser:

```
perch docs sandbox          → https://luowensheng.github.io/perch/sandbox/
perch docs cp               → https://...op-reference/#cp
perch docs --offline        → open the local copy bundled inside the binary
```

Effort: tiny. ~80 LOC mapping topic → URL.

#### F.3 Embedded offline docs

Bundle the rendered docs site inside the binary (via `--build` style embedding). `perch docs --offline` serves it via the existing `--server` on a free port. Useful for airgapped environments and travelling.

Effort: medium. ~150 LOC plus a build-time docs bundling step.

---

## 4. Prioritized roadmap

Three phases. Each phase delivers a meaningful UX bump and is shippable independently.

### Phase 1 — "Doesn't feel hostile" (small-effort wins)

These are mostly content + small code additions. Target: shipped in ~1 week of focused work.

- A.2 First-run greeting (tiny)
- B.1 Template library — 5 templates (small)
- C.1 `--explain` (small, reuses scan analyzer)
- C.4 `perch doctor` (small)
- D.2 Doc links on every error (small)
- E.1 `perch present` (small, reuses scan + commandhelp)
- F.2 `perch docs <topic>` (tiny)

**After this phase:** a new user can self-onboard, audit a script, and find the right doc without leaving the terminal.

### Phase 2 — "Genuinely friendly" (medium-effort wins)

Target: shipped in ~2-3 weeks of focused work.

- A.1 `--init --interactive` wizard (small-medium)
- A.3 `perch tutorial` — in-terminal walkthrough (medium)
- B.3 LSP snippet expansion (medium)
- C.2 Web UI redesign with previews + recent runs (medium)
- C.3 Output formatting (progress + color + `--quiet` + `--json`) (medium)
- C.5 Desktop notifications (small)
- D.1 Smart suggestions on every error (medium)
- D.3 `perch why <token>` (medium)
- F.1 `perch ops` searchable catalog (small)

**After this phase:** non-engineers can use the web UI productively; the CLI feels like a thoughtful tool, not a kit.

### Phase 3 — "Adoption-ready" (larger, more ambitious)

Target: shipped over a quarter alongside other work.

- B.2 `perch ai` LLM-assisted authoring (small but politically loaded — opt-in design matters)
- E.2 Web UI as default binary mode for non-engineers (medium)
- F.3 Embedded offline docs (medium)
- A polish pass on every error message and every help text (slow, ongoing)

**After this phase:** perch is something you'd hand to a support team without a how-to.

---

## 5. What we should NOT do

Worth saying explicitly:

- **Don't add a non-perch DSL on top.** The temptation to "make it look like Python" or "make it look like YAML" is real. Resist it. The cost of a second surface to learn outweighs the cost of helping users learn the one we have.
- **Don't bundle AI without an opt-in.** A perch binary should never call out to an external LLM by default. `perch ai` is opt-in, BYO key, off in the bundle.
- **Don't hide complexity that users will need.** The `--no-shell` / `--allow-bin` / `--env` flags are *the* security model. We can wrap them, we can recommend defaults, but we can't pretend they don't exist — users need them later.
- **Don't grow the op catalog endlessly chasing convenience.** Each new op is a new thing to learn. Adding `path_join` (clear win) is fine; adding `path_join_with_separator_and_normalize` (50 character function name) isn't.

---

## 6. What success looks like

In one paragraph:

> **A user with no prior perch knowledge can run `perch --init --interactive`, choose "wrap an existing tool," fill in three prompts, get a working `.perch` file, audit it with `perch present`, and run it with the suggested-safe flags — all without opening a browser. A non-engineer at the same team can then click a button in `perch --server` to run that same command, see a plain-English description of what's about to happen, and watch a progress bar while it runs. Both users feel like the tool is on their side.**

That's the bar. Everything in this report exists to move us toward it.

---

## 7. Open questions

- **Internationalisation.** Almost every UX choice here assumes English. If perch grows internationally, error messages / `--explain` outputs need to be translatable. Out of scope for v1 of this report, but worth noting.
- **Accessibility.** The web UI changes in C.2 should be screen-reader friendly. The terminal output in C.3 should be color-blind safe and degrade gracefully on dumb terminals.
- **Telemetry.** We currently collect nothing. A *strictly opt-in* "which ops are used most" / "which errors fire most" pipeline would tell us which UX investments matter most. Politically sensitive; default-off.
- **Multi-user web UI.** Today's `--server` has no auth — fine for `localhost`, wrong for shared deployments. If non-engineers reach perch via a hosted web UI, that's a real auth + audit story to design (and out of scope for this report).

These deserve their own reports when they become relevant.

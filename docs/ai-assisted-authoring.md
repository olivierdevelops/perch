# AI-assisted authoring in the web UI

> **Goal:** let a non-engineer using `perch --server` describe what they want in plain English and get back a reviewable `.perch` snippet — without ever sending data to perch's servers and without ever auto-executing code.

This is the design report. It covers the user-facing flows, the architecture, the safety/privacy posture, and a phased implementation plan. Companion to [user-experience.md](user-experience.md) (where AI-assisted authoring is item B.2 in the broader UX roadmap) and reusing the [SKILL.md](https://github.com/luowensheng/perch/blob/main/skills/perch/SKILL.md) authoring guide as the LLM's system prompt.

---

## 1. Why through the UI specifically

CLI users can already pipe to whatever AI tool they like. They have shell history, editors, and habits. The UI audience is different:

- They opened `perch --server` because they're not a CLI native.
- They see a button labeled "deploy" but want one labeled "back up postgres weekly to S3" — which doesn't exist yet.
- Asking them to learn capy syntax to add it is the wrong answer.
- Asking them to file a ticket with the team to add it is also the wrong answer.

The right answer: **a "Compose" panel in the web UI** that takes plain English, returns a reviewable snippet, and lets them save it with one click — after a human-readable explanation and a diff.

---

## 2. The user-facing flows

Four distinct interactions, in priority order. Build #1 first; the rest layer on cleanly.

### 2.1 Compose — generate a new command

The headline flow. A new "+ Compose with AI" tab next to the existing command buttons. The user types:

> *"Back up the production postgres database to S3 every Sunday at 03:00 UTC. Use pg_dump piped to gzip. Keep the last 12 weeks of backups."*

The panel streams a response with four sections:

1. **Plain-English summary** — what the AI thinks the user wants.
2. **Generated `.perch` snippet** — syntax-highlighted, complete (args + body + description).
3. **What this needs** — capabilities (shell access? network? specific binaries?). Pulls from the same analyzer that powers `perch --scan`.
4. **Follow-up questions** — "Should I add a `--dry-run` arg?" "Where should the backups land — `${cache_dir}` or a fixed `/var/backups`?"

Three buttons at the bottom: **Discard**, **Edit before saving**, **Save to file**. "Save" appends the generated `command … end` block to the file's `.draft`, then opens a side-by-side diff against the live file. The user clicks "Apply diff" to merge — *never auto-merged*, *never auto-run*.

### 2.2 Refine — modify an existing command

Each existing command card gets a small "Refine with AI" link. Clicking opens the panel pre-filled with the current command's source. The user types:

> *"Add a `--target` arg that picks between staging and prod. Default to staging. Refuse prod unless the user passes `--confirm prod`."*

The AI returns a diff of the original. Save flow is identical to Compose.

### 2.3 Explain — plain-English description

A "What does this do?" link on each command runs the AI in *read-only* mode, returns a human-readable summary. No edit, no save. Useful for non-engineers reading a file someone else wrote.

This complements `perch --scan` (which is security-focused) and `perch <cmd> --help` (which is structural).

### 2.4 Audit — natural-language security review

Sister flow to `perch --scan`. The AI gets the file + the scan report and writes prose prioritising risks, suggesting fixes, explaining trade-offs in the user's project's terms ("you probably don't want kubectl to run unbounded in this context because…"). Helpful when the bare `--scan` table isn't enough context.

---

## 3. Architecture

### 3.1 Provider abstraction

```go
// infra/aigen/aigen.go
package aigen

type Provider interface {
    Name() string                                  // "anthropic" / "openai" / "ollama" / "lmstudio"
    Compose(ctx context.Context, req Request) (<-chan Chunk, error)
}

type Request struct {
    System       string   // SKILL.md, embedded at build time
    File         string   // current commands.perch (full source)
    UserPrompt   string   // what the user typed
    Mode         Mode     // Compose / Refine / Explain / Audit
    TargetBlock  string   // for Refine: the command being modified
    MaxTokens    int
}

type Mode int
const (
    ModeCompose Mode = iota
    ModeRefine
    ModeExplain
    ModeAudit
)

type Chunk struct {
    Text string  // partial output
    Done bool    // final chunk
    Err  error
}
```

Adapters live in `infra/aigen/anthropic.go`, `infra/aigen/openai.go`, etc. — each implements `Provider`, talks to its own SSE / streaming endpoint, normalises into the common `Chunk` channel.

### 3.2 Configuration

`~/.config/perch/ai.toml`:

```toml
default_provider = "anthropic"

[providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-opus-4-5"
endpoint = "https://api.anthropic.com/v1/messages"

[providers.openai]
api_key_env = "OPENAI_API_KEY"
model = "gpt-4"

[providers.ollama]
endpoint = "http://localhost:11434"
model = "llama3.3:70b"
# No API key — local model.
```

`perch ai-config` opens the file in `$EDITOR` (or shows the path on stdout). `perch ai-config --test` makes a one-token round-trip to verify the configured provider works.

Keys never appear in the binary, never appear in logs, never leave the user's machine except in the LLM request body. Same security posture as the host shell using `${ANTHROPIC_API_KEY}`.

### 3.3 System prompt — reuse SKILL.md

The Claude Code skill at `skills/perch/SKILL.md` already encodes perch's grammar, op catalog, anti-patterns, and worked examples. It's been carefully tuned so Claude writes correct perch. Embed it via `//go:embed` and use it as the system prompt verbatim across all four modes.

Per-request additions:
- For **Compose**: append "Generate a single `command … end` block. Return JSON with fields `summary`, `perch_source`, `needs`, `follow_ups`. Match perch grammar exactly; the user runs `--check` immediately."
- For **Refine**: append the target command's current source + "Modify only this command; do not regenerate the whole file."
- For **Explain**: append "Read-only. Describe in plain English what this command does, in two sentences."
- For **Audit**: append the `--scan` report verbatim + "Prioritise the findings; suggest fixes in this project's context."

This reuse is the win: the same authoring rules that help Claude Code generate good perch from a CLI prompt help any LLM generate good perch from a UI prompt.

### 3.4 HTTP surface

Add four endpoints to the existing `infra/httpserver`:

```
POST /api/ai/compose   { prompt, file_path } → SSE stream of chunks
POST /api/ai/refine    { command, prompt, file_path } → SSE stream
POST /api/ai/explain   { command, file_path } → text
POST /api/ai/audit     { file_path } → SSE stream
```

SSE so the user sees the response as it's generated (the streaming UX is a big part of "feels like a tool that's helping you"). Requests are localhost-only by default; the same `--server` binding rules apply.

### 3.5 Save flow

Generated text never overwrites the user's file directly. Pipeline:

1. AI emits the generated block.
2. UI shows the diff (server-side rendered with a small Go diff lib).
3. User clicks **Apply**.
4. Server writes `commands.perch.draft` with the proposed contents.
5. Server runs `perch --check` against the draft. If it fails, the diff view turns red with the error inline; the user has to fix or retry.
6. If `--check` passes, the diff turns green; the user clicks **Commit**.
7. Server moves `.draft` → `.perch` atomically and triggers a UI reload so the new command appears.

`perch --check` as the gate is critical — the AI WILL occasionally produce subtly-wrong syntax. The validator catches it before the user is exposed to a runtime error.

### 3.6 Transcript history

Every AI interaction is appended to `commands.perch.ai.ndjson` (one record per prompt + response). Same shape as the runtime audit log:

```json
{"ts":"...","mode":"compose","prompt":"back up postgres ...","provider":"anthropic","model":"claude-opus-4-5","response_chars":1842,"applied":true}
```

Useful for:
- "Why does this command exist?" — grep the prompt history.
- Reviewing what was generated vs hand-written when the team rotates.
- A future "undo AI suggestion" feature.

`--audit-ai PATH` overrides the location; default is alongside the `.perch` file.

---

## 4. Safety posture

Three hard rules:

1. **The AI never executes code.** It only ever emits text. The text becomes a `.perch` file fragment that goes through normal `--check` validation; running it is a separate human action.

2. **No data leaves the user's machine except to the user's configured LLM provider.** perch's own servers receive nothing. There is no telemetry, no analytics, no "phone home." The only network traffic is to the endpoint the user configured.

3. **AI-generated commands are obvious in the file.** Every saved block gets a comment header:

   ```capy
   # ─── ai-generated 2024-12-08 — model: claude-opus-4-5 ───
   # prompt: "Back up postgres weekly to S3"
   command backup_postgres
       …
   end
   ```

   So a reviewer pulling the file six months later knows which parts came from an LLM and what was asked. Like a git author trailer, but in-file.

Soft guards on top:

- **Default-off.** `perch --server` does not show the Compose panel unless `~/.config/perch/ai.toml` exists with at least one configured provider. A user who never touches the config file sees the UI exactly as it is today.
- **Capability previewed before save.** The "what this needs" section runs the AI's draft through the `--scan` analyzer so the user sees "this will need shell access to kubectl" *before* they click Save.
- **Soft rate limit.** ~20 AI requests per hour per session, with a clear message when hit. Stops runaway costs from a stuck typing loop. User-overridable in the config.

---

## 5. Privacy

Four guarantees, in plain language:

1. **The user provides their own API key.** Anthropic, OpenAI, OpenRouter, or a local Ollama / LM Studio for fully-offline use. perch never ships keys.

2. **What gets sent.** Per request: the system prompt (SKILL.md), the user's current `commands.perch` file, the user's typed prompt, and (for Refine/Explain) the specific command. **Not** sent: anything outside the file, environment variables, host facts, the audit log.

3. **What gets received.** The model's response. Streamed token-by-token to the UI. Persisted only to `commands.perch.ai.ndjson` locally if the user clicks Apply.

4. **Local-model option.** Ollama or LM Studio in the config = nothing leaves the laptop. Whole feature works on an airgapped machine, just with a smaller model.

The privacy story matches "Claude Code writes a file for me" — same shape, same trust boundaries.

---

## 6. Implementation sketch

```
infra/aigen/
    aigen.go               Provider interface + Request/Chunk types
    config.go              ~/.config/perch/ai.toml loader
    anthropic.go           Anthropic API adapter (SSE)
    openai.go              OpenAI API adapter (SSE)
    ollama.go              Ollama local adapter
    prompt.go              Builds the per-mode system prompt
    diff.go                Server-side diff renderer

infra/httpserver/
    server.go              + /api/ai/* endpoints (when aigen.Configured())
    server.html            + Compose panel, Refine link, Explain link

usecases/aiconfig/
    aiconfig.go            `perch ai-config` and `--test` subcommands

skills/perch/SKILL.md      (already exists — reused verbatim as system prompt)
```

Estimated size: ~1,500 LOC + ~400 lines of HTML/JS. Largest single piece is the provider abstraction; second-largest is the Compose panel UI.

---

## 7. Phased implementation

Each phase is shippable. Stop after Phase 1 if AI assistance is enough.

### Phase 1 — Compose, one provider, one mode (~3 days)

- Anthropic adapter only (Claude was the SKILL.md target; tightest fit).
- `Compose` mode only.
- New panel in `--server` web UI behind a feature flag (`PERCH_AI=1` env var to enable).
- Save flow with `--check` gate, draft file, diff view, explicit commit.
- AI-generated header comments in saved blocks.
- Local transcript log (`commands.perch.ai.ndjson`).

**Ship criterion:** a user with an Anthropic API key can type "back up postgres weekly" and get a working `command backup_postgres … end` block that `--check`s clean.

### Phase 2 — Refine + Explain modes (~2 days)

- `Refine` and `Explain` endpoints + UI hooks on existing command cards.
- Diff renderer for Refine.
- Read-only explanation rendering for Explain (no save flow).

### Phase 3 — Multi-provider + local model (~2 days)

- OpenAI adapter.
- Ollama adapter (local model — biggest privacy win).
- `perch ai-config` and `perch ai-config --test` subcommands.
- Provider switcher in the UI.

### Phase 4 — Audit mode (~2 days)

- Wire the `--scan` analyzer into the AI prompt; emit a prioritised, prose security review.
- "AI security review" button next to "Run security scan" in the UI.

### Phase 5 — Soft polish (~ongoing)

- Rate-limit display.
- "Why does this command exist?" — grep the local transcript for any command's history.
- Cost-per-request estimator (Anthropic / OpenAI token-pricing aware).
- Undo-AI-suggestion via the transcript.

---

## 8. What this is NOT

To be honest about the boundaries:

- **It's not autonomous.** The AI never runs perch commands. It only ever returns text that the user reviews and saves.
- **It's not the only way to author perch.** Editing `commands.perch` by hand, using the LSP, or using `perch --import` all still work. The AI panel is a third option for users who'd otherwise be stuck.
- **It's not bundled.** Default install ships without AI. The Compose panel only appears when the user opts in by writing the config file. The binary has no LLM client unless the user configures one.
- **It's not a chatbot.** This is *structured-output* AI: every response is a JSON envelope shaped to fit perch's grammar. Not free-form conversation, not memory across sessions (transcripts are local-only and only for the user's grep).
- **It's not a substitute for `--scan` or `--check`.** Those run statically and deterministically. AI Audit is a supplement that adds prose context; it doesn't replace the static checks. Every save still runs through `--check`.

---

## 9. Composition with existing features

This is the part that makes AI-assisted authoring actually safe:

| Existing | How AI plays with it |
|---|---|
| `perch --check` | Every AI save is gated by `--check`. Invalid output never lands. |
| `perch --scan` | Audit mode hands the `--scan` report to the LLM as context. The recommended invocation appears alongside the AI's prose. |
| `perch --ask` / `--dry-run` | A newly-generated command can be previewed before its first run. The user *should* `--ask` it the first time. |
| `--audit FILE.ndjson` | Runtime audit log. Combined with `commands.perch.ai.ndjson`, you get "this command was AI-suggested by prompt P on date D, then ran X times with these args." Full provenance. |
| `--no-shell` / `--allow-bin` / `--env` | The AI's "what this needs" section is generated by running the draft through the scan analyzer. The user sees the recommended-tightest flags before saving. |
| MCP server (`perch-mcp`) | Future: a perch-mcp tool could itself be one of the LLM providers — i.e., perch hosting AI assistance for a parent LLM. Out of scope for v1. |
| `perch present` | A reviewer reading a file written with AI assistance sees the AI header comments inline. Provenance is part of the file. |

The AI layer is additive, not load-bearing. Everything still works without it.

---

## 10. Summary

**One paragraph:**

> The web UI gets a "+ Compose with AI" panel. Users type natural language; the AI emits a `.perch` snippet that's validated by `--check`, previewed via the `--scan` analyzer, and only saved after the user clicks Apply on a side-by-side diff. The AI never runs code, never sees data outside the file, and never operates without the user's own API key. SKILL.md is the system prompt across modes (Compose / Refine / Explain / Audit). A local-model option (Ollama) makes the whole feature offline-capable. Every AI-saved block carries an in-file comment header so the provenance lives with the file forever. Default-off; opt-in via `~/.config/perch/ai.toml`. ~1,500 LOC over four 2–3-day phases. Phase 1 is one provider (Anthropic), one mode (Compose), and a working save flow gated by `--check`.

That's the design. Open questions in the [UX report's §7](user-experience.md#7-open-questions) about i18n / accessibility / telemetry / multi-user web UI auth all apply here too. Feedback welcome on the [tracking issue](https://github.com/luowensheng/perch/issues).

# AI-assisted authoring — via MCP, not via perch

> **The architecture:** perch ships zero LLM client code. AI-assisted authoring happens in the agent's host (Claude Desktop, Cursor, Zed, custom MCP client) using perch-mcp as the workspace. The agent brings its own AI access; perch provides the file operations, validation, and analysis.

This is the design report for AI-assisted `.perch` authoring. An earlier draft of this doc proposed baking LLM client adapters into perch itself; that was the wrong shape. The right one is **agent → MCP → perch**. perch never holds an API key, never makes an outbound LLM call, never embeds a provider SDK. The agent already has all of those; the agent's user already trusts it; perch's role is the workspace, not the AI.

---

## 1. Why MCP and not a baked-in AI client

Four reasons:

1. **The trust boundary already exists.** The user installed Claude Desktop / Cursor / Zed. They already gave that app their API key. They already accepted that app sending data to a model. Adding a second trust boundary (in perch) duplicates the audit surface without adding capability.

2. **The AI is the agent's responsibility, not the tool's.** This is the same logic that makes `git` better off NOT shipping an LLM client. The agent calls `git`; the agent reasons about diffs; the agent's host owns the model access. Same here.

3. **MCP is *exactly* the integration shape for this.** Model Context Protocol exists so an agent can call tools and read resources in a structured, sandboxable way. `perch-mcp` already exposes `perch_list` and `perch_run` for execution. Adding `perch_read_file`, `perch_write_draft`, `perch_check`, `perch_scan`, `perch_present` extends the same surface for authoring.

4. **Zero LLM code in perch keeps the security story clean.** A `perch` binary that never makes outbound HTTP to an LLM provider is much easier to reason about than one with a provider adapter and a configurable system prompt. `--no-network` actually means no network.

---

## 2. The architecture

```
┌──────────────────────────┐
│  User                    │
│  ───                     │
│  "Back up postgres       │
│   weekly to S3"          │
└────────────┬─────────────┘
             │ types in
             ▼
┌──────────────────────────┐    LLM provider
│  Claude Desktop /        │◄────────────────► (Anthropic / OpenAI / Ollama)
│  Cursor / Zed /          │      agent's
│  any MCP client          │      existing
│                          │      AI access
└────────────┬─────────────┘
             │ MCP JSON-RPC over stdio
             ▼
┌──────────────────────────┐
│  perch-mcp               │
│  ───                     │
│  Tools:                  │
│   perch_list / _run      │  ← execution (already shipped)
│   perch_read_file        │  ← authoring (new)
│   perch_write_draft      │
│   perch_diff             │
│   perch_apply_draft      │
│   perch_check            │
│   perch_scan             │
│   perch_present          │
│  Resources:              │
│   perch://skill          │  ← skills/perch/SKILL.md
│   perch://op-catalog     │  ← docs/op-reference.md
│   perch://file/<path>    │  ← current .perch source
└────────────┬─────────────┘
             │ in-process Go calls
             ▼
┌──────────────────────────┐
│  perch core              │
│  ───                     │
│  capyloader              │
│  interpreter             │
│  ops registry            │
│  scan analyzer           │
│  validate                │
│  (no LLM code anywhere)  │
└──────────────────────────┘
```

perch never sees a model, an API key, or an HTTP request to an LLM provider. The agent does all of that on its own. perch-mcp is the bridge — it lets the agent read, draft, validate, analyze, and commit changes to a `.perch` file, using the same Go libraries that power the CLI.

---

## 3. The agent's flow (concretely)

User types into Claude Desktop's chat:

> *"Add a command that backs up our postgres database to S3 every week."*

The agent's host invokes its model with all of the user's chat history plus MCP tool definitions. The model decides to:

1. **Read the workspace** — calls `perch_read_file` to get the current `commands.perch`. The MCP server returns the source.
2. **Read the authoring guide** — fetches the `perch://skill` resource. The MCP server returns `skills/perch/SKILL.md`.
3. **Read the op catalog** — fetches `perch://op-catalog`. Now the model has SKILL.md + every op in scope.
4. **Compose** — using its host's LLM, the model generates a `command backup_postgres … end` block.
5. **Validate as it goes** — calls `perch_write_draft` to put the proposal into `commands.perch.draft`, then `perch_check` to verify it parses. If `perch_check` returns errors, the model iterates.
6. **Analyse** — calls `perch_scan` on the draft. Returns the needs-{shell, network, write} report + risk findings.
7. **Show the user** — the agent presents the generated command + the scan analysis + a diff vs. the live file, all *in the agent's chat UI*, not in perch.
8. **Wait for approval** — the user clicks the agent's "Apply" button (whatever Claude Desktop / Cursor calls it).
9. **Commit** — the agent calls `perch_apply_draft`. The MCP server atomically renames `.draft` → `.perch` and signals the change.

perch's role across all 9 steps: read files, run analyzers, hold a `.draft` until told to commit. No LLM. No keys. No prompts. **It's a workspace, not an AI.**

---

## 4. The new MCP tools — minimal surface

Five new tools on top of the existing `perch_list` / `perch_run`. Each is a thin wrapper around an existing Go function:

| Tool | Wraps | Purpose |
|---|---|---|
| `perch_read_file` | `os.ReadFile` | Return the current `.perch` source so the agent has context |
| `perch_write_draft` | `os.WriteFile` to `*.draft` | Propose a change without touching the live file |
| `perch_check` | `usecases/validate` | Run `--check` against the live file or a draft |
| `perch_scan` | `usecases/scan` | Run `--scan` against the live file or a draft |
| `perch_present` | `usecases/scan` + per-command help | Plain-English summary of every command |
| `perch_diff` | `golang.org/x/tools/internal/diff` (or stdlib) | Server-rendered diff between live and draft |
| `perch_apply_draft` | `os.Rename` of `.draft` → `.perch` | Commit after user approval |

That's it. Each tool has a JSON schema; the agent's host generates the schema as it does for any MCP server. The reuse of existing Go code (validate, scan) means no logic is duplicated — the agent sees exactly what `perch --check` and `perch --scan` see at the CLI.

### Plus MCP resources (read-only, content addressable)

| Resource URI | Returns |
|---|---|
| `perch://skill` | `skills/perch/SKILL.md` — the authoring guide |
| `perch://op-catalog` | `docs/op-reference.md` — every built-in op |
| `perch://language` | `docs/language.md` — the grammar reference |
| `perch://file/<path>` | The named `.perch` file's source |

Resources are the MCP-native way to hand "background context" to a model. They cost the agent zero per-request bandwidth once cached and they let the agent load the right docs *as needed* rather than having SKILL.md inflated into every system prompt.

---

## 5. What changes inside perch-mcp

The existing `cmd/perch-mcp/` binary already handles MCP framing (JSON-RPC over stdio, content-length framed). It currently registers two tools. We add the seven above + four resources. Everything else stays the same.

Concrete code shape:

```
cmd/perch-mcp/
    main.go            unchanged — wiring
    tools/
        list.go        existing — perch_list
        run.go         existing — perch_run
        read_file.go   NEW
        write_draft.go NEW
        check.go       NEW (calls usecases/validate)
        scan.go        NEW (calls usecases/scan)
        present.go     NEW (renders the scan + per-command help into prose)
        diff.go        NEW
        apply_draft.go NEW
    resources/
        skill.go       NEW (returns embedded skills/perch/SKILL.md)
        ops.go         NEW (returns embedded docs/op-reference.md)
        language.go    NEW (returns embedded docs/language.md)
        file.go        NEW (reads requested .perch from disk)
```

Estimated ~600 LOC. About a day-and-a-half of focused work.

### Safety invariants enforced by the server

The MCP layer enforces a few things the agent can't bypass even if it wants to:

- **No execution from authoring tools.** `perch_write_draft` writes to `*.draft` only. `perch_apply_draft` is the only path to the live file, and it explicitly checks the draft `--check`-passes before renaming.
- **The agent can read its workspace, not the whole disk.** `perch_read_file` and `perch://file/<path>` reject paths outside the workspace root (configurable via `perch-mcp --workspace DIR`, default = cwd).
- **No new running of perch commands during authoring.** `perch_run` exists, but the agent should not call it on a freshly-authored command without the user explicitly asking. We can't enforce that in code, but we can document it as a sharp edge for any MCP-client author reading our docs.

---

## 6. What happens on the user's screen

Two real workflows the design supports:

### 6.1 Claude Desktop user

1. User runs Claude Desktop with `perch-mcp` configured in `claude_desktop_config.json`.
2. User opens a chat: *"Add a command to back up postgres weekly."*
3. Claude (the assistant inside Claude Desktop) does the 9-step flow in §3.
4. Claude shows the user the generated command + the scan analysis + the diff *in the chat*. This is the Claude Desktop UI doing its native job — perch isn't involved in rendering.
5. User clicks "Approve" (Claude Desktop's UI element). Claude calls `perch_apply_draft`. Done.

### 6.2 perch --server user, wanting AI assistance

This is the case from the earlier (now-superseded) draft. The right answer here is:

**Recommend Claude Desktop running alongside `perch --server`.**

The user has two windows open: Claude Desktop for chat + AI, perch's web UI for clicking buttons to run commands. Both connect to perch-mcp; both see the same `.perch` file; changes Claude makes appear in perch's web UI on the next refresh. The user gets:

- A polished chat UI for prompting (Claude Desktop's job)
- A polished command-runner UI for invoking (`perch --server`'s job)
- Synchronisation through the file (which is where state belongs)

What we explicitly do NOT do: embed a chat panel inside `perch --server`. That would require perch to take on LLM client code, key management, provider adapters — exactly the design we just rejected.

If a user wants a single-window AI experience in their browser, the right path is for Claude Desktop / similar to ship a web UI mode, or for someone to build a thin web-MCP-bridge that puts a chat panel in front of any MCP server. That's a separate project; not perch's concern.

---

## 7. What this saves us

Compared to the discarded "AI in perch" design:

| Concern | Earlier design | This design |
|---|---|---|
| LLM provider adapters in perch | 3–4 (Anthropic / OpenAI / Ollama / LM Studio) | **0** |
| API key handling in perch | required (ai.toml + env var refs) | **none — agent owns it** |
| System prompt embedded in perch binary | yes (SKILL.md) | exposed as an MCP resource — agent reads when needed |
| Streaming SSE endpoint in perch's web UI | yes (`/api/ai/compose`) | **none — agent has its own chat UI** |
| Outbound HTTP from perch to LLM provider | yes | **none — `--no-network` actually means no network** |
| New panel UI in `server.html` | substantial | none |
| New use case in `usecases/` | `aiconfig` + provider wiring | none |
| Per-user opt-in mechanism | `~/.config/perch/ai.toml` | none — the agent's already-configured |
| LOC of new code | ~1,500 | **~600 (just MCP tools + resources)** |
| Trust-boundary additions | one (perch → LLM) | **zero** |

The MCP design is strictly less code, strictly less trust surface, strictly more aligned with existing protocols.

---

## 8. Implementation plan

### Phase 1 — Authoring tools (1.5 days)

- `perch_read_file` tool
- `perch_write_draft` tool (with workspace-root check)
- `perch_check` tool (wraps `usecases/validate`)
- `perch_diff` tool (server-rendered)
- `perch_apply_draft` tool (with --check gate)

**Ship criterion:** an agent connected via MCP can read the file, propose a change, validate it, diff it, and commit it.

### Phase 2 — Analysis tools (0.5 day)

- `perch_scan` tool (wraps `usecases/scan`)
- `perch_present` tool (wraps `usecases/scan` + `usecases/commandhelp` into prose)

**Ship criterion:** an agent can produce capability + risk reports without running the script.

### Phase 3 — Resources (0.5 day)

- `perch://skill` (embedded SKILL.md)
- `perch://op-catalog` (embedded op-reference.md)
- `perch://language` (embedded language.md)
- `perch://file/<path>` (reads from workspace)

**Ship criterion:** the agent can `read_resource("perch://skill")` and get the authoring guide as context without us inflating any prompts.

### Phase 4 — Documentation + recipes (0.5 day)

- Update [mcp.md](mcp.md) with the seven new tools' schemas.
- Add a short "authoring with Claude Desktop" walkthrough to the LLM control plane doc.
- Recipe: a Claude Desktop `mcpServers` entry that points at `perch-mcp` with the workspace flag.

Total ~3 focused days. Half the previously-estimated effort, with a stronger trust story.

---

## 9. What this is NOT

- **It is not an AI feature *in* perch.** perch core gains nothing. `perch` and `perch --server` and `perch-lsp` continue to have no LLM code. The only addition is MCP tools, which are how agents already talk to tools.
- **It is not autonomous.** The agent still asks the user to approve every change. `perch_apply_draft` exists precisely so there's a human-clicked moment.
- **It is not bundled.** A user who doesn't use Claude Desktop / Cursor / Zed / any MCP client gets exactly the perch they have today. The MCP server only runs when someone invokes `perch-mcp`.
- **It is not a chatbot.** Agents make MCP tool calls and read MCP resources. There's no chat protocol inside perch. The chat lives in the agent's host.
- **It is not a substitute for `--check` / `--scan`.** Those run on the host (via the CLI) and in the agent (via MCP). Same code, same results, same gates. The agent isn't allowed to commit a draft that doesn't pass `--check`.

---

## 10. Composition with existing features

| Existing | Role in this design |
|---|---|
| `perch-mcp` | The integration point. Gains 7 tools + 4 resources. |
| `skills/perch/SKILL.md` | Reused verbatim as the `perch://skill` MCP resource. Same content that helps Claude Code write good perch from a CLI now helps any MCP-connected agent write good perch from any chat. |
| `usecases/validate` | Wraps to `perch_check` tool. |
| `usecases/scan` | Wraps to `perch_scan` tool, also feeds `perch_present`. |
| `usecases/commandhelp` | Feeds `perch_present` (per-command help). |
| `--audit FILE.ndjson` | Unchanged. The agent's calls go through `perch_run` (existing), which already records to the audit log. |
| `perch --server` | Unchanged. If the user wants AI alongside it, they open Claude Desktop in another window pointed at the same workspace. |

The authoring-AI story is a layer the agent adds on top, not a layer perch adds inside. That's the architecture choice the earlier draft got wrong and this one gets right.

---

## 11. Summary in one paragraph

**AI-assisted authoring happens entirely in the agent's host.** perch ships zero LLM client code, zero API-key handling, zero outbound LLM HTTP. The agent (Claude Desktop, Cursor, Zed, custom MCP client) uses its own already-configured AI access to compose `.perch` snippets, and connects to `perch-mcp` for the workspace operations — read the file, propose a draft, run `--check` and `--scan`, render a diff, commit after the user approves. SKILL.md, the op catalog, and the language reference are exposed as MCP resources the agent loads on demand. The whole new surface is ~600 LOC of MCP tools that wrap existing `usecases/validate` and `usecases/scan`. The user sees AI assistance in their agent's chat UI; perch sees only tool calls. `--no-network` continues to mean no network, because nothing in perch ever calls a model. That's the design.

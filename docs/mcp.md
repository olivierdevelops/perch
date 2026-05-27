# MCP server

> **Looking for the "why"?** Read **[LLM control plane](llm-control-plane.md)** first — it's the case for using perch as the *entire backend* for an LLM's tool surface. This page is the reference for the `perch-mcp` binary itself.

`perch-mcp` is a [Model Context Protocol](https://modelcontextprotocol.io/) server. Point Claude Desktop / Claude Code / Cursor / Zed at it, and an AI agent can:

- **`perch_list`** — discover the commands in a project's `commands.perch`, including arg names/types/defaults.
- **`perch_run`** — execute a command with named arguments and read back the combined stdout/stderr.

Same code path as `perch <cmd>` from the CLI. The agent sees the same world your terminal does.

## Install

```sh
go install github.com/luowensheng/perch/cmd/perch-mcp@latest
```

Or build from source:

```sh
git clone https://github.com/luowensheng/perch.git
cd perch
go build -o perch-mcp ./cmd/perch-mcp
```

## Configure Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "perch": {
      "command": "perch-mcp",
      "args": ["-f", "/abs/path/to/your/project/commands.perch"]
    }
  }
}
```

Restart Claude Desktop. You should see `perch_list` and `perch_run` in the tools picker.

## Configure Claude Code

```sh
claude mcp add perch -- perch-mcp -f /abs/path/to/your/project/commands.perch
```

## Configure Cursor / Zed

Both follow the same `mcpServers` schema as Claude Desktop. Place the config block in the IDE's MCP settings.

## What the agent sees

After `tools/list`, the agent has two tools available:

```jsonc
{
  "name": "perch_list",
  "description": "List all callable commands in the current commands.perch, with their args, types, and descriptions.",
  "inputSchema": { "type": "object", "properties": {} }
}
```

```jsonc
{
  "name": "perch_run",
  "description": "Run a named perch command. Returns combined stdout/stderr output.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "command": { "type": "string" },
      "args":    { "type": "object" }
    },
    "required": ["command"]
  }
}
```

## Streaming progress (live output during long-running commands)

By default, `perch_run` returns the command's full output as a single
`tools/call` response when the run finishes. For commands that take
seconds or minutes, agents wait in silence.

If the agent sends `_meta.progressToken` in the `tools/call` request
(MCP spec convention — value can be a string OR a number), perch-mcp
streams every line of stdout / stderr as a `notifications/progress`
event the moment the line is produced. The agent's UI renders these
live; the final `tools/call` response still arrives at completion
with the accumulated output.

### What the wire looks like

Client → server:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "perch_run",
    "arguments": { "command": "deploy" },
    "_meta": { "progressToken": "deploy-abc" }
  }
}
```

Server → client (as each line is produced — interleaved with the run):

```json
{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":"deploy-abc","progress":1,"message":"==> Setting up","_meta":{"stream":"stdout"}}}
{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":"deploy-abc","progress":2,"message":"==> Building darwin","_meta":{"stream":"stdout"}}}
{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":"deploy-abc","progress":3,"message":"==> Built (12 MB)","_meta":{"stream":"stdout"}}}
…
```

Server → client (final response):

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [
      { "type": "text", "text": "==> Setting up\n==> Building darwin\n==> Built (12 MB)\n…" }
    ],
    "isError": false
  }
}
```

### What the agent sees

| MCP client | What renders during a streamed run |
|---|---|
| **Claude Desktop** | Inline progress in the tool-call card; each line as it arrives |
| **Claude Code** | Tool-call progress shown in the activity area |
| **Cursor / Zed** | Per-client behavior; most render `progress.message` live |
| **Custom MCP client** | You decide — listen for `notifications/progress` and update UI |

### Concurrency safety

perch's `parallel` block-op runs op handlers in goroutines. Each
goroutine's stdout writes turn into independent progress
notifications. The JSON-RPC encoder is mutex-guarded so concurrent
writes never produce malformed JSON. Each notification gets a
monotonic `progress` counter so a client can order them even when
two arrive within the same millisecond.

### Backwards compat

Clients that don't send `_meta.progressToken` get the old buffered
behavior — one final `tools/call` response with the full output. No
breaking change to existing integrations.

### What's NOT included in v1 (roadmap)

- **Cancellation.** MCP `notifications/cancelled` from the client is
  not honored yet. A long-running command runs to completion (or
  hits `--max-runtime`). On the roadmap.
- **Span-level progress.** Today every line of stdout/stderr emits a
  progress event. The interpreter's `Tracer` (the one that powers
  `--trace` / `--report`) could emit richer "op started / op
  completed" events that include kind, args, duration. v2 idea.
- **Resource-style streaming.** For very high-volume output, MCP
  resource subscriptions with subscriptions would be more efficient
  than per-line progress events. Not needed for any current real
  workload.

## Security model

The MCP server runs commands. **Everything in `commands.perch` is callable by the agent, including any `shell` ops inside command bodies.** Treat it like exposing `bash -c` over RPC.

Mitigations:

- Mark sensitive commands as `private` so they're not exposed via `perch_list` (and `perch_run` refuses to call them).
- Run the MCP server pointed at a curated `commands.perch` rather than your everyday one.
- The server has no network surface beyond stdio — it can't be reached from outside the launching process.
- It runs in your user account; nothing it spawns has more privilege than `perch` from your terminal.

## See also

- [skills/perch/SKILL.md](https://github.com/luowensheng/perch/blob/main/skills/perch/SKILL.md) — Claude Code skill for *writing* commands.perch files (complements the MCP server, which *runs* them).
- [docs/op-reference.md](op-reference.md) — what ops are available inside command bodies the agent will be calling.

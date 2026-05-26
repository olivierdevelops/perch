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

// Package main is an MCP server for perch.
//
// It speaks the Model Context Protocol (JSON-RPC 2.0 over stdio) so that
// AI agents — Claude Desktop, Claude Code, Cursor, Zed — can discover the
// commands in a project's commands.perch and invoke them.
//
// Two tools are exposed:
//
//   perch_list  → returns the list of commands in the current commands.perch,
//                 including each command's args/types/descriptions.
//   perch_run   → runs a named command with arguments. Output (stdout +
//                 stderr) is returned as the tool result.
//
// Configure your MCP client with something like:
//
//   {
//     "mcpServers": {
//       "perch": {
//         "command": "perch-mcp",
//         "args": ["-f", "/abs/path/to/commands.perch"]
//       }
//     }
//   }
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/capyloader"
	"github.com/luowensheng/perch/infra/interpreter"
	"github.com/luowensheng/perch/infra/ops"
)

const (
	protocolVersion = "2025-06-18"
	serverName      = "perch-mcp"
	serverVersion   = "0.1.0"
)

type rpcReq struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type rpcResp struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcErr         `json:"error,omitempty"`
}

type rpcErr struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

func main() {
	cfg := flag.String("f", "commands.perch", "Path to commands.perch")
	flag.Parse()

	handlers := ops.AllHandlers()
	reader := bufio.NewReader(os.Stdin)
	writer := bufio.NewWriter(os.Stdout)
	encoder := json.NewEncoder(writer)

	for {
		line, err := reader.ReadString('\n')
		if err == io.EOF {
			return
		}
		if err != nil {
			fmt.Fprintln(os.Stderr, "mcp: read:", err)
			return
		}
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		var req rpcReq
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			respondErr(encoder, writer, nil, -32700, "parse error: "+err.Error())
			continue
		}
		handle(encoder, writer, &req, *cfg, handlers)
	}
}

func respond(enc *json.Encoder, w *bufio.Writer, id json.RawMessage, result any) {
	_ = enc.Encode(rpcResp{JSONRPC: "2.0", ID: id, Result: result})
	w.Flush()
}

func respondErr(enc *json.Encoder, w *bufio.Writer, id json.RawMessage, code int, msg string) {
	_ = enc.Encode(rpcResp{JSONRPC: "2.0", ID: id, Error: &rpcErr{Code: code, Message: msg}})
	w.Flush()
}

func handle(enc *json.Encoder, w *bufio.Writer, req *rpcReq, cfgPath string, handlers map[string]interpreter.Handler) {
	switch req.Method {

	case "initialize":
		respond(enc, w, req.ID, map[string]any{
			"protocolVersion": protocolVersion,
			"serverInfo": map[string]any{
				"name":    serverName,
				"version": serverVersion,
			},
			"capabilities": map[string]any{
				"tools": map[string]any{},
			},
		})

	case "notifications/initialized":
		// no response for notifications

	case "tools/list":
		respond(enc, w, req.ID, map[string]any{
			"tools": []map[string]any{
				{
					"name":        "perch_list",
					"description": "List all callable commands in the current commands.perch, with their args, types, and descriptions.",
					"inputSchema": map[string]any{
						"type":       "object",
						"properties": map[string]any{},
					},
				},
				{
					"name":        "perch_run",
					"description": "Run a named perch command. Returns combined stdout/stderr output.",
					"inputSchema": map[string]any{
						"type": "object",
						"properties": map[string]any{
							"command": map[string]any{
								"type":        "string",
								"description": "Name of the command to run (from commands.perch)",
							},
							"args": map[string]any{
								"type":        "object",
								"description": "Named arguments to pass to the command. Keys are arg names; values are strings/numbers/bools.",
							},
						},
						"required": []string{"command"},
					},
				},
			},
		})

	case "tools/call":
		var params struct {
			Name      string          `json:"name"`
			Arguments json.RawMessage `json:"arguments"`
		}
		if err := json.Unmarshal(req.Params, &params); err != nil {
			respondErr(enc, w, req.ID, -32602, "invalid params: "+err.Error())
			return
		}
		switch params.Name {
		case "perch_list":
			handlePerchList(enc, w, req.ID, cfgPath)
		case "perch_run":
			handlePerchRun(enc, w, req.ID, cfgPath, handlers, params.Arguments)
		default:
			respondErr(enc, w, req.ID, -32601, "unknown tool: "+params.Name)
		}

	default:
		// Unknown notifications: silent. Unknown requests: error.
		if len(req.ID) > 0 {
			respondErr(enc, w, req.ID, -32601, "method not found: "+req.Method)
		}
	}
}

func handlePerchList(enc *json.Encoder, w *bufio.Writer, id json.RawMessage, cfgPath string) {
	p, err := capyloader.Load(cfgPath)
	if err != nil {
		respondToolResult(enc, w, id, fmt.Sprintf("Error loading %s: %v", cfgPath, err), true)
		return
	}
	var b strings.Builder
	fmt.Fprintf(&b, "perch program: %s (v%s)\n", p.Name, p.Version)
	if p.Description != "" {
		fmt.Fprintf(&b, "%s\n\n", p.Description)
	}
	keys := make([]string, 0, len(p.Commands))
	for k, c := range p.Commands {
		if c.Modifiers.Private {
			continue
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		c := p.Commands[k]
		fmt.Fprintf(&b, "• %s — %s\n", k, c.Description)
		for _, a := range c.Args {
			def := ""
			if a.HasDefault {
				def = fmt.Sprintf(" (default %v)", a.Default)
			}
			fmt.Fprintf(&b, "    -%s %s%s — %s\n", a.Name, a.Type, def, a.Description)
		}
	}
	respondToolResult(enc, w, id, b.String(), false)
}

func handlePerchRun(enc *json.Encoder, w *bufio.Writer, id json.RawMessage, cfgPath string, handlers map[string]interpreter.Handler, raw json.RawMessage) {
	var args struct {
		Command string         `json:"command"`
		Args    map[string]any `json:"args"`
	}
	if err := json.Unmarshal(raw, &args); err != nil {
		respondErr(enc, w, id, -32602, "invalid arguments: "+err.Error())
		return
	}
	if args.Command == "" {
		respondToolResult(enc, w, id, "missing 'command' argument", true)
		return
	}
	p, err := capyloader.Load(cfgPath)
	if err != nil {
		respondToolResult(enc, w, id, fmt.Sprintf("Error loading %s: %v", cfgPath, err), true)
		return
	}
	if cmd, ok := p.Commands[args.Command]; !ok || cmd.Modifiers.Private {
		if p.Catch == nil {
			respondToolResult(enc, w, id, fmt.Sprintf("Unknown command: %q", args.Command), true)
			return
		}
	}

	// Capture output.
	var stdout, stderr bytes.Buffer
	i := interpreter.New(handlers, p)
	i.Stdout = &stdout
	i.Stderr = &stderr

	// Convert the named-args map to -k=v argv.
	argv := []string{}
	for k, v := range args.Args {
		argv = append(argv, fmt.Sprintf("-%s=%v", k, v))
	}

	runErr := i.Run(args.Command, argv)

	out := stdout.String()
	if stderr.Len() > 0 {
		if out != "" {
			out += "\n"
		}
		out += "--- stderr ---\n" + stderr.String()
	}
	isErr := runErr != nil
	if isErr {
		if out != "" {
			out += "\n"
		}
		out += "--- error ---\n" + runErr.Error()
	}
	respondToolResult(enc, w, id, strings.TrimRight(out, "\n"), isErr)
}

func respondToolResult(enc *json.Encoder, w *bufio.Writer, id json.RawMessage, text string, isError bool) {
	respond(enc, w, id, map[string]any{
		"content": []map[string]any{
			{"type": "text", "text": text},
		},
		"isError": isError,
	})
}

// Silence unused-import lints when only domain is referenced indirectly.
var _ = domain.Op{}

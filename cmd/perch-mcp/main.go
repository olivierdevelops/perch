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
	"sync"
	"sync/atomic"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/capyloader"
	"github.com/luowensheng/perch/infra/interpreter"
	"github.com/luowensheng/perch/infra/ops"
)

// encMu serializes every write to the JSON-RPC stream. Required because
// progress notifications emitted from `parallel` block goroutines run
// concurrently with each other and with the main loop's response writes.
var encMu sync.Mutex

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
	encMu.Lock()
	defer encMu.Unlock()
	_ = enc.Encode(rpcResp{JSONRPC: "2.0", ID: id, Result: result})
	w.Flush()
}

func respondErr(enc *json.Encoder, w *bufio.Writer, id json.RawMessage, code int, msg string) {
	encMu.Lock()
	defer encMu.Unlock()
	_ = enc.Encode(rpcResp{JSONRPC: "2.0", ID: id, Error: &rpcErr{Code: code, Message: msg}})
	w.Flush()
}

// progressWriter is an io.Writer that fans out to a buffer (for the
// final tools/call response) AND to MCP progress notifications on the
// JSON-RPC stream (so the agent sees output as the command produces
// it, not buffered until completion).
//
// Required for any long-running perch verb — `kubectl apply`, a full
// deploy pipeline, a parallel build matrix. Without progress
// notifications, the agent waits in silence for minutes and then
// receives the whole output as one blob.
//
// MCP spec: progress notifications carry a `progressToken` that
// matches the one the client sent in `params._meta.progressToken`.
// We forward it verbatim (as json.RawMessage) so a client that uses
// a string token or a numeric token both work.
//
// Concurrency: perch's `parallel` block-op runs op handlers in
// goroutines, so multiple Write() calls can race. encMu serializes
// emissions onto the JSON-RPC stream; the accumulated buffer is
// guarded by its own mutex.
type progressWriter struct {
	enc           *json.Encoder
	w             *bufio.Writer
	progressToken json.RawMessage // nil → no notifications, just buffer
	stream        string          // "stdout" or "stderr" — passed in _meta
	progressN     uint64          // monotonic counter for the progress field
	accMu         sync.Mutex
	accumulated   *bytes.Buffer
}

func newProgressWriter(enc *json.Encoder, w *bufio.Writer, token json.RawMessage, stream string) *progressWriter {
	return &progressWriter{
		enc:           enc,
		w:             w,
		progressToken: token,
		stream:        stream,
		accumulated:   new(bytes.Buffer),
	}
}

func (p *progressWriter) Write(b []byte) (int, error) {
	// Always accumulate — the final tool result includes the whole output.
	p.accMu.Lock()
	p.accumulated.Write(b)
	p.accMu.Unlock()

	// Only emit progress notifications if the client asked for them.
	if p.progressToken == nil {
		return len(b), nil
	}

	n := atomic.AddUint64(&p.progressN, 1)
	notif := map[string]any{
		"jsonrpc": "2.0",
		"method":  "notifications/progress",
		"params": map[string]any{
			"progressToken": p.progressToken,
			"progress":      n,
			// MCP progress notifications carry an optional `message`
			// (the human-readable description). We strip the trailing
			// newline because the MCP client typically renders each
			// message as its own line.
			"message": strings.TrimRight(string(b), "\n"),
			// Non-standard `_meta` — clients that recognize it can
			// render stdout vs stderr distinctly; clients that don't
			// just ignore.
			"_meta": map[string]any{"stream": p.stream},
		},
	}
	encMu.Lock()
	_ = p.enc.Encode(notif)
	_ = p.w.Flush()
	encMu.Unlock()
	return len(b), nil
}

func (p *progressWriter) String() string {
	p.accMu.Lock()
	defer p.accMu.Unlock()
	return p.accumulated.String()
}

func (p *progressWriter) Len() int {
	p.accMu.Lock()
	defer p.accMu.Unlock()
	return p.accumulated.Len()
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
			// MCP spec: clients send `_meta.progressToken` (string or
			// number) when they want server-pushed progress
			// notifications. We pass it through to handlePerchRun so
			// every stdout/stderr write turns into a `notifications/
			// progress` event the client renders live.
			Meta *struct {
				ProgressToken json.RawMessage `json:"progressToken,omitempty"`
			} `json:"_meta,omitempty"`
		}
		if err := json.Unmarshal(req.Params, &params); err != nil {
			respondErr(enc, w, req.ID, -32602, "invalid params: "+err.Error())
			return
		}
		var progressToken json.RawMessage
		if params.Meta != nil {
			progressToken = params.Meta.ProgressToken
		}
		switch params.Name {
		case "perch_list":
			handlePerchList(enc, w, req.ID, cfgPath)
		case "perch_run":
			handlePerchRun(enc, w, req.ID, cfgPath, handlers, params.Arguments, progressToken)
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

func handlePerchRun(enc *json.Encoder, w *bufio.Writer, id json.RawMessage, cfgPath string, handlers map[string]interpreter.Handler, raw json.RawMessage, progressToken json.RawMessage) {
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

	// Two progressWriters — one per stream. Both accumulate to
	// independent buffers (so the final response can label stdout vs
	// stderr) AND emit MCP progress notifications when the client
	// requested them. If progressToken is nil, no notifications fire;
	// behavior degrades to the old buffer-and-return model.
	stdout := newProgressWriter(enc, w, progressToken, "stdout")
	stderr := newProgressWriter(enc, w, progressToken, "stderr")

	i := interpreter.New(handlers, p)
	i.PreflightHook = ops.Preflight
	i.Stdout = stdout
	i.Stderr = stderr

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

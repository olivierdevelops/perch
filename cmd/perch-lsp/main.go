// Package main is `perch-lsp`, a Language Server Protocol implementation
// for `.perch` files. It speaks JSON-RPC 2.0 over stdio with the
// LSP-standard `Content-Length:` framing.
//
// v1 capabilities:
//   - textDocument/didOpen / didChange / didClose
//   - textDocument/publishDiagnostics (parse errors + static validation)
//   - textDocument/completion         (keywords, op kinds, command names)
//   - textDocument/hover              (doc for the keyword/op under cursor)
//   - textDocument/documentSymbol     (outline of commands and args)
//
// Wire it up from any LSP client (Neovim, Helix, VS Code) — see
// docs/lsp.md.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"

	"github.com/olivierdevelops/perch/infra/capyloader"
	"github.com/olivierdevelops/perch/infra/interpreter"
	"github.com/olivierdevelops/perch/infra/ops"
	"github.com/olivierdevelops/perch/usecases/validate"
)

// ────────────────────────────────────────────────────────────────────────
// JSON-RPC framing
// ────────────────────────────────────────────────────────────────────────

type rpcMessage struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type framedReader struct{ r *bufio.Reader }

func newFramedReader(r io.Reader) *framedReader { return &framedReader{r: bufio.NewReader(r)} }

// Read one LSP message from the underlying stream. Returns the JSON
// payload bytes; framing headers are consumed and dropped.
func (f *framedReader) Read() ([]byte, error) {
	contentLength := -1
	for {
		line, err := f.r.ReadString('\n')
		if err != nil {
			return nil, err
		}
		line = strings.TrimRight(line, "\r\n")
		if line == "" {
			break
		}
		if strings.HasPrefix(strings.ToLower(line), "content-length:") {
			n, err := strconv.Atoi(strings.TrimSpace(line[len("Content-Length:"):]))
			if err != nil {
				return nil, fmt.Errorf("bad content-length: %w", err)
			}
			contentLength = n
		}
	}
	if contentLength < 0 {
		return nil, fmt.Errorf("missing Content-Length header")
	}
	buf := make([]byte, contentLength)
	if _, err := io.ReadFull(f.r, buf); err != nil {
		return nil, err
	}
	return buf, nil
}

func writeFramed(w io.Writer, payload []byte) error {
	header := fmt.Sprintf("Content-Length: %d\r\n\r\n", len(payload))
	if _, err := w.Write([]byte(header)); err != nil {
		return err
	}
	_, err := w.Write(payload)
	return err
}

// ────────────────────────────────────────────────────────────────────────
// Server state
// ────────────────────────────────────────────────────────────────────────

type server struct {
	mu       sync.Mutex
	docs     map[string]string // uri → current text
	out      io.Writer
	outMu    sync.Mutex
	handlers map[string]interpreter.Handler
}

func main() {
	s := &server{
		docs:     map[string]string{},
		out:      os.Stdout,
		handlers: ops.AllHandlers(),
	}
	r := newFramedReader(os.Stdin)
	for {
		raw, err := r.Read()
		if err != nil {
			if err == io.EOF {
				return
			}
			fmt.Fprintln(os.Stderr, "perch-lsp: read:", err)
			return
		}
		var msg rpcMessage
		if err := json.Unmarshal(raw, &msg); err != nil {
			continue
		}
		s.handle(&msg)
	}
}

// ────────────────────────────────────────────────────────────────────────
// Dispatch
// ────────────────────────────────────────────────────────────────────────

func (s *server) handle(req *rpcMessage) {
	switch req.Method {
	case "initialize":
		s.respond(req.ID, map[string]any{
			"capabilities": map[string]any{
				"textDocumentSync":      1, // 1 = full content on every change
				"completionProvider":    map[string]any{"triggerCharacters": []string{" ", "$", "{"}},
				"hoverProvider":         true,
				"documentSymbolProvider": true,
			},
			"serverInfo": map[string]any{
				"name":    "perch-lsp",
				"version": "0.1.0",
			},
		})
	case "initialized", "$/setTrace":
		// notifications — no response
	case "shutdown":
		s.respond(req.ID, nil)
	case "exit":
		os.Exit(0)

	case "textDocument/didOpen":
		s.didOpen(req.Params)
	case "textDocument/didChange":
		s.didChange(req.Params)
	case "textDocument/didClose":
		s.didClose(req.Params)
	case "textDocument/didSave":
		// no-op; we re-validate on every change

	case "textDocument/completion":
		s.completion(req)
	case "textDocument/hover":
		s.hover(req)
	case "textDocument/documentSymbol":
		s.documentSymbol(req)

	default:
		if len(req.ID) > 0 {
			s.respondErr(req.ID, -32601, "method not found: "+req.Method)
		}
	}
}

func (s *server) respond(id json.RawMessage, result any) {
	if len(id) == 0 {
		return // notifications get no response
	}
	s.send(&rpcMessage{JSONRPC: "2.0", ID: id, Result: result})
}

func (s *server) respondErr(id json.RawMessage, code int, msg string) {
	s.send(&rpcMessage{JSONRPC: "2.0", ID: id, Error: &rpcError{Code: code, Message: msg}})
}

func (s *server) notify(method string, params any) {
	b, _ := json.Marshal(params)
	s.send(&rpcMessage{JSONRPC: "2.0", Method: method, Params: b})
}

func (s *server) send(m *rpcMessage) {
	s.outMu.Lock()
	defer s.outMu.Unlock()
	b, _ := json.Marshal(m)
	_ = writeFramed(s.out, b)
}

// ────────────────────────────────────────────────────────────────────────
// Document tracking + diagnostics
// ────────────────────────────────────────────────────────────────────────

type didOpenParams struct {
	TextDocument struct {
		URI  string `json:"uri"`
		Text string `json:"text"`
	} `json:"textDocument"`
}

type didChangeParams struct {
	TextDocument   struct{ URI string `json:"uri"` } `json:"textDocument"`
	ContentChanges []struct {
		Text string `json:"text"`
	} `json:"contentChanges"`
}

type didCloseParams struct {
	TextDocument struct{ URI string `json:"uri"` } `json:"textDocument"`
}

func (s *server) didOpen(raw json.RawMessage) {
	var p didOpenParams
	if err := json.Unmarshal(raw, &p); err != nil {
		return
	}
	s.setDoc(p.TextDocument.URI, p.TextDocument.Text)
	s.publishDiagnostics(p.TextDocument.URI, p.TextDocument.Text)
}

func (s *server) didChange(raw json.RawMessage) {
	var p didChangeParams
	if err := json.Unmarshal(raw, &p); err != nil {
		return
	}
	if len(p.ContentChanges) == 0 {
		return
	}
	text := p.ContentChanges[len(p.ContentChanges)-1].Text
	s.setDoc(p.TextDocument.URI, text)
	s.publishDiagnostics(p.TextDocument.URI, text)
}

func (s *server) didClose(raw json.RawMessage) {
	var p didCloseParams
	if err := json.Unmarshal(raw, &p); err != nil {
		return
	}
	s.mu.Lock()
	delete(s.docs, p.TextDocument.URI)
	s.mu.Unlock()
	s.notify("textDocument/publishDiagnostics", map[string]any{
		"uri":         p.TextDocument.URI,
		"diagnostics": []any{},
	})
}

func (s *server) setDoc(uri, text string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.docs[uri] = text
}

func (s *server) getDoc(uri string) (string, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	t, ok := s.docs[uri]
	return t, ok
}

// LSP diagnostic shape.
type diagnostic struct {
	Range    rng    `json:"range"`
	Severity int    `json:"severity"` // 1=error, 2=warning, 3=info, 4=hint
	Source   string `json:"source"`
	Message  string `json:"message"`
}
type rng struct {
	Start position `json:"start"`
	End   position `json:"end"`
}
type position struct {
	Line      int `json:"line"`
	Character int `json:"character"`
}

var lineNumRE = regexp.MustCompile(`(?:line\s+|^)(\d+)(?::\d+)?(?:\s|:)`)

func (s *server) publishDiagnostics(uri, text string) {
	diags := []diagnostic{}

	prog, parseErr := capyloader.LoadFromString(text)
	if parseErr != nil {
		line := extractLine(parseErr.Error())
		diags = append(diags, diagnostic{
			Range:    fullLineRange(line),
			Severity: 1,
			Source:   "perch (parse)",
			Message:  parseErr.Error(),
		})
	}
	if prog != nil {
		// Map command name → first line where `^command NAME` appears, so
		// validator findings can land at the right place.
		commandLine := scanCommandLines(text)
		known := knownOps(s.handlers)
		for _, is := range validate.Check(prog, known) {
			ln := commandLineFromWhere(is.Where, commandLine)
			sev := 1
			if is.Severity == "warning" {
				sev = 2
			}
			diags = append(diags, diagnostic{
				Range:    fullLineRange(ln),
				Severity: sev,
				Source:   "perch (--check)",
				Message:  is.Message,
			})
		}
	}

	s.notify("textDocument/publishDiagnostics", map[string]any{
		"uri":         uri,
		"diagnostics": diags,
	})
}

// extractLine pulls a 0-indexed line number out of a perch/capy error
// string. Recognises two shapes:
//   - "line 4: unterminated string"   (capy lexer)
//   - "parse script: 11:1: …"        (capy parser; "LINE:COL")
func extractLine(s string) int {
	if m := regexp.MustCompile(`line\s+(\d+)`).FindStringSubmatch(s); len(m) >= 2 {
		if n, _ := strconv.Atoi(m[1]); n > 0 {
			return n - 1
		}
	}
	if m := regexp.MustCompile(`(\d+):\d+:`).FindStringSubmatch(s); len(m) >= 2 {
		if n, _ := strconv.Atoi(m[1]); n > 0 {
			return n - 1
		}
	}
	return 0
}

func fullLineRange(line int) rng {
	return rng{Start: position{Line: line, Character: 0}, End: position{Line: line + 1, Character: 0}}
}

var commandHeaderRE = regexp.MustCompile(`(?m)^\s*command\s+([A-Za-z_][A-Za-z0-9_]*)\s*$`)

func scanCommandLines(text string) map[string]int {
	out := map[string]int{}
	for i, line := range strings.Split(text, "\n") {
		if m := commandHeaderRE.FindStringSubmatch(line); m != nil {
			if _, exists := out[m[1]]; !exists {
				out[m[1]] = i
			}
		}
	}
	return out
}

// commandLineFromWhere derives the source line from a validator Issue's
// "where" string, which is the command name (or "command arg \"…\"").
func commandLineFromWhere(where string, commandLine map[string]int) int {
	name := where
	if idx := strings.Index(where, " "); idx >= 0 {
		name = where[:idx]
	}
	if ln, ok := commandLine[name]; ok {
		return ln
	}
	return 0
}

func knownOps(handlers map[string]interpreter.Handler) map[string]struct{} {
	out := make(map[string]struct{}, len(handlers))
	for k := range handlers {
		out[k] = struct{}{}
	}
	return out
}

// ────────────────────────────────────────────────────────────────────────
// Completion
// ────────────────────────────────────────────────────────────────────────

type completionParams struct {
	TextDocument struct{ URI string `json:"uri"` } `json:"textDocument"`
	Position     position                          `json:"position"`
}

func (s *server) completion(req *rpcMessage) {
	var p completionParams
	if err := json.Unmarshal(req.Params, &p); err != nil {
		s.respondErr(req.ID, -32602, err.Error())
		return
	}
	text, _ := s.getDoc(p.TextDocument.URI)
	scope := detectScope(text, p.Position.Line)
	items := completionsFor(text, scope)
	s.respond(req.ID, items)
}

// scope is a coarse "where in the file are we" classifier used to pick
// the right completion set.
type scope int

const (
	scopeTop scope = iota
	scopeGlobals
	scopeCommand    // inside `command NAME ... end`, NOT yet in `do`
	scopeCommandArg // inside `arg NAME ... end` within a command
	scopeDoBody     // inside `do ... end`
)

func detectScope(text string, line int) scope {
	lines := strings.Split(text, "\n")
	if line >= len(lines) {
		line = len(lines) - 1
	}
	// Walk top→down, tracking nesting of command/globals/arg/do.
	cur := scopeTop
	stack := []scope{}
	for i := 0; i <= line && i < len(lines); i++ {
		l := strings.TrimSpace(lines[i])
		switch {
		case l == "" || strings.HasPrefix(l, "#"):
			continue
		case strings.HasPrefix(l, "globals"):
			stack = append(stack, cur)
			cur = scopeGlobals
		case strings.HasPrefix(l, "command "):
			stack = append(stack, cur)
			cur = scopeCommand
		case strings.HasPrefix(l, "catch "):
			stack = append(stack, cur)
			cur = scopeCommand // catch behaves the same for our purposes
		case strings.HasPrefix(l, "arg ") && cur == scopeCommand:
			stack = append(stack, cur)
			cur = scopeCommandArg
		case strings.HasPrefix(l, "do"):
			stack = append(stack, cur)
			cur = scopeDoBody
		case l == "end":
			if n := len(stack); n > 0 {
				cur = stack[n-1]
				stack = stack[:n-1]
			}
		}
	}
	return cur
}

type completionItem struct {
	Label  string `json:"label"`
	Kind   int    `json:"kind"`  // 14 = Keyword, 3 = Function, 6 = Variable, 22 = Struct
	Detail string `json:"detail,omitempty"`
	Doc    string `json:"documentation,omitempty"`
}

func completionsFor(text string, s scope) []completionItem {
	switch s {
	case scopeTop:
		return staticItems([]kw{
			{"name", "Program name shown in --help."},
			{"about", "Top-level program description."},
			{"version", "Version string returned by --version."},
			{"globals", "Block of bindings shared by every command (`NAME = LITERAL`)."},
			{"command", "Declare a callable command. Opens a block terminated by `end`."},
			{"catch", "Fallback handler for unknown command names."},
		}, 14)

	case scopeGlobals:
		return []completionItem{{Label: "end", Kind: 14, Detail: "close globals block"}}

	case scopeCommand:
		return staticItems([]kw{
			{"description", "Help text shown in `--help`."},
			{"arg", "Declare a typed CLI argument. Opens a block terminated by `end`."},
			{"private", "Hide from CLI; callable only via `run`."},
			{"detached", "Don't wait on processes started by `shell_detached`."},
			{"proxy_args", "Skip arg parsing; argv → ${proxy_args}."},
			{"require_os", "Refuse to run on other OSes. Pass strings: \"darwin\" \"linux\" …"},
			{"require_arch", "Refuse to run on other archs."},
			{"dir", "Set the cwd for the body."},
			{"on_signal", "Run HANDLER on SIGINT/SIGTERM."},
			{"env", "Set an env var for the body's shell calls."},
			{"do", "Open the executable body block."},
			{"end", "Close the command block."},
		}, 14)

	case scopeCommandArg:
		return staticItems([]kw{
			{"type", "string / int / float / bool (required)."},
			{"default", "Default literal value; presence makes the arg optional."},
			{"description", "Help text shown in --help."},
			{"optional", "Mark optional even without a default."},
			{"index", "Bind to a positional index (instead of -name flag)."},
			{"end", "Close the arg block."},
		}, 14)

	case scopeDoBody:
		items := []completionItem{}
		// Built-in ops.
		for _, op := range builtinOps() {
			items = append(items, completionItem{Label: op.name, Kind: 3, Detail: op.detail, Doc: op.doc})
		}
		// Keywords valid in body.
		items = append(items, completionItem{Label: "end", Kind: 14, Detail: "close do block"})
		// `if` block opener — surfaces the unified `if EXPR ... end`.
		items = append(items, completionItem{Label: "if", Kind: 14, Detail: "if EXPR ... end — comparison / predicate / truthy / falsy"})
		// Command names from this file (for `run`).
		for _, n := range scanCommandNames(text) {
			items = append(items, completionItem{Label: n, Kind: 22, Detail: "command in this file"})
		}
		return items
	}
	return nil
}

type kw struct{ name, doc string }

func staticItems(in []kw, kind int) []completionItem {
	out := make([]completionItem, len(in))
	for i, k := range in {
		out[i] = completionItem{Label: k.name, Kind: kind, Detail: k.doc}
	}
	return out
}

func scanCommandNames(text string) []string {
	out := []string{}
	for _, m := range commandHeaderRE.FindAllStringSubmatch(text, -1) {
		out = append(out, m[1])
	}
	sort.Strings(out)
	return out
}

// ────────────────────────────────────────────────────────────────────────
// Hover
// ────────────────────────────────────────────────────────────────────────

type hoverParams struct {
	TextDocument struct{ URI string `json:"uri"` } `json:"textDocument"`
	Position     position                          `json:"position"`
}

var identUnderCursorRE = regexp.MustCompile(`[A-Za-z_][A-Za-z0-9_]*`)

func (s *server) hover(req *rpcMessage) {
	var p hoverParams
	if err := json.Unmarshal(req.Params, &p); err != nil {
		s.respondErr(req.ID, -32602, err.Error())
		return
	}
	text, _ := s.getDoc(p.TextDocument.URI)
	word := wordAt(text, p.Position)
	if word == "" {
		s.respond(req.ID, nil)
		return
	}
	doc := hoverDoc(word)
	if doc == "" {
		s.respond(req.ID, nil)
		return
	}
	s.respond(req.ID, map[string]any{
		"contents": map[string]any{
			"kind":  "markdown",
			"value": doc,
		},
	})
}

func wordAt(text string, pos position) string {
	lines := strings.Split(text, "\n")
	if pos.Line >= len(lines) {
		return ""
	}
	line := lines[pos.Line]
	if pos.Character > len(line) {
		pos.Character = len(line)
	}
	matches := identUnderCursorRE.FindAllStringIndex(line, -1)
	for _, m := range matches {
		if pos.Character >= m[0] && pos.Character <= m[1] {
			return line[m[0]:m[1]]
		}
	}
	return ""
}

func hoverDoc(word string) string {
	if op, ok := opDocs[word]; ok {
		return fmt.Sprintf("**op** `%s`\n\n%s\n\n%s", word, op.detail, op.doc)
	}
	if kw, ok := keywordDocs[word]; ok {
		return fmt.Sprintf("**keyword** `%s`\n\n%s", word, kw)
	}
	return ""
}

// ────────────────────────────────────────────────────────────────────────
// documentSymbol
// ────────────────────────────────────────────────────────────────────────

type docSymbolParams struct {
	TextDocument struct{ URI string `json:"uri"` } `json:"textDocument"`
}

type docSymbol struct {
	Name           string      `json:"name"`
	Detail         string      `json:"detail,omitempty"`
	Kind           int         `json:"kind"` // 12=Function, 13=Variable, 22=Struct
	Range          rng         `json:"range"`
	SelectionRange rng         `json:"selectionRange"`
	Children       []docSymbol `json:"children,omitempty"`
}

var argHeaderRE = regexp.MustCompile(`(?m)^\s*arg\s+([A-Za-z_][A-Za-z0-9_]*)\s*$`)

func (s *server) documentSymbol(req *rpcMessage) {
	var p docSymbolParams
	if err := json.Unmarshal(req.Params, &p); err != nil {
		s.respondErr(req.ID, -32602, err.Error())
		return
	}
	text, _ := s.getDoc(p.TextDocument.URI)
	lines := strings.Split(text, "\n")

	symbols := []docSymbol{}

	// Outer pass: find each `command NAME` and its terminating `end`.
	depth := 0
	for i, l := range lines {
		t := strings.TrimSpace(l)
		switch {
		case strings.HasPrefix(t, "command "):
			name := strings.TrimSpace(strings.TrimPrefix(t, "command"))
			endLine := findMatchingEnd(lines, i)
			cmd := docSymbol{
				Name:           name,
				Kind:           12,
				Range:          fullLineRange(i),
				SelectionRange: fullLineRange(i),
			}
			cmd.Range.End = position{Line: endLine + 1, Character: 0}
			// Children: each `arg NAME` block.
			for j := i + 1; j < endLine; j++ {
				if m := argHeaderRE.FindStringSubmatch(lines[j]); m != nil {
					argEnd := findMatchingEnd(lines, j)
					arg := docSymbol{
						Name:           m[1],
						Detail:         "arg",
						Kind:           13,
						Range:          fullLineRange(j),
						SelectionRange: fullLineRange(j),
					}
					arg.Range.End = position{Line: argEnd + 1, Character: 0}
					cmd.Children = append(cmd.Children, arg)
				}
			}
			symbols = append(symbols, cmd)
		case t == "end":
			if depth > 0 {
				depth--
			}
		}
	}
	s.respond(req.ID, symbols)
}

// findMatchingEnd returns the line index of the `end` matching the block
// opener on line `start`. Uses simple block counting; comments and strings
// are NOT tokenised so this is best-effort.
func findMatchingEnd(lines []string, start int) int {
	depth := 0
	openers := regexp.MustCompile(`^\s*(command|catch|globals|arg|do|if|for_each)\b`)
	for i := start; i < len(lines); i++ {
		t := strings.TrimSpace(lines[i])
		if openers.MatchString(t) {
			depth++
		}
		if t == "end" {
			depth--
			if depth == 0 {
				return i
			}
		}
	}
	return len(lines) - 1
}

// ────────────────────────────────────────────────────────────────────────
// Static doc tables
// ────────────────────────────────────────────────────────────────────────

type opDoc struct{ detail, doc string }

type builtinOp struct{ name, detail, doc string }

func builtinOps() []builtinOp {
	out := make([]builtinOp, 0, len(opDocs))
	names := make([]string, 0, len(opDocs))
	for n := range opDocs {
		names = append(names, n)
	}
	sort.Strings(names)
	for _, n := range names {
		out = append(out, builtinOp{name: n, detail: opDocs[n].detail, doc: opDocs[n].doc})
	}
	return out
}

var keywordDocs = map[string]string{
	"name":         "Top-level: program name. `name \"myapp\"`",
	"about":        "Top-level: program description. `about \"…\"`",
	"version":      "Top-level: version. `version \"0.1.0\"`",
	"globals":      "Top-level: open a block of `NAME = LITERAL` bindings shared by every command.",
	"command":      "Declare a callable command. Opens a block terminated by `end`.",
	"catch":        "Fallback for unknown command names. Body runs with the unknown name bound.",
	"description":  "Help text. Inside a `command` sets the command's; inside an `arg` block sets the arg's.",
	"arg":          "Declare a typed CLI argument. Inside: `type`, `default`, `description`, `optional`, `index`.",
	"type":         "Inside an arg block: `type string`/`int`/`float`/`bool`. Required.",
	"default":      "Inside an arg block: default literal; presence makes the arg optional.",
	"optional":     "Inside an arg block: arg may be omitted with no default.",
	"index":        "Inside an arg block: bind to a positional index (instead of a `-name` flag).",
	"private":      "Hide command from CLI; only callable via `run`.",
	"detached":     "Don't wait on processes started by `shell_detached`.",
	"proxy_args":   "Skip arg parsing; argv → `${proxy_args}` for the body.",
	"require_os":   "Refuse to run on other OSes. Pass: `\"darwin\" \"linux\" \"windows\"`.",
	"require_arch": "Refuse to run on other CPU architectures.",
	"dir":          "Set the cwd for the body.",
	"on_signal":    "Run HANDLER (another command) on SIGINT/SIGTERM.",
	"env":          "Set an env var for the body's `shell` calls.",
	"do":           "Open the executable body block.",
	"end":          "Close the most-recent block.",
}

// opDocs covers the catalogue. Keep in sync with infra/ops/.
var opDocs = map[string]opDoc{
	"print":          {"(string)", "Print MSG + newline to stdout."},
	"println":        {"(string)", "Alias for print."},
	"eprintln":       {"(string)", "Print MSG + newline to stderr."},
	"shell":          {"(string)", "Run CMD via bash (POSIX) or cmd.exe (Windows)."},
	"shell_output":   {"(string) → string", "Run CMD; capture stdout as the return value."},
	"shell_detached": {"(string)", "Start CMD detached; return immediately."},
	"fail":           {"(string)", "Exit the command with the given message."},
	"exit":           {"(int)", "Exit the process with the given code."},
	"sleep":          {"(number)", "Sleep for N seconds."},
	"run":            {"(ident)", "Call another command."},
	"list_commands":  {"()", "Print the visible command list."},

	"mkdir":      {"(string)", "Create directories (with parents)."},
	"cp":         {"(string, string)", "Copy src → dst."},
	"mv":         {"(string, string)", "Rename src → dst."},
	"rm":         {"(string)", "Remove path (recursive)."},
	"cd":         {"(string)", "Change the cwd of subsequent ops."},
	"chmod":      {"(string, string)", "chmod path MODE (octal)."},
	"touch":      {"(string)", "Touch a path."},
	"write_file": {"(string, string)", "Write CONTENT to PATH."},
	"read_file":  {"(string) → string", "Read PATH and return its contents."},
	"exists":     {"(string) → bool", "Return whether the path exists."},
	"is_dir":     {"(string) → bool", "Return whether the path is a directory."},
	"is_file":    {"(string) → bool", "Return whether the path is a regular file."},
	"file_size":  {"(string) → int", "Return size in bytes."},

	"if":      {"EXPR … end", "Run nested ops when EXPR holds. EXPR can be: `NAME == LIT` / `!= LIT` / `> N` / `< N` / `>= N` / `<= N` / bare `NAME` (truthy) / `not NAME` (falsy)."},
	"if_call": {"FUNC ARG … end", "Run nested ops if FUNC(ARG) is truthy. E.g. `if exists \"./bin\"`."},

	"upper":      {"(string) → string", "Uppercase. Use via `X = upper \"hi\"`."},
	"lower":      {"(string) → string", "Lowercase."},
	"trim":       {"(string) → string", "Strip surrounding whitespace."},
	"capitalize": {"(string) → string", "Title-case first letter."},
	"length":     {"(string) → int", "Length in bytes."},
	"contains":   {"(string, string) → bool", "Substring test."},
	"has_prefix": {"(string, string) → bool", "Prefix test."},
	"has_suffix": {"(string, string) → bool", "Suffix test."},
	"replace":    {"(string, string) → string", "Replace; second arg is \"OLD,NEW\"."},
	"split":      {"(string, string) → []string", "Split by SEP."},
	"join":       {"([]any, string) → string", "Join with SEP."},
	"repeat":     {"(string, int) → string", "Repeat N times."},
	"format":     {"(string, any) → string", "fmt.Sprintf-style."},

	"md5":         {"(string) → string", "MD5 hex digest."},
	"sha1":        {"(string) → string", "SHA1 hex digest."},
	"sha256":      {"(string) → string", "SHA256 hex digest."},
	"crc32":       {"(string) → string", "CRC32 IEEE hex digest."},
	"md5_file":    {"(string) → string", "MD5 of the file at PATH."},
	"sha1_file":   {"(string) → string", "SHA1 of the file at PATH."},
	"sha256_file": {"(string) → string", "SHA256 of the file at PATH."},

	"base64_encode": {"(string) → string", "Base64-encode."},
	"base64_decode": {"(string) → string", "Base64-decode."},
	"hex_encode":    {"(string) → string", "Hex-encode."},
	"hex_decode":    {"(string) → string", "Hex-decode."},
	"url_encode":    {"(string) → string", "URL-percent-encode."},
	"url_decode":    {"(string) → string", "URL-percent-decode."},
	"json_parse":    {"(string) → any", "Parse JSON to a value."},
	"json_stringify": {"(any) → string", "Marshal to JSON."},
	"json_get":      {"(any, string) → any", "Dotted-path lookup into a JSON value."},

	"http_get":    {"(string) → string", "HTTP GET; return body as string."},
	"http_post":   {"(string, string) → string", "HTTP POST; second arg is the body."},
	"http_put":    {"(string, string) → string", "HTTP PUT."},
	"http_delete": {"(string) → string", "HTTP DELETE."},
	"download":    {"(string, string)", "GET URL, save to DST."},

	"gzip":        {"(string, string)", "gzip SRC → DST."},
	"ungzip":      {"(string, string)", "ungzip SRC → DST."},
	"tar_create":  {"(string, string)", "Gzipped-tar SRC_DIR → DST."},
	"tar_extract": {"(string, string)", "Extract SRC.tar.gz → DST dir."},
	"zip_create":  {"(string, string)", "Zip SRC_DIR → DST."},
	"zip_extract": {"(string, string)", "Extract SRC.zip → DST dir."},

	"now":          {"(string?) → string", "Current time. Formats: rfc3339, rfc822, unix, unix_milli, date, time, datetime, or any Go layout."},
	"unix_to_iso":  {"(int) → string", "Unix seconds → RFC3339 UTC."},

	"regex_match":    {"(string, string) → bool", "Test regex match."},
	"regex_replace":  {"(string, string, string) → string", "Regex replace."},
	"regex_find_all": {"(string, string) → []string", "Find all matches."},

	"hostname":     {"() → string", "Host name."},
	"dns_lookup":   {"(string) → []string", "Resolve host → IPs."},
	"port_check":   {"(string, string) → bool", "Can we open TCP to host:port?"},
	"get_os":       {"() → string", "darwin/linux/windows."},
	"get_arch":     {"() → string", "amd64/arm64/…"},
	"get_env":      {"(string) → string", "os.Getenv(name)."},
	"set_env":      {"(string, string)", "os.Setenv(name, value)."},
	"cwd":          {"() → string", "Current working directory."},
	"home_dir":     {"() → string", "User home dir."},
	"temp_dir":     {"() → string", "OS temp dir."},
	"app_data_dir": {"() → string", "Platform-aware app-data dir."},
	"cache_dir":    {"() → string", "Platform-aware cache dir."},
	"pid":          {"() → int", "Current process id."},
	"user":         {"() → string", "$USER."},
}

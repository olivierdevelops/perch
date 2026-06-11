// Package httpserver hosts the loaded perch program behind an HTML UI
// and a small set of JSON / NDJSON endpoints.
//
// Endpoints:
//   GET  /                   the UI
//   GET  /api/program        Program metadata (name, version, commands, globals)
//   POST /api/exec           run a command, stream NDJSON (out/err/status)
//   POST /api/check          validate the program, return issues
//   POST /api/scan           static capability + risk audit
//   POST /api/simulate       simulate a command against a hypothetical env
//                            (CLI flags via SimEnv; optional fixture JSON body)
package httpserver

import (
	"bytes"
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"html/template"
	"log"
	"net/http"
	"sort"
	"strings"
	"sync"

	"github.com/olivierdevelops/perch/domain"
	"github.com/olivierdevelops/perch/infra/interpreter"
	"github.com/olivierdevelops/perch/infra/ops"
	"github.com/olivierdevelops/perch/usecases/scan"
	"github.com/olivierdevelops/perch/usecases/simulate"
	"github.com/olivierdevelops/perch/usecases/validate"
)

//go:embed index.html
var indexHTML []byte

// Server holds the wiring to serve a Program. The optional ConfigPath,
// KnownOps, and Loader fields enable the pre-flight endpoints (check,
// scan, simulate). They're optional — if KnownOps is nil, /api/check
// degrades to "no known-ops table available."
type Server struct {
	Handlers   map[string]interpreter.Handler
	ConfigPath string                              // path to the loaded .perch file
	KnownOps   func() map[string]struct{}         // for validate
}

// Serve listens on host:port and serves p. configPath is the path of
// the .perch file that produced p — used in the UI header and for
// context in pre-flight endpoints.
func (s *Server) Serve(p *domain.Program, host string, port int, configPath string) error {
	if configPath != "" {
		s.ConfigPath = configPath
	}
	mux := http.NewServeMux()

	tmpl, err := template.New("index").Parse(string(indexHTML))
	if err != nil {
		return err
	}
	cmds := visibleCommands(p)
	type tplData struct {
		Program  *domain.Program
		Commands []*domain.Command
		Path     string
	}
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		_ = tmpl.Execute(w, tplData{Program: p, Commands: cmds, Path: s.ConfigPath})
	})

	mux.HandleFunc("/api/program", func(w http.ResponseWriter, r *http.Request) {
		s.handleProgram(w, r, p)
	})
	mux.HandleFunc("/api/exec", func(w http.ResponseWriter, r *http.Request) {
		s.handleExec(w, r, p)
	})
	mux.HandleFunc("/api/check", func(w http.ResponseWriter, r *http.Request) {
		s.handleCheck(w, r, p)
	})
	mux.HandleFunc("/api/scan", func(w http.ResponseWriter, r *http.Request) {
		s.handleScan(w, r, p)
	})
	mux.HandleFunc("/api/simulate", func(w http.ResponseWriter, r *http.Request) {
		s.handleSimulate(w, r, p)
	})

	addr := fmt.Sprintf("%s:%d", host, port)
	log.Printf("perch UI on http://%s", addr)
	return http.ListenAndServe(addr, mux)
}

// ── /api/program ──────────────────────────────────────────────────────

func (s *Server) handleProgram(w http.ResponseWriter, r *http.Request, p *domain.Program) {
	type argView struct {
		Name        string `json:"name"`
		Type        string `json:"type"`
		Description string `json:"description,omitempty"`
		Default     any    `json:"default,omitempty"`
		HasDefault  bool   `json:"has_default,omitempty"`
		Optional    bool   `json:"optional,omitempty"`
		Index       *int   `json:"index,omitempty"`
		Rest        bool   `json:"rest,omitempty"`
	}
	type cmdView struct {
		Name        string    `json:"name"`
		Description string    `json:"description,omitempty"`
		Args        []argView `json:"args,omitempty"`
		IsTest      bool      `json:"is_test,omitempty"`
		Detached    bool      `json:"detached,omitempty"`
		ProxyArgs   bool      `json:"proxy_args,omitempty"`
	}
	conv := func(a domain.ArgSpec) argView {
		return argView{
			Name: a.Name, Type: a.Type, Description: a.Description,
			Default: a.Default, HasDefault: a.HasDefault, Optional: a.Optional,
			Index: a.Index, Rest: a.Rest,
		}
	}
	out := map[string]any{
		"name":        p.Name,
		"description": p.Description,
		"version":     p.Version,
		"path":        s.ConfigPath,
	}
	cmds := make([]cmdView, 0, len(p.Commands))
	for _, c := range visibleCommands(p) {
		v := cmdView{
			Name: c.Name, Description: c.Description,
			IsTest:   c.Modifiers.Test,
			Detached: c.Modifiers.Detached, ProxyArgs: c.Modifiers.ProxyArgs,
		}
		for _, a := range c.Args {
			v.Args = append(v.Args, conv(a))
		}
		cmds = append(cmds, v)
	}
	out["commands"] = cmds

	out["globals"] = p.Globals.Bindings
	if p.Catch != nil {
		out["catch"] = map[string]any{
			"bind":        p.Catch.Bind,
			"description": p.Catch.Description,
		}
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(out)
}

// ── /api/exec (NDJSON stream) ────────────────────────────────────────

type execRequest struct {
	Command string         `json:"command"`
	Args    map[string]any `json:"args"`
	// Per-request capability overrides (TODO: wire these to a fresh
	// handler set + CapMask; today the server inherits whatever
	// restrictions the operator launched `perch --server` with).
	EnvOnly   []string `json:"env_only,omitempty"`
	AllowBin  []string `json:"allow_bin,omitempty"`
	AllowHost []string `json:"allow_host,omitempty"`
}

func (s *Server) handleExec(w http.ResponseWriter, r *http.Request, p *domain.Program) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}
	var req execRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	cmd, ok := p.Commands[req.Command]
	if !ok || cmd.Modifiers.Private {
		http.Error(w, "command not found", http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "application/x-ndjson")
	w.Header().Set("Cache-Control", "no-cache")
	flusher, _ := w.(http.Flusher)

	mu := &sync.Mutex{}
	emit := func(kind, msg string) {
		mu.Lock()
		defer mu.Unlock()
		_ = json.NewEncoder(w).Encode(map[string]any{"kind": kind, "msg": strings.TrimRight(msg, "\n")})
		if flusher != nil {
			flusher.Flush()
		}
	}

	stdout := writerFunc(func(b []byte) (int, error) { emit("out", string(b)); return len(b), nil })
	stderr := writerFunc(func(b []byte) (int, error) { emit("err", string(b)); return len(b), nil })

	i := interpreter.New(s.Handlers, p)
	i.PreflightHook = ops.Preflight
	i.HookCategory = ops.HookCategoryOf
	i.Stdout = stdout
	i.Stderr = stderr
	if len(req.AllowBin) > 0 {
		allow := map[string]bool{}
		for _, b := range req.AllowBin {
			allow[b] = true
		}
		i.AllowedShellBins = allow
	}
	if len(req.AllowHost) > 0 {
		i.HTTPPolicy = &interpreter.HTTPPolicy{AllowedHosts: req.AllowHost, MaxRedirects: 5}
	}
	if len(req.EnvOnly) > 0 {
		allow := map[string]bool{}
		for _, e := range req.EnvOnly {
			allow[e] = true
		}
		i.EnvAllowlist = allow
	}

	// Turn the named-args map into a flat -k=v argv.
	argv := []string{}
	for k, v := range req.Args {
		argv = append(argv, fmt.Sprintf("-%s=%v", k, v))
	}

	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()
	_ = ctx // reserved for future cancel-token wiring

	emit("status", "started")
	if err := i.Run(req.Command, argv); err != nil {
		emit("err", err.Error())
		emit("status", "error")
		return
	}
	emit("status", "ok")
}

// ── /api/check ────────────────────────────────────────────────────────

func (s *Server) handleCheck(w http.ResponseWriter, r *http.Request, p *domain.Program) {
	known := map[string]struct{}{}
	if s.KnownOps != nil {
		known = s.KnownOps()
	}
	issues := validate.Check(p, known)
	type out struct {
		Severity string `json:"severity"`
		Where    string `json:"where,omitempty"`
		Message  string `json:"message"`
	}
	res := make([]out, 0, len(issues))
	errors, warnings := 0, 0
	for _, iss := range issues {
		res = append(res, out{Severity: iss.Severity, Where: iss.Where, Message: iss.Message})
		switch iss.Severity {
		case "error":
			errors++
		case "warning":
			warnings++
		}
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{
		"ok":       errors == 0,
		"errors":   errors,
		"warnings": warnings,
		"issues":   res,
	})
}

// ── /api/scan ─────────────────────────────────────────────────────────

func (s *Server) handleScan(w http.ResponseWriter, r *http.Request, p *domain.Program) {
	rep := scan.Analyze(p)
	var buf bytes.Buffer
	scan.PrintReport(&buf, p, s.ConfigPath, rep)
	recommended := scan.RecommendedInvocation(s.ConfigPath, rep)
	score, reasons := scan.ScoreReport(rep)
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{
		"report":      buf.String(),
		"risk": map[string]any{
			"score":   score.String(),
			"reasons": reasons,
		},
		"capabilities": map[string]any{
			"shell":          rep.NeedsShell,
			"shell_bins":     rep.ShellBins,
			"subprocess":     rep.NeedsSubprocess,
			"subprocess_ops": rep.SubprocessOps,
			"network":        rep.Hosts,
			"writes":         rep.WriteRoots,
			"envs":           rep.EnvVars,
			"sudo":           rep.HasShellSudo,
			"shell_pipe":     rep.HasShellPipe,
			"proxy_args":     rep.HasProxyArgs,
			"catch_forwards": rep.CatchForwards,
		},
		"findings":    rep.Findings,
		"recommended": recommended,
	})
}

// ── /api/simulate ─────────────────────────────────────────────────────

func (s *Server) handleSimulate(w http.ResponseWriter, r *http.Request, p *domain.Program) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST only", http.StatusMethodNotAllowed)
		return
	}
	type simReq struct {
		Command   string           `json:"command"`
		Env       simulate.SimEnv  `json:"env"`
		Fixture   *simulate.Fixture `json:"fixture,omitempty"` // inline fixture (overrides file)
	}
	var req simReq
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	var buf bytes.Buffer
	if req.Fixture != nil {
		// Inline fixture path: write it to a temp file path and re-use Impl,
		// OR call SimulateWithState directly per scenario. Direct is cleaner.
		fix := *req.Fixture
		merged := mergeFixture(fix, req.Env)
		scenarios := fix.Scenarios
		if len(scenarios) == 0 {
			scenarios = []simulate.Scenario_{{Name: "default"}}
		}
		var failures int
		for idx, sc := range scenarios {
			if idx > 0 {
				fmt.Fprintln(&buf)
			}
			fmt.Fprintf(&buf, "═══ Scenario: %s ═══\n", sc.Name)
			oracles := simulate.MergeOracles(fix.Oracles, sc.Overrides)
			names := []string{req.Command}
			if req.Command == "" {
				names = commandNamesOf(p)
			}
			for j, name := range names {
				if j > 0 {
					fmt.Fprintln(&buf)
				}
				res, _ := simulate.SimulateWithState(p, name, merged, oracles)
				simulate.RenderResult(&buf, res, p, name)
				failures += res.WillFail
			}
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"ok":     failures == 0,
			"report": buf.String(),
		})
		return
	}
	// No fixture — use the static walker via simulate.Impl with the loaded program.
	// We have p in memory; call simulateCommand internally via the public RenderResult path.
	names := []string{req.Command}
	if req.Command == "" {
		names = commandNamesOf(p)
	}
	var failures int
	for idx, name := range names {
		if idx > 0 {
			fmt.Fprintln(&buf)
		}
		res := simulate.SimulateCommand(p, name, req.Env)
		simulate.RenderResult(&buf, res, p, name)
		failures += res.WillFail
	}
	_ = failures
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{
		"ok":     true, // single-walk path returns the report; caller inspects WillFail in the rendered text
		"report": buf.String(),
	})
}

// mergeFixture mirrors simulate.mergeFixtureIntoEnv but lives here since
// that helper is unexported. We don't compete with CLI flags in the UI
// (the UI provides one env source), so we just layer fixture over env.
func mergeFixture(f simulate.Fixture, cli simulate.SimEnv) simulate.SimEnv {
	fxEnv := f.ToSimEnv()
	out := cli
	if out.OS == "" {
		out.OS = fxEnv.OS
	}
	if out.Arch == "" {
		out.Arch = fxEnv.Arch
	}
	if out.Env == nil {
		out.Env = fxEnv.Env
	}
	if !out.EnvRestrict {
		out.EnvRestrict = fxEnv.EnvRestrict
	}
	if out.FsRead == nil {
		out.FsRead = fxEnv.FsRead
	}
	if out.FsWrite == nil {
		out.FsWrite = fxEnv.FsWrite
	}
	if out.Bins == nil {
		out.Bins = fxEnv.Bins
	}
	if out.Network == nil {
		out.Network = fxEnv.Network
	}
	out.NoShell = out.NoShell || fxEnv.NoShell
	out.NoSubprocess = out.NoSubprocess || fxEnv.NoSubprocess
	out.NoNetwork = out.NoNetwork || fxEnv.NoNetwork
	out.NoWrite = out.NoWrite || fxEnv.NoWrite
	return out
}

func commandNamesOf(p *domain.Program) []string {
	out := make([]string, 0, len(p.Commands))
	for n, c := range p.Commands {
		if c.Modifiers.Private {
			continue
		}
		out = append(out, n)
	}
	sort.Strings(out)
	return out
}

func visibleCommands(p *domain.Program) []*domain.Command {
	out := make([]*domain.Command, 0, len(p.Commands))
	for _, c := range p.Commands {
		if c.Modifiers.Private {
			continue
		}
		out = append(out, c)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}

type writerFunc func([]byte) (int, error)

func (f writerFunc) Write(b []byte) (int, error) { return f(b) }

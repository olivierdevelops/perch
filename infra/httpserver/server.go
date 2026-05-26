// Package httpserver hosts the loaded perch program behind a small HTML
// UI and an NDJSON-streamed exec endpoint.
package httpserver

import (
	_ "embed"
	"encoding/json"
	"fmt"
	"html/template"
	"log"
	"net/http"
	"sort"
	"strings"
	"sync"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

//go:embed index.html
var indexHTML []byte

// Server holds the wiring to serve a Program.
type Server struct {
	Handlers map[string]interpreter.Handler
}

// Serve listens on host:port and serves p.
func (s *Server) Serve(p *domain.Program, host string, port int) error {
	mux := http.NewServeMux()

	// Pre-render the HTML once.
	tmpl, err := template.New("index").Parse(string(indexHTML))
	if err != nil {
		return err
	}
	cmds := visibleCommands(p)
	type tplData struct {
		Program  *domain.Program
		Commands []*domain.Command
	}
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		_ = tmpl.Execute(w, tplData{Program: p, Commands: cmds})
	})

	mux.HandleFunc("/api/exec", func(w http.ResponseWriter, r *http.Request) {
		s.handleExec(w, r, p)
	})

	addr := fmt.Sprintf("%s:%d", host, port)
	log.Printf("perch UI on http://%s", addr)
	return http.ListenAndServe(addr, mux)
}

type execRequest struct {
	Command string         `json:"command"`
	Args    map[string]any `json:"args"`
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
	i.Stdout = stdout
	i.Stderr = stderr

	// Turn the named-args map into a flat -k=v argv.
	argv := []string{}
	for k, v := range req.Args {
		argv = append(argv, fmt.Sprintf("-%s=%v", k, v))
	}

	if err := i.Run(req.Command, argv); err != nil {
		emit("err", err.Error())
		emit("status", "error")
		return
	}
	emit("status", "ok")
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

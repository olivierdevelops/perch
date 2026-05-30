// Package audit produces a structured NDJSON trace of every op the
// interpreter dispatches. One line per op call, plus a session-start
// and session-end record. Designed for two consumers:
//
//   1. Security review — every action a `.perch` file took, in order,
//      with timestamps, durations, args, errors. Same shape as Linux
//      auditd but at the perch-op level rather than the syscall level.
//
//   2. AI-agent supervision — when a `perch-mcp` server runs an agent's
//      requests, the audit stream is what your monitoring stack sees.
//      Pipe it into Loki / Datadog / CloudWatch / whatever.
//
// Each event is one self-contained JSON object on its own line:
//
//   {"event":"session_start","ts":"2024-…","cmd":"deploy","args":["-target=prod"]}
//   {"event":"op","ts":"…","cmd":"deploy","kind":"shell","args":{"_0":"docker …"},"dur_ms":1842,"ok":true}
//   {"event":"op","ts":"…","cmd":"deploy","kind":"write_file","args":{...},"dur_ms":3,"ok":false,"error":"op disabled by --no-write"}
//   {"event":"session_end","ts":"…","cmd":"deploy","dur_ms":2104,"ok":false,"exit":1}
//
// File is opened with O_APPEND so multiple invocations append to the
// same log. JSON-encoded so a downstream tool can grep / jq / ingest.
package audit

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sync"
	"time"

	"github.com/olivierdevelops/perch/domain"
	"github.com/olivierdevelops/perch/infra/interpreter"
)

// Sink is the destination for audit events. Construct via Open() and
// pass into the interpreter via WireInto(). Concurrent-safe (a single
// mutex serialises writes — Go's encoding/json isn't reentrant, and
// we want one line per record regardless of caller goroutines).
type Sink struct {
	mu  sync.Mutex
	w   io.Writer
	cmd string
}

// Open opens (or creates+appends to) the named file for audit output.
// "-" means stdout. The caller is responsible for closing the file (or
// just letting the process exit).
func Open(path string) (*Sink, io.Closer, error) {
	if path == "-" {
		return &Sink{w: os.Stdout}, io.NopCloser(os.Stdout), nil
	}
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return nil, nil, fmt.Errorf("audit: open %s: %w", path, err)
	}
	return &Sink{w: f}, f, nil
}

// WireInto attaches the sink to an interpreter, returning a finalise()
// closure to call when the command finishes. Records:
//
//   - one "session_start" event up front (with cmd + cliArgs)
//   - one "op" event per dispatched op (with kind, args, duration, error)
//   - one "session_end" event at finish (with total duration + error)
func (s *Sink) WireInto(i *interpreter.Interpreter, cmdName string, cliArgs []string) func(err error) {
	s.cmd = cmdName
	start := time.Now()
	s.emit(map[string]any{
		"event":    "session_start",
		"ts":       start.UTC().Format(time.RFC3339Nano),
		"cmd":      cmdName,
		"cli_args": cliArgs,
	})
	i.AfterOp = func(op domain.Op, args map[string]any, b *interpreter.Bindings, val any, err error, dur time.Duration) {
		rec := map[string]any{
			"event":  "op",
			"ts":     time.Now().UTC().Format(time.RFC3339Nano),
			"cmd":    cmdName,
			"kind":   op.Kind,
			"args":   sanitiseArgs(args),
			"dur_ms": dur.Milliseconds(),
			"ok":     err == nil,
		}
		if op.CaptureInto != "" {
			rec["capture"] = op.CaptureInto
		}
		if err != nil {
			rec["error"] = err.Error()
		}
		s.emit(rec)
	}
	return func(finalErr error) {
		s.emit(map[string]any{
			"event":  "session_end",
			"ts":     time.Now().UTC().Format(time.RFC3339Nano),
			"cmd":    cmdName,
			"dur_ms": time.Since(start).Milliseconds(),
			"ok":     finalErr == nil,
			"error":  errString(finalErr),
		})
	}
}

func (s *Sink) emit(rec map[string]any) {
	s.mu.Lock()
	defer s.mu.Unlock()
	enc := json.NewEncoder(s.w)
	enc.SetEscapeHTML(false)
	_ = enc.Encode(rec)
}

// sanitiseArgs drops the _body sentinel (block-op body slice; not JSON-
// serialisable as-is and not interesting to log — the body's ops show
// up as their own audit events). Other args pass through verbatim.
func sanitiseArgs(args map[string]any) map[string]any {
	if _, has := args["_body"]; !has {
		return args
	}
	out := make(map[string]any, len(args))
	for k, v := range args {
		if k == "_body" {
			continue
		}
		out[k] = v
	}
	return out
}

func errString(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}

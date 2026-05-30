package report

// LiveTracer is the human-readable real-time counterpart to the audit
// NDJSON stream and the after-run span report. It implements
// interpreter.Tracer and prints each op to a writer the moment it
// starts (Before) and again when it finishes (After), interleaved
// with the op's own stdout/stderr.
//
// Wire via `--trace` from the CLI. Composes with --audit and --report:
// all three write from the same hook order, just to different sinks
// with different shapes.
//
// Indentation reflects nesting — a `parallel` block prints, then each
// child indents one level under it, then the block's After prints at
// the parent level. Lets you eyeball "we're now inside the third
// branch of this retry" without ceremony.

import (
	"fmt"
	"io"
	"strings"
	"sync"
	"time"

	"github.com/olivierdevelops/perch/domain"
	"github.com/olivierdevelops/perch/infra/interpreter"
)

// NewLiveTracer constructs a Tracer that prints to w as ops fire. The
// stream looks like:
//
//	▸ shell                  "docker ps -q -f name=…"
//	▸ if                     lhs=running op=truthy
//	  ▸ print                msg="✓ already running"
//	  ✓                                                       0µs
//	✓                                                         12ms
//	▸ shell                  "docker run -d --name perch-redis …"
//	✓                                                         842ms
//
// Errors render `✗` and include the error message on the same line.
func NewLiveTracer(w io.Writer) *LiveTracer {
	return &LiveTracer{w: w}
}

// LiveTracer is the concrete Tracer implementation.
type LiveTracer struct {
	mu    sync.Mutex
	w     io.Writer
	depth int
}

// Before prints the op header.
func (t *LiveTracer) Before(op domain.Op, args map[string]any) {
	t.mu.Lock()
	defer t.mu.Unlock()
	indent := strings.Repeat("  ", t.depth)
	fmt.Fprintf(t.w, "%s▸ %-20s %s\n", indent, op.Kind, argPreview(shallowSanitize(args)))
	t.depth++
}

// After prints the outcome with the op's wall-clock duration.
func (t *LiveTracer) After(op domain.Op, _ any, err error, dur time.Duration) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.depth > 0 {
		t.depth--
	}
	indent := strings.Repeat("  ", t.depth)
	if err != nil {
		fmt.Fprintf(t.w, "%s✗  %s   (%s)\n", indent, err.Error(), formatDur(dur))
		return
	}
	// Skip the "✓" line for cheap ops that took <1µs — keeps the trace
	// readable on programs with hundreds of trivial ops. Block ops and
	// anything taking real time still print.
	if dur < time.Microsecond && len(op.Body) == 0 {
		return
	}
	fmt.Fprintf(t.w, "%s✓%s(%s)\n", indent, strings.Repeat(" ", 32-len(indent)*2), formatDur(dur))
}

// compile-time check
var _ interpreter.Tracer = (*LiveTracer)(nil)

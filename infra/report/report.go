// Package report builds a span-shaped trace of an interpreter run and
// renders it as a tree. Block ops (`parallel`, `retry`, `timeout`,
// `sandbox`, `cache`, `with_env`, `with_cwd`, `if`, `for_each`) naturally
// nest the children that ran inside their body. A flat audit stream
// (infra/audit) is the canonical artifact; this is a human renderer
// derived from the same hook order.
//
// Wire via interpreter.Tracer:
//
//	rec := report.NewRecorder()
//	itp.Tracer = rec
//	defer rec.Render(os.Stderr)  // print tree once the command returns
package report

import (
	"fmt"
	"io"
	"sort"
	"strings"
	"time"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

// Recorder builds the span tree as ops execute. NOT thread-safe — the
// `parallel` block-op spawns goroutines, so each goroutine pushes onto
// a fork of the tracer; the parent merges the forks back when it
// reaches its After. For now we accept that `parallel` children appear
// linearised in the rendered tree (preserving order of `.After()`
// arrival, which mirrors their wall-clock completion order).
type Recorder struct {
	root *Node
	cur  *Node
}

// Node is one entry in the span tree. Each op produces one Node;
// block-op nodes also carry their children.
type Node struct {
	Kind     string
	Args     map[string]any
	Children []*Node
	Parent   *Node
	Start    time.Time
	Dur      time.Duration
	OK       bool
	Error    string
	// ExpandedFrom carries through from domain.Op so template-expanded
	// children render as `print (from check_bin)` in the tree.
	ExpandedFrom string
	Capture      string
}

// NewRecorder produces a fresh recorder whose root is a synthetic
// "command" node. The command name + initial CLI args can be set via
// SetRoot before Before fires for the first real op.
func NewRecorder() *Recorder {
	r := &Recorder{}
	r.root = &Node{Kind: "command", Start: time.Now()}
	r.cur = r.root
	return r
}

// SetRoot labels the root span with a command name. Optional.
func (r *Recorder) SetRoot(cmdName string) {
	r.root.Kind = cmdName
}

// Before is the interpreter.Tracer hook. Pushes a new child onto the
// current node and makes it current.
func (r *Recorder) Before(op domain.Op, args map[string]any) {
	n := &Node{
		Kind:         op.Kind,
		Args:         shallowSanitize(args),
		Parent:       r.cur,
		Start:        time.Now(),
		ExpandedFrom: op.ExpandedFrom,
		Capture:      op.CaptureInto,
	}
	r.cur.Children = append(r.cur.Children, n)
	r.cur = n
}

// After records the outcome and pops back to the parent.
func (r *Recorder) After(op domain.Op, result any, err error, dur time.Duration) {
	r.cur.Dur = dur
	r.cur.OK = err == nil
	if err != nil {
		r.cur.Error = err.Error()
	}
	if r.cur.Parent != nil {
		r.cur = r.cur.Parent
	}
}

// Finish closes out the synthetic root span. Call once after the
// command returns so the rendered tree shows the total wall-clock.
func (r *Recorder) Finish(err error) {
	r.root.Dur = time.Since(r.root.Start)
	r.root.OK = err == nil
	if err != nil {
		r.root.Error = err.Error()
	}
}

// Render writes the tree to w. Output is a fixed-pitch ASCII tree with
// per-node duration and (on failure) the error message; siblings are
// printed in the order they completed.
func (r *Recorder) Render(w io.Writer) {
	fmt.Fprintln(w, "── perch trace ─────────────────────────────────")
	renderNode(w, r.root, "", true, true)
}

// renderNode writes one node and recurses into its children.
//
//	prefix    — the rope of vertical bars / spaces drawn at this depth
//	isLast    — whether THIS node is the last among its parent's children
//	isRoot    — root has no connector glyph
func renderNode(w io.Writer, n *Node, prefix string, isLast, isRoot bool) {
	connector := "├─ "
	if isLast {
		connector = "└─ "
	}
	header := nodeHeader(n)
	if isRoot {
		fmt.Fprintf(w, "%s\n", header)
	} else {
		fmt.Fprintf(w, "%s%s%s\n", prefix, connector, header)
	}
	if n.Error != "" {
		errPrefix := prefix
		if !isRoot {
			if isLast {
				errPrefix += "   "
			} else {
				errPrefix += "│  "
			}
		}
		fmt.Fprintf(w, "%s   ↳ error: %s\n", errPrefix, n.Error)
	}
	childPrefix := prefix
	if !isRoot {
		if isLast {
			childPrefix += "   "
		} else {
			childPrefix += "│  "
		}
	}
	for i, c := range n.Children {
		renderNode(w, c, childPrefix, i == len(n.Children)-1, false)
	}
}

// nodeHeader renders the one-line span summary: status glyph, kind, key
// arg, duration, optional template provenance.
func nodeHeader(n *Node) string {
	status := "✓"
	if !n.OK {
		status = "✗"
	}
	if n.Dur == 0 && len(n.Children) == 0 && n.Kind == "command" {
		status = "·"
	}
	summary := strings.TrimSpace(n.Kind + " " + argPreview(n.Args))
	dur := formatDur(n.Dur)
	parts := []string{status, summary}
	if n.Capture != "" {
		parts = append(parts, "→"+n.Capture)
	}
	parts = append(parts, "("+dur+")")
	if n.ExpandedFrom != "" {
		parts = append(parts, "[from template "+n.ExpandedFrom+"]")
	}
	return strings.Join(parts, " ")
}

// argPreview returns a short one-line summary of args, prioritising
// common keys (msg, cmd, path, key, duration, name) so the tree stays
// scannable for the common ops without dumping every JSON field.
func argPreview(args map[string]any) string {
	if len(args) == 0 {
		return ""
	}
	priority := []string{"msg", "cmd", "path", "_0", "key", "duration", "name", "flags"}
	for _, k := range priority {
		if v, ok := args[k]; ok {
			s := fmt.Sprintf("%v", v)
			return "\"" + truncate(s, 60) + "\""
		}
	}
	// Fall back: stable-ordered key=value list, capped.
	keys := make([]string, 0, len(args))
	for k := range args {
		if strings.HasPrefix(k, "_") {
			continue
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)
	if len(keys) == 0 {
		return ""
	}
	parts := []string{}
	for _, k := range keys {
		parts = append(parts, fmt.Sprintf("%s=%v", k, args[k]))
		if len(parts) >= 2 {
			break
		}
	}
	return strings.Join(parts, " ")
}

func truncate(s string, max int) string {
	s = strings.ReplaceAll(s, "\n", " ")
	if len(s) <= max {
		return s
	}
	return s[:max-3] + "..."
}

// formatDur renders a duration to one of: 12ms, 4.21s, 1m02s.
func formatDur(d time.Duration) string {
	switch {
	case d < time.Millisecond:
		return fmt.Sprintf("%dµs", d.Microseconds())
	case d < time.Second:
		return fmt.Sprintf("%dms", d.Milliseconds())
	case d < time.Minute:
		return fmt.Sprintf("%.2fs", d.Seconds())
	default:
		return fmt.Sprintf("%dm%02ds", int(d.Minutes()), int(d.Seconds())%60)
	}
}

// shallowSanitize drops the _body sentinel; mirrors infra/audit's helper.
// The body's ops show up as their own children in the tree, so the
// duplicate slice in args adds nothing.
func shallowSanitize(args map[string]any) map[string]any {
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

// compile-time check
var _ interpreter.Tracer = (*Recorder)(nil)

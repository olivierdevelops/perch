// Package preview provides interpreter BeforeOp hooks that let users
// inspect a command's ops before they run.
//
// Two modes:
//
//   - DryRun: walk the ops, print each one with its interpolated args,
//     skip the actual handler. Captures get set to "" so subsequent
//     ${x} interpolation still works. Side-effect-free preview.
//
//   - Ask: like DryRun but interactive — for each op, prompt
//     y/n/a/q (yes / no / all / quit). 'y' runs THIS op, 'n' skips,
//     'a' runs everything else without further asking, 'q' stops.
//
// The hook gets the interpolated args, so what the user sees is what
// the handler would actually receive — no surprises.
package preview

import (
	"bufio"
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

// DryRunHook returns a BeforeOp that prints every op and returns ActSkip,
// so nothing actually executes. Use with --dry-run.
func DryRunHook(out io.Writer) interpreter.BeforeOp {
	step := 0
	return func(op domain.Op, args map[string]any, b *interpreter.Bindings) interpreter.OpAction {
		step++
		fmt.Fprintf(out, "  [%d] %s\n", step, formatOp(op, args))
		return interpreter.ActSkip
	}
}

// AskHook returns a BeforeOp that prints each op and prompts y/n/a/q.
// Reads lines from `in`, writes prompts to `out`.
func AskHook(in io.Reader, out io.Writer) interpreter.BeforeOp {
	reader := bufio.NewReader(in)
	step := 0
	return func(op domain.Op, args map[string]any, b *interpreter.Bindings) interpreter.OpAction {
		step++
		fmt.Fprintf(out, "  [%d] %s\n", step, formatOp(op, args))
		for {
			fmt.Fprintf(out, "       run? [y/n/a/q] > ")
			line, err := reader.ReadString('\n')
			if err != nil {
				return interpreter.ActQuit
			}
			switch strings.ToLower(strings.TrimSpace(line)) {
			case "", "y", "yes":
				return interpreter.ActRun
			case "n", "no", "skip":
				fmt.Fprintln(out, "       (skipped)")
				return interpreter.ActSkip
			case "a", "all":
				fmt.Fprintln(out, "       (running all remaining)")
				return interpreter.ActRunAll
			case "q", "quit":
				return interpreter.ActQuit
			default:
				fmt.Fprintln(out, "       y = run, n = skip, a = run all remaining, q = quit")
			}
		}
	}
}

// formatOp renders one op + args in a readable single-line form. Args
// are sorted for deterministic output; bodies are summarised by length.
// The positional helpers (_0, _1, …) render as "X" (without the key).
func formatOp(op domain.Op, args map[string]any) string {
	parts := []string{}
	keys := make([]string, 0, len(args))
	for k := range args {
		if k == "_body" {
			continue
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		v := interpreter.ToStringValue(args[k])
		if v == "" {
			continue
		}
		// Trim very long values to keep the preview readable.
		if len(v) > 80 {
			v = v[:77] + "…"
		}
		if strings.HasPrefix(k, "_") {
			parts = append(parts, fmt.Sprintf("%q", v))
		} else {
			parts = append(parts, fmt.Sprintf("%s=%q", k, v))
		}
	}
	suffix := ""
	if op.CaptureInto != "" {
		suffix = "   → ${" + op.CaptureInto + "}"
	}
	if n := len(op.Body); n > 0 {
		suffix += fmt.Sprintf("   {%d body op%s}", n, plural(n))
	}
	return fmt.Sprintf("%s %s%s", op.Kind, strings.Join(parts, " "), suffix)
}

func plural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}

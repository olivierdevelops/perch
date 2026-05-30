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

	"github.com/olivierdevelops/perch/domain"
	"github.com/olivierdevelops/perch/infra/interpreter"
)

// DryRunHook returns a BeforeOp that prints every op and returns ActSkip,
// so nothing actually executes. Use with --dry-run.
//
// Block ops (`if`, `parallel`, `retry`, `cache`, `sandbox`, …) have their
// bodies expanded inline as an indented sub-tree so the user sees EVERY
// op that could fire, not just the block headers. The displayed args are
// the already-interpolated values for THIS op; nested ops' args are
// shown verbatim (their interpolation hasn't run yet since the body
// wasn't dispatched).
func DryRunHook(out io.Writer) interpreter.BeforeOp {
	step := 0
	return func(op domain.Op, args map[string]any, b *interpreter.Bindings) interpreter.OpAction {
		step++
		fmt.Fprintf(out, "  [%d] %s\n", step, formatOp(op, args))
		if op.IsBlock() && len(op.Body) > 0 {
			for _, child := range op.Body {
				printTree(out, child, "        ")
			}
		}
		return interpreter.ActSkip
	}
}

// printTree renders one op + its (possibly nested) body without
// interpolation. Used by --dry-run to expand block bodies that the
// interpreter would otherwise skip past.
func printTree(out io.Writer, op domain.Op, indent string) {
	fmt.Fprintf(out, "%s%s\n", indent, formatOpRaw(op))
	if len(op.Body) > 0 {
		for _, child := range op.Body {
			printTree(out, child, indent+"   ")
		}
	}
}

// formatOpRaw is formatOp without the step number prefix, for nested
// body printing where steps don't apply (the body might never fire).
func formatOpRaw(op domain.Op) string {
	return formatOp(op, op.Args)
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

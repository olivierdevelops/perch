package ops

// Line-oriented text ops — the perch-native replacements for the middle
// stages of a shell pipeline (grep / sed / awk / head / tail / sort / uniq
// / wc -l). See docs/sandboxed-by-design.md §3.5.
//
// Every op here is PURE: it transforms an in-memory string and touches
// nothing external, so none is gated by `requires`. They are designed to
// compose with captured `exec` output:
//
//	let log   = exec git "log" "--oneline"
//	let fixes = grep "fix" "${log}"
//	let first = head "5" "${fixes}"
//
// Invocation is the generic `let NAME = OP arg [arg]` grammar (the same
// path every pure op uses); the text is passed as the LAST argument so it
// reads like "grep PATTERN in TEXT".

import (
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/olivierdevelops/perch/infra/interpreter"
)

func registerTextLines(m map[string]interpreter.Handler) {
	m["grep"] = opGrep
	m["reject"] = opReject
	m["cut"] = opCut
	m["head"] = opHead
	m["tail"] = opTail
	m["sort_lines"] = opSortLines
	m["uniq_lines"] = opUniqLines
	m["count_lines"] = opCountLines
}

// splitLines splits on \n and drops a single trailing empty line (so a
// value ending in "\n" yields N lines, not N+1). Empty input → no lines.
func splitLines(s string) []string {
	if s == "" {
		return nil
	}
	s = strings.TrimRight(s, "\n")
	if s == "" {
		return []string{""}
	}
	return strings.Split(s, "\n")
}

// opGrep keeps lines matching the regex pattern. `grep PAT TEXT`.
func opGrep(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	pat := argString(args, "_0", "pattern")
	text := argString(args, "_1", "text")
	re, err := regexp.Compile(pat)
	if err != nil {
		return "", err
	}
	var keep []string
	for _, l := range splitLines(text) {
		if re.MatchString(l) {
			keep = append(keep, l)
		}
	}
	return strings.Join(keep, "\n"), nil
}

// opReject is grep's inverse — keeps lines NOT matching. `reject PAT TEXT`.
func opReject(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	pat := argString(args, "_0", "pattern")
	text := argString(args, "_1", "text")
	re, err := regexp.Compile(pat)
	if err != nil {
		return "", err
	}
	var keep []string
	for _, l := range splitLines(text) {
		if !re.MatchString(l) {
			keep = append(keep, l)
		}
	}
	return strings.Join(keep, "\n"), nil
}

// opCut extracts the Nth whitespace-delimited field (1-indexed) of each
// line. `cut N TEXT`. Lines with fewer than N fields contribute "".
// (Comma/other separators: use `split` per line instead.)
func opCut(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	n := int(toFloat(args["_0"]))
	text := argString(args, "_1", "text")
	if n < 1 {
		n = 1
	}
	var out []string
	for _, l := range splitLines(text) {
		fields := strings.Fields(l)
		if n <= len(fields) {
			out = append(out, fields[n-1])
		} else {
			out = append(out, "")
		}
	}
	return strings.Join(out, "\n"), nil
}

// opHead returns the first N lines. `head N TEXT`.
func opHead(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	n := int(toFloat(args["_0"]))
	text := argString(args, "_1", "text")
	lines := splitLines(text)
	if n < 0 {
		n = 0
	}
	if n > len(lines) {
		n = len(lines)
	}
	return strings.Join(lines[:n], "\n"), nil
}

// opTail returns the last N lines. `tail N TEXT`.
func opTail(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	n := int(toFloat(args["_0"]))
	text := argString(args, "_1", "text")
	lines := splitLines(text)
	if n < 0 {
		n = 0
	}
	if n > len(lines) {
		n = len(lines)
	}
	return strings.Join(lines[len(lines)-n:], "\n"), nil
}

// opSortLines sorts lines lexicographically. `sort_lines TEXT`.
func opSortLines(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	lines := splitLines(argString(args, "_0", "text"))
	sorted := make([]string, len(lines))
	copy(sorted, lines)
	sort.Strings(sorted)
	return strings.Join(sorted, "\n"), nil
}

// opUniqLines collapses ADJACENT duplicate lines (like `uniq`). Pair with
// sort_lines for global dedup. `uniq_lines TEXT`.
func opUniqLines(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	lines := splitLines(argString(args, "_0", "text"))
	var out []string
	for idx, l := range lines {
		if idx == 0 || l != lines[idx-1] {
			out = append(out, l)
		}
	}
	return strings.Join(out, "\n"), nil
}

// opCountLines returns the number of lines as a string-encoded int (so it
// composes with both ${} interpolation and numeric comparisons).
// `count_lines TEXT`.
func opCountLines(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	return strconv.Itoa(len(splitLines(argString(args, "_0", "text")))), nil
}

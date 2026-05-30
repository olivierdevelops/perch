package ops

import (
	"strconv"
	"strings"

	"github.com/olivierdevelops/perch/domain"
	"github.com/olivierdevelops/perch/infra/interpreter"
)

func registerFlow(m map[string]interpreter.Handler) {
	m["if"] = opIf
	m["if_call"] = opIfCall
	m["for_each"] = opForEach
	m["os"] = opOsBlock
	m["arch"] = opArchBlock
}

// opArchBlock — architecture execution context block. Body runs only
// when ${arch} matches the declared target. Targets are Go GOARCH
// values ("amd64", "arm64", "386", "arm", "riscv64", …); exact match
// only — no umbrella names because matrix builds want exact pinning.
func opArchBlock(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	target, _ := args["target"].(string)
	currentArch, _ := b.Lookup("arch")
	if !ArchTargetMatches(target, currentArch) {
		return nil, nil
	}
	body, _ := args["_body"].([]domain.Op)
	return nil, i.RunOps(body, b)
}

// ArchTargetMatches reports whether the declared target matches the
// host's ${arch}. Exact match; future-proofed for arch families if
// they become useful.
func ArchTargetMatches(target, host string) bool {
	if target == "" || host == "" {
		return false
	}
	return target == host
}

// opOsBlock — OS execution context block. `os "linux" ... end` runs
// its body only when the host's ${os} matches the declared target.
//
// Targets:
//   "darwin" | "linux" | "windows"     — exact ${os} match
//   "freebsd" | "openbsd" | "netbsd"   — exact ${os} match
//   "unix"                              — umbrella matching the same
//                                         set as ${is_unix}
//
// Design note: a future revision will fold this into a unified
// `with ... then ... end` block that carries any combination of
// context attributes. That refactor is blocked on a capy grammar
// capability that isn't in the current engine.
func opOsBlock(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	target, _ := args["target"].(string)
	currentOS, _ := b.Lookup("os")
	if !osTargetMatches(target, currentOS) {
		return nil, nil
	}
	body, _ := args["_body"].([]domain.Op)
	return nil, i.RunOps(body, b)
}

// OsTargetMatches reports whether the declared target matches the
// host's ${os}. Exposed for the simulator + scanner.
func OsTargetMatches(target, host string) bool {
	return osTargetMatches(target, host)
}

func osTargetMatches(target, host string) bool {
	if target == "" || host == "" {
		return false
	}
	if target == host {
		return true
	}
	if target == "unix" {
		switch host {
		case "darwin", "linux", "freebsd", "openbsd", "netbsd":
			return true
		}
	}
	return false
}

// opForEach iterates over a newline-separated string value, binding each
// non-empty line to the loop variable and running the body. Used to walk
// a `rest`-typed arg, a `glob` result, `list_dir`, `interfaces`, or any
// op whose value is a newline-joined list.
//
// Form in capy:
//
//   for_each "${files}" file
//       print "→ ${file}"
//   end
//
// Empty input is a clean no-op (the body never runs). The previous value
// of the loop variable (if any) is restored after the loop so for_each
// blocks compose cleanly.
func opForEach(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	source := argString(args, "value", "_0")
	loopVar := argString(args, "var", "_1")
	if loopVar == "" {
		loopVar = "item"
	}
	body, _ := args["_body"].([]domain.Op)

	prev, hadPrev := b.Vars[loopVar]
	defer func() {
		if hadPrev {
			b.Vars[loopVar] = prev
		} else {
			delete(b.Vars, loopVar)
		}
	}()

	for _, line := range strings.Split(source, "\n") {
		if line == "" {
			continue
		}
		b.Set(loopVar, line)
		if err := i.RunOps(body, b); err != nil {
			return nil, err
		}
	}
	return nil, nil
}

func runBody(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) error {
	body, ok := args["_body"].([]domain.Op)
	if !ok {
		return nil
	}
	return i.RunOps(body, b)
}

// opIf evaluates `if NAME OP VALUE`, `if NAME`, or `if not NAME`.
// args.op ∈ { "eq", "neq", "gt", "lt", "ge", "le", "truthy", "falsy" }.
// args.lhs is a binding name (auto-bound + globals + args + lets + env).
// args.rhs is the literal to compare against (for comparison ops only).
func opIf(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	op, _ := args["op"].(string)
	lhsName, _ := args["lhs"].(string)
	lhsVal, _ := b.Lookup(lhsName)
	rhs := args["rhs"]

	var matched bool
	switch op {
	case "eq":
		matched = lhsVal == interpreter.ToStringValue(rhs)
	case "neq":
		matched = lhsVal != interpreter.ToStringValue(rhs)
	case "gt":
		matched = compareValues(lhsVal, rhs) > 0
	case "lt":
		matched = compareValues(lhsVal, rhs) < 0
	case "ge":
		matched = compareValues(lhsVal, rhs) >= 0
	case "le":
		matched = compareValues(lhsVal, rhs) <= 0
	case "truthy":
		matched = truthy(lhsVal)
	case "falsy":
		matched = !truthy(lhsVal)
	}
	if matched {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

// compareValues is the ordering used by `if X >= Y` / `if X > Y` / etc.
// Auto-detects two regimes:
//
//   - Both sides parse as versions (start with optional 'v', contain a
//     dot, segments are all digits) → semver-aware comparison.
//     ("1.10.0" > "1.9.0", which plain-float would get wrong.)
//   - Otherwise → numeric (toFloat).
//
// The same `if v >= "1.28.0"` users would write for version checks
// stays correct WITHOUT requiring a separate `if_version` keyword.
// Plain numeric comparisons (file sizes, counts) unaffected.
func compareValues(lhs, rhs any) int {
	ls := interpreter.ToStringValue(lhs)
	rs := interpreter.ToStringValue(rhs)
	if looksLikeVersion(ls) && looksLikeVersion(rs) {
		return versionCompare(ls, rs)
	}
	lf := toFloat(lhs)
	rf := toFloat(rhs)
	switch {
	case lf < rf:
		return -1
	case lf > rf:
		return +1
	}
	return 0
}

// looksLikeVersion returns true if s is shaped like a dotted-number
// version string (e.g. "1.28.0", "v1.29.3", "20.10.0-rc.1"). The
// heuristic: optional leading 'v', then at least one '.', and the
// segments before any '-' / '+' tail must be all digits.
func looksLikeVersion(s string) bool {
	s = strings.TrimSpace(s)
	s = strings.TrimPrefix(s, "v")
	if !strings.Contains(s, ".") {
		return false
	}
	// Strip pre-release/build tail.
	if i := strings.IndexAny(s, "-+"); i >= 0 {
		s = s[:i]
	}
	for _, part := range strings.Split(s, ".") {
		if part == "" {
			return false
		}
		for _, c := range part {
			if c < '0' || c > '9' {
				return false
			}
		}
	}
	return true
}

// versionCompare lives in version.go; declared here for go-doc proximity.
// (No re-declaration needed — same package.)

// opIfCall evaluates `if FUNC ARG ... end` by invoking the named op
// with one argument and running the body if the return value is truthy.
func opIfCall(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	fname, _ := args["func"].(string)
	if fname == "" {
		return nil, nil
	}
	h, ok := i.Handlers[fname]
	if !ok {
		return nil, nil
	}
	val, err := h(i, b, map[string]any{"_0": args["_0"]})
	if err != nil {
		return nil, err
	}
	if truthyValue(val) {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

// ── helpers ────────────────────────────────────────────────────────────

// truthy: empty string / "false" / "0" / "" are false; everything else true.
func truthy(s string) bool {
	if s == "" || s == "false" || s == "0" {
		return false
	}
	return true
}

// truthyValue: the raw return value from an op handler (a Go any).
func truthyValue(v any) bool {
	switch x := v.(type) {
	case nil:
		return false
	case bool:
		return x
	case string:
		return truthy(x)
	case int:
		return x != 0
	case int64:
		return x != 0
	case float64:
		return x != 0
	}
	return true
}

func toFloat(v any) float64 {
	switch x := v.(type) {
	case float64:
		return x
	case int:
		return float64(x)
	case int64:
		return float64(x)
	case bool:
		if x {
			return 1
		}
		return 0
	case string:
		f, _ := strconv.ParseFloat(x, 64)
		return f
	}
	return 0
}

package ops

import (
	"strconv"
	"strings"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

func registerFlow(m map[string]interpreter.Handler) {
	m["if"] = opIf
	m["if_call"] = opIfCall
	m["for_each"] = opForEach
	m["os"] = opOsBlock
}

// opOsBlock — execution context block. `os "linux" ... end` runs its
// body only when the host's ${os} matches the declared target.
// Semantically richer than `if os == "linux" ... end`: the body has a
// KNOWN OS context, which the simulator + --scan + the UI use to make
// stronger statements about what the program needs.
//
// Targets:
//   "darwin"  | "linux" | "windows"   — exact ${os} match
//   "unix"    — umbrella: matches darwin, linux, freebsd, openbsd, netbsd
//               (mirrors the existing ${is_unix} auto-bound var)
//   "freebsd" / "openbsd" / "netbsd"  — exact ${os} match (rare but works)
//
// Runtime semantics: skip body if the target doesn't match. That's it.
// The "stronger statements" live in the static-analysis layers
// (--scan, simulate), not in the interpreter.
func opOsBlock(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	target, _ := args["target"].(string)
	currentOS, _ := b.Lookup("os")
	if !osTargetMatches(target, currentOS) {
		return nil, nil // wrong OS; skip
	}
	body, _ := args["_body"].([]domain.Op)
	return nil, i.RunOps(body, b)
}

// OsTargetMatches reports whether the declared target matches the
// host's ${os}. Exposed (capital) so the simulator + scanner can apply
// the same rule.
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
		// Same set ${is_unix} covers: anything that ISN'T windows.
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
		matched = toFloat(lhsVal) > toFloat(rhs)
	case "lt":
		matched = toFloat(lhsVal) < toFloat(rhs)
	case "ge":
		matched = toFloat(lhsVal) >= toFloat(rhs)
	case "le":
		matched = toFloat(lhsVal) <= toFloat(rhs)
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

// Version ops — extract + compare.
//
// Two primitives compose into version-gated workflows:
//
//   version_extract STRING [PATTERN]   pull a version string out of arbitrary text
//   version_eq / _ne / _gt / _ge / _lt / _le / _compat   compare two versions
//
// Common use: gate execution on a tool's reported version.
//
//   let raw = shell_output "kubectl version --client -o json"
//   let v   = version_extract "${raw}"
//   let ok  = version_ge "${v}" "1.28.0"
//   if ok ... end
//
// Design notes:
//
//   - No semver library dependency. The comparator handles plain
//     dotted numeric tuples ("1.29.3", "3.14", "20.10.0") with optional
//     "v" prefix and an optional pre-release tail ("-rc.1", "+meta").
//     Pre-release tails sort BEFORE the un-suffixed version (semver
//     spec); build metadata is ignored.
//   - version_extract defaults to the most-permissive pattern that
//     matches the widest set of real CLI version outputs: an optional
//     "v", at least two numeric segments, optional further numeric
//     segments, optional pre-release/build suffix. Users override
//     when their tool's format is unusual.
//   - Comparators return a bool (string "true"/"false"). They never
//     error on parse failure — they fall back to lexicographic
//     comparison so the caller's `if ok` still reaches a decision.
package ops

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

// defaultVersionPattern matches the most common shapes that appear in
// real `--version` / `version --client` output:
//
//   v1.29.3
//   1.29.3
//   3.14
//   v20.10.0+meta123
//   1.29.3-rc.1
//
// The first capture group is the version string proper (with optional
// v prefix stripped on extraction).
var defaultVersionPattern = regexp.MustCompile(`v?(\d+(?:\.\d+)+(?:[-+][\w.+-]+)?)`)

func registerVersion(m map[string]interpreter.Handler) {
	m["version_extract"] = opVersionExtract
	m["version_eq"] = versionCmpOp(func(c int) bool { return c == 0 })
	m["version_ne"] = versionCmpOp(func(c int) bool { return c != 0 })
	m["version_gt"] = versionCmpOp(func(c int) bool { return c > 0 })
	m["version_ge"] = versionCmpOp(func(c int) bool { return c >= 0 })
	m["version_lt"] = versionCmpOp(func(c int) bool { return c < 0 })
	m["version_le"] = versionCmpOp(func(c int) bool { return c <= 0 })
	// version_compat — same major version. The classic "compatible
	// release" check (e.g. PEP 440's `~=`, Cargo's caret), useful for
	// "any 1.x is fine but not 2.0".
	m["version_compat"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		a := versionParts(argString(args, "a", "_0"))
		b2 := versionParts(argString(args, "b", "_1"))
		if len(a) == 0 || len(b2) == 0 {
			return "false", nil
		}
		return strconv.FormatBool(a[0] == b2[0]), nil
	}
	m["assert_version_ge"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		got := argString(args, "got", "_0")
		want := argString(args, "want", "_1")
		if versionCompare(got, want) >= 0 {
			return nil, nil
		}
		return nil, domain.NewOpError("assert_version_ge", domain.ErrAssertFailed,
			fmt.Sprintf("version %q is below required %q", got, want)).
			WithDetail(fmt.Sprintf("got=%q want_at_least=%q", got, want))
	}
	// assert_version with infix operator — `assert_version "X" OP "Y"`.
	// The grammar carries the operator as args.op; this single handler
	// dispatches all six comparison operators + the compat (~=) check.
	m["assert_version"] = opAssertVersionInfix
}

func opAssertVersionInfix(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	op, _ := args["op"].(string)
	// lhs can be either:
	//   - args.lhs       (string form: `assert_version "${v}" >= "1.28.0"`)
	//   - args._lhs_var  (ident form:  `assert_version v >= "1.28.0"` → look up
	//                     "v" in bindings at runtime)
	var lhs string
	if lhsVar, ok := args["_lhs_var"].(string); ok && lhsVar != "" {
		val, _ := b.Lookup(lhsVar)
		lhs = val
	} else {
		lhs = argString(args, "lhs")
	}
	rhs := argString(args, "rhs")
	cmp := versionCompare(lhs, rhs)

	var ok bool
	var symbol string
	switch op {
	case "ge":
		ok, symbol = cmp >= 0, ">="
	case "gt":
		ok, symbol = cmp > 0, ">"
	case "le":
		ok, symbol = cmp <= 0, "<="
	case "lt":
		ok, symbol = cmp < 0, "<"
	case "eq":
		ok, symbol = cmp == 0, "=="
	case "ne":
		ok, symbol = cmp != 0, "!="
	case "compat":
		la := versionParts(lhs)
		lb := versionParts(rhs)
		ok = len(la) > 0 && len(lb) > 0 && la[0] == lb[0]
		symbol = "~"
	default:
		return nil, domain.NewOpError("assert_version", domain.ErrUnclassified,
			"unknown comparison operator "+strconv.Quote(op))
	}
	if ok {
		return nil, nil
	}
	return nil, domain.NewOpError("assert_version", domain.ErrAssertFailed,
		fmt.Sprintf("version assertion failed: %q %s %q is not true", lhs, symbol, rhs)).
		WithDetail(fmt.Sprintf("got=%q op=%s want=%q", lhs, symbol, rhs))
}

// opVersionExtract pulls a version string out of arbitrary text.
//
//   version_extract "some text with v1.29.3 in it"          → "1.29.3"
//   version_extract "$raw"                                   → first numeric tuple
//   version_extract "$raw" `"gitVersion":"v(\d+\.\d+\.\d+)"` → uses your pattern
//
// When a custom PATTERN is provided, the FIRST CAPTURE GROUP wins
// (so users can pinpoint the exact substring). With no pattern, the
// default heuristic matches widely.
//
// Returns "" if nothing matches (so downstream `version_*` ops
// return false rather than erroring, keeping flow control clean).
func opVersionExtract(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := argString(args, "src", "_0")
	pat := argString(args, "pattern", "_1")
	if pat == "" {
		m := defaultVersionPattern.FindStringSubmatch(src)
		if len(m) >= 2 {
			return m[1], nil
		}
		return "", nil
	}
	re, err := regexp.Compile(pat)
	if err != nil {
		return "", domain.NewOpError("version_extract", domain.ErrUnclassified,
			"regex compile: "+err.Error())
	}
	m := re.FindStringSubmatch(src)
	switch {
	case len(m) >= 2:
		return m[1], nil // first capture group
	case len(m) == 1:
		return m[0], nil // no group → whole match
	default:
		return "", nil
	}
}

func versionCmpOp(pred func(int) bool) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		a := argString(args, "a", "_0")
		bv := argString(args, "b", "_1")
		return strconv.FormatBool(pred(versionCompare(a, bv))), nil
	}
}

// versionCompare returns -1 if a<b, 0 if a==b, +1 if a>b.
//
// Comparison rules (best-effort, dependency-free):
//
//   1. Strip an optional "v" prefix from each side.
//   2. Split off any pre-release tail at the first "-" (build metadata
//      after "+" is dropped).
//   3. Compare the numeric tuples element-wise. Missing elements are
//      treated as 0 (so "1.2" == "1.2.0").
//   4. If numeric tuples tie, an un-suffixed version is GREATER than
//      one with a pre-release tail (semver: "1.0.0" > "1.0.0-rc.1").
//   5. If both have pre-release tails, fall back to string compare.
//
// If either input is unparseable (no numeric tuple at all), returns
// strings.Compare(a, b) as a last-resort ordering so callers always
// get a deterministic decision.
func versionCompare(a, b string) int {
	apts, atail := splitVersion(a)
	bpts, btail := splitVersion(b)
	if len(apts) == 0 || len(bpts) == 0 {
		return strings.Compare(a, b)
	}
	// Element-wise numeric compare.
	n := len(apts)
	if len(bpts) > n {
		n = len(bpts)
	}
	for i := 0; i < n; i++ {
		ai, bi := 0, 0
		if i < len(apts) {
			ai = apts[i]
		}
		if i < len(bpts) {
			bi = bpts[i]
		}
		if ai < bi {
			return -1
		}
		if ai > bi {
			return +1
		}
	}
	// Numeric tuples tie. Pre-release tail rules:
	//   no tail > has tail
	//   both tails → string compare
	switch {
	case atail == "" && btail == "":
		return 0
	case atail == "" && btail != "":
		return +1
	case atail != "" && btail == "":
		return -1
	default:
		return strings.Compare(atail, btail)
	}
}

// splitVersion("v1.29.3-rc.1+meta5") → ([1,29,3], "rc.1")
// splitVersion("1.2")                   → ([1,2], "")
func splitVersion(s string) ([]int, string) {
	s = strings.TrimPrefix(strings.TrimSpace(s), "v")
	// Drop build metadata.
	if i := strings.IndexByte(s, '+'); i >= 0 {
		s = s[:i]
	}
	tail := ""
	if i := strings.IndexByte(s, '-'); i >= 0 {
		tail = s[i+1:]
		s = s[:i]
	}
	if s == "" {
		return nil, tail
	}
	parts := strings.Split(s, ".")
	out := make([]int, 0, len(parts))
	for _, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil {
			return nil, tail // unparseable
		}
		out = append(out, n)
	}
	return out, tail
}

// versionParts returns just the numeric tuple (used by version_compat
// to look at the major component).
func versionParts(s string) []int {
	pts, _ := splitVersion(s)
	if len(pts) > 0 {
		return pts
	}
	// Fallback: try extracting via defaultVersionPattern.
	if m := defaultVersionPattern.FindStringSubmatch(s); len(m) >= 2 {
		pts, _ = splitVersion(m[1])
	}
	return pts
}

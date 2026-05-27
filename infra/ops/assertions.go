package ops

// Assertion ops — thin sugar over `if X fail "msg"`. They exist so
// tests read naturally and produce helpful failure messages that name
// the values that didn't match.
//
//   assert_eq           "${target}" "darwin"
//   assert_neq          "${target}" ""
//   assert_contains     "${output}" "Build complete"
//   assert_not_contains "${output}" "warning:"
//   assert_exists       "bin/darwin/app"
//   assert_not_exists   "bin/darwin/app.bak"
//   assert_match        "${version}" "^v[0-9]+\\.[0-9]+\\.[0-9]+$"
//
// Each calls `errors.New(...)` so the test runner counts the test as
// failed. The message points at the expected/actual values; the runner
// adds context (which test, which line) when rendering the failure.
//
// These ops are NOT gated by --no-shell / --no-network / etc. — they
// touch nothing the runtime can't see. Existence checks are FS reads
// only; the FS-write restriction doesn't apply.

import (
	"fmt"
	"os"
	"regexp"
	"strings"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerAssertions(m map[string]interpreter.Handler) {
	m["assert_eq"] = opAssertEq
	m["assert_neq"] = opAssertNeq
	m["assert_contains"] = opAssertContains
	m["assert_not_contains"] = opAssertNotContains
	m["assert_exists"] = opAssertExists
	m["assert_not_exists"] = opAssertNotExists
	m["assert_match"] = opAssertMatch
}

func opAssertEq(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	actual := argString(args, "actual", "_0")
	expected := argString(args, "expected", "_1")
	if actual != expected {
		return nil, fmt.Errorf("assert_eq failed: expected %q, got %q", expected, actual)
	}
	return nil, nil
}

func opAssertNeq(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	a := argString(args, "actual", "_0")
	notExpected := argString(args, "not_expected", "_1")
	if a == notExpected {
		return nil, fmt.Errorf("assert_neq failed: value should not be %q", notExpected)
	}
	return nil, nil
}

func opAssertContains(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	haystack := argString(args, "haystack", "_0")
	needle := argString(args, "needle", "_1")
	if !strings.Contains(haystack, needle) {
		return nil, fmt.Errorf("assert_contains failed: %q not found in %q", needle, truncateForError(haystack))
	}
	return nil, nil
}

func opAssertNotContains(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	haystack := argString(args, "haystack", "_0")
	needle := argString(args, "needle", "_1")
	if strings.Contains(haystack, needle) {
		return nil, fmt.Errorf("assert_not_contains failed: %q unexpectedly found in %q", needle, truncateForError(haystack))
	}
	return nil, nil
}

func opAssertExists(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	raw := argString(args, "path", "_0")
	p := resolve(raw, b)
	if _, err := os.Stat(p); err != nil {
		if os.IsNotExist(err) {
			return nil, fmt.Errorf("assert_exists failed: %q does not exist", raw)
		}
		return nil, fmt.Errorf("assert_exists failed: %q: %w", raw, err)
	}
	return nil, nil
}

func opAssertNotExists(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	raw := argString(args, "path", "_0")
	p := resolve(raw, b)
	if _, err := os.Stat(p); err == nil {
		return nil, fmt.Errorf("assert_not_exists failed: %q exists but shouldn't", raw)
	} else if !os.IsNotExist(err) {
		return nil, fmt.Errorf("assert_not_exists failed: %q: %w", raw, err)
	}
	return nil, nil
}

func opAssertMatch(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	actual := argString(args, "actual", "_0")
	pattern := argString(args, "pattern", "_1")
	re, err := regexp.Compile(pattern)
	if err != nil {
		return nil, fmt.Errorf("assert_match: invalid regex %q: %w", pattern, err)
	}
	if !re.MatchString(actual) {
		return nil, fmt.Errorf("assert_match failed: %q did not match /%s/", truncateForError(actual), pattern)
	}
	return nil, nil
}

// truncateForError shortens long values so error messages stay readable.
// Tests that compare big strings still get useful context (the prefix),
// without burying the actual failure under a wall of output.
func truncateForError(s string) string {
	const max = 120
	if len(s) <= max {
		return s
	}
	return s[:max-3] + "..."
}

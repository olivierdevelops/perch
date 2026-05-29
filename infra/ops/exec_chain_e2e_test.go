package ops_test

import (
	"strings"
	"testing"
)

// exec chains: `&&` runs next on success, `||` on failure, `;` always.
// Operators are literal source tokens (loader-folded) — never from ${…}.

func TestExecChain_AndAllRun(t *testing.T) {
	out, err := runSource(t, `name "x"
command t
    do
        exec echo one && exec echo two && exec echo three
    end
end
`, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	for _, w := range []string{"one", "two", "three"} {
		if !strings.Contains(out, w) {
			t.Errorf("missing %q in %q", w, out)
		}
	}
}

func TestExecChain_AndShortCircuitsAndAborts(t *testing.T) {
	out, err := runSource(t, `name "x"
command t
    do
        exec false && exec echo nope
    end
end
`, "t")
	if err == nil {
		t.Fatal("a failing && LHS should make the chain raise")
	}
	if strings.Contains(out, "nope") {
		t.Errorf("&& must not run RHS after LHS failed; out=%q", out)
	}
}

func TestExecChain_OrRecovers(t *testing.T) {
	out, err := runSource(t, `name "x"
command t
    do
        exec false || exec echo recovered
    end
end
`, "t")
	if err != nil {
		t.Fatalf("|| should recover (RHS succeeds): %v", err)
	}
	if !strings.Contains(out, "recovered") {
		t.Errorf("|| did not run RHS after LHS failed; out=%q", out)
	}
}

func TestExecChain_SemicolonAlwaysRuns(t *testing.T) {
	out, err := runSource(t, `name "x"
command t
    do
        exec echo a ; exec echo b
    end
end
`, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "a") || !strings.Contains(out, "b") {
		t.Errorf("; should run both; out=%q", out)
	}
}

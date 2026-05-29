package ops_test

import (
	"strings"
	"testing"
)

// try/rescue/finally now parses (capy block_sections) and runs end-to-end.

func TestTry_RescueAndFinally(t *testing.T) {
	src := `name "x"
command t
    do
        try
            fail "boom"
        rescue
            print "caught:${err.kind}"
        finally
            print "cleanup"
        end
        print "after"
    end
end
`
	out, err := runSource(t, src, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	for _, want := range []string{"caught:user_fail", "cleanup", "after"} {
		if !strings.Contains(out, want) {
			t.Errorf("missing %q in out=%q", want, out)
		}
	}
}

// finally-only (no rescue): the error must RE-RAISE after cleanup runs —
// the always-emitted _catch marker with an empty body must not swallow it.
func TestTry_FinallyOnlyReRaises(t *testing.T) {
	src := `name "x"
command t
    do
        try
            fail "uncaught"
        finally
            print "cleanup-ran"
        end
        print "should-not-print"
    end
end
`
	out, err := runSource(t, src, "t")
	if err == nil {
		t.Fatal("expected the error to propagate (no rescue arm)")
	}
	if !strings.Contains(out, "cleanup-ran") {
		t.Errorf("finally should still run; out=%q", out)
	}
	if strings.Contains(out, "should-not-print") {
		t.Errorf("control should not continue past an uncaught try; out=%q", out)
	}
}

// Bare `match err.kind` (dotted ident) dispatches on the error binding.
func TestMatch_DottedIdent(t *testing.T) {
	src := `name "x"
command t
    do
        try
            fail "x"
        rescue
            match err.kind
                case user_fail
                    print "hit-user-fail"
                else
                    print "miss"
            end
        end
    end
end
`
	out, err := runSource(t, src, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "hit-user-fail") {
		t.Errorf("bare `match err.kind` did not dispatch; out=%q", out)
	}
}

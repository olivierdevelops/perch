package ops_test

import (
	"runtime"
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
)

// pipe wires exec stages stdout->stdin with no shell. echo|tr|rev are real
// bins on the test host.
func TestPipe_WiresStagesAndCaptures(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("uses tr/rev which aren't on Windows PATH; pipe stage-gating is covered by TestPipe_GatesEachStage")
	}
	src := `name "x"
requires
    bin "echo"
    bin "tr"
    bin "rev"
end
command t
    do
        let out = pipe
            exec echo hello
            exec tr a-z A-Z
            exec rev
        end
        print "r=${out}"
    end
end
`
	out, err := runSource(t, src, "t")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "r=OLLEH") {
		t.Errorf("pipe did not produce reversed-uppercased value; out=%q", out)
	}
}

// Every stage's bin is gated: an undeclared bin in the chain is refused.
func TestPipe_GatesEachStage(t *testing.T) {
	src := `name "x"
requires
    bin "echo"
end
command t
    do
        let out = pipe
            exec echo "hi"
            exec wc -c
        end
        print "${out}"
    end
end
`
	_, err := runSource(t, src, "t")
	if !isOpKind(err, domain.ErrBinNotDeclared) {
		t.Fatalf("undeclared pipe stage should be bin_not_declared, got %v", err)
	}
}

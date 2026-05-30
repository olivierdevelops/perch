package capyloader

import "testing"

// TestExecGrammar verifies the `exec BIN "arg"…` op parses with the bin
// routed to args["bin"] and each quoted token to a positional argv slot
// (_0, _1, …) — the structured-argv shape opExec consumes. Also covers the
// let-capture form `let x = exec BIN …`.
func TestExecGrammar(t *testing.T) {
	src := `name "x"
requires
    bin "git"
    bin "docker"
end
command t
    do
        exec git status
        exec docker run -d --name web
        head = exec git rev-parse HEAD
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	ops := p.Commands["t"].Ops
	if len(ops) != 3 {
		t.Fatalf("want 3 ops, got %d (%+v)", len(ops), ops)
	}

	// exec git "status"
	if ops[0].Kind != "exec" {
		t.Fatalf("op0 kind: want exec, got %q", ops[0].Kind)
	}
	if got := argStr(ops[0].Args, "bin"); got != "git" {
		t.Errorf("op0 bin: want git, got %q", got)
	}
	if got := argStr(ops[0].Args, "_0"); got != "status" {
		t.Errorf("op0 _0: want status, got %q", got)
	}

	// exec docker "run" "-d" "--name" "web"
	if got := argStr(ops[1].Args, "bin"); got != "docker" {
		t.Errorf("op1 bin: want docker, got %q", got)
	}
	wantArgv := []string{"run", "-d", "--name", "web"}
	for i, w := range wantArgv {
		if got := argStr(ops[1].Args, "_"+itoa(i)); got != w {
			t.Errorf("op1 _%d: want %q, got %q", i, w, got)
		}
	}

	// let head = exec git "rev-parse" "HEAD"
	if ops[2].Kind != "exec" {
		t.Fatalf("op2 kind: want exec, got %q", ops[2].Kind)
	}
	if ops[2].CaptureInto != "head" {
		t.Errorf("op2 capture: want head, got %q", ops[2].CaptureInto)
	}
	if got := argStr(ops[2].Args, "bin"); got != "git" {
		t.Errorf("op2 bin: want git, got %q", got)
	}
	if got := argStr(ops[2].Args, "_0"); got != "rev-parse" {
		t.Errorf("op2 _0: want rev-parse, got %q", got)
	}
	if got := argStr(ops[2].Args, "_1"); got != "HEAD" {
		t.Errorf("op2 _1: want HEAD, got %q", got)
	}
}

func argStr(m map[string]any, k string) string {
	if v, ok := m[k]; ok {
		if s, ok := v.(string); ok {
			return s
		}
	}
	return ""
}

func itoa(i int) string { return string(rune('0' + i)) }

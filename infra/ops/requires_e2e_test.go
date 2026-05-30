package ops_test

import (
	"bytes"
	"os"
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/capyloader"
	"github.com/luowensheng/perch/infra/interpreter"
	"github.com/luowensheng/perch/infra/ops"
)

// runSource loads a .perch source string through the real loader, builds an
// interpreter with the full gated handler set + Preflight hook, runs one
// command, and returns captured stdout + any run error.
func runSource(t *testing.T, src, command string, args ...string) (string, error) {
	t.Helper()
	prog, err := capyloader.LoadFromString(src)
	if err != nil {
		// Load-time errors (bin_not_declared from the zero-ambient gate) are
		// returned so tests can assert on them.
		return "", err
	}
	i := interpreter.New(ops.AllHandlers(), prog)
	i.PreflightHook = ops.Preflight
	var out bytes.Buffer
	i.Stdout = &out
	i.Stderr = &out
	runErr := i.Run(command, args)
	return out.String(), runErr
}

// Bare-ident args: `print x` (no ${}) must interpolate the variable, and
// `let v = upper name` must capture op output from a bare-ident arg.
func TestE2E_BareIdentArgs(t *testing.T) {
	src := `name "x"
requires
end
command greet
    arg who
        type string
        default "world"
    end
    do
        let shout = upper who
        print shout
    end
end
`
	out, err := runSource(t, src, "greet", "-who=team")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "TEAM") {
		t.Errorf("bare-ident interpolation+capture failed; out=%q", out)
	}
}

// Flat (non-`let`) statement with a single bare-ident arg must resolve the
// ident as a binding, not pass it literally: `ensure_dir BUILD_DIR` writes the
// directory named by BUILD_DIR's value, mirroring `write BUILD_DIR` in requires.
func TestE2E_FlatBareIdentArg(t *testing.T) {
	dir := t.TempDir()
	src := `name "x"
OUT = "` + dir + `/sub"
requires
    write "${OUT}"
end
command build
    do
        ensure_dir OUT
        print "${OUT}"
    end
end
`
	out, err := runSource(t, src, "build")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, dir+"/sub") {
		t.Errorf("flat bare-ident arg not resolved to binding; out=%q", out)
	}
	if _, statErr := os.Stat(dir + "/sub"); statErr != nil {
		t.Errorf("ensure_dir did not create the binding-named dir: %v", statErr)
	}
}

// match on a bare ident (`match os`) must dispatch on the host fact.
func TestE2E_MatchIdent(t *testing.T) {
	src := `name "x"
requires
end
command pick
    do
        match os
            case "nope-os"
                print "wrong"
            else
                print "matched-default"
            end
        end
    end
end
`
	out, err := runSource(t, src, "pick")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "matched-default") {
		t.Errorf("match ident did not reach default; out=%q", out)
	}
}

// A requires block with read/write scopes must allow in-scope FS ops and
// refuse out-of-scope ones with the right error kind — end to end.
func TestE2E_FSScopeGating(t *testing.T) {
	dir := t.TempDir()
	src := `name "x"
requires
    write "` + dir + `"
end
command w
    do
        write_file "` + dir + `/ok.txt" "hi"
        write_file "/etc/should-not-write.txt" "nope"
    end
end
`
	_, err := runSource(t, src, "w")
	if !isOpKind(err, domain.ErrWriteNotDeclared) {
		t.Fatalf("expected write_not_declared for out-of-scope write, got %v", err)
	}
}

// A file with no requires block is treated as empty manifest: pure ops run,
// undeclared filesystem writes fail at runtime with write_not_declared.
func TestE2E_MissingRequiresTreatedAsEmpty(t *testing.T) {
	dir := t.TempDir()
	src := `name "x"
command w
    do
        write_file "` + dir + `/x.txt" "hi"
    end
end
`
	_, err := runSource(t, src, "w")
	if !isOpKind(err, domain.ErrWriteNotDeclared) {
		t.Fatalf("missing requires (empty manifest) must refuse undeclared write, got %v", err)
	}
}

func isOpKind(err error, want domain.ErrorKind) bool {
	if oe, ok := err.(*domain.OpError); ok {
		return oe.Kind == want
	}
	return false
}

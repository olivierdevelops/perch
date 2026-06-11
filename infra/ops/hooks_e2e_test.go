package ops_test

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/olivierdevelops/perch/infra/capyloader"
	"github.com/olivierdevelops/perch/infra/interpreter"
	"github.com/olivierdevelops/perch/infra/ops"
)

// runWithHooks is runSource but wires the hook-category classifier (as the
// orchestrator does), so category targets like `write` resolve.
func runWithHooks(t *testing.T, src, command string) (string, error) {
	t.Helper()
	prog, err := capyloader.LoadFromString(src)
	if err != nil {
		return "", err
	}
	i := interpreter.New(ops.AllHandlers(), prog)
	i.PreflightHook = ops.Preflight
	i.HookCategory = ops.HookCategoryOf
	var out bytes.Buffer
	i.Stdout = &out
	i.Stderr = &out
	runErr := i.Run(command, nil)
	return out.String(), runErr
}

// A `before write` hook fires around every write op, sees ${hook.target}, and
// an `after` hook runs once the op returns.
func TestE2E_Hooks_FireAroundOp(t *testing.T) {
	dir := t.TempDir()
	src := `name "x"
hooks
    before write audit
    after  write done
end
requires
    write "` + dir + `"
end
command audit
    do
        print "before:${hook.target}"
    end
end
command done
    do
        print "after:${hook.op}"
    end
end
command go
    do
        write_file "` + dir + `/a.txt" "hi"
    end
end
`
	out, err := runWithHooks(t, src, "go")
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	if !strings.Contains(out, "before:"+dir) {
		t.Errorf("before-hook didn't see the write target; out=%q", out)
	}
	if !strings.Contains(out, "after:write_file") {
		t.Errorf("after-hook didn't fire with hook.op; out=%q", out)
	}
}

// A `before` hook that fails VETOES the op — the write never happens.
func TestE2E_Hooks_BeforeVetoes(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "blocked.txt")
	src := `name "x"
hooks
    before write guard
end
requires
    write "` + dir + `"
end
command guard
    do
        fail "denied: ${hook.target}"
    end
end
command go
    do
        write_file "` + target + `" "x"
        print "reached"
    end
end
`
	out, err := runWithHooks(t, src, "go")
	if err == nil {
		t.Fatal("expected the before-hook veto to abort the op")
	}
	if strings.Contains(out, "reached") {
		t.Errorf("op continued after a before-hook veto; out=%q", out)
	}
	if _, statErr := os.Stat(target); statErr == nil {
		t.Errorf("vetoed write still created the file %s", target)
	}
}

// A hook handler's own ops must NOT recursively fire the same hook
// (re-entrancy guard): a `before write` hook that itself writes runs exactly
// once, not forever.
func TestE2E_Hooks_NoReentrancy(t *testing.T) {
	dir := t.TempDir()
	log := filepath.Join(dir, "audit.log")
	src := `name "x"
hooks
    before write log_it
end
requires
    write "` + dir + `"
end
command log_it
    do
        append_line "` + log + `" "w"
    end
end
command go
    do
        write_file "` + dir + `/data.txt" "payload"
    end
end
`
	if _, err := runWithHooks(t, src, "go"); err != nil {
		t.Fatalf("run: %v", err)
	}
	data, err := os.ReadFile(log)
	if err != nil {
		t.Fatalf("hook didn't run (no audit log): %v", err)
	}
	if n := strings.Count(strings.TrimSpace(string(data)), "\n") + 1; n != 1 {
		t.Errorf("re-entrancy: hook ran %d times, want 1:\n%s", n, data)
	}
}

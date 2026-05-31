package ops

import (
	"os/exec"
	"runtime"
	"strings"
	"testing"

	"github.com/olivierdevelops/perch/domain"
	"github.com/olivierdevelops/perch/infra/interpreter"
)

// A declared subprocess must NOT inherit the full host environment: only the
// default operational set (PATH, …), env vars declared via `requires env`, and
// the program's own bindings reach it. An undeclared secret is scrubbed — the
// core fix for the subprocess `vars` escape.
func TestApplyEnv_ScrubsUndeclaredSecrets(t *testing.T) {
	t.Setenv("PERCH_TEST_SECRET", "shhh")    // never declared → must be dropped
	t.Setenv("PERCH_TEST_DECLARED", "okval")  // declared via requires env → kept

	prog := &domain.Program{
		Requirements: domain.Requirements{
			Declared: true,
			Envs:     []domain.EnvReq{{Name: "PERCH_TEST_DECLARED"}},
		},
	}
	i := &interpreter.Interpreter{Program: prog}
	b := &interpreter.Bindings{
		Vars: map[string]any{"BUILD_DIR": "./out"}, // uppercase binding → exported
		Env:  map[string]string{},
	}

	cmd := exec.Command("true")
	applyEnv(i, cmd, b)
	env := strings.Join(cmd.Env, "\n")

	if strings.Contains(env, "PERCH_TEST_SECRET") {
		t.Errorf("undeclared secret leaked into subprocess env:\n%s", env)
	}
	if !strings.Contains(env, "PERCH_TEST_DECLARED=okval") {
		t.Errorf("declared `requires env` var did not reach subprocess:\n%s", env)
	}
	if !strings.Contains(env, "BUILD_DIR=./out") {
		t.Errorf("uppercase binding not exported to subprocess:\n%s", env)
	}
	// The default operational set must still pass so tools can run.
	pathName := "PATH="
	if runtime.GOOS == "windows" {
		pathName = "PATH=" // Windows env is case-insensitive; Go normalizes to PATH
	}
	if !strings.Contains(env, pathName) {
		t.Errorf("PATH (operational default) missing from subprocess env:\n%s", env)
	}
}

// With no requires manifest AND no --env, legacy inherit-all is preserved (a
// hand-built Program with Declared==false — e.g. an embedded/test program).
func TestApplyEnv_LegacyInheritWhenUndeclared(t *testing.T) {
	t.Setenv("PERCH_TEST_AMBIENT", "visible")
	prog := &domain.Program{Requirements: domain.Requirements{Declared: false}}
	i := &interpreter.Interpreter{Program: prog}
	b := &interpreter.Bindings{Vars: map[string]any{}, Env: map[string]string{}}

	cmd := exec.Command("true")
	applyEnv(i, cmd, b)
	if !strings.Contains(strings.Join(cmd.Env, "\n"), "PERCH_TEST_AMBIENT=visible") {
		t.Errorf("undeclared program should inherit host env (legacy), but didn't")
	}
}

package validate

import (
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
)

func mkProg(t *testing.T) *domain.Program {
	t.Helper()
	idx0 := 0
	return &domain.Program{
		Commands: map[string]*domain.Command{
			"build": {
				Name:        "build",
				Description: "Build the binary",
				Args: []domain.ArgSpec{
					{Name: "target", Type: "string", Default: "darwin", HasDefault: true, Description: "Target OS"},
					{Name: "path", Type: "string", Index: &idx0, Description: "Output path"},
				},
				Modifiers: domain.Modifiers{OnSignal: "cleanup"},
				Ops: []domain.Op{
					{Kind: "print", Args: map[string]any{"msg": "Building for ${target} into ${path}"}},
					{Kind: "shell", Args: map[string]any{"cmd": "echo ${target} ${HOME}"}},
				},
			},
			"cleanup": {Name: "cleanup", Description: "cleanup", Modifiers: domain.Modifiers{Private: true}},
		},
	}
}

func known(kinds ...string) map[string]struct{} {
	m := map[string]struct{}{}
	for _, k := range kinds {
		m[k] = struct{}{}
	}
	return m
}

func TestCleanProgram(t *testing.T) {
	p := mkProg(t)
	issues := Check(p, known("print", "shell"))
	for _, i := range issues {
		if i.Severity == "error" {
			t.Errorf("unexpected error: %s", i)
		}
	}
}

func TestUnknownOp(t *testing.T) {
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name:        "x",
				Description: "d",
				Ops:         []domain.Op{{Kind: "blorp", Args: map[string]any{}}},
			},
		},
	}
	issues := Check(p, known("print"))
	if !hasErr(issues, "unknown op kind") {
		t.Errorf("expected unknown-op error; got %v", issues)
	}
}

func TestUnknownPlaceholder(t *testing.T) {
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name:        "x",
				Description: "d",
				Ops:         []domain.Op{{Kind: "print", Args: map[string]any{"msg": "hi ${nope}"}}},
			},
		},
	}
	issues := Check(p, known("print"))
	if !hasErr(issues, "${nope}") {
		t.Errorf("expected unknown-placeholder error; got %v", issues)
	}
}

func TestEnvLookingPlaceholderPasses(t *testing.T) {
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name:        "x",
				Description: "d",
				Ops:         []domain.Op{{Kind: "print", Args: map[string]any{"msg": "PATH=${PATH}"}}},
			},
		},
	}
	issues := Check(p, known("print"))
	for _, i := range issues {
		if i.Severity == "error" {
			t.Errorf("expected no error for env-looking ${PATH}; got %s", i)
		}
	}
}

func TestRunTargetMissing(t *testing.T) {
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name:        "x",
				Description: "d",
				Ops:         []domain.Op{{Kind: "run", Args: map[string]any{"target": "ghost"}}},
			},
		},
	}
	issues := Check(p, known("run"))
	if !hasErr(issues, "run ghost") {
		t.Errorf("expected run-target error; got %v", issues)
	}
}

func TestOnSignalMissing(t *testing.T) {
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name:        "x",
				Description: "d",
				Modifiers:   domain.Modifiers{OnSignal: "ghost"},
				Ops:         []domain.Op{{Kind: "print", Args: map[string]any{"msg": "hi"}}},
			},
		},
	}
	issues := Check(p, known("print"))
	if !hasErr(issues, "on_signal") {
		t.Errorf("expected on_signal error; got %v", issues)
	}
}

func TestArgValidation(t *testing.T) {
	cases := []struct {
		name      string
		arg       domain.ArgSpec
		wantError string
	}{
		{"bad type", domain.ArgSpec{Name: "a", Type: "strring"}, "unknown type"},
		{"default mismatch", domain.ArgSpec{Name: "a", Type: "int", Default: "hello", HasDefault: true}, "does not match"},
		{"missing type", domain.ArgSpec{Name: "a"}, "missing `type`"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			p := &domain.Program{Commands: map[string]*domain.Command{
				"x": {
					Name: "x", Description: "d",
					Args: []domain.ArgSpec{tc.arg},
					Ops:  []domain.Op{{Kind: "print", Args: map[string]any{"msg": "hi"}}},
				},
			}}
			issues := Check(p, known("print"))
			if !hasErr(issues, tc.wantError) {
				t.Errorf("want %q in issues; got %v", tc.wantError, issues)
			}
		})
	}
}

func TestDuplicateArgIndex(t *testing.T) {
	idx0 := 0
	idx0b := 0
	p := &domain.Program{Commands: map[string]*domain.Command{
		"x": {
			Name: "x", Description: "d",
			Args: []domain.ArgSpec{
				{Name: "a", Type: "string", Index: &idx0},
				{Name: "b", Type: "string", Index: &idx0b},
			},
			Ops: []domain.Op{{Kind: "print", Args: map[string]any{"msg": "hi"}}},
		},
	}}
	issues := Check(p, known("print"))
	if !hasErr(issues, "collides") {
		t.Errorf("expected index-collision error; got %v", issues)
	}
}

func TestLetCaptureFlowsToNextOp(t *testing.T) {
	p := &domain.Program{Commands: map[string]*domain.Command{
		"x": {
			Name: "x", Description: "d",
			Ops: []domain.Op{
				{Kind: "upper", Args: map[string]any{"_0": "hi"}, CaptureInto: "U"},
				{Kind: "print", Args: map[string]any{"msg": "got ${U}"}}, // should resolve
			},
		},
	}}
	issues := Check(p, known("upper", "print"))
	for _, i := range issues {
		if i.Severity == "error" {
			t.Errorf("unexpected error after let-capture: %s", i)
		}
	}
}

func TestBlockOpRecurses(t *testing.T) {
	p := &domain.Program{Commands: map[string]*domain.Command{
		"x": {
			Name: "x", Description: "d",
			Ops: []domain.Op{
				{Kind: "if", Args: map[string]any{"op": "eq", "lhs": "os", "rhs": "darwin"}, Body: []domain.Op{
					{Kind: "print", Args: map[string]any{"msg": "${nope}"}},
				}},
			},
		},
	}}
	issues := Check(p, known("if", "print"))
	if !hasErr(issues, "${nope}") {
		t.Errorf("expected placeholder error inside block; got %v", issues)
	}
}

func hasErr(issues []Issue, substr string) bool {
	for _, i := range issues {
		if i.Severity == "error" && strings.Contains(i.Message, substr) {
			return true
		}
	}
	return false
}

func TestRequiresStaticEnforcement(t *testing.T) {
	p := &domain.Program{
		Requirements: domain.Requirements{
			Declared: true,
			Bins:     []domain.BinReq{{Name: "echo"}},
			Hosts:    []domain.HostReq{{Name: "api.github.com"}},
			Envs:     []domain.EnvReq{{Name: "HOME"}},
		},
		Commands: map[string]*domain.Command{
			"x": {
				Name: "x", Description: "d",
				Ops: []domain.Op{
					{Kind: "shell", Args: map[string]any{"cmd": "curl https://evil.example.com"}},
					{Kind: "http_get", Args: map[string]any{"_0": "https://untrusted.org/x"}, CaptureInto: "b"},
					{Kind: "get_env", Args: map[string]any{"_0": "AWS_SECRET_ACCESS_KEY"}, CaptureInto: "k"},
					// These are declared / dynamic — must NOT error:
					{Kind: "shell", Args: map[string]any{"cmd": "echo ok"}},
					{Kind: "http_get", Args: map[string]any{"_0": "https://api.github.com/x"}, CaptureInto: "c"},
					{Kind: "get_env", Args: map[string]any{"_0": "HOME"}, CaptureInto: "h"},
					{Kind: "shell", Args: map[string]any{"cmd": "${os}"}}, // dynamic (auto-bound) → skip
				},
			},
		},
	}
	issues := Check(p, known("shell", "http_get", "get_env"))
	if !hasErr(issues, `bin "curl"`) {
		t.Errorf("expected bin_not_declared for curl; got %v", issues)
	}
	if !hasErr(issues, `host "untrusted.org"`) {
		t.Errorf("expected host_not_declared for untrusted.org; got %v", issues)
	}
	if !hasErr(issues, `"AWS_SECRET_ACCESS_KEY"`) {
		t.Errorf("expected env_not_declared for AWS_SECRET_ACCESS_KEY; got %v", issues)
	}
	// Exactly 3 errors — declared + dynamic usages must not add more.
	errs := 0
	for _, i := range issues {
		if i.Severity == "error" {
			errs++
		}
	}
	if errs != 3 {
		t.Errorf("expected exactly 3 errors, got %d: %v", errs, issues)
	}
}

func TestRequiresNotDeclaredNoEnforcement(t *testing.T) {
	// No requires block → legacy behavior, undeclared shell bins are fine.
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name: "x", Description: "d",
				Ops: []domain.Op{
					{Kind: "shell", Args: map[string]any{"cmd": "curl https://anywhere.com"}},
				},
			},
		},
	}
	issues := Check(p, known("shell"))
	if hasErr(issues, "not declared") {
		t.Errorf("no requires block should mean no enforcement; got %v", issues)
	}
}


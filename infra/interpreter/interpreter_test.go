package interpreter

import (
	"bytes"
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
)

// makeHandlers builds a small handler set for testing without dragging in the
// full ops package (which would create an import cycle).
func makeHandlers(t *testing.T) (map[string]Handler, *bytes.Buffer) {
	t.Helper()
	var out bytes.Buffer
	return map[string]Handler{
		"print": func(i *Interpreter, b *Bindings, args map[string]any) (any, error) {
			msg, _ := args["msg"].(string)
			i.Stdout.Write([]byte(msg + "\n"))
			return nil, nil
		},
		"upper": func(i *Interpreter, b *Bindings, args map[string]any) (any, error) {
			v, _ := args["_0"].(string)
			return strings.ToUpper(v), nil
		},
		"if": func(i *Interpreter, b *Bindings, args map[string]any) (any, error) {
			// Minimal test stub: matches "eq" via direct args.lhs/args.rhs.
			lhs, _ := args["lhs"].(string)
			rhs, _ := args["rhs"].(string)
			lv, _ := b.Lookup(lhs)
			if lv == rhs {
				if body, ok := args["_body"].([]domain.Op); ok {
					return nil, i.RunOps(body, b)
				}
			}
			return nil, nil
		},
	}, &out
}

func TestRunCommandWithDefaultArg(t *testing.T) {
	handlers, out := makeHandlers(t)
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"greet": {
				Name: "greet",
				Args: []domain.ArgSpec{
					{Name: "name", Type: "string", Default: "world", HasDefault: true},
				},
				Ops: []domain.Op{
					{Kind: "print", Args: map[string]any{"msg": "Hello ${name}"}},
				},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	if err := i.Run("greet", []string{}); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "Hello world") {
		t.Errorf("output: %q", out.String())
	}
}

func TestRunCommandWithArg(t *testing.T) {
	handlers, out := makeHandlers(t)
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"greet": {
				Name: "greet",
				Args: []domain.ArgSpec{
					{Name: "name", Type: "string", Default: "world", HasDefault: true},
				},
				Ops: []domain.Op{
					{Kind: "print", Args: map[string]any{"msg": "Hello ${name}"}},
				},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	if err := i.Run("greet", []string{"-name=Alice"}); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "Hello Alice") {
		t.Errorf("output: %q", out.String())
	}
}

func TestMissingRequiredArg(t *testing.T) {
	handlers, out := makeHandlers(t)
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name: "x",
				Args: []domain.ArgSpec{
					{Name: "must", Type: "string"},
				},
				Ops: []domain.Op{},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	err := i.Run("x", []string{})
	if err == nil || !strings.Contains(err.Error(), "missing required") {
		t.Errorf("want missing-required error, got %v", err)
	}
}

func TestLetCaptureFlowsToNextOp(t *testing.T) {
	handlers, out := makeHandlers(t)
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"x": {
				Name: "x",
				Ops: []domain.Op{
					{Kind: "upper", Args: map[string]any{"_0": "hi"}, CaptureInto: "U"},
					{Kind: "print", Args: map[string]any{"msg": "got ${U}"}},
				},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	if err := i.Run("x", nil); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "got HI") {
		t.Errorf("output: %q", out.String())
	}
}

func TestCatch(t *testing.T) {
	handlers, out := makeHandlers(t)
	p := &domain.Program{
		Commands: map[string]*domain.Command{},
		Catch: &domain.Catch{
			Bind: "name",
			Ops: []domain.Op{
				{Kind: "print", Args: map[string]any{"msg": "huh: ${name}"}},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	if err := i.Run("blorp", nil); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "huh: blorp") {
		t.Errorf("output: %q", out.String())
	}
}

func TestCatchProxyArgs(t *testing.T) {
	// catch should expose the full unknown invocation as ${proxy_args}
	// so wrappers can forward to an underlying tool.
	handlers, out := makeHandlers(t)
	p := &domain.Program{
		Commands: map[string]*domain.Command{},
		Catch: &domain.Catch{
			Bind: "name",
			Ops: []domain.Op{
				{Kind: "print", Args: map[string]any{"msg": "→ ${proxy_args}"}},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	if err := i.Run("log", []string{"--oneline", "-10"}); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "→ log --oneline -10") {
		t.Errorf("output: %q", out.String())
	}
}

func TestPrivateCommandFallsToCatch(t *testing.T) {
	handlers, out := makeHandlers(t)
	p := &domain.Program{
		Commands: map[string]*domain.Command{
			"hidden": {Name: "hidden", Modifiers: domain.Modifiers{Private: true}},
		},
		Catch: &domain.Catch{
			Bind: "name",
			Ops: []domain.Op{
				{Kind: "print", Args: map[string]any{"msg": "caught: ${name}"}},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	if err := i.Run("hidden", nil); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "caught: hidden") {
		t.Errorf("output: %q", out.String())
	}
}

func TestBlockOpRunsBody(t *testing.T) {
	handlers, out := makeHandlers(t)
	// The stub `if` handler resolves lhs via bindings. Auto-bound `os`
	// is convenient: compare it to itself via a host-derived value.
	p := &domain.Program{
		Globals: domain.Globals{Bindings: []domain.GlobalBinding{
			{Name: "mode", Type: "string", Value: "yes"},
		}},
		Commands: map[string]*domain.Command{
			"x": {
				Name: "x",
				Ops: []domain.Op{
					{
						Kind: "if",
						Args: map[string]any{"lhs": "mode", "rhs": "yes"},
						Body: []domain.Op{
							{Kind: "print", Args: map[string]any{"msg": "matched"}},
						},
					},
				},
			},
		},
	}
	i := New(handlers, p)
	i.Stdout = out
	if err := i.Run("x", nil); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "matched") {
		t.Errorf("output: %q", out.String())
	}
}

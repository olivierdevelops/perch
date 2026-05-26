package capyloader

import (
	"testing"
)

func TestLoadMinimal(t *testing.T) {
	src := `name "app"
about "hi"
version "0.1"
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	if p.Name != "app" {
		t.Errorf("Name: want %q got %q", "app", p.Name)
	}
	if p.Description != "hi" {
		t.Errorf("Description: want %q got %q", "hi", p.Description)
	}
	if p.Version != "0.1" {
		t.Errorf("Version: want %q got %q", "0.1", p.Version)
	}
}

func TestGlobals(t *testing.T) {
	src := `name "x"
globals
    flag = true
    n = 42
    s = "hello"
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]string{"flag": "bool", "n": "int", "s": "string"}
	got := map[string]string{}
	for _, b := range p.Globals.Bindings {
		got[b.Name] = b.Type
	}
	for k, v := range want {
		if got[k] != v {
			t.Errorf("global %q: type want %q got %q", k, v, got[k])
		}
	}
}

func TestCommandWithArgsAndOps(t *testing.T) {
	src := `name "x"
command build
    description "compile"
    arg target string "Target OS"
    arg_default target "darwin"
    private
    do
        print "Building ${target}"
        shell "go build"
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	c, ok := p.Commands["build"]
	if !ok {
		t.Fatal("command build missing")
	}
	if c.Description != "compile" {
		t.Errorf("Description: want %q got %q", "compile", c.Description)
	}
	if !c.Modifiers.Private {
		t.Error("Private modifier not set")
	}
	if len(c.Args) != 1 || c.Args[0].Name != "target" || c.Args[0].Type != "string" {
		t.Errorf("Args: %+v", c.Args)
	}
	if !c.Args[0].HasDefault || c.Args[0].Default != "darwin" {
		t.Errorf("default: hasDefault=%v default=%v", c.Args[0].HasDefault, c.Args[0].Default)
	}
	if len(c.Ops) != 2 {
		t.Fatalf("Ops: want 2 got %d", len(c.Ops))
	}
	if c.Ops[0].Kind != "print" || c.Ops[1].Kind != "shell" {
		t.Errorf("Op kinds: %s, %s", c.Ops[0].Kind, c.Ops[1].Kind)
	}
}

func TestNestedBlockOps(t *testing.T) {
	src := `name "x"
command setup
    do
        if_os "darwin"
            shell "brew install jq"
        end
        if_os "linux"
            shell "apt install jq"
        end
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	c := p.Commands["setup"]
	if len(c.Ops) != 2 {
		t.Fatalf("Ops: want 2 got %d", len(c.Ops))
	}
	for i, op := range c.Ops {
		if op.Kind != "if_os" {
			t.Errorf("ops[%d].Kind: want if_os got %s", i, op.Kind)
		}
		if len(op.Body) != 1 || op.Body[0].Kind != "shell" {
			t.Errorf("ops[%d].Body: want one shell, got %+v", i, op.Body)
		}
	}
}

func TestLetCapture(t *testing.T) {
	src := `name "x"
command t
    do
        let h = sha256_file "./bin"
        let report = json_count "{}" "[?]"
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	c := p.Commands["t"]
	if len(c.Ops) != 2 {
		t.Fatalf("want 2 ops, got %d", len(c.Ops))
	}
	if c.Ops[0].Kind != "sha256_file" || c.Ops[0].CaptureInto != "h" {
		t.Errorf("op[0]: %+v", c.Ops[0])
	}
	if c.Ops[1].Kind != "json_count" || c.Ops[1].CaptureInto != "report" {
		t.Errorf("op[1]: %+v", c.Ops[1])
	}
}

func TestCatch(t *testing.T) {
	src := `name "x"
catch unknown
    description "fallback"
    do
        print "no such: ${unknown}"
        exit 1
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	if p.Catch == nil {
		t.Fatal("catch missing")
	}
	if p.Catch.Bind != "unknown" {
		t.Errorf("Bind: want unknown, got %q", p.Catch.Bind)
	}
	if p.Catch.Description != "fallback" {
		t.Errorf("Description: %q", p.Catch.Description)
	}
	if len(p.Catch.Ops) != 2 {
		t.Errorf("ops len: %d", len(p.Catch.Ops))
	}
}

func TestParseError(t *testing.T) {
	src := `command foo
    blorp_unknown "x"
end
`
	if _, err := LoadFromString(src); err == nil {
		t.Error("expected parse error for unknown function")
	}
}

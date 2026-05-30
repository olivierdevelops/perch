package opcatalog

import (
	"strings"
	"testing"

	"github.com/olivierdevelops/perch/infra/ops"
)

func TestEveryOpHasExample(t *testing.T) {
	c := Build(ops.BuiltinKinds())
	for _, o := range c.Ops {
		if strings.TrimSpace(o.Example) == "" {
			t.Errorf("op %q missing example", o.Name)
		}
	}
}

func TestEveryVarHasExample(t *testing.T) {
	c := Build([]string{"print"})
	for _, v := range c.Vars {
		if strings.TrimSpace(v.Example) == "" {
			t.Errorf("var %q missing example", v.Name)
		}
	}
}

func TestExampleForKnownOps(t *testing.T) {
	c := Build([]string{"print", "exec", "write_file", "if", "http_get"})
	byName := map[string]Op{}
	for _, o := range c.Ops {
		byName[o.Name] = o
	}
	if !strings.Contains(byName["print"].Example, `print "`) {
		t.Fatalf("print: %q", byName["print"].Example)
	}
	if !strings.Contains(byName["exec"].Example, "exec git") {
		t.Fatalf("exec: %q", byName["exec"].Example)
	}
	if !strings.Contains(byName["if"].Example, "if count") {
		t.Fatalf("if: %q", byName["if"].Example)
	}
	if !strings.Contains(byName["http_get"].Example, "http_get \"https://") {
		t.Fatalf("http_get: %q", byName["http_get"].Example)
	}
}

func TestExampleForVar(t *testing.T) {
	c := Build([]string{"print"})
	for _, v := range c.Vars {
		if v.Name == "script_dir" && !strings.Contains(v.Example, "${script_dir}") {
			t.Fatalf("script_dir: %q", v.Example)
		}
		if v.Type == "bool" && v.Name == "is_linux" && !strings.HasPrefix(v.Example, "if is_linux") {
			t.Fatalf("is_linux: %q", v.Example)
		}
	}
}

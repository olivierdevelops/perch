package opcatalog

import (
	"strings"
	"testing"
)

func TestBuildIncludesKnownOps(t *testing.T) {
	c := Build([]string{"print", "exec", "http_get", "mkdir"})
	if len(c.Ops) != 4 {
		t.Fatalf("want 4 ops, got %d", len(c.Ops))
	}
	byName := map[string]Op{}
	for _, o := range c.Ops {
		byName[o.Name] = o
	}
	if !byName["print"].Requirements.Pure {
		t.Error("print should be pure")
	}
	if !byName["exec"].Requirements.Bin {
		t.Error("exec should require bin")
	}
	if !byName["http_get"].Requirements.Host {
		t.Error("http_get should require host")
	}
	if !byName["mkdir"].Requirements.Write {
		t.Error("mkdir should require write")
	}
}

func TestParseCapyArgsPrint(t *testing.T) {
	args := parseCapyArgs()["print"]
	if len(args) == 0 {
		t.Fatal("print should have args from grammar")
	}
	found := false
	for _, a := range args {
		if a.Name == "msg" && a.Type == "string" {
			found = true
		}
	}
	if !found {
		t.Fatalf("print args: %+v", args)
	}
}

func TestMarshalJSONValid(t *testing.T) {
	data, err := MarshalJSON([]string{"print", "shell"})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), `"schema": "perch.ops.v1"`) {
		t.Fatalf("missing schema: %s", data)
	}
}

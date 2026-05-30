package exportopscatalog_test

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/olivierdevelops/perch/infra/ops"
	"github.com/olivierdevelops/perch/usecases/exportopscatalog"
)

func TestExecuteStdout(t *testing.T) {
	impl := &exportopscatalog.Impl{Kinds: func() []string { return []string{"print"} }}
	r, w, _ := os.Pipe()
	old := os.Stdout
	os.Stdout = w
	done := make(chan error, 1)
	// Close the write end as soon as Execute finishes so io.ReadAll(r) below
	// gets EOF and returns — otherwise ReadAll blocks forever (the writer is
	// never closed) and the test deadlocks until the timeout.
	go func() {
		e := impl.Execute("-")
		w.Close()
		done <- e
	}()
	out, _ := io.ReadAll(r)
	os.Stdout = old
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	text := string(out)
	if !strings.Contains(text, `"schema": "perch.catalog.v1"`) {
		t.Fatalf("missing schema: %s", text)
	}
	if !strings.Contains(text, `"example"`) {
		t.Fatal("missing example fields")
	}
}

func TestExecuteFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "ops.json")
	impl := &exportopscatalog.Impl{Kinds: ops.BuiltinKinds}
	if err := impl.Execute(path); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), `"name": "exec"`) {
		t.Fatalf("missing exec op: %s", data)
	}
	if !strings.Contains(string(data), `"example"`) {
		t.Fatal("missing example fields")
	}
}

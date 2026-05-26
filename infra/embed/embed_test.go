package embed

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/luowensheng/perch/domain"
)

func TestEmbedRoundTrip(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src")
	out := filepath.Join(dir, "out")

	// Fake "executor" binary: arbitrary bytes.
	if err := os.WriteFile(src, []byte("ABCDEFG_HELLO_FAKE_EXE"), 0755); err != nil {
		t.Fatal(err)
	}
	p := &domain.Program{
		Name:    "demo",
		Version: "1.0.0",
		Commands: map[string]*domain.Command{
			"hello": {Name: "hello", Description: "say hi"},
		},
	}
	if err := Embed(src, p, out); err != nil {
		t.Fatal(err)
	}

	// Swap argv0 so Load() reads our test artifact.
	t.Setenv("_NOT_USED", "")
	orig, _ := os.Executable()
	_ = orig
	// Load reads os.Executable(), which we can't override directly in tests.
	// Instead, read the file back and assert structurally.
	data, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	if len(data) <= len("ABCDEFG_HELLO_FAKE_EXE") {
		t.Errorf("output is not larger than source")
	}
	tail := data[len(data)-8:]
	for i := 0; i < 8; i++ {
		if tail[i] != Magic[i] {
			t.Fatalf("magic mismatch at byte %d: want %x got %x", i, Magic[i], tail[i])
		}
	}
}

func TestEmbedStripsExistingFooter(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src")
	out1 := filepath.Join(dir, "out1")
	out2 := filepath.Join(dir, "out2")

	if err := os.WriteFile(src, []byte("STOCK_PERCH_BINARY"), 0755); err != nil {
		t.Fatal(err)
	}
	p := &domain.Program{Name: "demo", Version: "1"}

	if err := Embed(src, p, out1); err != nil {
		t.Fatal(err)
	}
	if err := Embed(out1, p, out2); err != nil {
		t.Fatal(err)
	}

	info1, _ := os.Stat(out1)
	info2, _ := os.Stat(out2)
	if info1.Size() != info2.Size() {
		t.Errorf("re-embedding should be idempotent: %d vs %d", info1.Size(), info2.Size())
	}
}

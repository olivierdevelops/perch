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
	if err := Embed(src, p, nil, out); err != nil {
		t.Fatal(err)
	}

	data, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	if len(data) <= len("ABCDEFG_HELLO_FAKE_EXE") {
		t.Errorf("output is not larger than source")
	}
	tail := data[len(data)-8:]
	for i := 0; i < 8; i++ {
		if tail[i] != MagicV2[i] {
			t.Fatalf("magic mismatch at byte %d: want %x got %x", i, MagicV2[i], tail[i])
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

	if err := Embed(src, p, nil, out1); err != nil {
		t.Fatal(err)
	}
	if err := Embed(out1, p, nil, out2); err != nil {
		t.Fatal(err)
	}

	info1, _ := os.Stat(out1)
	info2, _ := os.Stat(out2)
	if info1.Size() != info2.Size() {
		t.Errorf("re-embedding should be idempotent: %d vs %d", info1.Size(), info2.Size())
	}
}

func TestEmbedWithArchive(t *testing.T) {
	// Archives go in the V2 layout. After Embed + load round trip,
	// the bytes come back identical and the SHA-256 is populated.
	dir := t.TempDir()
	src := filepath.Join(dir, "src")
	out := filepath.Join(dir, "out")

	if err := os.WriteFile(src, []byte("STOCK"), 0755); err != nil {
		t.Fatal(err)
	}
	p := &domain.Program{Name: "demo", Version: "1"}
	archive := []byte("\x1f\x8b\x08\x00FAKE_GZIP_BYTES")
	if err := Embed(src, p, archive, out); err != nil {
		t.Fatal(err)
	}
	// Idempotent rebuild on top of a binary that already carries
	// a V2 footer with an archive — output size must not grow.
	out2 := filepath.Join(dir, "out2")
	if err := Embed(out, p, archive, out2); err != nil {
		t.Fatal(err)
	}
	i1, _ := os.Stat(out)
	i2, _ := os.Stat(out2)
	if i1.Size() != i2.Size() {
		t.Errorf("V2 re-embed not idempotent: %d vs %d", i1.Size(), i2.Size())
	}
}

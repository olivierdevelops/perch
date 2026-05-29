package ops

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
)

// writeTemp writes content to a temp file and returns its path + sha256 hex.
func writeTemp(t *testing.T, content string) (path, hexsum string) {
	t.Helper()
	dir := t.TempDir()
	path = filepath.Join(dir, "artifact.bin")
	if err := os.WriteFile(path, []byte(content), 0o755); err != nil {
		t.Fatalf("write temp: %v", err)
	}
	sum := sha256.Sum256([]byte(content))
	return path, hex.EncodeToString(sum[:])
}

func TestCheckBinHash_Match(t *testing.T) {
	path, sum := writeTemp(t, "fake-binary-bytes")
	bin := domain.BinReq{Name: "tool", Hash: "sha256:" + sum}
	if err := checkBinHash(bin, path); err != nil {
		t.Fatalf("expected match, got error: %v", err)
	}
	// Case-insensitive on the hex digest.
	binUpper := domain.BinReq{Name: "tool", Hash: "sha256:" + strings.ToUpper(sum)}
	if err := checkBinHash(binUpper, path); err != nil {
		t.Errorf("uppercase hex should still match: %v", err)
	}
}

func TestCheckBinHash_Mismatch(t *testing.T) {
	path, _ := writeTemp(t, "fake-binary-bytes")
	bin := domain.BinReq{Name: "tool", Hash: "sha256:" + strings.Repeat("0", 64)}
	err := checkBinHash(bin, path)
	if err == nil {
		t.Fatal("expected mismatch error, got nil")
	}
	oe, ok := err.(*domain.OpError)
	if !ok {
		t.Fatalf("want *domain.OpError, got %T", err)
	}
	if oe.Kind != domain.ErrRequirementUnmet {
		t.Errorf("kind: want %v, got %v", domain.ErrRequirementUnmet, oe.Kind)
	}
	if !strings.Contains(err.Error(), "hash mismatch") {
		t.Errorf("message should mention mismatch: %v", err)
	}
}

func TestCheckBinHash_BadFormat(t *testing.T) {
	path, _ := writeTemp(t, "x")
	// missing ALGO: prefix
	if err := checkBinHash(domain.BinReq{Name: "t", Hash: "deadbeef"}, path); err == nil {
		t.Error("expected error for hash without ALGO: prefix")
	}
	// unsupported algorithm
	if err := checkBinHash(domain.BinReq{Name: "t", Hash: "md5:deadbeef"}, path); err == nil {
		t.Error("expected error for unsupported algo")
	} else if !strings.Contains(err.Error(), "sha256") {
		t.Errorf("error should name the only supported algo: %v", err)
	}
}

func TestLoadHashFile_Formats(t *testing.T) {
	dir := t.TempDir()
	prog := &domain.Program{ScriptPath: filepath.Join(dir, "commands.perch")}

	cases := []struct {
		name, content, want string
	}{
		{"bare-hex", strings.Repeat("a", 64) + "\n", "sha256:" + strings.Repeat("a", 64)},
		{"prefixed", "sha256:" + strings.Repeat("b", 64), "sha256:" + strings.Repeat("b", 64)},
		{"shasum-line", strings.Repeat("c", 64) + "  ./tool\n", "sha256:" + strings.Repeat("c", 64)},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			fn := filepath.Join(dir, c.name+".sha256")
			if err := os.WriteFile(fn, []byte(c.content), 0o644); err != nil {
				t.Fatal(err)
			}
			// reference relative to the script dir
			got, err := loadHashFile(prog, filepath.Base(fn))
			if err != nil {
				t.Fatalf("loadHashFile: %v", err)
			}
			if got != c.want {
				t.Errorf("want %q, got %q", c.want, got)
			}
		})
	}
}

func TestLoadHashFile_Missing(t *testing.T) {
	prog := &domain.Program{ScriptPath: filepath.Join(t.TempDir(), "commands.perch")}
	if _, err := loadHashFile(prog, "does-not-exist.sha256"); err == nil {
		t.Error("expected error for missing hash_file")
	}
}

func TestPathWithinAny(t *testing.T) {
	cwd := "/work/project"
	roots := []string{"./src", "/abs/data"}

	tests := []struct {
		path string
		want bool
	}{
		{"/work/project/src", true},          // equals a relative root made abs
		{"/work/project/src/main.go", true},  // nested under relative root
		{"/abs/data/file.txt", true},         // nested under abs root
		{"/abs/data", true},                  // equals abs root
		{"/work/project/other", false},       // sibling, not under any root
		{"/abs/database", false},             // prefix string but not a path child
		{"/etc/passwd", false},               // entirely outside
	}
	for _, tt := range tests {
		got := pathWithinAny(filepath.Clean(tt.path), roots, cwd)
		if got != tt.want {
			t.Errorf("pathWithinAny(%q): want %v, got %v", tt.path, tt.want, got)
		}
	}
}

func TestAbsUnder_DotDotEscape(t *testing.T) {
	cwd := "/work/project"
	// A `..` traversal escaping the cwd must NOT be silently accepted by the
	// root check: absUnder cleans it to the parent, which then fails
	// pathWithinAny against a root confined to the project.
	abs := absUnder("../secrets", cwd)
	if abs != "/work/secrets" {
		t.Fatalf("absUnder cleaning: want /work/secrets, got %q", abs)
	}
	if pathWithinAny(abs, []string{"./data"}, cwd) {
		t.Error("a ../ escape must not be considered within ./data")
	}
}

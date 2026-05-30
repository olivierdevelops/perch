// Package installlsp installs the perch-lsp binary onto the user's
// machine via `go install`.
//
// Falls back to a clear actionable error if Go isn't on PATH (we don't
// download pre-built binaries yet — the release workflow produces them
// at github.com/olivierdevelops/perch/releases and a future revision will
// fetch + sha256-verify them).
package installlsp

import (
	"fmt"
	"os"
	"os/exec"
)

type UseCase interface {
	Execute() error
}

type Impl struct{}

// The `@main` ref tracks the bleeding-edge branch. Once releases catch
// up (perch-lsp landed after v0.1.0), this should switch back to
// `@latest`. The release workflow also publishes pre-built perch-lsp
// binaries on each tag.
const modulePath = "github.com/olivierdevelops/perch/cmd/perch-lsp@main"

func (i *Impl) Execute() error {
	goBin, err := exec.LookPath("go")
	if err != nil {
		return fmt.Errorf(
			"`go` not found on PATH. Install Go (https://go.dev/dl/) and retry, " +
				"or download a perch-lsp binary from " +
				"https://github.com/olivierdevelops/perch/releases",
		)
	}
	fmt.Printf("→ go install %s\n", modulePath)
	cmd := exec.Command(goBin, "install", modulePath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = os.Environ()
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("go install failed: %w", err)
	}

	// Best-effort: figure out where it landed and tell the user.
	loc := whereInstalled(goBin)
	if loc != "" {
		fmt.Printf("✓ installed: %s\n", loc)
	} else {
		fmt.Println("✓ perch-lsp installed (check `$(go env GOBIN)` or `$(go env GOPATH)/bin`)")
	}
	return nil
}

func whereInstalled(goBin string) string {
	// Prefer GOBIN if set.
	for _, candidate := range []string{"GOBIN", "GOPATH"} {
		out, err := exec.Command(goBin, "env", candidate).Output()
		if err != nil {
			continue
		}
		dir := trim(string(out))
		if dir == "" {
			continue
		}
		if candidate == "GOPATH" {
			dir = dir + "/bin"
		}
		path := dir + "/perch-lsp"
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	return ""
}

func trim(s string) string {
	for len(s) > 0 && (s[len(s)-1] == '\n' || s[len(s)-1] == ' ') {
		s = s[:len(s)-1]
	}
	return s
}

// Package installvscode installs the perch VS Code extension. It:
//
//   1. ensures `perch-lsp` is installed (via the installlsp use case)
//   2. extracts the embedded extension files into a temp directory
//   3. runs `npm install` (silently) to fetch vscode-languageclient
//   4. runs `vsce package` to produce a .vsix
//   5. runs `code --install-extension <vsix>`
//
// Requires: go, node + npm, and the VS Code `code` CLI on PATH.
package installvscode

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/luowensheng/perch/infra/vscodeext"
)

type UseCase interface {
	Execute() error
}

type InstallLSPFn func() error

type Impl struct {
	InstallLSP InstallLSPFn
}

func (i *Impl) Execute() error {
	if err := requireBinary("node"); err != nil {
		return err
	}
	if err := requireBinary("npm"); err != nil {
		return err
	}
	if err := requireBinary("code"); err != nil {
		return fmt.Errorf("%w\n\nThe VS Code `code` CLI isn't on PATH. From VS Code:\n  Cmd-Shift-P → 'Shell Command: Install code command in PATH'", err)
	}

	if i.InstallLSP != nil {
		fmt.Println("→ Installing perch-lsp")
		if err := i.InstallLSP(); err != nil {
			return fmt.Errorf("install perch-lsp: %w", err)
		}
	}

	tmp, err := os.MkdirTemp("", "perch-vscode-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)

	fmt.Println("→ Extracting embedded extension to", tmp)
	if err := vscodeext.Extract(tmp); err != nil {
		return fmt.Errorf("extract embedded extension: %w", err)
	}

	if err := run(tmp, "npm", "install", "--silent", "--no-audit", "--no-fund"); err != nil {
		return fmt.Errorf("npm install: %w", err)
	}

	if err := run(tmp, "npx", "--yes", "@vscode/vsce", "package",
		"--no-dependencies", "--skip-license", "-o", "perch.vsix"); err != nil {
		return fmt.Errorf("vsce package: %w", err)
	}

	vsix := filepath.Join(tmp, "perch.vsix")
	if err := run(tmp, "code", "--install-extension", vsix, "--force"); err != nil {
		return fmt.Errorf("code --install-extension: %w", err)
	}

	fmt.Println()
	fmt.Println("✓ Installed. Open a .perch file to activate the extension.")
	fmt.Println("  If perch-lsp isn't on your PATH, set perch.lsp.path in VS Code settings.")
	return nil
}

func requireBinary(name string) error {
	if _, err := exec.LookPath(name); err != nil {
		return fmt.Errorf("%q is required but not on PATH", name)
	}
	return nil
}

func run(dir, name string, args ...string) error {
	fmt.Printf("→ %s %v\n", name, args)
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = os.Environ()
	return cmd.Run()
}

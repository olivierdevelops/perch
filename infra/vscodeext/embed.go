// Package vscodeext embeds the VS Code extension source files so that
// `perch --install-vscode` can extract them to a temp directory and
// run npm + vsce + `code --install-extension` without requiring the
// user to have a perch repo checked out.
//
// The canonical copy of these files lives at editors/vscode-perch/.
// This package is a mirror; keep them in sync (a CI guard is on the
// roadmap).
package vscodeext

import (
	"embed"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
)

//go:embed template
var content embed.FS

// Extract copies the embedded VS Code extension into dst (must exist),
// preserving directory structure under `template/`.
func Extract(dst string) error {
	return fs.WalkDir(content, "template", func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel := path[len("template"):] // strip the "template" prefix
		if rel == "" {
			return nil
		}
		out := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(out, 0755)
		}
		if err := os.MkdirAll(filepath.Dir(out), 0755); err != nil {
			return err
		}
		in, err := content.Open(path)
		if err != nil {
			return err
		}
		defer in.Close()
		f, err := os.Create(out)
		if err != nil {
			return fmt.Errorf("create %s: %w", out, err)
		}
		defer f.Close()
		if _, err := io.Copy(f, in); err != nil {
			return err
		}
		return nil
	})
}

// Package initconfig writes a starter commands.perch in the current
// working directory if one does not already exist.
package initconfig

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type UseCase interface {
	Execute(path string) error
}

type Impl struct{}

func (i *Impl) Execute(path string) error {
	if _, err := os.Stat(path); err == nil {
		fmt.Println("File already exists at:", path)
		return nil
	}
	cwd, _ := os.Getwd()
	// The shebang line makes the file directly executable once the user
	// `chmod +x`s it — `./commands.perch hello` runs without typing
	// `perch` first. Capy treats `#` lines as comments, so the shebang
	// has no effect on parsing; it's just a hint to the kernel + a hint
	// to the user that "this file is itself a program."
	data := strings.TrimSpace(fmt.Sprintf(`#!/usr/bin/env perch
name    "%s"
about   "A perch project"
version "0.1.0"

# Shared bindings are declared bare at top level (no globals block).
verbose = false

# The manifest is mandatory. An empty block means "pure ops only, spawns
# nothing"; add bin/host/env/read/write lines as your commands need them.
requires
end

command hello
    description "Say hello"
    do
        print "Hello from perch"
    end
end

command main
    description "Default action — runs when the file is invoked with no command"
    do
        hello
    end
end
`, filepath.Base(cwd))) + "\n"
	if err := os.WriteFile(path, []byte(data), 0755); err != nil {
		return err
	}
	fmt.Printf("✓ wrote %s\n", path)
	fmt.Println()
	fmt.Println("Try:")
	fmt.Printf("  perch -f %s --help\n", path)
	fmt.Printf("  perch -f %s hello\n", path)
	fmt.Println()
	fmt.Println("Or run it as a script (perch must be on $PATH):")
	fmt.Printf("  chmod +x %s\n", path)
	fmt.Printf("  ./%s          # runs the `main` command\n", path)
	fmt.Printf("  ./%s hello    # runs `hello` directly\n", path)
	return nil
}

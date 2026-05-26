// Package initconfig writes a starter commands.capy in the current
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
	data := strings.TrimSpace(fmt.Sprintf(`
name    "%s"
about   "A perch project"
version "0.1.0"

globals
    verbose = false
end

command hello
    description "Say hello"
    do
        print "Hello from {{HOME}}"
    end
end
`, filepath.Base(cwd))) + "\n"
	return os.WriteFile(path, []byte(data), 0644)
}

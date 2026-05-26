// Package runbuild produces a portable, self-extracting perch binary
// with the loaded program embedded.
package runbuild

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"github.com/luowensheng/perch/domain"
)

type UseCase interface {
	Execute(configPath string, args []string) error
}

type LoadFn func(path string) (*domain.Program, error)
type EmbedFn func(sourceBinary string, p *domain.Program, outPath string) error

type Impl struct {
	Load  LoadFn
	Embed EmbedFn
}

func (i *Impl) Execute(configPath string, args []string) error {
	fs := flag.NewFlagSet("build", flag.ExitOnError)
	out := fs.String("o", "", "Output binary path (default: program name from commands.capy)")
	if err := fs.Parse(args); err != nil {
		return err
	}

	p, err := i.Load(configPath)
	if err != nil {
		return err
	}

	outPath := *out
	if outPath == "" {
		outPath = p.Name
		if outPath == "" {
			outPath = "perch-app"
		}
	}

	self, err := os.Executable()
	if err != nil {
		return fmt.Errorf("locate self: %w", err)
	}

	if err := i.Embed(self, p, outPath); err != nil {
		return err
	}

	abs, _ := filepath.Abs(outPath)
	fmt.Println("Built binary:", abs)
	return nil
}

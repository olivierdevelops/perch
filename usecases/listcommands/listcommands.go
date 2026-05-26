// Package listcommands prints a summary of the loaded program.
package listcommands

import (
	"fmt"
	"sort"
	"strings"

	"github.com/luowensheng/perch/domain"
)

type UseCase interface {
	Execute(configPath string) error
}

type LoadFn func(path string) (*domain.Program, error)

type Impl struct {
	Load LoadFn
}

func (i *Impl) Execute(configPath string) error {
	p, err := i.Load(configPath)
	if err != nil {
		return err
	}
	fmt.Printf("Name:        %s\n", p.Name)
	fmt.Printf("Version:     %s\n", p.Version)
	fmt.Printf("Description: %s\n", p.Description)
	fmt.Println(strings.Repeat("─", 70))
	fmt.Println()

	if len(p.Globals.Bindings) > 0 {
		fmt.Println("Globals:")
		for _, g := range p.Globals.Bindings {
			fmt.Printf("  %-20s (%s) = %v\n", g.Name, g.Type, g.Value)
		}
		fmt.Println()
	}

	keys := make([]string, 0, len(p.Commands))
	for k, c := range p.Commands {
		if c.Modifiers.Private {
			continue
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)

	fmt.Printf("Commands (%d):\n\n", len(keys))
	for _, k := range keys {
		c := p.Commands[k]
		fmt.Printf("  ▸ %s\n", k)
		if c.Description != "" {
			for _, line := range strings.Split(c.Description, "\n") {
				if t := strings.TrimSpace(line); t != "" {
					fmt.Printf("      %s\n", t)
				}
			}
		}
		if len(c.Args) > 0 {
			for _, a := range c.Args {
				def := ""
				if a.HasDefault {
					def = fmt.Sprintf(" (default %v)", a.Default)
				}
				fmt.Printf("        -%s %s%s — %s\n", a.Name, a.Type, def, a.Description)
			}
		}
		fmt.Println()
	}
	return nil
}

// Package runcommand executes a named command from a loaded program.
package runcommand

import (
	"fmt"
	"strings"

	"github.com/luowensheng/perch/domain"
)

// UseCase is the consumer-owned protocol.
type UseCase interface {
	Execute(configPath, commandName string, args []string) error
}

type LoadFn func(path string) (*domain.Program, error)
type RunFn func(p *domain.Program, name string, args []string) error
type SuggestFn func(query string, candidates []string) []string

type Impl struct {
	Load    LoadFn
	Run     RunFn
	Suggest SuggestFn // optional: fuzzy-match suggestion provider
}

func (i *Impl) Execute(configPath, name string, args []string) error {
	p, err := i.Load(configPath)
	if err != nil {
		return err
	}
	// If the command doesn't exist AND there's no catch, give a friendly
	// "Did you mean…?" before erroring out.
	if _, ok := p.Commands[name]; !ok && p.Catch == nil {
		if i.Suggest != nil {
			candidates := []string{}
			for n, c := range p.Commands {
				if c != nil && !c.Modifiers.Private {
					candidates = append(candidates, n)
				}
			}
			if matches := i.Suggest(name, candidates); len(matches) > 0 {
				return fmt.Errorf("unknown command %q. Did you mean: %s?",
					name, strings.Join(matches, ", "))
			}
		}
		return fmt.Errorf("unknown command %q. Try `perch --help` to list available commands", name)
	}
	return i.Run(p, name, args)
}

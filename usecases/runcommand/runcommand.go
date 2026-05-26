// Package runcommand executes a named command from a loaded program.
package runcommand

import "github.com/luowensheng/perch/domain"

// UseCase is the consumer-owned protocol.
type UseCase interface {
	Execute(configPath, commandName string, args []string) error
}

type LoadFn func(path string) (*domain.Program, error)
type RunFn func(p *domain.Program, name string, args []string) error

type Impl struct {
	Load LoadFn
	Run  RunFn
}

func (i *Impl) Execute(configPath, name string, args []string) error {
	p, err := i.Load(configPath)
	if err != nil {
		return err
	}
	return i.Run(p, name, args)
}

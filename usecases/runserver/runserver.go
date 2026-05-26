// Package runserver serves a loaded perch program over HTTP.
package runserver

import (
	"flag"

	"github.com/luowensheng/perch/domain"
)

type UseCase interface {
	Execute(configPath string, args []string) error
}

type LoadFn func(path string) (*domain.Program, error)
type ServeFn func(p *domain.Program, host string, port int) error

type Impl struct {
	Load  LoadFn
	Serve ServeFn
}

func (i *Impl) Execute(configPath string, args []string) error {
	fs := flag.NewFlagSet("server", flag.ExitOnError)
	port := fs.Int("port", 10032, "Port")
	host := fs.String("host", "127.0.0.1", "Host")
	if err := fs.Parse(args); err != nil {
		return err
	}
	p, err := i.Load(configPath)
	if err != nil {
		return err
	}
	return i.Serve(p, *host, *port)
}

// Package exportopscatalog writes a JSON catalog of built-in perch ops
// (the statements people use inside .perch files — exec, print, if, …).
package exportopscatalog

import (
	"fmt"
	"os"

	"github.com/luowensheng/perch/infra/opcatalog"
)

type UseCase interface {
	Execute(path string) error
}

type KindsFn func() []string

type Impl struct {
	Kinds KindsFn
}

func (i *Impl) Execute(path string) error {
	if i.Kinds == nil {
		return fmt.Errorf("export ops catalog: no kinds provider")
	}
	data, err := opcatalog.MarshalJSON(i.Kinds())
	if err != nil {
		return err
	}
	if path == "" || path == "-" {
		_, err = os.Stdout.Write(data)
		return err
	}
	return os.WriteFile(path, data, 0644)
}

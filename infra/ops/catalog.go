package ops

import (
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/luowensheng/perch/infra/interpreter"
	"github.com/luowensheng/perch/infra/opcatalog"
)

func registerCatalog(m map[string]interpreter.Handler) {
	m["export_ops_catalog"] = opExportOpsCatalog
}

func opExportOpsCatalog(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	path := argString(args, "path", "_0")
	if path == "" {
		return nil, fmt.Errorf("export_ops_catalog: path argument is required")
	}
	kinds := catalogKinds()
	data, err := opcatalog.MarshalJSON(kinds)
	if err != nil {
		return nil, err
	}
	if err := os.WriteFile(path, data, 0644); err != nil {
		return nil, err
	}
	return path, nil
}

func catalogKinds() []string {
	m := AllHandlers()
	out := make([]string, 0, len(m))
	for k := range m {
		if strings.HasPrefix(k, "_") {
			continue
		}
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

package ops

import (
	"sort"
	"strings"
)

// BuiltinKinds returns sorted names of built-in op handlers exposed to .perch
// authors. Internal handlers (_prefix) are omitted.
func BuiltinKinds() []string {
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

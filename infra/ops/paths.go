// Cross-platform path manipulation ops. These wrap filepath so users
// don't have to think about whether the host uses "/" or "\".
package ops

import (
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"

	"github.com/olivierdevelops/perch/infra/interpreter"
)

func registerPaths(m map[string]interpreter.Handler) {
	// path_join "a" "b" "c"  → "a/b/c" (or "a\b\c" on windows)
	m["path_join"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		parts := collectPositional(args)
		return filepath.Join(parts...), nil
	}
	m["path_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.Dir(argString(args, "path", "_0")), nil
	}
	m["path_base"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.Base(argString(args, "path", "_0")), nil
	}
	m["path_ext"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.Ext(argString(args, "path", "_0")), nil
	}
	m["path_abs"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.Abs(argString(args, "path", "_0"))
	}
	m["path_clean"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.Clean(argString(args, "path", "_0")), nil
	}
	m["path_rel"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.Rel(argString(args, "base", "_0"), argString(args, "target", "_1"))
	}
	m["path_with_ext"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		p := argString(args, "path", "_0")
		ext := argString(args, "ext", "_1")
		if !strings.HasPrefix(ext, ".") && ext != "" {
			ext = "." + ext
		}
		return strings.TrimSuffix(p, filepath.Ext(p)) + ext, nil
	}
	m["is_abs"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.IsAbs(argString(args, "path", "_0")), nil
	}
	m["to_slash"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.ToSlash(argString(args, "path", "_0")), nil
	}
	m["from_slash"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return filepath.FromSlash(argString(args, "path", "_0")), nil
	}

	// expand_path "~/.config/x"  → "/Users/me/.config/x".
	// Also expands a leading "~user" via lookup (best-effort).
	m["expand_path"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		p := argString(args, "path", "_0")
		if p == "" {
			return "", nil
		}
		if strings.HasPrefix(p, "~") {
			rest := p[1:]
			if rest == "" || rest[0] == '/' || rest[0] == filepath.Separator {
				if h, err := os.UserHomeDir(); err == nil {
					return filepath.Join(h, strings.TrimLeft(rest, "/"+string(filepath.Separator))), nil
				}
			}
		}
		// Also expand $VAR / %VAR% style for convenience.
		if runtime.GOOS == "windows" {
			return os.ExpandEnv(p), nil
		}
		return os.ExpandEnv(p), nil
	}
}

// collectPositional returns all _0, _1, _2 … args in order, stopping at
// the first gap. Used by variadic ops like path_join.
func collectPositional(args map[string]any) []string {
	out := []string{}
	for idx := 0; ; idx++ {
		v, ok := args["_"+strconv.Itoa(idx)]
		if !ok {
			break
		}
		out = append(out, interpreter.ToStringValue(v))
	}
	return out
}

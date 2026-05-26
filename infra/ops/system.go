package ops

import (
	"os"
	"path/filepath"
	"runtime"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerSystem(m map[string]interpreter.Handler) {
	m["get_os"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return runtime.GOOS, nil
	}
	m["get_arch"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return runtime.GOARCH, nil
	}
	m["get_env"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return os.Getenv(argString(args, "name", "_0")), nil
	}
	m["set_env"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return nil, os.Setenv(argString(args, "name"), argString(args, "value"))
	}
	m["cwd"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return b.Cwd, nil
	}
	m["temp_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return os.TempDir(), nil
	}
	m["home_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		h, _ := os.UserHomeDir()
		return h, nil
	}
	m["app_data_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		switch runtime.GOOS {
		case "windows":
			return os.Getenv("APPDATA"), nil
		case "darwin":
			h, _ := os.UserHomeDir()
			return filepath.Join(h, "Library", "Application Support"), nil
		default:
			h, _ := os.UserHomeDir()
			return filepath.Join(h, ".local", "share"), nil
		}
	}
	m["cache_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		d, err := os.UserCacheDir()
		return d, err
	}
	m["pid"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return os.Getpid(), nil
	}
	m["user"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return os.Getenv("USER"), nil
	}
}

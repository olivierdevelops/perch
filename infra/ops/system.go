package ops

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

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
	m["config_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return os.UserConfigDir()
	}
	m["data_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		switch runtime.GOOS {
		case "windows":
			return os.Getenv("APPDATA"), nil
		case "darwin":
			h, err := os.UserHomeDir()
			if err != nil {
				return "", err
			}
			return filepath.Join(h, "Library", "Application Support"), nil
		default:
			h, err := os.UserHomeDir()
			if err != nil {
				return "", err
			}
			return filepath.Join(h, ".local", "share"), nil
		}
	}
	m["exe_path"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		p, err := os.Executable()
		if err != nil {
			return "", err
		}
		if resolved, err := filepath.EvalSymlinks(p); err == nil {
			p = resolved
		}
		return p, nil
	}
	m["exe_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		p, err := os.Executable()
		if err != nil {
			return "", err
		}
		if resolved, err := filepath.EvalSymlinks(p); err == nil {
			p = resolved
		}
		return filepath.Dir(p), nil
	}
	m["script_path"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		if i.Program == nil {
			return "", nil
		}
		return i.Program.ScriptPath, nil
	}
	m["script_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		if i.Program == nil || i.Program.ScriptPath == "" {
			return "", nil
		}
		return filepath.Dir(i.Program.ScriptPath), nil
	}
	m["cpu_count"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return runtime.NumCPU(), nil
	}
	// os_version: best-effort string identifying the running OS release.
	// Uses `sw_vers -productVersion` on macOS, `uname -r` on linux,
	// `cmd /c ver` on windows. Returns "" if probing fails.
	m["os_version"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		var cmd *exec.Cmd
		switch runtime.GOOS {
		case "darwin":
			cmd = exec.Command("sw_vers", "-productVersion")
		case "windows":
			cmd = exec.Command("cmd", "/c", "ver")
		default:
			cmd = exec.Command("uname", "-r")
		}
		out, err := cmd.CombinedOutput()
		if err != nil {
			return "", nil
		}
		return strings.TrimSpace(string(out)), nil
	}
	m["env_default"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		name := argString(args, "name", "_0")
		def := argString(args, "default", "_1")
		if v, ok := os.LookupEnv(name); ok && v != "" {
			return v, nil
		}
		return def, nil
	}
	m["env_has"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		_, ok := os.LookupEnv(argString(args, "name", "_0"))
		return ok, nil
	}
	m["path_sep"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return string(filepath.Separator), nil
	}
	m["path_list_sep"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return string(os.PathListSeparator), nil
	}
	m["exe_ext"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		if runtime.GOOS == "windows" {
			return ".exe", nil
		}
		return "", nil
	}
	m["null_device"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		if runtime.GOOS == "windows" {
			return "NUL", nil
		}
		return "/dev/null", nil
	}
}

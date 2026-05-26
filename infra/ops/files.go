package ops

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerFiles(m map[string]interpreter.Handler) {
	m["mkdir"] = opMkdir
	m["cp"] = opCp
	m["mv"] = opMv
	m["rm"] = opRm
	m["cd"] = opCd
	m["chmod"] = opChmod
	m["touch"] = opTouch
	m["write_file"] = opWriteFile
	m["read_file"] = opReadFile
	m["exists"] = opExists
	m["is_dir"] = opIsDir
	m["is_file"] = opIsFile
	m["file_size"] = opFileSize
	m["make_executable"] = opMakeExecutable
	m["ensure_dir"] = opEnsureDir
	m["copy_dir"] = opCopyDir
	m["append_file"] = opAppendFile
	m["append_line"] = opAppendLine
	m["ensure_line_in_file"] = opEnsureLineInFile
	m["replace_in_file"] = opReplaceInFile
	m["backup_file"] = opBackupFile
	m["glob"] = opGlob
	m["list_dir"] = opListDir
	m["symlink"] = opSymlink
	m["read_link"] = opReadLink
	m["mktemp_dir"] = opMktempDir
	m["mktemp_file"] = opMktempFile
}

// resolve joins a path against the bindings' cwd if relative.
func resolve(p string, b *interpreter.Bindings) string {
	if filepath.IsAbs(p) {
		return p
	}
	return filepath.Join(b.Cwd, p)
}

func opMkdir(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	return nil, os.MkdirAll(resolve(argString(args, "path", "_0"), b), 0755)
}

func opCp(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src", "_0"), b)
	dst := resolve(argString(args, "dst", "_1"), b)
	in, err := os.Open(src)
	if err != nil {
		return nil, err
	}
	defer in.Close()
	if err := os.MkdirAll(filepath.Dir(dst), 0755); err != nil {
		return nil, err
	}
	out, err := os.Create(dst)
	if err != nil {
		return nil, err
	}
	defer out.Close()
	_, err = io.Copy(out, in)
	return nil, err
}

func opMv(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	return nil, os.Rename(resolve(argString(args, "src", "_0"), b), resolve(argString(args, "dst", "_1"), b))
}

func opRm(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	return nil, os.RemoveAll(resolve(argString(args, "path", "_0"), b))
}

func opCd(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	b.Cwd = p
	return nil, nil
}

func opChmod(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	mode := argString(args, "mode")
	n, err := strconv.ParseUint(mode, 8, 32)
	if err != nil {
		return nil, fmt.Errorf("chmod: invalid mode %q", mode)
	}
	return nil, os.Chmod(resolve(argString(args, "path"), b), os.FileMode(n))
}

func opTouch(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	f, err := os.OpenFile(p, os.O_RDWR|os.O_CREATE, 0644)
	if err != nil {
		return nil, err
	}
	return nil, f.Close()
}

func opWriteFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path"), b)
	return nil, os.WriteFile(p, []byte(argString(args, "content")), 0644)
}

func opReadFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	data, err := os.ReadFile(p)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

func opExists(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	_, err := os.Stat(p)
	return err == nil, nil
}

func opIsDir(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	info, err := os.Stat(p)
	if err != nil {
		return false, nil
	}
	return info.IsDir(), nil
}

func opIsFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	info, err := os.Stat(p)
	if err != nil {
		return false, nil
	}
	return !info.IsDir(), nil
}

func opFileSize(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	info, err := os.Stat(p)
	if err != nil {
		return int64(0), err
	}
	return info.Size(), nil
}

// opMakeExecutable sets the executable bit on PATH. No-op on Windows
// (where executability is decided by file extension).
func opMakeExecutable(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if runtime.GOOS == "windows" {
		return nil, nil
	}
	p := resolve(argString(args, "path", "_0"), b)
	info, err := os.Stat(p)
	if err != nil {
		return nil, err
	}
	return nil, os.Chmod(p, info.Mode()|0o111)
}

// opEnsureDir creates PATH (and parents) if missing, returns the abs path.
func opEnsureDir(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	if err := os.MkdirAll(p, 0755); err != nil {
		return "", err
	}
	abs, _ := filepath.Abs(p)
	return abs, nil
}

// opCopyDir recursively copies SRC into DST. Symlinks are followed.
func opCopyDir(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src", "_0"), b)
	dst := resolve(argString(args, "dst", "_1"), b)
	return nil, filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		out := filepath.Join(dst, rel)
		if info.IsDir() {
			return os.MkdirAll(out, info.Mode())
		}
		return copyFile(path, out)
	})
}

func opAppendFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	content := argString(args, "content", "_1")
	f, err := os.OpenFile(p, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	_, err = f.WriteString(content)
	return nil, err
}

func opAppendLine(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	line := argString(args, "line", "_1")
	f, err := os.OpenFile(p, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	data, _ := os.ReadFile(p)
	prefix := ""
	if len(data) > 0 && data[len(data)-1] != '\n' {
		prefix = "\n"
	}
	_, err = f.WriteString(prefix + line + "\n")
	return nil, err
}

// opEnsureLineInFile appends LINE only if not already present. Returns
// true if it added the line (false if it was already there). Critical
// for idempotent install scripts editing .bashrc / .gitignore / hosts.
func opEnsureLineInFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	line := argString(args, "line", "_1")
	return ensureLineInFile(p, line)
}

func opReplaceInFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	old := argString(args, "old", "_1")
	new := argString(args, "new", "_2")
	data, err := os.ReadFile(p)
	if err != nil {
		return nil, err
	}
	return nil, os.WriteFile(p, []byte(strings.ReplaceAll(string(data), old, new)), 0644)
}

// opBackupFile copies PATH to PATH.bak (overwriting any existing backup).
func opBackupFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	return p + ".bak", copyFile(p, p+".bak")
}

// opGlob returns matches as newline-separated strings (Bindings stores
// strings, not slices, by convention).
func opGlob(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	pattern := resolve(argString(args, "pattern", "_0"), b)
	matches, err := filepath.Glob(pattern)
	if err != nil {
		return "", err
	}
	return strings.Join(matches, "\n"), nil
}

// opListDir returns entry names in PATH, one per line. Hidden files included.
func opListDir(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	p := resolve(argString(args, "path", "_0"), b)
	entries, err := os.ReadDir(p)
	if err != nil {
		return "", err
	}
	names := make([]string, 0, len(entries))
	for _, e := range entries {
		names = append(names, e.Name())
	}
	return strings.Join(names, "\n"), nil
}

func opSymlink(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	target := argString(args, "target", "_0")
	link := resolve(argString(args, "link", "_1"), b)
	_ = os.Remove(link)
	return nil, os.Symlink(target, link)
}

func opReadLink(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	return os.Readlink(resolve(argString(args, "path", "_0"), b))
}

func opMktempDir(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	prefix := argString(args, "prefix", "_0")
	if prefix == "" {
		prefix = "perch-"
	}
	return os.MkdirTemp("", prefix+"*")
}

func opMktempFile(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	prefix := argString(args, "prefix", "_0")
	if prefix == "" {
		prefix = "perch-"
	}
	f, err := os.CreateTemp("", prefix+"*")
	if err != nil {
		return "", err
	}
	f.Close()
	return f.Name(), nil
}

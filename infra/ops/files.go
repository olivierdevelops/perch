package ops

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"

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

package ops

import (
	"compress/gzip"
	"io"
	"os"

	"github.com/olivierdevelops/perch/infra/interpreter"
)

func registerCompression(m map[string]interpreter.Handler) {
	m["gzip"] = opGzip
	m["ungzip"] = opUngzip
}

func opGzip(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src"), b)
	dst := resolve(argString(args, "dst"), b)
	in, err := os.Open(src)
	if err != nil {
		return nil, err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return nil, err
	}
	defer out.Close()
	gz := gzip.NewWriter(out)
	defer gz.Close()
	_, err = io.Copy(gz, in)
	return nil, err
}

func opUngzip(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src"), b)
	dst := resolve(argString(args, "dst"), b)
	in, err := os.Open(src)
	if err != nil {
		return nil, err
	}
	defer in.Close()
	gz, err := gzip.NewReader(in)
	if err != nil {
		return nil, err
	}
	defer gz.Close()
	out, err := os.Create(dst)
	if err != nil {
		return nil, err
	}
	defer out.Close()
	_, err = io.Copy(out, gz)
	return nil, err
}

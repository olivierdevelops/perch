package ops

import (
	"os"
	"runtime"
	"strconv"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

func registerFlow(m map[string]interpreter.Handler) {
	m["if_os"] = opIfOS
	m["if_arch"] = opIfArch
	m["if_eq"] = opIfEq
	m["if_neq"] = opIfNeq
	m["if_gt"] = opIfGt
	m["if_lt"] = opIfLt
	m["if_exists"] = opIfExists
	m["if_empty"] = opIfEmpty
	m["if_not_empty"] = opIfNotEmpty
}

func runBody(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) error {
	body, ok := args["_body"].([]domain.Op)
	if !ok {
		return nil
	}
	return i.RunOps(body, b)
}

func opIfOS(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if argString(args, "os") == runtime.GOOS {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfArch(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if argString(args, "arch") == runtime.GOARCH {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfEq(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if argString(args, "lhs") == argString(args, "rhs") {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfNeq(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if argString(args, "lhs") != argString(args, "rhs") {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfGt(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if asFloat(args["lhs"]) > asFloat(args["rhs"]) {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfLt(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if asFloat(args["lhs"]) < asFloat(args["rhs"]) {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfExists(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	path := argString(args, "path")
	if _, err := os.Stat(path); err == nil {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfEmpty(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if argString(args, "value", "_0") == "" {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func opIfNotEmpty(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if argString(args, "value", "_0") != "" {
		return nil, runBody(i, b, args)
	}
	return nil, nil
}

func asFloat(v any) float64 {
	switch x := v.(type) {
	case float64:
		return x
	case int:
		return float64(x)
	case int64:
		return float64(x)
	case string:
		f, _ := strconv.ParseFloat(x, 64)
		return f
	case bool:
		if x {
			return 1
		}
		return 0
	}
	return 0
}

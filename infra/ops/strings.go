package ops

import (
	"fmt"
	"strings"
	"unicode"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerStrings(m map[string]interpreter.Handler) {
	m["trim"] = unary(strings.TrimSpace)
	m["lower"] = unary(strings.ToLower)
	m["upper"] = unary(strings.ToUpper)
	m["capitalize"] = unary(func(s string) string {
		if s == "" {
			return s
		}
		r := []rune(s)
		r[0] = unicode.ToUpper(r[0])
		return string(r)
	})
	m["length"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return len(argString(args, "value", "_0")), nil
	}
	m["replace"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		// let x = replace SUBJECT  "OLD,NEW"
		s := argString(args, "value", "_0")
		spec := argString(args, "pattern", "_1")
		if idx := strings.Index(spec, ","); idx >= 0 {
			return strings.ReplaceAll(s, spec[:idx], spec[idx+1:]), nil
		}
		return s, nil
	}
	m["split"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		s := argString(args, "value", "_0")
		sep := argString(args, "sep", "_1")
		return strings.Split(s, sep), nil
	}
	m["join"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		v := args["_0"]
		sep := argString(args, "sep", "_1")
		switch x := v.(type) {
		case []string:
			return strings.Join(x, sep), nil
		case []any:
			parts := make([]string, len(x))
			for i, p := range x {
				parts[i] = interpreter.ToStringValue(p)
			}
			return strings.Join(parts, sep), nil
		case string:
			return x, nil
		}
		return "", nil
	}
	m["contains"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return strings.Contains(argString(args, "value", "_0"), argString(args, "sub", "_1")), nil
	}
	m["has_prefix"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return strings.HasPrefix(argString(args, "value", "_0"), argString(args, "prefix", "_1")), nil
	}
	m["has_suffix"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return strings.HasSuffix(argString(args, "value", "_0"), argString(args, "suffix", "_1")), nil
	}
	m["repeat"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		s := argString(args, "value", "_0")
		n := int(toFloat(args["count"]))
		if n < 0 {
			return "", fmt.Errorf("repeat: negative count")
		}
		return strings.Repeat(s, n), nil
	}
	m["format"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		// `let s = format "Hello %s" "world"` — first arg is the format,
		// second arg is the single value to interpolate.
		fmtStr := argString(args, "format", "_0")
		v := args["_1"]
		return fmt.Sprintf(fmtStr, v), nil
	}
}

func unary(fn func(string) string) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return fn(argString(args, "value", "_0")), nil
	}
}

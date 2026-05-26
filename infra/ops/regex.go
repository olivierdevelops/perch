package ops

import (
	"regexp"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerRegex(m map[string]interpreter.Handler) {
	m["regex_match"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		re, err := regexp.Compile(argString(args, "pattern", "_0"))
		if err != nil {
			return false, err
		}
		return re.MatchString(argString(args, "value", "_1")), nil
	}
	m["regex_replace"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		re, err := regexp.Compile(argString(args, "pattern", "_0"))
		if err != nil {
			return "", err
		}
		return re.ReplaceAllString(argString(args, "value", "_1"), argString(args, "replacement", "_2")), nil
	}
	m["regex_find_all"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		re, err := regexp.Compile(argString(args, "pattern", "_0"))
		if err != nil {
			return nil, err
		}
		return re.FindAllString(argString(args, "value", "_1"), -1), nil
	}
}

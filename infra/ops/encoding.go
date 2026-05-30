package ops

import (
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"net/url"
	"strings"

	"github.com/olivierdevelops/perch/infra/interpreter"
)

func registerEncoding(m map[string]interpreter.Handler) {
	m["base64_encode"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return base64.StdEncoding.EncodeToString([]byte(argString(args, "value", "_0"))), nil
	}
	m["base64_decode"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		v, err := base64.StdEncoding.DecodeString(argString(args, "value", "_0"))
		return string(v), err
	}
	m["hex_encode"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return hex.EncodeToString([]byte(argString(args, "value", "_0"))), nil
	}
	m["hex_decode"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		v, err := hex.DecodeString(argString(args, "value", "_0"))
		return string(v), err
	}
	m["url_encode"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return url.QueryEscape(argString(args, "value", "_0")), nil
	}
	m["url_decode"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return url.QueryUnescape(argString(args, "value", "_0"))
	}
	m["json_parse"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		var v any
		if err := json.Unmarshal([]byte(argString(args, "value", "_0")), &v); err != nil {
			return nil, err
		}
		return v, nil
	}
	m["json_stringify"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		b1, err := json.Marshal(args["_0"])
		return string(b1), err
	}
	m["json_get"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		// let v = json_get DOC "a.b.c"
		v := args["_0"]
		path := argString(args, "path", "_1")
		if s, ok := v.(string); ok {
			var parsed any
			if err := json.Unmarshal([]byte(s), &parsed); err == nil {
				v = parsed
			}
		}
		for _, part := range strings.Split(path, ".") {
			m, ok := v.(map[string]any)
			if !ok {
				return nil, nil
			}
			v = m[part]
		}
		return v, nil
	}
}

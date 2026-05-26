package ops

import (
	"strconv"
	"time"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerTime(m map[string]interpreter.Handler) {
	m["now"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		layout := argString(args, "format", "_0")
		if layout == "" {
			return time.Now().Format(time.RFC3339), nil
		}
		switch layout {
		case "rfc3339":
			return time.Now().Format(time.RFC3339), nil
		case "rfc822":
			return time.Now().Format(time.RFC822), nil
		case "unix":
			return strconv.FormatInt(time.Now().Unix(), 10), nil
		case "unix_milli":
			return strconv.FormatInt(time.Now().UnixMilli(), 10), nil
		case "date":
			return time.Now().Format("2006-01-02"), nil
		case "time":
			return time.Now().Format("15:04:05"), nil
		case "datetime":
			return time.Now().Format("2006-01-02 15:04:05"), nil
		}
		return time.Now().Format(layout), nil
	}
	m["unix_to_iso"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		secs := int64(toFloat(args["_0"]))
		return time.Unix(secs, 0).UTC().Format(time.RFC3339), nil
	}
}

package interpreter

import (
	"fmt"
	"strings"
)

// Interpolate substitutes {{name}} placeholders in s with values from b.
// Unknown names produce an error. Double-open `\{{` is an escape for
// a literal `{{`.
func Interpolate(s string, b *Bindings) (string, error) {
	if !strings.Contains(s, "{{") {
		return s, nil
	}
	var out strings.Builder
	i := 0
	for i < len(s) {
		if i+1 < len(s) && s[i] == '\\' && s[i+1] == '{' {
			out.WriteByte('{')
			i += 2
			continue
		}
		if i+1 < len(s) && s[i] == '{' && s[i+1] == '{' {
			// find matching }}
			end := strings.Index(s[i+2:], "}}")
			if end < 0 {
				return "", fmt.Errorf("unterminated {{ in %q", s)
			}
			name := strings.TrimSpace(s[i+2 : i+2+end])
			v, ok := b.Lookup(name)
			if !ok {
				return "", fmt.Errorf("unknown placeholder {{%s}} in %q", name, s)
			}
			out.WriteString(v)
			i += 2 + end + 2
			continue
		}
		out.WriteByte(s[i])
		i++
	}
	return out.String(), nil
}

// InterpolateArgs returns a fresh map with every string value
// interpolated against b. Non-string values are passed through.
func InterpolateArgs(args map[string]any, b *Bindings) (map[string]any, error) {
	if len(args) == 0 {
		return args, nil
	}
	out := make(map[string]any, len(args))
	for k, v := range args {
		if s, ok := v.(string); ok {
			rs, err := Interpolate(s, b)
			if err != nil {
				return nil, err
			}
			out[k] = rs
			continue
		}
		out[k] = v
	}
	return out, nil
}

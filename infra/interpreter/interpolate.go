package interpreter

import (
	"fmt"
	"strings"
)

// Interpolate substitutes ${name} placeholders in s with values from b.
// Unknown names produce an error. Use `\${name}` (literal backslash) to
// emit a literal `${name}` without substitution — useful for shell
// variables that should reach bash untouched.
func Interpolate(s string, b *Bindings) (string, error) {
	if !strings.Contains(s, "${") {
		return s, nil
	}
	var out strings.Builder
	i := 0
	for i < len(s) {
		// `\${` → emit literal `${`, skip both bytes of the escape and the `{`.
		if i+2 < len(s) && s[i] == '\\' && s[i+1] == '$' && s[i+2] == '{' {
			out.WriteString("${")
			i += 3
			continue
		}
		if i+1 < len(s) && s[i] == '$' && s[i+1] == '{' {
			end := strings.IndexByte(s[i+2:], '}')
			if end < 0 {
				return "", fmt.Errorf("unterminated ${ in %q", s)
			}
			name := strings.TrimSpace(s[i+2 : i+2+end])
			v, ok := b.Lookup(name)
			if !ok {
				// When the env allowlist is active and the name LOOKS like
				// a host env var (uppercase / underscores / digits), say so
				// — that's almost always why it didn't resolve.
				if b.EnvRestricted() && looksLikeEnvName(name) {
					return "", fmt.Errorf(
						"env var ${%s} is not in --env allowlist (declare with --env %s — run `perch help --env` for details)",
						name, name,
					)
				}
				return "", fmt.Errorf("unknown placeholder ${%s} in %q — run `perch help interpolation` for resolution order", name, s)
			}
			out.WriteString(v)
			i += 2 + end + 1
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
	// Universal bare-ident resolution: keys of the form `_NAME_var` carry
	// a binding NAME captured at parse time (via ident-form grammar
	// overloads like `http_get url`). Resolve via b.Lookup and write the
	// value to the canonical `NAME` key so the handler reads it via the
	// usual `argString(args, "NAME", "_0")` call without modification.
	// This is what makes `http_get url` / `print msg` / `shell cmd` etc.
	// work with zero handler-side changes — adding one grammar overload
	// per op surfaces the bare-ident form universally.
	for k, v := range out {
		if !strings.HasPrefix(k, "_") || !strings.HasSuffix(k, "_var") || len(k) <= 5 {
			continue
		}
		name, ok := v.(string)
		if !ok {
			continue
		}
		// Strip leading `_` and trailing `_var` to derive the canonical
		// key. For named args this is `_msg_var → msg`. For positional
		// args the convention is keys of the form `_0`, `_1` (leading
		// underscore is significant), so we must KEEP it when the inner
		// token starts with a digit: `_0_var → _0`.
		canonical := k[1 : len(k)-4]
		if len(canonical) > 0 && canonical[0] >= '0' && canonical[0] <= '9' {
			canonical = "_" + canonical
		}
		if got, ok := b.Lookup(name); ok {
			out[canonical] = got
		} else {
			// Not a binding — fall back to the literal token text, like a CLI:
			// `git rev-parse HEAD` / `replace text foo bar` pass HEAD / foo / bar
			// through verbatim. A defined binding of the same name resolves
			// (above); an undefined bare word is just its own value.
			out[canonical] = name
		}
	}
	return out, nil
}

// looksLikeEnvName: ${PATH} / ${HOME} / ${API_KEY_2} look like env vars
// (all uppercase, possibly with underscores and digits). Anything with
// a lowercase letter is almost certainly a binding name.
func looksLikeEnvName(name string) bool {
	if name == "" {
		return false
	}
	for _, r := range name {
		if r >= 'a' && r <= 'z' {
			return false
		}
	}
	return true
}

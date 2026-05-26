// Package interpreter walks a parsed perch program and dispatches each op
// to a Go handler. Bindings hold the per-invocation runtime state.
package interpreter

import (
	"fmt"
	"os"
	"strconv"
)

// Bindings is the runtime state for one command invocation. Globals,
// command-args, and `let` captures all live in Vars.
type Bindings struct {
	Cwd  string
	Env  map[string]string
	Vars map[string]any
}

// NewBindings constructs a Bindings with empty maps.
func NewBindings(cwd string) *Bindings {
	return &Bindings{
		Cwd:  cwd,
		Env:  map[string]string{},
		Vars: map[string]any{},
	}
}

// Lookup returns the string form of a binding's value (suitable for
// substitution into op args). Resolution order: command bindings (args /
// globals / lets), per-command env, then the host process env (so
// {{HOME}}, {{USER}}, {{PATH}} etc. work out of the box).
func (b *Bindings) Lookup(name string) (string, bool) {
	if v, ok := b.Vars[name]; ok {
		return ToStringValue(v), true
	}
	if v, ok := b.Env[name]; ok {
		return v, true
	}
	if v, ok := os.LookupEnv(name); ok {
		return v, true
	}
	return "", false
}

// Set stores a binding.
func (b *Bindings) Set(name string, v any) {
	b.Vars[name] = v
}

// ToStringValue converts any to its string representation for use in
// interpolation and shell environments.
func ToStringValue(v any) string {
	switch x := v.(type) {
	case nil:
		return ""
	case string:
		return x
	case bool:
		if x {
			return "true"
		}
		return "false"
	case int:
		return strconv.Itoa(x)
	case int64:
		return strconv.FormatInt(x, 10)
	case float64:
		if x == float64(int64(x)) {
			return strconv.FormatInt(int64(x), 10)
		}
		return strconv.FormatFloat(x, 'f', -1, 64)
	default:
		return fmt.Sprintf("%v", v)
	}
}

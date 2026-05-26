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
	// EnvAllowlist, when non-nil, restricts which host env vars resolve
	// via ${NAME} fallthrough. nil = legacy behavior (every host env
	// var visible). Empty non-nil map = no host env vars visible at
	// all. Populated via `perch --env A,B,C`. Auto-bound names (home,
	// cache_dir, …) are NOT env vars and are unaffected.
	EnvAllowlist map[string]bool
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
// ${HOME}, ${USER}, ${PATH} etc. work out of the box).
//
// When EnvAllowlist is non-nil, host-env fallthrough is restricted to
// the listed names. This implements `--env`.
func (b *Bindings) Lookup(name string) (string, bool) {
	if v, ok := b.Vars[name]; ok {
		return ToStringValue(v), true
	}
	if v, ok := b.Env[name]; ok {
		return v, true
	}
	if b.EnvAllowlist != nil && !b.EnvAllowlist[name] {
		// Allowlist is active and this name isn't on it. Do not fall
		// through to the host env, even if the host has the value.
		return "", false
	}
	if v, ok := os.LookupEnv(name); ok {
		return v, true
	}
	return "", false
}

// EnvRestricted reports whether the env allowlist is active. Lets
// callers produce a more helpful error ("env var X is not in --env
// allowlist") instead of the generic "unknown placeholder."
func (b *Bindings) EnvRestricted() bool {
	return b.EnvAllowlist != nil
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

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
	// CapMask is the active capability mask. A `sandbox no_shell …` block
	// pushes a narrower mask onto a stack; on exit the prior mask is
	// restored. Op handlers (shell, http_*, write_*) consult ActiveCaps
	// before doing work. nil means "no in-language gate" (the process-
	// level CLI flags still apply, enforced inside the handlers).
	//
	// The intersection rule is enforced on push: an inner block may
	// disable capabilities, never re-enable them.
	CapMask *CapMask
}

// CapMask is one layer of in-language capability restriction. A nil mask
// means "permit everything (the CLI flags are the only gate)". A non-nil
// mask carries explicit deny flags + narrowed allowlists.
//
// Lookup goes through ActiveCaps which walks the mask chain via Parent.
type CapMask struct {
	NoShell      bool
	NoSubprocess bool
	NoNetwork    bool
	NoWrite      bool
	// AllowedBins, when non-nil, restricts shell to argv[0] in this set.
	// nil = no in-language narrowing (CLI --allow-bin still applies).
	AllowedBins map[string]bool
	// AllowedHosts narrows the network host allowlist further.
	AllowedHosts []string
	// EnvAllow narrows the env-var allowlist further. nil = inherit; empty
	// non-nil set = block all host envs from this layer down.
	EnvAllow map[string]bool
	// ReadOnlyRoots, when non-empty, restricts write ops to paths under
	// these roots. Each is the absolute path of a directory the inner
	// block may write to; anything outside errors.
	ReadOnlyRoots []string
	// Parent is the next mask outward. Lookups walk the chain.
	Parent *CapMask
}

// Push returns a new mask whose state is the intersection of `next` and
// the current chain. Because capabilities can only be narrowed, the
// returned mask carries every restriction from both layers — never widens.
func (m *CapMask) Push(next CapMask) *CapMask {
	next.Parent = m
	return &next
}

// AnyNoShell reports whether ANY mask in the chain forbids shell.
func (m *CapMask) AnyNoShell() bool {
	for cur := m; cur != nil; cur = cur.Parent {
		if cur.NoShell {
			return true
		}
	}
	return false
}

// AnyNoSubprocess reports the same for subprocess ops.
func (m *CapMask) AnyNoSubprocess() bool {
	for cur := m; cur != nil; cur = cur.Parent {
		if cur.NoSubprocess {
			return true
		}
	}
	return false
}

// AnyNoNetwork reports the same for network ops.
func (m *CapMask) AnyNoNetwork() bool {
	for cur := m; cur != nil; cur = cur.Parent {
		if cur.NoNetwork {
			return true
		}
	}
	return false
}

// AnyNoWrite reports the same for FS-write ops.
func (m *CapMask) AnyNoWrite() bool {
	for cur := m; cur != nil; cur = cur.Parent {
		if cur.NoWrite {
			return true
		}
	}
	return false
}

// AllowedBinPermitted reports whether `bin` may be invoked under the
// current mask chain. A non-nil AllowedBins anywhere in the chain
// restricts to that set (intersected across layers).
func (m *CapMask) AllowedBinPermitted(bin string) bool {
	for cur := m; cur != nil; cur = cur.Parent {
		if cur.AllowedBins != nil && !cur.AllowedBins[bin] {
			return false
		}
	}
	return true
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

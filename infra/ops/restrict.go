// Restriction flags disable groups of ops by category. Each `--no-X`
// CLI flag toggles one category; multiple flags compose (additive).
// This replaces the earlier `--mode safe/offline/...` knob — the
// composable flags say exactly what they disable.
//
// A blocked op is replaced with a sentinel handler returning a
// friendly "disabled by --no-X" error. Other ops are untouched.

package ops

import (
	"fmt"
	"sort"
	"strings"

	"github.com/luowensheng/perch/infra/interpreter"
)

// Restriction names. The value of each constant is the CLI flag minus
// the leading `--`, so the same string drives the flag, the error
// message, and the docs.
const (
	RestrictNoShell      = "no-shell"
	RestrictNoSubprocess = "no-subprocess"
	RestrictNoNetwork    = "no-network"
	RestrictNoWrite      = "no-write"
)

// restrictBlocks lists, per restriction, the op kinds it forbids.
var restrictBlocks = map[string][]string{
	RestrictNoShell: {
		// Anything that hands a string to the host shell.
		"shell", "shell_output", "shell_detached", "shell_in", "try_shell",
	},
	RestrictNoSubprocess: {
		// Process management beyond shell — anything that fork/execs
		// without going through the `shell` op. These can leak host env
		// vars, network access, file access into the spawned process
		// just like `shell` can, so they belong to the same category.
		"exec", // shell-free direct subprocess (sandboxed-by-design §3.2)
		"pkg_install", "pkg_uninstall",
		"kill_by_name", "process_running",
		"bin_version",   // runs `BIN --version`
		"os_version",    // runs `sw_vers` / `uname -r` / `cmd /c ver`
	},
	RestrictNoNetwork: {
		"http_get", "http_post", "http_put", "http_delete", "http_status",
		"download",
		"dns_lookup",
		"port_check", "port_free", "find_free_port",
		"wait_for_port", "wait_for_url",
		"public_ip",
		// Network introspection also reveals host facts.
		"local_ip", "mac_address", "interfaces",
	},
	RestrictNoWrite: {
		// Filesystem mutation.
		"write_file", "append_file", "append_line", "ensure_line_in_file",
		"replace_in_file", "backup_file",
		"cp", "mv", "rm", "mkdir", "chmod", "touch",
		"copy_dir", "ensure_dir", "make_executable",
		"symlink", "link_into_path",
		"mktemp_file", "mktemp_dir",
		"add_to_path",
		"tar_extract", "zip_extract", "gzip", "ungzip", "tar_create", "zip_create",
		"bundle_extract", "bundle_dir",
	},
}

// Restrictions is the set of active --no-X flags. Construct via parse-CLI
// then apply once to the handler map.
type Restrictions struct {
	NoShell      bool
	NoSubprocess bool
	NoNetwork    bool
	NoWrite      bool
}

// Active returns true if any restriction is on.
func (r Restrictions) Active() bool {
	return r.NoShell || r.NoSubprocess || r.NoNetwork || r.NoWrite
}

// AsFlags returns the active restrictions as CLI-flag strings, e.g.
// ["--no-shell", "--no-network"]. Used in error messages so the user
// sees the exact flag that blocked their op.
func (r Restrictions) AsFlags() []string {
	out := []string{}
	if r.NoShell {
		out = append(out, "--"+RestrictNoShell)
	}
	if r.NoSubprocess {
		out = append(out, "--"+RestrictNoSubprocess)
	}
	if r.NoNetwork {
		out = append(out, "--"+RestrictNoNetwork)
	}
	if r.NoWrite {
		out = append(out, "--"+RestrictNoWrite)
	}
	return out
}

// ApplyRestrictions mutates handlers in place: every op blocked by one
// of the active restrictions is replaced with a sentinel that returns
// `op "X" is disabled by --no-Y`. Iterates flags in canonical order so
// the FIRST applicable restriction is the one cited in errors (deterministic).
func ApplyRestrictions(handlers map[string]interpreter.Handler, r Restrictions) {
	if !r.Active() {
		return
	}
	order := []struct {
		on   bool
		name string
	}{
		{r.NoShell, RestrictNoShell},
		{r.NoSubprocess, RestrictNoSubprocess},
		{r.NoNetwork, RestrictNoNetwork},
		{r.NoWrite, RestrictNoWrite},
	}
	alreadyBlocked := map[string]bool{}
	for _, restr := range order {
		if !restr.on {
			continue
		}
		for _, op := range restrictBlocks[restr.name] {
			if alreadyBlocked[op] {
				continue
			}
			if _, present := handlers[op]; present {
				handlers[op] = makeDeny(restr.name, op)
				alreadyBlocked[op] = true
			}
		}
	}
}

func makeDeny(flag, op string) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return nil, fmt.Errorf(
			"op %q is disabled by --%s — run `perch help --%s` for details",
			op, flag, flag,
		)
	}
}

// Restrictions reflects on the catalog: for each known restriction, the
// ops it blocks. Used by `perch --restrictions` to print a discovery list.
func RestrictionList() []string {
	out := []string{
		RestrictNoShell,
		RestrictNoSubprocess,
		RestrictNoNetwork,
		RestrictNoWrite,
	}
	sort.Strings(out)
	return out
}

// BlockedByRestriction returns the ops blocked by ONE restriction (for
// the `--restrictions` discovery list). Sorted for deterministic output.
func BlockedByRestriction(name string) []string {
	out := append([]string{}, restrictBlocks[name]...)
	sort.Strings(out)
	return out
}

// SummariseRestrictions returns a human-readable summary like
// "--no-shell, --no-network" for error messages and audit logging.
func SummariseRestrictions(r Restrictions) string {
	flags := r.AsFlags()
	if len(flags) == 0 {
		return "(none)"
	}
	return strings.Join(flags, ", ")
}

// opCategory maps an op kind back to the restriction category that
// governs it ("no-shell" / "no-network" / etc.). Built once from
// restrictBlocks. Lets a `sandbox` block check whether a kind is gated
// without replicating the catalogue.
var opCategory = func() map[string]string {
	out := map[string]string{}
	for cat, ops := range restrictBlocks {
		for _, op := range ops {
			out[op] = cat
		}
	}
	return out
}()

// ApplyMaskGating wraps every restrictable handler with a runtime check
// against b.CapMask. This is what makes `sandbox no_shell ... end` work:
// the CLI restrictions block ops at the handler registration layer (the
// outermost, never-narrowed gate), and on top of that this wrapping pass
// adds an inner check that consults the dynamic mask each call.
//
// Call AFTER ApplyRestrictions so CLI-blocked handlers are already
// sentinels — wrapping them is harmless (the mask check runs first and
// if both gates would block, the user sees the sandbox-flavored message
// pointing at the file rather than the CLI flag).
func ApplyMaskGating(handlers map[string]interpreter.Handler) {
	for kind, cat := range opCategory {
		h, ok := handlers[kind]
		if !ok {
			continue
		}
		category := cat
		opName := kind
		inner := h
		handlers[kind] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
			if b.CapMask != nil {
				blocked := false
				switch category {
				case RestrictNoShell:
					blocked = b.CapMask.AnyNoShell()
				case RestrictNoSubprocess:
					blocked = b.CapMask.AnyNoSubprocess()
				case RestrictNoNetwork:
					blocked = b.CapMask.AnyNoNetwork()
				case RestrictNoWrite:
					blocked = b.CapMask.AnyNoWrite()
				}
				if blocked {
					return nil, fmt.Errorf(
						"op %q forbidden by sandbox (%s scope) — narrow the body or move the call outside the sandbox block",
						opName, category,
					)
				}
				// allow_bin narrowing: applies only to shell ops.
				if category == RestrictNoShell && b.CapMask.AllowedBins != nil {
					if cmd, ok := args["cmd"].(string); ok {
						first := firstToken(cmd)
						if first != "" && !b.CapMask.AllowedBinPermitted(first) {
							return nil, fmt.Errorf(
								"shell binary %q forbidden by sandbox allow_bin",
								first,
							)
						}
					}
				}
			}
			return inner(i, b, args)
		}
	}
}

// firstToken returns the first whitespace-separated word of s. Used for
// argv[0] checks against allow_bin allowlists.
func firstToken(s string) string {
	s = strings.TrimSpace(s)
	for i := 0; i < len(s); i++ {
		if s[i] == ' ' || s[i] == '\t' {
			return s[:i]
		}
	}
	return s
}

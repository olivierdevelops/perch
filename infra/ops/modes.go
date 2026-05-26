// Modes are named, opinionated subsets of the op catalog. Users pick
// one with `perch --mode NAME` (or future top-level `mode NAME` in
// the .perch file) to disable whole categories of ops at once.
//
// Each blocked op is replaced with a sentinel handler that returns a
// friendly "disabled by mode" error — so callers see exactly why a
// call failed, rather than a confusing "unknown op" message. Unblocked
// ops are unchanged.
//
// Modes are the Phase-0 alternative to the full sandbox spec at
// docs/sandbox.md: they ship a usable security knob today without
// requiring per-op declarations in the .perch file.

package ops

import (
	"fmt"
	"sort"
	"strings"

	"github.com/luowensheng/perch/infra/interpreter"
)

// Mode names. The empty string ("" / "trusted") is the default — full catalog.
const (
	ModeTrusted  = "trusted"
	ModeSafe     = "safe"
	ModeOffline  = "offline"
	ModeReadOnly = "read-only"
	ModePure     = "pure"
)

// modeBlocks lists, per mode, the op kinds that mode forbids.
// "pure" is intentionally the union of safe + offline + read-only.
var modeBlocks = map[string][]string{
	ModeSafe: append([]string{},
		// Shell / subprocess access. Without these, no arbitrary code
		// from the .perch file can run on the host.
		"shell", "shell_output", "shell_detached", "shell_in", "try_shell",
		"pkg_install", "pkg_uninstall",
		"kill_by_name", "process_running",
	),
	ModeOffline: append([]string{},
		// All ops that touch the network.
		"http_get", "http_post", "http_put", "http_delete", "http_status",
		"download",
		"dns_lookup",
		"port_check", "port_free", "find_free_port",
		"wait_for_port", "wait_for_url",
		"public_ip",
		// public_ip leaks; local_ip / mac_address / interfaces don't dial
		// but they reveal host facts. We block them too so "offline"
		// also implies "no network introspection."
		"local_ip", "mac_address", "interfaces",
	),
	ModeReadOnly: append([]string{},
		// Filesystem mutation.
		"write_file", "append_file", "append_line", "ensure_line_in_file",
		"replace_in_file", "backup_file",
		"cp", "mv", "rm", "mkdir", "chmod", "touch",
		"copy_dir", "ensure_dir", "make_executable",
		"symlink", "link_into_path",
		"mktemp_file", "mktemp_dir",
		"add_to_path",
		// Archive ops also write.
		"tar_extract", "zip_extract", "gzip", "ungzip", "tar_create", "zip_create",
		// Bundle ops that extract.
		"bundle_extract", "bundle_dir",
	),
}

func init() {
	// "pure" is the union of safe + offline + read-only. Build once.
	seen := map[string]bool{}
	all := []string{}
	for _, m := range []string{ModeSafe, ModeOffline, ModeReadOnly} {
		for _, op := range modeBlocks[m] {
			if !seen[op] {
				seen[op] = true
				all = append(all, op)
			}
		}
	}
	sort.Strings(all)
	modeBlocks[ModePure] = all
}

// Modes returns the list of known mode names, sorted, for CLI help.
func Modes() []string {
	out := []string{ModeTrusted}
	for m := range modeBlocks {
		out = append(out, m)
	}
	sort.Strings(out)
	return out
}

// IsValidMode is true for "" / "trusted" / "safe" / "offline" / "read-only" / "pure".
func IsValidMode(name string) bool {
	if name == "" || name == ModeTrusted {
		return true
	}
	_, ok := modeBlocks[name]
	return ok
}

// ApplyMode replaces every op blocked by the given mode with a sentinel
// handler that returns a "disabled by mode X" error. The trusted mode
// (or empty string) is a no-op. Unknown mode names error.
//
// The returned map is the SAME map (mutated in place) so callers can
// pass it onward without resyncing references.
func ApplyMode(handlers map[string]interpreter.Handler, mode string) error {
	if mode == "" || mode == ModeTrusted {
		return nil
	}
	blocked, ok := modeBlocks[mode]
	if !ok {
		return fmt.Errorf("unknown mode %q (valid: %s)", mode, strings.Join(Modes(), ", "))
	}
	deny := makeDenyHandler(mode)
	for _, op := range blocked {
		if _, present := handlers[op]; present {
			handlers[op] = deny(op)
		}
	}
	return nil
}

// BlockedOps returns the ops blocked by the given mode (for `--help` /
// `perch --modes`).
func BlockedOps(mode string) []string {
	out := append([]string{}, modeBlocks[mode]...)
	sort.Strings(out)
	return out
}

// makeDenyHandler returns a Handler factory that closes over the mode
// name. Each call yields a handler tagged with the op it stands in for,
// so the error message can name it precisely.
func makeDenyHandler(mode string) func(op string) interpreter.Handler {
	return func(op string) interpreter.Handler {
		return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
			return nil, fmt.Errorf(
				"op %q is disabled by --mode %s (see https://luowensheng.github.io/perch/sandbox/)",
				op, mode,
			)
		}
	}
}

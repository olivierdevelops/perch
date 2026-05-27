package simulate

import (
	"path/filepath"
	"strings"
)

// SimState is the *mutable* world the simulator threads through the
// op walk. Unlike SimEnv (capability declarations), SimState
// represents what's TRUE NOW — values produced by earlier ops, files
// the simulator has seen written, etc.
//
// Each scenario starts with a fresh SimState derived from the
// fixture's oracles. As ops execute (conceptually), the state
// evolves:
//
//   write_file "/tmp/x" "data"  → state.Files["/tmp/x"] = true
//   let n = shell_output "echo 5" → state.Vars["n"] = "5" (from oracle)
//   cd /srv                       → state.Cwd = "/srv"
//   rm "/tmp/x"                   → state.Files["/tmp/x"] = false
//
// Block ops snapshot the state, simulate the body with the snapshot,
// then either commit (for sequential blocks like `if` taken-branch)
// or merge (for branching blocks like `if not taken`, where the
// not-taken state shouldn't bleed).
type SimState struct {
	// Vars are the symbolic bindings — args from the command-line
	// shape, globals, and `let X = ...` captures resolved against
	// oracles. Stringly-typed (matches the runtime's bindings).
	Vars map[string]string

	// Env is the active host-env layer (host env + with_env overlays).
	// Separate from Vars so capability checks (EnvRestrict) can apply
	// to env-var interpolation without affecting `let` bindings.
	Env map[string]string

	// Cwd is the simulated current working directory.
	Cwd string

	// Files tracks file existence as the simulator has observed it.
	// Starts populated from the fixture's FileExists oracle; ops
	// mutate it (write_file / touch → true, rm → false).
	Files map[string]bool

	// Captured tracks which Vars were assigned by `let` from an
	// op whose result the simulator couldn't statically resolve.
	// Downstream uses of `${name}` for these get a MIGHT_FAIL
	// reason ("value depends on a runtime call that wasn't oracled").
	Unknown map[string]bool

	// Oracles are the current scenario's effective oracle set.
	// Read-only during the walk.
	Oracles OracleSet
}

// NewSimState builds a fresh state for a scenario. Initial Files
// comes from the FileExists oracle; Vars/Env are seeded with the
// SimEnv's Env map; Cwd defaults to /.
func NewSimState(env SimEnv, oracles OracleSet) *SimState {
	st := &SimState{
		Vars:    map[string]string{},
		Env:     map[string]string{},
		Cwd:     "/",
		Files:   map[string]bool{},
		Unknown: map[string]bool{},
		Oracles: oracles,
	}
	for k, v := range env.Env {
		st.Env[k] = v
	}
	for k, v := range oracles.FileExists {
		st.Files[k] = v
	}
	// Auto-bound vars the simulator can resolve from the SimEnv.
	if env.OS != "" {
		st.Vars["os"] = env.OS
		st.Vars["is_windows"] = boolStr(env.OS == "windows")
		st.Vars["is_macos"] = boolStr(env.OS == "darwin")
		st.Vars["is_linux"] = boolStr(env.OS == "linux")
		st.Vars["is_unix"] = boolStr(env.OS != "windows")
	}
	if env.Arch != "" {
		st.Vars["arch"] = env.Arch
		st.Vars["is_arm64"] = boolStr(env.Arch == "arm64")
		st.Vars["is_amd64"] = boolStr(env.Arch == "amd64")
	}
	return st
}

// Snapshot returns a deep-enough copy for branching simulation
// (`if` taken vs not-taken, parallel branches). Maps copied; slices
// in Oracles are read-only and shared.
func (s *SimState) Snapshot() *SimState {
	cp := &SimState{
		Vars:    cloneStrMap(s.Vars),
		Env:     cloneStrMap(s.Env),
		Cwd:     s.Cwd,
		Files:   cloneBoolMap(s.Files),
		Unknown: cloneBoolMap(s.Unknown),
		Oracles: s.Oracles,
	}
	return cp
}

// Substitute interpolates ${name} placeholders in s against the
// state's Vars + Env. Returns the resolved string and a bool
// indicating whether every reference resolved (false means at least
// one ${name} stayed as a placeholder because the value was unknown).
func (s *SimState) Substitute(in string) (string, bool) {
	if !strings.Contains(in, "${") {
		return in, true
	}
	out := strings.Builder{}
	out.Grow(len(in))
	allResolved := true
	i := 0
	for i < len(in) {
		if i+1 < len(in) && in[i] == '$' && in[i+1] == '{' {
			end := strings.IndexByte(in[i+2:], '}')
			if end < 0 {
				out.WriteString(in[i:])
				return out.String(), false
			}
			name := in[i+2 : i+2+end]
			if v, ok := s.Vars[name]; ok {
				if s.Unknown[name] {
					out.WriteString(in[i : i+2+end+1])
					allResolved = false
				} else {
					out.WriteString(v)
				}
			} else if v, ok := s.Env[name]; ok {
				out.WriteString(v)
			} else {
				out.WriteString(in[i : i+2+end+1])
				allResolved = false
			}
			i += 2 + end + 1
			continue
		}
		out.WriteByte(in[i])
		i++
	}
	return out.String(), allResolved
}

// FileExists consults the simulator's current view of the file
// system. Returns (exists, known) — known=false means we have no
// information about this path.
func (s *SimState) FileExists(path string) (bool, bool) {
	// Resolve relative path against current cwd.
	abs := path
	if !filepath.IsAbs(path) {
		abs = filepath.Join(s.Cwd, path)
	}
	if v, ok := s.Files[abs]; ok {
		return v, true
	}
	if v, ok := s.Files[path]; ok {
		return v, true
	}
	return false, false
}

// MarkFile records that `path` does (or doesn't) exist after this
// step. Used by write_file (true), touch (true), rm (false), etc.
func (s *SimState) MarkFile(path string, exists bool) {
	abs := path
	if !filepath.IsAbs(path) {
		abs = filepath.Join(s.Cwd, path)
	}
	s.Files[abs] = exists
}

// SetVar binds `name` → `value`. If unknown, also flag it so
// downstream interpolation knows the value is a placeholder.
func (s *SimState) SetVar(name, value string, unknown bool) {
	s.Vars[name] = value
	if unknown {
		s.Unknown[name] = true
	} else {
		delete(s.Unknown, name)
	}
}

func cloneStrMap(m map[string]string) map[string]string {
	out := make(map[string]string, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

func cloneBoolMap(m map[string]bool) map[string]bool {
	out := make(map[string]bool, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

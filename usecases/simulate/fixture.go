package simulate

import (
	"encoding/json"
	"fmt"
	"os"
)

// LoadFixture reads + parses a JSON fixture file describing capabilities,
// oracles, and named scenarios.
func LoadFixture(path string) (Fixture, error) {
	var f Fixture
	b, err := os.ReadFile(path)
	if err != nil {
		return f, err
	}
	if err := json.Unmarshal(b, &f); err != nil {
		return f, fmt.Errorf("parsing fixture JSON: %w", err)
	}
	return f, nil
}

// Fixture is the JSON-loadable companion to SimEnv: the same capability
// declarations PLUS oracles (simulated outputs for ops the static walk
// can't resolve) PLUS named scenarios (override sets that branch the
// simulation).
//
// File shape:
//
//   {
//     "os": "linux", "arch": "amd64",
//     "env": {"HOME": "/h", "PATH": "/usr/bin"}, "env_only": true,
//     "fs_read":  ["/srv"], "fs_write": ["/tmp"],
//     "bins":     ["docker", "kubectl"],
//     "network":  ["api.github.com"],
//     "no_shell": false, "no_network": false,
//
//     "oracles": {
//       "file_exists":  {"./manifest.yaml": true, "/etc/passwd": false},
//       "shell_output": {"git rev-parse HEAD": "1f1db7b"},
//       "http":         {"https://api.github.com/health": {"status": 200, "body": "OK"}},
//       "has_bin":      {"docker": true, "kubectl": true}
//     },
//
//     "scenarios": [
//       {"name": "happy",      "overrides": {}},
//       {"name": "github-down","overrides": {
//         "http": {"https://api.github.com/health": {"status": 500}}
//       }}
//     ]
//   }
//
// If `scenarios` is absent or empty, the fixture is treated as ONE
// implicit scenario named "default" using the top-level oracles.
type Fixture struct {
	// Capability declarations (same shape as SimEnv, JSON-tagged).
	OS           string            `json:"os,omitempty"`
	Arch         string            `json:"arch,omitempty"`
	Env          map[string]string `json:"env,omitempty"`
	EnvOnly      bool              `json:"env_only,omitempty"`
	FsRead       []string          `json:"fs_read,omitempty"`
	FsWrite      []string          `json:"fs_write,omitempty"`
	Bins         []string          `json:"bins,omitempty"`
	Network      []string          `json:"network,omitempty"`
	NoShell      bool              `json:"no_shell,omitempty"`
	NoSubprocess bool              `json:"no_subprocess,omitempty"`
	NoNetwork    bool              `json:"no_network,omitempty"`
	NoWrite      bool              `json:"no_write,omitempty"`

	// Oracles supply concrete simulated outputs to ops the static walk
	// can't statically resolve. Each is keyed by the op's primary
	// argument after ${} substitution against the current state.
	Oracles OracleSet `json:"oracles,omitempty"`

	// Scenarios let one fixture file describe multiple what-ifs.
	// Each scenario inherits the top-level oracles and overrides any
	// keys it specifies. If scenarios is empty, one implicit
	// "default" scenario runs with the top-level oracles.
	Scenarios []Scenario_ `json:"scenarios,omitempty"`
}

// OracleSet — concrete simulated outputs for ops whose result would
// otherwise be "unknown."
type OracleSet struct {
	// FileExists maps an absolute or relative path → present? Used by
	// `if exists "PATH"` and by `write_file "PATH"` to decide whether
	// the op's stateful effect was already in place. Paths recorded
	// by the simulator as written by an earlier op shadow this.
	FileExists map[string]bool `json:"file_exists,omitempty"`

	// ShellOutput maps the (post-interpolation) shell command → its
	// simulated stdout. Used by `let X = shell_output "Y"` so
	// downstream ${X} resolves to the simulated value.
	ShellOutput map[string]string `json:"shell_output,omitempty"`

	// HTTP maps URL → simulated response. Used by `http_get`,
	// `http_post`, etc. Lets you set status codes and bodies per URL.
	HTTP map[string]HTTPResponse `json:"http,omitempty"`

	// HasBin overrides the `has_bin "X"` predicate's result. Useful
	// when you want to simulate "what if kubectl is missing?" without
	// removing kubectl from the Bins capability allowlist.
	HasBin map[string]bool `json:"has_bin,omitempty"`
}

// HTTPResponse is the simulated outcome of an http_* op.
type HTTPResponse struct {
	Status int    `json:"status,omitempty"` // default 200 if zero
	Body   string `json:"body,omitempty"`
	// Redirect, when non-empty, simulates the server returning a 3xx
	// pointing here. Useful for "what if api.github.com redirects to
	// evil.com?" scenarios.
	Redirect string `json:"redirect,omitempty"`
}

// Scenario_ is one named override set. (Underscore suffix because
// "Scenario" already exists in simulate.go for per-op alternatives.
// Distinct concept, distinct name.)
type Scenario_ struct {
	Name      string    `json:"name"`
	Overrides OracleSet `json:"overrides,omitempty"`
	// EnvOverride lets a scenario tweak env vars too — e.g.
	// "what if GITHUB_TOKEN isn't set in this scenario?"
	Env map[string]string `json:"env,omitempty"`
}

// ToSimEnv lifts the capability declarations from a Fixture into the
// SimEnv shape the simulator already uses.
func (f Fixture) ToSimEnv() SimEnv {
	bins := map[string]bool{}
	for _, b := range f.Bins {
		bins[b] = true
	}
	if len(bins) == 0 {
		bins = nil
	}
	return SimEnv{
		OS:           f.OS,
		Arch:         f.Arch,
		Env:          f.Env,
		EnvRestrict:  f.EnvOnly,
		FsRead:       f.FsRead,
		FsWrite:      f.FsWrite,
		Bins:         bins,
		Network:      f.Network,
		NoShell:      f.NoShell,
		NoSubprocess: f.NoSubprocess,
		NoNetwork:    f.NoNetwork,
		NoWrite:      f.NoWrite,
	}
}

// MergeOracles produces an OracleSet that is the base oracles
// overlaid with overrides — used to build a scenario's effective
// oracles from the fixture's defaults + the scenario's tweaks.
func MergeOracles(base, override OracleSet) OracleSet {
	out := OracleSet{
		FileExists:  map[string]bool{},
		ShellOutput: map[string]string{},
		HTTP:        map[string]HTTPResponse{},
		HasBin:      map[string]bool{},
	}
	for k, v := range base.FileExists {
		out.FileExists[k] = v
	}
	for k, v := range override.FileExists {
		out.FileExists[k] = v
	}
	for k, v := range base.ShellOutput {
		out.ShellOutput[k] = v
	}
	for k, v := range override.ShellOutput {
		out.ShellOutput[k] = v
	}
	for k, v := range base.HTTP {
		out.HTTP[k] = v
	}
	for k, v := range override.HTTP {
		out.HTTP[k] = v
	}
	for k, v := range base.HasBin {
		out.HasBin[k] = v
	}
	for k, v := range override.HasBin {
		out.HasBin[k] = v
	}
	return out
}

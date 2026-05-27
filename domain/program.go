// Package domain holds the data types that describe a perch program:
// a set of named, callable commands compiled from a .perch source file.
//
// A Program is the canonical artifact. Capy parses the user's source
// file into JSON; the loader hydrates that JSON into a Program; the
// interpreter walks Program.Commands[name].Ops to actually do work.
//
// Domain types are pure data. They have no behavior beyond simple
// transformations and import nothing from the rest of the project.
package domain

// Program is the whole parsed config. One per .perch file.
type Program struct {
	Name        string              `json:"name"`
	Description string              `json:"description"`
	Version     string              `json:"version"`
	Globals     Globals             `json:"globals"`
	Commands    map[string]*Command `json:"commands"`
	Catch       *Catch              `json:"catch,omitempty"`
	// Templates are parse-time stamps — `template NAME ... end` blocks.
	// Their bodies are spliced inline at each call site (`call NAME a b`)
	// during loading, with positional args substituted as ${NAME}
	// bindings. By the time the interpreter sees the Program, templates
	// have already been expanded — they exist here purely so `perch
	// --check` and the LSP can introspect them.
	Templates map[string]*Template `json:"templates,omitempty"`
	// ScriptPath is the absolute path of the .perch source the program
	// was loaded from. Empty when the program was embedded in a binary.
	// Surfaces as ${script_path} / ${script_dir} auto-bindings.
	ScriptPath string `json:"-"`
	// Bundle declares what files/directories should be embedded into the
	// fat binary at `perch --build` time. Lifts the on-CLI `--include`
	// flag into the source file so the .perch is the complete buildable
	// spec — `perch --build` alone (no extra args) produces the right
	// artifact. CLI `--include` is additive on top of this.
	Bundle Bundle `json:"bundle,omitempty"`
}

// Bundle declares the file tree to embed into the fat binary at
// `perch --build` time. Each Include is a path relative to the .perch
// source file (or absolute). At runtime the embedded archive is
// reachable via aliased identifiers (declared inline via `as NAME`)
// or directly via `bundle_dir` / `bundle_extract` / the
// `ops.BundleReadFile` Go helper.
//
// Example .perch:
//
//   bundle
//       include "./modules"                       # whole dir, no alias
//       include "./policy.wasm" as policy_wasm    # one file, aliased
//   end
//
//   command run_plugin do
//       wasm_run policy_wasm   # bare ident → resolved to bundle bytes
//           wasm_arg "/ro/deploy"
//       end
//   end
type Bundle struct {
	Includes []string       `json:"includes,omitempty"`
	Aliases  []BundleAlias  `json:"aliases,omitempty"`
}

// BundleAlias is one `include "PATH" as NAME` declaration. Name is the
// bare identifier the user types in ops (e.g. `wasm_run policy_wasm`);
// Entry is the bundle-relative path under which the file lives after
// `--build` archives it (typically the basename, or a dir-relative
// subpath when the include was a directory).
type BundleAlias struct {
	Name  string `json:"name"`
	Entry string `json:"entry"`
}

// Template is a parse-time, parameterized op-sequence. Identical structure
// to Command — same arg-block syntax, same body — but expanded inline at
// every call site rather than executed as a top-level verb. The validator
// rejects recursion and templates that emit declaration events.
type Template struct {
	Name        string    `json:"name"`
	Description string    `json:"description"`
	Args        []ArgSpec `json:"args,omitempty"`
	Ops         []Op      `json:"ops"`
}

// Globals holds bindings shared by every command invocation.
// Each entry's value type is preserved (bool/int/float/string) and
// surfaces as both a CLI flag and a ${name} binding at runtime.
type Globals struct {
	Bindings []GlobalBinding `json:"bindings"`
}

// GlobalBinding is one `NAME = VALUE` line from a globals block.
type GlobalBinding struct {
	Name  string `json:"name"`
	Type  string `json:"type"` // "bool" | "int" | "float" | "string"
	Value any    `json:"value"`
}

// Command is one declared, callable unit.
type Command struct {
	Name        string            `json:"name"`
	Description string            `json:"description"`
	Args        []ArgSpec         `json:"args,omitempty"`
	Env         map[string]string `json:"env,omitempty"`
	Modifiers   Modifiers         `json:"modifiers"`
	Ops         []Op              `json:"ops"`
}

// ArgSpec declares one typed CLI argument on a command.
type ArgSpec struct {
	Name        string `json:"name"`
	Type        string `json:"type"` // "string" | "int" | "float" | "bool"
	Description string `json:"description"`
	Default     any    `json:"default,omitempty"`
	HasDefault  bool   `json:"has_default,omitempty"`
	Index       *int   `json:"index,omitempty"`
	Optional    bool   `json:"optional,omitempty"`
	// Rest, when true, makes this arg consume every remaining positional
	// argument. The arg's value becomes a newline-joined string; a
	// companion `${NAME_count}` int binding gives the number of values.
	// Must be the LAST declared arg, type "string", no default. Equivalent
	// to Go's `args ...string`.
	Rest bool `json:"rest,omitempty"`
}

// Modifiers are the boolean / list flags declared on a command before its `do` block.
type Modifiers struct {
	Private              bool     `json:"private,omitempty"`
	Detached             bool     `json:"detached,omitempty"`
	ProxyArgs            bool     `json:"proxy_args,omitempty"`
	RequireOS            []string `json:"require_os,omitempty"`
	RequireArch          []string `json:"require_arch,omitempty"`
	Dir                  string   `json:"dir,omitempty"`
	OnSignal             string   `json:"on_signal,omitempty"`
	PostStartDelaySecs   int      `json:"post_start_delay_secs,omitempty"`

	// ── Test modifiers ────────────────────────────────────────────────
	//
	// `test` marks a command as a test. It's hidden from `perch --help`
	// (like `private`) but discovered by `perch test` and run in a
	// sandboxed environment. A test passes unless any op errors (the
	// usual `fail "msg"` model). Tests are real commands — same ops,
	// same templates, same execution contexts.

	// Test marks this command as a test discovered by `perch test`.
	Test bool `json:"test,omitempty"`
	// TestAllowNetwork opts out of the default --no-network sandbox.
	TestAllowNetwork bool `json:"test_allow_network,omitempty"`
	// TestAllowShell opts out of the default --no-shell sandbox.
	TestAllowShell bool `json:"test_allow_shell,omitempty"`
	// TestAllowWrite opts out of write-roots restriction to the temp cwd.
	TestAllowWrite bool `json:"test_allow_write,omitempty"`
	// TestAllowSubprocess opts out of --no-subprocess.
	TestAllowSubprocess bool `json:"test_allow_subprocess,omitempty"`
	// TestKeepCwd disables the auto temp-cwd switch; the test runs in
	// the file's directory. Useful when the test reads fixtures relative
	// to the .perch file.
	TestKeepCwd bool `json:"test_keep_cwd,omitempty"`
	// TestTimeoutSecs caps wall-clock for this test. 0 means use the
	// global default (30s per test). The flag --test-timeout=N
	// overrides this at the runner level.
	TestTimeoutSecs int `json:"test_timeout_secs,omitempty"`
}

// Catch is the optional catch-all handler for unknown command names.
type Catch struct {
	Bind        string `json:"bind"`        // name of the implicit arg holding the unknown name
	Description string `json:"description"`
	Ops         []Op   `json:"ops"`
}

// Op is one statement inside a command body (or inside a block op's body).
//
// Block ops (`if`, `if_call`, `for_each`) carry their nested ops in Body.
type Op struct {
	Kind        string         `json:"kind"`
	Line        int            `json:"line,omitempty"`
	Args        map[string]any `json:"args,omitempty"`
	Body        []Op           `json:"body,omitempty"`
	CaptureInto string         `json:"capture_into,omitempty"`
	// ExpandedFrom, when non-empty, names the template this op was
	// expanded from. Set by the loader during the template-expansion
	// pass; carried in error messages and audit traces so users see
	// `failed at print (from template `check_bin` line 4)` instead of
	// just the post-expansion location.
	ExpandedFrom string `json:"expanded_from,omitempty"`
}

// IsBlock reports whether this op kind contains a nested body.
//
// Block ops fall into two groups:
//   - control flow: `if`, `if_call`, `for_each`
//   - execution contexts: `parallel`, `timeout`, `retry`, `with_env`,
//     `with_cwd`, `sandbox` — wrap a body to change *how* it runs
//     (concurrency, deadline, retry policy, env overlay, cwd, capability
//     gate) without changing what ops it can express.
func (o Op) IsBlock() bool {
	switch o.Kind {
	case "if", "if_call", "for_each",
		"parallel", "timeout", "retry", "with_env", "with_cwd", "sandbox", "cache",
		"wasm_run":
		return true
	}
	return false
}

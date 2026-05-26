// Package domain holds the data types that describe a perch program:
// a set of named, callable commands compiled from a .capy source file.
//
// A Program is the canonical artifact. Capy parses the user's source
// file into JSON; the loader hydrates that JSON into a Program; the
// interpreter walks Program.Commands[name].Ops to actually do work.
//
// Domain types are pure data. They have no behavior beyond simple
// transformations and import nothing from the rest of the project.
package domain

// Program is the whole parsed config. One per .capy file.
type Program struct {
	Name        string              `json:"name"`
	Description string              `json:"description"`
	Version     string              `json:"version"`
	Globals     Globals             `json:"globals"`
	Commands    map[string]*Command `json:"commands"`
	Catch       *Catch              `json:"catch,omitempty"`
}

// Globals holds bindings shared by every command invocation.
// Each entry's value type is preserved (bool/int/float/string) and
// surfaces as both a CLI flag and a {{name}} binding at runtime.
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
}

// Catch is the optional catch-all handler for unknown command names.
type Catch struct {
	Bind        string `json:"bind"`        // name of the implicit arg holding the unknown name
	Description string `json:"description"`
	Ops         []Op   `json:"ops"`
}

// Op is one statement inside a command body (or inside a block op's body).
//
// Block ops (if_os, if_arch, if_eq, if_neq, if_gt, if_lt, if_exists,
// if_empty, if_not_empty, for_each) carry their nested ops in Body.
type Op struct {
	Kind        string         `json:"kind"`
	Line        int            `json:"line,omitempty"`
	Args        map[string]any `json:"args,omitempty"`
	Body        []Op           `json:"body,omitempty"`
	CaptureInto string         `json:"capture_into,omitempty"`
}

// IsBlock reports whether this op kind contains a nested body.
func (o Op) IsBlock() bool {
	switch o.Kind {
	case "if_os", "if_arch", "if_eq", "if_neq",
		"if_gt", "if_lt", "if_exists", "if_empty", "if_not_empty",
		"for_each":
		return true
	}
	return false
}

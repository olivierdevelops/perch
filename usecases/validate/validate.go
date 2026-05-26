// Package validate checks a perch program for problems before it runs:
//
//   - every arg has a valid `type` and its `default` (if any) matches that type
//   - duplicate arg names within a command
//   - duplicate `index` slots within a command
//   - every `run TARGET` op resolves to an existing command
//   - every `on_signal HANDLER` modifier resolves to an existing command
//   - every op kind in a body is registered with the interpreter
//   - every `${name}` placeholder in a string-valued op arg resolves to a
//     declared arg / `let` capture / global, or looks like an env var
//     (all-uppercase identifier)
package validate

import (
	"fmt"
	"regexp"
	"sort"
	"strings"

	"github.com/luowensheng/perch/domain"
)

// UseCase is the consumer-owned protocol.
type UseCase interface {
	Execute(configPath string) error
}

// LoadFn loads a program from disk.
type LoadFn func(path string) (*domain.Program, error)

// KnownOps returns the registered op kinds (so the validator can check that
// every kind referenced in a body has a handler).
type KnownOps func() map[string]struct{}

type Impl struct {
	Load     LoadFn
	KnownOps KnownOps
}

// Issue is one validation finding.
type Issue struct {
	Severity string // "error" | "warning"
	Where    string // command name + sub-location, or "" for file-level
	Message  string
}

func (i Issue) String() string {
	if i.Where == "" {
		return fmt.Sprintf("%s: %s", i.Severity, i.Message)
	}
	return fmt.Sprintf("%s: %s: %s", i.Severity, i.Where, i.Message)
}

// Execute runs the full validation pass and prints a report. Returns a
// non-nil error if any issues are of severity "error".
func (i *Impl) Execute(configPath string) error {
	p, err := i.Load(configPath)
	if err != nil {
		return fmt.Errorf("✗ %s: %w", configPath, err)
	}
	known := i.KnownOps()
	issues := Check(p, known)

	errors, warnings := 0, 0
	for _, iss := range issues {
		fmt.Println(iss.String())
		if iss.Severity == "error" {
			errors++
		} else {
			warnings++
		}
	}

	cmds := len(p.Commands)
	globals := len(p.Globals.Bindings)
	catch := 0
	if p.Catch != nil {
		catch = 1
	}

	if errors == 0 {
		fmt.Printf("✓ %s: %d command%s, %d catch, %d global%s — %s\n",
			configPath, cmds, plural(cmds), catch, globals, plural(globals),
			summary(warnings))
		return nil
	}
	fmt.Printf("✗ %s: %d error%s, %d warning%s\n",
		configPath, errors, plural(errors), warnings, plural(warnings))
	return fmt.Errorf("validation failed")
}

func plural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}
func summary(warnings int) string {
	if warnings == 0 {
		return "no issues"
	}
	return fmt.Sprintf("%d warning%s", warnings, plural(warnings))
}

// Check is the pure validation entry point — useful in tests.
func Check(p *domain.Program, knownOps map[string]struct{}) []Issue {
	v := &checker{prog: p, ops: knownOps}
	v.run()
	sort.SliceStable(v.issues, func(a, b int) bool {
		ia, ib := v.issues[a], v.issues[b]
		if ia.Severity != ib.Severity {
			return ia.Severity == "error"
		}
		return ia.Where < ib.Where
	})
	return v.issues
}

// ────────────────────────────────────────────────────────────────────────

type checker struct {
	prog   *domain.Program
	ops    map[string]struct{}
	issues []Issue
}

var validTypes = map[string]bool{"string": true, "int": true, "float": true, "bool": true}

func (c *checker) addErr(where, msg string)  { c.issues = append(c.issues, Issue{"error", where, msg}) }
func (c *checker) addWarn(where, msg string) { c.issues = append(c.issues, Issue{"warning", where, msg}) }

func (c *checker) run() {
	// Sorted iteration for stable output.
	names := make([]string, 0, len(c.prog.Commands))
	for n := range c.prog.Commands {
		names = append(names, n)
	}
	sort.Strings(names)

	for _, n := range names {
		c.checkCommand(c.prog.Commands[n])
	}
	if c.prog.Catch != nil {
		c.checkCatch(c.prog.Catch)
	}
}

func (c *checker) checkCommand(cmd *domain.Command) {
	where := cmd.Name

	if cmd.Description == "" {
		c.addWarn(where, "no description (won't show up nicely in --help)")
	}

	// Args: types, defaults, uniqueness, index collisions.
	seenNames := map[string]bool{}
	seenIdx := map[int]string{}
	for _, a := range cmd.Args {
		argWhere := fmt.Sprintf("%s arg %q", cmd.Name, a.Name)
		if a.Name == "" {
			c.addErr(where, "arg with empty name")
			continue
		}
		if seenNames[a.Name] {
			c.addErr(argWhere, "duplicate arg name")
		}
		seenNames[a.Name] = true

		if a.Type == "" {
			c.addErr(argWhere, "missing `type` field")
		} else if !validTypes[a.Type] {
			c.addErr(argWhere, fmt.Sprintf("unknown type %q (use string/int/float/bool)", a.Type))
		} else if a.HasDefault {
			if !defaultMatchesType(a.Type, a.Default) {
				c.addErr(argWhere, fmt.Sprintf("default %v does not match type %q", a.Default, a.Type))
			}
		}
		if a.Index != nil {
			if *a.Index < 0 {
				c.addErr(argWhere, "index must be ≥ 0")
			}
			if other, ok := seenIdx[*a.Index]; ok {
				c.addErr(argWhere, fmt.Sprintf("index %d collides with arg %q", *a.Index, other))
			}
			seenIdx[*a.Index] = a.Name
		}
	}

	if h := cmd.Modifiers.OnSignal; h != "" {
		if _, ok := c.prog.Commands[h]; !ok {
			c.addErr(where, fmt.Sprintf("on_signal handler %q is not a declared command", h))
		}
	}

	// Walk the body.
	known := autoBoundNames()
	for _, a := range cmd.Args {
		known[a.Name] = true
	}
	for _, g := range c.prog.Globals.Bindings {
		known[g.Name] = true
	}
	for k := range cmd.Env {
		known[k] = true
	}
	c.checkOps(cmd.Ops, where, known)
}

func (c *checker) checkCatch(ca *domain.Catch) {
	where := "catch"
	known := autoBoundNames()
	known[ca.Bind] = true
	known["proxy_args"] = true
	for _, g := range c.prog.Globals.Bindings {
		known[g.Name] = true
	}
	c.checkOps(ca.Ops, where, known)
}

// known is the set of names available for ${...} resolution at this point
// in the op list. We add capture_into names as we walk.
func (c *checker) checkOps(ops []domain.Op, where string, known map[string]bool) {
	for _, op := range ops {
		opWhere := where
		if _, ok := c.ops[op.Kind]; !ok {
			c.addErr(opWhere, fmt.Sprintf("unknown op kind %q", op.Kind))
		}
		// `run TARGET` — validate target exists.
		if op.Kind == "run" {
			if t, ok := op.Args["target"].(string); ok {
				if _, exists := c.prog.Commands[t]; !exists {
					c.addErr(opWhere, fmt.Sprintf("`run %s` — no such command", t))
				}
			}
		}
		// Walk string args for ${name} placeholders.
		for _, v := range op.Args {
			if s, ok := v.(string); ok {
				for _, name := range placeholders(s) {
					if name == "" || known[name] {
						continue
					}
					if looksLikeEnv(name) {
						continue // probably an env var; can't statically verify
					}
					c.addErr(opWhere, fmt.Sprintf("unknown placeholder ${%s} in op %q", name, op.Kind))
				}
			}
		}
		// Capture introduces a new known name for subsequent ops.
		if op.CaptureInto != "" {
			known = clone(known)
			known[op.CaptureInto] = true
		}
		// Recurse into block ops.
		if len(op.Body) > 0 {
			inner := clone(known)
			c.checkOps(op.Body, opWhere, inner)
		}
	}
}

// ────────────────────────────────────────────────────────────────────────
// helpers

func clone(m map[string]bool) map[string]bool {
	out := make(map[string]bool, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

var placeholderRE = regexp.MustCompile(`\$\{([A-Za-z_][A-Za-z_0-9]*)\}`)

// placeholders extracts every ${name} placeholder in s, except those
// escaped with a leading backslash.
func placeholders(s string) []string {
	var out []string
	matches := placeholderRE.FindAllStringIndex(s, -1)
	for _, m := range matches {
		// Skip escaped \${name}
		if m[0] > 0 && s[m[0]-1] == '\\' {
			continue
		}
		name := s[m[0]+2 : m[1]-1]
		out = append(out, name)
	}
	return out
}

// looksLikeEnv: convention — uppercase identifier likely refers to a host
// env var. We can't statically verify env presence, so we treat it as OK.
func looksLikeEnv(name string) bool {
	return strings.ToUpper(name) == name
}

// autoBoundNames returns the set of placeholder names every command
// has access to without declaring them. Mirrors interpreter.seedGlobalsAndEnv.
func autoBoundNames() map[string]bool {
	names := []string{
		// OS / arch / convenience flags
		"os", "arch", "is_windows", "is_macos", "is_linux", "is_unix",
		"is_arm64", "is_amd64",
		"cpu_count", "pid", "now_unix",
		// Path conventions
		"path_sep", "path_list_sep", "exe_ext", "null_device", "shell_name",
		// Standard directories
		"home", "home_dir", "config_dir", "cache_dir", "data_dir", "temp_dir",
		// Binary / script
		"exe_path", "exe_dir", "exe_name", "script_path", "script_dir",
		// Identity
		"user", "uid", "hostname",
	}
	m := make(map[string]bool, len(names))
	for _, n := range names {
		m[n] = true
	}
	return m
}

func defaultMatchesType(t string, v any) bool {
	switch t {
	case "string":
		_, ok := v.(string)
		return ok
	case "bool":
		_, ok := v.(bool)
		return ok
	case "int":
		switch x := v.(type) {
		case int, int64:
			return true
		case float64:
			return x == float64(int64(x))
		}
	case "float":
		switch v.(type) {
		case float64, float32, int, int64:
			return true
		}
	}
	return false
}

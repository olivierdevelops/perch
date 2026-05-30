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
	"github.com/luowensheng/perch/infra/interpreter"
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
		fmt.Printf("✓ %s: %d command%s, %d catch, %d binding%s — %s\n",
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

	// Test-marked commands are hidden from --help, so a missing
	// description isn't a UX problem worth warning about. Same for
	// `private` commands.
	if cmd.Description == "" && !cmd.Modifiers.Test && !cmd.Modifiers.Private {
		c.addWarn(where, "no description (won't show up nicely in --help)")
	}

	// Args: types, defaults, uniqueness, index collisions, rest position.
	seenNames := map[string]bool{}
	seenIdx := map[int]string{}
	for argIdx, a := range cmd.Args {
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
		// `rest` constraints: must be last, must be positional (have an
		// index), must be type string, must not carry a default. Catching
		// these statically saves a confusing runtime error.
		if a.Rest {
			if argIdx != len(cmd.Args)-1 {
				c.addErr(argWhere, "`rest` arg must be the last declared arg")
			}
			if a.Index == nil {
				c.addErr(argWhere, "`rest` arg must be positional (declare `index N`)")
			}
			if a.Type != "" && a.Type != "string" {
				c.addErr(argWhere, fmt.Sprintf("`rest` arg must be type \"string\" (got %q)", a.Type))
			}
			if a.HasDefault {
				c.addErr(argWhere, "`rest` arg cannot have a default")
			}
		}
	}

	if h := cmd.Modifiers.OnSignal; h != "" {
		if _, ok := c.prog.Commands[h]; !ok {
			c.addErr(where, fmt.Sprintf("on_signal handler %q is not a declared command", h))
		}
	}

	// Walk the body.
	known := interpreter.ProvidedVarNames()
	// A command declaring the `proxy_args` modifier binds ${proxy_args}
	// (the full argv joined), same as a catch block does.
	if cmd.Modifiers.ProxyArgs {
		known["proxy_args"] = true
	}
	for _, a := range cmd.Args {
		known[a.Name] = true
		// A rest arg also exposes ${NAME_count}.
		if a.Rest {
			known[a.Name+"_count"] = true
		}
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
	known := interpreter.ProvidedVarNames()
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
			if op.Kind == "_template_call" {
				name, _ := op.Args["name"].(string)
				c.addErr(opWhere, fmt.Sprintf("`%s` — no such template (check imports + spelling)", name))
			} else if !c.prog.Requirements.BinAllowed(op.Kind) {
				// Unknown op kind → treated as a bare declared-bin invocation
				// at runtime (`let r = docker ps`). Valid if the bin is declared
				// in `requires`; otherwise it's an undeclared bin or a typo.
				c.addErr(opWhere, fmt.Sprintf("`%s` is not a known op and not a declared bin in `requires`", op.Kind))
			}
		}
		// `run TARGET` — validate target exists.
		if op.Kind == "run" {
			if t, ok := op.Args["target"].(string); ok {
				if _, exists := c.prog.Commands[t]; !exists {
					c.addErr(opWhere, fmt.Sprintf("`run %s` — no such command", t))
				}
			}
		}
		// requires-manifest static enforcement: when the file declared a
		// `requires` block, every literal shell bin / HTTP host / env-var
		// read must be in the manifest. This catches `bin_not_declared` /
		// `host_not_declared` / `env_not_declared` at --check time instead
		// of waiting for the op to fire at runtime. Args containing ${...}
		// are skipped — their value isn't known until runtime.
		if c.prog.Requirements.Declared {
			c.checkRequiresUsage(op, opWhere)
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
			// for_each binds a loop variable inside its body. The
			// variable name is the second positional arg ("_1") set by
			// the capy `for_each VALUE NAME ... end` form.
			if op.Kind == "for_each" {
				if v, ok := op.Args["_1"]; ok {
					if name, ok := v.(string); ok && name != "" {
						inner[name] = true
					}
				}
			}
			c.checkOps(op.Body, opWhere, inner)
		}
	}
}

// checkRequiresUsage statically verifies one op against the file's
// `requires` manifest. Only literal args (no `${...}`) are checked —
// interpolated values aren't known until runtime, so those fall through
// to the runtime guard. This is the static half of the same enforcement
// `infra/ops/requires.go` does dynamically.
func (c *checker) checkRequiresUsage(op domain.Op, where string) {
	r := c.prog.Requirements
	switch op.Kind {
	case "shell", "shell_output", "shell_detached":
		cmd := argStr(op, "cmd", "_0")
		if cmd == "" || strings.Contains(cmd, "${") {
			return // dynamic — defer to runtime
		}
		bin := firstShellToken(cmd)
		if bin == "" || isShellBuiltin(bin) || binDeclared(r, bin) {
			return
		}
		c.addErr(where, fmt.Sprintf(
			"shell uses bin %q which is not declared in `requires` (add `bin %q` or run with the bin allowed)",
			bin, bin))

	case "http_get", "http_post", "http_put", "http_delete", "download":
		url := argStr(op, "url", "_0")
		if url == "" || strings.Contains(url, "${") {
			return
		}
		host := hostFromURL(url)
		if host == "" || hostDeclared(r, host) {
			return
		}
		c.addErr(where, fmt.Sprintf(
			"%s targets host %q which is not declared in `requires` (add `host %q`)",
			op.Kind, host, host))

	case "get_env":
		name := argStr(op, "name", "_0")
		if name == "" || strings.Contains(name, "${") {
			return
		}
		if envDeclared(r, name) {
			return
		}
		c.addErr(where, fmt.Sprintf(
			"get_env reads %q which is not declared in `requires` (add `env %q`)",
			name, name))

	// Filesystem ops — literal paths checked against declared read/write roots.
	case "mkdir", "rm", "touch", "chmod", "write_file", "append_file", "append_line",
		"ensure_dir", "make_executable", "ensure_line_in_file", "replace_in_file":
		c.checkPathArg(where, op, "write", argStr(op, "path", "_0"))
	case "cp", "mv", "copy_dir":
		c.checkPathArg(where, op, "read", argStr(op, "src", "_0"))
		c.checkPathArg(where, op, "write", argStr(op, "dst", "_1"))
	case "read_file", "exists", "is_dir", "is_file", "file_size", "list_dir",
		"read_link", "sha256_file", "md5_file":
		c.checkPathArg(where, op, "read", argStr(op, "path", "_0"))
	}
}

// checkPathArg statically flags a literal fs path outside the declared
// read/write roots. Dynamic paths (`${…}`) defer to the runtime gate.
func (c *checker) checkPathArg(where string, op domain.Op, mode, p string) {
	if p == "" || strings.Contains(p, "${") {
		return
	}
	r := c.prog.Requirements
	if mode == "write" {
		if !pathInRoots(p, r.WriteRoots) {
			c.addErr(where, fmt.Sprintf(
				"%s writes %q which is outside every declared `write` root (add `write \"%s\"`)",
				op.Kind, p, p))
		}
		return
	}
	if !pathInRoots(p, r.ReadRoots) && !pathInRoots(p, r.WriteRoots) {
		c.addErr(where, fmt.Sprintf(
			"%s reads %q which is outside every declared `read` root (add `read \"%s\"`)",
			op.Kind, p, p))
	}
}

// pathInRoots is a textual prefix check (the validator can't canonicalize
// against a runtime cwd). Matches the runtime gate's intent closely enough
// for static flagging; the runtime gate is authoritative.
func pathInRoots(p string, roots []string) bool {
	clean := strings.TrimPrefix(p, "./")
	for _, root := range roots {
		r := strings.TrimPrefix(root, "./")
		if clean == r || strings.HasPrefix(clean, r+"/") {
			return true
		}
	}
	return false
}

// argStr returns the first present string arg among names.
func argStr(op domain.Op, names ...string) string {
	for _, n := range names {
		if v, ok := op.Args[n]; ok {
			if s, ok := v.(string); ok {
				return s
			}
		}
	}
	return ""
}

// firstShellToken extracts the basename of the first non-assignment token
// of a shell command. Mirrors ops.firstShellToken (kept separate to avoid
// a usecase→infra import).
func firstShellToken(raw string) string {
	for _, f := range strings.Fields(raw) {
		if eq := strings.IndexByte(f, '='); eq > 0 && !strings.ContainsAny(f[:eq], " \t") {
			before := f[:eq]
			if before == strings.ToUpper(before) { // GOOS=linux style prefix
				continue
			}
		}
		base := f
		if idx := strings.LastIndexAny(base, "/\\"); idx >= 0 {
			base = base[idx+1:]
		}
		return base
	}
	return ""
}

func isShellBuiltin(name string) bool {
	switch name {
	case "echo", "cd", "true", "false", "pwd", ":", "set", "unset", "export", "test", "[":
		return true
	}
	return false
}

// hostFromURL extracts the hostname from a literal URL. Returns "" if it
// can't parse one (in which case we skip the check rather than false-flag).
func hostFromURL(u string) string {
	s := u
	if i := strings.Index(s, "://"); i >= 0 {
		s = s[i+3:]
	}
	// Strip path / query.
	if i := strings.IndexAny(s, "/?#"); i >= 0 {
		s = s[:i]
	}
	// Strip userinfo and port.
	if i := strings.LastIndexByte(s, '@'); i >= 0 {
		s = s[i+1:]
	}
	if i := strings.LastIndexByte(s, ':'); i >= 0 {
		s = s[:i]
	}
	return strings.ToLower(s)
}

func binDeclared(r domain.Requirements, bin string) bool {
	for _, b := range r.Bins {
		if b.Name == bin {
			return true
		}
	}
	return false
}

func hostDeclared(r domain.Requirements, host string) bool {
	host = strings.ToLower(host)
	for _, h := range r.Hosts {
		want := strings.ToLower(h.Name)
		if want == host {
			return true
		}
		if strings.HasPrefix(want, "*.") && strings.HasSuffix(host, want[1:]) {
			return true
		}
	}
	return false
}

func envDeclared(r domain.Requirements, name string) bool {
	for _, e := range r.Envs {
		if e.Name == name {
			return true
		}
	}
	return false
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

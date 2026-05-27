// Package capyloader compiles a perch .perch source file into a
// domain.Program. The pipeline:
//
//  1. Run the source through the embedded lib.capy via the capy engine.
//     Output is an NDJSON event stream.
//  2. Stream-parse events into a Program: each line corresponds to one
//     of the lib's `write` calls (name, command_begin, config, op, …).
//  3. Fold flat `_enter` / `_leave` op markers into nested Op.Body slices.
package capyloader

import (
	"bufio"
	_ "embed"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/luowensheng/perch/domain"

	"github.com/luowensheng/capy"
)

//go:embed lib.capy
var librarySource string

// Load parses a .perch source file and returns a Program, recursively
// resolving any `import "PATH"` directives.
//
// Special case: path == "-" reads the source from os.Stdin. Imports
// from a stdin-loaded program are resolved against cwd (no $0 to
// derive a directory from). This is what makes piping work:
//
//   curl -fsSL https://.../commands.perch | perch -f - <command>
//
// Imports form a graph: cycles are detected and reported (not followed).
// Each file's transitive imports are merged into the root program's
// command set — flat by default, namespaced when written as
// `import "X" as ALIAS` (commands become callable as `ALIAS.name`).
func Load(path string) (*domain.Program, error) {
	return loadRecursive(path, map[string]bool{})
}

// LoadFromString is Load but reads from an in-memory string. Imports
// declared in the source ARE resolved (against cwd), so a string
// passed via the REPL or the embedded MCP server can still pull in
// sibling files.
func LoadFromString(scriptSrc string) (*domain.Program, error) {
	prog, imports, err := parseOnce(scriptSrc)
	if err != nil {
		return nil, err
	}
	return resolveImports(prog, imports, "", map[string]bool{})
}

// loadRecursive is the file-backed entry point. `visited` tracks every
// absolute path on the current import stack so cycles surface as a
// helpful error rather than infinite recursion. Passed by value (cloned
// at each call) so siblings don't see each other's stacks.
func loadRecursive(path string, visited map[string]bool) (*domain.Program, error) {
	var (
		src     string
		absPath string
	)
	if path == "-" {
		data, err := io.ReadAll(os.Stdin)
		if err != nil {
			return nil, fmt.Errorf("read stdin: %w", err)
		}
		src = string(data)
		absPath = "-"
	} else {
		var err error
		absPath, err = filepath.Abs(path)
		if err != nil {
			absPath = path
		}
		if visited[absPath] {
			return nil, fmt.Errorf("import cycle detected: %s already on the import stack", absPath)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("read %s: %w", path, err)
		}
		src = string(data)
	}

	prog, imports, err := parseOnce(src)
	if err != nil {
		return nil, err
	}
	prog.ScriptPath = absPath

	// Mark this file as visited for the duration of its subtree.
	visited[absPath] = true
	defer delete(visited, absPath)

	// Directory that import paths are resolved against. For stdin,
	// imports resolve against cwd.
	importBase := ""
	if absPath != "-" {
		importBase = filepath.Dir(absPath)
	}
	return resolveImports(prog, imports, importBase, visited)
}

// parseOnce runs one file through capy and returns the program plus
// the list of import directives encountered. Pure (no IO).
func parseOnce(scriptSrc string) (*domain.Program, []importDirective, error) {
	lib, err := capy.NewLibrary(librarySource)
	if err != nil {
		return nil, nil, fmt.Errorf("compile perch library: %w", err)
	}
	stream, err := lib.Run(scriptSrc)
	if err != nil {
		return nil, nil, fmt.Errorf("parse script: %w", err)
	}
	return parseEventStream(stream)
}

// resolveImports loads each import target and merges its commands +
// globals into `prog`. Errors carry the importing file's directive so
// debugging an "import not found" error doesn't require git-bisecting
// the import graph.
func resolveImports(prog *domain.Program, imports []importDirective, base string, visited map[string]bool) (*domain.Program, error) {
	for _, imp := range imports {
		target := imp.Path
		if !filepath.IsAbs(target) && base != "" {
			target = filepath.Join(base, target)
		}
		sub, err := loadRecursive(target, visited)
		if err != nil {
			return nil, fmt.Errorf("import %q: %w", imp.Path, err)
		}
		if err := mergeProgram(prog, sub, imp.Alias); err != nil {
			return nil, fmt.Errorf("import %q: %w", imp.Path, err)
		}
	}
	return prog, nil
}

// mergeProgram folds `from`'s commands into `into`. Flat imports merge
// commands by their bare names; aliased imports prefix each with
// `ALIAS.`. Conflicts surface as errors so duplicate-name bugs don't
// silently win-or-lose. Globals merge silently with parent-wins
// precedence (the importer can override a default from a shared file).
func mergeProgram(into, from *domain.Program, alias string) error {
	for name, cmd := range from.Commands {
		if cmd == nil {
			continue
		}
		// Skip imported private commands on flat import — they're the
		// imported file's internal helpers and shouldn't pollute the
		// caller's namespace. Aliased import keeps them callable as
		// `ALIAS.privateName` for advanced composition.
		if cmd.Modifiers.Private && alias == "" {
			continue
		}
		targetName := name
		if alias != "" {
			targetName = alias + "." + name
		}
		if _, conflict := into.Commands[targetName]; conflict {
			return fmt.Errorf("command %q already declared (conflict from import)", targetName)
		}
		into.Commands[targetName] = cmd
	}
	// Globals: parent wins. If the importer declared NAME, we keep its
	// value; otherwise we adopt the imported value. This gives the
	// caller a "default with override" semantic without surfacing a
	// conflict.
	existing := map[string]bool{}
	for _, g := range into.Globals.Bindings {
		existing[g.Name] = true
	}
	for _, g := range from.Globals.Bindings {
		if !existing[g.Name] {
			into.Globals.Bindings = append(into.Globals.Bindings, g)
		}
	}
	// Catch handlers don't propagate via import — only the root file's
	// catch (if any) is active. Imported catches would race with the
	// importer's, and there's no clear right answer for what wins.
	return nil
}

// event is one line of NDJSON emitted by lib.capy.
type event struct {
	Event       string         `json:"event"`
	Name        string         `json:"name,omitempty"`
	Kind        string         `json:"kind,omitempty"`
	Value       any            `json:"value,omitempty"`
	Args        map[string]any `json:"args,omitempty"`
	CaptureInto string         `json:"capture_into,omitempty"`
	Path        string         `json:"path,omitempty"`  // `import` event
	Alias       string         `json:"alias,omitempty"` // `import` event
}

// importDirective is one `import "PATH" [as ALIAS]` statement from a
// .perch file. Loader-internal; not exposed on domain.Program.
type importDirective struct {
	Path  string // raw path as written
	Alias string // empty for flat, set for namespaced
}

type parserState int

const (
	stTop parserState = iota
	stGlobals
	stCommand
	stCommandArg
	stCommandDo
	stCatch
	stCatchArg
	stCatchDo
)

func parseEventStream(stream string) (*domain.Program, []importDirective, error) {
	prog := &domain.Program{
		Commands: map[string]*domain.Command{},
		Globals:  domain.Globals{Bindings: nil},
	}
	var imports []importDirective

	state := stTop
	var curCmd *domain.Command
	var curCatch *domain.Catch
	var curArg *domain.ArgSpec
	// opStack[0] is the destination slice for the next op (either a
	// command's Ops or a block op's Body). Push on _enter, pop on _leave.
	var opStack []*[]domain.Op

	scanner := bufio.NewScanner(strings.NewReader(stream))
	scanner.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	lineNum := 0
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		lineNum++
		if line == "" {
			continue
		}
		var ev event
		if err := json.Unmarshal([]byte(line), &ev); err != nil {
			return nil, nil, fmt.Errorf("line %d: malformed event %q: %w", lineNum, line, err)
		}

		switch ev.Event {

		case "name":
			prog.Name = asString(ev.Value)
		case "about":
			prog.Description = asString(ev.Value)
		case "version":
			prog.Version = asString(ev.Value)
		case "import":
			// Imports are collected at parse time and resolved by the
			// caller (Load / resolveImports). This keeps parseEventStream
			// pure — no IO. Only valid at the top level.
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: import must be at top level", lineNum)
			}
			imports = append(imports, importDirective{Path: ev.Path, Alias: ev.Alias})

		case "globals_begin":
			state = stGlobals
		case "globals_end":
			state = stTop
		case "global":
			if state != stGlobals {
				return nil, nil, fmt.Errorf("line %d: 'global' event outside globals block", lineNum)
			}
			prog.Globals.Bindings = append(prog.Globals.Bindings, domain.GlobalBinding{
				Name:  ev.Name,
				Type:  inferLiteralType(ev.Value),
				Value: ev.Value,
			})

		case "command_begin":
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: nested command not allowed", lineNum)
			}
			curCmd = &domain.Command{
				Name:      ev.Name,
				Env:       map[string]string{},
				Modifiers: domain.Modifiers{},
			}
			prog.Commands[ev.Name] = curCmd
			state = stCommand
			opStack = nil

		case "command_end":
			if state != stCommand && state != stCommandDo {
				return nil, nil, fmt.Errorf("line %d: command_end while not in command", lineNum)
			}
			curCmd = nil
			state = stTop
			opStack = nil

		case "catch_begin":
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: nested catch not allowed", lineNum)
			}
			curCatch = &domain.Catch{Bind: ev.Name}
			prog.Catch = curCatch
			state = stCatch
			opStack = nil

		case "catch_end":
			curCatch = nil
			state = stTop
			opStack = nil

		case "do_begin":
			switch state {
			case stCommand:
				state = stCommandDo
				opStack = []*[]domain.Op{&curCmd.Ops}
			case stCatch:
				state = stCatchDo
				opStack = []*[]domain.Op{&curCatch.Ops}
			default:
				return nil, nil, fmt.Errorf("line %d: 'do' outside command/catch", lineNum)
			}

		case "do_end":
			switch state {
			case stCommandDo:
				state = stCommand
			case stCatchDo:
				state = stCatch
			default:
				return nil, nil, fmt.Errorf("line %d: 'do_end' without matching do_begin", lineNum)
			}
			opStack = nil

		case "arg_begin":
			switch state {
			case stCommand:
				curArg = &domain.ArgSpec{Name: ev.Name}
				state = stCommandArg
			case stCatch:
				curArg = &domain.ArgSpec{Name: ev.Name}
				state = stCatchArg
			default:
				return nil, nil, fmt.Errorf("line %d: 'arg' outside command/catch config region", lineNum)
			}

		case "arg_end":
			if curArg == nil {
				return nil, nil, fmt.Errorf("line %d: 'arg_end' without matching 'arg'", lineNum)
			}
			if curArg.Type == "" {
				return nil, nil, fmt.Errorf("line %d: arg %q has no `type` field", lineNum, curArg.Name)
			}
			switch state {
			case stCommandArg:
				curCmd.Args = append(curCmd.Args, *curArg)
				state = stCommand
			case stCatchArg:
				// Catch doesn't currently track its own arg list; ignore.
				state = stCatch
			}
			curArg = nil

		case "arg_field":
			if curArg == nil {
				return nil, nil, fmt.Errorf("line %d: '%s' field outside an `arg` block", lineNum, ev.Kind)
			}
			switch ev.Kind {
			case "type":
				curArg.Type = asString(ev.Value)
			case "default":
				curArg.Default = ev.Value
				curArg.HasDefault = true
			case "optional":
				curArg.Optional = true
			case "rest":
				curArg.Rest = true
			case "index":
				idx := int(asFloatish(ev.Value))
				curArg.Index = &idx
			default:
				return nil, nil, fmt.Errorf("line %d: unknown arg field %q", lineNum, ev.Kind)
			}

		case "config":
			// `description` inside an arg block sets the arg's description.
			if curArg != nil && ev.Kind == "description" {
				curArg.Description = asString(ev.Value)
				break
			}
			if err := applyConfig(curCmd, curCatch, ev); err != nil {
				return nil, nil, fmt.Errorf("line %d: %w", lineNum, err)
			}

		case "op":
			if len(opStack) == 0 {
				return nil, nil, fmt.Errorf("line %d: op '%s' outside a do block", lineNum, ev.Kind)
			}
			if ev.Kind == "_enter" {
				// Push a new nested op whose body becomes the active target.
				newOp := domain.Op{
					Kind: ev.Name,
					Args: ev.Args,
				}
				*opStack[len(opStack)-1] = append(*opStack[len(opStack)-1], newOp)
				// Reslice the just-appended op so we can grow its Body.
				dest := opStack[len(opStack)-1]
				idx := len(*dest) - 1
				opStack = append(opStack, &(*dest)[idx].Body)
			} else if ev.Kind == "_leave" {
				if len(opStack) < 2 {
					return nil, nil, fmt.Errorf("line %d: _leave without matching _enter", lineNum)
				}
				opStack = opStack[:len(opStack)-1]
			} else {
				op := domain.Op{
					Kind:        ev.Kind,
					Args:        ev.Args,
					CaptureInto: ev.CaptureInto,
				}
				*opStack[len(opStack)-1] = append(*opStack[len(opStack)-1], op)
			}

		default:
			return nil, nil, fmt.Errorf("line %d: unknown event %q", lineNum, ev.Event)
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, nil, fmt.Errorf("scan: %w", err)
	}

	return prog, imports, nil
}

func applyConfig(cmd *domain.Command, catch *domain.Catch, ev event) error {
	if cmd != nil {
		return applyConfigToCommand(cmd, ev)
	}
	if catch != nil {
		if ev.Kind == "description" {
			catch.Description = asString(ev.Value)
			return nil
		}
		// Catch ignores other config kinds for now.
		return nil
	}
	return fmt.Errorf("config '%s' outside command/catch", ev.Kind)
}

func applyConfigToCommand(c *domain.Command, ev event) error {
	switch ev.Kind {
	case "description":
		c.Description = asString(ev.Value)
	case "private":
		c.Modifiers.Private = true
	case "detached":
		c.Modifiers.Detached = true
	case "proxy_args":
		c.Modifiers.ProxyArgs = true
	case "require_os":
		c.Modifiers.RequireOS = append(c.Modifiers.RequireOS, asString(ev.Value))
	case "require_arch":
		c.Modifiers.RequireArch = append(c.Modifiers.RequireArch, asString(ev.Value))
	case "dir":
		c.Modifiers.Dir = asString(ev.Value)
	case "on_signal":
		c.Modifiers.OnSignal = asString(ev.Value)
	case "env":
		c.Env[ev.Name] = asString(ev.Value)
	default:
		return fmt.Errorf("unknown config kind: %q", ev.Kind)
	}
	return nil
}

func asFloatish(v any) float64 {
	switch x := v.(type) {
	case float64:
		return x
	case int:
		return float64(x)
	case int64:
		return float64(x)
	}
	return 0
}

func asString(v any) string {
	switch x := v.(type) {
	case string:
		return x
	case nil:
		return ""
	default:
		b, _ := json.Marshal(x)
		return string(b)
	}
}

func inferLiteralType(v any) string {
	switch v.(type) {
	case bool:
		return "bool"
	case float64:
		// JSON unmarshal gives all numbers as float64; distinguish int vs
		// float by checking the fractional part.
		f := v.(float64)
		if f == float64(int64(f)) {
			return "int"
		}
		return "float"
	case string:
		return "string"
	default:
		return "string"
	}
}

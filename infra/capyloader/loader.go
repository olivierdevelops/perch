// Package capyloader compiles a perch .capy source file into a
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
	"os"
	"strings"

	"github.com/luowensheng/perch/domain"

	"github.com/luowensheng/capy"
)

//go:embed lib.capy
var librarySource string

// Load parses a .capy source file and returns a Program.
func Load(path string) (*domain.Program, error) {
	script, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}
	return LoadFromString(string(script))
}

// LoadFromString is Load but reads from an in-memory string.
func LoadFromString(scriptSrc string) (*domain.Program, error) {
	lib, err := capy.NewLibrary(librarySource)
	if err != nil {
		return nil, fmt.Errorf("compile perch library: %w", err)
	}
	stream, err := lib.Run(scriptSrc)
	if err != nil {
		return nil, fmt.Errorf("parse script: %w", err)
	}
	return parseEventStream(stream)
}

// event is one line of NDJSON emitted by lib.capy.
type event struct {
	Event       string         `json:"event"`
	Name        string         `json:"name,omitempty"`
	Kind        string         `json:"kind,omitempty"`
	Value       any            `json:"value,omitempty"`
	Argtype     string         `json:"argtype,omitempty"`
	Description string         `json:"description,omitempty"`
	Index       *int           `json:"index,omitempty"`
	Args        map[string]any `json:"args,omitempty"`
	CaptureInto string         `json:"capture_into,omitempty"`
}

type parserState int

const (
	stTop parserState = iota
	stGlobals
	stCommand
	stCommandDo
	stCatch
	stCatchDo
)

func parseEventStream(stream string) (*domain.Program, error) {
	prog := &domain.Program{
		Commands: map[string]*domain.Command{},
		Globals:  domain.Globals{Bindings: nil},
	}

	state := stTop
	var curCmd *domain.Command
	var curCatch *domain.Catch
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
			return nil, fmt.Errorf("line %d: malformed event %q: %w", lineNum, line, err)
		}

		switch ev.Event {

		case "name":
			prog.Name = asString(ev.Value)
		case "about":
			prog.Description = asString(ev.Value)
		case "version":
			prog.Version = asString(ev.Value)

		case "globals_begin":
			state = stGlobals
		case "globals_end":
			state = stTop
		case "global":
			if state != stGlobals {
				return nil, fmt.Errorf("line %d: 'global' event outside globals block", lineNum)
			}
			prog.Globals.Bindings = append(prog.Globals.Bindings, domain.GlobalBinding{
				Name:  ev.Name,
				Type:  inferLiteralType(ev.Value),
				Value: ev.Value,
			})

		case "command_begin":
			if state != stTop {
				return nil, fmt.Errorf("line %d: nested command not allowed", lineNum)
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
				return nil, fmt.Errorf("line %d: command_end while not in command", lineNum)
			}
			curCmd = nil
			state = stTop
			opStack = nil

		case "catch_begin":
			if state != stTop {
				return nil, fmt.Errorf("line %d: nested catch not allowed", lineNum)
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
				return nil, fmt.Errorf("line %d: 'do' outside command/catch", lineNum)
			}

		case "do_end":
			switch state {
			case stCommandDo:
				state = stCommand
			case stCatchDo:
				state = stCatch
			default:
				return nil, fmt.Errorf("line %d: 'do_end' without matching do_begin", lineNum)
			}
			opStack = nil

		case "config":
			if err := applyConfig(curCmd, curCatch, ev); err != nil {
				return nil, fmt.Errorf("line %d: %w", lineNum, err)
			}

		case "op":
			if len(opStack) == 0 {
				return nil, fmt.Errorf("line %d: op '%s' outside a do block", lineNum, ev.Kind)
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
					return nil, fmt.Errorf("line %d: _leave without matching _enter", lineNum)
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
			return nil, fmt.Errorf("line %d: unknown event %q", lineNum, ev.Event)
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("scan: %w", err)
	}

	return prog, nil
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
	case "arg":
		c.Args = append(c.Args, domain.ArgSpec{
			Name:        ev.Name,
			Type:        ev.Argtype,
			Description: ev.Description,
		})
	case "arg_default":
		setArgField(c, ev.Name, func(a *domain.ArgSpec) {
			a.Default = ev.Value
			a.HasDefault = true
		})
	case "arg_index":
		setArgField(c, ev.Name, func(a *domain.ArgSpec) {
			idx := 0
			if ev.Index != nil {
				idx = *ev.Index
			}
			a.Index = &idx
		})
	case "arg_optional":
		setArgField(c, ev.Name, func(a *domain.ArgSpec) {
			a.Optional = true
		})
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

func setArgField(c *domain.Command, name string, fn func(*domain.ArgSpec)) {
	for i := range c.Args {
		if c.Args[i].Name == name {
			fn(&c.Args[i])
			return
		}
	}
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

package interpreter

import (
	"flag"
	"fmt"
	"io"
	"os"
	"runtime"
	"slices"
	"strconv"
	"strings"

	"github.com/luowensheng/perch/domain"
)

// Handler runs one op against the runtime state. It receives interpolated
// args (string values already substituted) and may capture into bindings.
// The interpreter passes itself so block ops can recurse.
type Handler func(i *Interpreter, b *Bindings, args map[string]any) (any, error)

// Interpreter walks a Program's ops, holding the handler registry and
// the IO sinks.
type Interpreter struct {
	Handlers map[string]Handler
	Program  *domain.Program
	Stdout   io.Writer
	Stderr   io.Writer
	Stdin    io.Reader
}

// New constructs an Interpreter with stdout/stderr/stdin defaulted to os.*.
func New(handlers map[string]Handler, p *domain.Program) *Interpreter {
	return &Interpreter{
		Handlers: handlers,
		Program:  p,
		Stdout:   os.Stdout,
		Stderr:   os.Stderr,
		Stdin:    os.Stdin,
	}
}

// Run dispatches to the named command (or the catch handler) with the
// supplied CLI args. Returns the process-style error.
func (i *Interpreter) Run(commandName string, cliArgs []string) error {
	cwd, _ := os.Getwd()
	b := NewBindings(cwd)
	i.seedGlobalsAndEnv(b)

	cmd, ok := i.Program.Commands[commandName]
	if !ok || (cmd != nil && cmd.Modifiers.Private) {
		if i.Program.Catch == nil {
			return fmt.Errorf("command not found: %q", commandName)
		}
		// catch: bind the unknown name plus the full argv as proxy_args
		// (the command name + every remaining arg, joined with spaces).
		// This lets `shell "real-tool ${proxy_args}"` forward unknown
		// invocations through to the real tool — the "extend an existing
		// tool" pattern.
		b.Set(i.Program.Catch.Bind, commandName)
		full := append([]string{commandName}, cliArgs...)
		b.Set("proxy_args", strings.Join(full, " "))
		return i.RunOps(i.Program.Catch.Ops, b)
	}

	if err := i.checkPlatform(cmd); err != nil {
		return err
	}

	parsed, err := i.parseArgs(cmd, cliArgs)
	if err != nil {
		return err
	}
	for k, v := range parsed {
		b.Set(k, v)
	}
	for k, v := range cmd.Env {
		rv, err := Interpolate(v, b)
		if err != nil {
			return err
		}
		b.Env[k] = rv
	}
	if cmd.Modifiers.Dir != "" {
		d, err := Interpolate(cmd.Modifiers.Dir, b)
		if err != nil {
			return err
		}
		b.Cwd = d
	}

	return i.RunOps(cmd.Ops, b)
}

// RunOps walks a slice of ops in order.
func (i *Interpreter) RunOps(ops []domain.Op, b *Bindings) error {
	for _, op := range ops {
		if err := i.RunOp(op, b); err != nil {
			return err
		}
	}
	return nil
}

// RunOp interpolates args and dispatches one op.
func (i *Interpreter) RunOp(op domain.Op, b *Bindings) error {
	args, err := InterpolateArgs(op.Args, b)
	if err != nil {
		return fmt.Errorf("op %s: %w", op.Kind, err)
	}
	// Block ops receive their Body via the op itself, not args; the
	// handler reads i and recurses with RunOps.
	h, ok := i.Handlers[op.Kind]
	if !ok {
		return fmt.Errorf("unknown op: %q", op.Kind)
	}
	// Attach Body for block-op handlers via a sentinel key. Block handlers
	// look at this; non-block handlers ignore.
	if len(op.Body) > 0 {
		// Don't mutate the caller's map.
		argsWithBody := make(map[string]any, len(args)+1)
		for k, v := range args {
			argsWithBody[k] = v
		}
		argsWithBody["_body"] = op.Body
		args = argsWithBody
	}
	val, err := h(i, b, args)
	if err != nil {
		return err
	}
	if op.CaptureInto != "" {
		b.Set(op.CaptureInto, val)
	}
	return nil
}

func (i *Interpreter) seedGlobalsAndEnv(b *Bindings) {
	// Auto-bindings: stable, host-derived values that conditionals can
	// reference without the user declaring them. These appear BEFORE
	// globals so a user-declared global of the same name takes priority.
	b.Set("os", runtime.GOOS)
	b.Set("arch", runtime.GOARCH)
	for _, g := range i.Program.Globals.Bindings {
		b.Set(g.Name, g.Value)
	}
}

func (i *Interpreter) checkPlatform(cmd *domain.Command) error {
	if len(cmd.Modifiers.RequireOS) > 0 && !slices.Contains(cmd.Modifiers.RequireOS, runtime.GOOS) {
		return fmt.Errorf("command %q is restricted to OS in [%s]; running on %s",
			cmd.Name, strings.Join(cmd.Modifiers.RequireOS, ", "), runtime.GOOS)
	}
	if len(cmd.Modifiers.RequireArch) > 0 && !slices.Contains(cmd.Modifiers.RequireArch, runtime.GOARCH) {
		return fmt.Errorf("command %q is restricted to arch in [%s]; running on %s",
			cmd.Name, strings.Join(cmd.Modifiers.RequireArch, ", "), runtime.GOARCH)
	}
	return nil
}

func (i *Interpreter) parseArgs(cmd *domain.Command, cliArgs []string) (map[string]any, error) {
	out := map[string]any{}

	if cmd.Modifiers.ProxyArgs {
		out["proxy_args"] = strings.Join(cliArgs, " ")
		return out, nil
	}

	fs := flag.NewFlagSet(cmd.Name, flag.ContinueOnError)
	fs.SetOutput(io.Discard)

	// Register flags for non-positional args.
	type flagRef struct {
		spec *domain.ArgSpec
		val  any
	}
	refs := []*flagRef{}
	for idx := range cmd.Args {
		a := &cmd.Args[idx]
		if a.Index != nil {
			continue
		}
		var def any
		if a.HasDefault {
			def = a.Default
		}
		ref := &flagRef{spec: a}
		switch a.Type {
		case "string":
			d, _ := def.(string)
			ref.val = fs.String(a.Name, d, a.Description)
		case "bool":
			d, _ := def.(bool)
			ref.val = fs.Bool(a.Name, d, a.Description)
		case "int":
			d := 0
			switch v := def.(type) {
			case int:
				d = v
			case float64:
				d = int(v)
			}
			ref.val = fs.Int(a.Name, d, a.Description)
		case "float":
			d := 0.0
			if f, ok := def.(float64); ok {
				d = f
			}
			ref.val = fs.Float64(a.Name, d, a.Description)
		default:
			return nil, fmt.Errorf("arg %q: unknown type %q", a.Name, a.Type)
		}
		refs = append(refs, ref)
	}

	if err := fs.Parse(cliArgs); err != nil {
		return nil, err
	}
	provided := map[string]bool{}
	fs.Visit(func(f *flag.Flag) { provided[f.Name] = true })

	// Apply parsed flags + check required.
	for _, ref := range refs {
		switch v := ref.val.(type) {
		case *string:
			out[ref.spec.Name] = *v
		case *bool:
			out[ref.spec.Name] = *v
		case *int:
			out[ref.spec.Name] = *v
		case *float64:
			out[ref.spec.Name] = *v
		}
		if !ref.spec.HasDefault && !ref.spec.Optional && !provided[ref.spec.Name] {
			return nil, fmt.Errorf("missing required argument -%s", ref.spec.Name)
		}
	}

	// Positional args.
	for idx := range cmd.Args {
		a := &cmd.Args[idx]
		if a.Index == nil {
			continue
		}
		if *a.Index >= fs.NArg() {
			if !a.HasDefault && !a.Optional {
				return nil, fmt.Errorf("missing positional argument #%d (%s)", *a.Index, a.Name)
			}
			out[a.Name] = a.Default
			continue
		}
		raw := fs.Arg(*a.Index)
		switch a.Type {
		case "string":
			out[a.Name] = raw
		case "int":
			n, err := strconv.Atoi(raw)
			if err != nil {
				return nil, fmt.Errorf("arg %s: invalid int %q", a.Name, raw)
			}
			out[a.Name] = n
		case "float":
			f, err := strconv.ParseFloat(raw, 64)
			if err != nil {
				return nil, fmt.Errorf("arg %s: invalid float %q", a.Name, raw)
			}
			out[a.Name] = f
		case "bool":
			b, err := strconv.ParseBool(raw)
			if err != nil {
				return nil, fmt.Errorf("arg %s: invalid bool %q", a.Name, raw)
			}
			out[a.Name] = b
		}
	}
	return out, nil
}

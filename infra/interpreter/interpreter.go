package interpreter

import (
	"flag"
	"fmt"
	"io"
	"os"
	"os/user"
	"path/filepath"
	"runtime"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/luowensheng/perch/domain"
)

// Handler runs one op against the runtime state. It receives interpolated
// args (string values already substituted) and may capture into bindings.
// The interpreter passes itself so block ops can recurse.
type Handler func(i *Interpreter, b *Bindings, args map[string]any) (any, error)

// OpAction is the verdict from a BeforeOp hook — run the op, skip it,
// run this op + everything else without further asking, or stop the
// whole walk. Powers --ask / --dry-run / --step previews.
type OpAction int

const (
	ActRun     OpAction = iota // execute as normal
	ActSkip                    // don't execute; if `let X = …` set X to ""
	ActRunAll                  // execute this op, then clear BeforeOp
	ActQuit                    // stop the whole command immediately
)

// ErrQuit is returned by Run / RunOps when the BeforeOp hook returns
// ActQuit. Callers treat it as a clean stop rather than a failure.
var ErrQuit = fmt.Errorf("interpreter: quit requested")

// BeforeOp is an optional pre-dispatch hook. When non-nil, the
// interpreter calls it before each op (with already-interpolated args)
// and respects the returned action. nil = no hook = normal execution.
type BeforeOp func(op domain.Op, args map[string]any, b *Bindings) OpAction

// Interpreter walks a Program's ops, holding the handler registry and
// the IO sinks.
type Interpreter struct {
	Handlers map[string]Handler
	Program  *domain.Program
	Stdout   io.Writer
	Stderr   io.Writer
	Stdin    io.Reader
	// BeforeOp, when set, is consulted before each op dispatch. Used by
	// --ask (step-through confirmation) and --dry-run (skip everything,
	// just print). nil means "run normally."
	BeforeOp BeforeOp
	// EnvAllowlist, when non-nil, restricts which host env vars resolve
	// via ${NAME} fallthrough. Wired into every fresh Bindings made by
	// Run. nil means "all host env vars visible" (legacy).
	EnvAllowlist map[string]bool
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
	b.EnvAllowlist = i.EnvAllowlist
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

	err = i.RunOps(cmd.Ops, b)
	if err == ErrQuit {
		fmt.Fprintln(i.Stdout, "↪ stopped by user")
		return nil
	}
	return err
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
	// Consult the optional preview hook BEFORE dispatch so previews see
	// the same interpolated args the handler would.
	if i.BeforeOp != nil {
		switch i.BeforeOp(op, args, b) {
		case ActSkip:
			// Skipped: capture an empty value so downstream ${X} still
			// resolves (to ""), keeping interpolation alive in dry-run.
			if op.CaptureInto != "" {
				b.Set(op.CaptureInto, "")
			}
			return nil
		case ActQuit:
			return ErrQuit
		case ActRunAll:
			i.BeforeOp = nil // disable hook for the rest of the walk
		}
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
	//
	// The full catalog is intentionally large because cross-platform
	// install / build / uninstall scripts otherwise paper over differences
	// with shell glue. Pre-binding ${home}, ${cache_dir}, ${exe_path},
	// ${path_sep}, ${exe_ext}, ${is_windows} etc. removes that need.
	b.Set("os", runtime.GOOS)
	b.Set("arch", runtime.GOARCH)
	b.Set("is_windows", runtime.GOOS == "windows")
	b.Set("is_macos", runtime.GOOS == "darwin")
	b.Set("is_linux", runtime.GOOS == "linux")
	b.Set("is_unix", runtime.GOOS != "windows")
	b.Set("is_arm64", runtime.GOARCH == "arm64")
	b.Set("is_amd64", runtime.GOARCH == "amd64")
	b.Set("cpu_count", runtime.NumCPU())
	b.Set("pid", os.Getpid())
	b.Set("now_unix", time.Now().Unix())

	// Path / filesystem conventions that differ by OS.
	if runtime.GOOS == "windows" {
		b.Set("path_sep", "\\")
		b.Set("path_list_sep", ";")
		b.Set("exe_ext", ".exe")
		b.Set("null_device", "NUL")
		b.Set("shell_name", "cmd")
	} else {
		b.Set("path_sep", "/")
		b.Set("path_list_sep", ":")
		b.Set("exe_ext", "")
		b.Set("null_device", "/dev/null")
		b.Set("shell_name", "bash")
	}

	// Standard directories. Each falls back to "" rather than crashing
	// if the platform can't report it.
	if h, err := os.UserHomeDir(); err == nil {
		b.Set("home", h)
		b.Set("home_dir", h)
	}
	if d, err := os.UserConfigDir(); err == nil {
		b.Set("config_dir", d)
	}
	if d, err := os.UserCacheDir(); err == nil {
		b.Set("cache_dir", d)
	}
	b.Set("temp_dir", os.TempDir())
	switch runtime.GOOS {
	case "windows":
		b.Set("data_dir", os.Getenv("APPDATA"))
	case "darwin":
		if h, err := os.UserHomeDir(); err == nil {
			b.Set("data_dir", filepath.Join(h, "Library", "Application Support"))
		}
	default:
		if h, err := os.UserHomeDir(); err == nil {
			b.Set("data_dir", filepath.Join(h, ".local", "share"))
		}
	}

	// The running binary itself.
	if exe, err := os.Executable(); err == nil {
		if resolved, err := filepath.EvalSymlinks(exe); err == nil {
			exe = resolved
		}
		b.Set("exe_path", exe)
		b.Set("exe_dir", filepath.Dir(exe))
		b.Set("exe_name", filepath.Base(exe))
	}

	// The source .perch file (empty when embedded inside a built binary).
	if i.Program != nil && i.Program.ScriptPath != "" {
		b.Set("script_path", i.Program.ScriptPath)
		b.Set("script_dir", filepath.Dir(i.Program.ScriptPath))
	} else {
		b.Set("script_path", "")
		b.Set("script_dir", "")
	}

	// Identity.
	if u, err := user.Current(); err == nil {
		b.Set("user", u.Username)
		b.Set("uid", u.Uid)
	} else {
		b.Set("user", os.Getenv("USER"))
	}
	if h, err := os.Hostname(); err == nil {
		b.Set("hostname", h)
	}
	// Globals are seeded in declared order. String values are
	// interpolated against the bindings built so far, so globals can
	// reference earlier globals and host env (e.g. ${HOME}) at seed
	// time — no recursive substitution at op-run time.
	for _, g := range i.Program.Globals.Bindings {
		if s, ok := g.Value.(string); ok {
			if rv, err := Interpolate(s, b); err == nil {
				b.Set(g.Name, rv)
				continue
			}
		}
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

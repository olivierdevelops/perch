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

	"github.com/olivierdevelops/perch/domain"
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
	ActRun    OpAction = iota // execute as normal
	ActSkip                   // don't execute; if `let X = …` set X to ""
	ActRunAll                 // execute this op, then clear BeforeOp
	ActQuit                   // stop the whole command immediately
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
	// AllowedShellBins, when non-nil, restricts the first token of every
	// `shell` command to the listed names. Without this, even with
	// `--env` scrubbing the subprocess could be anything (`curl`,
	// `nc`, `cp ~/.ssh/...`). With it, the user pre-declares which
	// binaries may be invoked. nil = no restriction.
	AllowedShellBins map[string]bool
	// NoShellMetachars, when true, rejects shell commands containing
	// pipes / redirects / command-substitution / && / || / `;`. Combined
	// with AllowedShellBins this neutralizes most shell-injection
	// vectors even inside an otherwise-allowed shell call.
	NoShellMetachars bool
	// AfterOp, when set, is called AFTER each op's handler returns —
	// with the op, its interpolated args, the result value, the error
	// (or nil), and the duration. Used by `--audit FILE.ndjson` to
	// produce a structured trace of every op call. Zero-overhead when nil.
	AfterOp AfterOp
	// Deadline, when non-zero, caps the wall-clock budget for the
	// invocation. Checked before each op dispatch — a long-running shell
	// can't be interrupted mid-call, but the NEXT op after it returns
	// ErrTimeout. Set by `--max-runtime SECS`.
	Deadline time.Time
	// HTTPPolicy governs http_get / http_post / download redirect and
	// destination behaviour. nil = secure defaults (no private IPs, no
	// scheme downgrade, max 5 hops). Set by `--max-redirects`,
	// `--no-redirects`, `--allow-private-ips`, `--allow-scheme-downgrade`.
	HTTPPolicy *HTTPPolicy
	// Tracer, when set, receives Before/After events for every op. Used
	// to build the --report span tree. The Tracer's nesting structure
	// falls out naturally from RunOp's recursion — a block op's children
	// emit their Before/After between the block's Before and After.
	Tracer Tracer
	// PreflightHook, when non-nil, is called once per command invocation
	// BEFORE any op fires (after platform/args checks). Used by the
	// `requires` block enforcer (infra/ops/requires.go) to verify the
	// host machine satisfies the file's declared manifest. Wired up in
	// orchestrator/main.go to avoid an infra/ops → infra/interpreter
	// → infra/ops circular import.
	PreflightHook func(*Interpreter, *domain.Program) error
}

// HTTPPolicy gates which URLs perch's HTTP ops will dial and which
// redirects they'll follow. See infra/ops/http.go for the enforcement.
type HTTPPolicy struct {
	// MaxRedirects caps the redirect chain. 0 = refuse any redirect.
	MaxRedirects int
	// AllowPrivateIPs disables the SSRF guard. With this off (default),
	// perch refuses to dial any host that resolves to a private /
	// loopback / link-local / unspecified IP — closes the AWS-metadata
	// SSRF and the localhost-pivot.
	AllowPrivateIPs bool
	// AllowSchemeDowngrade permits https → http redirects. Off by
	// default — a 30x downgrade is almost always an attack signal.
	AllowSchemeDowngrade bool
	// AllowedHosts, when non-empty, restricts EVERY request URL (and
	// every redirect destination) to the listed hosts. Patterns:
	//   - exact:        api.github.com
	//   - single-label: *.s3.amazonaws.com     (matches api.x.com NOT a.b.x.com)
	//   - IP literal:   10.0.0.1
	//   - host:port:    localhost:8080         (port must match exactly)
	// Composes AND-wise with the SSRF guard — a host in the allowlist
	// still has to pass the private-IP check unless --allow-private-ips
	// is also set.
	AllowedHosts []string
}

// AfterOp receives each op's outcome after its handler returns. Used by
// the audit log (infra/audit) to record a structured trace.
type AfterOp func(op domain.Op, args map[string]any, b *Bindings, result any, err error, dur time.Duration)

// Tracer is a paired Before/After hook independent of --ask / --dry-run.
// Block ops naturally nest (their children's Before/After fire between
// the block's Before and After), so a tracer that maintains a stack
// builds a span tree for free. Used by --report to produce the post-run
// tree view.
type Tracer interface {
	Before(op domain.Op, args map[string]any)
	After(op domain.Op, result any, err error, dur time.Duration)
}

// ErrTimeout is returned when the interpreter exceeds its Deadline.
var ErrTimeout = fmt.Errorf("interpreter: --max-runtime exceeded")

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

// RunPrivate is Run that bypasses the private/test gate. Used by the
// test runner to invoke `test`-marked commands directly. Otherwise
// identical to Run.
func (i *Interpreter) RunPrivate(commandName string, cliArgs []string) error {
	return i.runCommand(commandName, cliArgs, true)
}

// Run dispatches to the named command (or the catch handler) with the
// supplied CLI args. Returns the process-style error.
func (i *Interpreter) Run(commandName string, cliArgs []string) error {
	return i.runCommand(commandName, cliArgs, false)
}

func (i *Interpreter) runCommand(commandName string, cliArgs []string, allowPrivate bool) error {
	cwd, _ := os.Getwd()
	b := NewBindings(cwd)
	b.EnvAllowlist = i.EnvAllowlist
	i.seedGlobalsAndEnv(b)

	cmd, ok := i.Program.Commands[commandName]
	// Test-marked commands are only callable via `perch test`. From the
	// regular run path they look like `private` — treat them the same.
	// The test runner uses RunPrivate (allowPrivate=true) to invoke
	// them anyway.
	gated := cmd != nil && (cmd.Modifiers.Private || cmd.Modifiers.Test) && !allowPrivate
	if !ok || gated {
		if i.Program.Catch == nil {
			return fmt.Errorf("command not found: %q", commandName)
		}
		// catch: bind the unknown name. ${proxy_args} (the full unknown
		// invocation joined with spaces) is bound ONLY if the catch
		// declared the `proxy_args` modifier — otherwise referencing it
		// fails with unresolved_var, which is the right behavior for the
		// "I don't intend to forward arbitrary input to shell" case.
		// Declaring `proxy_args` is the explicit opt-in to the
		// catch→shell forwarding pattern that --scan flags as HIGH.
		b.Set(i.Program.Catch.Bind, commandName)
		if i.Program.Catch.ProxyArgs {
			full := append([]string{commandName}, cliArgs...)
			b.Set("proxy_args", strings.Join(full, " "))
		}
		return i.RunOps(i.Program.Catch.Ops, b)
	}

	if err := i.checkPlatform(cmd); err != nil {
		return err
	}

	// File-declared `requires` manifest preflight. Hook is wired by the
	// orchestrator to ops.Preflight; nil when running in contexts where
	// requirements shouldn't be enforced (e.g. `perch --check`).
	if i.PreflightHook != nil {
		if err := i.PreflightHook(i, i.Program); err != nil {
			return err
		}
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
	if err == ErrTimeout {
		fmt.Fprintln(i.Stderr, "↪ stopped: --max-runtime exceeded")
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
	// Wall-clock budget: refuse to start a new op if we're past the
	// deadline. We can't interrupt a long-running op mid-call (Go's
	// exec.Cmd needs context-aware wiring for that), but we can prevent
	// the next one from firing.
	if !i.Deadline.IsZero() && time.Now().After(i.Deadline) {
		return ErrTimeout
	}
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
		// Unknown op kind → treat it as a bare declared-bin invocation (exec):
		// the kind IS the bin. This makes `let r = docker ps` work without the
		// `exec` keyword for the short-arg capture forms that the grammar would
		// otherwise resolve to an op-kind. The exec handler applies the same
		// requires/bin gate, so an undeclared bin still fails bin_not_declared,
		// and a genuine op typo surfaces as that error too. Block ops never
		// reach here (their kinds are registered handlers).
		execH, hasExec := i.Handlers["exec"]
		if !hasExec || op.Kind == "exec" {
			return fmt.Errorf("unknown op: %q", op.Kind)
		}
		na := make(map[string]any, len(args)+1)
		for k, v := range args {
			na[k] = v
		}
		na["bin"] = op.Kind
		args = na
		op.Kind = "exec"
		h = execH
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
	// Signal capture intent so output-producing ops (exec, pipe) can stay
	// quiet when their result is bound (`let x = exec …`) but stream when
	// used as a bare statement — matching the old shell vs shell_output split.
	if op.CaptureInto != "" {
		if _, ok := args["_capture"]; !ok {
			argsWithCap := make(map[string]any, len(args)+1)
			for k, v := range args {
				argsWithCap[k] = v
			}
			argsWithCap["_capture"] = true
			args = argsWithCap
		}
	}
	if i.Tracer != nil {
		i.Tracer.Before(op, args)
	}
	start := time.Now()
	val, err := h(i, b, args)
	dur := time.Since(start)
	if i.AfterOp != nil {
		i.AfterOp(op, args, b, val, err, dur)
	}
	if i.Tracer != nil {
		i.Tracer.After(op, val, err, dur)
	}
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

// ParseCLIArgs exposes the CLI argv → arg-name-map parsing used by Run.
// Used by `opRun` (in infra/ops) so `run NAME -arg=value` inside a body
// goes through the same parser as `perch NAME -arg=value` from a shell.
func (i *Interpreter) ParseCLIArgs(cmd *domain.Command, cliArgs []string) (map[string]any, error) {
	return i.parseArgs(cmd, cliArgs)
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
		// Rest arg: gather every remaining positional from this index on
		// into a newline-joined string plus a count. The validator
		// ensures this is the last arg and type == "string".
		if a.Rest {
			start := *a.Index
			values := []string{}
			if start < fs.NArg() {
				for k := start; k < fs.NArg(); k++ {
					values = append(values, fs.Arg(k))
				}
			}
			out[a.Name] = strings.Join(values, "\n")
			out[a.Name+"_count"] = len(values)
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

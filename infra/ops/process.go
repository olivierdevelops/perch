package ops

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

// lockedWriter serializes concurrent writes to an underlying io.Writer. The
// pipe stages run as concurrent processes whose stderr-copy goroutines all
// target the program's stderr; without serialization, writing to a shared
// unsynchronized writer (e.g. a *bytes.Buffer in tests) is a data race.
type lockedWriter struct {
	mu sync.Mutex
	w  io.Writer
}

func (l *lockedWriter) Write(p []byte) (int, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.w.Write(p)
}

func registerProcess(m map[string]interpreter.Handler) {
	m["print"] = opPrint
	m["println"] = opPrintln
	m["eprintln"] = opEprintln
	m["shell"] = opShell
	m["shell_output"] = opShellOutput
	m["shell_detached"] = opShellDetached
	m["exec"] = opExec
	m["exec_chain"] = opExecChain
	m["pipe"] = opPipe
	m["fail"] = opFail
	m["exit"] = opExit
	m["sleep"] = opSleep
	m["run"] = opRun
	m["list_commands"] = opListCommands
	m["try_shell"] = opTryShell
	m["shell_in"] = opShellIn
	m["process_running"] = opProcessRunning
	m["kill_by_name"] = opKillByName
}

// opTryShell runs a shell command and returns true on success, false on
// failure — without erroring. Use when you genuinely want a probe (e.g.
// `let docker_ok = try_shell "docker info"`).
func opTryShell(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	cmd := argString(args, "cmd", "_0")
	// Same allowlist / metachar checks as `shell`. A blocked call returns
	// the error rather than being silently false — the user explicitly
	// restricted the surface, and a silent false would lie about why.
	if err := checkShell(i, cmd); err != nil {
		return false, err
	}
	c := buildShell(cmd)
	c.Dir = b.Cwd
	applyEnv(c, b)
	return c.Run() == nil, nil
}

// opShellIn runs a shell command in an explicit directory, regardless
// of the current binding cwd. Equivalent to `dir "X"` config + `shell`,
// but local to one op.
func opShellIn(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	dir := argString(args, "dir", "_0")
	cmd := argString(args, "cmd", "_1")
	if err := checkShell(i, cmd); err != nil {
		return nil, err
	}
	c := buildShell(cmd)
	if dir == "" {
		c.Dir = b.Cwd
	} else {
		c.Dir = dir
	}
	applyEnv(c, b)
	c.Stdout = i.Stdout
	c.Stderr = i.Stderr
	c.Stdin = i.Stdin
	return nil, c.Run()
}

// opProcessRunning: does any process with the given name (substring
// match) currently exist? Uses tasklist on Windows, pgrep elsewhere.
func opProcessRunning(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	if name == "" {
		return false, nil
	}
	tool := "pgrep"
	if runtime.GOOS == "windows" {
		tool = "tasklist"
	}
	if err := CheckSubprocessBin(i, tool); err != nil {
		return false, err
	}
	if runtime.GOOS == "windows" {
		out, _ := exec.Command("tasklist", "/FI", "IMAGENAME eq "+name).CombinedOutput()
		return strings.Contains(strings.ToLower(string(out)), strings.ToLower(name)), nil
	}
	return exec.Command("pgrep", "-f", name).Run() == nil, nil
}

// opKillByName kills processes matching NAME. Best-effort; errors are
// swallowed so re-running an uninstall after a partial cleanup is safe.
func opKillByName(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	if name == "" {
		return nil, nil
	}
	tool := "pkill"
	if runtime.GOOS == "windows" {
		tool = "taskkill"
	}
	if err := CheckSubprocessBin(i, tool); err != nil {
		return nil, err
	}
	if runtime.GOOS == "windows" {
		_ = exec.Command("taskkill", "/F", "/IM", name).Run()
		return nil, nil
	}
	_ = exec.Command("pkill", "-f", name).Run()
	return nil, nil
}

func argString(args map[string]any, names ...string) string {
	for _, n := range names {
		if v, ok := args[n]; ok {
			return interpreter.ToStringValue(v)
		}
	}
	return ""
}

func opPrint(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	fmt.Fprintln(i.Stdout, argString(args, "msg", "_0"))
	return nil, nil
}

func opPrintln(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	fmt.Fprintln(i.Stdout, argString(args, "msg", "_0"))
	return nil, nil
}

func opEprintln(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	fmt.Fprintln(i.Stderr, argString(args, "msg", "_0"))
	return nil, nil
}

// checkShell enforces the two user-side defenses against subprocess
// escape when shell is allowed: a binary allowlist (first token must
// be permitted) and a metachar filter (no pipes / redirects / && / ;
// / $(...) / backticks). Returns nil when the command passes both
// checks (or when neither is active).
func checkShell(i *interpreter.Interpreter, raw string) error {
	// File-declared manifest enforcement (no-op unless `requires` declared).
	if err := CheckShellBinDeclared(i, raw); err != nil {
		return err
	}
	if i.NoShellMetachars {
		for _, ch := range []string{"|", ">", "<", "&", ";", "`", "$("} {
			if strings.Contains(raw, ch) {
				return domain.NewOpError("shell", domain.ErrShellMetacharsDenied,
					fmt.Sprintf("shell metachar %q rejected by --no-shell-metachars", ch),
				).WithDetail(raw)
			}
		}
	}
	if i.AllowedShellBins != nil {
		fields := strings.Fields(raw)
		first := ""
		for _, f := range fields {
			if strings.Contains(f, "=") && !strings.ContainsAny(f, " \t") {
				continue
			}
			first = f
			break
		}
		if first == "" {
			return domain.NewOpError("shell", domain.ErrShellBinNotAllowed,
				"empty command rejected by --allow-bin")
		}
		base := first
		if idx := strings.LastIndexAny(base, "/\\"); idx >= 0 {
			base = base[idx+1:]
		}
		if !i.AllowedShellBins[base] {
			names := make([]string, 0, len(i.AllowedShellBins))
			for n := range i.AllowedShellBins {
				names = append(names, n)
			}
			return domain.NewOpError("shell", domain.ErrShellBinNotAllowed,
				fmt.Sprintf("binary %q is not in --allow-bin (allowed: %s)",
					base, strings.Join(names, ", "))).WithDetail(base)
		}
	}
	return nil
}

func opShell(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	raw := argString(args, "cmd", "_0")
	if err := checkShell(i, raw); err != nil {
		return nil, err
	}
	cmd := buildShell(raw)
	applyEnv(cmd, b)
	cmd.Dir = b.Cwd
	cmd.Stdin = i.Stdin
	cmd.Stdout = i.Stdout
	cmd.Stderr = i.Stderr
	if err := cmd.Run(); err != nil {
		return nil, tagShellErr(err, raw)
	}
	return nil, nil
}

// tagShellErr wraps a shell exec error with an appropriate ErrorKind so
// `match err.kind` can discriminate. Exit-code !=0 → shell_exit_nonzero;
// signal-killed → shell_signal_killed.
func tagShellErr(err error, cmd string) error {
	if err == nil {
		return nil
	}
	if exitErr, ok := err.(*exec.ExitError); ok {
		oe := domain.NewOpError("shell", domain.ErrShellExitNonzero, err.Error())
		oe.Code = strconv.Itoa(exitErr.ExitCode())
		oe.Detail = cmd
		// Signal-killed has ExitCode -1 on most platforms.
		if exitErr.ExitCode() == -1 {
			oe.Kind = domain.ErrShellSignalKilled
		}
		return oe
	}
	return domain.NewOpError("shell", domain.ErrShellExitNonzero, err.Error()).WithDetail(cmd)
}

func opShellOutput(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	raw := argString(args, "cmd", "_0")
	if err := checkShell(i, raw); err != nil {
		return "", err
	}
	cmd := buildShell(raw)
	applyEnv(cmd, b)
	cmd.Dir = b.Cwd
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = i.Stderr
	if err := cmd.Run(); err != nil {
		return strings.TrimRight(out.String(), "\n"), tagShellErr(err, raw)
	}
	return strings.TrimRight(out.String(), "\n"), nil
}

// opExec runs a DECLARED binary directly — never through a shell. The
// first arg ("bin") is the binary; "_0".."_N" are argv slots, each passed
// untouched (no word-splitting, no glob, no metachar surface). This is the
// shell-free subprocess primitive from docs/sandboxed-by-design.md §3.2.
//
// Gating: identical to `shell`'s first-token check — when a `requires`
// block is present, the bin must be a declared `bin "…"` or the op fails
// bin_not_declared. The capability mask (`sandbox no_subprocess`) and any
// hash pin still apply through the same paths shell uses.
//
// stdout is tee'd to the program's stdout AND captured as the op's return
// value, so a bare `exec git status` streams while `let h = exec git
// "rev-parse" "HEAD"` captures.
func opExec(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	bin := argString(args, "bin")
	if bin == "" {
		return nil, domain.NewOpError("exec", domain.ErrUnclassified, "exec: missing binary")
	}
	// Capability + manifest gate. Reuses the same declared-bin enforcement
	// as `shell` (and the CapMask no_subprocess / allow-bin checks).
	if err := checkExecBin(i, bin); err != nil {
		return nil, err
	}
	// Collect argv slots _0.._N in order until the first gap.
	var argv []string
	for n := 0; ; n++ {
		v, ok := args[fmt.Sprintf("_%d", n)]
		if !ok {
			break
		}
		argv = append(argv, interpreter.ToStringValue(v))
	}
	cmd := exec.Command(resolveExecPath(i, bin), argv...)
	applyEnv(cmd, b)
	cmd.Dir = b.Cwd
	// Capture stdout into a buffer (so `let x = exec …` works), then tee
	// the captured bytes to the program's stdout so a bare `exec …` streams.
	// We capture-then-write rather than io.MultiWriter(i.Stdout, …) because
	// the latter races with exec's stdout-copy goroutine against a plain
	// bytes.Buffer (the buffer the caller reads can be a different alias of
	// the writer the goroutine flushes into).
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = i.Stderr
	cmd.Stdin = i.Stdin
	display := bin
	if len(argv) > 0 {
		display = bin + " " + strings.Join(argv, " ")
	}
	runErr := cmd.Run()
	// Tee stdout to the program's stdout for a bare `exec …`, but stay quiet
	// when the result is bound (`let x = exec …`) — matching the old
	// shell (stream) vs shell_output (silent capture) split.
	if i.Stdout != nil && !truthyValue(args["_capture"]) {
		_, _ = i.Stdout.Write(out.Bytes())
	}
	if runErr != nil {
		return strings.TrimRight(out.String(), "\n"), tagShellErr(runErr, display)
	}
	return strings.TrimRight(out.String(), "\n"), nil
}

// opExecChain runs `exec a && exec b || exec c ; exec d` — a chain of exec
// clauses joined by perch-level operators (NOT shell metachars; they're
// literal source tokens the loader folded into Body + Args["ops"], so an
// interpolated ${x} can never become an operator — the §3.3 keystone).
//
// Each clause is a child exec op in Body. Operators drive short-circuit
// evaluation on the previous clause's exit status:
//
//	&&  run next only if the previous clause SUCCEEDED
//	||  run next only if the previous clause FAILED
//	;   always run next
//
// The chain's result is the error of the LAST actually-run clause (so a bare
// chain aborts on a real failure; wrap in `try` or use `||` to continue).
func opExecChain(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	ops := toStringSlice(args["ops"])
	if len(body) == 0 {
		return nil, nil
	}
	runClause := func(op domain.Op) error {
		a, err := interpreter.InterpolateArgs(op.Args, b)
		if err != nil {
			return err
		}
		_, e := opExec(i, b, a)
		return e
	}
	lastErr := runClause(body[0])
	for k := 1; k < len(body); k++ {
		op := ""
		if k-1 < len(ops) {
			op = ops[k-1]
		}
		run := false
		switch op {
		case "&&":
			run = lastErr == nil
		case "||":
			run = lastErr != nil
		default: // ";"
			run = true
		}
		if run {
			lastErr = runClause(body[k])
		}
	}
	return nil, lastErr
}

func toStringSlice(v any) []string {
	switch xs := v.(type) {
	case []string:
		return xs
	case []any:
		out := make([]string, 0, len(xs))
		for _, x := range xs {
			out = append(out, interpreter.ToStringValue(x))
		}
		return out
	}
	return nil
}

// opPipe runs a `pipe ... end` block: each body stage is an `exec BIN …`,
// and perch wires stage N's stdout into stage N+1's stdin with in-process
// OS pipes — no shell, no `sh -c`. The block's value is the final stage's
// stdout (captured for `let out = pipe … end`) and is also streamed.
// docs/sandboxed-by-design.md §3.5. Each stage's bin is gated exactly like
// a standalone `exec`.
func opPipe(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	var stages []*exec.Cmd
	var display []string
	// All stages run concurrently and share the program's stderr; wrap it so
	// their stderr-copy goroutines serialize writes (no data race on a shared
	// buffer). nil stays nil (discarded).
	var sharedErr io.Writer
	if i.Stderr != nil {
		sharedErr = &lockedWriter{w: i.Stderr}
	}
	for _, op := range body {
		if op.Kind != "exec" {
			return nil, domain.NewOpError("pipe", domain.ErrUnclassified,
				fmt.Sprintf("pipe: every stage must be `exec`, got %q", op.Kind))
		}
		a, err := interpreter.InterpolateArgs(op.Args, b)
		if err != nil {
			return nil, err
		}
		bin := argString(a, "bin")
		if bin == "" {
			return nil, domain.NewOpError("pipe", domain.ErrUnclassified, "pipe: exec stage missing binary")
		}
		if err := checkExecBin(i, bin); err != nil {
			return nil, err
		}
		var argv []string
		for n := 0; ; n++ {
			v, ok := a[fmt.Sprintf("_%d", n)]
			if !ok {
				break
			}
			argv = append(argv, interpreter.ToStringValue(v))
		}
		c := exec.Command(resolveExecPath(i, bin), argv...)
		applyEnv(c, b)
		c.Dir = b.Cwd
		c.Stderr = sharedErr
		stages = append(stages, c)
		d := bin
		if len(argv) > 0 {
			d = bin + " " + strings.Join(argv, " ")
		}
		display = append(display, d)
	}
	if len(stages) == 0 {
		return "", nil
	}
	// Wire stage[k-1].stdout -> stage[k].stdin via OS pipes.
	for k := 1; k < len(stages); k++ {
		pr, err := stages[k-1].StdoutPipe()
		if err != nil {
			return "", err
		}
		stages[k].Stdin = pr
	}
	stages[0].Stdin = i.Stdin
	var out bytes.Buffer
	stages[len(stages)-1].Stdout = &out
	for _, c := range stages {
		if err := c.Start(); err != nil {
			return "", tagShellErr(err, strings.Join(display, " | "))
		}
	}
	var firstErr error
	for k, c := range stages {
		if err := c.Wait(); err != nil && firstErr == nil {
			firstErr = tagShellErr(err, display[k])
		}
	}
	if i.Stdout != nil && !truthyValue(args["_capture"]) {
		_, _ = i.Stdout.Write(out.Bytes())
	}
	res := strings.TrimRight(out.String(), "\n")
	return res, firstErr
}

func opShellDetached(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	raw := argString(args, "cmd", "_0")
	if err := checkShell(i, raw); err != nil {
		return nil, err
	}
	cmd := buildShell(raw)
	applyEnv(cmd, b)
	cmd.Dir = b.Cwd
	return nil, cmd.Start()
}

func opFail(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	msg := argString(args, "msg", "_0")
	if msg == "" {
		msg = "(no message)"
	}
	return nil, domain.NewOpError("fail", domain.ErrUserFail, msg)
}

func opExit(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	code := 0
	if v, ok := args["code"]; ok {
		switch x := v.(type) {
		case int:
			code = x
		case float64:
			code = int(x)
		case string:
			n, _ := strconv.Atoi(x)
			code = n
		}
	}
	os.Exit(code)
	return nil, nil
}

func opSleep(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	secs := 0.0
	if v, ok := args["seconds"]; ok {
		switch x := v.(type) {
		case float64:
			secs = x
		case int:
			secs = float64(x)
		case string:
			secs, _ = strconv.ParseFloat(x, 64)
		}
	}
	time.Sleep(time.Duration(secs * float64(time.Second)))
	return nil, nil
}

func opRun(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	target := argString(args, "target")
	if target == "" {
		return nil, errors.New("run: missing target")
	}
	cmd, ok := i.Program.Commands[target]
	if !ok {
		return nil, fmt.Errorf("run: unknown command %q", target)
	}
	// Collect trailing arg tokens (a0, a1, …) emitted by the run_Nargs
	// grammar overloads. These are CLI-style — `-name=value`,
	// `-name value`, `--name=value` — and feed the same parseArgs
	// used by `perch NAME -arg=value` from a shell. Up to 8 args.
	var cliArgs []string
	for n := 0; n < 8; n++ {
		key := fmt.Sprintf("_%d", n)
		v, present := args[key]
		if !present {
			break
		}
		s := interpreter.ToStringValue(v)
		if s == "" {
			break
		}
		cliArgs = append(cliArgs, s)
	}
	if len(cliArgs) > 0 {
		parsed, err := i.ParseCLIArgs(cmd, cliArgs)
		if err != nil {
			return nil, fmt.Errorf("run %s: %w", target, err)
		}
		// Overlay parsed args onto current bindings — caller's `let`
		// captures stay visible, the target's declared args get
		// their CLI-parsed values on top.
		for k, v := range parsed {
			b.Set(k, v)
		}
	}
	return nil, i.RunOps(cmd.Ops, b)
}

func opListCommands(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	for name, c := range i.Program.Commands {
		if c.Modifiers.Private {
			continue
		}
		desc := c.Description
		if desc == "" {
			fmt.Fprintf(i.Stdout, "  %s\n", name)
		} else {
			fmt.Fprintf(i.Stdout, "  %-20s %s\n", name, desc)
		}
	}
	return nil, nil
}

// buildShell creates an exec.Cmd that runs s via the host shell.
func buildShell(s string) *exec.Cmd {
	if runtime.GOOS == "windows" {
		return exec.Command("cmd", "/C", s)
	}
	return exec.Command("bash", "-c", s)
}

// applyEnv copies the bindings' env into the cmd's environment.
//
// SECURITY: if the bindings carry an EnvAllowlist (set by `perch --env`),
// the subprocess inherits ONLY the named host env vars — not the full
// host environment. That closes the most obvious subprocess escape:
// without scrubbing, `shell "echo $AWS_SECRET_KEY"` would happily print
// a secret even when the user `--env`-excluded it from perch's own
// interpolation. With scrubbing, the subprocess literally cannot see
// what the allowlist didn't permit.
//
// nil allowlist preserves legacy "everything inherits" behavior so files
// that don't opt into restrictions work unchanged.
func applyEnv(cmd *exec.Cmd, b *interpreter.Bindings) {
	if b.EnvAllowlist == nil {
		cmd.Env = append(cmd.Env, os.Environ()...)
	} else {
		// Pass through only the explicitly-allowed host env vars.
		for name := range b.EnvAllowlist {
			if v, ok := os.LookupEnv(name); ok {
				cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", name, v))
			}
		}
	}
	// Globals are visible as env vars too — convention: any binding with
	// an uppercase first letter becomes an env var. These are values the
	// user explicitly declared in the .perch file (or via CLI flag), so
	// they're considered "in scope" regardless of the allowlist.
	for k, v := range b.Vars {
		if k == "" {
			continue
		}
		c := k[0]
		if c >= 'A' && c <= 'Z' {
			cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", k, interpreter.ToStringValue(v)))
		}
	}
	for k, v := range b.Env {
		cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", k, v))
	}
}

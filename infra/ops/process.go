package ops

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
	"time"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

func registerProcess(m map[string]interpreter.Handler) {
	m["print"] = opPrint
	m["println"] = opPrintln
	m["eprintln"] = opEprintln
	m["shell"] = opShell
	m["shell_output"] = opShellOutput
	m["shell_detached"] = opShellDetached
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
	// Re-use the current bindings so ${var} stays visible across calls.
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

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

func opShell(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	cmd := buildShell(argString(args, "cmd", "_0"))
	applyEnv(cmd, b)
	cmd.Dir = b.Cwd
	cmd.Stdin = i.Stdin
	cmd.Stdout = i.Stdout
	cmd.Stderr = i.Stderr
	return nil, cmd.Run()
}

func opShellOutput(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	cmd := buildShell(argString(args, "cmd", "_0"))
	applyEnv(cmd, b)
	cmd.Dir = b.Cwd
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = i.Stderr
	if err := cmd.Run(); err != nil {
		return strings.TrimRight(out.String(), "\n"), err
	}
	return strings.TrimRight(out.String(), "\n"), nil
}

func opShellDetached(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	cmd := buildShell(argString(args, "cmd", "_0"))
	applyEnv(cmd, b)
	cmd.Dir = b.Cwd
	return nil, cmd.Start()
}

func opFail(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	return nil, errors.New(argString(args, "msg", "_0"))
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
func applyEnv(cmd *exec.Cmd, b *interpreter.Bindings) {
	cmd.Env = append(cmd.Env, os.Environ()...)
	// Globals are visible as env vars too — convention: any binding with
	// an uppercase first letter becomes an env var.
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

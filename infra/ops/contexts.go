package ops

// Execution-context block ops. Each wraps a body and modifies *how* the
// body runs — concurrency (parallel), deadline (timeout), retry policy
// (retry), env overlay (with_env), cwd override (with_cwd). They are
// declarative wrappers, not new control flow.
//
// All five follow the same block-op shape used by `if`, `for_each`, etc.:
// capy emits `_enter` / `_leave`, the loader folds into Op.Body, and the
// handler reads `args["_body"]` and recurses via i.RunOps.
//
// Identity guardrails:
//   - parallel forbids `let` captures and `cd` inside (the validator
//     enforces this statically — see usecases/validate). Each goroutine
//     gets its own Bindings copy to avoid races on Vars.
//   - timeout's deadline is wall-clock; a long-running op can't be
//     interrupted mid-call, but the next op after it returns ErrTimeout.
//   - retry never retries past the outer command's deadline.
//   - with_env / with_cwd auto-restore on block exit so they compose.

import (
	"fmt"
	"math"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

func registerContexts(m map[string]interpreter.Handler) {
	m["timeout"] = opTimeout
	m["retry"] = opRetry
	m["parallel"] = opParallel
	m["with_env"] = opWithEnv
	m["with_cwd"] = opWithCwd
	m["sandbox"] = opSandbox
}

// opTimeout caps wall-clock for the inner body. Form:
//
//	timeout "30s"
//	    shell "kubectl apply -f manifest.yaml"
//	end
//
// Implementation: temporarily narrows the interpreter's Deadline to
// min(existing, now+duration), runs the body, restores. Long-running ops
// can't be interrupted mid-call (Go's exec.Cmd needs explicit context
// wiring); the *next* op after the deadline trips returns ErrTimeout.
func opTimeout(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	durStr := argString(args, "duration", "_0")
	dur, err := time.ParseDuration(durStr)
	if err != nil {
		return nil, fmt.Errorf("timeout: invalid duration %q: %w", durStr, err)
	}
	newDeadline := time.Now().Add(dur)
	prev := i.Deadline
	if prev.IsZero() || newDeadline.Before(prev) {
		i.Deadline = newDeadline
	}
	defer func() { i.Deadline = prev }()
	return nil, i.RunOps(body, b)
}

// opRetry runs the body up to `attempts` times. On non-nil error it
// sleeps according to the chosen backoff and retries. Form:
//
//	retry attempts:3 backoff:"exponential" max_delay:"30s"
//	    shell "curl -fsSL https://flaky.example.com/"
//	end
//
// Backoff strategies: "fixed" (constant), "linear" (n*base),
// "exponential" (2^(n-1)*base). Default base = 1s. max_delay caps each
// sleep. Returns the LAST error if every attempt failed.
func opRetry(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	attempts := int(toFloat(args["attempts"]))
	if attempts <= 0 {
		attempts = 3
	}
	backoff := argString(args, "backoff", "")
	if backoff == "" {
		backoff = "exponential"
	}
	base := time.Second
	if s := argString(args, "base", ""); s != "" {
		if d, err := time.ParseDuration(s); err == nil {
			base = d
		}
	}
	maxDelay := 5 * time.Minute
	if s := argString(args, "max_delay", ""); s != "" {
		if d, err := time.ParseDuration(s); err == nil {
			maxDelay = d
		}
	}

	var lastErr error
	for n := 1; n <= attempts; n++ {
		err := i.RunOps(body, b)
		if err == nil {
			return nil, nil
		}
		if err == interpreter.ErrTimeout || err == interpreter.ErrQuit {
			return nil, err
		}
		lastErr = err
		if n == attempts {
			break
		}
		// Compute the next sleep duration.
		var d time.Duration
		switch backoff {
		case "fixed":
			d = base
		case "linear":
			d = time.Duration(n) * base
		default: // exponential
			d = time.Duration(math.Pow(2, float64(n-1))) * base
		}
		if d > maxDelay {
			d = maxDelay
		}
		// Don't retry if doing so would breach the outer deadline.
		if !i.Deadline.IsZero() && time.Now().Add(d).After(i.Deadline) {
			break
		}
		time.Sleep(d)
	}
	return nil, fmt.Errorf("retry: %d attempts failed; last error: %w", attempts, lastErr)
}

// opParallel runs each direct child op of the body concurrently. Waits
// for ALL to complete; returns the first error encountered (subsequent
// errors are appended to the message). Form:
//
//	parallel
//	    run build_darwin
//	    run build_linux
//	    run build_windows
//	end
//
// Each child runs against a *copy* of Bindings (Vars/Env/Cwd). This
// avoids races on Vars without requiring locks on every op. The validator
// statically rejects `let X = …` and `cd …` inside parallel — captures
// inside a goroutine wouldn't survive the block anyway. Nesting is
// permitted: each inner block sees its own Bindings copy.
func opParallel(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	if len(body) == 0 {
		return nil, nil
	}
	if len(body) == 1 {
		// Degenerate case — one child, just run it inline.
		return nil, i.RunOp(body[0], b)
	}

	var (
		wg      sync.WaitGroup
		mu      sync.Mutex
		errs    []error
		quitErr error
	)
	for _, op := range body {
		op := op
		wg.Add(1)
		go func() {
			defer wg.Done()
			child := copyBindings(b)
			err := i.RunOp(op, child)
			if err != nil {
				mu.Lock()
				if err == interpreter.ErrQuit {
					quitErr = err
				}
				errs = append(errs, err)
				mu.Unlock()
			}
		}()
	}
	wg.Wait()
	if quitErr != nil {
		return nil, quitErr
	}
	if len(errs) == 0 {
		return nil, nil
	}
	if len(errs) == 1 {
		return nil, errs[0]
	}
	msgs := make([]string, 0, len(errs))
	for _, e := range errs {
		msgs = append(msgs, e.Error())
	}
	return nil, fmt.Errorf("parallel: %d branches failed: %s", len(errs), strings.Join(msgs, " | "))
}

// copyBindings produces a deep-enough copy for parallel branches. Vars
// and Env are copied (so branches can't see each other's writes); CapMask
// is shared by pointer (it's immutable from the inside — pushes return
// new masks). EnvAllowlist is shared (read-only at runtime).
func copyBindings(b *interpreter.Bindings) *interpreter.Bindings {
	cp := &interpreter.Bindings{
		Cwd:          b.Cwd,
		Env:          make(map[string]string, len(b.Env)),
		Vars:         make(map[string]any, len(b.Vars)),
		EnvAllowlist: b.EnvAllowlist,
		CapMask:      b.CapMask,
	}
	for k, v := range b.Env {
		cp.Env[k] = v
	}
	for k, v := range b.Vars {
		cp.Vars[k] = v
	}
	return cp
}

// opWithEnv overlays per-block environment variables onto the bindings
// for the duration of the body, then restores. Form:
//
//	with_env GOOS="linux" CGO_ENABLED="0"
//	    shell "go build ./cmd"
//	end
//
// Variables are passed as args["env"] which is a comma-joined "KEY=val"
// string. The block-form is more readable than the per-command `env`
// modifier when the override is scoped to a few ops, not the whole
// command.
func opWithEnv(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	raw := argString(args, "env", "_0")
	if raw == "" {
		return nil, i.RunOps(body, b)
	}
	// Save prior values for restore.
	prior := make(map[string]struct {
		val    string
		hadVal bool
	})
	for _, pair := range strings.Split(raw, ",") {
		pair = strings.TrimSpace(pair)
		if pair == "" {
			continue
		}
		eq := strings.IndexByte(pair, '=')
		if eq <= 0 {
			return nil, fmt.Errorf("with_env: expected KEY=value, got %q", pair)
		}
		k := strings.TrimSpace(pair[:eq])
		v := strings.TrimSpace(pair[eq+1:])
		v = strings.Trim(v, `"`)
		if existing, ok := b.Env[k]; ok {
			prior[k] = struct {
				val    string
				hadVal bool
			}{existing, true}
		} else {
			prior[k] = struct {
				val    string
				hadVal bool
			}{"", false}
		}
		b.Env[k] = v
	}
	defer func() {
		for k, p := range prior {
			if p.hadVal {
				b.Env[k] = p.val
			} else {
				delete(b.Env, k)
			}
		}
	}()
	return nil, i.RunOps(body, b)
}

// opWithCwd temporarily switches cwd for the body, then restores. Unlike
// the standalone `cd` op (which persists for the rest of the command),
// `with_cwd` auto-restores even when the body errors. Form:
//
//	with_cwd "./subproject"
//	    shell "npm install"
//	    shell "npm run build"
//	end
func opWithCwd(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	raw := argString(args, "path", "_0")
	if raw == "" {
		return nil, fmt.Errorf("with_cwd: missing path")
	}
	path := resolve(raw, b)
	if info, err := os.Stat(path); err != nil {
		return nil, fmt.Errorf("with_cwd %q: %w", path, err)
	} else if !info.IsDir() {
		return nil, fmt.Errorf("with_cwd %q: not a directory", path)
	}
	prev := b.Cwd
	b.Cwd = path
	defer func() { b.Cwd = prev }()
	return nil, i.RunOps(body, b)
}

// opSandbox narrows the active capability mask for the body. Can only
// remove capabilities, never add them — the intersection with any outer
// mask (and the process-level CLI flags) is what's enforced. Form:
//
//	sandbox no_shell no_network env "HOME,PATH"
//	    run vendor.update_check
//	end
//
// Available capability declarations (args, all optional):
//   - flags        — comma-joined: "no_shell,no_subprocess,no_network,no_write"
//   - allow_bin    — comma-joined argv[0] allowlist (intersected with outer)
//   - allow_host   — comma-joined host allowlist (additive — narrower than CLI)
//   - env          — comma-joined env-var name allowlist
//   - read_only    — comma-joined dirs writes must stay under
//   - max_runtime  — duration string, applied as a timeout to the body
//
// The mask is pushed on entry and popped on exit. Op handlers consult
// b.CapMask.AnyNo* / AllowedBinPermitted etc. Static enforcement at
// --check time is a follow-up; this turn delivers runtime enforcement.
func opSandbox(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	next := interpreter.CapMask{}
	if s := argString(args, "flags", ""); s != "" {
		for _, f := range strings.Split(s, ",") {
			switch strings.TrimSpace(f) {
			case "no_shell":
				next.NoShell = true
			case "no_subprocess":
				next.NoSubprocess = true
			case "no_network":
				next.NoNetwork = true
			case "no_write":
				next.NoWrite = true
			case "":
			default:
				return nil, fmt.Errorf("sandbox: unknown flag %q", strings.TrimSpace(f))
			}
		}
	}
	if s := argString(args, "allow_bin", ""); s != "" {
		next.AllowedBins = map[string]bool{}
		for _, n := range strings.Split(s, ",") {
			n = strings.TrimSpace(n)
			if n != "" {
				next.AllowedBins[n] = true
			}
		}
	}
	if s := argString(args, "allow_host", ""); s != "" {
		for _, h := range strings.Split(s, ",") {
			h = strings.TrimSpace(h)
			if h != "" {
				next.AllowedHosts = append(next.AllowedHosts, h)
			}
		}
	}
	if s := argString(args, "env", ""); s != "" {
		next.EnvAllow = map[string]bool{}
		for _, n := range strings.Split(s, ",") {
			n = strings.TrimSpace(n)
			if n != "" {
				next.EnvAllow[n] = true
			}
		}
	}
	if s := argString(args, "read_only", ""); s != "" {
		for _, p := range strings.Split(s, ",") {
			p = strings.TrimSpace(p)
			if p != "" {
				next.ReadOnlyRoots = append(next.ReadOnlyRoots, p)
			}
		}
	}
	prev := b.CapMask
	b.CapMask = prev.Push(next)
	defer func() { b.CapMask = prev }()

	// Optional sandbox-scoped timeout.
	if s := argString(args, "max_runtime", ""); s != "" {
		if d, err := time.ParseDuration(s); err == nil {
			newDeadline := time.Now().Add(d)
			prevDL := i.Deadline
			if prevDL.IsZero() || newDeadline.Before(prevDL) {
				i.Deadline = newDeadline
			}
			defer func() { i.Deadline = prevDL }()
		}
	}
	return nil, i.RunOps(body, b)
}

// (helpers argString / toFloat live in process.go / flow.go)

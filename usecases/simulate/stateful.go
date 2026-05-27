package simulate

// The stateful simulator. Threads SimState through the op walk so
// later ops see the effects of earlier ones (file written → exists
// downstream; let X = shell_output Y → ${X} resolves to oracle value;
// cd PATH → cwd-relative paths resolve correctly).
//
// Kept in its own file because it overlays the existing static
// simulateOp dispatch in simulate.go. The static path remains the
// fallback when no Fixture / no oracles are provided — same behavior
// as before this refactor.

import (
	"fmt"
	"strings"

	"github.com/luowensheng/perch/domain"
)

// SimulateWithState walks the named command's ops with full state
// threading and oracle consultation. The state mutates as ops "run":
//
//   write_file X        → state.Files[X] = true
//   rm X                → state.Files[X] = false
//   cd PATH             → state.Cwd = PATH
//   let X = shell_output Y → state.Vars[X] = oracles.ShellOutput[Y]
//
// Returns the per-op results AND the final state (useful for chained
// scenarios — not yet wired through the CLI, but available to callers).
func SimulateWithState(p *domain.Program, cmdName string, env SimEnv, oracles OracleSet) (SimResult, *SimState) {
	res := SimResult{Command: cmdName}
	cmd, ok := p.Commands[cmdName]
	if !ok {
		res.Ops = []OpResult{{Outcome: WillFailOut, Reasons: []string{fmt.Sprintf("command %q not found", cmdName)}}}
		res.WillFail = 1
		return res, nil
	}
	state := NewSimState(env, oracles)
	for _, op := range cmd.Ops {
		r := simulateOpStateful(op, state, env, 0, p, cmdName)
		tallyTree(&res, r)
		res.Ops = append(res.Ops, r)
	}
	return res, state
}

// simulateOpStateful is the state-threading counterpart to
// simulateOp (in simulate.go). It dispatches the same kinds but:
//   - resolves ${name} against the live state
//   - consults oracles before declaring an op MIGHT_FAIL
//   - mutates state after side-effectful ops
func simulateOpStateful(op domain.Op, state *SimState, env SimEnv, depth int, p *domain.Program, cmdName string) OpResult {
	r := OpResult{Op: op, Depth: depth, Outcome: WillRun}

	// Resolve interpolation against the live state.
	resolvedArgs := substituteAll(op.Args, state)

	switch op.Kind {

	case "shell", "shell_output", "shell_detached", "shell_in", "try_shell":
		classifyShellStateful(&r, op, resolvedArgs, env, state)

	case "pkg_install", "pkg_uninstall", "kill_by_name", "process_running", "bin_version", "os_version":
		if env.NoSubprocess {
			r.Outcome = WillFailOut
			r.Reasons = append(r.Reasons, "subprocess capability denied by sim --no-subprocess")
		}

	case "http_get", "http_post", "http_put", "http_delete", "http_status", "download":
		classifyHTTPStateful(&r, op, resolvedArgs, env, state)

	case "write_file", "append_file", "ensure_line_in_file", "replace_in_file", "touch":
		classifyWriteStateful(&r, resolvedArgs, env, state, true)

	case "cp", "mv", "mkdir", "copy_dir", "ensure_dir", "symlink":
		classifyWriteStateful(&r, resolvedArgs, env, state, true)

	case "rm":
		classifyWriteStateful(&r, resolvedArgs, env, state, false)

	case "cd":
		path := stringArg(resolvedArgs, "path", "_0")
		if path != "" && !strings.Contains(path, "${") {
			state.Cwd = path
			r.Reasons = append(r.Reasons, fmt.Sprintf("cwd → %q", path))
		} else {
			r.Outcome = MightFail
			r.Reasons = append(r.Reasons, "cwd target not statically determinable")
		}

	case "read_file", "exists", "is_dir", "is_file", "file_size", "file_mtime", "sha256_file", "md5_file":
		classifyReadStateful(&r, op, resolvedArgs, env, state)

	case "has_bin":
		classifyHasBinStateful(&r, resolvedArgs, env, state)

	case "if":
		classifyIfStateful(&r, op, env, state, depth, p, cmdName)
		return r

	case "if_call":
		classifyIfCallStateful(&r, op, env, state, depth, p, cmdName)
		return r

	case "os":
		r.IsBlockEntry = true
		target, _ := op.Args["target"].(string)
		if env.OS == "" || osMatches(target, env.OS) {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("os %q matches — body runs", target))
			r.Children = simulateBodyStateful(op.Body, env, state, depth+1, p, cmdName)
		} else {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("os %q != sim-os %q — body skipped", target, env.OS))
		}
		rollupChildren(&r)
		return r

	case "arch":
		r.IsBlockEntry = true
		target, _ := op.Args["target"].(string)
		if env.Arch == "" || target == env.Arch {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("arch %q matches — body runs", target))
			r.Children = simulateBodyStateful(op.Body, env, state, depth+1, p, cmdName)
		} else {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("arch %q != sim-arch %q — body skipped", target, env.Arch))
		}
		rollupChildren(&r)
		return r

	case "parallel", "retry", "timeout", "with_env", "with_cwd", "sandbox", "cache", "for_each":
		r.IsBlockEntry = true
		// Block ops snapshot the state so concurrent branches don't
		// mutate each other (and so the not-taken-if branch leaves no
		// trace).
		bodyEnv := applyBlockEnv(op, env)
		bodyState := state.Snapshot()
		// for_each / with_cwd should also propagate their effect into
		// the parent state — for now keep it isolated to the body for
		// safety; future enhancement can merge.
		r.Children = simulateBodyStateful(op.Body, bodyEnv, bodyState, depth+1, p, cmdName)
		rollupChildren(&r)
		return r

	case "run":
		classifyRunStateful(&r, op, env, state, depth, p)
		return r

	case "wasm_run":
		classifyWasmRun(&r, op, env, depth) // static; no state effects
		return r

	case "_template_call":
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "unresolved template call (check imports + spelling)")

	case "fail":
		r.Outcome = WillFailOut
		msg := stringArg(resolvedArgs, "msg", "_0")
		if msg == "" {
			msg = "(no message)"
		}
		r.Reasons = append(r.Reasons, fmt.Sprintf("explicit fail: %s", msg))
	}

	// `let X = ...` capture — bind oracle output (if any) into state
	// so downstream interpolation works.
	if op.CaptureInto != "" {
		captureValue(&r, op, resolvedArgs, state)
	}

	return r
}

func simulateBodyStateful(ops []domain.Op, env SimEnv, state *SimState, depth int, p *domain.Program, cmdName string) []OpResult {
	out := make([]OpResult, 0, len(ops))
	for _, op := range ops {
		out = append(out, simulateOpStateful(op, state, env, depth, p, cmdName))
	}
	return out
}

func substituteAll(args map[string]any, state *SimState) map[string]any {
	out := make(map[string]any, len(args))
	for k, v := range args {
		if s, ok := v.(string); ok {
			resolved, _ := state.Substitute(s)
			out[k] = resolved
		} else {
			out[k] = v
		}
	}
	return out
}

// ── Stateful classifiers ──────────────────────────────────────────

func classifyShellStateful(r *OpResult, op domain.Op, args map[string]any, env SimEnv, state *SimState) {
	if env.NoShell {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "shell capability denied by sim --no-shell")
		return
	}
	cmd := stringArg(args, "cmd", "_0")
	first := firstShellToken(cmd)
	if strings.Contains(first, "${") {
		r.Outcome = MightFail
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: fmt.Sprintf("argv[0] still unresolved after interpolation: %q", first),
			Outcome:     MightFail,
			Reason:      "depends on a value not in state.Vars (likely a `let` from an oracled call that was missing)",
		})
		return
	}
	if first == "" {
		r.Outcome = MightFail
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: "argv[0] is empty after interpolation",
			Outcome:     MightFail,
		})
		return
	}
	if env.Bins != nil && !env.Bins[first] {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("shell binary %q not in --sim-have-bin allowlist (have: %s)", first, sortedBins(env.Bins)))
		return
	}

	// Consult shell_output oracle if the op's result will be captured.
	if op.Kind == "shell_output" && op.CaptureInto != "" {
		if v, ok := state.Oracles.ShellOutput[cmd]; ok {
			r.Reasons = append(r.Reasons, fmt.Sprintf("oracle: shell_output(%q) = %q → ${%s}", cmd, v, op.CaptureInto))
		} else {
			r.Reasons = append(r.Reasons, fmt.Sprintf("shell_output: no oracle for %q — ${%s} downstream will be MIGHT_FAIL", cmd, op.CaptureInto))
		}
	}
}

func classifyHTTPStateful(r *OpResult, op domain.Op, args map[string]any, env SimEnv, state *SimState) {
	if env.NoNetwork {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "network capability denied by sim --no-network")
		return
	}
	url := stringArg(args, "url", "_0")
	if url == "" || strings.Contains(url, "${") {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "URL not fully resolvable from state")
		return
	}
	host := extractHost(url)
	if env.Network != nil && !hostAllowed(host, env.Network) {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("HTTP host %q not in --sim-allow-host allowlist", host))
		return
	}
	// Consult HTTP oracle.
	if resp, ok := state.Oracles.HTTP[url]; ok {
		status := resp.Status
		if status == 0 {
			status = 200
		}
		switch {
		case status >= 200 && status < 300:
			r.Reasons = append(r.Reasons, fmt.Sprintf("oracle: %s → %d %s", url, status, truncate(resp.Body, 60)))
		case status >= 300 && status < 400:
			r.Outcome = MightFail
			r.Reasons = append(r.Reasons, fmt.Sprintf("oracle: %s → %d redirect to %q", url, status, resp.Redirect))
			if env.Network != nil && resp.Redirect != "" {
				redirectHost := extractHost(resp.Redirect)
				if !hostAllowed(redirectHost, env.Network) {
					r.Outcome = WillFailOut
					r.Reasons = append(r.Reasons, fmt.Sprintf("redirect destination %q is NOT in --sim-allow-host allowlist", redirectHost))
				}
			}
		case status >= 400:
			r.Outcome = WillFailOut
			r.Reasons = append(r.Reasons, fmt.Sprintf("oracle: %s → %d %s", url, status, truncate(resp.Body, 60)))
		}
	} else if env.Network != nil {
		// No oracle, allowlist active — same uncertainty note as the
		// static path.
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: fmt.Sprintf("no oracle for %q; server could return any status / redirect anywhere", url),
			Outcome:     MightFail,
			Reason:      "supply an oracles.http entry to pin the simulated response",
		})
		if r.Outcome == WillRun {
			r.Outcome = MightFail
		}
	}
}

func classifyWriteStateful(r *OpResult, args map[string]any, env SimEnv, state *SimState, exists bool) {
	if env.NoWrite {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "write capability denied by sim --no-write")
		return
	}
	path := firstPathArg(args)
	if path == "" || strings.Contains(path, "${") {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "write target not fully resolvable from state")
		return
	}
	if env.FsWrite != nil && !pathUnder(path, env.FsWrite) {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("write path %q outside --sim-fs-write roots (%s)", path, strings.Join(env.FsWrite, ", ")))
		return
	}
	// State effect: mark the file as existing (or not, for rm).
	state.MarkFile(path, exists)
	if exists {
		r.Reasons = append(r.Reasons, fmt.Sprintf("state: %q now exists", path))
	} else {
		r.Reasons = append(r.Reasons, fmt.Sprintf("state: %q removed", path))
	}
}

func classifyReadStateful(r *OpResult, op domain.Op, args map[string]any, env SimEnv, state *SimState) {
	path := firstPathArg(args)
	if path == "" || strings.Contains(path, "${") {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "read path not fully resolvable from state")
		return
	}
	if env.FsRead != nil && !pathUnder(path, env.FsRead) {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("read path %q outside --sim-fs-read roots (%s)", path, strings.Join(env.FsRead, ", ")))
		return
	}
	// State + oracle: does the file actually exist for the simulator?
	if op.Kind == "exists" {
		// `let X = exists "PATH"` — return true/false from state.
		if ex, known := state.FileExists(path); known {
			r.Reasons = append(r.Reasons, fmt.Sprintf("state: exists(%q) = %v", path, ex))
		} else {
			r.Scenarios = append(r.Scenarios, Scenario{
				Description: fmt.Sprintf("exists(%q): no oracle, no prior write — outcome uncertain", path),
				Outcome:     MightFail,
			})
			if r.Outcome == WillRun {
				r.Outcome = MightFail
			}
		}
	}
}

func classifyHasBinStateful(r *OpResult, args map[string]any, env SimEnv, state *SimState) {
	bin := stringArg(args, "_0", "name")
	if bin == "" {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "bin name not resolvable")
		return
	}
	// Oracle takes precedence over env.Bins capability list.
	if v, ok := state.Oracles.HasBin[bin]; ok {
		r.Reasons = append(r.Reasons, fmt.Sprintf("oracle: has_bin(%q) = %v", bin, v))
		return
	}
	if env.Bins != nil {
		r.Reasons = append(r.Reasons, fmt.Sprintf("has_bin(%q) = %v (from --sim-have-bin)", bin, env.Bins[bin]))
	}
}

func classifyIfStateful(r *OpResult, op domain.Op, env SimEnv, state *SimState, depth int, p *domain.Program, cmdName string) {
	r.IsBlockEntry = true
	cond := stringArg(op.Args, "op")
	lhs := stringArg(op.Args, "lhs")
	rhs := stringArg(op.Args, "rhs")

	value, ok := state.Vars[lhs]
	if !ok {
		// Not bound; check env interpolation.
		value, ok = state.Env[lhs]
	}
	if ok && !state.Unknown[lhs] {
		take := false
		switch cond {
		case "eq":
			take = value == rhs
		case "neq":
			take = value != rhs
		case "truthy":
			take = value != "" && value != "false" && value != "0"
		case "falsy":
			take = value == "" || value == "false" || value == "0"
		}
		if take {
			r.Reasons = append(r.Reasons, fmt.Sprintf("condition %s %s %q TRUE (state %s=%q) — body runs", lhs, cond, rhs, lhs, value))
			r.Children = simulateBodyStateful(op.Body, env, state, depth+1, p, cmdName)
		} else {
			r.Reasons = append(r.Reasons, fmt.Sprintf("condition %s %s %q FALSE (state %s=%q) — body skipped", lhs, cond, rhs, lhs, value))
		}
		rollupChildren(r)
		return
	}
	// Can't resolve — body presented as MIGHT-RUN.
	branchState := state.Snapshot()
	r.Children = simulateBodyStateful(op.Body, env, branchState, depth+1, p, cmdName)
	r.Reasons = append(r.Reasons, fmt.Sprintf("condition (%s %s) depends on a runtime value — body presented as MIGHT-RUN", lhs, cond))
	rollupChildren(r)
	if r.Outcome == WillRun {
		r.Outcome = MightFail
	}
}

func classifyIfCallStateful(r *OpResult, op domain.Op, env SimEnv, state *SimState, depth int, p *domain.Program, cmdName string) {
	r.IsBlockEntry = true
	fn := stringArg(op.Args, "func")
	arg := stringArg(op.Args, "_0")
	if resolved, allOK := state.Substitute(arg); allOK {
		arg = resolved
	}
	resolvedNow := false
	take := false
	reason := ""
	switch fn {
	case "exists":
		if ex, known := state.FileExists(arg); known {
			take = ex
			resolvedNow = true
			reason = fmt.Sprintf("state: exists(%q) = %v", arg, ex)
		} else if env.FsRead != nil {
			take = pathUnder(arg, env.FsRead)
			resolvedNow = true
			reason = fmt.Sprintf("exists(%q) → %v under --sim-fs-read (no specific oracle)", arg, take)
		}
	case "has_bin":
		if v, ok := state.Oracles.HasBin[arg]; ok {
			take = v
			resolvedNow = true
			reason = fmt.Sprintf("oracle: has_bin(%q) = %v", arg, v)
		} else if env.Bins != nil {
			take = env.Bins[arg]
			resolvedNow = true
			reason = fmt.Sprintf("has_bin(%q) = %v (--sim-have-bin)", arg, v)
		}
	}
	if resolvedNow {
		r.Reasons = append(r.Reasons, reason)
		if take {
			r.Children = simulateBodyStateful(op.Body, env, state, depth+1, p, cmdName)
		}
		rollupChildren(r)
		return
	}
	branchState := state.Snapshot()
	r.Children = simulateBodyStateful(op.Body, env, branchState, depth+1, p, cmdName)
	r.Reasons = append(r.Reasons, fmt.Sprintf("predicate %s(%q) not resolvable — body MIGHT run", fn, arg))
	rollupChildren(r)
	if r.Outcome == WillRun {
		r.Outcome = MightFail
	}
}

func classifyRunStateful(r *OpResult, op domain.Op, env SimEnv, state *SimState, depth int, p *domain.Program) {
	target := stringArg(op.Args, "target")
	if _, ok := p.Commands[target]; !ok {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("run: target command %q not found", target))
		return
	}
	r.IsBlockEntry = true
	r.Reasons = append(r.Reasons, fmt.Sprintf("dispatches to command %q", target))
	r.Children = simulateBodyStateful(p.Commands[target].Ops, env, state, depth+1, p, target)
	rollupChildren(r)
}

// captureValue binds the op's simulated output into state.Vars,
// flagging Unknown if no oracle was found for an op whose result is
// otherwise unknowable statically.
func captureValue(r *OpResult, op domain.Op, args map[string]any, state *SimState) {
	switch op.Kind {
	case "shell_output", "try_shell":
		cmd := stringArg(args, "cmd", "_0")
		if v, ok := state.Oracles.ShellOutput[cmd]; ok {
			state.SetVar(op.CaptureInto, v, false)
		} else {
			state.SetVar(op.CaptureInto, fmt.Sprintf("${%s}", op.CaptureInto), true)
		}
	case "http_get", "http_post":
		url := stringArg(args, "url", "_0")
		if resp, ok := state.Oracles.HTTP[url]; ok {
			state.SetVar(op.CaptureInto, resp.Body, false)
		} else {
			state.SetVar(op.CaptureInto, fmt.Sprintf("${%s}", op.CaptureInto), true)
		}
	case "exists":
		path := firstPathArg(args)
		if ex, known := state.FileExists(path); known {
			state.SetVar(op.CaptureInto, boolStr(ex), false)
		} else {
			state.SetVar(op.CaptureInto, fmt.Sprintf("${%s}", op.CaptureInto), true)
		}
	case "has_bin":
		bin := stringArg(args, "_0", "name")
		if v, ok := state.Oracles.HasBin[bin]; ok {
			state.SetVar(op.CaptureInto, boolStr(v), false)
		}
	default:
		// Unknown capture — value is symbolic.
		state.SetVar(op.CaptureInto, fmt.Sprintf("${%s}", op.CaptureInto), true)
	}
	if state.Unknown[op.CaptureInto] {
		r.Reasons = append(r.Reasons, fmt.Sprintf("captured ${%s} from %s — no oracle, value is symbolic downstream", op.CaptureInto, op.Kind))
	}
}

func truncate(s string, max int) string {
	if len(s) <= max {
		return s
	}
	return s[:max-3] + "..."
}

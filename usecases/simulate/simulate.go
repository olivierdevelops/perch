// Package simulate analyzes a perch program against a hypothetical
// runtime environment and produces a per-op outcome report.
//
// The missing third tool in perch's pre-flight suite:
//
//   --check     : syntactic validation (no env, no execution)
//   --scan      : static capability analysis (no env, no execution)
//   --dry-run   : real env, walks the plan, skips execution
//   simulate    : HYPOTHETICAL env, walks the plan, classifies each op
//                 as WILL_RUN / WILL_FAIL(why) / MIGHT_FAIL(scenarios)
//
// Use it to answer "what would this perch program do on a host with
// these env vars / these allowed binaries / this network allowlist?"
// without executing anything.
package simulate

import (
	"fmt"
	"io"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/olivierdevelops/perch/domain"
)

// UseCase is the consumer-owned protocol used by the CLI.
type UseCase interface {
	Execute(configPath, commandName string, env SimEnv, fixturePath string, w io.Writer) error
}

// LoadFn parses a .perch file into a Program (same shape capyloader.Load
// uses; injected so the use case can be tested without filesystem).
type LoadFn func(path string) (*domain.Program, error)

// Impl is the production implementation.
type Impl struct {
	Load LoadFn
}

// Execute simulates `commandName` against `env`, writing a human report
// to w. Returns a non-nil error if the simulation reports any
// WILL_FAIL outcome — so this is CI-droppable like --check / perch test.
func (i *Impl) Execute(configPath, commandName string, env SimEnv, fixturePath string, w io.Writer) error {
	p, err := i.Load(configPath)
	if err != nil {
		return fmt.Errorf("loading %s: %w", configPath, err)
	}

	// If a fixture file is supplied, iterate its scenarios — each scenario
	// is one independent state-threading walk against effective oracles.
	if fixturePath != "" {
		fix, err := LoadFixture(fixturePath)
		if err != nil {
			return fmt.Errorf("loading fixture %s: %w", fixturePath, err)
		}
		merged := mergeFixtureIntoEnv(fix, env)
		scenarios := fix.Scenarios
		if len(scenarios) == 0 {
			scenarios = []Scenario_{{Name: "default"}}
		}
		var failures int
		for idx, sc := range scenarios {
			if idx > 0 {
				fmt.Fprintln(w)
			}
			oracles := MergeOracles(fix.Oracles, sc.Overrides)
			scEnv := merged
			if len(sc.Env) > 0 {
				scEnv = withEnvOverlay(scEnv, sc.Env)
			}
			fmt.Fprintf(w, "═══ Scenario: %s ═══\n", sc.Name)
			names := []string{commandName}
			if commandName == "" {
				names = commandNames(p)
			}
			for j, name := range names {
				if j > 0 {
					fmt.Fprintln(w)
				}
				res, _ := SimulateWithState(p, name, scEnv, oracles)
				renderResult(w, res, p, name)
				failures += res.WillFail
			}
		}
		if failures > 0 {
			return fmt.Errorf("%d op(s) would fail across simulated scenarios", failures)
		}
		return nil
	}

	if commandName == "" {
		// Simulate every command.
		var failures int
		names := commandNames(p)
		for idx, name := range names {
			if idx > 0 {
				fmt.Fprintln(w)
			}
			res := simulateCommand(p, name, env, 0)
			renderResult(w, res, p, name)
			failures += res.WillFail
		}
		if failures > 0 {
			return fmt.Errorf("%d op(s) would fail under the simulated environment", failures)
		}
		return nil
	}
	res := simulateCommand(p, commandName, env, 0)
	renderResult(w, res, p, commandName)
	if res.WillFail > 0 {
		return fmt.Errorf("%d op(s) would fail under the simulated environment", res.WillFail)
	}
	return nil
}

// mergeFixtureIntoEnv layers fixture capabilities on top of CLI-flag env.
// CLI flags win when both are set (so a user can `--sim-no-network` a
// fixture that has Network entries). Fields the CLI didn't touch fall
// through from the fixture.
func mergeFixtureIntoEnv(f Fixture, cli SimEnv) SimEnv {
	fxEnv := f.ToSimEnv()
	out := cli
	if out.OS == "" {
		out.OS = fxEnv.OS
	}
	if out.Arch == "" {
		out.Arch = fxEnv.Arch
	}
	if out.Env == nil {
		out.Env = fxEnv.Env
	}
	if !out.EnvRestrict {
		out.EnvRestrict = fxEnv.EnvRestrict
	}
	if out.FsRead == nil {
		out.FsRead = fxEnv.FsRead
	}
	if out.FsWrite == nil {
		out.FsWrite = fxEnv.FsWrite
	}
	if out.Bins == nil {
		out.Bins = fxEnv.Bins
	}
	if out.Network == nil {
		out.Network = fxEnv.Network
	}
	out.NoShell = out.NoShell || fxEnv.NoShell
	out.NoSubprocess = out.NoSubprocess || fxEnv.NoSubprocess
	out.NoNetwork = out.NoNetwork || fxEnv.NoNetwork
	out.NoWrite = out.NoWrite || fxEnv.NoWrite
	return out
}

func withEnvOverlay(base SimEnv, overlay map[string]string) SimEnv {
	merged := map[string]string{}
	for k, v := range base.Env {
		merged[k] = v
	}
	for k, v := range overlay {
		merged[k] = v
	}
	base.Env = merged
	return base
}

// SimEnv describes the hypothetical host the program will run on.
// Fields default to "anything allowed" — an empty SimEnv simulates a
// fully-permissive host. Restrict by populating each field.
type SimEnv struct {
	// OS / Arch — what runtime.GOOS / runtime.GOARCH return.
	OS   string
	Arch string

	// Env is the simulated host environment. Names not in this map
	// are reported as "unset" — the perch ${NAME} lookup would fail.
	// Set to nil (vs empty) to mean "every env var is set to its real
	// host value" (no restriction).
	Env map[string]string
	// EnvRestrict, when true, means Env is exhaustive — anything not
	// in it is invisible. Mirrors `perch --env A,B,C`.
	EnvRestrict bool

	// FsRead lists absolute paths or path roots the simulated process
	// can read. nil = read-anywhere (no restriction).
	FsRead []string
	// FsWrite lists paths the simulated process can write under.
	// nil = write-anywhere.
	FsWrite []string

	// Bins is the set of binaries the simulated host has on PATH.
	// Used for `shell "X args"` argv[0] checks and for `has_bin "X"`
	// predicate evaluation. nil = every bin available.
	Bins map[string]bool

	// Network is the set of allowed host names (exact or wildcard).
	// nil = network is open. Empty non-nil = network blocked entirely.
	Network []string

	// Capability flags (mirror perch's --no-* CLI). When true, the
	// corresponding op class will-fail regardless of Bins / Network /
	// FsWrite contents.
	NoShell      bool
	NoSubprocess bool
	NoNetwork    bool
	NoWrite      bool
}

// IsZero reports whether the SimEnv applies any restriction. Used to
// add a banner explaining "no restrictions configured — every op
// passes by default" to the report.
func (e SimEnv) IsZero() bool {
	return e.OS == "" && e.Arch == "" && e.Env == nil && !e.EnvRestrict &&
		e.FsRead == nil && e.FsWrite == nil && e.Bins == nil &&
		e.Network == nil && !e.NoShell && !e.NoSubprocess &&
		!e.NoNetwork && !e.NoWrite
}

// Outcome is the simulator's verdict for a single op.
type Outcome int

const (
	WillRun     Outcome = iota // every check the simulator can perform passes
	WillFailOut                // at least one check definitively fails
	MightFail                  // depends on runtime data the simulator can't know
)

// OpResult is the simulator's analysis of one op.
type OpResult struct {
	Op           domain.Op
	Path         string // "command setup → if os == \"darwin\" → shell"
	Outcome      Outcome
	Reasons      []string   // why-fail / why-uncertain
	Scenarios    []Scenario // for MightFail: possible cases
	IsBlockEntry bool       // true for the block-op itself (children render under)
	Depth        int        // for tree indent
	Children     []OpResult // for block ops
}

// Scenario is one possible runtime path the op could take. Used for
// http_get with redirects, shell with computed argv, etc.
type Scenario struct {
	Description string
	Outcome     Outcome
	Reason      string
}

// SimResult is the aggregate outcome of simulating one command.
type SimResult struct {
	Command   string
	Ops       []OpResult
	WillRun   int
	WillFail  int
	Uncertain int
}

// simulateCommand entrypoint — walks the command's ops and produces a
// SimResult. Recursive for block ops.
// SimulateCommand is the exported entry-point for callers (the HTTP UI
// in particular). Equivalent to the unexported simulateCommand with
// depth=0.
func SimulateCommand(p *domain.Program, name string, env SimEnv) SimResult {
	return simulateCommand(p, name, env, 0)
}

func simulateCommand(p *domain.Program, name string, env SimEnv, depth int) SimResult {
	res := SimResult{Command: name}
	cmd, ok := p.Commands[name]
	if !ok {
		res.Ops = []OpResult{
			{Outcome: WillFailOut, Reasons: []string{fmt.Sprintf("command %q not found in program", name)}},
		}
		res.WillFail = 1
		return res
	}
	// Platform pre-checks.
	if env.OS != "" && len(cmd.Modifiers.RequireOS) > 0 {
		ok := false
		for _, o := range cmd.Modifiers.RequireOS {
			if o == env.OS {
				ok = true
				break
			}
		}
		if !ok {
			res.Ops = []OpResult{{
				Outcome: WillFailOut,
				Reasons: []string{fmt.Sprintf("require_os: command needs %v; sim env is OS=%q", cmd.Modifiers.RequireOS, env.OS)},
			}}
			res.WillFail = 1
			return res
		}
	}
	for _, op := range cmd.Ops {
		r := simulateOp(op, env, depth, p, name)
		tallyTree(&res, r)
		res.Ops = append(res.Ops, r)
	}
	return res
}

func tallyTree(res *SimResult, r OpResult) {
	switch r.Outcome {
	case WillRun:
		res.WillRun++
	case WillFailOut:
		res.WillFail++
	case MightFail:
		res.Uncertain++
	}
	for _, c := range r.Children {
		tallyTree(res, c)
	}
}

// simulateOp classifies one op against the SimEnv. Block ops recurse.
func simulateOp(op domain.Op, env SimEnv, depth int, p *domain.Program, cmdName string) OpResult {
	r := OpResult{Op: op, Depth: depth, Outcome: WillRun}

	// Resolve args (best-effort) against the SimEnv's env map. Args
	// that depend on values we can't know stay as ${name} — the
	// classifiers then mark the op MightFail.
	args := resolveArgs(op.Args, env)

	switch op.Kind {

	case "shell", "shell_output", "shell_detached", "shell_in", "try_shell":
		classifyShell(&r, args, env)

	case "pkg_install", "pkg_uninstall", "kill_by_name", "process_running", "bin_version", "os_version":
		if env.NoSubprocess {
			r.Outcome = WillFailOut
			r.Reasons = append(r.Reasons, "subprocess capability denied by sim --no-subprocess")
		}

	case "http_get", "http_post", "http_put", "http_delete", "http_status", "download":
		classifyHTTP(&r, args, env)

	case "write_file", "append_file", "ensure_line_in_file", "replace_in_file",
		"cp", "mv", "rm", "mkdir", "chmod", "touch", "copy_dir", "ensure_dir",
		"make_executable", "symlink", "tar_extract", "zip_extract", "gzip", "ungzip":
		classifyWrite(&r, args, env)

	case "read_file", "exists", "is_dir", "is_file", "list_dir", "walk_dir",
		"file_size", "file_mtime", "sha256_file", "md5_file":
		classifyRead(&r, args, env)

	case "has_bin":
		classifyHasBin(&r, args, env)

	case "if":
		classifyIf(&r, op, env, depth, p, cmdName)
		return r

	case "if_call":
		// Predicate calls (`if exists "X"`). For now, evaluate the
		// predicate if we recognize it; otherwise mark uncertain.
		classifyIfCall(&r, op, env, depth, p, cmdName)
		return r

	case "parallel", "retry", "timeout", "with_env", "with_cwd", "sandbox", "cache", "for_each":
		r.IsBlockEntry = true
		r.Children = simulateBody(op.Body, applyBlockEnv(op, env), depth+1, p, cmdName)
		rollupChildren(&r)
		return r

	case "os":
		// OS execution context block. Prune by --sim-os.
		r.IsBlockEntry = true
		target, _ := op.Args["target"].(string)
		if env.OS == "" || osMatches(target, env.OS) {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("os %q matches sim-os — body will run", target))
			r.Children = simulateBody(op.Body, env, depth+1, p, cmdName)
		} else {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("os %q does NOT match sim-os %q — body skipped",
					target, env.OS))
		}
		rollupChildren(&r)
		return r

	case "arch":
		// Architecture execution context block. Prune by --sim-arch.
		r.IsBlockEntry = true
		target, _ := op.Args["target"].(string)
		if env.Arch == "" || target == env.Arch {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("arch %q matches sim-arch — body will run", target))
			r.Children = simulateBody(op.Body, env, depth+1, p, cmdName)
		} else {
			r.Reasons = append(r.Reasons,
				fmt.Sprintf("arch %q does NOT match sim-arch %q — body skipped",
					target, env.Arch))
		}
		rollupChildren(&r)
		return r

	case "run":
		classifyRun(&r, op, env, depth, p)
		return r

	case "wasm_run":
		classifyWasmRun(&r, op, env, depth)
		return r

	case "_template_call":
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "unresolved template call (check imports + spelling)")

	case "fail":
		r.Outcome = WillFailOut
		msg := stringArg(args, "msg", "_0")
		if msg == "" {
			msg = "(no message)"
		}
		r.Reasons = append(r.Reasons, fmt.Sprintf("explicit fail: %s", msg))

	case "exit":
		// Explicit exit. Treat as terminator — runs but stops the flow.
		// We still mark WILL_RUN since the op itself succeeds.
	}

	// Env-var interpolation check applies to nearly every op.
	envFail, envScenarios := checkEnvInterpolation(op.Args, env)
	if envFail != "" {
		// If we already failed for a stronger reason, keep that.
		if r.Outcome == WillRun {
			r.Outcome = WillFailOut
		}
		r.Reasons = append(r.Reasons, envFail)
	}
	for _, s := range envScenarios {
		r.Scenarios = append(r.Scenarios, s)
		if r.Outcome == WillRun {
			r.Outcome = MightFail
		}
	}

	return r
}

func simulateBody(ops []domain.Op, env SimEnv, depth int, p *domain.Program, cmdName string) []OpResult {
	out := make([]OpResult, 0, len(ops))
	for _, op := range ops {
		out = append(out, simulateOp(op, env, depth, p, cmdName))
	}
	return out
}

// applyBlockEnv returns a SimEnv with the block-op's modifications
// applied. `sandbox no_shell` narrows the env; `with_env "K=v"` adds
// to the env map; etc.
func applyBlockEnv(op domain.Op, env SimEnv) SimEnv {
	out := env
	switch op.Kind {
	case "sandbox":
		flags := stringArg(op.Args, "flags", "_0")
		for _, f := range strings.Split(flags, ",") {
			switch strings.TrimSpace(f) {
			case "no_shell":
				out.NoShell = true
			case "no_subprocess":
				out.NoSubprocess = true
			case "no_network":
				out.NoNetwork = true
			case "no_write":
				out.NoWrite = true
			}
		}
	case "with_env":
		// Add the declared env vars to the SimEnv. We don't know their
		// runtime values; treat them as "set to something".
		envs := stringArg(op.Args, "env", "_0")
		if out.Env == nil {
			out.Env = map[string]string{}
		} else {
			// Don't mutate the caller's map.
			cp := make(map[string]string, len(out.Env))
			for k, v := range out.Env {
				cp[k] = v
			}
			out.Env = cp
		}
		for _, pair := range strings.Split(envs, ",") {
			pair = strings.TrimSpace(pair)
			eq := strings.IndexByte(pair, '=')
			if eq > 0 {
				out.Env[pair[:eq]] = strings.Trim(pair[eq+1:], `"`)
			}
		}
	}
	return out
}

func rollupChildren(r *OpResult) {
	r.Outcome = WillRun
	for _, c := range r.Children {
		if c.Outcome == WillFailOut {
			r.Outcome = WillFailOut
			return
		}
		if c.Outcome == MightFail && r.Outcome == WillRun {
			r.Outcome = MightFail
		}
	}
}

// ── Per-op classifiers ─────────────────────────────────────────────

func classifyShell(r *OpResult, args map[string]any, env SimEnv) {
	if env.NoShell {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "shell capability denied by sim --no-shell")
		return
	}
	cmd := stringArg(args, "cmd", "_0")
	first := firstShellToken(cmd)
	if first == "" {
		r.Outcome = MightFail
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: "argv[0] is computed at runtime",
			Outcome:     MightFail,
			Reason:      "can't verify bin against allowlist without knowing the value",
		})
		return
	}
	if strings.Contains(first, "${") {
		r.Outcome = MightFail
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: fmt.Sprintf("argv[0] = %q (contains unresolved interpolation)", first),
			Outcome:     MightFail,
			Reason:      "value depends on runtime bindings",
		})
		return
	}
	if env.Bins != nil && !env.Bins[first] {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("shell binary %q not in sim --allow-bin allowlist (have: %s)", first, sortedBins(env.Bins)))
		return
	}
	// Shell metacharacters in the command are a yellow flag — at runtime
	// they could expand to anything.
	if strings.ContainsAny(cmd, "|`$;&") && !strings.Contains(cmd, "${") {
		// Shell substitution likely; flag as uncertain.
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: "command contains shell metachars (pipes / substitution / chained)",
			Outcome:     MightFail,
			Reason:      "subsequent processes are not bin-allowlist-checked",
		})
		if r.Outcome == WillRun {
			r.Outcome = MightFail
		}
	}
}

func classifyHTTP(r *OpResult, args map[string]any, env SimEnv) {
	if env.NoNetwork {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "network capability denied by sim --no-network")
		return
	}
	url := stringArg(args, "url", "_0")
	if url == "" {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "URL not statically determinable")
		return
	}
	host := extractHost(url)
	if host == "" || strings.Contains(host, "${") {
		r.Outcome = MightFail
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: fmt.Sprintf("URL host = %q (interpolated)", host),
			Outcome:     MightFail,
			Reason:      "can't verify against host allowlist without runtime values",
		})
		return
	}
	if env.Network != nil && !hostAllowed(host, env.Network) {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("HTTP host %q not in sim --allow-host allowlist (have: %s)", host, strings.Join(env.Network, ", ")))
		return
	}
	// Redirect scenario: even if `host` is allowed, the server can
	// redirect anywhere. Worth flagging when an allowlist is in play.
	if env.Network != nil && len(env.Network) > 0 {
		r.Scenarios = append(r.Scenarios, Scenario{
			Description: fmt.Sprintf("server at %q could redirect to any host", host),
			Outcome:     MightFail,
			Reason:      "perch re-checks every redirect against the allowlist; this op succeeds if redirects stay within the allowlist or there are no redirects",
		})
		if r.Outcome == WillRun {
			r.Outcome = MightFail
		}
	}
}

func classifyWrite(r *OpResult, args map[string]any, env SimEnv) {
	if env.NoWrite {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "write capability denied by sim --no-write")
		return
	}
	path := firstPathArg(args)
	if path == "" || strings.Contains(path, "${") {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "target path not statically determinable")
		return
	}
	if env.FsWrite != nil && !pathUnder(path, env.FsWrite) {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("write path %q is outside sim --allow-write roots (allowed: %s)", path, strings.Join(env.FsWrite, ", ")))
	}
}

func classifyRead(r *OpResult, args map[string]any, env SimEnv) {
	path := firstPathArg(args)
	if path == "" || strings.Contains(path, "${") {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "read path not statically determinable")
		return
	}
	if env.FsRead != nil && !pathUnder(path, env.FsRead) {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("read path %q is outside sim --allow-read roots (allowed: %s)", path, strings.Join(env.FsRead, ", ")))
	}
}

func classifyHasBin(r *OpResult, args map[string]any, env SimEnv) {
	bin := stringArg(args, "_0", "name")
	if bin == "" || strings.Contains(bin, "${") {
		r.Outcome = MightFail
		r.Reasons = append(r.Reasons, "bin name not statically determinable")
		return
	}
	if env.Bins != nil {
		if env.Bins[bin] {
			r.Reasons = append(r.Reasons, fmt.Sprintf("has_bin(%q) = true (in sim --have-bin)", bin))
		} else {
			r.Reasons = append(r.Reasons, fmt.Sprintf("has_bin(%q) = false (not in sim --have-bin)", bin))
		}
	}
}

func classifyIf(r *OpResult, op domain.Op, env SimEnv, depth int, p *domain.Program, cmdName string) {
	r.IsBlockEntry = true
	cond := stringArg(op.Args, "op")
	lhs := stringArg(op.Args, "lhs")
	rhs := stringArg(op.Args, "rhs")

	// Try to evaluate the condition against the SimEnv.
	known, value := resolveAutoBound(lhs, env)
	if known {
		// We can decide this branch statically.
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
			r.Reasons = append(r.Reasons, fmt.Sprintf("condition %s %s %q evaluates TRUE (sim %s=%q) — body runs", lhs, cond, rhs, lhs, value))
			r.Children = simulateBody(op.Body, env, depth+1, p, cmdName)
		} else {
			r.Reasons = append(r.Reasons, fmt.Sprintf("condition %s %s %q evaluates FALSE (sim %s=%q) — body skipped", lhs, cond, rhs, lhs, value))
			// Don't simulate the body — it would mislead.
		}
		rollupChildren(r)
		return
	}
	// Condition can't be resolved against the sim env. Simulate the
	// body as a "maybe runs" branch.
	r.Children = simulateBody(op.Body, env, depth+1, p, cmdName)
	r.Reasons = append(r.Reasons, fmt.Sprintf("condition (%s %s) depends on runtime value — body presented as MIGHT-RUN", lhs, cond))
	rollupChildren(r)
	if r.Outcome == WillRun {
		r.Outcome = MightFail
	}
}

func classifyIfCall(r *OpResult, op domain.Op, env SimEnv, depth int, p *domain.Program, cmdName string) {
	r.IsBlockEntry = true
	fn := stringArg(op.Args, "func")
	arg := stringArg(op.Args, "_0")
	resolved := false
	take := false
	reason := ""
	switch fn {
	case "exists":
		if env.FsRead != nil {
			take = pathUnder(arg, env.FsRead)
			resolved = true
			reason = fmt.Sprintf("exists(%q) → %v under sim --allow-read", arg, take)
		}
	case "has_bin":
		if env.Bins != nil {
			take = env.Bins[arg]
			resolved = true
			reason = fmt.Sprintf("has_bin(%q) → %v under sim --have-bin", arg, take)
		}
	}
	if resolved {
		r.Reasons = append(r.Reasons, reason)
		if take {
			r.Children = simulateBody(op.Body, env, depth+1, p, cmdName)
		}
		rollupChildren(r)
		return
	}
	// Unknown predicate or no env restriction — simulate as MIGHT-RUN.
	r.Children = simulateBody(op.Body, env, depth+1, p, cmdName)
	r.Reasons = append(r.Reasons, fmt.Sprintf("predicate %s(%q) not resolvable in sim env — body MIGHT run", fn, arg))
	rollupChildren(r)
	if r.Outcome == WillRun {
		r.Outcome = MightFail
	}
}

func classifyRun(r *OpResult, op domain.Op, env SimEnv, depth int, p *domain.Program) {
	target := stringArg(op.Args, "target")
	if target == "" {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, "run: empty target")
		return
	}
	_, ok := p.Commands[target]
	if !ok {
		r.Outcome = WillFailOut
		r.Reasons = append(r.Reasons, fmt.Sprintf("run: target command %q not found", target))
		return
	}
	r.IsBlockEntry = true
	r.Reasons = append(r.Reasons, fmt.Sprintf("dispatches to command %q", target))
	r.Children = simulateBody(p.Commands[target].Ops, env, depth+1, p, target)
	rollupChildren(r)
}

func classifyWasmRun(r *OpResult, op domain.Op, env SimEnv, depth int) {
	modulePath := stringArg(op.Args, "path", "_0")
	// We can't statically open the .wasm to verify; just note it.
	r.Reasons = append(r.Reasons, fmt.Sprintf("wasm module: %s (capability-gated execution)", modulePath))
	// The body has marker ops (wasm_arg etc.); they don't really
	// "fail" but we list them for completeness.
	r.IsBlockEntry = true
	for _, b := range op.Body {
		r.Children = append(r.Children, OpResult{Op: b, Depth: depth + 1, Outcome: WillRun})
	}
}

// ── env-interpolation check (cross-cutting) ────────────────────────

var envRefRe = regexp.MustCompile(`\$\{([A-Z][A-Z0-9_]*)\}`)

func checkEnvInterpolation(args map[string]any, env SimEnv) (string, []Scenario) {
	if !env.EnvRestrict || env.Env == nil {
		return "", nil
	}
	for _, v := range args {
		s, ok := v.(string)
		if !ok {
			continue
		}
		for _, m := range envRefRe.FindAllStringSubmatch(s, -1) {
			name := m[1]
			if _, set := env.Env[name]; !set {
				return fmt.Sprintf("references ${%s} but sim --env restricts host envs to %s", name, sortedKeys(env.Env)), nil
			}
		}
	}
	return "", nil
}

// ── helpers ────────────────────────────────────────────────────────

func commandNames(p *domain.Program) []string {
	out := make([]string, 0, len(p.Commands))
	for n, c := range p.Commands {
		if c.Modifiers.Private || c.Modifiers.Test {
			continue
		}
		out = append(out, n)
	}
	sort.Strings(out)
	return out
}

func resolveArgs(args map[string]any, env SimEnv) map[string]any {
	out := make(map[string]any, len(args))
	for k, v := range args {
		out[k] = v
	}
	return out
}

func stringArg(args map[string]any, keys ...string) string {
	for _, k := range keys {
		if v, ok := args[k]; ok {
			if s, ok := v.(string); ok && s != "" {
				return s
			}
		}
	}
	return ""
}

func firstPathArg(args map[string]any) string {
	for _, k := range []string{"path", "dst", "_0", "_1"} {
		if v, ok := args[k]; ok {
			if s, ok := v.(string); ok && s != "" {
				return s
			}
		}
	}
	return ""
}

func firstShellToken(s string) string {
	s = strings.TrimSpace(s)
	for i := 0; i < len(s); i++ {
		if s[i] == ' ' || s[i] == '\t' {
			return s[:i]
		}
	}
	return s
}

func extractHost(url string) string {
	// crude: strip scheme then take up to the first / or ?
	if idx := strings.Index(url, "://"); idx > -1 {
		url = url[idx+3:]
	}
	for i := 0; i < len(url); i++ {
		c := url[i]
		if c == '/' || c == '?' || c == '#' {
			return url[:i]
		}
	}
	return url
}

func hostAllowed(host string, allow []string) bool {
	for _, a := range allow {
		if a == host {
			return true
		}
		// single-label wildcard: *.example.com matches a.example.com (not a.b.example.com)
		if strings.HasPrefix(a, "*.") {
			suffix := a[1:] // ".example.com"
			if strings.HasSuffix(host, suffix) {
				// require no extra label between * and suffix
				head := host[:len(host)-len(suffix)]
				if !strings.Contains(head, ".") {
					return true
				}
			}
		}
	}
	return false
}

func pathUnder(path string, roots []string) bool {
	clean := filepath.Clean(path)
	for _, r := range roots {
		cr := filepath.Clean(r)
		if clean == cr || strings.HasPrefix(clean, cr+string(filepath.Separator)) {
			return true
		}
	}
	return false
}

// resolveAutoBound returns true if `name` is an auto-bound variable
// the simulator can resolve against the SimEnv (os / arch / etc.) and
// the corresponding value.
func resolveAutoBound(name string, env SimEnv) (bool, string) {
	switch name {
	case "os":
		if env.OS != "" {
			return true, env.OS
		}
	case "arch":
		if env.Arch != "" {
			return true, env.Arch
		}
	case "is_windows":
		if env.OS != "" {
			return true, boolStr(env.OS == "windows")
		}
	case "is_macos":
		if env.OS != "" {
			return true, boolStr(env.OS == "darwin")
		}
	case "is_linux":
		if env.OS != "" {
			return true, boolStr(env.OS == "linux")
		}
	case "is_unix":
		if env.OS != "" {
			return true, boolStr(env.OS != "windows")
		}
	case "is_arm64":
		if env.Arch != "" {
			return true, boolStr(env.Arch == "arm64")
		}
	case "is_amd64":
		if env.Arch != "" {
			return true, boolStr(env.Arch == "amd64")
		}
	}
	return false, ""
}

// osMatches mirrors ops.OsTargetMatches but lives here to avoid a
// dependency on the ops package from usecases/. Kept in sync by hand.
func osMatches(target, host string) bool {
	if target == "" || host == "" {
		return false
	}
	if target == host {
		return true
	}
	if target == "unix" {
		switch host {
		case "darwin", "linux", "freebsd", "openbsd", "netbsd":
			return true
		}
	}
	return false
}

func boolStr(b bool) string {
	if b {
		return "true"
	}
	return "false"
}

func sortedBins(m map[string]bool) string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return strings.Join(keys, ", ")
}

func sortedKeys(m map[string]string) string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return strings.Join(keys, ", ")
}

// ── report rendering ───────────────────────────────────────────────

// RenderResult is the exported wrapper. Renders the per-op tree the
// CLI prints, into w.
func RenderResult(w io.Writer, res SimResult, p *domain.Program, name string) {
	renderResult(w, res, p, name)
}

func renderResult(w io.Writer, res SimResult, p *domain.Program, name string) {
	cmd := p.Commands[name]
	fmt.Fprintf(w, "── command %s ", name)
	if cmd != nil && cmd.Description != "" {
		fmt.Fprintf(w, "— %s", cmd.Description)
	}
	fmt.Fprintln(w)
	for _, op := range res.Ops {
		renderOpResult(w, op, "")
	}
	fmt.Fprintln(w)
	fmt.Fprintf(w, "summary: %d will-run · %d will-fail · %d uncertain\n",
		res.WillRun, res.WillFail, res.Uncertain)
}

func renderOpResult(w io.Writer, r OpResult, indent string) {
	glyph := "✓"
	switch r.Outcome {
	case WillFailOut:
		glyph = "✗"
	case MightFail:
		glyph = "?"
	}
	fmt.Fprintf(w, "%s%s %s\n", indent, glyph, summarizeOp(r.Op))
	for _, reason := range r.Reasons {
		fmt.Fprintf(w, "%s   ↳ %s\n", indent, reason)
	}
	for _, s := range r.Scenarios {
		fmt.Fprintf(w, "%s   • %s\n", indent, s.Description)
		fmt.Fprintf(w, "%s     %s\n", indent, s.Reason)
	}
	for _, c := range r.Children {
		renderOpResult(w, c, indent+"   ")
	}
}

func summarizeOp(op domain.Op) string {
	switch op.Kind {
	case "_template_call":
		return fmt.Sprintf("call %s", stringArg(op.Args, "name"))
	case "if":
		return fmt.Sprintf("if %s %s %s", stringArg(op.Args, "lhs"), stringArg(op.Args, "op"), stringArg(op.Args, "rhs"))
	case "if_call":
		return fmt.Sprintf("if %s %q", stringArg(op.Args, "func"), stringArg(op.Args, "_0"))
	case "run":
		return fmt.Sprintf("run %s", stringArg(op.Args, "target"))
	case "wasm_run":
		return fmt.Sprintf("wasm_run %q", stringArg(op.Args, "path", "_0"))
	}
	preview := opPreview(op.Args)
	if preview == "" {
		return op.Kind
	}
	return fmt.Sprintf("%s %s", op.Kind, preview)
}

func opPreview(args map[string]any) string {
	priority := []string{"msg", "cmd", "url", "path", "dst", "_0", "key", "name"}
	for _, k := range priority {
		if v, ok := args[k]; ok {
			s, _ := v.(string)
			if s != "" {
				if len(s) > 60 {
					s = s[:57] + "..."
				}
				return fmt.Sprintf("%q", s)
			}
		}
	}
	return ""
}

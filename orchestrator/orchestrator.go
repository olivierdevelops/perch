// Package orchestrator is the wiring layer — the only place that
// imports concrete types from every other VHCO module and composes
// them. Every *Impl, every `make…` factory, every closure that bridges
// one module to another lives here.
//
// The package exports a single entry point — Run — that the top-level
// main.go calls. main.go itself stays a one-liner.
package orchestrator

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/audit"
	"github.com/luowensheng/perch/infra/capyloader"
	"github.com/luowensheng/perch/infra/embed"
	"github.com/luowensheng/perch/infra/httpserver"
	"github.com/luowensheng/perch/infra/interpreter"
	"github.com/luowensheng/perch/infra/ops"
	"github.com/luowensheng/perch/infra/preview"

	"github.com/luowensheng/perch/io/cli"
	"github.com/luowensheng/perch/usecases/commandhelp"
	"github.com/luowensheng/perch/usecases/initconfig"
	"github.com/luowensheng/perch/usecases/installlsp"
	"github.com/luowensheng/perch/usecases/installvscode"
	"github.com/luowensheng/perch/usecases/help"
	"github.com/luowensheng/perch/usecases/importsh"
	"github.com/luowensheng/perch/usecases/scan"
	"github.com/luowensheng/perch/usecases/listcommands"
	"github.com/luowensheng/perch/usecases/runbuild"
	"github.com/luowensheng/perch/usecases/runcommand"
	"github.com/luowensheng/perch/usecases/runserver"
	"github.com/luowensheng/perch/usecases/runshell"
	"github.com/luowensheng/perch/usecases/validate"
)

// Version is the perch version string. Bumped on each release tag.
const Version = "0.1.0"

// DefaultCommandsFile is the default config the CLI looks for when no
// -f flag is given.
const DefaultCommandsFile = "commands.perch"

// Run is the perch entry point. It:
//
//   1. Detects whether the running binary has an embedded program
//      (created by `perch --build`) and runs that if so.
//   2. Otherwise wires the regular CLI against the host filesystem.
//   3. Calls os.Exit with the CLI's exit code.
//
// Top-level main.go is a one-liner that calls this function.
func Run() {
	// `perch help TOPIC` and `perch help --json` need the literal flag
	// tokens preserved (the topic name may BE a flag like --no-shell).
	// Skip the global-flag stripping in that case — the help use case
	// reads os.Args directly.
	if len(os.Args) >= 2 && os.Args[1] == "help" {
		os.Exit(buildCLI(ops.Restrictions{}, nil, nil, false, "", 0, nil, "", false).Run())
	}

	// Global flags are stripped from os.Args before any sub-CLI sees
	// them. Each call below removes the flags it owns and returns the
	// parsed value. See docs/sandbox.md for the design.
	restrictions := extractRestrictionFlags()
	envAllow := extractEnvFlag()
	allowBins, noMeta := extractShellGuards()
	allow := extractAllowFlags()
	auditPath := extractAuditFlag()
	maxRuntime := extractMaxRuntimeFlag()
	httpPolicy := extractHTTPPolicyFlags()
	previewMode := extractPreviewFlags()

	// Stdin input (-f -) is treated as untrusted by default. The user
	// piped this from somewhere — curl, paste, a sibling process — and
	// we have no chain of custody. Apply the strictest restrictions
	// and require explicit --allow-X to grant capabilities. The user
	// can pass --trust-stdin to skip this entirely (e.g. when piping
	// their own .perch). Same model Deno uses for `--allow-*` flags.
	stdinUntrusted := false
	if isStdinInvocation() && !allow.TrustStdin {
		stdinUntrusted = true
		if !allow.Shell {
			restrictions.NoShell = true
		}
		if !allow.Subprocess {
			restrictions.NoSubprocess = true
		}
		if !allow.Network {
			restrictions.NoNetwork = true
		}
		if !allow.Write {
			restrictions.NoWrite = true
		}
		// Empty (non-nil) allowlist blocks all host env-var fallthrough,
		// unless the user passed --env A,B,C explicitly which already
		// populated this map.
		if envAllow == nil {
			envAllow = map[string]bool{}
		}
	}

	if showRestrictionsAndExit() {
		os.Exit(0)
	}

	if bundle, ok, err := embed.Load(); err != nil {
		fmt.Fprintln(os.Stderr, "embedded program:", err)
		os.Exit(1)
	} else if ok {
		os.Exit(buildEmbeddedCLI(bundle, restrictions, envAllow, allowBins, noMeta, auditPath, maxRuntime, httpPolicy, previewMode, stdinUntrusted).Run())
	}
	os.Exit(buildCLI(restrictions, envAllow, allowBins, noMeta, auditPath, maxRuntime, httpPolicy, previewMode, stdinUntrusted).Run())
}

// extractHTTPPolicyFlags strips `--max-redirects N`, `--no-redirects`,
// `--allow-private-ips`, `--allow-scheme-downgrade`. Returns nil to
// signal "use the secure defaults" (no explicit flags given) — the
// caller / http ops handle that case.
func extractHTTPPolicyFlags() *interpreter.HTTPPolicy {
	out := os.Args[:1]
	pol := interpreter.HTTPPolicy{MaxRedirects: 5} // secure defaults
	touched := false
	i := 1
	for i < len(os.Args) {
		a := os.Args[i]
		switch {
		case a == "--no-redirects":
			pol.MaxRedirects = 0
			touched = true
			i++
		case a == "--allow-private-ips":
			pol.AllowPrivateIPs = true
			touched = true
			i++
		case a == "--allow-scheme-downgrade":
			pol.AllowSchemeDowngrade = true
			touched = true
			i++
		case a == "--max-redirects":
			if i+1 < len(os.Args) {
				n, err := strconv.Atoi(os.Args[i+1])
				if err != nil || n < 0 {
					fmt.Fprintf(os.Stderr, "--max-redirects: bad value %q\n", os.Args[i+1])
					os.Exit(2)
				}
				pol.MaxRedirects = n
				touched = true
				i += 2
				continue
			}
			fmt.Fprintln(os.Stderr, "--max-redirects requires a non-negative integer")
			os.Exit(2)
		case strings.HasPrefix(a, "--max-redirects="):
			n, err := strconv.Atoi(a[len("--max-redirects="):])
			if err != nil || n < 0 {
				fmt.Fprintf(os.Stderr, "--max-redirects: bad value %q\n", a)
				os.Exit(2)
			}
			pol.MaxRedirects = n
			touched = true
			i++
		default:
			out = append(out, a)
			i++
		}
	}
	os.Args = out
	if !touched {
		return nil // signal "use defaults"
	}
	return &pol
}

// allowFlags captures the user's explicit `--allow-*` opt-ins, used to
// override the stdin-default deny posture. Each flag is purely positive:
// it can only loosen what stdin-mode would have tightened. It never
// loosens an explicit `--no-X`.
type allowFlags struct {
	Shell      bool
	Subprocess bool
	Network    bool
	Write      bool
	TrustStdin bool
}

// extractAllowFlags strips `--allow-shell`, `--allow-subprocess`,
// `--allow-network`, `--allow-write`, and `--trust-stdin` from os.Args.
func extractAllowFlags() allowFlags {
	out := os.Args[:1]
	var a allowFlags
	for _, arg := range os.Args[1:] {
		switch arg {
		case "--allow-shell":
			a.Shell = true
		case "--allow-subprocess":
			a.Subprocess = true
		case "--allow-network":
			a.Network = true
		case "--allow-write":
			a.Write = true
		case "--trust-stdin":
			a.TrustStdin = true
		default:
			out = append(out, arg)
		}
	}
	os.Args = out
	return a
}

// isStdinInvocation peeks at the remaining os.Args (after all global
// flags are stripped) to see if the user passed `-f -`. That signals
// "load the .perch source from stdin" and triggers the untrusted-by-
// default posture.
func isStdinInvocation() bool {
	for i := 1; i < len(os.Args)-1; i++ {
		if os.Args[i] == "-f" && os.Args[i+1] == "-" {
			return true
		}
	}
	return false
}

// extractAuditFlag peels off `--audit PATH` / `--audit=PATH`. Returns ""
// when not given. PATH "-" means stdout.
func extractAuditFlag() string {
	out := os.Args[:1]
	path := ""
	i := 1
	for i < len(os.Args) {
		a := os.Args[i]
		switch {
		case a == "--audit":
			if i+1 < len(os.Args) {
				path = os.Args[i+1]
				i += 2
				continue
			}
			fmt.Fprintln(os.Stderr, "--audit requires a path (use - for stdout)")
			os.Exit(2)
		case strings.HasPrefix(a, "--audit="):
			path = a[len("--audit="):]
			i++
		default:
			out = append(out, a)
			i++
		}
	}
	os.Args = out
	return path
}

// extractMaxRuntimeFlag peels off `--max-runtime SECS` / `--max-runtime=SECS`.
// Returns 0 (no limit) when not given.
func extractMaxRuntimeFlag() time.Duration {
	out := os.Args[:1]
	var d time.Duration
	parse := func(s string) {
		n, err := strconv.Atoi(s)
		if err != nil || n < 0 {
			fmt.Fprintf(os.Stderr, "--max-runtime: bad value %q (want a non-negative integer of seconds)\n", s)
			os.Exit(2)
		}
		d = time.Duration(n) * time.Second
	}
	i := 1
	for i < len(os.Args) {
		a := os.Args[i]
		switch {
		case a == "--max-runtime":
			if i+1 < len(os.Args) {
				parse(os.Args[i+1])
				i += 2
				continue
			}
			fmt.Fprintln(os.Stderr, "--max-runtime requires a value in seconds")
			os.Exit(2)
		case strings.HasPrefix(a, "--max-runtime="):
			parse(a[len("--max-runtime="):])
			i++
		default:
			out = append(out, a)
			i++
		}
	}
	os.Args = out
	return d
}

// extractShellGuards peels off `--allow-bin NAME[,NAME…]` (additive,
// repeatable) and `--no-shell-metachars`. These are the second line of
// defense when `shell` is allowed but you still want to bound which
// binaries it may invoke and reject pipe / redirect / $() / && / ; tricks.
// Returns (nil, false) when neither flag is given (legacy "no extra check").
func extractShellGuards() (map[string]bool, bool) {
	out := os.Args[:1]
	var allow map[string]bool
	noMeta := false
	add := func(s string) {
		if allow == nil {
			allow = map[string]bool{}
		}
		for _, name := range strings.Split(s, ",") {
			name = strings.TrimSpace(name)
			if name != "" {
				allow[name] = true
			}
		}
	}
	i := 1
	for i < len(os.Args) {
		a := os.Args[i]
		switch {
		case a == "--no-shell-metachars":
			noMeta = true
			i++
		case a == "--allow-bin":
			if i+1 < len(os.Args) {
				add(os.Args[i+1])
				i += 2
				continue
			}
			if allow == nil {
				allow = map[string]bool{}
			}
			i++
		case strings.HasPrefix(a, "--allow-bin="):
			add(a[len("--allow-bin="):])
			i++
		default:
			out = append(out, a)
			i++
		}
	}
	os.Args = out
	return allow, noMeta
}

// extractRestrictionFlags walks os.Args, peels off any of:
//
//	--no-shell        --no-subprocess
//	--no-network      --no-write
//
// and returns a Restrictions struct reflecting which were present.
// Multiple flags compose (additive). Stripped flags are removed from
// os.Args so downstream parsing doesn't see them.
func extractRestrictionFlags() ops.Restrictions {
	out := os.Args[:1]
	r := ops.Restrictions{}
	for _, a := range os.Args[1:] {
		switch a {
		case "--no-shell":
			r.NoShell = true
		case "--no-subprocess":
			r.NoSubprocess = true
		case "--no-network":
			r.NoNetwork = true
		case "--no-write":
			r.NoWrite = true
		default:
			out = append(out, a)
		}
	}
	os.Args = out
	return r
}

// extractEnvFlag peels off every `--env A,B,C` (or `--env=A,B,C`) from
// os.Args and returns the union as a set. nil = flag never given =
// legacy behavior (every host env var visible). Empty non-nil = `--env`
// given with no names = no env vars visible at all.
func extractEnvFlag() map[string]bool {
	out := os.Args[:1]
	var allow map[string]bool
	add := func(s string) {
		if allow == nil {
			allow = map[string]bool{}
		}
		for _, name := range strings.Split(s, ",") {
			name = strings.TrimSpace(name)
			if name != "" {
				allow[name] = true
			}
		}
	}
	i := 1
	for i < len(os.Args) {
		a := os.Args[i]
		switch {
		case a == "--env":
			if i+1 < len(os.Args) {
				add(os.Args[i+1])
				i += 2
				continue
			}
			// Bare `--env` with no value: treat as "allow nothing."
			if allow == nil {
				allow = map[string]bool{}
			}
			i++
		case strings.HasPrefix(a, "--env="):
			add(a[len("--env="):])
			i++
		default:
			out = append(out, a)
			i++
		}
	}
	os.Args = out
	return allow
}

// showRestrictionsAndExit prints the available --no-X flags + the ops
// each one blocks, then returns true if the user asked for the listing.
func showRestrictionsAndExit() bool {
	for _, a := range os.Args[1:] {
		if a == "--restrictions" {
			fmt.Println("Available restriction flags:")
			for _, name := range ops.RestrictionList() {
				fmt.Printf("  --%-15s blocks: %v\n", name, ops.BlockedByRestriction(name))
			}
			fmt.Println()
			fmt.Println("Compose freely:")
			fmt.Println("  perch --no-shell --no-network --no-write <cmd>")
			fmt.Println("  perch --env HOME,PATH,API_KEY <cmd>   # restrict host env-var visibility")
			return true
		}
	}
	return false
}

// extractPreviewFlags strips `--ask` / `--dry-run` from os.Args and
// returns one of "", "ask", "dry-run". Both flags are mutually exclusive
// with each other; `--ask` wins if both are given.
func extractPreviewFlags() string {
	out := os.Args[:1]
	mode := ""
	for _, a := range os.Args[1:] {
		switch a {
		case "--ask":
			mode = "ask"
		case "--dry-run":
			if mode == "" {
				mode = "dry-run"
			}
		default:
			out = append(out, a)
		}
	}
	os.Args = out
	return mode
}

// announceSecurityPosture prints a one-line banner naming the active
// restrictions (if any) so users / reviewers see the posture without
// having to dig. Silent when nothing's restricted.
func announceSecurityPosture(r ops.Restrictions, envAllow, allowBins map[string]bool, noMeta bool, auditPath string, maxRuntime time.Duration, stdinUntrusted bool) {
	parts := []string{}
	if r.Active() {
		parts = append(parts, strings.Join(r.AsFlags(), " "))
	}
	if envAllow != nil {
		names := make([]string, 0, len(envAllow))
		for k := range envAllow {
			names = append(names, k)
		}
		if len(names) > 0 {
			parts = append(parts, fmt.Sprintf("--env %s", strings.Join(names, ",")))
		} else {
			parts = append(parts, "--env (empty)")
		}
	}
	if allowBins != nil {
		names := make([]string, 0, len(allowBins))
		for k := range allowBins {
			names = append(names, k)
		}
		parts = append(parts, fmt.Sprintf("--allow-bin %s", strings.Join(names, ",")))
	}
	if noMeta {
		parts = append(parts, "--no-shell-metachars")
	}
	if auditPath != "" {
		parts = append(parts, fmt.Sprintf("--audit %s", auditPath))
	}
	if maxRuntime > 0 {
		parts = append(parts, fmt.Sprintf("--max-runtime %ds", int(maxRuntime.Seconds())))
	}
	if stdinUntrusted {
		fmt.Fprintf(os.Stderr, "🔒 stdin (untrusted): %s\n", strings.Join(parts, "  "))
		fmt.Fprintln(os.Stderr, "   → grant capabilities with --allow-shell / --allow-subprocess / --allow-network / --allow-write / --env A,B,C")
		fmt.Fprintln(os.Stderr, "   → or skip the deny-by-default posture with --trust-stdin")
	} else if len(parts) > 0 {
		fmt.Fprintf(os.Stderr, "🔒 security: %s\n", strings.Join(parts, "  "))
	}
}

// buildInterpreterHook returns the BeforeOp matching previewMode, or
// nil for normal execution. Centralized so buildCLI and
// buildEmbeddedCLI share the wiring.
func buildInterpreterHook(previewMode string) interpreter.BeforeOp {
	switch previewMode {
	case "ask":
		fmt.Println("──── Step-through preview — y=run, n=skip, a=all, q=quit ────")
		return preview.AskHook(os.Stdin, os.Stdout)
	case "dry-run":
		fmt.Println("──── Dry-run — printing plan; no ops execute ────")
		return preview.DryRunHook(os.Stdout)
	}
	return nil
}


// knownOps returns the set of op kinds the interpreter knows how to
// dispatch. Used by the validator to flag misspelt op kinds.
func knownOps(handlers map[string]interpreter.Handler) func() map[string]struct{} {
	return func() map[string]struct{} {
		out := make(map[string]struct{}, len(handlers))
		for k := range handlers {
			out[k] = struct{}{}
		}
		return out
	}
}

func buildCLI(r ops.Restrictions, envAllow, allowBins map[string]bool, noMeta bool, auditPath string, maxRuntime time.Duration, httpPolicy *interpreter.HTTPPolicy, previewMode string, stdinUntrusted bool) *cli.CLI {
	handlers := ops.AllHandlers()
	ops.ApplyRestrictions(handlers, r)
	announceSecurityPosture(r, envAllow, allowBins, noMeta, auditPath, maxRuntime, stdinUntrusted)
	hook := buildInterpreterHook(previewMode)
	runFn := func(p *domain.Program, name string, args []string) error {
		i := interpreter.New(handlers, p)
		i.BeforeOp = hook
		i.EnvAllowlist = envAllow
		i.AllowedShellBins = allowBins
		i.NoShellMetachars = noMeta
		i.HTTPPolicy = httpPolicy
		if maxRuntime > 0 {
			i.Deadline = time.Now().Add(maxRuntime)
		}
		var auditDone func(error)
		if auditPath != "" {
			sink, _, err := audit.Open(auditPath)
			if err != nil {
				return err
			}
			auditDone = sink.WireInto(i, name, args)
		}
		err := i.Run(name, args)
		if auditDone != nil {
			auditDone(err)
		}
		return err
	}
	srv := &httpserver.Server{Handlers: handlers}

	use := cli.UseCases{
		Run: &runcommand.Impl{
			Load:    capyloader.Load,
			Run:     runFn,
			Suggest: commandhelp.Suggest,
		},
		List: &listcommands.Impl{
			Load: capyloader.Load,
		},
		Init: &initconfig.Impl{},
		Build: &runbuild.Impl{
			Load:  capyloader.Load,
			Embed: embed.Embed,
		},
		Server: &runserver.Impl{
			Load:  capyloader.Load,
			Serve: srv.Serve,
		},
		Shell: &runshell.Impl{
			LoadString: capyloader.LoadFromString,
			Load:       capyloader.Load,
			Handlers:   handlers,
		},
		Validate: &validate.Impl{
			Load:     capyloader.Load,
			KnownOps: knownOps(handlers),
		},
		CommandHelp: &commandhelp.Impl{
			Load: capyloader.Load,
		},
		InstallLSP:    &installlsp.Impl{},
		InstallVSCode: &installvscode.Impl{InstallLSP: (&installlsp.Impl{}).Execute},
		ImportSh:      &importsh.Impl{},
		Scan:          &scan.Impl{Load: capyloader.Load},
		Help:          &help.Impl{Version: Version},
	}

	return &cli.CLI{
		UseCases: use,
		Config: cli.Config{
			DefaultCommandsFile: DefaultCommandsFile,
			Version:             Version,
		},
	}
}

// buildEmbeddedCLI returns a CLI whose Run/List use-cases ignore the
// supplied config path and serve the embedded program instead.
func buildEmbeddedCLI(bundle *embed.Bundle, r ops.Restrictions, envAllow, allowBins map[string]bool, noMeta bool, auditPath string, maxRuntime time.Duration, httpPolicy *interpreter.HTTPPolicy, previewMode string, stdinUntrusted bool) *cli.CLI {
	p := bundle.Program
	handlers := ops.AllHandlers()
	ops.ApplyRestrictions(handlers, r)
	announceSecurityPosture(r, envAllow, allowBins, noMeta, auditPath, maxRuntime, stdinUntrusted)
	hook := buildInterpreterHook(previewMode)
	// Wire the bundle archive into the ops registry so bundle_dir /
	// bundle_hash / bundle_extract have something to read.
	ops.SetBundle(bundle.Archive, bundle.ArchiveHash)
	loadEmbedded := func(_ string) (*domain.Program, error) { return p, nil }
	runFn := func(_ *domain.Program, name string, args []string) error {
		i := interpreter.New(handlers, p)
		i.BeforeOp = hook
		i.EnvAllowlist = envAllow
		i.AllowedShellBins = allowBins
		i.NoShellMetachars = noMeta
		i.HTTPPolicy = httpPolicy
		if maxRuntime > 0 {
			i.Deadline = time.Now().Add(maxRuntime)
		}
		var auditDone func(error)
		if auditPath != "" {
			sink, _, err := audit.Open(auditPath)
			if err != nil {
				return err
			}
			auditDone = sink.WireInto(i, name, args)
		}
		err := i.Run(name, args)
		if auditDone != nil {
			auditDone(err)
		}
		return err
	}
	srv := &httpserver.Server{Handlers: handlers}

	use := cli.UseCases{
		Run: &runcommand.Impl{
			Load:    loadEmbedded,
			Run:     runFn,
			Suggest: commandhelp.Suggest,
		},
		List: &listcommands.Impl{
			Load: loadEmbedded,
		},
		Init: &initconfig.Impl{},
		// Embedded binaries shouldn't re-embed; surface a friendly error.
		Build: &disabledBuild{},
		Server: &runserver.Impl{
			Load:  loadEmbedded,
			Serve: srv.Serve,
		},
		Shell: &runshell.Impl{
			LoadString: capyloader.LoadFromString,
			Load:       loadEmbedded,
			Handlers:   handlers,
		},
		Validate: &validate.Impl{
			Load:     loadEmbedded,
			KnownOps: knownOps(handlers),
		},
		CommandHelp: &commandhelp.Impl{
			Load: loadEmbedded,
		},
		InstallLSP:    &installlsp.Impl{},
		InstallVSCode: &installvscode.Impl{InstallLSP: (&installlsp.Impl{}).Execute},
		ImportSh:      &importsh.Impl{},
		Scan:          &scan.Impl{Load: capyloader.Load},
		Help:          &help.Impl{Version: Version},
	}

	version := p.Version
	if version == "" {
		version = Version
	}
	return &cli.CLI{
		UseCases: use,
		Config: cli.Config{
			DefaultCommandsFile: DefaultCommandsFile,
			Version:             version,
		},
	}
}

type disabledBuild struct{}

func (disabledBuild) Execute(string, []string) error {
	return fmt.Errorf("--build is disabled in a binary that already embeds a program")
}

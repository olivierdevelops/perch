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

	"github.com/luowensheng/perch/domain"
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
	// `--mode NAME` is global to the invocation; strip it from os.Args
	// before any sub-CLI sees it, then apply to the op catalog. Modes
	// are how a caller picks a Phase-0 security posture — see
	// docs/sandbox.md and the modes package for the full list.
	mode := extractModeFlag()
	previewMode := extractPreviewFlags()
	if mode == "list" {
		fmt.Println("Available --mode values:")
		for _, m := range ops.Modes() {
			if m == ops.ModeTrusted {
				fmt.Printf("  %-10s  (default; full op catalog)\n", m)
			} else {
				fmt.Printf("  %-10s  blocks: %v\n", m, ops.BlockedOps(m))
			}
		}
		os.Exit(0)
	}

	if bundle, ok, err := embed.Load(); err != nil {
		fmt.Fprintln(os.Stderr, "embedded program:", err)
		os.Exit(1)
	} else if ok {
		os.Exit(buildEmbeddedCLI(bundle, mode, previewMode).Run())
	}
	os.Exit(buildCLI(mode, previewMode).Run())
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

// extractModeFlag removes any `--mode NAME` / `--mode=NAME` pair from
// os.Args and returns NAME (or "" if not provided). Done before the
// CLI parser so sub-commands never see the flag. Returns "list" if
// the user passed `--modes` (the discovery flag).
func extractModeFlag() string {
	out := os.Args[:1]
	mode := ""
	i := 1
	for i < len(os.Args) {
		a := os.Args[i]
		switch {
		case a == "--modes":
			return "list"
		case a == "--mode":
			if i+1 < len(os.Args) {
				mode = os.Args[i+1]
				i += 2
				continue
			}
			fmt.Fprintln(os.Stderr, "--mode requires a value (one of: "+joinModes()+")")
			os.Exit(2)
		case len(a) > 7 && a[:7] == "--mode=":
			mode = a[7:]
			i++
			continue
		default:
			out = append(out, a)
			i++
		}
	}
	if mode != "" && !ops.IsValidMode(mode) {
		fmt.Fprintf(os.Stderr, "unknown --mode %q (valid: %s)\n", mode, joinModes())
		os.Exit(2)
	}
	os.Args = out
	return mode
}

func joinModes() string {
	out := ""
	for i, m := range ops.Modes() {
		if i > 0 {
			out += ", "
		}
		out += m
	}
	return out
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

func buildCLI(mode, previewMode string) *cli.CLI {
	handlers := ops.AllHandlers()
	if err := ops.ApplyMode(handlers, mode); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	hook := buildInterpreterHook(previewMode)
	runFn := func(p *domain.Program, name string, args []string) error {
		i := interpreter.New(handlers, p)
		i.BeforeOp = hook
		return i.Run(name, args)
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
func buildEmbeddedCLI(bundle *embed.Bundle, mode, previewMode string) *cli.CLI {
	p := bundle.Program
	handlers := ops.AllHandlers()
	if err := ops.ApplyMode(handlers, mode); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	hook := buildInterpreterHook(previewMode)
	// Wire the bundle archive into the ops registry so bundle_dir /
	// bundle_hash / bundle_extract have something to read.
	ops.SetBundle(bundle.Archive, bundle.ArchiveHash)
	loadEmbedded := func(_ string) (*domain.Program, error) { return p, nil }
	runFn := func(_ *domain.Program, name string, args []string) error {
		i := interpreter.New(handlers, p)
		i.BeforeOp = hook
		return i.Run(name, args)
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

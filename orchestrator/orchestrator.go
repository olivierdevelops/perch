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
	if embedded, ok, err := embed.Load(); err != nil {
		fmt.Fprintln(os.Stderr, "embedded program:", err)
		os.Exit(1)
	} else if ok {
		os.Exit(buildEmbeddedCLI(embedded).Run())
	}
	os.Exit(buildCLI().Run())
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

func buildCLI() *cli.CLI {
	handlers := ops.AllHandlers()
	runFn := func(p *domain.Program, name string, args []string) error {
		return interpreter.New(handlers, p).Run(name, args)
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
func buildEmbeddedCLI(p *domain.Program) *cli.CLI {
	handlers := ops.AllHandlers()
	loadEmbedded := func(_ string) (*domain.Program, error) { return p, nil }
	runFn := func(_ *domain.Program, name string, args []string) error {
		return interpreter.New(handlers, p).Run(name, args)
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

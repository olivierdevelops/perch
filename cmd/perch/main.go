// Package main is the orchestrator: the one place that imports
// concrete types from every other module and wires them together.
package main

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
	"github.com/luowensheng/perch/usecases/listcommands"
	"github.com/luowensheng/perch/usecases/runbuild"
	"github.com/luowensheng/perch/usecases/runcommand"
	"github.com/luowensheng/perch/usecases/runserver"
	"github.com/luowensheng/perch/usecases/runshell"
	"github.com/luowensheng/perch/usecases/validate"
)

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

const Version = "0.1.0"
const DefaultCommandsFile = "commands.capy"

func main() {
	// If the binary has an embedded program (built via `perch --build`),
	// that program wins — the CLI ignores any -f flag.
	if embedded, ok, err := embed.Load(); err != nil {
		fmt.Fprintln(os.Stderr, "embedded program:", err)
		os.Exit(1)
	} else if ok {
		os.Exit(buildEmbeddedCLI(embedded).Run())
	}
	os.Exit(buildCLI().Run())
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

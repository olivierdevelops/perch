// Package cli parses argv into an intent and dispatches to use-cases.
package cli

import (
	_ "embed"
	"fmt"
	"os"
)

//go:embed completions/perch.bash
var completionBash string

//go:embed completions/perch.zsh
var completionZsh string

//go:embed completions/perch.fish
var completionFish string

type RunCommandUseCase interface {
	Execute(configPath, commandName string, args []string) error
}

type ListCommandsUseCase interface {
	Execute(configPath string) error
}

type InitConfigUseCase interface {
	Execute(path string) error
}

type RunBuildUseCase interface {
	Execute(configPath string, args []string) error
}

type RunServerUseCase interface {
	Execute(configPath string, args []string) error
}

type RunShellUseCase interface {
	Execute(configPath string) error
}

type ValidateUseCase interface {
	Execute(configPath string) error
}

type CommandHelpUseCase interface {
	Execute(configPath, commandName string) error
}

type InstallLSPUseCase interface {
	Execute() error
}

type InstallVSCodeUseCase interface {
	Execute() error
}

type ImportShUseCase interface {
	Execute(srcPath, outPath string) error
}

type UseCases struct {
	Run           RunCommandUseCase
	List          ListCommandsUseCase
	Init          InitConfigUseCase
	Build         RunBuildUseCase
	Server        RunServerUseCase
	Shell         RunShellUseCase
	Validate      ValidateUseCase
	CommandHelp   CommandHelpUseCase
	InstallLSP    InstallLSPUseCase
	InstallVSCode InstallVSCodeUseCase
	ImportSh      ImportShUseCase
}

type Config struct {
	DefaultCommandsFile string
	Version             string
}

type CLI struct {
	UseCases UseCases
	Config   Config
}

func (c *CLI) Run() int {
	if len(os.Args) == 1 {
		c.printHelp()
		return 0
	}

	args := os.Args[1:]
	var commandName string
	var remaining []string

	if args[0] == "-f" {
		if len(args) < 3 {
			fmt.Println("Usage: perch -f <file> <command> [args...]")
			return 1
		}
		filePath := args[1]
		commandName = args[2]
		remaining = append([]string{"-f", filePath}, args[3:]...)
	} else {
		commandName = args[0]
		remaining = args[1:]
	}

	switch commandName {
	case "--help", "-h":
		path, _ := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		return errExit(c.UseCases.List.Execute(path))
	case "--version":
		fmt.Println(c.Config.Version)
		return 0
	case "--init":
		return errExit(c.UseCases.Init.Execute(c.Config.DefaultCommandsFile))
	case "--build":
		path, rest := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		return errExit(c.UseCases.Build.Execute(path, rest))
	case "--server":
		path, rest := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		return errExit(c.UseCases.Server.Execute(path, rest))
	case "--shell":
		path, _ := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		return errExit(c.UseCases.Shell.Execute(path))
	case "--completions":
		return printCompletions(remaining)
	case "--check", "--validate":
		path, _ := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		return errExit(c.UseCases.Validate.Execute(path))
	case "--install-lsp":
		return errExit(c.UseCases.InstallLSP.Execute())
	case "--install-vscode":
		return errExit(c.UseCases.InstallVSCode.Execute())
	case "--import":
		// `perch --import script.sh [-o out.perch]` → best-effort
		// translation. Pure code-gen, no execution.
		if len(remaining) < 1 {
			fmt.Println("Usage: perch --import <script.sh> [-o <out.perch>]")
			return 1
		}
		src := remaining[0]
		out := ""
		for i := 1; i < len(remaining)-1; i++ {
			if remaining[i] == "-o" {
				out = remaining[i+1]
				break
			}
		}
		return errExit(c.UseCases.ImportSh.Execute(src, out))
	}

	path, rest := parseFileFlag(remaining, c.Config.DefaultCommandsFile)

	// If the next arg is --help/-h, route to per-command help instead.
	if hasHelp(rest) {
		return errExit(c.UseCases.CommandHelp.Execute(path, commandName))
	}

	return errExit(c.UseCases.Run.Execute(path, commandName, rest))
}

func hasHelp(args []string) bool {
	for _, a := range args {
		if a == "--help" || a == "-h" {
			return true
		}
	}
	return false
}

func errExit(err error) int {
	if err != nil {
		fmt.Println(err.Error())
		return 1
	}
	return 0
}

func printCompletions(args []string) int {
	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, "usage: perch --completions <bash|zsh|fish>")
		return 1
	}
	switch args[0] {
	case "bash":
		fmt.Print(completionBash)
	case "zsh":
		fmt.Print(completionZsh)
	case "fish":
		fmt.Print(completionFish)
	default:
		fmt.Fprintf(os.Stderr, "unknown shell: %q (use bash|zsh|fish)\n", args[0])
		return 1
	}
	return 0
}

func parseFileFlag(args []string, def string) (string, []string) {
	for i, a := range args {
		if a == "-f" {
			if i+1 >= len(args) {
				fmt.Println("-f flag requires a file path")
				os.Exit(1)
			}
			path := args[i+1]
			out := append([]string{}, args[:i]...)
			if i+2 < len(args) {
				out = append(out, args[i+2:]...)
			}
			return path, out
		}
	}
	return def, args
}

func (c *CLI) printHelp() {
	fmt.Println("perch — a cross-platform command runner")
	fmt.Println()
	fmt.Println("Usage:")
	fmt.Println("  perch <command> [args...]            Run a command from commands.perch")
	fmt.Println("  perch -f <file> <command> [args...]  Run a command from a custom file")
	fmt.Println()
	fmt.Println("Flags:")
	fmt.Println("  --help        Show summary of the commands file")
	fmt.Println("  --version     Print perch version")
	fmt.Println("  --init        Write a starter commands.perch in the current dir")
	fmt.Println("  --build -o X  Bundle commands.perch into a portable binary at X")
	fmt.Println("  --server      Serve commands.perch as an HTTP UI")
	fmt.Println("  --shell       REPL — type one op per line; bindings persist")
    fmt.Println("  --check       Parse and statically check commands.perch; report problems")
	fmt.Println("  --completions SHELL  Print shell completions (bash|zsh|fish)")
	fmt.Println("  --install-lsp        Install the perch-lsp language server (via `go install`)")
	fmt.Println("  --install-vscode     Install perch-lsp + the perch VS Code extension")
	fmt.Println()
	fmt.Println("Per-command help:")
	fmt.Println("  perch <command> --help   Show args, defaults, examples for one command")
	fmt.Println()
	fmt.Println("Options:")
	fmt.Println("  -f <path>     Specify the config file (default: commands.perch)")
}

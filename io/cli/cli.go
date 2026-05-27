// Package cli parses argv into an intent and dispatches to use-cases.
package cli

import (
	_ "embed"
	"fmt"
	"os"
	"strings"
)

//go:embed completions/perch.bash
var completionBash string

//go:embed completions/perch.zsh
var completionZsh string

//go:embed completions/perch.fish
var completionFish string

type RunCommandUseCase interface {
	Execute(configPath, commandName string, args []string) error
	// HasCommand reports whether the named command is declared in the
	// file at configPath. Used by the shebang-default path to fall back
	// cleanly when `main` is missing (no spurious "unknown command"
	// noise before the listing prints).
	HasCommand(configPath, commandName string) bool
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

type ScanUseCase interface {
	Execute(configPath string) error
}

type HelpUseCase interface {
	Execute(topic string, asJSON bool) error
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
	Scan          ScanUseCase
	Help          HelpUseCase
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
	} else if looksLikeScriptPath(args[0]) {
		// Shebang-style invocation. The kernel runs:
		//   `perch /abs/path/to/script.perch ARGS…`
		// when a file starting with `#!/usr/bin/env perch` is executed.
		// We auto-promote the first arg to `-f` and treat the rest as
		// the command + its args. With no command we fall back to a
		// conventional `main` target so `./script.perch` (no args)
		// just runs the default action — same shape Python / bash users
		// expect from `./script`. Empty `commandName` is resolved
		// downstream after parseFileFlag.
		filePath := args[0]
		if len(args) >= 2 {
			commandName = args[1]
			remaining = append([]string{"-f", filePath}, args[2:]...)
		} else {
			commandName = scriptDefaultCommand // sentinel: "try main, then list"
			remaining = []string{"-f", filePath}
		}
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
	case "--scan":
		// `perch --scan FILE` — static security audit. Prints what
		// capabilities the script needs, risk findings, and the
		// recommended-tightest CLI invocation. No execution.
		path, _ := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		return errExit(c.UseCases.Scan.Execute(path))
	case "help":
		// `perch help [TOPIC] [--json]` — auto-generated reference of
		// every CLI flag, subcommand, and concept. Plain text by default;
		// `--json` dumps the full machine-readable catalog for agents.
		topic := ""
		asJSON := false
		for _, a := range remaining {
			if a == "--json" {
				asJSON = true
				continue
			}
			if topic == "" {
				topic = a
			}
		}
		return errExit(c.UseCases.Help.Execute(topic, asJSON))
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

	// Shebang invocation with no command name: try `main` first
	// (Python / bash convention), fall back to listing commands. Use
	// HasCommand for a clean check — running with an unknown name and
	// catching the error would print "unknown command" before our
	// listing, which is noisy.
	if commandName == scriptDefaultCommand {
		if c.UseCases.Run.HasCommand(path, "main") {
			return errExit(c.UseCases.Run.Execute(path, "main", rest))
		}
		return errExit(c.UseCases.List.Execute(path))
	}

	return errExit(c.UseCases.Run.Execute(path, commandName, rest))
}

// scriptDefaultCommand is the sentinel used when `perch ./script.perch`
// is invoked with no command after it. The dispatcher resolves it to
// `main` (if the file declares one) or to a command listing.
const scriptDefaultCommand = "\x00__script_default__"

// looksLikeScriptPath returns true when arg ought to be treated as a
// path to a `.perch` file (shebang-style invocation: `perch script.perch`
// or `./script.perch`). Conservative — we want to avoid mistaking a
// command name like `deploy` for a file.
//
// A token qualifies if (1) it's an existing regular file AND (2) it
// either ends in `.perch` or looks path-shaped (starts with `./`, `../`,
// `/`, `~`, or contains a `/`). That last clause is what makes
// `#!/usr/bin/env perch` work: the kernel passes the absolute path,
// which always contains `/`.
func looksLikeScriptPath(arg string) bool {
	pathShaped := strings.HasSuffix(arg, ".perch") ||
		strings.HasPrefix(arg, "./") ||
		strings.HasPrefix(arg, "../") ||
		strings.HasPrefix(arg, "/") ||
		strings.HasPrefix(arg, "~") ||
		strings.Contains(arg, "/")
	if !pathShaped {
		return false
	}
	info, err := os.Stat(arg)
	if err != nil || info.IsDir() {
		return false
	}
	return true
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

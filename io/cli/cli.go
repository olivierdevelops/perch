// Package cli parses argv into an intent and dispatches to use-cases.
package cli

import (
	_ "embed"
	"fmt"
	"io"
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

// TestUseCase runs every command marked with the `test` modifier in
// the program at configPath. Filter narrows discovery to commands
// whose names contain the substring. Verbose surfaces each test's
// captured output even on pass.
type TestUseCase interface {
	Execute(configPath string, filter string, verbose bool) error
}

// SimulateUseCase analyzes the program against a hypothetical runtime
// environment and reports per-op outcomes. The simulate Args struct
// carries every flag from `perch simulate`.
type SimulateUseCase interface {
	Execute(configPath, commandName string, env SimulateEnv, fixturePath string, w io.Writer) error
}

// SimulateEnv is the CLI-side mirror of usecases/simulate.SimEnv.
// Defined here so the cli package doesn't import usecases.
type SimulateEnv struct {
	OS, Arch     string
	Env          map[string]string
	EnvRestrict  bool
	FsRead       []string
	FsWrite      []string
	Bins         map[string]bool
	Network      []string
	NoShell      bool
	NoSubprocess bool
	NoNetwork    bool
	NoWrite      bool
}

type ExportOpsCatalogUseCase interface {
	Execute(path string) error
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
	Test              TestUseCase
	Simulate          SimulateUseCase
	ExportOpsCatalog  ExportOpsCatalogUseCase
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

	if path, ok := parseExportFlag(os.Args[1:]); ok {
		return errExit(c.UseCases.ExportOpsCatalog.Execute(path))
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
		path, _ := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		return errExit(c.UseCases.Init.Execute(path))
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
	case "test", "--test":
		// `perch test [--filter PAT] [-v|--verbose]` — discover and run
		// every command marked `test`. Sandboxed by default; opt out
		// per-test via test_allow_* modifiers.
		path, rest := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		filter, verbose := parseTestFlags(rest)
		return errExit(c.UseCases.Test.Execute(path, filter, verbose))
	case "simulate", "--simulate":
		// `perch simulate COMMAND [sim flags]` — analyze the program
		// against a hypothetical runtime environment and report per-op
		// outcomes (WILL_RUN / WILL_FAIL / MIGHT_FAIL). No execution.
		path, rest := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
		cmdName, env, fixture := parseSimulateFlags(rest)
		return errExit(c.UseCases.Simulate.Execute(path, cmdName, env, fixture, os.Stdout))
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

// parseExportFlag reports whether argv requests the language catalog export.
// Bare `--export` writes JSON to stdout; `--export=PATH` writes to PATH (`-` is stdout).
func parseExportFlag(args []string) (path string, ok bool) {
	for _, a := range args {
		switch {
		case a == "--export":
			return "-", true
		case strings.HasPrefix(a, "--export="):
			path = a[len("--export="):]
			if path == "" {
				path = "-"
			}
			return path, true
		}
	}
	return "", false
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

// parseTestFlags strips the `test`-specific flags from args. Recognises:
//   --filter PATTERN   substring match against test names
//   --filter=PATTERN
//   -v / --verbose     also surface per-test output on pass
// Unrecognised tokens are left in place (and ignored by the runner).
func parseTestFlags(args []string) (filter string, verbose bool) {
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch {
		case a == "--filter":
			if i+1 < len(args) {
				filter = args[i+1]
				i++
			}
		case len(a) > 9 && a[:9] == "--filter=":
			filter = a[9:]
		case a == "-v", a == "--verbose":
			verbose = true
		}
	}
	return
}

// parseSimulateFlags reads the `perch simulate` argv:
//
//	COMMAND                              — name of the command to simulate (optional; defaults to all)
//	--sim-os OS                          — "darwin" | "linux" | "windows"
//	--sim-arch ARCH                      — "amd64" | "arm64"
//	--sim-env K=v,K=v[,...]              — sim host env (sets the named vars)
//	--sim-env-only                       — together with --sim-env, restrict envs to ONLY listed names
//	--sim-fs-read PATH,PATH,...          — sim allowed read roots
//	--sim-fs-write PATH,PATH,...         — sim allowed write roots
//	--sim-have-bin NAME,NAME,...         — sim binaries present on PATH
//	--sim-allow-host HOST,HOST,...       — sim network allowlist
//	--sim-no-shell                       — sim --no-shell capability flag
//	--sim-no-subprocess                  — sim --no-subprocess
//	--sim-no-network                     — sim --no-network
//	--sim-no-write                       — sim --no-write
//	--sim-file PATH                      — JSON fixture: capabilities + oracles + scenarios
//
// First non-flag arg (or argv after the flag block) is the command name.
// Empty command means "simulate all callable commands."
func parseSimulateFlags(args []string) (cmd string, env SimulateEnv, fixturePath string) {
	pullStringList := func(s string) []string {
		out := []string{}
		for _, p := range strings.Split(s, ",") {
			p = strings.TrimSpace(p)
			if p != "" {
				out = append(out, p)
			}
		}
		return out
	}
	pullKV := func(s string) map[string]string {
		out := map[string]string{}
		for _, pair := range strings.Split(s, ",") {
			pair = strings.TrimSpace(pair)
			if pair == "" {
				continue
			}
			eq := strings.IndexByte(pair, '=')
			if eq > 0 {
				out[pair[:eq]] = pair[eq+1:]
			} else {
				out[pair] = ""
			}
		}
		return out
	}
	pullBinSet := func(s string) map[string]bool {
		out := map[string]bool{}
		for _, b := range pullStringList(s) {
			out[b] = true
		}
		return out
	}
	for i := 0; i < len(args); i++ {
		a := args[i]
		take := func() string {
			if i+1 >= len(args) {
				return ""
			}
			i++
			return args[i]
		}
		switch {
		case strings.HasPrefix(a, "--sim-os="):
			env.OS = strings.TrimPrefix(a, "--sim-os=")
		case a == "--sim-os":
			env.OS = take()
		case strings.HasPrefix(a, "--sim-arch="):
			env.Arch = strings.TrimPrefix(a, "--sim-arch=")
		case a == "--sim-arch":
			env.Arch = take()
		case strings.HasPrefix(a, "--sim-env="):
			env.Env = pullKV(strings.TrimPrefix(a, "--sim-env="))
		case a == "--sim-env":
			env.Env = pullKV(take())
		case a == "--sim-env-only":
			env.EnvRestrict = true
		case strings.HasPrefix(a, "--sim-fs-read="):
			env.FsRead = pullStringList(strings.TrimPrefix(a, "--sim-fs-read="))
		case a == "--sim-fs-read":
			env.FsRead = pullStringList(take())
		case strings.HasPrefix(a, "--sim-fs-write="):
			env.FsWrite = pullStringList(strings.TrimPrefix(a, "--sim-fs-write="))
		case a == "--sim-fs-write":
			env.FsWrite = pullStringList(take())
		case strings.HasPrefix(a, "--sim-have-bin="):
			env.Bins = pullBinSet(strings.TrimPrefix(a, "--sim-have-bin="))
		case a == "--sim-have-bin":
			env.Bins = pullBinSet(take())
		case strings.HasPrefix(a, "--sim-allow-host="):
			env.Network = pullStringList(strings.TrimPrefix(a, "--sim-allow-host="))
		case a == "--sim-allow-host":
			env.Network = pullStringList(take())
		case a == "--sim-no-shell":
			env.NoShell = true
		case a == "--sim-no-subprocess":
			env.NoSubprocess = true
		case a == "--sim-no-network":
			env.NoNetwork = true
		case a == "--sim-no-write":
			env.NoWrite = true
		case strings.HasPrefix(a, "--sim-file="):
			fixturePath = strings.TrimPrefix(a, "--sim-file=")
		case a == "--sim-file":
			fixturePath = take()
		default:
			// First non-flag token = the command name. Subsequent tokens
			// after that are ignored (perch simulate doesn't take
			// command-arg overrides in v1).
			if cmd == "" && !strings.HasPrefix(a, "-") {
				cmd = a
			}
		}
	}
	return
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

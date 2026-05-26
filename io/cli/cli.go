// Package cli parses argv into an intent and dispatches to use-cases.
package cli

import (
	"fmt"
	"os"
)

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

type UseCases struct {
	Run    RunCommandUseCase
	List   ListCommandsUseCase
	Init   InitConfigUseCase
	Build  RunBuildUseCase
	Server RunServerUseCase
	Shell  RunShellUseCase
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
	}

	path, rest := parseFileFlag(remaining, c.Config.DefaultCommandsFile)
	return errExit(c.UseCases.Run.Execute(path, commandName, rest))
}

func errExit(err error) int {
	if err != nil {
		fmt.Println(err.Error())
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
	fmt.Println("  perch <command> [args...]            Run a command from commands.capy")
	fmt.Println("  perch -f <file> <command> [args...]  Run a command from a custom file")
	fmt.Println()
	fmt.Println("Flags:")
	fmt.Println("  --help        Show summary of the commands file")
	fmt.Println("  --version     Print perch version")
	fmt.Println("  --init        Write a starter commands.capy in the current dir")
	fmt.Println("  --build -o X  Bundle commands.capy into a portable binary at X")
	fmt.Println("  --server      Serve commands.capy as an HTTP UI")
	fmt.Println("  --shell       REPL — type one op per line; bindings persist")
	fmt.Println()
	fmt.Println("Options:")
	fmt.Println("  -f <path>     Specify the config file (default: commands.capy)")
}

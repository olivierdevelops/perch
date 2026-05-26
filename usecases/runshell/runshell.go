// Package runshell is the perch REPL: each line is wrapped as a
// throwaway `command __repl__ do <LINE> end`, fed through capy, and
// dispatched by the interpreter. Bindings persist across lines so
// `let x = …` then `print "${x}"` works one line apart.
package runshell

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

type UseCase interface {
	Execute(configPath string) error
}

// LoadStringFn compiles a perch source string into a Program.
type LoadStringFn func(scriptSrc string) (*domain.Program, error)

// LoadFn loads the host program (the user's commands.perch) so REPL
// lines can `run other_cmd`.
type LoadFn func(path string) (*domain.Program, error)

// Handlers is the op-handler registry.
type Handlers map[string]interpreter.Handler

type Impl struct {
	LoadString LoadStringFn
	Load       LoadFn
	Handlers   Handlers
}

const replCommand = "__repl__"

func (i *Impl) Execute(configPath string) error {
	host, err := i.Load(configPath)
	if err != nil {
		// Allow the REPL to run even without a host file.
		host = &domain.Program{Commands: map[string]*domain.Command{}}
	}

	intr := interpreter.New(i.Handlers, host)
	cwd, _ := os.Getwd()
	binds := interpreter.NewBindings(cwd)
	for _, g := range host.Globals.Bindings {
		binds.Set(g.Name, g.Value)
	}

	fmt.Println("perch shell — Ctrl-D to exit. Each line runs as one op.")
	fmt.Println("Available host commands:", commandList(host))

	reader := bufio.NewReader(os.Stdin)
	for {
		fmt.Print("» ")
		line, err := reader.ReadString('\n')
		if err == io.EOF {
			fmt.Println()
			return nil
		}
		if err != nil {
			return err
		}
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		if line == "exit" || line == "quit" {
			return nil
		}
		if err := i.runLine(intr, host, binds, line); err != nil {
			fmt.Fprintln(os.Stderr, "error:", err)
		}
	}
}

func (i *Impl) runLine(intr *interpreter.Interpreter, host *domain.Program, b *interpreter.Bindings, line string) error {
	// If the user typed a bare host-command name, just dispatch it.
	if cmd, ok := host.Commands[strings.SplitN(line, " ", 2)[0]]; ok && !cmd.Modifiers.Private {
		return intr.RunOps(cmd.Ops, b)
	}

	// Otherwise wrap the line as a throwaway command body.
	src := fmt.Sprintf("command %s\n    do\n        %s\n    end\nend\n", replCommand, line)
	tmp, err := i.LoadString(src)
	if err != nil {
		return err
	}
	cmd, ok := tmp.Commands[replCommand]
	if !ok {
		return fmt.Errorf("internal: failed to wrap REPL line")
	}
	return intr.RunOps(cmd.Ops, b)
}

func commandList(p *domain.Program) string {
	if len(p.Commands) == 0 {
		return "(none)"
	}
	names := []string{}
	for n, c := range p.Commands {
		if !c.Modifiers.Private {
			names = append(names, n)
		}
	}
	return strings.Join(names, ", ")
}

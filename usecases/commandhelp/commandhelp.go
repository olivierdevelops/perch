// Package commandhelp prints rich, per-command help (description, args
// table, defaults, env, examples). Triggered by `perch <cmd> --help`.
package commandhelp

import (
	"fmt"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/olivierdevelops/perch/domain"
)

type UseCase interface {
	Execute(configPath, commandName string) error
}

type LoadFn func(path string) (*domain.Program, error)

type Impl struct {
	Load LoadFn
	Out  io.Writer // nil → os.Stdout
}

func (i *Impl) Execute(configPath, commandName string) error {
	out := i.Out
	if out == nil {
		out = os.Stdout
	}
	p, err := i.Load(configPath)
	if err != nil {
		return err
	}
	cmd, ok := p.Commands[commandName]
	if !ok {
		// Treat as unknown — surface suggestions.
		suggestions := Suggest(commandName, visible(p))
		fmt.Fprintf(out, "Unknown command: %q\n", commandName)
		if len(suggestions) > 0 {
			fmt.Fprintf(out, "Did you mean: %s?\n", strings.Join(suggestions, ", "))
		}
		return fmt.Errorf("command not found")
	}
	Render(out, p, cmd, configPath)
	return nil
}

// Render writes the formatted help block for one command.
func Render(out io.Writer, p *domain.Program, cmd *domain.Command, configPath string) {
	progName := p.Name
	if progName == "" {
		progName = "perch"
	}

	header := cmd.Name
	if cmd.Description != "" {
		header = fmt.Sprintf("%s — %s", cmd.Name, cmd.Description)
	}
	fmt.Fprintln(out, header)
	fmt.Fprintln(out, strings.Repeat("─", min(70, len(header))))
	fmt.Fprintln(out)

	// Usage line.
	usage := strings.Builder{}
	fmt.Fprintf(&usage, "%s %s", progName, cmd.Name)
	for _, a := range cmd.Args {
		if a.Index != nil {
			if a.HasDefault || a.Optional {
				fmt.Fprintf(&usage, " [%s]", a.Name)
			} else {
				fmt.Fprintf(&usage, " <%s>", a.Name)
			}
		}
	}
	for _, a := range cmd.Args {
		if a.Index != nil {
			continue
		}
		if a.HasDefault || a.Optional {
			fmt.Fprintf(&usage, " [-%s=…]", a.Name)
		} else {
			fmt.Fprintf(&usage, " -%s=…", a.Name)
		}
	}
	fmt.Fprintln(out, "USAGE")
	fmt.Fprintf(out, "  %s\n", usage.String())
	fmt.Fprintln(out)

	// Args table.
	if len(cmd.Args) > 0 {
		fmt.Fprintln(out, "ARGUMENTS")
		// Sort: positional first by index, then flags by name.
		args := append([]domain.ArgSpec{}, cmd.Args...)
		sort.SliceStable(args, func(a, b int) bool {
			ai, bi := args[a].Index, args[b].Index
			switch {
			case ai != nil && bi != nil:
				return *ai < *bi
			case ai != nil:
				return true
			case bi != nil:
				return false
			default:
				return args[a].Name < args[b].Name
			}
		})
		colWidth := 0
		for _, a := range args {
			if w := len(a.Name) + 2 + len(a.Type); w > colWidth {
				colWidth = w
			}
		}
		for _, a := range args {
			left := fmt.Sprintf("-%s %s", a.Name, a.Type)
			if a.Index != nil {
				left = fmt.Sprintf("<%s> %s", a.Name, a.Type)
			}
			req := ""
			switch {
			case a.HasDefault:
				req = fmt.Sprintf("(default %v)", a.Default)
			case a.Optional:
				req = "(optional)"
			default:
				req = "(required)"
			}
			fmt.Fprintf(out, "  %-*s  %-12s  %s\n", colWidth+2, left, req, a.Description)
		}
		fmt.Fprintln(out)
	}

	// Env vars set by the command.
	if len(cmd.Env) > 0 {
		fmt.Fprintln(out, "ENVIRONMENT (set by this command)")
		keys := make([]string, 0, len(cmd.Env))
		for k := range cmd.Env {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			fmt.Fprintf(out, "  %-20s  %s\n", k, cmd.Env[k])
		}
		fmt.Fprintln(out)
	}

	// Modifiers.
	mods := []string{}
	m := cmd.Modifiers
	if m.Private {
		mods = append(mods, "private")
	}
	if m.Detached {
		mods = append(mods, "detached")
	}
	if m.ProxyArgs {
		mods = append(mods, "proxy_args")
	}
	if len(m.RequireOS) > 0 {
		mods = append(mods, "require_os "+strings.Join(m.RequireOS, "/"))
	}
	if len(m.RequireArch) > 0 {
		mods = append(mods, "require_arch "+strings.Join(m.RequireArch, "/"))
	}
	if m.OnSignal != "" {
		mods = append(mods, "on_signal "+m.OnSignal)
	}
	if m.Dir != "" {
		mods = append(mods, "dir "+m.Dir)
	}
	if len(mods) > 0 {
		fmt.Fprintln(out, "MODIFIERS")
		fmt.Fprintf(out, "  %s\n", strings.Join(mods, ", "))
		fmt.Fprintln(out)
	}

	// Examples.
	if ex := examples(p, cmd, progName); len(ex) > 0 {
		fmt.Fprintln(out, "EXAMPLES")
		for _, e := range ex {
			fmt.Fprintf(out, "  %s\n", e)
		}
		fmt.Fprintln(out)
	}

	if configPath != "" {
		fmt.Fprintln(out, "DEFINED IN")
		fmt.Fprintf(out, "  %s\n", configPath)
	}
}

func examples(p *domain.Program, cmd *domain.Command, progName string) []string {
	out := []string{}
	out = append(out, fmt.Sprintf("%s %s", progName, cmd.Name))
	if len(cmd.Args) > 0 {
		// Show one fully-explicit invocation.
		ex := fmt.Sprintf("%s %s", progName, cmd.Name)
		for _, a := range cmd.Args {
			if a.Index != nil {
				ex += " " + sampleValue(a)
			} else {
				ex += fmt.Sprintf(" -%s=%s", a.Name, sampleValue(a))
			}
		}
		out = append(out, ex)
	}
	return out
}

func sampleValue(a domain.ArgSpec) string {
	if a.HasDefault {
		switch v := a.Default.(type) {
		case string:
			return v
		default:
			return fmt.Sprintf("%v", v)
		}
	}
	switch a.Type {
	case "int":
		return "0"
	case "float":
		return "0.0"
	case "bool":
		return "true"
	}
	return "VALUE"
}

func visible(p *domain.Program) []string {
	out := []string{}
	for n, c := range p.Commands {
		if c != nil && !c.Modifiers.Private {
			out = append(out, n)
		}
	}
	sort.Strings(out)
	return out
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

// ────────────────────────────────────────────────────────────────────────
// Fuzzy matching

// Suggest returns up to 3 candidate command names whose Levenshtein
// distance to `query` is ≤ 3, ordered by closeness.
func Suggest(query string, candidates []string) []string {
	type scored struct {
		name string
		dist int
	}
	var ranked []scored
	for _, c := range candidates {
		d := levenshtein(query, c)
		if d <= 3 {
			ranked = append(ranked, scored{c, d})
		}
	}
	sort.SliceStable(ranked, func(i, j int) bool { return ranked[i].dist < ranked[j].dist })
	out := []string{}
	for i, r := range ranked {
		if i >= 3 {
			break
		}
		out = append(out, r.name)
	}
	return out
}

func levenshtein(a, b string) int {
	la, lb := len(a), len(b)
	if la == 0 {
		return lb
	}
	if lb == 0 {
		return la
	}
	prev := make([]int, lb+1)
	cur := make([]int, lb+1)
	for j := 0; j <= lb; j++ {
		prev[j] = j
	}
	for i := 1; i <= la; i++ {
		cur[0] = i
		for j := 1; j <= lb; j++ {
			cost := 1
			if a[i-1] == b[j-1] {
				cost = 0
			}
			cur[j] = minInt3(cur[j-1]+1, prev[j]+1, prev[j-1]+cost)
		}
		prev, cur = cur, prev
	}
	return prev[lb]
}

func minInt3(a, b, c int) int {
	m := a
	if b < m {
		m = b
	}
	if c < m {
		m = c
	}
	return m
}

// Package importsh translates a bash / sh script into a best-effort
// .perch scaffold. Goal: take a real working script and emit a .perch
// file the user can immediately run (semantics preserved by routing
// most lines through the `shell` op) and then progressively promote
// individual lines to native ops as they like.
//
// What we recognise:
//   - shebang + comments  → comments in the output
//   - `NAME=value` at top level → a bare top-level binding
//   - `function NAME { ... }` or `NAME() { ... }` → `command NAME ... end`
//   - `cmd &` → `shell_detached`
//   - `echo "X"` (when no shell metachars) → `print "X"`
//   - everything else → `shell "..."` preserving the original line
//
// What we DON'T try to do:
//   - parse bash control flow (if/while/case) — preserved as shell lines
//   - typed args (the .sh doesn't carry types; user adds them later)
//   - sub-shells, command substitution rewrites, here-docs — kept verbatim
//
// $VAR / ${VAR} references are preserved because perch's ${name}
// interpolation syntax matches bash's. Top-level executable lines (not
// inside any function) are collected into a default `main` command.
package importsh

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// UseCase is the consumer-owned protocol.
type UseCase interface {
	Execute(srcPath, outPath string) error
}

// Impl is the importsh use case. No dependencies — pure translation.
type Impl struct{}

// Execute reads srcPath, translates, writes outPath (default = srcPath
// with .sh swapped for .perch). Refuses to overwrite an existing output
// unless the user passes the same path back (idempotent re-runs are fine).
func (i *Impl) Execute(srcPath, outPath string) error {
	src, err := os.ReadFile(srcPath)
	if err != nil {
		return fmt.Errorf("read %s: %w", srcPath, err)
	}
	if outPath == "" {
		base := strings.TrimSuffix(srcPath, filepath.Ext(srcPath))
		outPath = base + ".perch"
	}
	if outPath != srcPath {
		if _, err := os.Stat(outPath); err == nil {
			return fmt.Errorf("refusing to overwrite existing file: %s\n(re-run with -o to choose a different output path)", outPath)
		}
	}
	name := strings.TrimSuffix(filepath.Base(srcPath), filepath.Ext(srcPath))
	out := Translate(string(src), name)
	if err := os.WriteFile(outPath, []byte(out), 0644); err != nil {
		return err
	}
	fmt.Printf("✓ imported %s → %s\n", srcPath, outPath)
	fmt.Println()
	fmt.Println("Next:")
	fmt.Printf("  1. perch --check -f %s\n", outPath)
	fmt.Printf("  2. perch -f %s --help\n", outPath)
	fmt.Println("  3. Review the file — most lines became `shell` ops. Promote to")
	fmt.Println("     native ops (cp, mkdir, http_get, …) or `exec BIN args` (the")
	fmt.Println("     shell-free way to run a binary; `shell` is deprecated).")
	fmt.Println("  4. Once nothing needs `shell`, add `--no-shell` to your invocation")
	fmt.Println("     for cross-platform + audit-friendly guarantees.")
	return nil
}

// ─── translation ─────────────────────────────────────────────────────

var (
	reAssign  = regexp.MustCompile(`^([A-Za-z_][A-Za-z_0-9]*)=(.*)$`)
	reFuncA   = regexp.MustCompile(`^function\s+([A-Za-z_][A-Za-z_0-9]*)\s*\(?\s*\)?\s*\{?\s*$`)
	reFuncB   = regexp.MustCompile(`^([A-Za-z_][A-Za-z_0-9]*)\s*\(\)\s*\{?\s*$`)
	reEchoStr = regexp.MustCompile(`^echo\s+"([^"]*)"\s*$`)
	reEchoSQ  = regexp.MustCompile(`^echo\s+'([^']*)'\s*$`)
	reBareVar = regexp.MustCompile(`\$([A-Za-z_][A-Za-z_0-9]*)\b`)
)

// section is one of {bindings, command-NAME, main-command}.
type section struct {
	name string   // "" for top-level / main, otherwise function name
	body []string // perch lines (already translated)
}

// bareVarToBraces rewrites bash `$NAME` → perch `${NAME}` so the
// translated file uses native perch interpolation. Safe because bash
// treats `${NAME}` identically to `$NAME` — every working bash script
// keeps working, AND perch now sees the binding. `$1`, `$@`, `$?` etc.
// are left alone (the regex requires an identifier start).
func bareVarToBraces(s string) string {
	return reBareVar.ReplaceAllString(s, `${${1}}`)
}

// Translate produces the .perch source. Exported so callers can test
// translation without touching the filesystem.
func Translate(bashSrc, programName string) string {
	lines := splitLines(bashSrc)
	prologueComments := []string{} // file-level comments at the top
	globals := []string{}
	commands := []*section{}
	var cur *section // current function body
	mainBody := []string{}
	seenCode := false // any executable line yet?

	for idx := 0; idx < len(lines); idx++ {
		raw := lines[idx]
		trim := strings.TrimSpace(raw)

		// blank line — preserve as gap inside the current body
		if trim == "" {
			if cur != nil {
				cur.body = append(cur.body, "")
			} else if seenCode {
				mainBody = append(mainBody, "")
			}
			continue
		}

		// shebang and full-line comments. Before the first code line
		// they're "file-level" — emitted above `name` as plain comments.
		// After the first code line, they sit with the code that follows.
		if strings.HasPrefix(trim, "#!") || strings.HasPrefix(trim, "#") {
			if !seenCode {
				prologueComments = append(prologueComments, "# "+strings.TrimPrefix(trim, "#"))
				continue
			}
			emit(cur, &mainBody, "        "+trim)
			continue
		}

		// closing brace of a function definition
		if trim == "}" {
			if cur != nil {
				commands = append(commands, cur)
				cur = nil
			}
			continue
		}

		// function NAME { … } or NAME() { … }
		if m := reFuncA.FindStringSubmatch(trim); m != nil {
			if cur != nil {
				commands = append(commands, cur)
			}
			cur = &section{name: m[1]}
			seenCode = true
			continue
		}
		if m := reFuncB.FindStringSubmatch(trim); m != nil {
			if cur != nil {
				commands = append(commands, cur)
			}
			cur = &section{name: m[1]}
			seenCode = true
			continue
		}

		// NAME=VALUE — a bare top-level binding
		if cur == nil {
			if m := reAssign.FindStringSubmatch(trim); m != nil {
				name, val := m[1], m[2]
				globals = append(globals,
					fmt.Sprintf("%s = %s", name, quote(bareVarToBraces(stripQuotes(val)))),
				)
				seenCode = true
				continue
			}
		}

		seenCode = true

		// echo "literal" with no shell metachars → print
		if m := reEchoStr.FindStringSubmatch(trim); m != nil {
			emit(cur, &mainBody, fmt.Sprintf("        print %s", quoteWithInterp(bareVarToBraces(m[1]))))
			continue
		}
		if m := reEchoSQ.FindStringSubmatch(trim); m != nil {
			// single-quoted echo: bash doesn't interpolate inside ''
			emit(cur, &mainBody, fmt.Sprintf("        print %s", literalSingle(m[1])))
			continue
		}

		// `cmd &` (background) → shell_detached
		if strings.HasSuffix(trim, "&") && !strings.HasSuffix(trim, "&&") {
			body := strings.TrimSpace(strings.TrimSuffix(trim, "&"))
			emit(cur, &mainBody, fmt.Sprintf("        shell_detached %s", quoteForShell(bareVarToBraces(body))))
			continue
		}

		// everything else: keep as a shell op, preserving original text
		emit(cur, &mainBody, fmt.Sprintf("        shell %s", quoteForShell(bareVarToBraces(trim))))
	}
	// flush any unclosed function
	if cur != nil {
		commands = append(commands, cur)
	}

	// trailing main command if there were top-level executable lines
	if hasContent(mainBody) {
		commands = append(commands, &section{name: "main", body: collapseBlanks(mainBody)})
	}
	for _, c := range commands {
		c.body = collapseBlanks(c.body)
	}

	return assemble(programName, prologueComments, globals, commands)
}

// collapseBlanks reduces any run of two or more blank lines to a single
// blank — translated bodies otherwise inherit awkward gaps from the
// original .sh structure.
func collapseBlanks(lines []string) []string {
	out := make([]string, 0, len(lines))
	prevBlank := false
	for _, l := range lines {
		isBlank := strings.TrimSpace(l) == ""
		if isBlank && prevBlank {
			continue
		}
		out = append(out, l)
		prevBlank = isBlank
	}
	return out
}

func emit(cur *section, mainBody *[]string, line string) {
	if cur != nil {
		cur.body = append(cur.body, line)
	} else {
		*mainBody = append(*mainBody, line)
	}
}

// assemble glues prologue + bindings + commands into the final .perch text.
func assemble(programName string, prologueComments, globals []string, commands []*section) string {
	var b strings.Builder
	b.WriteString("# Generated by `perch --import` — best-effort translation.\n")
	b.WriteString("# Most lines became `shell` ops to preserve semantics exactly;\n")
	b.WriteString("# promote them to native ops (cp / mkdir / http_get / …) where\n")
	b.WriteString("# it makes sense. `perch --check` will validate as you go.\n")
	for _, c := range prologueComments {
		b.WriteString(c)
		b.WriteString("\n")
	}
	b.WriteString("\n")
	b.WriteString(fmt.Sprintf("name %s\n\n", quote(programName)))

	if len(globals) > 0 {
		// Bare top-level bindings (the `globals … end` block was removed).
		for _, g := range globals {
			b.WriteString(g)
			b.WriteString("\n")
		}
		b.WriteString("\n")
	}

	for _, c := range commands {
		b.WriteString(fmt.Sprintf("command %s\n", c.name))
		b.WriteString(fmt.Sprintf("    description %s\n", quote("(imported from .sh — review me)")))
		b.WriteString("    do\n")
		for _, line := range trimTrailingBlanks(c.body) {
			b.WriteString(line)
			b.WriteString("\n")
		}
		b.WriteString("    end\n")
		b.WriteString("end\n\n")
	}
	return b.String()
}

// ─── quoting helpers ─────────────────────────────────────────────────

// quoteForShell wraps a bash line as a perch string. We pick the
// delimiter that does NOT appear in the content (the same rule we
// document for hand-written perch). Falls back to double quotes with
// raw content even if it has " — capy doesn't interpret escapes so the
// user has to fix that one by hand; we flag with a TODO comment.
func quoteForShell(s string) string {
	// Bash's $VAR is already perch's ${VAR}-friendly; ${VAR} passes through.
	// We don't rewrite $VAR → ${VAR}: bash and perch both interpret ${VAR},
	// and bash treats bare $VAR as ${VAR} too, so a bash-line going through
	// `shell` keeps working. Only when we promote to native ops does
	// $VAR vs ${VAR} matter — out of scope for this translator.
	hasD := strings.Contains(s, `"`)
	hasS := strings.Contains(s, `'`)
	hasB := strings.Contains(s, "`")
	switch {
	case !hasD:
		return `"` + s + `"`
	case !hasS:
		return `'` + s + `'`
	case !hasB:
		return "`" + s + "`"
	default:
		// All three delimiters present — flag for manual review.
		return `"` + strings.ReplaceAll(s, `"`, `"`) + `"   # TODO: fix quoting`
	}
}

// quote is the simpler version for plain literals (file names,
// description text). Always picks double quotes; assumes no " in input.
func quote(s string) string {
	if strings.Contains(s, `"`) {
		return "'" + s + "'"
	}
	return `"` + s + `"`
}

// quoteWithInterp is for echo-derived prints — content already had
// double-quote delimiters in bash, so it almost certainly contains no "
// (bash would have ended the string). $VAR / ${VAR} pass through to
// perch interpolation unchanged.
func quoteWithInterp(s string) string {
	return `"` + s + `"`
}

// literalSingle is for echo 'X' — bash didn't interpolate inside '',
// so we use perch's "raw" single-quote delimiter to preserve that.
func literalSingle(s string) string {
	return `'` + s + `'`
}

// stripQuotes peels a single layer of surrounding quotes from a bash
// rvalue: `"foo"` → `foo`, `'foo'` → `foo`, `foo` → `foo`.
func stripQuotes(s string) string {
	s = strings.TrimSpace(s)
	if len(s) >= 2 {
		first, last := s[0], s[len(s)-1]
		if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
			return s[1 : len(s)-1]
		}
	}
	return s
}

func comment(line string) string {
	// Already starts with #; just preserve. Indent inside a body so it
	// stays inside the command.
	return "        " + line
}

// splitLines preserves line content; joining \ line-continuations is
// out of scope — multi-line backslash continuations get translated as
// separate ops, which the user can recombine if needed.
func splitLines(s string) []string {
	s = strings.ReplaceAll(s, "\r\n", "\n")
	return strings.Split(s, "\n")
}

func hasContent(lines []string) bool {
	for _, l := range lines {
		if strings.TrimSpace(l) != "" && !strings.HasPrefix(strings.TrimSpace(l), "#") {
			return true
		}
	}
	return false
}

func trimTrailingBlanks(lines []string) []string {
	for len(lines) > 0 && strings.TrimSpace(lines[len(lines)-1]) == "" {
		lines = lines[:len(lines)-1]
	}
	return lines
}

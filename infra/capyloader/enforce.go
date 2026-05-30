package capyloader

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
	"sync"

	"github.com/luowensheng/perch/domain"
)

// enforceZeroAmbient is the load-time gate for the zero-ambient-authority
// model (docs/sandboxed-by-design.md). It runs once on the fully-merged root
// program (after imports union their manifests in) and enforces:
//
//  1. A file with no `requires` block is treated as an empty manifest
//     (`requires`/`end` with no entries): Declared==true, pure ops only.
//     An explicit empty block and a missing block are equivalent.
//
//  2. Every subprocess bin is declared. Both the explicit `exec BIN …` op
//     and the implicit bare-invocation form (`docker ps`, folded by the
//     grammar into an exec op) resolve their bin against the declared
//     `bin "…"` set. An undeclared bin is bin_not_declared at LOAD time —
//     promoted from the old runtime-only gate so a typo or a forgotten
//     declaration fails before anything runs. The error carries a
//     did-you-mean hint against the op vocabulary (catching `dcoker`/`prnit`
//     style slips) and the declared bins.
//
// `exec_chain` (a && b) and `pipe` stages carry their child execs in Body, so
// the recursive walk reaches them. Pure ops, control flow, and template/catch
// bodies are all walked.
func enforceZeroAmbient(prog *domain.Program) error {
	if prog == nil {
		return nil
	}
	normalizeRequirements(prog)

	req := prog.Requirements

	var walk func(ops []domain.Op, where string) error
	walk = func(ops []domain.Op, where string) error {
		for _, op := range ops {
			if op.Kind == "exec" && op.Args != nil {
				bin, _ := op.Args["bin"].(string)
				// Skip interpolated bins (`exec ${tool} …`, `exec ${HOME}/bin/x`):
				// their value isn't known until runtime, so the static gate can't
				// resolve them. They defer to the runtime guard, consistent with
				// how requires treats interpolated hosts/paths. Only LITERAL bins
				// are gated at load. A declared alias or path satisfies the gate.
				if bin != "" && !strings.Contains(bin, "${") && !req.BinAllowed(bin) {
					return undeclaredBinError(bin, op, where, req)
				}
			}
			if len(op.Body) > 0 {
				if err := walk(op.Body, where); err != nil {
					return err
				}
			}
		}
		return nil
	}

	for name, cmd := range prog.Commands {
		if cmd == nil {
			continue
		}
		if err := walk(cmd.Ops, "command "+name); err != nil {
			return err
		}
	}
	if prog.Catch != nil {
		if err := walk(prog.Catch.Ops, "catch"); err != nil {
			return err
		}
	}
	for name, tpl := range prog.Templates {
		if tpl == nil {
			continue
		}
		if err := walk(tpl.Ops, "template "+name); err != nil {
			return err
		}
	}
	return nil
}

// normalizeRequirements treats a missing `requires` block as an empty manifest.
// After this, Declared is always true for programs that pass through Load, and
// runtime/path/bin gates apply even when the author omitted the block.
func normalizeRequirements(prog *domain.Program) {
	if prog == nil || prog.Requirements.Declared {
		return
	}
	prog.Requirements.Declared = true
}

// undeclaredBinError builds the bin_not_declared load error with a
// did-you-mean hint. If the bin name is close to a known op keyword, the user
// probably typo'd an op (`prnit` → `print`); otherwise they likely forgot to
// declare a real binary. Both cases suggest the fix.
func undeclaredBinError(bin string, op domain.Op, where string, req domain.Requirements) error {
	var hint string
	if sug := closest(bin, opVocabulary()); sug != "" {
		hint = fmt.Sprintf(" — did you mean the op `%s`?", sug)
	} else {
		declaredNames := make([]string, 0, len(req.Bins)*2)
		for _, b := range req.Bins {
			declaredNames = append(declaredNames, b.Name)
			if b.Alias != "" {
				declaredNames = append(declaredNames, b.Alias)
			}
		}
		if sug := closest(bin, declaredNames); sug != "" {
			hint = fmt.Sprintf(" — did you mean declared bin `%s`?", sug)
		}
	}
	return domain.NewOpError("exec", domain.ErrBinNotDeclared, fmt.Sprintf(
		"%s line %d: `%s` is not a known op and not declared in `requires` "+
			"(add `bin %q` to the requires block to run it)%s",
		where, op.Line, bin, bin, hint))
}

// ── op vocabulary, extracted from the embedded grammar ──────────────────────

var (
	opVocabOnce sync.Once
	opVocab     []string
)

// firstLiteralRe matches a grammar function's FIRST `arg literal "X"` line.
// We over-collect (every leading literal across the file) which is fine: the
// set is only used for typo suggestions, never for correctness.
var firstLiteralRe = regexp.MustCompile(`(?m)^\s*arg literal "([a-zA-Z_][a-zA-Z0-9_]*)"`)

// opVocabulary returns the set of grammar keywords (op + config names),
// parsed once from the embedded lib.capy source. Self-maintaining: adding an
// op to the grammar automatically extends the suggestion vocabulary.
func opVocabulary() []string {
	opVocabOnce.Do(func() {
		set := map[string]bool{}
		for _, m := range firstLiteralRe.FindAllStringSubmatch(librarySource, -1) {
			set[m[1]] = true
		}
		opVocab = make([]string, 0, len(set))
		for k := range set {
			opVocab = append(opVocab, k)
		}
		sort.Strings(opVocab)
	})
	return opVocab
}

var (
	opKindOnce sync.Once
	opKindMap  map[string]bool
)

// opSet returns the canonical built-in op-kind membership set, parsed once from
// the embedded opkinds.txt (generated from ops.BuiltinKinds()).
// resolveBareDispatch uses it to tell a bare built-in op (`glob`, `sha256`,
// `file_size`) from a declared subprocess bin when folding implicit-exec
// captures. Unlike the grammar-derived opVocabulary(), this includes the
// value-returning ops that have no dedicated grammar keyword.
func opSet() map[string]bool {
	opKindOnce.Do(func() {
		opKindMap = map[string]bool{}
		for _, line := range strings.Split(opKindsSource, "\n") {
			if k := strings.TrimSpace(line); k != "" {
				opKindMap[k] = true
			}
		}
	})
	return opKindMap
}

// OpKinds returns the sorted canonical built-in op-kind names the loader knows
// about (from the embedded opkinds.txt). Exposed so a drift test in infra/ops
// can assert it stays in sync with the handler registry.
func OpKinds() []string {
	set := opSet()
	out := make([]string, 0, len(set))
	for k := range set {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// closest returns the nearest candidate within Levenshtein distance 2, or ""
// if none qualifies. Mirrors the fuzzy command-suggestion threshold.
func closest(s string, candidates []string) string {
	best := ""
	bestD := 3
	for _, c := range candidates {
		if d := levenshtein(s, c); d < bestD {
			bestD = d
			best = c
		}
	}
	return best
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
			cur[j] = min3(prev[j]+1, cur[j-1]+1, prev[j-1]+cost)
		}
		prev, cur = cur, prev
	}
	return prev[lb]
}

func min3(a, b, c int) int {
	if b < a {
		a = b
	}
	if c < a {
		a = c
	}
	return a
}

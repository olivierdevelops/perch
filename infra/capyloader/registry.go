package capyloader

import (
	"fmt"
	"sort"
	"strings"

	"github.com/olivierdevelops/perch/domain"
)

// checkNameRegistry enforces the unique-name rule that makes bare-name dispatch
// unambiguous: every dispatchable name — command, template, and declared bin
// alias / bare bin name — must be distinct, and a command or template may not
// shadow a built-in op or grammar keyword. With names globally unique, a bare
// `deploy` / `ensure_dir` / `tool` resolves to exactly one action with no
// precedence rules and no disambiguation keywords needed.
//
// Path-form bins (`./bins/x`) and interpolated bins (`${tool}`) are NOT
// dispatchable bare names, so they don't participate. Catch handlers aren't
// bare-invokable either and are excluded.
func checkNameRegistry(prog *domain.Program) error {
	if prog == nil {
		return nil
	}
	reserved := map[string]bool{}
	for _, k := range opVocabulary() {
		reserved[k] = true
	}

	seen := map[string]string{} // name -> category that claimed it
	claim := func(name, category string) error {
		if name == "" {
			return nil
		}
		if reserved[name] {
			return fmt.Errorf("%s %q collides with a built-in op/keyword named %q — rename the %s",
				category, name, name, category)
		}
		if prev, ok := seen[name]; ok {
			return fmt.Errorf("name %q is declared as both a %s and a %s — names must be unique",
				name, prev, category)
		}
		seen[name] = category
		return nil
	}

	for _, n := range sortedKeys(prog.Commands) {
		if err := claim(n, "command"); err != nil {
			return err
		}
	}
	for _, n := range sortedTemplateKeys(prog.Templates) {
		if err := claim(n, "template"); err != nil {
			return err
		}
	}
	// Bin aliases are dispatchable names → fully claimed. A bare-name bin only
	// needs to not collide with a command/template (which would make `name …`
	// ambiguous); two bins sharing a bare name is merely redundant, not fatal.
	for _, b := range prog.Requirements.Bins {
		if b.Alias != "" {
			if err := claim(b.Alias, "bin alias"); err != nil {
				return err
			}
		}
		if b.Name != "" && !strings.ContainsAny(b.Name, "/\\") && !strings.Contains(b.Name, "${") {
			if prev, ok := seen[b.Name]; ok && prev != "bin" {
				return fmt.Errorf("name %q is declared as both a %s and a bin — names must be unique",
					b.Name, prev)
			}
			if _, ok := seen[b.Name]; !ok {
				seen[b.Name] = "bin"
			}
		}
	}
	return nil
}

func sortedKeys(m map[string]*domain.Command) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	sort.Strings(ks)
	return ks
}

func sortedTemplateKeys(m map[string]*domain.Template) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	sort.Strings(ks)
	return ks
}

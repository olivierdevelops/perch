package interpreter

import "testing"

func TestProvidedVarNamesMatchesCatalog(t *testing.T) {
	names := ProvidedVarNames()
	if len(names) != len(providedVars) {
		t.Fatalf("name set size %d != catalog %d", len(names), len(providedVars))
	}
	for _, v := range providedVars {
		if !names[v.Name] {
			t.Fatalf("missing %q in ProvidedVarNames", v.Name)
		}
	}
}

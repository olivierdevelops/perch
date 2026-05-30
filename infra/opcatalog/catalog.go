package opcatalog

import (
	"encoding/json"
	"sort"

	"github.com/olivierdevelops/perch/infra/interpreter"
)

// Arg is one grammar argument for an op.
type Arg struct {
	Name        string `json:"name"`
	Type        string `json:"type"`
	Description string `json:"description"`
}

// Op is one built-in perch op with docs and capability requirements.
type Op struct {
	Name         string       `json:"name"`
	Description  string       `json:"description"`
	Example      string       `json:"example"`
	Signature    string       `json:"signature,omitempty"`
	Category     string       `json:"category"`
	Args         []Arg        `json:"args,omitempty"`
	Requirements Requirements `json:"requirements"`
}

// Catalog is the full export document.
type Catalog struct {
	Schema  string `json:"schema"`
	Version string `json:"version"`
	Vars    []Var  `json:"vars"`
	Ops     []Op   `json:"ops"`
}

// Var is an auto-bound ${name} available in every command body.
type Var struct {
	Name        string `json:"name"`
	Type        string `json:"type"`
	Category    string `json:"category"`
	Description string `json:"description"`
	Example     string `json:"example"`
}

// Build assembles the catalog for the given handler kinds.
func Build(kinds []string) Catalog {
	grammar := parseCapyArgs()
	ops := make([]Op, 0, len(kinds))
	for _, k := range kinds {
		sig, desc := docFor(k)
		args := grammar[k]
		if args == nil {
			args = []Arg{}
		}
		ops = append(ops, Op{
			Name:         k,
			Description:  desc,
			Example:      exampleForOp(k, args, sig),
			Signature:    sig,
			Category:     categoryFor(k),
			Args:         args,
			Requirements: requirementsFor(k),
		})
	}
	sort.Slice(ops, func(i, j int) bool { return ops[i].Name < ops[j].Name })
	vars := providedVars()
	sort.Slice(vars, func(i, j int) bool { return vars[i].Name < vars[j].Name })
	return Catalog{
		Schema:  "perch.catalog.v1",
		Version: "1",
		Vars:    vars,
		Ops:     ops,
	}
}

func providedVars() []Var {
	src := interpreter.ProvidedVars()
	out := make([]Var, len(src))
	for i, v := range src {
		out[i] = Var{
			Name:        v.Name,
			Type:        v.Type,
			Category:    v.Category,
			Description: v.Description,
			Example:     exampleForVar(v.Name, v.Type),
		}
	}
	return out
}

// MarshalJSON returns indented JSON for the given op kinds.
func MarshalJSON(kinds []string) ([]byte, error) {
	return json.MarshalIndent(Build(kinds), "", "  ")
}

package opcatalog

import (
	"encoding/json"
	"sort"
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
	Signature    string       `json:"signature,omitempty"`
	Category     string       `json:"category"`
	Args         []Arg        `json:"args,omitempty"`
	Requirements Requirements `json:"requirements"`
}

// Catalog is the full export document.
type Catalog struct {
	Schema  string `json:"schema"`
	Version string `json:"version"`
	Ops     []Op   `json:"ops"`
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
			Signature:    sig,
			Category:     categoryFor(k),
			Args:         args,
			Requirements: requirementsFor(k),
		})
	}
	sort.Slice(ops, func(i, j int) bool { return ops[i].Name < ops[j].Name })
	return Catalog{
		Schema:  "perch.ops.v1",
		Version: "1",
		Ops:     ops,
	}
}

// MarshalJSON returns indented JSON for the given op kinds.
func MarshalJSON(kinds []string) ([]byte, error) {
	return json.MarshalIndent(Build(kinds), "", "  ")
}

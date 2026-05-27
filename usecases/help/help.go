// Package help is the auto-generated reference for everything the
// perch CLI exposes: top-level flags, subcommands, and key concepts.
// Two surfaces:
//
//   perch help            top-level index, grouped, human-readable
//   perch help TOPIC      detail on one flag / concept / command
//   perch help --json     full machine-readable dump (agents, tooling)
//
// Topic names match the actual CLI tokens (including the leading "--")
// so a user can copy any flag from a banner or error message and paste
// it into `perch help`.
//
// Op-level help (the ~140 cross-platform built-ins) lives in
// docs/op-reference.md, linked from here. We don't duplicate it: that
// would double the maintenance burden every time an op is added.
package help

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"
)

// Topic is one entry in the help catalog.
type Topic struct {
	Name        string   `json:"name"`         // "--no-shell" / "--check" / "shebang"
	Kind        string   `json:"kind"`         // "flag" / "subcommand" / "concept"
	Group       string   `json:"group"`        // "Security" / "Authoring" / "Execution" / "Concepts"
	Synopsis    string   `json:"synopsis"`     // one-liner
	Usage       string   `json:"usage"`        // syntax skeleton — `perch --foo BAR`
	Description string   `json:"description"`  // 1-3 paragraphs
	Examples    []string `json:"examples"`     // bash snippets
	DocURL      string   `json:"doc_url"`      // canonical doc page
	SeeAlso     []string `json:"see_also"`     // related topic names
}

type UseCase interface {
	Execute(topic string, asJSON bool) error
}

type Impl struct {
	Version string // perch version, surfaced in JSON dumps
}

func (i *Impl) Execute(topic string, asJSON bool) error {
	cat := Catalog()
	if asJSON {
		return printJSON(os.Stdout, i.Version, cat, topic)
	}
	if topic == "" {
		printIndex(os.Stdout, cat)
		return nil
	}
	matches := Find(cat, topic)
	switch len(matches) {
	case 0:
		fmt.Fprintf(os.Stderr, "no help topic matches %q.\n", topic)
		fmt.Fprintln(os.Stderr, "try: perch help                      (top-level index)")
		fmt.Fprintln(os.Stderr, "     perch help --json               (full catalog as JSON)")
		fmt.Fprintln(os.Stderr, "     perch help <fragment>           (fuzzy search)")
		return fmt.Errorf("no match")
	case 1:
		printTopic(os.Stdout, matches[0])
		return nil
	default:
		fmt.Fprintf(os.Stdout, "%d matches for %q:\n\n", len(matches), topic)
		for _, t := range matches {
			fmt.Fprintf(os.Stdout, "  %-22s %s\n", t.Name, t.Synopsis)
		}
		fmt.Fprintln(os.Stdout, "\nRefine: `perch help <exact-name>`")
		return nil
	}
}

// Find returns all topics whose Name contains the query (case-insensitive).
// Used by both the CLI fuzzy match and the "did you mean…" error hints.
func Find(cat []Topic, query string) []Topic {
	q := strings.ToLower(strings.TrimSpace(query))
	if q == "" {
		return nil
	}
	// Exact match first.
	for _, t := range cat {
		if strings.EqualFold(t.Name, q) {
			return []Topic{t}
		}
	}
	// Substring match.
	out := []Topic{}
	for _, t := range cat {
		if strings.Contains(strings.ToLower(t.Name), q) {
			out = append(out, t)
		}
	}
	return out
}

// ─── output ──────────────────────────────────────────────────────────

func printJSON(w io.Writer, version string, cat []Topic, topic string) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	enc.SetEscapeHTML(false)
	if topic == "" {
		return enc.Encode(map[string]any{
			"perch_version": version,
			"doc_root":      "https://luowensheng.github.io/perch/",
			"topics":        cat,
		})
	}
	matches := Find(cat, topic)
	return enc.Encode(map[string]any{
		"query":   topic,
		"matches": matches,
	})
}

func printIndex(w io.Writer, cat []Topic) {
	fmt.Fprintln(w, "perch help — auto-generated reference")
	fmt.Fprintln(w)
	fmt.Fprintln(w, "Usage:")
	fmt.Fprintln(w, "  perch help                 list all topics (this page)")
	fmt.Fprintln(w, "  perch help <TOPIC>         detail on one flag / subcommand / concept")
	fmt.Fprintln(w, "  perch help --json          dump the full catalog (for agents / tooling)")
	fmt.Fprintln(w, "  perch help --json <TOPIC>  one topic as JSON")
	fmt.Fprintln(w)

	groups := map[string][]Topic{}
	order := []string{}
	for _, t := range cat {
		if _, seen := groups[t.Group]; !seen {
			order = append(order, t.Group)
		}
		groups[t.Group] = append(groups[t.Group], t)
	}
	for _, g := range order {
		fmt.Fprintf(w, "%s\n", g)
		for _, t := range groups[g] {
			fmt.Fprintf(w, "  %-26s %s\n", t.Name, t.Synopsis)
		}
		fmt.Fprintln(w)
	}
	fmt.Fprintln(w, "Op reference (~140 built-in ops):")
	fmt.Fprintln(w, "  https://luowensheng.github.io/perch/op-reference/")
	fmt.Fprintln(w)
}

func printTopic(w io.Writer, t Topic) {
	fmt.Fprintf(w, "%s — %s\n", t.Name, t.Synopsis)
	fmt.Fprintln(w, strings.Repeat("─", 60))
	if t.Usage != "" {
		fmt.Fprintf(w, "\nUSAGE\n  %s\n", t.Usage)
	}
	if t.Description != "" {
		fmt.Fprintln(w, "\nDESCRIPTION")
		for _, line := range strings.Split(strings.TrimSpace(t.Description), "\n") {
			fmt.Fprintf(w, "  %s\n", line)
		}
	}
	if len(t.Examples) > 0 {
		fmt.Fprintln(w, "\nEXAMPLES")
		for _, ex := range t.Examples {
			fmt.Fprintf(w, "  $ %s\n", ex)
		}
	}
	if len(t.SeeAlso) > 0 {
		fmt.Fprintf(w, "\nSEE ALSO\n  %s\n", strings.Join(t.SeeAlso, ", "))
	}
	if t.DocURL != "" {
		fmt.Fprintf(w, "\nMORE\n  %s\n", t.DocURL)
	}
	fmt.Fprintln(w)
}

// ─── catalog ─────────────────────────────────────────────────────────

// Catalog returns the full list of topics, sorted within each group.
func Catalog() []Topic {
	cat := append([]Topic{}, catalog...)
	sort.SliceStable(cat, func(i, j int) bool {
		if cat[i].Group != cat[j].Group {
			return groupOrder(cat[i].Group) < groupOrder(cat[j].Group)
		}
		return cat[i].Name < cat[j].Name
	})
	return cat
}

func groupOrder(g string) int {
	for i, name := range []string{
		"Execution", "Authoring", "Security", "Scripts", "Build", "Agents", "Concepts",
	} {
		if g == name {
			return i
		}
	}
	return 99
}

// HelpHint returns a one-line "→ run `perch help X` for details" hint
// that error sites can append to their messages. Exported so the
// interpreter and use cases can call it without re-importing the catalog.
func HelpHint(topic string) string {
	return fmt.Sprintf("→ run `perch help %s` for details", topic)
}

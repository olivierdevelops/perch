package opcatalog

import (
	"regexp"
	"strings"

	"github.com/luowensheng/perch/infra/capyloader"
)

var (
	reFuncHeader   = regexp.MustCompile(`(?m)^function\s+\w+`)
	reKind         = regexp.MustCompile(`"kind":"([^"]+)"`)
	reCapture      = regexp.MustCompile(`(?m)^\s*arg capture (\w+) (\w+)`)
	reFuncName     = regexp.MustCompile(`(?m)^function\s+(\w+)`)
	reFirstLiteral = regexp.MustCompile(`(?m)^\s*arg literal "([^"]+)"`)
)

// parseCapyArgs extracts grammar arg captures grouped by emitted op kind.
// Multiple grammar functions for one kind (e.g. exec / exec_args) are merged.
func parseCapyArgs() map[string][]Arg {
	out := map[string][]Arg{}
	src := capyloader.LibrarySource()
	headers := reFuncHeader.FindAllStringIndex(src, -1)
	for i, h := range headers {
		end := len(src)
		if i+1 < len(headers) {
			end = headers[i+1][0]
		}
		block := src[h[0]:end]
		kindM := reKind.FindStringSubmatch(block)
		if len(kindM) < 2 {
			continue
		}
		kind := kindM[1]
		if strings.HasPrefix(kind, "_") {
			continue
		}
		if fn := reFuncName.FindStringSubmatch(block); len(fn) >= 2 && strings.HasSuffix(fn[1], "_ident") {
			continue
		}
		// Skip `let NAME = op …` grammar variants — they share the op kind but
		// carry capture-specific args (name) that aren't part of the statement form.
		if lit := reFirstLiteral.FindStringSubmatch(block); len(lit) >= 2 && lit[1] == "let" {
			continue
		}
		seen := map[string]bool{}
		for _, a := range out[kind] {
			seen[a.Name] = true
		}
		for _, cap := range reCapture.FindAllStringSubmatch(block, -1) {
			name, typ := cap[1], cap[2]
			if seen[name] {
				continue
			}
			seen[name] = true
			out[kind] = append(out[kind], Arg{
				Name:        name,
				Type:        typ,
				Description: argDescription(kind, name, typ),
			})
		}
	}
	return out
}

// diff-summary.wasm — agent-safe git-diff structured summary.
//
// Reads a unified diff from stdin, returns a JSON blob describing
// what changed:
//
//   {
//     "files_changed":  5,
//     "added_lines":    127,
//     "removed_lines":   42,
//     "by_extension":   { ".go": 3, ".md": 2 },
//     "files":          [{"path":"…","added":12,"removed":5}, …],
//     "risk":           "low|medium|high",
//     "risk_reasons":   ["touches auth", "removes tests"]
//   }
//
// Designed as the agent-tool execution lane for "summarize this PR":
// an LLM agent calls perch via MCP, perch routes to wasm_run, the
// .wasm computes the summary deterministically — no shell, no
// network, no fs access beyond the declared mount.
//
// Build (stdlib only):
//   GOOS=wasip1 GOARCH=wasm go build -o diff-summary.wasm .
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

type fileChange struct {
	Path    string `json:"path"`
	Added   int    `json:"added"`
	Removed int    `json:"removed"`
}

type summary struct {
	FilesChanged int            `json:"files_changed"`
	AddedLines   int            `json:"added_lines"`
	RemovedLines int            `json:"removed_lines"`
	ByExtension  map[string]int `json:"by_extension"`
	Files        []fileChange   `json:"files"`
	Risk         string         `json:"risk"`
	RiskReasons  []string       `json:"risk_reasons,omitempty"`
}

// riskTouchpoints — substrings that, when seen in changed file
// paths, signal elevated risk to a reviewer (or to a downstream
// human approval gate).
var riskTouchpoints = []struct {
	Pattern string
	Reason  string
}{
	{"auth", "touches auth"},
	{"crypt", "touches crypto"},
	{"secret", "touches secrets"},
	{"sandbox", "touches sandbox"},
	{".sql", "modifies schema"},
	{"migration", "modifies migrations"},
	{"main.go", "touches entry point"},
	{".tf", "modifies infrastructure"},
}

func main() {
	src := io.Reader(os.Stdin)
	// Allow a file path as argv[1] for non-stdin use.
	if len(os.Args) >= 2 && os.Args[1] != "" {
		f, err := os.Open(os.Args[1])
		if err != nil {
			fmt.Fprintln(os.Stderr, "open:", err)
			os.Exit(2)
		}
		defer f.Close()
		src = f
	}

	s, err := parse(src)
	if err != nil {
		fmt.Fprintln(os.Stderr, "parse:", err)
		os.Exit(2)
	}

	out, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		fmt.Fprintln(os.Stderr, "encode:", err)
		os.Exit(2)
	}
	fmt.Println(string(out))
}

// parse reads a unified-diff stream and tallies per-file added /
// removed lines. Handles the common formats: `git diff`, `diff -u`,
// and `git format-patch` (it skips email headers).
func parse(r io.Reader) (*summary, error) {
	s := &summary{ByExtension: map[string]int{}}
	scan := bufio.NewScanner(r)
	scan.Buffer(make([]byte, 0, 256*1024), 4*1024*1024)

	var (
		cur     *fileChange
		inDiff  bool
	)
	for scan.Scan() {
		line := scan.Text()

		// "diff --git a/foo b/foo" → start of a new file.
		if strings.HasPrefix(line, "diff --git ") {
			cur = startFile(s, line)
			inDiff = false
			continue
		}
		// "+++ b/path" — alternative format-patch shape.
		if strings.HasPrefix(line, "+++ b/") {
			if cur == nil {
				cur = startFileByPath(s, strings.TrimPrefix(line, "+++ b/"))
			}
			continue
		}
		// "@@ ... @@" — hunk header; lines below are content lines.
		if strings.HasPrefix(line, "@@") {
			inDiff = true
			continue
		}
		if !inDiff || cur == nil {
			continue
		}
		if strings.HasPrefix(line, "+") && !strings.HasPrefix(line, "+++") {
			cur.Added++
			s.AddedLines++
		} else if strings.HasPrefix(line, "-") && !strings.HasPrefix(line, "---") {
			cur.Removed++
			s.RemovedLines++
		}
	}
	if err := scan.Err(); err != nil {
		return nil, err
	}

	s.FilesChanged = len(s.Files)

	// Per-extension breakdown.
	for _, f := range s.Files {
		ext := filepath.Ext(f.Path)
		if ext == "" {
			ext = "(none)"
		}
		s.ByExtension[ext]++
	}

	// Risk heuristics. Cheap, transparent, easy to tune.
	s.Risk, s.RiskReasons = assessRisk(s)

	// Stable order — agents prefer determinism.
	sort.Slice(s.Files, func(i, j int) bool { return s.Files[i].Path < s.Files[j].Path })
	return s, nil
}

func startFile(s *summary, header string) *fileChange {
	// header is "diff --git a/old b/new"; the new path is what we want.
	parts := strings.Fields(header)
	path := ""
	for _, p := range parts {
		if strings.HasPrefix(p, "b/") {
			path = strings.TrimPrefix(p, "b/")
			break
		}
	}
	if path == "" {
		path = "(unknown)"
	}
	return startFileByPath(s, path)
}

func startFileByPath(s *summary, path string) *fileChange {
	for i := range s.Files {
		if s.Files[i].Path == path {
			return &s.Files[i]
		}
	}
	s.Files = append(s.Files, fileChange{Path: path})
	return &s.Files[len(s.Files)-1]
}

func assessRisk(s *summary) (string, []string) {
	var reasons []string
	for _, t := range riskTouchpoints {
		for _, f := range s.Files {
			if strings.Contains(strings.ToLower(f.Path), t.Pattern) {
				reasons = append(reasons, t.Reason)
				break
			}
		}
	}
	// Largeness heuristic.
	totalDelta := s.AddedLines + s.RemovedLines
	if totalDelta > 500 {
		reasons = append(reasons, fmt.Sprintf("large diff (%d ± lines)", totalDelta))
	}

	switch {
	case len(reasons) >= 2 || totalDelta > 1000:
		return "high", reasons
	case len(reasons) == 1 || totalDelta > 200:
		return "medium", reasons
	default:
		return "low", reasons
	}
}

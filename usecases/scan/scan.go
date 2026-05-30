// Package scan audits a perch program for what it actually needs and
// what posture it could be run under safely. Static analysis only —
// no execution. Produces:
//
//   - CAPABILITIES NEEDED: shell? subprocess? network? writes? — with
//     the specific binaries / hosts / paths it touches.
//   - ENV VARS REFERENCED: every ${UPPER_SNAKE} in any string arg.
//   - RISK FINDINGS: a ranked list of patterns worth a human's
//     attention (sudo, shell injection on unvalidated args, catch-→
//     shell passthroughs, downloads + chmod + exec, etc.).
//   - RECOMMENDED INVOCATION: the tightest CLI flag combination that
//     should still let the script run.
//
// The goal is to make reviewing a stranger's .perch file (or your
// own, before shipping it) something you do in ~30 seconds instead
// of ~30 minutes.
package scan

import (
	"fmt"
	"io"
	"os"
	"regexp"
	"sort"
	"strings"

	"github.com/olivierdevelops/perch/domain"
)

type UseCase interface {
	Execute(configPath string) error
}

type LoadFn func(path string) (*domain.Program, error)

type Impl struct {
	Load LoadFn
}

func (i *Impl) Execute(path string) error {
	p, err := i.Load(path)
	if err != nil {
		return err
	}
	r := Analyze(p)
	PrintReport(os.Stdout, p, path, r)
	return nil
}

// Report is the result of analyzing a program. Maps record how many
// times each thing appears, which makes "you have one shell call to git
// and twelve to docker" actionable rather than a binary yes/no.
type Report struct {
	NeedsShell      bool
	ShellBins       map[string]int // bash first-token → count
	HasShellSudo    bool
	HasShellPipe    bool // any shell has |, >, $(, ;, &&, ...
	NeedsSubprocess bool
	SubprocessOps   map[string]int
	NeedsNetwork    bool
	Hosts           map[string]int
	NeedsWrite      bool
	WriteRoots      map[string]int
	EnvVars         map[string]int
	HasProxyArgs    bool
	HasCatch        bool
	CatchForwards   bool // catch contains a shell op (open passthrough)
	Findings        []Finding
}

type Finding struct {
	Severity string // "high" | "med" | "low" | "info"
	Where    string
	Issue    string
	Fix      string
}

// Analyze walks the program and produces a Report. Pure — no IO.
func Analyze(p *domain.Program) Report {
	r := Report{
		ShellBins:     map[string]int{},
		SubprocessOps: map[string]int{},
		Hosts:         map[string]int{},
		WriteRoots:    map[string]int{},
		EnvVars:       map[string]int{},
	}

	if p.Catch != nil {
		r.HasCatch = true
		walkOps(p.Catch.Ops, "catch", &r, true)
	}
	// Sort commands for deterministic finding order.
	names := make([]string, 0, len(p.Commands))
	for n := range p.Commands {
		names = append(names, n)
	}
	sort.Strings(names)
	for _, n := range names {
		c := p.Commands[n]
		if c.Modifiers.ProxyArgs {
			r.HasProxyArgs = true
		}
		walkOps(c.Ops, "command "+n, &r, false)
	}
	// Globals' string values can reference env vars too.
	for _, g := range p.Globals.Bindings {
		if s, ok := g.Value.(string); ok {
			recordEnv(s, &r)
		}
	}
	return r
}

// walkOps is the recursive scanner. inCatch flags catch-block context
// so we can report "catch forwards to shell" as its own finding.
func walkOps(ops []domain.Op, where string, r *Report, inCatch bool) {
	for i, op := range ops {
		opWhere := fmt.Sprintf("%s op #%d (%s)", where, i+1, op.Kind)

		// Harvest env-var references from every string arg.
		for _, v := range op.Args {
			if s, ok := v.(string); ok {
				recordEnv(s, r)
			}
		}

		switch op.Kind {
		case "shell", "shell_output", "shell_detached", "shell_in", "try_shell":
			r.NeedsShell = true
			if inCatch {
				r.CatchForwards = true
			}
			classifyShell(op, opWhere, r)
		case "pkg_install", "pkg_uninstall", "kill_by_name", "process_running",
			"bin_version", "os_version":
			r.NeedsSubprocess = true
			r.SubprocessOps[op.Kind]++
		case "http_get", "http_post", "http_put", "http_delete", "http_status", "download":
			r.NeedsNetwork = true
			recordHost(op, r)
		case "dns_lookup", "port_check", "port_free", "find_free_port",
			"wait_for_url", "wait_for_port", "public_ip", "local_ip",
			"mac_address", "interfaces":
			r.NeedsNetwork = true
		case "write_file", "append_file", "append_line", "ensure_line_in_file",
			"replace_in_file", "backup_file", "cp", "mv", "rm", "mkdir",
			"chmod", "touch", "copy_dir", "ensure_dir", "make_executable",
			"symlink", "link_into_path", "mktemp_file", "mktemp_dir",
			"add_to_path", "tar_create", "tar_extract", "gzip", "ungzip",
			"zip_create", "zip_extract", "bundle_extract", "bundle_dir":
			r.NeedsWrite = true
			recordWrite(op, r)
		}

		// Risk findings checked after classification so the message can
		// reference the same opWhere string.
		checkRisks(op, opWhere, r, inCatch)

		// Recurse into block bodies.
		if len(op.Body) > 0 {
			walkOps(op.Body, where, r, inCatch)
		}
	}
}

// classifyShell inspects a shell op's command-line for binary + risk patterns.
func classifyShell(op domain.Op, opWhere string, r *Report) {
	cmd := firstStringArg(op, "cmd", "_0", "_1")
	if cmd == "" {
		return
	}
	bin := firstShellToken(cmd)
	if bin != "" {
		r.ShellBins[bin]++
	}
	if bin == "sudo" || strings.HasPrefix(strings.TrimSpace(cmd), "sudo ") {
		r.HasShellSudo = true
	}
	for _, ch := range []string{"|", ">", "<", "&", ";", "`", "$("} {
		if strings.Contains(cmd, ch) {
			r.HasShellPipe = true
			break
		}
	}
}

// recordHost extracts host from a URL-like arg.
func recordHost(op domain.Op, r *Report) {
	url := firstStringArg(op, "url", "_0")
	if url == "" {
		return
	}
	h := extractHost(url)
	if h != "" {
		r.Hosts[h]++
	}
}

// recordWrite captures the target path-or-root of a write op.
func recordWrite(op domain.Op, r *Report) {
	p := firstStringArg(op, "path", "dst", "link", "_0", "_1")
	if p == "" {
		return
	}
	r.WriteRoots[pathRoot(p)]++
}

func checkRisks(op domain.Op, where string, r *Report, inCatch bool) {
	switch op.Kind {
	case "shell", "shell_output", "shell_detached", "shell_in", "try_shell":
		cmd := firstStringArg(op, "cmd", "_0", "_1")
		if strings.Contains(cmd, "sudo ") {
			r.Findings = append(r.Findings, Finding{
				Severity: "high",
				Where:    where,
				Issue:    "shell command uses `sudo` (privilege escalation)",
				Fix:      "drop sudo, or guard with `if is_admin ... end` and run perch itself elevated",
			})
		}
		if inCatch && strings.Contains(cmd, "${proxy_args}") {
			r.Findings = append(r.Findings, Finding{
				Severity: "med",
				Where:    where,
				Issue:    "catch forwards `${proxy_args}` to a shell — any input becomes a shell command",
				Fix:      "intentional for `extend an existing tool` patterns; document it and pin `--allow-bin` to the wrapped binary only",
			})
		}
		// Crude shell-injection heuristic: a non-validated ${var} inside
		// a shell string is hard to bound. Flag as low — the user often
		// has done validation we can't see.
		if hasUnvalidatedInterp(cmd) {
			r.Findings = append(r.Findings, Finding{
				Severity: "low",
				Where:    where,
				Issue:    "shell command interpolates `${var}` with no obvious validation",
				Fix:      "add a `regex_match` guard, or promote to a native op (which receives args structurally and can't be shell-injected)",
			})
		}
	case "make_executable":
		r.Findings = append(r.Findings, Finding{
			Severity: "med",
			Where:    where,
			Issue:    "`make_executable` flips the +x bit — downstream `shell` could then run unverified code",
			Fix:      "pair with `verify_sha256` against a known hash before flipping +x",
		})
	}
}

// ─── helpers ─────────────────────────────────────────────────────────

func firstStringArg(op domain.Op, names ...string) string {
	for _, n := range names {
		if v, ok := op.Args[n]; ok {
			if s, ok := v.(string); ok {
				return s
			}
		}
	}
	return ""
}

// firstShellToken returns the basename of the first non-env-assignment
// token. Mirrors the runtime --allow-bin matcher so the suggestion is
// directly actionable.
func firstShellToken(s string) string {
	for _, f := range strings.Fields(s) {
		if strings.Contains(f, "=") && !strings.ContainsAny(f, " \t") {
			continue // FOO=bar style assignment, skip
		}
		// Strip leading ./ or path prefix.
		if idx := strings.LastIndexAny(f, "/\\"); idx >= 0 {
			return f[idx+1:]
		}
		return f
	}
	return ""
}

var (
	reURLHost = regexp.MustCompile(`^[a-z]+://([^/:?#]+)`)
	reEnvVar  = regexp.MustCompile(`\$\{([A-Z][A-Z0-9_]*)\}`)
	reInterp  = regexp.MustCompile(`\$\{[a-z_][a-zA-Z0-9_]*\}`)
)

func extractHost(url string) string {
	if m := reURLHost.FindStringSubmatch(url); m != nil {
		return m[1]
	}
	return ""
}

func recordEnv(s string, r *Report) {
	for _, m := range reEnvVar.FindAllStringSubmatch(s, -1) {
		r.EnvVars[m[1]]++
	}
}

// pathRoot summarises a write target — full absolute paths and
// ${anchor}/sub paths get collapsed so the report doesn't repeat the
// same prefix dozens of times. Best-effort.
func pathRoot(p string) string {
	p = strings.TrimSpace(p)
	if strings.HasPrefix(p, "${") {
		end := strings.Index(p, "}")
		if end > 0 {
			anchor := p[:end+1]
			// Include first path segment after the anchor, if any.
			rest := strings.TrimPrefix(p[end+1:], "/")
			if i := strings.Index(rest, "/"); i > 0 {
				return anchor + "/" + rest[:i] + "/…"
			}
			if rest != "" {
				return anchor + "/" + rest
			}
			return anchor
		}
	}
	if strings.HasPrefix(p, "/") {
		parts := strings.SplitN(p[1:], "/", 3)
		if len(parts) >= 2 {
			return "/" + parts[0] + "/" + parts[1] + "/…"
		}
		return "/" + parts[0]
	}
	return p
}

// hasUnvalidatedInterp is a quick heuristic: any lowercase ${ident} in
// a shell string. Real validation would track guards (regex_match etc.)
// in the surrounding scope — out of scope for this static pass.
func hasUnvalidatedInterp(s string) bool {
	return reInterp.MatchString(s)
}

// ─── pretty-print ────────────────────────────────────────────────────

// PrintReport writes a human-readable scan report to w.
func PrintReport(w io.Writer, p *domain.Program, path string, r Report) {
	fmt.Fprintf(w, "\n  ── %s ──────────────────────────────────────────────\n\n", path)
	fmt.Fprintf(w, "  %d command(s), %s catch, %d binding(s)\n",
		len(p.Commands), boolStr(p.Catch != nil), len(p.Globals.Bindings))
	fmt.Fprintln(w)

	// RISK SCORE — a one-glance summary for non-experts. Computed from
	// declared capabilities + finding severities. See ScoreReport.
	score, reasons := ScoreReport(r)
	fmt.Fprintf(w, "  RISK: %s\n", riskBadge(score))
	for _, why := range reasons {
		fmt.Fprintf(w, "    · %s\n", why)
	}
	fmt.Fprintln(w)

	// CAPABILITIES
	fmt.Fprintln(w, "  CAPABILITIES NEEDED")
	cap(w, "shell", r.NeedsShell, summariseShell(r))
	cap(w, "subprocess", r.NeedsSubprocess, summariseSubprocess(r))
	cap(w, "network", r.NeedsNetwork, summariseNet(r))
	cap(w, "writes", r.NeedsWrite, summariseWrites(r))
	fmt.Fprintln(w)

	// ENV VARS
	if len(r.EnvVars) > 0 {
		fmt.Fprintln(w, "  ENV VARS REFERENCED")
		fmt.Fprintf(w, "    %s\n\n", strings.Join(sortKeys(r.EnvVars), ", "))
	}

	// FINDINGS
	if len(r.Findings) > 0 {
		fmt.Fprintln(w, "  RISK FINDINGS")
		ord := []string{"high", "med", "low", "info"}
		for _, sev := range ord {
			for _, f := range r.Findings {
				if f.Severity != sev {
					continue
				}
				tag := strings.ToUpper(sev)
				fmt.Fprintf(w, "    [%-4s] %s\n", tag, f.Where)
				fmt.Fprintf(w, "             %s\n", f.Issue)
				if f.Fix != "" {
					fmt.Fprintf(w, "         →   %s\n", f.Fix)
				}
				fmt.Fprintln(w)
			}
		}
	} else {
		fmt.Fprintln(w, "  RISK FINDINGS")
		fmt.Fprintln(w, "    (none)")
		fmt.Fprintln(w)
	}

	// RECOMMENDED INVOCATION
	fmt.Fprintln(w, "  RECOMMENDED INVOCATION")
	for _, line := range RecommendedInvocation(path, r) {
		fmt.Fprintf(w, "    %s\n", line)
	}
	fmt.Fprintln(w)
	fmt.Fprintln(w, "  Compared to bare `perch -f "+path+"`:")
	for _, line := range deltaSummary(r) {
		fmt.Fprintf(w, "    %s\n", line)
	}
	fmt.Fprintln(w)
}

// RecommendedInvocation synthesises the tightest CLI command the script
// should still run under. Returned as a list of lines so the printer
// can backslash-wrap nicely.
func RecommendedInvocation(path string, r Report) []string {
	lines := []string{"perch \\"}
	add := func(s string) { lines = append(lines, "  "+s+" \\") }

	// Negative caps the script doesn't need.
	if !r.NeedsShell {
		add("--no-shell")
	}
	if !r.NeedsSubprocess {
		add("--no-subprocess")
	}
	if !r.NeedsNetwork {
		add("--no-network")
	}
	if !r.NeedsWrite {
		add("--no-write")
	}

	// Positive scoping where the script does need a thing.
	if r.NeedsShell && 0 < len(r.ShellBins) && len(r.ShellBins) <= 8 {
		bins := sortKeys(r.ShellBins)
		add("--allow-bin " + strings.Join(bins, ","))
	}
	if r.NeedsShell && !r.HasShellPipe {
		add("--no-shell-metachars")
	}
	if len(r.EnvVars) > 0 && len(r.EnvVars) <= 20 {
		add("--env " + strings.Join(sortKeys(r.EnvVars), ","))
	}

	// Belt-and-braces.
	add("--max-runtime 600")
	// Audit path uses the script's basename so the suggestion is
	// portable across directories (avoid leaking the user's full path).
	scriptName := strings.TrimSuffix(path, ".perch")
	if idx := strings.LastIndexAny(scriptName, "/\\"); idx >= 0 {
		scriptName = scriptName[idx+1:]
	}
	add("--audit /var/log/perch-" + scriptName + ".ndjson")
	lines = append(lines, "  -f "+path)
	return lines
}

func deltaSummary(r Report) []string {
	out := []string{}
	if !r.NeedsShell {
		out = append(out, "- shell subprocess entirely disabled")
	} else if len(r.ShellBins) > 0 && len(r.ShellBins) <= 8 {
		out = append(out, fmt.Sprintf("- shell pinned to {%s}", strings.Join(sortKeys(r.ShellBins), ", ")))
		if !r.HasShellPipe {
			out = append(out, "- no pipes / redirects / `$()` allowed in shell args")
		}
	}
	if !r.NeedsNetwork {
		out = append(out, "- network access entirely disabled")
	}
	if !r.NeedsWrite {
		out = append(out, "- filesystem mutation entirely disabled")
	}
	if !r.NeedsSubprocess {
		out = append(out, "- pkg_install/kill_by_name/etc. disabled")
	}
	if len(r.EnvVars) > 0 {
		out = append(out, fmt.Sprintf("- host env scoped to %d declared var(s) (rest scrubbed from subprocesses)", len(r.EnvVars)))
	}
	out = append(out, "- 10-minute wall-clock cap; structured audit trail")
	return out
}

func cap(w io.Writer, name string, on bool, detail string) {
	marker := "✗"
	if on {
		marker = "✓"
	}
	fmt.Fprintf(w, "    %s %-12s %s\n", marker, name, detail)
}

func summariseShell(r Report) string {
	if !r.NeedsShell {
		return "— add `--no-shell` for free"
	}
	if len(r.ShellBins) == 0 {
		return "(no binary parsed)"
	}
	bins := sortKeys(r.ShellBins)
	tag := ""
	if r.HasShellSudo {
		tag = "  ⚠ uses sudo"
	}
	if r.HasShellPipe {
		tag += "  ⚠ pipes/redirects"
	}
	return fmt.Sprintf("(%d call(s), binaries: %s)%s", sum(r.ShellBins), strings.Join(bins, ", "), tag)
}

func summariseSubprocess(r Report) string {
	if !r.NeedsSubprocess {
		return "— add `--no-subprocess` for free"
	}
	return "(" + strings.Join(sortKeys(r.SubprocessOps), ", ") + ")"
}

func summariseNet(r Report) string {
	if !r.NeedsNetwork {
		return "— add `--no-network` for free"
	}
	if len(r.Hosts) == 0 {
		return "(unknown hosts — only `${var}` URLs found)"
	}
	return fmt.Sprintf("(%d host(s): %s)", len(r.Hosts), strings.Join(sortKeys(r.Hosts), ", "))
}

func summariseWrites(r Report) string {
	if !r.NeedsWrite {
		return "— add `--no-write` for free"
	}
	if len(r.WriteRoots) == 0 {
		return "(paths from `${var}` only)"
	}
	return fmt.Sprintf("(roots: %s)", strings.Join(sortKeys(r.WriteRoots), ", "))
}

func boolStr(b bool) string {
	if b {
		return "1"
	}
	return "0"
}

func sortKeys(m map[string]int) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

func sum(m map[string]int) int {
	t := 0
	for _, v := range m {
		t += v
	}
	return t
}

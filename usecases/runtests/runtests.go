// Package runtests discovers commands marked with the `test` modifier,
// runs each in a sandboxed environment, and reports pass/fail.
//
// A test is a regular perch command — same ops, same templates, same
// execution contexts — that has the `test` modifier set. It passes
// unless any op returns an error (including the `fail "msg"` op and
// every `assert_*` op).
//
// Sandboxing defaults (each may be opted out per-test via modifiers):
//
//   - cwd is set to a fresh ${TMPDIR}/perch-test-${name}-XXXXX/
//   - --no-network is on
//   - --no-shell is on (most tests should not shell out)
//   - --no-subprocess is on
//   - writes are restricted to the temp cwd (via a sandbox CapMask)
//   - the test gets its own --max-runtime of 30s (or test_timeout N)
//
// Each test runs with a fresh interpreter so state doesn't leak
// between tests. The runner aggregates results and exits non-zero if
// any test failed.
package runtests

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/luowensheng/perch/domain"
)

// UseCase is the consumer-owned protocol used by the CLI to drive
// `perch test`.
type UseCase interface {
	Execute(configPath string, filter string, verbose bool) error
}

// LoadFn parses a .perch file into a Program. Same shape as the other
// use cases use (deliberately matches usecases/runcommand.LoadFn so
// the orchestrator can pass capyloader.Load directly).
type LoadFn func(path string) (*domain.Program, error)

// RunTestFn runs a single test command against the supplied program
// with the supplied bindings overrides (cwd, capability flags, timeout).
// Returns the interpreter error (nil = pass). The orchestrator wires
// this against ops.AllHandlers + the runtime sandboxing the test
// runner asks for.
type RunTestFn func(p *domain.Program, name string, sandbox TestSandbox, output io.Writer) error

// TestSandbox captures the per-test environment a test should run in.
// Populated by the runner from the test's modifiers + the runner's
// defaults; consumed by the orchestrator-supplied RunTestFn when it
// builds an interpreter for the test.
type TestSandbox struct {
	Cwd          string        // absolute path; usually a temp dir
	NoShell      bool          // default true; opt out via test_allow_shell
	NoNetwork    bool          // default true; opt out via test_allow_network
	NoSubprocess bool          // default true; opt out via test_allow_subprocess
	NoWrite      bool          // default false; on if the test runs in temp cwd (writes still go through the inner sandbox)
	Timeout      time.Duration // wall-clock cap; 0 = use runner default
}

type Impl struct {
	Load    LoadFn
	RunTest RunTestFn
	// DefaultTimeout caps each test when the test itself doesn't declare
	// `test_timeout N`. The CLI flag `--test-timeout=N` populates this
	// before calling Execute.
	DefaultTimeout time.Duration
	// KeepTempDir, when true, leaves the per-test temp cwd intact so the
	// user can inspect it after a failure. Set by `--keep-tempdir`.
	KeepTempDir bool
}

// Execute discovers every `test`-marked command, runs them in order,
// and writes a summary to stderr. Returns a non-nil error if any test
// failed.
func (i *Impl) Execute(configPath string, filter string, verbose bool) error {
	p, err := i.Load(configPath)
	if err != nil {
		return fmt.Errorf("loading %s: %w", configPath, err)
	}

	names := discover(p, filter)
	if len(names) == 0 {
		if filter != "" {
			fmt.Fprintf(os.Stderr, "no tests matching %q\n", filter)
		} else {
			fmt.Fprintln(os.Stderr, "no tests declared. Add `test` to any command's modifiers to mark it as a test.")
		}
		return nil
	}

	fmt.Fprintln(os.Stderr, "── perch test ─────────────────────────────────")
	defaultTimeout := i.DefaultTimeout
	if defaultTimeout <= 0 {
		defaultTimeout = 30 * time.Second
	}

	results := make([]result, 0, len(names))
	for _, name := range names {
		cmd := p.Commands[name]
		sb, tmp := i.buildSandbox(cmd, defaultTimeout)
		var buf bytes.Buffer
		start := time.Now()
		err := i.RunTest(p, name, sb, &buf)
		dur := time.Since(start)
		results = append(results, result{
			Name:    name,
			Pass:    err == nil,
			Err:     err,
			Dur:     dur,
			Output:  buf.String(),
			TempDir: tmp,
		})
		printOne(os.Stderr, results[len(results)-1], verbose)
		// Clean up the temp dir unless the user wants to inspect it.
		if tmp != "" && !i.KeepTempDir {
			_ = os.RemoveAll(tmp)
		}
	}

	failed := 0
	for _, r := range results {
		if !r.Pass {
			failed++
		}
	}
	passed := len(results) - failed
	totalDur := time.Duration(0)
	for _, r := range results {
		totalDur += r.Dur
	}

	fmt.Fprintln(os.Stderr)
	if failed == 0 {
		fmt.Fprintf(os.Stderr, "%d passed, 0 failed in %s.\n", passed, formatDur(totalDur))
		return nil
	}
	fmt.Fprintf(os.Stderr, "%d passed, %d failed in %s.\n", passed, failed, formatDur(totalDur))
	// Echo failed test details at the end so they're easy to scroll back to.
	fmt.Fprintln(os.Stderr)
	fmt.Fprintln(os.Stderr, "Failures:")
	for _, r := range results {
		if r.Pass {
			continue
		}
		fmt.Fprintf(os.Stderr, "  ✗ %s\n", r.Name)
		fmt.Fprintf(os.Stderr, "    %s\n", r.Err.Error())
		if r.TempDir != "" && i.KeepTempDir {
			fmt.Fprintf(os.Stderr, "    (sandbox kept at %s)\n", r.TempDir)
		}
	}
	return fmt.Errorf("%d test(s) failed", failed)
}

type result struct {
	Name    string
	Pass    bool
	Err     error
	Dur     time.Duration
	Output  string
	TempDir string
}

// discover returns the sorted list of test-marked command names that
// match the (optional) filter substring.
func discover(p *domain.Program, filter string) []string {
	out := []string{}
	for name, cmd := range p.Commands {
		if cmd == nil || !cmd.Modifiers.Test {
			continue
		}
		if filter != "" && !strings.Contains(name, filter) {
			continue
		}
		out = append(out, name)
	}
	sort.Strings(out)
	return out
}

// buildSandbox materialises the per-test environment from the command's
// modifiers + runner defaults. Returns the sandbox plus the temp dir
// path (empty if test_keep_cwd is set), so the runner can clean it up.
func (i *Impl) buildSandbox(cmd *domain.Command, defaultTimeout time.Duration) (TestSandbox, string) {
	m := cmd.Modifiers
	sb := TestSandbox{
		NoShell:      !m.TestAllowShell,
		NoNetwork:    !m.TestAllowNetwork,
		NoSubprocess: !m.TestAllowSubprocess,
		NoWrite:      false, // writes are allowed but the inner CapMask scopes them to the temp cwd
		Timeout:      defaultTimeout,
	}
	if m.TestTimeoutSecs > 0 {
		sb.Timeout = time.Duration(m.TestTimeoutSecs) * time.Second
	}
	var tmp string
	if !m.TestKeepCwd {
		base, err := os.MkdirTemp("", fmt.Sprintf("perch-test-%s-*", sanitizeName(cmd.Name)))
		if err == nil {
			tmp = base
			sb.Cwd = base
		}
	}
	if sb.Cwd == "" {
		// Fall back to cwd if MkdirTemp failed or test_keep_cwd was set.
		cwd, _ := os.Getwd()
		sb.Cwd = cwd
	}
	return sb, tmp
}

// sanitizeName makes a command name safe for use in a temp-dir name.
// Mostly to handle namespaced names like `aws.upload` which would
// otherwise become subdirectories.
func sanitizeName(s string) string {
	out := make([]byte, 0, len(s))
	for i := 0; i < len(s); i++ {
		c := s[i]
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_' || c == '-' {
			out = append(out, c)
		} else {
			out = append(out, '_')
		}
	}
	return string(out)
}

// printOne writes the one-line outcome for a single test.
func printOne(w io.Writer, r result, verbose bool) {
	if r.Pass {
		fmt.Fprintf(w, "✓ %-40s (%s)\n", r.Name, formatDur(r.Dur))
		if verbose && r.Output != "" {
			fmt.Fprintln(w, indent(r.Output, "    "))
		}
		return
	}
	fmt.Fprintf(w, "✗ %-40s (%s)\n", r.Name, formatDur(r.Dur))
	fmt.Fprintf(w, "    %s\n", r.Err.Error())
	if r.Output != "" {
		fmt.Fprintln(w, indent(r.Output, "    "))
	}
}

func indent(s, pad string) string {
	lines := strings.Split(strings.TrimRight(s, "\n"), "\n")
	for i, l := range lines {
		lines[i] = pad + l
	}
	return strings.Join(lines, "\n")
}

// formatDur mirrors infra/report's helper for visual consistency.
func formatDur(d time.Duration) string {
	switch {
	case d < time.Millisecond:
		return fmt.Sprintf("%dµs", d.Microseconds())
	case d < time.Second:
		return fmt.Sprintf("%dms", d.Milliseconds())
	case d < time.Minute:
		return fmt.Sprintf("%.2fs", d.Seconds())
	default:
		return fmt.Sprintf("%dm%02ds", int(d.Minutes()), int(d.Seconds())%60)
	}
}

// scriptDir is exposed so the orchestrator (which knows the source
// .perch path) can resolve relative test resources. Currently unused
// by Impl directly; kept for downstream consumers.
func scriptDir(p *domain.Program) string {
	if p.ScriptPath == "" {
		return ""
	}
	return filepath.Dir(p.ScriptPath)
}

// avoid "unused" warnings on scriptDir (kept for future tutorial use)
var _ = scriptDir

// File-declared requirements enforcement.
//
// When a `requires ... end` block is declared at file scope, the
// interpreter switches to a strict-manifest model: any shell binary,
// HTTP host, or env-var the program touches must be enumerated in the
// block. Undeclared use surfaces as bin_not_declared / host_not_declared
// / env_not_declared so static analysis (and the runtime) can refuse
// the operation.
//
// Three responsibilities live here:
//
//  1. Preflight — runs once per command invocation BEFORE any op fires.
//     Verifies every required bin is on PATH (and, when a hash is pinned,
//     that its bytes match), every required env var is set, and the host
//     OS/arch is on the declared list. NO version checking: that would
//     require executing the binary before the sandbox exists, and a
//     trojaned binary lies about its version. Hash pinning needs no
//     execution and pins the exact artifact.
//
//  2. Per-op enforcement — `CheckShellBinDeclared`, `CheckHostDeclared`,
//     and `CheckEnvDeclared` consult the parsed Requirements. Called
//     inline from checkShell / runHTTP / get_env.
//
//  3. Helpers used by --check / simulate to pre-validate without running.
package ops

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

// Preflight runs all `requires`-block checks. Returns the FIRST failure
// as an OpError tagged requirement_unmet (with detail naming the missing
// piece). Caller is the interpreter, just after `parseArgs` succeeds.
//
// No-op when prog.Requirements.Declared is false.
func Preflight(i *interpreter.Interpreter, prog *domain.Program) error {
	if prog == nil || !prog.Requirements.Declared {
		return nil
	}
	r := prog.Requirements

	// OS / arch first — fastest, no IO.
	if len(r.OS) > 0 && !inList(r.OS, runtime.GOOS) && !matchesUnix(r.OS) {
		return domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("host OS %q not in declared list: %s", runtime.GOOS, strings.Join(r.OS, ", "))).
			WithDetail(runtime.GOOS)
	}
	if len(r.Arch) > 0 && !inList(r.Arch, runtime.GOARCH) {
		return domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("host arch %q not in declared list: %s", runtime.GOARCH, strings.Join(r.Arch, ", "))).
			WithDetail(runtime.GOARCH)
	}

	// Required env vars.
	for _, e := range r.Envs {
		if e.Optional {
			continue
		}
		if os.Getenv(e.Name) == "" {
			return domain.NewOpError("requires", domain.ErrRequirementUnmet,
				fmt.Sprintf("required env var %q is not set", e.Name)).WithDetail(e.Name)
		}
	}

	// Required bins (existence + optional hash pin — no execution).
	for _, bin := range r.Bins {
		path, err := exec.LookPath(bin.Name)
		if err != nil {
			if bin.Optional {
				continue
			}
			return domain.NewOpError("requires", domain.ErrRequirementUnmet,
				fmt.Sprintf("required bin %q not found on PATH", bin.Name)).WithDetail(bin.Name)
		}
		// Hash pin. Inline `hash "..."` and external `hash_file "PATH"`
		// both feed into the same comparison; hash_file is resolved here
		// so the hash check itself stays format-agnostic. This is a
		// read-only check — perch reads the binary's bytes, never runs it.
		if bin.HashFile != "" {
			loaded, err := loadHashFile(prog, bin.HashFile)
			if err != nil {
				if bin.Optional {
					continue
				}
				return err
			}
			if bin.Hash == "" {
				bin.Hash = loaded
			} else if normalizeHash(bin.Hash) != normalizeHash(loaded) {
				return domain.NewOpError("requires", domain.ErrRequirementUnmet,
					fmt.Sprintf("bin %q: inline hash and hash_file disagree", bin.Name)).
					WithDetail(bin.Name)
			}
		}
		if bin.Hash != "" {
			if err := checkBinHash(bin, path); err != nil {
				if bin.Optional {
					continue
				}
				return err
			}
		}
	}
	return nil
}

// checkBinHash hashes the resolved binary file and compares it against
// the declared pin. Format is "ALGO:HEX". Only sha256 is supported today
// — extending to sha512/blake2b is a one-switch addition when needed.
func checkBinHash(bin domain.BinReq, path string) error {
	algo, want, ok := strings.Cut(bin.Hash, ":")
	if !ok {
		return domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("bin %q: hash must be ALGO:HEX (got %q)", bin.Name, bin.Hash)).
			WithDetail(bin.Name)
	}
	algo = strings.ToLower(strings.TrimSpace(algo))
	want = strings.ToLower(strings.TrimSpace(want))
	if algo != "sha256" {
		return domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("bin %q: unsupported hash algorithm %q (only sha256)", bin.Name, algo)).
			WithDetail(bin.Name)
	}
	f, err := os.Open(path)
	if err != nil {
		return domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("bin %q: cannot open %s for hashing: %v", bin.Name, path, err)).
			WithDetail(bin.Name)
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("bin %q: read error while hashing: %v", bin.Name, err)).
			WithDetail(bin.Name)
	}
	got := hex.EncodeToString(h.Sum(nil))
	if got != want {
		return domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("bin %q: hash mismatch\n  expected sha256:%s\n  got      sha256:%s\n  path     %s",
				bin.Name, want, got, path)).
			WithDetail(bin.Name + " sha256:" + got)
	}
	return nil
}

// loadHashFile resolves bin.HashFile to a hash string. Two path forms:
//
//   - "bundle:RELPATH"  — read from the embedded bundle archive (works
//     against the fat binary at runtime; no on-disk extraction).
//   - regular path      — read from the filesystem, resolved relative
//     to the .perch script directory (so checksums files travel with
//     the source).
//
// The file's first non-empty whitespace-delimited token is taken as the
// hash. Standard shasum/sha256sum output lines look like `<hex>  <path>`
// — only the leading token is used, so those files work unchanged.
func loadHashFile(prog *domain.Program, ref string) (string, error) {
	var raw []byte
	if strings.HasPrefix(ref, "bundle:") {
		entry := strings.TrimPrefix(ref, "bundle:")
		data, ok, err := BundleReadFile(entry)
		if err != nil {
			return "", domain.NewOpError("requires", domain.ErrRequirementUnmet,
				fmt.Sprintf("hash_file: read bundle entry %q: %v", entry, err)).WithDetail(ref)
		}
		if !ok {
			return "", domain.NewOpError("requires", domain.ErrRequirementUnmet,
				fmt.Sprintf("hash_file: bundle entry %q not found", entry)).WithDetail(ref)
		}
		raw = data
	} else {
		path := ref
		if !filepath.IsAbs(path) && prog != nil && prog.ScriptPath != "" {
			path = filepath.Join(filepath.Dir(prog.ScriptPath), path)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return "", domain.NewOpError("requires", domain.ErrRequirementUnmet,
				fmt.Sprintf("hash_file: read %s: %v", path, err)).WithDetail(ref)
		}
		raw = data
	}
	// Take the first whitespace-delimited token. Supports plain `<hex>`
	// files, `sha256:<hex>` files, and shasum-format `<hex>  filename`.
	tok := strings.TrimSpace(string(raw))
	if i := strings.IndexAny(tok, " \t\n\r"); i >= 0 {
		tok = tok[:i]
	}
	if tok == "" {
		return "", domain.NewOpError("requires", domain.ErrRequirementUnmet,
			fmt.Sprintf("hash_file: %q is empty", ref)).WithDetail(ref)
	}
	if !strings.Contains(tok, ":") {
		// Bare hex — default to sha256.
		tok = "sha256:" + tok
	}
	return tok, nil
}

// normalizeHash lowercases the hash so case-insensitive comparison
// between inline `hash` and `hash_file` works.
func normalizeHash(s string) string {
	return strings.ToLower(strings.TrimSpace(s))
}

// CheckShellBinDeclared verifies that `bin` (the first token of a shell
// command) is in the program's declared bin list. No-op when no
// requires block is declared. Returns *OpError with ErrBinNotDeclared
// on miss.
func CheckShellBinDeclared(i *interpreter.Interpreter, raw string) error {
	if i == nil || i.Program == nil || !i.Program.Requirements.Declared {
		return nil
	}
	bin := firstShellToken(raw)
	if bin == "" {
		return nil
	}
	for _, b := range i.Program.Requirements.Bins {
		if b.Name == bin {
			return nil
		}
	}
	// Also allow shell built-ins (the user can't really declare these).
	if isShellBuiltin(bin) {
		return nil
	}
	return domain.NewOpError("shell", domain.ErrBinNotDeclared,
		fmt.Sprintf("bin %q is not declared in `requires`", bin)).
		WithDetail(bin)
}

func isShellBuiltin(name string) bool {
	switch name {
	case "echo", "cd", "true", "false", "pwd", ":", "set", "unset", "export", "test", "[":
		return true
	}
	return false
}

// CheckHostDeclared verifies `host` matches one of the declared hosts
// (exact match, or matches a `*.suffix` wildcard).
func CheckHostDeclared(i *interpreter.Interpreter, host string) error {
	if i == nil || i.Program == nil || !i.Program.Requirements.Declared {
		return nil
	}
	host = strings.ToLower(host)
	for _, h := range i.Program.Requirements.Hosts {
		want := strings.ToLower(h.Name)
		if want == host {
			return nil
		}
		if strings.HasPrefix(want, "*.") && strings.HasSuffix(host, want[1:]) {
			return nil
		}
	}
	return domain.NewOpError("http", domain.ErrHostNotDeclared,
		fmt.Sprintf("host %q is not declared in `requires`", host)).
		WithDetail(host)
}

// CheckEnvDeclared verifies that `name` was listed under `requires env`.
func CheckEnvDeclared(i *interpreter.Interpreter, name string) error {
	if i == nil || i.Program == nil || !i.Program.Requirements.Declared {
		return nil
	}
	for _, e := range i.Program.Requirements.Envs {
		if e.Name == name {
			return nil
		}
	}
	return domain.NewOpError("get_env", domain.ErrEnvNotDeclared,
		fmt.Sprintf("env var %q is not declared in `requires`", name)).
		WithDetail(name)
}

// CheckNetDeclared gates network ops that don't name a specific host
// (public_ip, local_ip, interfaces, mac_address, port_free, find_free_port).
// When a requires block is present, the program must have declared at least
// one `host` — i.e. declared that it touches the network at all — or the op
// is refused. No-op when no requires block is declared.
func CheckNetDeclared(i *interpreter.Interpreter) error {
	if i == nil || i.Program == nil || !i.Program.Requirements.Declared {
		return nil
	}
	if len(i.Program.Requirements.Hosts) > 0 {
		return nil
	}
	return domain.NewOpError("net", domain.ErrHostNotDeclared,
		"network access is not declared in `requires` (declare a `host`)")
}

// CheckSubprocessBin gates ops that spawn an external program WITHOUT going
// through the `shell` op (pkg_install, bin_version, os_version, kill_by_name,
// …). The spawned binary must be in the declared bin list, exactly like a
// `shell` first token. No-op when no requires block is declared.
func CheckSubprocessBin(i *interpreter.Interpreter, name string) error {
	if i == nil || i.Program == nil || !i.Program.Requirements.Declared {
		return nil
	}
	if name == "" {
		return nil
	}
	base := name
	if idx := strings.LastIndexAny(base, "/\\"); idx >= 0 {
		base = base[idx+1:]
	}
	for _, b := range i.Program.Requirements.Bins {
		if b.Name == base {
			return nil
		}
	}
	return domain.NewOpError("subprocess", domain.ErrBinNotDeclared,
		fmt.Sprintf("subprocess bin %q is not declared in `requires`", base)).
		WithDetail(base)
}

// checkExecBin gates the `exec` op: when a `requires` block is present, the
// binary must be a declared `bin "…"` or the op fails bin_not_declared. This
// is the same control as `shell`'s first-token check, but exec names the bin
// structurally (no command-line parsing), so the check is exact — there is
// no string to mis-tokenize. No-op when no requires block is declared.
func checkExecBin(i *interpreter.Interpreter, name string) error {
	if i == nil || i.Program == nil || !i.Program.Requirements.Declared {
		return nil
	}
	if name == "" {
		return nil
	}
	base := name
	if idx := strings.LastIndexAny(base, "/\\"); idx >= 0 {
		base = base[idx+1:]
	}
	for _, b := range i.Program.Requirements.Bins {
		if b.Name == base {
			return nil
		}
	}
	return domain.NewOpError("exec", domain.ErrBinNotDeclared,
		fmt.Sprintf("bin %q is not declared in `requires`", base)).
		WithDetail(base)
}

// hostOfURL extracts the hostname from a URL for host-allowlist checks.
func hostOfURL(u string) string {
	s := u
	if idx := strings.Index(s, "://"); idx >= 0 {
		s = s[idx+3:]
	}
	if idx := strings.IndexAny(s, "/?#"); idx >= 0 {
		s = s[:idx]
	}
	if idx := strings.LastIndexByte(s, '@'); idx >= 0 {
		s = s[idx+1:]
	}
	if idx := strings.LastIndexByte(s, ':'); idx >= 0 {
		s = s[:idx]
	}
	return s
}

// firstShellToken returns the basename of the first non-assignment
// token of a shell command. Mirrors the logic in checkShell's
// allow-list parsing.
func firstShellToken(raw string) string {
	for _, f := range strings.Fields(raw) {
		// Skip `KEY=VAL` prefixes like `GOOS=linux`.
		if strings.Contains(f, "=") && !strings.ContainsAny(f, " \t") {
			eq := strings.IndexByte(f, '=')
			before := f[:eq]
			isAllUpperSnake := before != "" && before == strings.ToUpper(before)
			if isAllUpperSnake {
				continue
			}
		}
		base := f
		if idx := strings.LastIndexAny(base, "/\\"); idx >= 0 {
			base = base[idx+1:]
		}
		return base
	}
	return ""
}

func inList(xs []string, want string) bool {
	for _, x := range xs {
		if x == want {
			return true
		}
	}
	return false
}

// matchesUnix returns true when the declared OS list contains "unix"
// and the host is one of the unixes.
func matchesUnix(list []string) bool {
	if !inList(list, "unix") {
		return false
	}
	switch runtime.GOOS {
	case "darwin", "linux", "freebsd", "openbsd", "netbsd":
		return true
	}
	return false
}

// ─── Filesystem path gating (read / write roots) ──────────────────────────
//
// The filesystem is an external resource like bins / hosts / env, so when a
// `requires` block is present every filesystem op's path must fall inside a
// declared `read` / `write` root — otherwise it errors with read_not_declared
// / write_not_declared. A write root implies read on the same tree.

// fsPathSpec classifies a filesystem op: which arg keys hold write paths and
// which hold read paths. Keys mirror the handlers in files.go.
type fsPathSpec struct {
	writeKeys [][]string // each inner slice is an argString fallback chain
	readKeys  [][]string
}

var fsPathOps = map[string]fsPathSpec{
	// write-only (single path)
	"mkdir":               {writeKeys: [][]string{{"path", "_0"}}},
	"rm":                  {writeKeys: [][]string{{"path", "_0"}}},
	"touch":               {writeKeys: [][]string{{"path", "_0"}}},
	"chmod":               {writeKeys: [][]string{{"path", "_0"}}},
	"write_file":          {writeKeys: [][]string{{"path", "_0"}}},
	"append_file":         {writeKeys: [][]string{{"path", "_0"}}},
	"append_line":         {writeKeys: [][]string{{"path", "_0"}}},
	"ensure_dir":          {writeKeys: [][]string{{"path", "_0"}}},
	"make_executable":     {writeKeys: [][]string{{"path", "_0"}}},
	"ensure_line_in_file": {writeKeys: [][]string{{"path", "_0"}}},
	"replace_in_file":     {writeKeys: [][]string{{"path", "_0"}}},
	"symlink":             {writeKeys: [][]string{{"link", "_1"}}},
	// read+write (src read, dst write)
	"cp":          {readKeys: [][]string{{"src", "_0"}}, writeKeys: [][]string{{"dst", "_1"}}},
	"mv":          {readKeys: [][]string{{"src", "_0"}}, writeKeys: [][]string{{"dst", "_1"}}},
	"copy_dir":    {readKeys: [][]string{{"src", "_0"}}, writeKeys: [][]string{{"dst", "_1"}}},
	"backup_file": {readKeys: [][]string{{"path", "_0"}}, writeKeys: [][]string{{"path", "_0"}}},
	// read-only
	"read_file":   {readKeys: [][]string{{"path", "_0"}}},
	"exists":      {readKeys: [][]string{{"path", "_0"}}},
	"is_dir":      {readKeys: [][]string{{"path", "_0"}}},
	"is_file":     {readKeys: [][]string{{"path", "_0"}}},
	"file_size":   {readKeys: [][]string{{"path", "_0"}}},
	"list_dir":    {readKeys: [][]string{{"path", "_0"}}},
	"read_link":   {readKeys: [][]string{{"path", "_0"}}},
	"sha256_file": {readKeys: [][]string{{"path", "_0"}}},
	"md5_file":    {readKeys: [][]string{{"path", "_0"}}},
	"glob":        {readKeys: [][]string{{"pattern", "_0"}}},
	// archive / compression — read src, write dst
	"tar_create":  {readKeys: [][]string{{"src"}}, writeKeys: [][]string{{"dst"}}},
	"tar_extract": {readKeys: [][]string{{"src"}}, writeKeys: [][]string{{"dst"}}},
	"zip_create":  {readKeys: [][]string{{"src"}}, writeKeys: [][]string{{"dst"}}},
	"zip_extract": {readKeys: [][]string{{"src"}}, writeKeys: [][]string{{"dst"}}},
	"gzip":        {readKeys: [][]string{{"src"}}, writeKeys: [][]string{{"dst"}}},
	"ungzip":      {readKeys: [][]string{{"src"}}, writeKeys: [][]string{{"dst"}}},
	// bundle extraction writes the destination tree
	"bundle_extract": {writeKeys: [][]string{{"dst", "_0"}}},
	// sha-of-file ops read the file
	"sha1_file":     {readKeys: [][]string{{"path", "_0"}}},
	"verify_sha256": {readKeys: [][]string{{"path", "_0"}}},
}

// ApplyRequiresPathGating wraps every filesystem op so that, when a
// `requires` block is declared, its read/write paths are checked against
// the declared roots before the handler runs. No-op for programs without a
// requires block (legacy behavior preserved).
func ApplyRequiresPathGating(m map[string]interpreter.Handler) {
	for kind, spec := range fsPathOps {
		h, ok := m[kind]
		if !ok {
			continue
		}
		spec := spec
		inner := h
		m[kind] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
			if i != nil && i.Program != nil && i.Program.Requirements.Declared {
				for _, keys := range spec.writeKeys {
					if p := argString(args, keys...); p != "" {
						if err := checkPathDeclared(i, b, p, true); err != nil {
							return nil, err
						}
					}
				}
				for _, keys := range spec.readKeys {
					if p := argString(args, keys...); p != "" {
						if err := checkPathDeclared(i, b, p, false); err != nil {
							return nil, err
						}
					}
				}
			}
			return inner(i, b, args)
		}
	}
}

// checkPathDeclared verifies `raw` is inside a declared root. Write paths
// must be within a WriteRoot; read paths within a ReadRoot OR a WriteRoot
// (you may read what you may write). Paths are cleaned + made absolute
// (relative to b.Cwd) before comparison.
func checkPathDeclared(i *interpreter.Interpreter, b *interpreter.Bindings, raw string, isWrite bool) error {
	if i == nil || i.Program == nil || !i.Program.Requirements.Declared || raw == "" {
		return nil
	}
	r := i.Program.Requirements
	abs := absUnder(raw, b.Cwd)
	writeRoots := expandRoots(r.WriteRoots, b)
	if isWrite {
		if pathWithinAny(abs, writeRoots, b.Cwd) {
			return nil
		}
		return domain.NewOpError("fs", domain.ErrWriteNotDeclared,
			fmt.Sprintf("write to %q is outside every declared `write` root in `requires`", raw)).
			WithDetail(raw)
	}
	if pathWithinAny(abs, expandRoots(r.ReadRoots, b), b.Cwd) || pathWithinAny(abs, writeRoots, b.Cwd) {
		return nil
	}
	return domain.NewOpError("fs", domain.ErrReadNotDeclared,
		fmt.Sprintf("read of %q is outside every declared `read` root in `requires`", raw)).
		WithDetail(raw)
}

// expandRoots interpolates ${…} placeholders (script_dir, temp_dir, home,
// declared globals, …) in each declared root against the live bindings, so a
// manifest can scope writes to a runtime-resolved location like
// "${script_dir}/.ignore" — the documented behavior (requires.md: roots are
// matched "at runtime after ${…} interpolation"). A root that fails to
// interpolate (unknown placeholder) is dropped: it can never match a concrete
// path, and surfacing the gate error (read/write_not_declared) is clearer than
// an opaque interpolation failure from inside the path check.
func expandRoots(roots []string, b *interpreter.Bindings) []string {
	if len(roots) == 0 {
		return roots
	}
	out := make([]string, 0, len(roots))
	for _, root := range roots {
		if expanded, err := interpreter.Interpolate(root, b); err == nil {
			out = append(out, expanded)
		}
	}
	return out
}

// absUnder cleans p and makes it absolute, resolving relatives under cwd.
func absUnder(p, cwd string) string {
	if !filepath.IsAbs(p) {
		p = filepath.Join(cwd, p)
	}
	return filepath.Clean(p)
}

// pathWithinAny reports whether abs is equal to, or nested under, any root.
// Roots are themselves cleaned + made absolute relative to cwd.
func pathWithinAny(abs string, roots []string, cwd string) bool {
	for _, root := range roots {
		r := absUnder(root, cwd)
		if abs == r {
			return true
		}
		if strings.HasPrefix(abs, r+string(filepath.Separator)) {
			return true
		}
	}
	return false
}

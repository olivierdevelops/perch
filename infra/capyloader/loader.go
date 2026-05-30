// Package capyloader compiles a perch .perch source file into a
// domain.Program. The pipeline:
//
//  1. Run the source through the embedded lib.capy via the capy engine.
//     Output is an NDJSON event stream.
//  2. Stream-parse events into a Program: each line corresponds to one
//     of the lib's `write` calls (name, command_begin, config, op, …).
//  3. Fold flat `_enter` / `_leave` op markers into nested Op.Body slices.
package capyloader

import (
	"bufio"
	_ "embed"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/user"
	"path/filepath"
	"strings"

	"github.com/luowensheng/perch/domain"

	"github.com/luowensheng/capy"
)

//go:embed lib.capy
var librarySource string

// LibrarySource returns the embedded lib.capy grammar source.
func LibrarySource() string { return librarySource }

// opKindsSource is the canonical list of built-in op kinds (one per line),
// generated from ops.BuiltinKinds(). The loader needs this to disambiguate a
// bare capture's leading name — `let s = sha256 "x"` (op) vs `let r = docker
// ps` (declared bin) — at load time, before any interpreter handler map is
// available. Most value-returning ops (file_size, get_env, sha256_file, …)
// have no dedicated grammar keyword: they are reachable only via the generic
// capture form, so they wouldn't appear in the grammar-derived opVocabulary().
// A drift test in infra/ops keeps this file in sync with the handler registry.
//
//go:embed opkinds.txt
var opKindsSource string

// Load parses a .perch source file and returns a Program, recursively
// resolving any `import "PATH"` directives.
//
// Special case: path == "-" reads the source from os.Stdin. Imports
// from a stdin-loaded program are resolved against cwd (no $0 to
// derive a directory from). This is what makes piping work:
//
//	curl -fsSL https://.../commands.perch | perch -f - <command>
//
// Imports form a graph: cycles are detected and reported (not followed).
// Each file's transitive imports are merged into the root program's
// command set — flat by default, namespaced when written as
// `import "X" as ALIAS` (commands become callable as `ALIAS.name`).
func Load(path string) (*domain.Program, error) {
	prog, err := loadRecursive(path, map[string]bool{})
	if err != nil {
		return nil, err
	}
	if err := checkNameRegistry(prog); err != nil {
		return nil, err
	}
	if err := enforceZeroAmbient(prog); err != nil {
		return nil, err
	}
	return prog, nil
}

// LoadFromString is Load but reads from an in-memory string. Imports
// declared in the source ARE resolved (against cwd), so a string
// passed via the REPL or the embedded MCP server can still pull in
// sibling files.
func LoadFromString(scriptSrc string) (*domain.Program, error) {
	prog, imports, err := parseOnce(scriptSrc)
	if err != nil {
		return nil, err
	}
	merged, err := resolveImports(prog, imports, "", map[string]bool{})
	if err != nil {
		return nil, err
	}
	if err := checkNameRegistry(merged); err != nil {
		return nil, err
	}
	if err := enforceZeroAmbient(merged); err != nil {
		return nil, err
	}
	return merged, nil
}

// loadRecursive is the file-backed entry point. `visited` tracks every
// absolute path on the current import stack so cycles surface as a
// helpful error rather than infinite recursion. Passed by value (cloned
// at each call) so siblings don't see each other's stacks.
func loadRecursive(path string, visited map[string]bool) (*domain.Program, error) {
	var (
		src     string
		absPath string
	)
	if path == "-" {
		data, err := io.ReadAll(os.Stdin)
		if err != nil {
			return nil, fmt.Errorf("read stdin: %w", err)
		}
		src = string(data)
		absPath = "-"
	} else {
		var err error
		absPath, err = filepath.Abs(path)
		if err != nil {
			absPath = path
		}
		if visited[absPath] {
			return nil, fmt.Errorf("import cycle detected: %s already on the import stack", absPath)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("read %s: %w", path, err)
		}
		src = string(data)
	}

	prog, imports, err := parseOnce(src)
	if err != nil {
		return nil, err
	}
	prog.ScriptPath = absPath

	// Mark this file as visited for the duration of its subtree.
	visited[absPath] = true
	defer delete(visited, absPath)

	// Directory that import paths are resolved against. For stdin,
	// imports resolve against cwd.
	importBase := ""
	if absPath != "-" {
		importBase = filepath.Dir(absPath)
	}
	return resolveImports(prog, imports, importBase, visited)
}

// parseOnce runs one file through capy and returns the program plus
// the list of import directives encountered. Pure (no IO).
func parseOnce(scriptSrc string) (*domain.Program, []importDirective, error) {
	lib, err := capy.NewLibrary(librarySource)
	if err != nil {
		return nil, nil, fmt.Errorf("compile perch library: %w", err)
	}
	stream, err := lib.Run(scriptSrc)
	if err != nil {
		return nil, nil, fmt.Errorf("parse script: %w", err)
	}
	return parseEventStream(stream)
}

// resolveImports loads each import target and merges its commands +
// globals into `prog`. Errors carry the importing file's directive so
// debugging an "import not found" error doesn't require git-bisecting
// the import graph.
//
// The raw path string supports `${name}` substitution before filesystem
// resolution — see expandImportPath. This is what makes
//
//	import "${file_dir}/shared/aws.perch" as aws
//
// portable across machines and machine-independent of the cwd at
// invocation time.
func resolveImports(prog *domain.Program, imports []importDirective, base string, visited map[string]bool) (*domain.Program, error) {
	for _, imp := range imports {
		expanded, err := expandImportPath(imp.Path, base)
		if err != nil {
			return nil, fmt.Errorf("import %q: %w", imp.Path, err)
		}
		target := expanded
		if !filepath.IsAbs(target) && base != "" {
			target = filepath.Join(base, target)
		}
		sub, err := loadRecursive(target, visited)
		if err != nil {
			return nil, fmt.Errorf("import %q: %w", imp.Path, err)
		}
		if err := mergeProgram(prog, sub, imp.Alias); err != nil {
			return nil, fmt.Errorf("import %q: %w", imp.Path, err)
		}
	}
	// Resolve bare-name dispatch BEFORE template expansion: a bare `deploy` /
	// `ensure_dir "x"` was folded by the grammar into an implicit `exec` op;
	// now that the full command + template sets are merged, rewrite those
	// whose bin is actually a command (→ `run`) or a template (→
	// `_template_call`, which the expansion pass below then inlines). Bins are
	// left as exec. This is what lets `run`/`call` be dropped from the surface.
	resolveBareDispatch(prog)

	// Re-run template expansion now that imported templates are merged
	// into prog.Templates. parseEventStream did a first pass on the
	// parent file's own templates; that pass leaves `_template_call`
	// markers for any template the parent didn't define locally. Now
	// that imported templates are visible, expand those remaining calls.
	if len(prog.Templates) > 0 {
		if err := expandAllTemplates(prog); err != nil {
			return nil, err
		}
	}
	return prog, nil
}

// resolveBareDispatch rewrites implicit-exec ops (`deploy`, `ensure_dir "x"`)
// whose leading name is a command or template into the corresponding `run` /
// `_template_call` op. The unique-name registry guarantees the name resolves
// to at most one of {command, template, bin}, so the mapping is unambiguous.
// Names that are neither command nor template stay as exec (a real bin, or an
// undeclared-bin error raised later by enforceZeroAmbient).
func resolveBareDispatch(prog *domain.Program) {
	cmds := prog.Commands
	tmpls := prog.Templates
	req := prog.Requirements
	opKinds := opSet()
	var walk func(ops []domain.Op)
	walk = func(ops []domain.Op) {
		for i := range ops {
			op := &ops[i]
			if op.Kind == "exec" && op.Args != nil && truthyArg(op.Args["implicit"]) {
				bin, _ := op.Args["bin"].(string)
				switch {
				case opKinds[bin] && !req.BinAllowed(bin):
					// Bare leading name is a built-in OP, not a declared bin:
					// rewrite the captured exec back into a native op-capture.
					// splitExecArgvAll already populated _0.._N (+ `_N_var` for
					// bare-ident args) from the tail, so the op handler sees the
					// same positional args it would from an explicit `op a "b"`
					// call — and a bare-ident arg still resolves against bindings.
					// A name that is BOTH an op and a declared bin stays exec —
					// the explicit `bin "…"` declaration signals the subprocess.
					args := map[string]any{}
					copyArgvSlots(op.Args, args)
					*op = domain.Op{Kind: bin, Args: args, CaptureInto: op.CaptureInto, Line: op.Line}
				case cmds[bin] != nil:
					demoteVars(op.Args)
					args := map[string]any{"target": bin}
					copyArgvSlots(op.Args, args)
					*op = domain.Op{Kind: "run", Args: args, CaptureInto: op.CaptureInto, Line: op.Line}
				case tmpls[bin] != nil:
					demoteVars(op.Args)
					args := map[string]any{"name": bin}
					copyArgvSlots(op.Args, args)
					*op = domain.Op{Kind: "_template_call", Args: args, Line: op.Line}
				default:
					// Stays a subprocess exec (a declared bin): argv tokens are
					// literal, so a bare-ident token is its own text, never a
					// binding lookup (`docker ps` → literal `ps`).
					demoteVars(op.Args)
				}
			} else if op.Kind != "exec" && len(op.Body) == 0 && op.Args != nil &&
				(op.CaptureInto != "" || truthyArg(op.Args["implicit_ident"])) &&
				(op.Args["_0_var"] != nil || truthyArg(op.Args["implicit_ident"])) {
				// A single-bare-ident bare-name op: either a capture
				// (`let x = NAME arg`, let_1arg_ident) or a flat statement
				// (`ensure_dir BUILD_DIR`, implicit_ident). The arg was emitted as
				// `_0_var` — a var-ref, the right default when NAME is a built-in
				// op (`upper who`, `ensure_dir BUILD_DIR`). The marker is internal
				// only; drop it now.
				delete(op.Args, "implicit_ident")
				name := op.Kind
				if opKinds[name] {
					// Genuine built-in op: leave the var-ref `_0_var` in place for
					// InterpolateArgs to resolve. Nothing else to do.
				} else {
					// NAME is a bin/command/template, not an op: the bare ident is a
					// LITERAL positional token, not a var-ref, so demote `_0_var` →
					// `_0` before folding.
					if vn, ok := op.Args["_0_var"].(string); ok {
						delete(op.Args, "_0_var")
						op.Args["_0"] = vn
					}
					switch {
					case cmds[name] != nil:
						args := map[string]any{"target": name}
						copyArgvSlots(op.Args, args)
						*op = domain.Op{Kind: "run", Args: args, CaptureInto: op.CaptureInto, Line: op.Line}
					case tmpls[name] != nil:
						args := map[string]any{"name": name}
						copyArgvSlots(op.Args, args)
						*op = domain.Op{Kind: "_template_call", Args: args, Line: op.Line}
					default:
						args := map[string]any{"bin": name, "implicit": true}
						copyArgvSlots(op.Args, args)
						*op = domain.Op{Kind: "exec", Args: args, CaptureInto: op.CaptureInto, Line: op.Line}
					}
				}
			}
			if len(op.Body) > 0 {
				walk(op.Body)
			}
		}
	}
	for _, c := range cmds {
		if c != nil {
			walk(c.Ops)
		}
	}
	if prog.Catch != nil {
		walk(prog.Catch.Ops)
	}
	for _, t := range tmpls {
		if t != nil {
			walk(t.Ops)
		}
	}
}

// copyArgvSlots copies the positional argv slots from an exec op's args into a
// destination map. Both literal `_N` and var-ref `_N_var` slots are preserved,
// so converting an implicit-exec into a native op-capture keeps a bare-ident
// argument resolving against the bindings (`join SRC DST`).
func copyArgvSlots(src, dst map[string]any) {
	for n := 0; ; n++ {
		lit := fmt.Sprintf("_%d", n)
		varK := lit + "_var"
		lv, hasLit := src[lit]
		vv, hasVar := src[varK]
		if !hasLit && !hasVar {
			break
		}
		if hasLit {
			dst[lit] = lv
		}
		if hasVar {
			dst[varK] = vv
		}
	}
}

// demoteVars rewrites every `_N_var` slot to a literal `_N` slot in place — the
// bare-identifier token becomes its own literal text. Used when an implicit-exec
// resolves to a subprocess bin / command / template, where a positional arg is
// a literal token (`docker ps`, not the binding `ps`), never a var-ref.
func demoteVars(args map[string]any) {
	for n := 0; ; n++ {
		lit := fmt.Sprintf("_%d", n)
		varK := lit + "_var"
		_, hasLit := args[lit]
		vv, hasVar := args[varK]
		if !hasLit && !hasVar {
			break
		}
		if hasVar {
			args[lit] = vv
			delete(args, varK)
		}
	}
}

func truthyArg(v any) bool {
	b, ok := v.(bool)
	return ok && b
}

// expandAllTemplates re-runs the template expansion pass across every
// command body, catch body, and template body in `prog`. Idempotent —
// commands that already had every call resolved by parseEventStream are
// unchanged. Used after imports merge to resolve `call` markers that
// referred to imported templates.
func expandAllTemplates(prog *domain.Program) error {
	for _, cmd := range prog.Commands {
		expanded, err := expandTemplateOps(cmd.Ops, prog.Templates, map[string]bool{})
		if err != nil {
			return fmt.Errorf("expanding template in command %q: %w", cmd.Name, err)
		}
		cmd.Ops = expanded
	}
	if prog.Catch != nil {
		expanded, err := expandTemplateOps(prog.Catch.Ops, prog.Templates, map[string]bool{})
		if err != nil {
			return fmt.Errorf("expanding template in catch: %w", err)
		}
		prog.Catch.Ops = expanded
	}
	for name, tpl := range prog.Templates {
		expanded, err := expandTemplateOps(tpl.Ops, prog.Templates, map[string]bool{name: true})
		if err != nil {
			return fmt.Errorf("expanding template in template %q: %w", name, err)
		}
		tpl.Ops = expanded
	}
	return nil
}

// expandImportPath substitutes `${name}` placeholders in an import path
// before filesystem resolution. Recognised names:
//
//	${file_dir} / ${script_dir} — directory of the importing file
//	${home} / ${HOME}           — user's home directory
//	${cache_dir}                — OS user cache dir
//	${config_dir}               — OS user config dir
//	${temp_dir}                 — OS temp dir
//	${exe_dir}                  — directory of the running perch binary
//	${user} / ${USER}           — current username
//	${ANY_OTHER}                — falls through to os.LookupEnv
//
// Unknown names fail with a clear error rather than expanding to
// empty (which would silently produce a wrong path like `/shared/aws.perch`
// instead of the intended `${HOME}/shared/aws.perch`).
//
// This is the same set perch auto-binds at runtime — kept aligned so
// users don't have to learn a separate vocabulary for import-time vs
// command-time interpolation.
func expandImportPath(raw, fileDir string) (string, error) {
	return expandTemplate(raw, func(name string) (string, bool) {
		switch name {
		case "file_dir", "script_dir":
			if fileDir == "" {
				if cwd, err := os.Getwd(); err == nil {
					return cwd, true
				}
			}
			return fileDir, true
		case "home", "HOME":
			if h, err := os.UserHomeDir(); err == nil {
				return h, true
			}
		case "cache_dir":
			if d, err := os.UserCacheDir(); err == nil {
				return d, true
			}
		case "config_dir":
			if d, err := os.UserConfigDir(); err == nil {
				return d, true
			}
		case "temp_dir":
			return os.TempDir(), true
		case "exe_dir":
			if exe, err := os.Executable(); err == nil {
				if resolved, err := filepath.EvalSymlinks(exe); err == nil {
					exe = resolved
				}
				return filepath.Dir(exe), true
			}
		case "user", "USER":
			if u, err := user.Current(); err == nil {
				return u.Username, true
			}
		}
		// Fall through to host env. ${ANYTHING_ELSE} reads the live env.
		if v, ok := os.LookupEnv(name); ok {
			return v, true
		}
		return "", false
	})
}

// expandTemplate is a tiny `${name}` scanner. It mirrors the interpreter's
// interpolation rules at the surface (same syntax) but lives in this
// package to avoid a loader→interpreter import cycle, and uses a
// resolver callback so the caller decides what each name means.
func expandTemplate(s string, resolve func(name string) (string, bool)) (string, error) {
	var out strings.Builder
	i := 0
	for i < len(s) {
		if i+1 < len(s) && s[i] == '$' && s[i+1] == '{' {
			end := strings.IndexByte(s[i+2:], '}')
			if end < 0 {
				return "", fmt.Errorf("unterminated ${ in %q", s)
			}
			name := strings.TrimSpace(s[i+2 : i+2+end])
			v, ok := resolve(name)
			if !ok {
				return "", fmt.Errorf("unknown placeholder ${%s} in import path", name)
			}
			out.WriteString(v)
			i += 2 + end + 1
			continue
		}
		out.WriteByte(s[i])
		i++
	}
	return out.String(), nil
}

// mergeProgram folds `from`'s commands into `into`. Flat imports merge
// commands by their bare names; aliased imports prefix each with
// `ALIAS.`. Conflicts surface as errors so duplicate-name bugs don't
// silently win-or-lose. Globals merge silently with parent-wins
// precedence (the importer can override a default from a shared file).
func mergeProgram(into, from *domain.Program, alias string) error {
	for name, cmd := range from.Commands {
		if cmd == nil {
			continue
		}
		// Skip imported private commands on flat import — they're the
		// imported file's internal helpers and shouldn't pollute the
		// caller's namespace. Aliased import keeps them callable as
		// `ALIAS.privateName` for advanced composition.
		if cmd.Modifiers.Private && alias == "" {
			continue
		}
		targetName := name
		if alias != "" {
			targetName = alias + "." + name
		}
		if _, conflict := into.Commands[targetName]; conflict {
			return fmt.Errorf("command %q already declared (conflict from import)", targetName)
		}
		into.Commands[targetName] = cmd
	}
	// Globals: parent wins. If the importer declared NAME, we keep its
	// value; otherwise we adopt the imported value. This gives the
	// caller a "default with override" semantic without surfacing a
	// conflict.
	existing := map[string]bool{}
	for _, g := range into.Globals.Bindings {
		existing[g.Name] = true
	}
	for _, g := range from.Globals.Bindings {
		if !existing[g.Name] {
			into.Globals.Bindings = append(into.Globals.Bindings, g)
		}
	}
	// Templates from imports are merged into the parent's template map
	// flat (no alias prefix — templates are inlined at parse time, so
	// there's no namespace at runtime to worry about). Importer-defined
	// templates win on conflict because the importer is the "more
	// specific" definition; an imported template can't shadow one the
	// caller already wrote.
	if from.Templates != nil {
		if into.Templates == nil {
			into.Templates = map[string]*domain.Template{}
		}
		for name, tpl := range from.Templates {
			if _, exists := into.Templates[name]; exists {
				continue
			}
			into.Templates[name] = tpl
		}
	}
	// Requirements union. Under zero-ambient-authority the spawnable-bin set
	// is the manifest, so an imported file's declared bins/hosts/env/fs must
	// count toward the merged program's allowlist — otherwise a command
	// imported from a file that legitimately declared `bin "docker"` would
	// fail bin_not_declared at load. We union (dedup) rather than parent-wins:
	// capabilities are additive across the import graph. Declared is OR'd so a
	// root that imports a manifest-bearing partial counts as declared.
	if from.Requirements.Declared {
		into.Requirements.Declared = true
	}
	into.Requirements.Bins = unionBins(into.Requirements.Bins, from.Requirements.Bins)
	into.Requirements.Envs = unionEnvs(into.Requirements.Envs, from.Requirements.Envs)
	into.Requirements.Hosts = unionHosts(into.Requirements.Hosts, from.Requirements.Hosts)
	into.Requirements.ReadRoots = unionStrings(into.Requirements.ReadRoots, from.Requirements.ReadRoots)
	into.Requirements.WriteRoots = unionStrings(into.Requirements.WriteRoots, from.Requirements.WriteRoots)
	into.Requirements.OS = unionStrings(into.Requirements.OS, from.Requirements.OS)
	into.Requirements.Arch = unionStrings(into.Requirements.Arch, from.Requirements.Arch)

	// Catch handlers don't propagate via import — only the root file's
	// catch (if any) is active. Imported catches would race with the
	// importer's, and there's no clear right answer for what wins.
	return nil
}

func unionBins(a, b []domain.BinReq) []domain.BinReq {
	seen := map[string]bool{}
	for _, x := range a {
		seen[x.Name] = true
	}
	for _, x := range b {
		if !seen[x.Name] {
			a = append(a, x)
			seen[x.Name] = true
		}
	}
	return a
}

func unionEnvs(a, b []domain.EnvReq) []domain.EnvReq {
	seen := map[string]bool{}
	for _, x := range a {
		seen[x.Name] = true
	}
	for _, x := range b {
		if !seen[x.Name] {
			a = append(a, x)
			seen[x.Name] = true
		}
	}
	return a
}

func unionHosts(a, b []domain.HostReq) []domain.HostReq {
	seen := map[string]bool{}
	for _, x := range a {
		seen[x.Name] = true
	}
	for _, x := range b {
		if !seen[x.Name] {
			a = append(a, x)
			seen[x.Name] = true
		}
	}
	return a
}

func unionStrings(a, b []string) []string {
	seen := map[string]bool{}
	for _, x := range a {
		seen[x] = true
	}
	for _, x := range b {
		if !seen[x] {
			a = append(a, x)
			seen[x] = true
		}
	}
	return a
}

// event is one line of NDJSON emitted by lib.capy.
type event struct {
	Event       string         `json:"event"`
	Name        string         `json:"name,omitempty"`
	Kind        string         `json:"kind,omitempty"`
	Value       any            `json:"value,omitempty"`
	Args        map[string]any `json:"args,omitempty"`
	CaptureInto string         `json:"capture_into,omitempty"`
	Path        string         `json:"path,omitempty"`  // `import` event
	Alias       string         `json:"alias,omitempty"` // `import` event
	// `requires_bin` events
	Optional bool `json:"optional,omitempty"`
}

// importDirective is one `import "PATH" [as ALIAS]` statement from a
// .perch file. Loader-internal; not exposed on domain.Program.
type importDirective struct {
	Path  string // raw path as written
	Alias string // empty for flat, set for namespaced
}

type parserState int

const (
	stTop parserState = iota
	stGlobals
	stCommand
	stCommandArg
	stCommandDo
	stCatch
	stCatchArg
	stCatchDo
	stTemplate
	stTemplateArg
	stTemplateDo
	stBundle
	stRequires
)

func parseEventStream(stream string) (*domain.Program, []importDirective, error) {
	prog := &domain.Program{
		Commands:  map[string]*domain.Command{},
		Templates: map[string]*domain.Template{},
		Globals:   domain.Globals{Bindings: nil},
	}
	var imports []importDirective

	state := stTop
	var curCmd *domain.Command
	var curCatch *domain.Catch
	var curTpl *domain.Template
	var curArg *domain.ArgSpec
	// opStack[0] is the destination slice for the next op (either a
	// command's Ops or a block op's Body). Push on _enter, pop on _leave.
	var opStack []*[]domain.Op

	scanner := bufio.NewScanner(strings.NewReader(stream))
	scanner.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	lineNum := 0
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		lineNum++
		if line == "" {
			continue
		}
		var ev event
		if err := json.Unmarshal([]byte(line), &ev); err != nil {
			return nil, nil, fmt.Errorf("line %d: malformed event %q: %w", lineNum, line, err)
		}

		switch ev.Event {

		case "name":
			prog.Name = asString(ev.Value)
		case "about":
			prog.Description = asString(ev.Value)
		case "version":
			prog.Version = asString(ev.Value)
		case "import":
			// Imports are collected at parse time and resolved by the
			// caller (Load / resolveImports). This keeps parseEventStream
			// pure — no IO. Only valid at the top level.
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: import must be at top level", lineNum)
			}
			imports = append(imports, importDirective{Path: ev.Path, Alias: ev.Alias})

		case "globals_removed":
			return nil, nil, fmt.Errorf("line %d: the `globals ... end` block was removed — "+
				"declare shared bindings bare at top level instead, e.g. `BUILD_DIR = \"%s/.out\"`",
				lineNum, "${script_dir}")
		case "bundle_begin":
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: bundle block must be at top level", lineNum)
			}
			state = stBundle
		case "bundle_end":
			state = stTop
		case "bundle_include":
			if state != stBundle {
				return nil, nil, fmt.Errorf("line %d: 'include' event outside bundle block", lineNum)
			}
			path := asString(ev.Value)
			prog.Bundle.Includes = append(prog.Bundle.Includes, path)
			if ev.Alias != "" {
				// Default entry: basename of the source path. This is what
				// `tarballPaths` writes for a file include. For directory
				// includes the alias points at the dir root; users wanting
				// to address a specific file inside should alias the file
				// path directly, e.g. `include "./modules/policy.wasm" as policy_wasm`.
				entry := filepath.Base(path)
				prog.Bundle.Aliases = append(prog.Bundle.Aliases, domain.BundleAlias{
					Name:  ev.Alias,
					Entry: entry,
				})
			}
		case "requires_begin":
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: requires block must be at top level", lineNum)
			}
			state = stRequires
			prog.Requirements.Declared = true
		case "requires_end":
			state = stTop
		case "requires_bin":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'bin' outside requires block", lineNum)
			}
			prog.Requirements.Bins = append(prog.Requirements.Bins, domain.BinReq{
				Name:     ev.Name,
				Alias:    ev.Alias,
				Optional: ev.Optional,
			})
		case "requires_bin_field":
			// Mutates the most-recently-appended BinReq (hash / hash_file).
			n := len(prog.Requirements.Bins)
			if n == 0 || state != stRequires {
				return nil, nil, fmt.Errorf("line %d: '%s' outside a `bin ... end` block", lineNum, ev.Kind)
			}
			switch ev.Kind {
			case "hash":
				prog.Requirements.Bins[n-1].Hash = asString(ev.Value)
			case "hash_file":
				prog.Requirements.Bins[n-1].HashFile = asString(ev.Value)
			default:
				return nil, nil, fmt.Errorf("line %d: unknown bin field %q", lineNum, ev.Kind)
			}
		case "requires_env":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'env' outside requires block", lineNum)
			}
			prog.Requirements.Envs = append(prog.Requirements.Envs, domain.EnvReq{
				Name:     ev.Name,
				Optional: ev.Optional,
			})
		case "requires_host":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'host' outside requires block", lineNum)
			}
			prog.Requirements.Hosts = append(prog.Requirements.Hosts, domain.HostReq{
				Name:     ev.Name,
				Optional: ev.Optional,
			})
		case "requires_read":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'read' outside requires block", lineNum)
			}
			prog.Requirements.ReadRoots = append(prog.Requirements.ReadRoots, asString(ev.Value))
		case "requires_write":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'write' outside requires block", lineNum)
			}
			prog.Requirements.WriteRoots = append(prog.Requirements.WriteRoots, asString(ev.Value))
		case "requires_read_var":
			// Bare-name form `read SRC` → wrap the binding name back into a
			// "${SRC}" root so it interpolates like the string form.
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'read' outside requires block", lineNum)
			}
			prog.Requirements.ReadRoots = append(prog.Requirements.ReadRoots, "${"+ev.Name+"}")
		case "requires_write_var":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'write' outside requires block", lineNum)
			}
			prog.Requirements.WriteRoots = append(prog.Requirements.WriteRoots, "${"+ev.Name+"}")
		case "requires_os":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'os' outside requires block", lineNum)
			}
			prog.Requirements.OS = append(prog.Requirements.OS, ev.Name)
		case "requires_arch":
			if state != stRequires {
				return nil, nil, fmt.Errorf("line %d: 'arch' outside requires block", lineNum)
			}
			prog.Requirements.Arch = append(prog.Requirements.Arch, ev.Name)

		case "global":
			// Bare top-level `NAME = value` binding (the `globals` block was
			// removed). Allowed only at file scope — a bare assignment inside
			// a command body is an error (use `let` there instead).
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: a bare `%s = ...` binding must be at top level "+
					"(inside a command body use `let %s = ...`)", lineNum, ev.Name, ev.Name)
			}
			prog.Globals.Bindings = append(prog.Globals.Bindings, domain.GlobalBinding{
				Name:  ev.Name,
				Type:  inferLiteralType(ev.Value),
				Value: ev.Value,
			})

		case "command_begin":
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: nested command not allowed", lineNum)
			}
			curCmd = &domain.Command{
				Name:      ev.Name,
				Env:       map[string]string{},
				Modifiers: domain.Modifiers{},
			}
			prog.Commands[ev.Name] = curCmd
			state = stCommand
			opStack = nil

		case "command_end":
			if state != stCommand && state != stCommandDo {
				return nil, nil, fmt.Errorf("line %d: command_end while not in command", lineNum)
			}
			curCmd = nil
			state = stTop
			opStack = nil

		case "catch_begin":
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: nested catch not allowed", lineNum)
			}
			curCatch = &domain.Catch{Bind: ev.Name}
			prog.Catch = curCatch
			state = stCatch
			opStack = nil

		case "catch_end":
			curCatch = nil
			state = stTop
			opStack = nil

		case "template_begin":
			if state != stTop {
				return nil, nil, fmt.Errorf("line %d: template must be at top level", lineNum)
			}
			if _, dup := prog.Templates[ev.Name]; dup {
				return nil, nil, fmt.Errorf("line %d: template %q redeclared", lineNum, ev.Name)
			}
			curTpl = &domain.Template{Name: ev.Name}
			prog.Templates[ev.Name] = curTpl
			state = stTemplate
			opStack = nil

		case "template_end":
			if state != stTemplate && state != stTemplateDo {
				return nil, nil, fmt.Errorf("line %d: template_end while not in template", lineNum)
			}
			curTpl = nil
			state = stTop
			opStack = nil

		case "do_begin":
			switch state {
			case stCommand:
				state = stCommandDo
				opStack = []*[]domain.Op{&curCmd.Ops}
			case stCatch:
				state = stCatchDo
				opStack = []*[]domain.Op{&curCatch.Ops}
			case stTemplate:
				state = stTemplateDo
				opStack = []*[]domain.Op{&curTpl.Ops}
			default:
				return nil, nil, fmt.Errorf("line %d: 'do' outside command/catch/template", lineNum)
			}

		case "do_end":
			switch state {
			case stCommandDo:
				state = stCommand
			case stCatchDo:
				state = stCatch
			case stTemplateDo:
				state = stTemplate
			default:
				return nil, nil, fmt.Errorf("line %d: 'do_end' without matching do_begin", lineNum)
			}
			opStack = nil

		case "arg_begin":
			switch state {
			case stCommand:
				curArg = &domain.ArgSpec{Name: ev.Name}
				state = stCommandArg
			case stCatch:
				curArg = &domain.ArgSpec{Name: ev.Name}
				state = stCatchArg
			case stTemplate:
				curArg = &domain.ArgSpec{Name: ev.Name}
				state = stTemplateArg
			default:
				return nil, nil, fmt.Errorf("line %d: 'arg' outside command/catch/template config region", lineNum)
			}

		case "arg_end":
			if curArg == nil {
				return nil, nil, fmt.Errorf("line %d: 'arg_end' without matching 'arg'", lineNum)
			}
			if curArg.Type == "" {
				return nil, nil, fmt.Errorf("line %d: arg %q has no `type` field", lineNum, curArg.Name)
			}
			switch state {
			case stCommandArg:
				curCmd.Args = append(curCmd.Args, *curArg)
				state = stCommand
			case stCatchArg:
				// Catch doesn't currently track its own arg list; ignore.
				state = stCatch
			case stTemplateArg:
				curTpl.Args = append(curTpl.Args, *curArg)
				state = stTemplate
			}
			curArg = nil

		case "arg_field":
			if curArg == nil {
				return nil, nil, fmt.Errorf("line %d: '%s' field outside an `arg` block", lineNum, ev.Kind)
			}
			switch ev.Kind {
			case "type":
				curArg.Type = asString(ev.Value)
			case "default":
				curArg.Default = ev.Value
				curArg.HasDefault = true
			case "optional":
				curArg.Optional = true
			case "rest":
				curArg.Rest = true
			case "index":
				idx := int(asFloatish(ev.Value))
				curArg.Index = &idx
			default:
				return nil, nil, fmt.Errorf("line %d: unknown arg field %q", lineNum, ev.Kind)
			}

		case "config":
			// `description` inside an arg block sets the arg's description.
			if curArg != nil && ev.Kind == "description" {
				curArg.Description = asString(ev.Value)
				break
			}
			// Templates support `description` as their only config statement.
			// Anything else (private, env, on_signal …) is meaningless on a
			// parse-time stamp and is rejected here so it surfaces early.
			if curTpl != nil {
				if ev.Kind != "description" {
					return nil, nil, fmt.Errorf("line %d: config %q not allowed inside a template", lineNum, ev.Kind)
				}
				curTpl.Description = asString(ev.Value)
				break
			}
			if err := applyConfig(curCmd, curCatch, ev); err != nil {
				return nil, nil, fmt.Errorf("line %d: %w", lineNum, err)
			}

		case "op":
			if len(opStack) == 0 {
				return nil, nil, fmt.Errorf("line %d: op '%s' outside a do block", lineNum, ev.Kind)
			}
			// `_template_call` is a placeholder the expansion pass replaces
			// with the template's body, after positional args are bound.
			// Templates can be defined later in the file than they're
			// called, so we collect markers here and resolve after the full
			// stream is parsed.
			if ev.Kind == "_template_call" {
				op := domain.Op{
					Kind: "_template_call",
					Args: ev.Args,
					Line: lineNum,
				}
				*opStack[len(opStack)-1] = append(*opStack[len(opStack)-1], op)
				break
			}
			if ev.Kind == "_enter" {
				// Push a new nested op whose body becomes the active target.
				// CaptureInto lets a block op's value be bound (e.g.
				// `let out = pipe ... end` captures the last stage's stdout).
				newOp := domain.Op{
					Kind:        ev.Name,
					Args:        ev.Args,
					CaptureInto: ev.CaptureInto,
				}
				*opStack[len(opStack)-1] = append(*opStack[len(opStack)-1], newOp)
				// Reslice the just-appended op so we can grow its Body.
				dest := opStack[len(opStack)-1]
				idx := len(*dest) - 1
				opStack = append(opStack, &(*dest)[idx].Body)
			} else if ev.Kind == "_leave" {
				if len(opStack) < 2 {
					return nil, nil, fmt.Errorf("line %d: _leave without matching _enter", lineNum)
				}
				opStack = opStack[:len(opStack)-1]
			} else {
				op := domain.Op{
					Kind:        ev.Kind,
					Args:        ev.Args,
					CaptureInto: ev.CaptureInto,
				}
				*opStack[len(opStack)-1] = append(*opStack[len(opStack)-1], op)
			}

		default:
			return nil, nil, fmt.Errorf("line %d: unknown event %q", lineNum, ev.Event)
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, nil, fmt.Errorf("scan: %w", err)
	}

	// Expand `_template_call` markers inline. Templates are pure parse-
	// time stamps: every call is replaced with the template's body, with
	// positional args bound as ${argname} substitutions in string args.
	// Done AFTER the stream is parsed so templates can be defined later
	// in the file than they're called. Recursion is rejected (visited
	// set); declaration-emitting templates were already prevented at
	// parse time (templates can't contain command_begin or import).
	if len(prog.Templates) > 0 {
		for _, cmd := range prog.Commands {
			expanded, err := expandTemplateOps(cmd.Ops, prog.Templates, map[string]bool{})
			if err != nil {
				return nil, nil, fmt.Errorf("expanding template in command %q: %w", cmd.Name, err)
			}
			cmd.Ops = expanded
		}
		if prog.Catch != nil {
			expanded, err := expandTemplateOps(prog.Catch.Ops, prog.Templates, map[string]bool{})
			if err != nil {
				return nil, nil, fmt.Errorf("expanding template in catch: %w", err)
			}
			prog.Catch.Ops = expanded
		}
		// Templates may also reference each other. Expand their own bodies
		// so a later-spliced call already has nested calls resolved.
		for name, tpl := range prog.Templates {
			expanded, err := expandTemplateOps(tpl.Ops, prog.Templates, map[string]bool{name: true})
			if err != nil {
				return nil, nil, fmt.Errorf("expanding template in template %q: %w", name, err)
			}
			tpl.Ops = expanded
		}
	}

	// Split `exec`/pipe-stage argv: the grammar captures all argv tokens as a
	// single `tail` string (`argv_raw`); we shell-split it into positional
	// slots _0.._N HERE, at load time, on the LITERAL source (before any
	// ${…} interpolation). That preserves the §3.3 keystone — a `${msg}`
	// token stays one slot even if its runtime value contains spaces — while
	// letting the grammar be one `exec BIN tail` function instead of an
	// arity-capped overload ladder. (capy >= ac128fb: quote-preserving tail.)
	splitExecArgvAll(prog)

	return prog, imports, nil
}

// splitExecArgvAll walks every command/catch/template body (recursing into
// block bodies) and expands any `exec` op's argv_raw into _0.._N slots.
func splitExecArgvAll(prog *domain.Program) {
	for _, cmd := range prog.Commands {
		splitExecArgv(cmd.Ops)
	}
	if prog.Catch != nil {
		splitExecArgv(prog.Catch.Ops)
	}
	for _, tpl := range prog.Templates {
		splitExecArgv(tpl.Ops)
	}
}

func splitExecArgv(ops []domain.Op) {
	for i := range ops {
		op := &ops[i]
		if op.Kind == "exec" && op.Args != nil {
			if raw, ok := op.Args["argv_raw"].(string); ok {
				delete(op.Args, "argv_raw")
				classified := shellSplitArgsClassified(raw)
				tokens := make([]string, len(classified))
				for i, t := range classified {
					tokens[i] = t.text
				}
				bin, _ := op.Args["bin"].(string)
				// Full token stream for this exec line: bin + argv.
				full := append([]string{bin}, tokens...)
				if clauses, opsBetween := splitChain(full); len(opsBetween) > 0 {
					// `exec a && exec b ; exec c` — fold into an exec_chain
					// block op whose Body holds one child exec per clause.
					// The operators (literal source tokens, never from ${…})
					// drive short-circuit evaluation at runtime (§3.3).
					capture := op.CaptureInto
					*op = domain.Op{
						Kind:        "exec_chain",
						Args:        map[string]any{"ops": toAnySlice(opsBetween)},
						CaptureInto: capture,
						Line:        op.Line,
					}
					for _, clause := range clauses {
						op.Body = append(op.Body, makeExecOp(clause))
					}
				} else {
					// Only IMPLICIT execs (a bare leading name, possibly an op) get
					// per-token classification: a bare-identifier token becomes a
					// `_N_var` var-ref (resolves to a binding of that name, else the
					// literal token), a quoted / `${…}` / flag token a literal `_N`.
					// resolveBareDispatch then keeps `_N_var` for a built-in op and
					// demotes it to literal for a subprocess bin. An EXPLICIT `exec`
					// is always a subprocess: its argv is literal, so no var-refs.
					implicit := truthyArg(op.Args["implicit"])
					for n, t := range classified {
						if implicit && t.bare {
							op.Args[fmt.Sprintf("_%d_var", n)] = t.text
						} else {
							op.Args[fmt.Sprintf("_%d", n)] = t.text
						}
					}
				}
			}
		}
		if len(op.Body) > 0 {
			splitExecArgv(op.Body)
		}
	}
}

// splitChain partitions a flat exec token stream on top-level `&&` / `||` /
// `;` operator tokens. Returns the per-clause token slices and the operators
// between them. A clause after an operator begins with the user's repeated
// `exec` keyword, which is stripped. Returns no operators when the line is a
// single command (the common case).
func splitChain(tokens []string) (clauses [][]string, ops []string) {
	var cur []string
	flush := func() {
		// Strip a leading "exec" keyword on chained clauses (the 1st clause's
		// bin came from the grammar, so it has no leading "exec").
		if len(clauses) > 0 && len(cur) > 0 && cur[0] == "exec" {
			cur = cur[1:]
		}
		clauses = append(clauses, cur)
		cur = nil
	}
	for _, t := range tokens {
		if t == "&&" || t == "||" || t == ";" {
			flush()
			ops = append(ops, t)
			continue
		}
		cur = append(cur, t)
	}
	flush()
	return clauses, ops
}

// makeExecOp builds a single exec Op from a clause's tokens (bin + argv).
func makeExecOp(clause []string) domain.Op {
	args := map[string]any{}
	if len(clause) > 0 {
		args["bin"] = clause[0]
		for n, tok := range clause[1:] {
			args[fmt.Sprintf("_%d", n)] = tok
		}
	}
	return domain.Op{Kind: "exec", Args: args}
}

func toAnySlice(ss []string) []any {
	out := make([]any, len(ss))
	for i, s := range ss {
		out[i] = s
	}
	return out
}

// argToken is one split argv token plus a classification: `bare` is true when
// the token in the LITERAL source was an unquoted bare identifier (no quotes,
// no `${…}`, matches `[A-Za-z_][A-Za-z0-9_]*`). A bare token is treated like a
// CLI variable reference — it resolves to a binding of that name if one exists,
// otherwise it stays the literal token text (see InterpolateArgs). Quoted
// tokens (`"foo"`), `${x}` placeholders, flags (`-q`), and `k=v` pairs are NOT
// bare and pass through as literal/interpolated positional slots.
type argToken struct {
	text string
	bare bool
}

// shellSplitArgs splits a tail-captured argv string into tokens, honoring
// double quotes (a quoted run is one token, with the surrounding quotes
// stripped) and backslash escapes. Runs on the LITERAL source, so `${x}`
// placeholders pass through as single tokens and are interpolated later.
func shellSplitArgs(s string) []string {
	cls := shellSplitArgsClassified(s)
	out := make([]string, len(cls))
	for i, t := range cls {
		out[i] = t.text
	}
	return out
}

// shellSplitArgsClassified is shellSplitArgs that also records, per token,
// whether it was an unquoted bare identifier (see argToken).
func shellSplitArgsClassified(s string) []argToken {
	var out []argToken
	var cur []byte
	inQuote := false
	started := false
	sawQuoteOrDollar := false
	flush := func() {
		out = append(out, argToken{text: string(cur), bare: !sawQuoteOrDollar && isBareIdent(cur)})
		cur = cur[:0]
		started = false
		sawQuoteOrDollar = false
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case c == '\\' && i+1 < len(s):
			cur = append(cur, s[i+1])
			i++
			started = true
			sawQuoteOrDollar = true
		case c == '"':
			inQuote = !inQuote
			started = true
			sawQuoteOrDollar = true
		case (c == ' ' || c == '\t') && !inQuote:
			if started {
				flush()
			}
		default:
			if c == '$' {
				sawQuoteOrDollar = true
			}
			cur = append(cur, c)
			started = true
		}
	}
	if started {
		flush()
	}
	return out
}

// isBareIdent reports whether b is a non-empty `[A-Za-z_][A-Za-z0-9_]*`.
func isBareIdent(b []byte) bool {
	if len(b) == 0 {
		return false
	}
	for i, c := range b {
		switch {
		case c == '_' || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z'):
		case i > 0 && c >= '0' && c <= '9':
		default:
			return false
		}
	}
	return true
}

// expandTemplateOps walks `ops`, replacing every `_template_call` op with
// the named template's body. Positional args (`_0`, `_1`, …) are bound to
// the template's declared arg names and substituted into string Args
// values via the same ${NAME} convention the runtime uses for bindings.
// `expanding` tracks the names currently mid-expansion — re-entering one
// is recursion and rejected with a clear error.
//
// Unknown templates are LEFT IN PLACE as `_template_call` markers (no
// error). The post-import pass (expandAllTemplates) re-runs this with
// the full template map after imports merge, then the final --check
// pass rejects any remaining unresolved markers as "unknown template".
func expandTemplateOps(ops []domain.Op, templates map[string]*domain.Template, expanding map[string]bool) ([]domain.Op, error) {
	out := make([]domain.Op, 0, len(ops))
	for _, op := range ops {
		// Block ops carry a Body. Recurse into it whether or not this
		// op is itself a template call so nested calls inside a `retry`
		// / `parallel` / `if` block also resolve.
		if len(op.Body) > 0 && op.Kind != "_template_call" {
			inner, err := expandTemplateOps(op.Body, templates, expanding)
			if err != nil {
				return nil, err
			}
			op.Body = inner
		}
		if op.Kind != "_template_call" {
			out = append(out, op)
			continue
		}
		// Resolve the call. If the template isn't visible yet (an
		// imported one not merged in), pass through unchanged so the
		// post-import expansion pass can resolve it.
		name, _ := op.Args["name"].(string)
		tpl, ok := templates[name]
		if !ok {
			out = append(out, op)
			continue
		}
		if expanding[name] {
			return nil, fmt.Errorf("line %d: template %q calls itself (recursion is not allowed)", op.Line, name)
		}
		// Bind positional args to declared template args.
		bindings := map[string]string{}
		for idx, spec := range tpl.Args {
			key := fmt.Sprintf("_%d", idx)
			val, has := op.Args[key]
			if !has {
				if spec.HasDefault {
					val = spec.Default
				} else if spec.Optional {
					val = ""
				} else {
					return nil, fmt.Errorf("line %d: template %q missing positional arg #%d (%s)", op.Line, name, idx, spec.Name)
				}
			}
			bindings[spec.Name] = stringifyArg(val)
		}
		// Expand the template body, then substitute bindings into every
		// string Args entry. The body might contain its own template calls
		// — recurse first.
		nextExpanding := map[string]bool{}
		for k, v := range expanding {
			nextExpanding[k] = v
		}
		nextExpanding[name] = true
		body, err := expandTemplateOps(tpl.Ops, templates, nextExpanding)
		if err != nil {
			return nil, err
		}
		spliced := substituteOps(body, bindings, name)
		out = append(out, spliced...)
	}
	return out, nil
}

// substituteOps deep-clones a slice of ops, replacing ${NAME} occurrences
// in every string-valued Args entry with the bound value, and tagging
// each emitted op with ExpandedFrom for diagnostics.
func substituteOps(ops []domain.Op, bindings map[string]string, templateName string) []domain.Op {
	out := make([]domain.Op, len(ops))
	for i, op := range ops {
		out[i] = op
		out[i].ExpandedFrom = templateName
		if len(op.Args) > 0 {
			newArgs := make(map[string]any, len(op.Args))
			for k, v := range op.Args {
				if s, ok := v.(string); ok {
					newArgs[k] = substituteString(s, bindings)
				} else {
					newArgs[k] = v
				}
			}
			out[i].Args = newArgs
		}
		if len(op.Body) > 0 {
			out[i].Body = substituteOps(op.Body, bindings, templateName)
		}
	}
	return out
}

// substituteString replaces every ${NAME} in s with bindings[NAME].
// Unknown names are left as literal ${...} so the runtime interpolation
// step can still complain about them in the post-expansion error path
// (which already knows about user-scope bindings).
func substituteString(s string, bindings map[string]string) string {
	if !strings.Contains(s, "${") {
		return s
	}
	var b strings.Builder
	i := 0
	for i < len(s) {
		if i+1 < len(s) && s[i] == '$' && s[i+1] == '{' {
			end := strings.IndexByte(s[i+2:], '}')
			if end < 0 {
				b.WriteString(s[i:])
				return b.String()
			}
			name := s[i+2 : i+2+end]
			if v, ok := bindings[name]; ok {
				b.WriteString(v)
			} else {
				b.WriteString(s[i : i+2+end+1])
			}
			i += 2 + end + 1
			continue
		}
		b.WriteByte(s[i])
		i++
	}
	return b.String()
}

// stringifyArg renders a positional template-call arg to the string form
// the substitution model expects. Strings come through unquoted; numbers
// and bools render with %v.
func stringifyArg(v any) string {
	switch x := v.(type) {
	case nil:
		return ""
	case string:
		return x
	case float64:
		// Integer-valued floats render without trailing zero, matching
		// the interpreter's ToStringValue behaviour.
		if x == float64(int64(x)) {
			return fmt.Sprintf("%d", int64(x))
		}
		return fmt.Sprintf("%v", x)
	default:
		return fmt.Sprintf("%v", x)
	}
}

func applyConfig(cmd *domain.Command, catch *domain.Catch, ev event) error {
	if cmd != nil {
		return applyConfigToCommand(cmd, ev)
	}
	if catch != nil {
		switch ev.Kind {
		case "description":
			catch.Description = asString(ev.Value)
		case "proxy_args":
			// Explicit opt-in to binding ${proxy_args} in the catch
			// body. Without this modifier, ${proxy_args} is unbound
			// inside the catch and referencing it errors — the
			// catch→shell forwarding pattern is no longer accidental.
			catch.ProxyArgs = true
		}
		// Other config kinds silently ignored for catch.
		return nil
	}
	return fmt.Errorf("config '%s' outside command/catch", ev.Kind)
}

func applyConfigToCommand(c *domain.Command, ev event) error {
	switch ev.Kind {
	case "description":
		c.Description = asString(ev.Value)
	case "private":
		c.Modifiers.Private = true
	case "detached":
		c.Modifiers.Detached = true
	case "proxy_args":
		c.Modifiers.ProxyArgs = true
	case "require_os":
		c.Modifiers.RequireOS = append(c.Modifiers.RequireOS, asString(ev.Value))
	case "require_arch":
		c.Modifiers.RequireArch = append(c.Modifiers.RequireArch, asString(ev.Value))
	case "dir":
		c.Modifiers.Dir = asString(ev.Value)
	case "on_signal":
		c.Modifiers.OnSignal = asString(ev.Value)
	case "env":
		c.Env[ev.Name] = asString(ev.Value)
	case "test":
		c.Modifiers.Test = true
	case "test_allow_network":
		c.Modifiers.TestAllowNetwork = true
	case "test_allow_shell":
		c.Modifiers.TestAllowShell = true
	case "test_allow_write":
		c.Modifiers.TestAllowWrite = true
	case "test_allow_subprocess":
		c.Modifiers.TestAllowSubprocess = true
	case "test_keep_cwd":
		c.Modifiers.TestKeepCwd = true
	case "test_timeout":
		c.Modifiers.TestTimeoutSecs = int(asFloatish(ev.Value))
	default:
		return fmt.Errorf("unknown config kind: %q", ev.Kind)
	}
	return nil
}

func asFloatish(v any) float64 {
	switch x := v.(type) {
	case float64:
		return x
	case int:
		return float64(x)
	case int64:
		return float64(x)
	}
	return 0
}

func asString(v any) string {
	switch x := v.(type) {
	case string:
		return x
	case nil:
		return ""
	default:
		b, _ := json.Marshal(x)
		return string(b)
	}
}

func inferLiteralType(v any) string {
	switch v.(type) {
	case bool:
		return "bool"
	case float64:
		// JSON unmarshal gives all numbers as float64; distinguish int vs
		// float by checking the fractional part.
		f := v.(float64)
		if f == float64(int64(f)) {
			return "int"
		}
		return "float"
	case string:
		return "string"
	default:
		return "string"
	}
}

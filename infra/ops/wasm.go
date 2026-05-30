package ops

// WebAssembly execution via wazero — the constrained execution lane.
//
// Where `shell` is the universal escape hatch (flexible, hard to
// constrain), `wasm_run` is the hard-isolation lane: the module's
// imports are enumerated at instantiation and **anything not declared
// fails at load time**. There's no shell escape, no syscall surface,
// no ambient filesystem — the module sees only what the body's
// `wasm_mount_*` / `wasm_env` declarations grant.
//
// Identity-fit notes:
//   - wasm_run is a block op. Its body holds marker ops
//     (wasm_arg / wasm_mount_read / wasm_mount_write / wasm_env)
//     that the handler walks to collect WASI config. The marker ops
//     themselves have no-op handlers and error if invoked at runtime
//     outside a wasm_run block (caught at --check time).
//   - The handler streams stdout/stderr through the interpreter's
//     normal sinks, so --trace / --audit / --report all see wasm_run
//     as one span with its real wall-clock duration.
//   - Module bytecode is cached at ~/.cache/perch/wasm/<sha256>.cwasm
//     after first compilation so re-runs are fast.
//
// What this is NOT (yet):
//   - No socket/network access. WASI Preview 1 sockets are
//     experimental; sockets land properly in Preview 2. Roadmap.
//   - No URL loading of modules. Local path only. Hash-pinned URL
//     loading is on the roadmap.
//   - No named-export function calls beyond _start. Modules built by
//     TinyGo / wasi-sdk / Rust+wasm32-wasi all expose _start, so this
//     covers the common case. Component-Model-style typed calls are
//     a v2 feature.

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/olivierdevelops/perch/domain"
	"github.com/olivierdevelops/perch/infra/interpreter"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
	"github.com/tetratelabs/wazero/sys"
)

func registerWasm(m map[string]interpreter.Handler) {
	m["wasm_run"] = opWasmRun
	// Marker ops — only meaningful inside a wasm_run body. The handler
	// here is the "called outside" sentinel; the wasm_run handler reads
	// them from op.Body directly, never dispatching.
	m["wasm_arg"] = wasmMarkerErr("wasm_arg")
	m["wasm_mount_read"] = wasmMarkerErr("wasm_mount_read")
	m["wasm_mount_write"] = wasmMarkerErr("wasm_mount_write")
	m["wasm_env"] = wasmMarkerErr("wasm_env")
	m["wasm_allow_host"] = wasmMarkerErr("wasm_allow_host")
}

func wasmMarkerErr(name string) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return nil, fmt.Errorf("%s is only valid inside a wasm_run or wasm_bundle block", name)
	}
}

// wasmCache is shared across wasm_run calls so a `parallel` block
// instantiating the same module on multiple branches reuses the compiled
// bytecode. Guarded by mu.
var (
	wasmCacheMu sync.Mutex
	wasmRuntime wazero.Runtime
	wasmCompiled = map[string]wazero.CompiledModule{}
)

// opWasmRun loads a WebAssembly module and runs it under WASI Preview 1.
// Capability declarations in the body are the *only* way the module
// sees the outside world.
//
// Two source forms (single op, no URI scheme):
//
//   wasm_run "./path/to/mod.wasm"   ← string literal → load from disk
//
//   bundle
//       include "./policy.wasm" as policy_wasm
//   end
//   wasm_run policy_wasm            ← bare ident → resolve to bundle bytes
//
// The grammar emits `_alias: true` in args when the user passed a bare
// identifier; we look it up in program.Bundle.Aliases and read straight
// from the embedded tar.gz. Otherwise the path is treated as a host
// file path.
func opWasmRun(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	rawPath := argString(args, "path", "_0")
	if rawPath == "" {
		return nil, fmt.Errorf("wasm_run: missing module path")
	}

	var (
		modulePath  string
		moduleBytes []byte
		cacheKey    string
	)

	isAlias, _ := args["_alias"].(bool)
	if isAlias {
		entry, ok := lookupBundleAlias(i.Program, rawPath)
		if !ok {
			return nil, fmt.Errorf("wasm_run: %q is not a declared bundle alias (add `include \"…\" as %s` to your `bundle ... end` section)", rawPath, rawPath)
		}
		buf, present, err := BundleReadFile(entry)
		if err != nil {
			return nil, fmt.Errorf("wasm_run %s: %w", rawPath, err)
		}
		if !present {
			return nil, fmt.Errorf("wasm_run %s: alias resolves to %q but this binary has no embedded bundle (build with `perch --build`)", rawPath, entry)
		}
		moduleBytes = buf
		cacheKey = "bundle:" + BundleHash() + ":" + entry
		rawPath = entry // for argv0 / error messages
	} else {
		modulePath = resolve(rawPath, b)
		if _, err := os.Stat(modulePath); err != nil {
			return nil, fmt.Errorf("wasm_run: module %q: %w", rawPath, err)
		}
	}
	return runWasmModule(i, b, args, rawPath, "wasm_run", modulePath, moduleBytes, cacheKey)
}

// lookupBundleAlias returns the bundle entry name for a declared alias.
func lookupBundleAlias(p *domain.Program, name string) (string, bool) {
	if p == nil {
		return "", false
	}
	for _, a := range p.Bundle.Aliases {
		if a.Name == name {
			return a.Entry, true
		}
	}
	return "", false
}

// runWasmModule is the shared backend. Exactly one of (modulePath,
// moduleBytes) is non-empty.
func runWasmModule(
	i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any,
	rawPath, opName, modulePath string, moduleBytes []byte, cacheKey string,
) (any, error) {
	// Walk the body to collect capability declarations. The body ops
	// aren't dispatched — they're config-bearers the handler reads
	// directly. Anything else in the body is an error (keeps the
	// surface honest: no general ops can sneak in alongside the
	// declarations).
	body, _ := args["_body"].([]domain.Op)
	cfg := wasmConfig{}
	for _, op := range body {
		opArgs, err := interpreter.InterpolateArgs(op.Args, b)
		if err != nil {
			return nil, fmt.Errorf("%s: interpolating %s: %w", opName, op.Kind, err)
		}
		switch op.Kind {
		case "wasm_arg":
			cfg.argv = append(cfg.argv, argString(opArgs, "value", "_0"))
		case "wasm_mount_read":
			cfg.mountsRO = append(cfg.mountsRO, resolve(argString(opArgs, "path", "_0"), b))
		case "wasm_mount_write":
			cfg.mountsRW = append(cfg.mountsRW, resolve(argString(opArgs, "path", "_0"), b))
		case "wasm_env":
			for _, n := range strings.Split(argString(opArgs, "names", "_0"), ",") {
				n = strings.TrimSpace(n)
				if n != "" {
					cfg.envAllow = append(cfg.envAllow, n)
				}
			}
		case "wasm_allow_host":
			// Per-module HTTP host allowlist. Each entry composes
			// AND-wise with the outer --allow-host policy. When the
			// module imports the `perch.http_get` host function, only
			// these hosts will resolve; everything else is refused.
			cfg.allowHost = append(cfg.allowHost, argString(opArgs, "host", "_0"))
		default:
			return nil, fmt.Errorf("%s: %q is not valid inside a %s block (only wasm_arg / wasm_mount_read / wasm_mount_write / wasm_env / wasm_allow_host)", opName, op.Kind, opName)
		}
	}

	// Compile (or fetch from cache).
	var compiled wazero.CompiledModule
	var err error
	if moduleBytes != nil {
		compiled, err = compileWasmBytes(cacheKey, moduleBytes)
	} else {
		compiled, err = compileWasm(modulePath)
	}
	if err != nil {
		return nil, fmt.Errorf("%s: compile %q: %w", opName, rawPath, err)
	}

	// Build the WASI configuration with EXACTLY the declared
	// capabilities. Everything else is omitted, which is how
	// wazero/WASI denies it.
	wasiCfg := wazero.NewModuleConfig().
		WithName(""). // anonymous; multiple instantiations OK
		WithStdout(i.Stdout).
		WithStderr(i.Stderr).
		WithStdin(i.Stdin).
		WithArgs(append([]string{wasmArgv0(modulePath, rawPath)}, cfg.argv...)...)

	// Env allowlist — only declared names pass through, and only their
	// real values. Anything not in the allowlist is invisible.
	for _, name := range cfg.envAllow {
		// b.Lookup honors --env restrictions; if the user's outer flag
		// also restricts envs, the intersection is what reaches WASI.
		if val, ok := b.Lookup(name); ok {
			wasiCfg = wasiCfg.WithEnv(name, val)
		}
	}

	// File-system mounts. WASI mounts directories into a virtual root;
	// wazero exposes one fs at a time, so we layer them by guest path.
	// Convention: read-only mounts go at /ro/<basename>, read-write at
	// /rw/<basename>. The user can also specify absolute host paths
	// for the module to use directly (path translation is the module's
	// responsibility).
	fsCfg := wazero.NewFSConfig()
	for _, p := range cfg.mountsRO {
		fsCfg = fsCfg.WithReadOnlyDirMount(p, "/ro/"+filepath.Base(p))
	}
	for _, p := range cfg.mountsRW {
		fsCfg = fsCfg.WithDirMount(p, "/rw/"+filepath.Base(p))
	}
	wasiCfg = wasiCfg.WithFSConfig(fsCfg)

	// Run. Honors the interpreter's wall-clock deadline (a `timeout`
	// block or --max-runtime) by wiring it into wazero's context.
	ctx := context.Background()
	if !i.Deadline.IsZero() {
		var cancel context.CancelFunc
		ctx, cancel = context.WithDeadline(ctx, i.Deadline)
		defer cancel()
	}
	// Attach per-call HTTP state to the context. The shared "perch"
	// host module (installed once at runtime init) reads this on every
	// `perch.http_get` call. Modules that didn't declare wasm_allow_host
	// will see http_get → -1 (host module is always linked but the
	// allow-table is empty for them).
	httpState := buildHTTPCallState(i, cfg.allowHost)
	ctx = context.WithValue(ctx, httpStateKey{}, httpState)
	mod, err := wasmRuntime.InstantiateModule(ctx, compiled, wasiCfg)
	if err != nil {
		// wazero returns *sys.ExitError when WASI's exit() is called;
		// exit code 0 is success.
		var exitErr *sys.ExitError
		if errors.As(err, &exitErr) && exitErr.ExitCode() == 0 {
			return nil, nil
		}
		return nil, fmt.Errorf("%s %q: %w", opName, rawPath, err)
	}
	if mod != nil {
		_ = mod.Close(ctx)
	}
	return nil, nil
}

// wasmArgv0 picks the argv[0] the module sees. From disk: basename of
// the host path. From bundle: basename of the bundle entry name.
func wasmArgv0(modulePath, rawPath string) string {
	if modulePath != "" {
		return filepath.Base(modulePath)
	}
	return filepath.Base(rawPath)
}

type wasmConfig struct {
	argv      []string
	mountsRO  []string
	mountsRW  []string
	envAllow  []string
	allowHost []string // wasm_allow_host declarations — per-module HTTP allowlist
}

// compileWasm reads a .wasm file from disk, compiles it via wazero
// (cached), and returns the compiled module. The cache is keyed by the
// SHA-256 of the file contents so re-running a recipe with the same
// module skips the parse/validate step.
func compileWasm(path string) (wazero.CompiledModule, error) {
	hash, bytes, err := readAndHash(path)
	if err != nil {
		return nil, err
	}
	return compileWasmBytes(hash, bytes)
}

// compileWasmBytes compiles a module from an in-memory buffer, keyed by
// the supplied cache key. The key should already incorporate enough
// identity to make collisions impossible — `compileWasm` uses the file
// content's sha256; the bundle path uses "bundle:<bundleHash>:<entry>".
func compileWasmBytes(cacheKey string, moduleBytes []byte) (wazero.CompiledModule, error) {
	wasmCacheMu.Lock()
	defer wasmCacheMu.Unlock()
	if cached, ok := wasmCompiled[cacheKey]; ok {
		return cached, nil
	}
	if wasmRuntime == nil {
		ctx := context.Background()
		wasmRuntime = wazero.NewRuntime(ctx)
		wasi_snapshot_preview1.MustInstantiate(ctx, wasmRuntime)
		// "perch" host module — exposes http_get / http_status /
		// http_body_len / http_read_body / http_close. Per-call gating
		// is via context-attached state; see wasm_http.go.
		if err := installPerchHostModule(ctx, wasmRuntime); err != nil {
			return nil, fmt.Errorf("install perch host module: %w", err)
		}
	}
	compiled, err := wasmRuntime.CompileModule(context.Background(), moduleBytes)
	if err != nil {
		return nil, err
	}
	wasmCompiled[cacheKey] = compiled
	return compiled, nil
}

func readAndHash(path string) (string, []byte, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", nil, err
	}
	defer f.Close()
	bytes, err := io.ReadAll(f)
	if err != nil {
		return "", nil, err
	}
	sum := sha256.Sum256(bytes)
	return hex.EncodeToString(sum[:]), bytes, nil
}


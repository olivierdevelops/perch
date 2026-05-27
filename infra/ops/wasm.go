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

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"

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
}

func wasmMarkerErr(name string) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return nil, fmt.Errorf("%s is only valid inside a wasm_run block", name)
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

// opWasmRun loads, configures, and runs a WebAssembly module under
// WASI Preview 1. Capability declarations in the body are the *only*
// way the module sees the outside world.
func opWasmRun(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	rawPath := argString(args, "path", "_0")
	if rawPath == "" {
		return nil, fmt.Errorf("wasm_run: missing module path")
	}

	// Source resolution. Two schemes:
	//   bundle:PATH  → load bytes directly from the embedded tar.gz. No
	//                  disk write. Hash cache key is "bundle:<bundleHash>:PATH".
	//   PATH (any)   → resolve against script_dir, os.Open the file as
	//                  before. The traditional path.
	//
	// The `bundle:` scheme is what makes `perch --build --include … myapp`
	// fully self-contained — wasm modules are bytes in the binary, never
	// touch the disk on the target machine.
	var (
		modulePath string
		moduleBytes []byte
		cacheKey    string
	)
	if strings.HasPrefix(rawPath, "bundle:") {
		entry := strings.TrimPrefix(rawPath, "bundle:")
		buf, present, err := BundleReadFile(entry)
		if err != nil {
			return nil, fmt.Errorf("wasm_run: %w", err)
		}
		if !present {
			return nil, fmt.Errorf("wasm_run: %q references the embedded bundle but this binary has no bundle (build with `perch --build --include <dir>`)", rawPath)
		}
		moduleBytes = buf
		cacheKey = "bundle:" + BundleHash() + ":" + entry
	} else {
		modulePath = resolve(rawPath, b)
		if _, err := os.Stat(modulePath); err != nil {
			return nil, fmt.Errorf("wasm_run: module %q: %w", rawPath, err)
		}
	}

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
			return nil, fmt.Errorf("wasm_run: interpolating %s: %w", op.Kind, err)
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
		default:
			return nil, fmt.Errorf("wasm_run: %q is not valid inside a wasm_run block (only wasm_arg / wasm_mount_read / wasm_mount_write / wasm_env)", op.Kind)
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
		return nil, fmt.Errorf("wasm_run: compile %q: %w", rawPath, err)
	}

	// Build the WASI configuration with EXACTLY the declared
	// capabilities. Everything else is omitted, which is how
	// wazero/WASI denies it.
	wasiCfg := wazero.NewModuleConfig().
		WithName(""). // anonymous; multiple instantiations OK
		WithStdout(i.Stdout).
		WithStderr(i.Stderr).
		WithStdin(i.Stdin).
		WithArgs(append([]string{filepath.Base(modulePath)}, cfg.argv...)...)

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
	mod, err := wasmRuntime.InstantiateModule(ctx, compiled, wasiCfg)
	if err != nil {
		// wazero returns *sys.ExitError when WASI's exit() is called;
		// exit code 0 is success.
		var exitErr *sys.ExitError
		if errors.As(err, &exitErr) && exitErr.ExitCode() == 0 {
			return nil, nil
		}
		return nil, fmt.Errorf("wasm_run %q: %w", rawPath, err)
	}
	if mod != nil {
		_ = mod.Close(ctx)
	}
	return nil, nil
}

type wasmConfig struct {
	argv     []string
	mountsRO []string
	mountsRW []string
	envAllow []string
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


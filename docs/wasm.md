# `wasm_run` — the constrained execution lane

> **Looking for end-to-end examples?** [`wasm-walkthroughs.md`](wasm-walkthroughs.md)
> walks through five real-world workflows: markdown frontmatter
> validation, JSON Schema validation with caching, AI-agent safe
> execution via MCP, polyglot pipelines, and CI hot loops. This page
> is the reference — every flag, every capability declaration, full
> spec.

> **perch has two execution lanes.**
>
> `shell` is the universal escape hatch — flexible, fast to write,
> best-effort to constrain. `wasm_run` is the constrained lane — your
> code (or a third party's) runs with exactly the capabilities you
> declared, enforced by the WASM runtime **by construction**.
>
> Mix freely. Use `wasm_run` for the parts where it matters; `shell`
> for everything else.

## TL;DR

```capy
wasm_run "./hello.wasm"
    wasm_arg "alice"
    wasm_arg "bob"
    wasm_env "GREETING,HOME"
    wasm_mount_read  "./src"
    wasm_mount_write "./bin"
end
```

- The module sees `argv = ["hello.wasm", "alice", "bob"]`
- Only `GREETING` and `HOME` env vars are visible inside the module —
  anything else on the host is **invisible**, including `PATH`.
- `./src` (host) is mounted read-only at `/ro/src` inside the module.
- `./bin` (host) is mounted read-write at `/rw/bin`.
- The module cannot see anything else on the host filesystem.
- No network. No subprocesses. No syscalls beyond what WASI Preview 1
  declares.

## Why `wasm_run` is a different kind of safety

Compared to perch's existing capability flags:

| Today (`shell` + `--no-*` flags) | `wasm_run` |
|---|---|
| `--no-shell` blocks the **op kind** | The module **cannot syscall** — nothing to block |
| `--allow-bin docker` matches argv[0] string at runtime | The module's WASI imports are enumerated at instantiation; unknown imports fail at load |
| `--allow-host api.x.com` is a runtime DNS check | The module gets no sockets unless they're imported (sockets aren't in v1 — see Roadmap) |
| `firejail` / `sandbox-exec` / `AppContainer` for genuinely adversarial input | WASM has memory isolation in the spec; cross-platform by one wazero binary |
| Best-effort enforcement on top of a permissive model | Enforcement by construction — nothing not declared exists in the module's environment |

This isn't an incremental security improvement; it's a different category
of guarantee. For agent-driven workflows (`perch-mcp`), it's the
difference between "the agent can't easily escape" and "the agent
cannot, by construction, do anything we didn't declare."

## Building a module

Any language that targets `wasm32-wasi` (Preview 1) works. Stock Go 1.21+
does it without any extra toolchain:

```go
// hello.go
package main

import (
	"fmt"
	"os"
)

func main() {
	fmt.Println("argv:", os.Args)
	if v, ok := os.LookupEnv("GREETING"); ok {
		fmt.Println("greeting:", v)
	}
}
```

```sh
GOOS=wasip1 GOARCH=wasm go build -o hello.wasm .
```

TinyGo, Rust (`cargo build --target wasm32-wasi`), Zig, AssemblyScript,
and C++ via wasi-sdk all target the same ABI. perch doesn't care which
toolchain produced the `.wasm` — it just loads and runs it under WASI.

## The capability vocabulary

Inside a `wasm_run` block, four declarations control what the module sees:

| Declaration | Effect |
|---|---|
| `wasm_arg "VALUE"` | Append `VALUE` to the module's argv. May appear multiple times. |
| `wasm_env "K1,K2,…"` | Comma-joined env var names. Only listed names pass through. Anything else is `(not set)` from inside the module. |
| `wasm_mount_read "PATH"` | Mount host `PATH` (a directory) as read-only at `/ro/<basename>` inside the module. |
| `wasm_mount_write "PATH"` | Same, but read-write, at `/rw/<basename>`. |

Anything not declared **does not exist** in the module's environment.
There is no escape hatch from inside.

The module is invoked via WASI's `_start` (no return value, exit code
indicates outcome). The standard streams (stdin / stdout / stderr) are
wired to perch's normal sinks, so `--audit`, `--trace`, `--report` all
see the module's output as if it were a regular op.

## Composition with everything else

`wasm_run` is a block op. It composes with every other primitive:

```capy
parallel
    wasm_run "validate-darwin.wasm"
        wasm_arg "darwin"
        wasm_mount_read "./manifests"
    end
    wasm_run "validate-linux.wasm"
        wasm_arg "linux"
        wasm_mount_read "./manifests"
    end
end

retry 3
    wasm_run "fetch-and-process.wasm"
        wasm_env "API_TOKEN"
        wasm_mount_write "./out"
    end
end

cache "build-${target}-${sha256_file('go.sum')}" "24h"
    wasm_run "build.wasm"
        wasm_arg "--target=${target}"
        wasm_mount_read "./src"
        wasm_mount_write "./bin"
    end
end

sandbox "no_shell,no_network"
    wasm_run "third-party-plugin.wasm"
        wasm_arg "${input}"
    end
end
```

The `sandbox` block above is belt-and-braces — the WASM module already
has no shell or network access by construction, so wrapping it adds no
extra guarantee. But the sandbox block IS useful for *the perch ops
around the wasm_run*, like a `run other_command` that might itself
shell out.

## Compose with `--trace` and `--report`

WASM execution is just another op in the span tree:

```sh
$ perch --trace -f release.perch deploy
▸ sandbox              flags="no_network"
  ▸ wasm_run             "./validate.wasm"
✓ manifest valid
  ✓                              (124ms)
  ▸ shell                "kubectl apply -f manifest.yaml"
deployment.apps/api configured
  ✓                              (1.20s)
✓                                (1.32s)
```

```sh
$ perch --report deploy
── perch trace ─────────────────────────────────
✓ deploy (1.32s)
└─ ✓ sandbox "no_network" (1.32s)
   ├─ ✓ wasm_run "./validate.wasm" (124ms)
   └─ ✓ shell "kubectl apply ..." (1.20s)
```

`--audit FILE.ndjson` records `wasm_run` events with the module's hash
in `args` — auditors can verify exactly which `.wasm` blob ran.

## Implementation details

- **Runtime: [wazero](https://github.com/tetratelabs/wazero) v1.11.0.**
  Pure Go, no CGO. Adds ~3 MB to the perch binary.
- **WASI level: Preview 1.** Broadest tooling support — Go's stdlib,
  TinyGo, Rust+wasm32-wasi, Zig, wasi-sdk all produce Preview 1
  modules.
- **Module cache:** compiled bytecode is keyed by SHA-256 of the
  module file. Re-running the same module skips parse/compile/validate.
  Cache lives in-process; survives across `wasm_run` calls within a
  single perch invocation.
- **Deadline integration:** if a `timeout` block or `--max-runtime`
  flag is active, wazero's context honors it. Module execution
  cancels at the same point any other op would.
- **Path mounts:** read-only mounts land at `/ro/<basename>`,
  read-write at `/rw/<basename>` inside the module. Convention; not
  user-configurable in v1 (see Roadmap).

## Status — what's in the v1 / what's coming

### ✅ Shipped (v1)

- WASI Preview 1 modules via `_start`
- argv via `wasm_arg`
- env allowlist via `wasm_env`
- fs mounts via `wasm_mount_read` / `wasm_mount_write`
- deadline integration with `timeout` block + `--max-runtime`
- module bytecode cache (in-process, sha256-keyed)
- composes with all execution contexts (`parallel`, `retry`, `cache`, `sandbox`, …)
- integrates with `--audit` / `--trace` / `--report`

### 🚧 Roadmap (not yet — fail loudly if you try)

- **Network / sockets.** WASI Preview 1 has no socket API. Preview 2
  introduces them properly. Until then, modules cannot make outbound
  network calls — period. If you need network access in your module,
  call out to perch's `http_get` op around the `wasm_run` block and
  pass results in via stdin or a mounted file.
- **URL-loaded modules.** Today `wasm_run` takes a local path only.
  Loading from HTTPS with a `--accept-wasm-hash SHA256` pin is on the
  roadmap. For now: `perch download "URL" "PATH" && perch -f ... cmd`.
- **Named-export typed calls.** Today only `_start` runs (the WASI
  convention). Calling `wasm_run "X.wasm" func:"build" arg1:42` with
  typed integer/float parameters is a v2 feature — needs Component
  Model support.
- **Configurable mount paths.** Today read mounts land at
  `/ro/<basename>` and writes at `/rw/<basename>`. A
  `wasm_mount_read "host" at:"/data"` form is on the roadmap.
- **WASI Preview 2 / Component Model.** Coming, but the ecosystem is
  still settling. v2 will likely be additive (the existing
  Preview 1 path stays as a "compatibility lane").
- **Module signature verification.** Cosign / sigstore-style verification
  before loading. Currently you can verify a module's sha256 externally
  but perch doesn't enforce a signature policy.
- **Persistent on-disk cache** (`~/.cache/perch/wasm/<sha>.cwasm`).
  Today the cache is in-process only — every fresh perch invocation
  re-compiles. Wazero supports the persistent format; just not wired yet.

If you reach for any of these and find them missing, that's a known gap
— please open an issue or PR; the design space is documented and the
implementation path is clear.

## When to reach for `wasm_run` vs `shell`

| Reach for `shell` when | Reach for `wasm_run` when |
|---|---|
| Wrapping docker / kubectl / git / aws / brew | Running validation/transformation logic on your own machine |
| Glue work that orchestrates real tools | Letting an AI agent execute arbitrary computation safely |
| The "we just need this to work" lane | Loading third-party / community plugins safely |
| Migrating from existing bash scripts | Anything where determinism + portability matters more than convenience |

Most real `.perch` files will mix both. The recipes folder uses `shell`
exclusively today; future recipes (e.g. content validators, format
converters, security scanners) are likely to be `wasm_run`-based.

## See also

- [`demos/wasm-hello/`](https://github.com/luowensheng/perch/tree/main/demos/wasm-hello) — the worked example (Go source + pre-built `.wasm` + `commands.perch`)
- [`execution-contexts.md`](execution-contexts.md) — the block-op shape that `wasm_run` plugs into
- [`sandbox.md`](sandbox.md) — the broader capability model
- [`ideas/07-hermetic-vs-capability.md`](https://github.com/luowensheng/perch/blob/main/ideas/07-hermetic-vs-capability.md) — the design discussion that led to this feature

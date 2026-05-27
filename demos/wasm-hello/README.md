# `wasm-hello` — demo of perch's `wasm_run`

A 5-line Go program compiled to WebAssembly (WASI Preview 1) that
demonstrates how perch's `wasm_run` block enforces capabilities by
construction — not by policy.

The module tries to read `argv`, four env vars, and three filesystem
paths. What it actually sees is **exactly what `commands.perch`
declared** — anything not in the allowlist is invisible (env returns
"not set"; fs returns ENOENT).

## Run it

```sh
$ GREETING="hello" perch demo
── hello.wasm ────────────────────────────────
argv: [hello.wasm alice bob]
env GREETING: hello
env HOME: /Users/you
env SECRET: (not visible — not in allowlist)
env PATH: (not visible — not in allowlist)
fs /ro/src: visible
fs /rw/bin: visible
fs /etc/passwd: (not visible — not mounted)
```

Then try the minimal version to see what the module sees with **no
capabilities granted**:

```sh
$ perch minimal
── hello.wasm ────────────────────────────────
argv: [hello.wasm no-caps]
env GREETING: (not visible — not in allowlist)
env HOME: (not visible — not in allowlist)
env SECRET: (not visible — not in allowlist)
env PATH: (not visible — not in allowlist)
fs /ro/src: (not visible — not mounted)
fs /rw/bin: (not visible — not mounted)
fs /etc/passwd: (not visible — not mounted)
```

And confirm SECRET in the host env is **invisible** without an
explicit allowlist entry:

```sh
$ SECRET=leak-me perch verify_envgate
── hello.wasm ────────────────────────────────
argv: [hello.wasm envgate-check]
env GREETING: (not visible — not in allowlist)
env HOME: /Users/you
env SECRET: (not visible — not in allowlist)
env PATH: (not visible — not in allowlist)
```

## The `.perch` shape

```capy
wasm_run "${script_dir}/hello.wasm"
    wasm_arg "alice"                          # appended to argv
    wasm_env "GREETING,HOME"                  # only these env vars pass through
    wasm_mount_read  "${script_dir}/src"      # mounted at /ro/src inside the module
    wasm_mount_write "${script_dir}/bin"      # mounted at /rw/bin (read-write)
end
```

Anything not declared is *not in the module's environment* — the
WebAssembly runtime never wires it up. This is fundamentally different
from `--no-network`: that flag *intercepts* a shell op; `wasm_run`
*never opens a hole* for the module to escape through.

## What's inside `main.go`

A tiny WASI Preview 1 program. Built with stock Go (1.21+):

```sh
GOOS=wasip1 GOARCH=wasm go build -o hello.wasm .
```

Or use the bundled `rebuild` command:

```sh
perch rebuild
```

The repo ships the pre-built `hello.wasm` so you can run the demo
without a Go toolchain.

## What this demonstrates

| `--no-shell` / `sandbox` blocks | `wasm_run` blocks |
|---|---|
| The op kind (shell) | Nothing — the module *cannot syscall* |
| `--allow-bin docker` checks argv[0] | Imports enumerated at load time |
| Runtime DNS check on every `http_*` | Module gets no sockets unless granted |
| Best effort under firejail / sandbox-exec | WASM has memory isolation in the spec |

Same MCP / `--audit` / `--report` / `--trace` integration as any other
op. Composes with `parallel`, `retry`, `timeout`, `cache`, `sandbox`.

## Status — what's in the v1 and what's coming

In: argv, env allowlist, fs mounts (read-only and read-write), WASI
Preview 1 `_start`, deadline (composes with `timeout`), span tree
integration, content-hash module cache.

[Roadmap](../../docs/wasm.md#status--whats-in-the-v1--whats-coming)
covers: sockets / network (Preview 2 / experimental), URL-loaded modules
with sha256 pinning, named-export typed calls beyond `_start`,
Component Model support.

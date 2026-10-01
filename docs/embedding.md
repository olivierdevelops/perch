# Embedding (`--build`)

`perch --build -f commands.perch -o myapp` produces a single, self-contained binary that boots straight into your commands — no perch install, no Rust or Go toolchain, no `.perch` file required on the target machine.

There are two ways to embed perch, and they solve different problems:

| You want to… | Use |
|---|---|
| **ship** a program as one executable others can run | `perch --build` (this page, below) |
| **run** `.perch` commands from inside your own Rust program, under a policy you set, getting a structured result back | the **runtime library** ([next section](#the-runtime-library-run-perch-from-rust)) |

## The runtime library — run perch from Rust

*(0.2.0)* The `perch` package is also a library. Build a `Runtime` from a `Policy`, load a program, run a command, and read a `RunResult` (`ok`, `stdout`, `stderr`, `error { kind, message, op, code, detail }`, `duration`) — in-process, no shelling out, no output parsing:

```rust
use perch::{Policy, Runtime};

let rt = Runtime::new(Policy::default().no_shell(true).no_network(true));
let prog = rt
    .load_str("command hello\n    do\n        print \"hi\"\n    end\nend\n")
    .unwrap();
let res = rt.run(&prog, "hello", &[]);
assert!(res.ok);
assert_eq!(res.stdout, "hi\n");
```

This is the crate's own doctest; it passes on this branch (`cargo test -p perch --doc`). Two runtimes with different policies can run side by side; `Loaded::check()` and `Loaded::scan()` return what `--check` and `--scan --json` report. Be aware that a few things are still process-wide (the environment, a few path helpers, the wasm cache) and that timeouts are checked between ops, not inside one. Complete example, API tables and constraints: [manuals/man-2026-0005-using-perch-as-a-library.md](manuals/man-2026-0005-using-perch-as-a-library.md). The CLI binary is a one-line entry (`perch::run_cli()`) over the same wiring; the placement of the library in the orchestrator crate is recorded in [ARCH-2026-0001](architecture/arch-2026-0001-layer-boundaries.md).

---

## Embedding with `--build`

## What you get

```sh
perch --build -o ./myapp
./myapp --help            # → lists commands from commands.perch
./myapp --version         # → version from commands.perch
./myapp <command> [args]  # → dispatches into the embedded program
scp ./myapp remote:~/     # → works on any same-OS, same-arch box
```

The output binary is functionally a private fork of perch with your program baked in. The `-f` flag is ignored — the embedded program always wins.

## On-disk format

A "perch fat binary" is layered (verified by inspecting a binary built with `perch --build` on this branch):

```
┌─────────────────────────────────┐
│ stock perch executable          │  ← original bytes, unmodified
├─────────────────────────────────┤
│ program JSON                    │  ← the serialized Program
├─────────────────────────────────┤
│ archive (optional)              │  ← gzipped tarball from `--include`; empty otherwise
├─────────────────────────────────┤
│ archive length (8 bytes, BE)    │
│ JSON length    (8 bytes, BE)    │
│ magic = "PRCHEMB2"  (8 bytes)   │  ← 24-byte footer
└─────────────────────────────────┘
```

At startup perch reads the last 24 bytes of its own executable. If the magic matches, it reads the JSON (and keeps the archive for `bundle_*` ops) and dispatches against that program. Older binaries with the 16-byte `PRCHEMB1` footer (no archive) still load. If the magic doesn't match, perch behaves normally and looks for a `.perch` file.

## Why this design

1. **No toolchain required.** The fat binary is just a copy + append; no compiler invocation. Build time is tens of milliseconds.
2. **Trivially distributable.** It's one file. Drop it on a server. Email it to a coworker. SCP it to a Pi.
3. **Source-available.** The host perch is just regular perch; anyone can read the embedded JSON section to audit what the binary will do.
4. **Idempotent rebuilds.** Running `--build` on a binary that already has an embedded program strips the old footer first, so accumulating layers can't happen.

## Limitations (current)

- **Same OS/arch only.** The output inherits the host's OS and architecture. Cross-compile (`--build --target linux-arm64`) is on the roadmap; the implementation needs per-target stubs.
- **Static op catalog.** Embedded binaries can only call the ops their host perch compiled in. If you `--build` from perch v0.1.0, the resulting binary doesn't have ops added in v0.2.0. Rebuild after upgrading.
- **No live reload.** The program JSON is frozen at build time. Changes to `commands.perch` require a fresh `--build`.

## Inspecting an embedded binary

You can dump the embedded program JSON without running anything:

```sh
# The footer is 24 bytes: [archive length][JSON length]["PRCHEMB2"], big-endian
python3 - <<'EOF'
import struct, json
b = open('./myapp', 'rb').read()
alen, jlen = struct.unpack('>QQ', b[-24:-8])
print(json.dumps(json.loads(b[-24 - alen - jlen:-24 - alen]), indent=2))
EOF
```

Output is the full parsed program: globals, commands, ops, the works.

## Security considerations

- The embedded JSON is **not signed**. If your distribution channel matters, sign the entire binary with `codesign` (macOS), `signtool` (Windows), or your platform's equivalent.
- The fat binary trusts the embedded program. Anyone who can `--build` on top of an existing perch can produce an unrelated binary with your name. Treat `--build` like any other build pipeline: the source `.perch` is the input you must trust.
- **Capabilities still apply.** The embedded program is enforced exactly like `perch -f` would enforce it: its [`requires` block](requires.md) gates every external op ([capability-gating.md](capability-gating.md)), and a program with **no** `requires` block is an empty manifest, so an embedded `shell "rm -rf ./x"` fails with `bin_not_declared` (verified) rather than running. Declared `read`/`write` roots also confine spawned binaries on macOS and Linux ([manuals/man-2026-0004-confining-spawned-binaries.md](manuals/man-2026-0004-confining-spawned-binaries.md)). The embedded binary still honors operator `--no-shell` / `--no-network` / `--no-write` / `--env` flags at launch. Either way, review the source `commands.perch` as you'd review any build input.

## Roadmap

- **Cross-compile** by shipping pre-built per-target perch stubs alongside the running binary.
- **Code signing helper** — `perch --build --sign` invoking the platform's signing tool.
- **Diffable inspection** — `perch --inspect ./myapp` to print a structured view of the embedded program.
- **Lockfile** — pin op-catalog version into the program JSON so rebuilds with a newer perch error rather than silently using new ops.

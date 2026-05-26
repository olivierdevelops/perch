# Embedding (`--build`)

`perch --build -f commands.perch -o myapp` produces a single, self-contained binary that boots straight into your commands — no perch install, no Go toolchain, no `.perch` file required on the target machine.

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

A "perch fat binary" is layered:

```
┌─────────────────────────────┐
│ stock perch executable      │  ← original bytes, unmodified
├─────────────────────────────┤
│ program.json                │  ← marshaled domain.Program
├─────────────────────────────┤
│ length (8 bytes big-endian) │  ← byte length of the JSON section
├─────────────────────────────┤
│ magic = "PRCHEMB1"          │  ← 8 ASCII bytes
└─────────────────────────────┘
```

At startup, perch reads the last 16 bytes of its own executable (`os.Executable()`). If the magic matches, it seeks back by `length + 16`, reads the JSON, parses it as a `domain.Program`, and dispatches against that program. If the magic doesn't match, perch behaves normally and looks for a `.perch` file.

## Why this design

1. **No Go toolchain required.** The fat binary is just a copy + append; no compiler invocation. Build time is tens of milliseconds.
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
# Find the footer
SIZE=$(wc -c < ./myapp)
LEN=$(tail -c 16 ./myapp | head -c 8 | xxd -p | awk '{print strtonum("0x" $0)}')

# Extract the JSON
tail -c $((LEN + 16)) ./myapp | head -c "$LEN" | jq .
```

Output is the full parsed program: globals, commands, ops, the works.

## Security considerations

- The embedded JSON is **not signed**. If your distribution channel matters, sign the entire binary with `codesign` (macOS), `signtool` (Windows), or your platform's equivalent.
- The fat binary trusts the embedded program. Anyone who can `--build` on top of an existing perch can produce an unrelated binary with your name. Treat `--build` like any other build pipeline: the source `.perch` is the input you must trust.
- The runtime has no sandbox. An embedded program with `shell "rm -rf /"` will run that. Source `commands.perch` should be reviewed exactly as you'd review a shell script.

## Roadmap

- **Cross-compile** by shipping pre-built per-target perch stubs alongside the running binary.
- **Code signing helper** — `perch --build --sign` invoking the platform's signing tool.
- **Diffable inspection** — `perch --inspect ./myapp` to print a structured view of the embedded program.
- **Lockfile** — pin op-catalog version into the program JSON so rebuilds with a newer perch error rather than silently using new ops.

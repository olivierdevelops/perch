# demo 04 — portable CLI

**The killer feature.** `perch --build` bundles this `commands.capy` into a single self-contained binary that runs anywhere — no perch install, no Go toolchain, no nothing.

```sh
# Build the binary
perch --build -o greet

# Run it
./greet hello                       # → Hello, world!
./greet hello -name=Alice           # → Hello, Alice!
./greet hello -name=Bob -greeting=Hi # → Hi, Bob!
./greet upper -name=alice           # → OI, ALICE!
./greet facts                       # → host / now
./greet --help                      # → built-in help, with arg defaults
./greet blorp                       # → falls into the catch-all
./greet --version                   # → 1.0.0  (the `version` field in commands.capy)

# Distribute
scp greet remote.host:/usr/local/bin/   # works on a fresh box
```

## How it works

`perch --build` copies the running perch binary, then appends:

1. The parsed `Program` as JSON
2. A footer: `<8 bytes length><8 bytes magic "PRCHEMB1">`

At startup, the resulting binary reads its own last 16 bytes; if the magic matches, it loads the embedded program instead of looking for a `.capy` file. So the same Go runtime that drove your dev loop now ships as the user-facing tool, with your commands baked in.

Details in [`docs/embedding.md`](../../docs/embedding.md).

## Concepts

- `catch NAME ... end` — fallback command. The unknown name is bound as `{{NAME}}` inside the body.
- The output binary inherits its `--version` from the program's `version` field.
- `--build` produces a binary for the **host** OS/arch. Cross-compile is on the roadmap.

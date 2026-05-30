# `wasm-schema-validator` — deterministic JSON validation as a bundled WASM

A self-contained demo: pure-Go JSON schema validator compiled to
WebAssembly, run inside perch's `wasm_run` block with declared file-
system capabilities. The same bytecode produces bit-identical results
on every dev's laptop AND in CI — **no Python, no Node, no `pip
install`**.

## Why this exists

Every team has the same pain:

- a config / fixture / OpenAPI validation script written in Python or Node
- CI breaks randomly due to dependency / version drift
- local results disagree with CI results

`wasm-schema-validator` replaces that with a single 3.7 MB `.wasm`
artifact you sha256-pin in CI. Same validator → same output → same
exit code, everywhere.

## Try it (no install — just perch)

```sh
$ perch -f commands.perch validate user good-alice.json
/ro/fixtures/good-alice.json: ✓ valid

$ perch -f commands.perch demo_bad
✗ <root>: unknown field "secret_field" (additionalProperties: false)
✗ age: must be ≤ 150, got 200
✗ email: must match /^[^@]+@[^@]+\.[^@]+$/, got "not-an-email"
✗ id: must be ≥ 1, got -1
✗ role: must be one of [admin user guest], got superuser
/ro/fixtures/bad-mallory.json: 5 error(s)
```

Exit code 0 on pass, 1 on violations, 2 on bad invocation. Drop into
CI directly — `perch validate_all` is the one-liner.

## Bundle into a single binary

The killer combo: `perch --build` embeds the .wasm inside a portable
binary. Recipients run one file, no perch install, no Go toolchain.

```sh
# Build the standalone tool (~13 MB — perch runtime + your .wasm)
$ perch --build -f commands.perch --include . -o validator-tool

# Distribute it
$ scp validator-tool ci-host:/usr/local/bin/

# Recipient runs it
$ ssh ci-host './validator-tool demo_good'
/ro/fixtures/good-alice.json: ✓ valid
```

The .wasm bytes are inside the binary; first run extracts to a cache
dir; subsequent runs hit the cache. Recipient never needs to know
WASM is involved.

## What the WASM module sees

When `wasm_run` instantiates `schema-validator.wasm`, the module's
WASI environment contains **exactly**:

| Inside the module | Maps to |
|---|---|
| `os.Args[0]` | `"schema-validator.wasm"` (just the basename) |
| `os.Args[1]` | `"/ro/schemas/user.json"` (declared `wasm_arg`) |
| `os.Args[2]` | `"/ro/fixtures/good-alice.json"` (declared `wasm_arg`) |
| `/ro/schemas/` | Read-only mount of `./schemas/` |
| `/ro/fixtures/` | Read-only mount of `./fixtures/` |
| `os.Environ()` | Empty — no env declared |
| Anything else | **Does not exist** — invisible by construction |

A bug in the validator can't accidentally read `/etc/passwd` because
`/etc/passwd` isn't part of the module's reality. There's no
`exec.Command()`, no `net.Dial()`, no `syscall.*` that resolves.

## The validator (200 lines, stdlib only)

`main.go` implements a useful subset of JSON Schema:

- `type` — string / number / integer / boolean / array / object / null
- `required` fields
- `enum` whitelist
- `pattern` (regex) for strings
- `minimum` / `maximum` for numbers
- `minLength` / `maxLength` for strings
- `items` for arrays (recurses)
- `additionalProperties: false` for objects
- Nested objects via `properties`

For richer validation (`$ref`, `$schema`, `allOf`/`anyOf`/`oneOf`,
format validators), drop in a real library like
[`santhosh-tekuri/jsonschema/v6`](https://github.com/santhosh-tekuri/jsonschema).
The validator API surface is the same.

## Rebuilding

```sh
$ perch -f commands.perch rebuild
✓ rebuilt schema-validator.wasm (3.7 MB)
```

Requires Go 1.21+ (the standard `wasip1` target). No TinyGo / Emscripten / wasi-sdk needed.

## Commands available

| Verb | What |
|---|---|
| `validate SCHEMA FILE` | Validate one file against one schema |
| `validate_all` | `parallel` validate every fixture against user.json |
| `demo_good` | Validate the passing fixture (alice) |
| `demo_bad` | Validate the failing fixture (mallory) — exits 1 |
| `rebuild` | Recompile the .wasm from main.go |

## CI integration

```yaml
# .github/workflows/validate.yml
- run: |
    curl -fsSL https://github.com/olivierdevelops/perch/releases/latest/download/perch-linux-amd64 -o perch
    chmod +x perch
    ./perch -f demos/wasm-schema-validator/commands.perch validate_all
```

Or bundle the demo into your own binary and ship it with the repo:

```sh
$ perch --build -f demos/wasm-schema-validator/commands.perch \
    --include demos/wasm-schema-validator \
    -o tools/validator-tool

# Commit ./tools/validator-tool — every dev's `make validate` runs the
# same binary as CI does. No version drift possible.
```

## See also

- [`docs/wasm.md`](../../docs/wasm.md) — `wasm_run` reference
- [`docs/wasm-walkthroughs.md`](../../docs/wasm-walkthroughs.md) — full walkthrough including the multi-schema pipeline pattern
- [`demos/wasm-policy-check/`](../wasm-policy-check/) — the "compliance enforcement" sibling demo
- [`demos/wasm-diff-summary/`](../wasm-diff-summary/) — agent-safe PR analysis sibling demo

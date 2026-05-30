# FAQ

## How is perch different from Make / Just / Task?

| | Make | Just | Task | perch |
|---|---|---|---|---|
| Config language | Makefile (kludgy) | Just-script | YAML | **capy DSL** |
| Cross-platform | painful | best of the three | OK | **first-class** |
| Built-in ops library | none | none | none | **~140 ops** |
| Web UI mode | — | — | — | **`--server`** |
| REPL | — | — | — | **`--shell`** |
| Portable binary output | — | — | — | **`--build`** |
| Args with types & defaults | — | partial | partial | **yes** |

The closest competitor in spirit is [Just](https://github.com/casey/just). perch differs by:

1. Treating `${name}` as a runtime placeholder — args/lets/globals/env are one uniform binding table.
2. Having a real op catalog (file ops, hashing, HTTP, regex…) so command bodies aren't just bash strings.
3. Producing a redistributable binary so non-developers on your team can run jobs without installing anything.

## Why capy and not YAML/TOML?

Capy is a transpiler engine for custom DSLs. We use it to define the perch grammar in a single declarative file ([`lib.capy`](https://github.com/olivierdevelops/perch/blob/main/infra/capyloader/lib.capy)). The benefits over hand-rolled parsing:

- The grammar is self-documenting. Adding a new keyword is one capy function.
- The output is structured JSON; no string-munging in the Go runtime.
- Users get sensible parse errors with line numbers for free.
- Future extensions (typed args, custom modifiers, alternate front-ends) are library changes, not parser rewrites.

See [capy](https://olivierdevelops.github.io/capy) for the engine.

## Can I share commands across projects?

Currently each project has its own `commands.perch`. **Import support is on the roadmap** — `import "../shared.perch"` to pull in commands from a sibling file.

For now: copy-paste. The files are small.

## How do I test my commands.perch?

**Built-in: `perch test`.** Mark a command with the `test` modifier; `perch test` discovers and runs it in a sandboxed temp cwd with `--no-shell` / `--no-network` / `--no-subprocess` on by default. Pass unless any op errors. Seven `assert_*` ops produce readable failure messages.

```capy
command build
    do
        mkdir "bin"
        write_file "bin/app" "fake binary"
    end
end

command test_build_produces_binary
    test
    do
        build
        assert_exists "bin/app"
    end
end

command test_against_real_api
    test
    test_allow_network         # opt out of the default --no-network
    test_timeout 10            # cap at 10s
    do
        body = http_get "https://api.staging.example.com/health"
        assert_contains "${body}" "ok"
    end
end
```

```sh
$ perch test
── perch test ─────────────────────────────────
✓ test_build_produces_binary               (2ms)

1 passed, 0 failed in 2ms.
```

Drop `perch test` into pre-commit and `.github/workflows/ci.yml` — exits non-zero on any failure. Filter with `--filter PATTERN`, surface output with `-v`. The full reference: **[testing.md](testing.md)**.

Other knobs that pair with testing:

- **`perch --check`** — static validator. Catches syntax / arg-type / unknown-op bugs without executing anything. Wire it into pre-commit alongside `perch test`.
- **`perch --report cmd`** — execute the command and render a span tree of every op that fired (with timing, errors, template provenance). Useful for debugging which inner op inside a `parallel` / `retry` actually failed. See [execution-contexts.md](execution-contexts.md).
- **`perch --shell`** — REPL for interactively poking at a single command without writing a test for it.

## Can ops fail silently?

No. Every op handler returns `(any, error)`. If it returns an error, the interpreter halts the command and propagates the error up. The exit code reflects this.

If you want fail-soft behavior, wrap the op in `if exists "..."` / `if A == B` etc. — the block-op style.

## What's the relationship to capy?

[capy](https://olivierdevelops.github.io/capy) is the engine that defines perch's grammar. perch is a real-world program built on top of capy.

The relationship:

- perch's `lib.capy` describes the entire DSL surface.
- capy parses user `.perch` files using that library and emits a JSON event stream.
- perch's Go-side loader consumes the events and produces a `domain.Program`.
- perch's interpreter walks the program.

If you want to build a similar tool with a different DSL, fork perch and rewrite the `lib.capy`. The runtime is reusable.

## How big is the perch binary?

About 12 MB stripped (`-ldflags='-s -w'`, no UPX). The runtime is mostly stdlib + capy + the op catalog. There's no plugin system, no embedded scripting language, no LLM dependencies.

## Can I write op handlers in something other than Go?

Not today. The op handler signature is a Go function:

```go
func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error)
```

A WASM / RPC plugin protocol is on the long-term roadmap. For now: contribute the handler upstream, or fork.

## Why "perch"?

Capybaras famously let other animals — birds, monkeys, turtles — perch on their back. Your commands perch on perch the same way: declared once, then run wherever they need to (CLI, web, REPL, embedded binary). The DSL is also built on [capy](https://olivierdevelops.github.io/capy), which is short for capybara. So the name nods both ways.

## Where do I report bugs / request features?

[github.com/olivierdevelops/perch/issues](https://github.com/olivierdevelops/perch/issues). New op requests are especially welcome — they're usually a one-file PR.

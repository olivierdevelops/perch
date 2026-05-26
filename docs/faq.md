# FAQ

## How is perch different from Make / Just / Task?

| | Make | Just | Task | perch |
|---|---|---|---|---|
| Config language | Makefile (kludgy) | Just-script | YAML | **capy DSL** |
| Cross-platform | painful | best of the three | OK | **first-class** |
| Built-in ops library | none | none | none | **~70 ops** |
| Web UI mode | — | — | — | **`--server`** |
| REPL | — | — | — | **`--shell`** |
| Portable binary output | — | — | — | **`--build`** |
| Args with types & defaults | — | partial | partial | **yes** |

The closest competitor in spirit is [Just](https://github.com/casey/just). perch differs by:

1. Treating `${name}` as a runtime placeholder — args/lets/globals/env are one uniform binding table.
2. Having a real op catalog (file ops, hashing, HTTP, regex…) so command bodies aren't just bash strings.
3. Producing a redistributable binary so non-developers on your team can run jobs without installing anything.

## Why capy and not YAML/TOML?

Capy is a transpiler engine for custom DSLs. We use it to define the perch grammar in a single declarative file ([`lib.capy`](https://github.com/luowensheng/perch/blob/main/infra/capyloader/lib.capy)). The benefits over hand-rolled parsing:

- The grammar is self-documenting. Adding a new keyword is one capy function.
- The output is structured JSON; no string-munging in the Go runtime.
- Users get sensible parse errors with line numbers for free.
- Future extensions (typed args, custom modifiers, alternate front-ends) are library changes, not parser rewrites.

See [capy](https://github.com/luowensheng/capy) for the engine.

## Can I share commands across projects?

Currently each project has its own `commands.perch`. **Import support is on the roadmap** — `import "../shared.perch"` to pull in commands from a sibling file.

For now: copy-paste. The files are small.

## How do I test my commands.perch?

Three approaches:

1. **`perch --shell`** — interactively run each command and watch the output.
2. **`if_*` block ops** — assert outcomes in a `test` command and `fail` if they don't hold:

    ```capy
    command test_build
        do
            run build
            if_exists "./bin/myapp"
                print "✓ build produced an artifact"
            end
            let s = file_size "./bin/myapp"
            if_eq "${s}" "0"
                fail "binary is empty"
            end
        end
    end
    ```

3. **CI integration** — call `perch test` from `.github/workflows/ci.yml`. Same file drives both local dev and CI.

First-class `perch test` (with assertions, mocks, golden files) is on the roadmap.

## Can ops fail silently?

No. Every op handler returns `(any, error)`. If it returns an error, the interpreter halts the command and propagates the error up. The exit code reflects this.

If you want fail-soft behavior, wrap the op in `if_exists` / `if_eq` etc. — the block-op style.

## What's the relationship to capy?

[capy](https://github.com/luowensheng/capy) is the engine that defines perch's grammar. perch is a real-world program built on top of capy.

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

Capybaras famously let other animals — birds, monkeys, turtles — perch on their back. Your commands perch on perch the same way: declared once, then run wherever they need to (CLI, web, REPL, embedded binary). The DSL is also built on [capy](https://github.com/luowensheng/capy), which is short for capybara. So the name nods both ways.

## Where do I report bugs / request features?

[github.com/luowensheng/perch/issues](https://github.com/luowensheng/perch/issues). New op requests are especially welcome — they're usually a one-file PR.

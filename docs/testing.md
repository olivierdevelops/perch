# Testing perch commands

> **`--check` validates the syntax. `perch test` validates the behavior.**
> A test is a command marked `test`. It runs in a sandboxed temp cwd with
> `--no-shell`, `--no-network`, and `--no-subprocess` on by default,
> opt-out per-test. It passes unless any op errors (including `fail
> "msg"` and the `assert_*` ops). No mocking framework, no expectations
> DSL — just commands that fail when something's wrong.

## TL;DR — the shortest possible example

```capy
command build
    do
        mkdir "bin"
        write_file "bin/app" "fake binary"
    end
end

command test_build_creates_binary
    test
    do
        build
        assert_exists "bin/app"
    end
end
```

```sh
$ perch test
── perch test ─────────────────────────────────
✓ test_build_creates_binary               (2ms)

1 passed, 0 failed in 2ms.
```

That's the whole feature in one snippet. The rest of this page covers
when to use it, what gets sandboxed, what `assert_*` helpers exist, and
how it composes with templates, execution contexts, and the `--report`
tree.

## Why test perch commands

The shorter answer to "should I write tests for a .perch file":

- You wouldn't ship a Python project without `pytest`. A `.perch` file
  that drives your CI / release / on-call automation deserves the
  same guardrails.
- `--check` catches syntactic bugs — wrong op kind, unknown
  binding, mismatched arg types. It doesn't catch semantic bugs.
  *"After `run release`, does `bin/darwin/app` exist?"* is a semantic
  question; only execution answers it.
- LLM agents editing `.perch` files (the whole `perch-mcp` use case)
  need verification. An agent that can write its own tests and then
  run `perch test` to confirm the edit didn't regress anything is
  meaningfully more trustworthy than one that can't.

## Declaring a test

A test is a regular `command` with the `test` modifier:

```capy
command test_build_writes_a_real_binary
    test
    description "build should produce an executable, not an empty file"
    do
        build
        let size = file_size "bin/darwin/app"
        if size < 1000
            fail "binary is suspiciously small (${size} bytes)"
        end
    end
end
```

Behavior:

- **Hidden from `perch --help`.** Tests aren't user-facing verbs.
- **Hidden from MCP.** Agents see commands, not tests.
- **Hidden from "did you mean…?" suggestions.** Typing
  `perch test_foo` doesn't suggest `test_foo` as a regular command.
- **Discovered by `perch test`.** Run in declaration order (Go map
  iteration is randomized; the test runner sorts alphabetically for a
  stable order).

You can name tests however you want — `test_*` is convention, not a
rule. The runner picks them up by the modifier, not the name.

## Running tests

```sh
perch test                          # run every test in commands.perch
perch test --filter build           # only tests whose name contains "build"
perch test -v                       # also surface per-test output on pass
perch test --filter foo -v          # combine
perch -f ops.perch test             # tests in a different file
```

`perch test` exits 0 if every test passed, non-zero if any failed. Wire
it into pre-commit and CI:

```yaml
# .github/workflows/ci.yml
- run: perch test
```

## Assertions

Six `assert_*` ops, each a thin sugar over `if X fail "msg"`. They
produce failure messages that name the values that didn't match.

| Op | Meaning |
|---|---|
| `assert_eq actual expected` | Fail if values differ |
| `assert_neq actual not_expected` | Fail if values are equal |
| `assert_contains haystack needle` | Fail if substring missing |
| `assert_not_contains haystack needle` | Fail if substring unexpectedly present |
| `assert_exists path` | Fail if path doesn't exist on disk |
| `assert_not_exists path` | Fail if path exists but shouldn't |
| `assert_match actual pattern` | Fail if actual doesn't match a regex |

These are pure ergonomic sugar — your tests work fine with bare
`if NAME == "X" ... end fail "msg" end`. The assertion ops just
produce a more readable diff in the failure message:

```
✗ test_default_target  (1ms)
    assert_eq failed: expected "darwin", got "linux"
```

## Sandboxing

Each test runs in a **fresh temp cwd** with **`--no-shell`,
`--no-network`, and `--no-subprocess` on by default**. The point: a
test shouldn't be able to clobber the developer's filesystem, hit the
real network, or fork off a daemon. If the test *needs* one of those
things, declare it explicitly via modifier:

| Modifier | Effect |
|---|---|
| `test_allow_shell` | Permit `shell` / `shell_output` / `shell_detached` |
| `test_allow_network` | Permit `http_*`, `download`, `dns_lookup`, port checks |
| `test_allow_write` | (Reserved for future use — writes already work inside the temp cwd) |
| `test_allow_subprocess` | Permit `pkg_install`, `kill_by_name`, `bin_version`, `os_version` |
| `test_keep_cwd` | Don't switch cwd; the test runs in the file's directory |
| `test_timeout N` | Cap this test at N seconds (default 30) |

Example:

```capy
command test_against_real_api
    test
    test_allow_network                # opt out of the default --no-network
    test_keep_cwd                     # read fixtures from the file's dir
    test_timeout 10                   # cap at 10s
    do
        let body = http_get "https://api.staging.example.com/health"
        assert_contains "${body}" "ok"
    end
end
```

After the test exits, perch deletes the temp cwd unless `--keep-tempdir`
was passed. A failed test reports the path so you can inspect what was
left behind:

```sh
$ perch test --keep-tempdir
✗ test_build_creates_binary  (2ms)
    assert_exists failed: "bin/app" does not exist
    (sandbox kept at /tmp/perch-test-test_build_creates_binary-9f3a/)
```

## Composing with templates and execution contexts

Tests are commands. Everything else in perch works inside a test:

```capy
template assert_built
    arg target type string end
    do
        assert_exists "bin/${target}/app"
        let size = file_size "bin/${target}/app"
        if size < 1000
            fail "${target} binary too small (${size} bytes)"
        end
    end
end

command test_all_targets_built
    test
    do
        parallel
            build_darwin
            build_linux
        end
        assert_built "darwin"
        assert_built "linux"
    end
end

command test_release_pipeline
    test
    test_timeout 60
    do
        timeout "30s"
            release
        end
        assert_built "darwin"
    end
end
```

## Tests with `--report`

`--report` works inside tests just like anywhere else, so a failed
test's tree is visible verbatim. Useful for debugging which inner op
in a `parallel` / `retry` body actually failed:

```sh
$ perch test --filter release --report -v
── perch test ─────────────────────────────────
✗ test_release_pipeline  (8s)
    retry: 3 attempts failed; last error: timeout after 30s
    ── perch trace ─────────────────────────────────
    ✗ test_release_pipeline (8s)
    └─ ✗ timeout "30s" (8s)
       └─ ✗ retry attempts=3 (8s)
          ├─ ✗ exec kubectl apply ... (30s)
          ├─ ✗ exec kubectl apply ... (30s)
          └─ ✗ exec kubectl apply ... (30s)

0 passed, 1 failed in 8s.
```

Same tree shape as `--report` outside tests. The verbose flag (`-v`)
surfaces the captured stdout/stderr from each test.

## What this is NOT

The line keeping the feature identity-aligned:

- **No mocking framework.** No `stub http_get returns:"…"`. That
  requires runtime indirection perch doesn't have and shouldn't grow.
  If your test needs a fake server, run one — `parallel` a real HTTP
  server alongside the assertion code, or shell out to a fixture
  binary.
- **No expectations DSL.** No `.should.equal()`, no `expect(x).toBe(y)`.
  Just `if X fail "msg"` and the seven `assert_*` helpers.
- **No before/after hooks.** Each test stands alone. Shared setup
  factors into a template that tests `call`.
- **No fixtures, no data providers, no parameterized tests.** A
  `for_each` loop inside a test does the parameterized-test thing
  when you need it.
- **Not parallel by default.** Tests run sequentially so failure
  output stays readable.

The framing for one-sentence docs:

> **A test is a command marked `test`.** Run them with `perch test`.
> They pass unless they call `fail` or any op errors. Same ops, same
> templates, same execution contexts, same `--report`. Tests *are*
> perch programs.

## Recommended workflow

```sh
# 1. Write your command + a test next to it
perch test --filter mything             # iterate fast

# 2. Once it passes, run the whole suite
perch test                              # confirm nothing else broke

# 3. Wire into pre-commit / CI
echo 'perch test' >> .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

For LLM-agent workflows, the equivalent is: have the agent run
`perch test` after every edit and surface failures back to itself.
A passing `perch test` is meaningful evidence that the edit didn't
regress anything tested.

## See also

- [`language.md`](language.md) — canonical syntax reference
- [`execution-contexts.md`](execution-contexts.md) — templates,
  contexts, and `--report` in detail
- [`sandbox.md`](sandbox.md) — the capability model tests sit on top of
- [`commands.perch` in the perch repo itself](https://github.com/luowensheng/perch/blob/main/commands.perch) — eats its own dog food

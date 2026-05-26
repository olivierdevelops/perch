# Language reference

The complete surface of the `commands.capy` DSL. Two firm rules to keep in mind everywhere:

1. **Config vs body is syntactic.** Between `command NAME` and `do` is *declarative configuration*. Inside `do … end` is the *executable body*. They never mix.
2. **`{{name}}` interpolates at runtime.** Capy itself doesn't touch `{{…}}` in strings, so the placeholder round-trips through parsing into the program JSON unchanged. The Go runtime substitutes from the bindings table (args → globals → host env) just before each op runs.

## File structure

```capy
name    "..."           # top-level metadata
about   "..."
version "..."

globals                 # bindings shared by every command
    KEY = VALUE
    ...
end

command NAME            # one or more commands
    ...config...
    do
        ...ops...
    end
end

catch NAME              # optional fallback for unknown commands
    do
        ...ops...
    end
end
```

## Top-level metadata

| Surface | Effect |
|---|---|
| `name "x"`    | Program name. Shown in `--help`. |
| `about "x"`   | Top-level description. Shown in `--help`. |
| `version "x"` | Program version. Returned by `--version`. |

## `globals … end`

Bindings shared by every command invocation. Each line is `NAME = LITERAL`. The literal's type — `bool`, `int`, `float`, `string` — is preserved.

```capy
globals
    verbose    = true             # bool
    PORT       = 8080             # int
    RATE       = 0.5              # float
    BUILD_DIR  = "./builds"       # string
end
```

Globals are visible inside every command body as `{{verbose}}`, `{{BUILD_DIR}}`, etc. By convention, UPPER_SNAKE_CASE globals are also exported as environment variables to every spawned `shell` call.

## Commands

A `command NAME ... end` declares one named, callable unit. Inside it the **config region** runs from the opening line to the `do` keyword; the **body region** runs from `do` to its matching `end`.

```capy
command build
    # ── config ──
    description "Compile myapp"
    arg         target string "Target OS"
    arg_default target "darwin"
    require_os  "darwin" "linux"
    env         GO111MODULE "on"

    # ── body ──
    do
        print "Building for {{target}}"
        shell "go build -o ./bin/{{target}}/myapp"
    end
end
```

### Config

| Surface | Effect |
|---|---|
| `description "x"`                 | Help text shown by `--help`. |
| `arg NAME TYPE "desc"`            | Declares a typed CLI flag. `TYPE` is one of `string`/`int`/`float`/`bool`. |
| `arg_default NAME VALUE`          | Default value for an arg (makes it optional). |
| `arg_index NAME N`                | Bind an arg to positional index `N`. |
| `arg_optional NAME`               | Mark arg optional even without a default. |
| `private`                         | Hide from CLI; only callable via `run` from another command. |
| `detached`                        | Don't wait on processes spawned by `shell_detached`. |
| `proxy_args`                      | Skip arg parsing; argv comes through as `{{proxy_args}}`. |
| `require_os "darwin" ...`         | Refuse to run on other OSes. Repeatable. |
| `require_arch "arm64" ...`        | Refuse to run on other architectures. |
| `dir "./subdir"`                  | Set the cwd for the body. |
| `on_signal HANDLER`               | Run `HANDLER` (another command) on SIGINT/SIGTERM. |
| `env KEY "value"`                 | Set an env var for the body's `shell` calls. |

### Body

```capy
do
    OP ARGS...
    let NAME = OP ARGS...        # capture an op's return value
    if_os "darwin"
        OP ARGS...               # nested ops, only run on macOS
    end
    run other_command            # dispatch into another command
    fail "explicit error"        # exit non-zero with a message
end
```

Inside the body:

- Every line is an **op call**. The op kind (`print`, `shell`, `mkdir`, …) is the first identifier; args follow.
- **`let NAME = OP ARGS`** runs the op and stores the return value under `NAME`. Subsequent strings interpolate via `{{NAME}}`.
- **Block ops** (`if_os`, `if_arch`, `if_exists`, `if_eq`, `if_gt`, `if_lt`) wrap a nested body that runs only when the condition holds.
- **`run NAME`** calls another command in the same file (private commands are callable here).

See the [op catalog](op-reference.md) for every built-in op.

## `catch NAME … end`

A fallback dispatched when the user types a command we don't have. The unknown name is bound to `NAME` inside the body.

```capy
catch unknown
    description "Help users who typo"
    do
        print "Unknown command: {{unknown}}"
        print "Try one of:"
        list_commands
        exit 1
    end
end
```

## Interpolation

`{{NAME}}` inside any string-valued op argument is resolved at runtime. Resolution order:

1. Command-local bindings: parsed arg values and `let` captures.
2. `globals` block.
3. Per-command `env` declarations.
4. Host process environment (so `{{HOME}}`, `{{USER}}`, `{{PATH}}` just work).

Unknown names produce an error at op-run time. To embed a literal `{{`, escape with `\{{`.

## Comments

`# ...` to end of line. Comment-only lines do not affect indentation.

## Reserved words

The DSL has *no reserved words*. `name`, `command`, `do`, `end`, `if_os`, etc. are just library-defined functions. You could rebind them by editing perch's [lib.capy](https://github.com/luowensheng/perch/blob/main/infra/capyloader/lib.capy) — and yes, that's the point of building on [capy](https://github.com/luowensheng/capy).

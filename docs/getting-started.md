# Getting started

A five-minute tour. By the end you'll have a working `commands.perch`, run it three different ways, and bundle it into a portable binary.

## Install

=== "Go"

    ```sh
    go install github.com/luowensheng/perch/cmd/perch@latest
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

This writes a starter `commands.perch`:

```capy
name    "hello-perch"
about   "A perch project"
version "0.1.0"

globals
    verbose = false
end

command hello
    description "Say hello"
    do
        print "Hello from ${HOME}"
    end
end
```

## Run it three ways

```sh
perch --help          # see what's in commands.perch
perch hello           # run the command
perch hello --help    # per-command help with args + defaults + examples
perch --check         # statically validate commands.perch before running
perch --shell         # REPL — type `hello` then Enter
perch --server        # serve a web UI at http://127.0.0.1:10032
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

## Ship it as a binary

```sh
perch --build -o ./greet
./greet                       # → Hello, WORLD!
./greet -name=Bob             # → Hello, BOB!
./greet --version             # → 0.1.0
scp ./greet remote.host:~/    # works on any same-OS box, no perch install needed
```

What just happened: `perch --build` copied itself, appended your parsed program as JSON + a magic footer, and produced a single file that boots straight into your commands. See [Embedding](embedding.md) for the format.

## What next

- [Language reference](language.md) — every config modifier, op, and block form.
- [Op catalog](op-reference.md) — the full list of built-in ops you can call.
- Browse the [demos folder](https://github.com/luowensheng/perch/tree/main/demos) for runnable examples.

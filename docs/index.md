# perch

> A cross-platform command runner. One `.capy` file → CLI, web UI, REPL, or a single portable binary you can ship.

`perch` collapses your Makefile, your CI workflow YAML, your `bin/` of bash scripts, and that helper CLI you keep meaning to write — into **one declarative file** that works on macOS, Linux, and Windows from day one.

```capy
command build
    arg         target string "Target OS"
    arg_default target "darwin"
    do
        shell "GOOS={{target}} go build -o ./bin/{{target}}/myapp ."
    end
end
```

```sh
perch build                    # → CLI
perch --server                 # → web UI of the same commands
perch --shell                  # → REPL
perch --build -o myapp         # → portable binary you can ship
```

## Reading order

1. **[Getting started](getting-started.md)** — install, scaffold, run something in 5 minutes.
2. **[Language reference](language.md)** — every keyword, modifier, and operator.
3. **[Op catalog](op-reference.md)** — the built-in "stdlib" of cross-platform operations.
4. **[Embedding (`--build`)](embedding.md)** — how the fat-binary feature works.
5. **[FAQ](faq.md)** — vs Make / Just / Task / shell scripts.

Source on GitHub: [luowensheng/perch](https://github.com/luowensheng/perch).

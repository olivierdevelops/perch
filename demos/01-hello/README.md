# demo 01 — hello

The simplest possible `commands.capy`. Two commands, one arg, one let-capture.

```sh
perch hello                  # → Hello, world!
perch hello -name=Alice      # → Hello, Alice!
perch sysinfo                # → host/user/home/now lines
```

## Concepts

- `globals` block holds shared values. `GREETING` is referenced from inside the command body as `{{GREETING}}`.
- `arg NAME TYPE "desc"` declares a typed CLI flag. `arg_default` gives it a fallback.
- `let X = OP ARGS` runs an op and captures its return value; subsequent ops use `{{X}}` to interpolate.
- Names that don't resolve to a global or arg fall back to the host process environment, so `{{HOME}}` and `{{USER}}` just work.

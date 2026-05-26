# Contributing to perch

Thanks for considering a contribution.

## Quick start

```sh
git clone https://github.com/luowensheng/perch.git
cd perch
go build -o perch ./cmd/perch
go test ./...
```

## Project layout

perch follows [VHCO](https://github.com/luowensheng/perch/blob/main/vhco-architecture.md): six top-level folders, each with a fixed role. The doc explains the rules; the `infra/ops/` subfolder is where most contributions land.

## Adding an op

The most common contribution. Two steps:

1. Add a Go handler in `infra/ops/<category>.go`:
   ```go
   m["my_op"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
       v := argString(args, "input", "_0")
       return strings.ToUpper(v), nil
   }
   ```

2. If the op is used as a statement (not just via `let`), add a matching capy function in `infra/capyloader/lib.capy`:
   ```capy
   function my_op
       arg literal "my_op"
       arg capture v string
       write `{"event":"op","kind":"my_op","args":{"input":${v}}}
   `
   end
   ```

3. Add a row to `docs/op-reference.md` and at least one test in `infra/ops/*_test.go`.

## PR guidelines

- One feature or fix per PR. Don't bundle.
- Run `go vet ./... && go test ./...` before pushing.
- For changes that affect surface DSL: update `docs/language.md` and `skills/perch/SKILL.md` in the same PR.
- For new ops: include a one-line example in `docs/op-reference.md` and a row in the catalog table.
- For new demos: drop a folder under `demos/` with `commands.perch` + `README.md` + the command(s) to run.

## Issues

- **Bugs:** include perch version (`perch --version`), OS, the failing `commands.perch` minimised to ~10 lines, and the actual vs expected output.
- **Op requests:** describe the use case, the proposed signature, and any prior art in other tools.
- **DSL changes:** open a discussion first — DSL surface changes ripple through docs, demos, the Claude skill, and external users.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).

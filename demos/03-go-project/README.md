# demo 03 — Go project

A complete build pipeline for a Go project. Side-by-side with what you'd write in a Makefile, it's shorter, OS-portable, and the same file drives both local dev and CI.

```sh
perch test                       # go test ./...
perch lint                       # vet + (optional) staticcheck
perch build -target=linux        # cross-compile to ./bin/linux/myapp
perch release                    # build all three targets
perch ci                         # lint + test + release
perch clean                      # delete ./bin
```

## Why this beats a Makefile

- `run other_command` is a real op that calls another command — no recursive-make tricks needed.
- `if_exists "…/staticcheck"` only invokes the linter if it's actually installed. No `command -v` boilerplate.
- One file describes both the dev workflow AND the CI workflow. Drop into `.github/workflows/`:

  ```yaml
  - run: perch ci
  ```

  That's the whole job. The matrix lives in your `commands.capy`, not your YAML.

## Concepts

- `globals` for cross-cutting paths and names.
- `run NAME` op for invoking other commands.
- `if_exists "PATH" … end` block op — skips the body if the path doesn't exist.

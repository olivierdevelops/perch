# demo 02 — cross-platform setup

One `commands.perch` that installs the same set of dev tools on **all three** major OSes.

```sh
perch setup     # → installs jq/ripgrep/watchexec on whatever OS you're on
perch which     # → shows where each binary landed
```

## Why this matters

In a Makefile you'd either:

- write three Makefiles (one per OS), or
- write one Makefile that breaks on Windows because `sudo` and `brew` don't exist there.

In perch you write the OS branches as **first-class structure**: `if_os "darwin" … end` is a block op the interpreter evaluates against the current host. Branches that don't match are skipped entirely.

## Concepts

- `if_os "NAME" … end` — conditional block op. Body runs only when `runtime.GOOS == NAME`.
- `if_arch "NAME" … end` — same idea for CPU architecture.
- Both work anywhere a regular op can go: inside `do`, inside other block ops, etc.

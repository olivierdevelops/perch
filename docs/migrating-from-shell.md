# Migrating from shell scripts

Most teams hitting perch already have `bin/*.sh`, a Makefile, a Justfile, or a `.github/workflows/*.yml` that's grown limbs. This page covers the three honest options for getting from there to a `.perch` file, ranked by effort.

You can stop at any rung. Each gives you real benefits over plain bash; each later rung gives you more.

---

## What stays the same

The first thing to know: **`.perch` files run as scripts**, the same way `.sh` files do.

```sh
chmod +x deploy.perch
./deploy.perch              # invokes the `main` command (if declared)
./deploy.perch up           # invokes a specific command
./deploy.perch --help       # lists commands
```

`perch --init` writes the shebang line (`#!/usr/bin/env perch`) at the top and sets the file executable. From your shell's perspective, a `.perch` is just another script — `./deploy.perch up` works identically to `./deploy.sh up`. No `perch` prefix needed once `perch` is on `$PATH`.

This matters because muscle memory is real. If your team types `./deploy.sh up` today, they'll type `./deploy.perch up` tomorrow without re-learning anything.

## TL;DR

| Option | Command | Effort | What you get | What you keep |
|---|---|---|---|---|
| **1. Wrap as-is** | hand-write a thin `.perch` calling `shell "./script.sh"` | minutes | typed args, `--help`, MCP, web UI, audit log, all five frontends | bash still runs underneath |
| **2. Translate** | `perch --import script.sh` | seconds + review | Above + structure + per-line auditability + path to restrictions | most lines are still `shell` ops |
| **3. Rewrite** | by hand, using `perch --check` to guide | hours per script | Above + cross-platform + `--no-shell` works (no bash at all) | full audit-friendly form |

---

## Option 1 — Wrap an existing script

Lowest effort. Keep the `.sh` files as-is; give perch a thin shim that hands the work to bash. You immediately gain typed CLI args, generated `--help`, the MCP / web UI / REPL frontends, the audit log, and the ability to fence the wrapper with `--no-network` etc.

```capy
name "deploy"

command deploy
    description "Deploy the app"
    arg target
        type string
        default "staging"
    end
    do
        shell "./scripts/deploy.sh ${target}"
    end
end

command logs
    description "Tail production logs"
    do
        shell "./scripts/logs.sh"
    end
end

catch passthrough
    description "Forward unknown commands to the legacy script"
    proxy_args                       # required to bind ${proxy_args}
    do
        shell "./scripts/run.sh ${proxy_args}"
    end
end
```

This is the right answer when:

- The shell scripts work today and you don't want to risk a rewrite.
- The team is comfortable with bash but wants a typed CLI surface over the top.
- You want to gradually migrate — wrap first, translate later, rewrite last.

The trade-off: `--no-shell` becomes meaningless (you need shell to run the wrapper), and you still maintain two layers. Most of perch's restrictions become advisory rather than enforced.

---

## Option 2 — Translate with `perch --import`

```sh
perch --import deploy.sh                 # writes deploy.perch beside it
perch --import deploy.sh -o cmds.perch   # explicit output path
```

Reads the `.sh`, produces a best-effort `.perch` scaffold, and prints next steps. The translator is **conservative** — it preserves bash semantics by wrapping most lines in `shell` ops. You can then promote them to native ops one at a time, with `perch --check` validating as you go.

### What it recognises

| Bash construct | Becomes |
|---|---|
| `#!/bin/bash` shebang | top-of-file comment |
| `# comment` | preserved comment in the body |
| `NAME=value` (top level) | `NAME = "value"` (bare, top level) |
| `function f { … }` or `f() { … }` | `command f ... do ... end end` |
| `echo "literal"` (no metachars) | `print "literal"` |
| `cmd &` (background) | `shell_detached "..."` |
| top-level executable lines | wrapped in a default `main` command |
| `$VAR` references | rewritten to `${VAR}` (perch interpolation; bash treats both the same) |
| anything else | `shell "..."` preserving the original line verbatim |

### What it does NOT do

- It does not parse bash control flow (`if`/`while`/`case`). Those land inside one `shell` op each and execute correctly — but they show up as opaque to perch's static analysis. Promote them to `if EXPR ... end` blocks when you're ready.
- It does not type-check arguments. `${1}`, `${2}`, `$@` pass through to `shell` as bash positionals. Add `arg NAME ... end` declarations where you want CLI-level typing.
- It does not unify duplicate logic. If you had two functions that mostly did the same thing, the translation has two commands; refactor with bare command calls when convenient.
- It does not pick the right quote delimiter when content has `"`, `'`, AND `` ` ``. Those lines get a `# TODO: fix quoting` comment.

### Round-trip example

Given:

```bash
#!/bin/bash
set -e

APP_DIR="/var/myapp"
LOG_FILE="$APP_DIR/log"

deploy() {
    echo "deploying to $APP_DIR..."
    cp ./build/* "$APP_DIR/"
    chmod +x "$APP_DIR/run"
    "$APP_DIR/run" &
}

# Default action
deploy
```

`perch --import deploy.sh` produces:

```capy
name "deploy"

APP_DIR = "/var/myapp"
LOG_FILE = "${APP_DIR}/log"

command deploy
    description "(imported from .sh — review me)"
    do
        print "deploying to ${APP_DIR}..."
        shell 'cp ./build/* "${APP_DIR}/"'
        shell 'chmod +x "${APP_DIR}/run"'
        shell_detached '"${APP_DIR}/run"'
    end
end

command main
    description "(imported from .sh — review me)"
    do
        shell "set -e"
        # Default action
        shell "deploy"
    end
end
```

Run `perch --check -f deploy.perch` — it passes. Run `perch -f deploy.perch deploy` — it works (identical semantics; `shell` runs your original lines).

Now you can iteratively promote:

```capy
# Before (one shell op):
shell 'cp ./build/* "${APP_DIR}/"'

# After (a native op — cross-platform, --no-shell-safe):
cp "./build" "${APP_DIR}"
```

The translator gives you the working starting point; your hands do the upgrade.

---

## Option 3 — Rewrite as native ops

The endgame. Every line becomes a native op (`cp`, `mkdir`, `http_get`, `sha256_file`, `tar_create`, …) — no `shell` involved. At that point:

- The file works identically on macOS / Linux / Windows. No bashisms.
- `--no-shell` is a real fence: any attempt to spawn a subprocess fails loudly.
- `--check` can statically verify every operation.
- The audit log shows op-level activity, not opaque shell-command strings.

This is the form for AI-agent surfaces, untrusted execution, and anything you'd hand to a reviewer who doesn't speak bash.

### Rewriting checklist

For each `shell` op in your translated file:

| If the line is… | Promote to… |
|---|---|
| `cp X Y` / `mv X Y` / `rm X` | `cp / mv / rm` ops |
| `mkdir -p X` | `mkdir "X"` (perch's `mkdir` is always recursive) |
| `chmod +x X` | `make_executable "X"` |
| `curl URL > FILE` | `download "URL" "FILE"` |
| `curl URL` (read body) | `let body = http_get "URL"` |
| `cat FILE` (read) | `let s = read_file "FILE"` |
| `echo X > FILE` | `write_file "FILE" "X"` |
| `echo X >> FILE` | `append_line "FILE" "X"` |
| `ln -s X Y` | `symlink "X" "Y"` |
| `tar czf OUT DIR` | `tar_create "DIR" "OUT"` |
| `unzip X` / `tar xzf X` | `zip_extract / tar_extract` |
| `sha256sum X` | `let h = sha256_file "X"` |
| `which X` / `command -v X` | `if has_bin "X"` / `let p = which "X"` |
| `apt install X` / `brew install X` | `pkg_install "X"` (auto-detects manager) |
| `if [ -f X ]; then …; fi` | `if exists "X" ... end` |
| `if [ "$x" = "y" ]; then …; fi` | `if x == "y" ... end` |
| `for x in *.txt; do …; done` | `let files = glob "*.txt"` + `for_each "${files}" x ... end` |
| `sleep N` | `sleep N` |
| `kill -9 $(pgrep X)` | `kill_by_name "X"` |
| `tool --flag arg` (a tool with no native op) | **`exec tool --flag arg`** — runs the binary directly, no shell. Bare flags work; quote only spaced args. Still cross-platform and injection-free. |
| `a \| b \| c` (a pipeline) | **`pipe ... end`** of `exec` stages — perch wires the pipes in-process, no shell (see below) |
| anything truly bash-specific | leave as `shell "..."` — and accept that this file will need `--allow-bin` to ship safely |

**Prefer `exec` over `shell` for the "no native op, but I just call a binary" case.** `exec git status` / `exec docker run -d --name web nginx` run the binary directly with structured argv — cross-platform, no metachar/injection surface, statically analyzable (the bin is structural, not buried in a string). `shell` is only needed for genuine shell features (`&&`, redirects, env-expansion-in-string). See [language.md](language.md) "shell vs exec".

Pipelines compose without a shell, too:

```perch
# bash:  docker ps -q | wc -l
let n = pipe
    exec docker ps -q
    exec wc -l
end
```

The pure line-toolbox (`grep` / `reject` / `cut` / `head` / `tail` / `sort_lines` / `uniq_lines` / `count_lines`) replaces the `grep`/`sed`/`awk` middle stages on captured output.

See [op-reference.md](op-reference.md) for the full catalog and [language.md](language.md#string-literals) for the quote-delimiter rule that makes JSON / SQL / shell-with-quotes painless.

---

## What about Makefiles, Justfiles, Taskfiles, GitHub workflows?

Future targets for `--import` (not shipped yet):

- **`perch --import Makefile`** — much easier to parse than bash; each rule maps cleanly to a command. Targeted next.
- **`perch --import justfile`** — even cleaner; Just's syntax is already command-shaped.
- **`perch --import Taskfile.yml`** — same shape.
- **`perch --import .github/workflows/ci.yml`** — convert each job to a command; the `if` / `with` / `env` maps reasonably well.

If you'd like one of these prioritised, open an issue on [GitHub](https://github.com/luowensheng/perch/issues) describing the file shape you'd want translated.

---

## When NOT to migrate

Be honest about the cases where bash is the right tool:

- **One-off scripts.** A 5-line file you'll run twice doesn't need typed args, an MCP surface, or an audit log. `bash one-thing.sh` is fine.
- **Heavy text-munging pipelines** (`awk` / `sed` / `grep` chained with pipes). perch now has a shell-free `pipe ... end` block + a pure line-toolbox (`grep`/`cut`/`head`/`sort_lines`/…) that cover most of these natively — but for a genuinely gnarly one-off `awk`/`sed` chain, the bash one-liner is still more concise, and `shell` runs it verbatim.
- **Scripts that ARE bash idioms.** Things using process substitution `<(…)`, named pipes `mkfifo`, complex `trap` chains. Perch's ops cover the common cases; the deep bash stuff is best left in bash.

Wrap (Option 1) those — keep the bash, gain the perch UX over the top. Don't translate or rewrite them.

---

## Summary

```
                      Effort →
                      
  Wrap         ─── existing .sh runs unchanged; perch is just the front
   ↓
  Translate    ─── perch --import; .perch with mostly `shell` ops; reviewable
   ↓
  Rewrite      ─── native ops; --no-shell becomes a real fence
                      
                      ← Cross-platform / sandbox-friendliness
```

Most teams land somewhere between Wrap and Translate, with a few critical paths reaching full Rewrite. That's a fine outcome — perch makes incremental migration the easy choice.

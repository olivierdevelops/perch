# Typed bin interfaces — declaring a binary like an FFI header (design)

> **Status: proposal / design only — not implemented.** This page specifies a
> feature, it does not document shipped behavior. Track the security context in
> [sandboxed-by-design § the subprocess trust boundary](sandboxed-by-design.md#the-subprocess-trust-boundary-honest-scope).

## 1. The problem this closes

A plain `bin "docker"` is an **opaque grant**: perch learns *docker may run*, but
nothing about *with what arguments*. Once perch `exec`s it, `requires read /
write / host` can't constrain the subprocess — perch never parsed its argv, so it
can't tell a path argument from a flag. That's the gap the
[capability-gating scope note](capability-gating.md) is honest about.

**The idea (yours):** treat a binary like a foreign-function interface. Instead of
"docker may run with anything," the author declares the *specific, typed calls*
they want to import from the binary. Because each argument now has a declared
**type**, perch can check it again — a `read`/`write` argument against the
filesystem scopes, a `host` argument against the declared hosts — *before*
spawning. And the binary becomes callable **only** through the declared interface,
so the attack surface is exactly the typed slots.

Not foolproof: the author can mistype a path as a plain string, and the binary can
still do whatever it wants internally. But it converts "opaque, zero enforcement"
into "typed surface, scope-checked arguments" — and it composes under an OS sandbox
later. It helps exactly when people are honest about their inputs.

## 2. Syntax — the same blocks you already use

No new punctuation, no `fn(x: T) -> ...`. A typed call is declared with the **same
`command` / `arg` / `type` / `do` blocks** as a top-level command — just nested
under a `bin` inside `requires`. The `do` body is the **argv** (the binary is
implied; you write the tokens that follow it).

```perch
name "deploy"

requires
    bin "docker"
        command up
            description "Bring a compose project up"
            arg file
                type read                    # ← scope type: a path that must be readable
                description "compose file"
            end
            do
                compose -f ${file} up -d     # argv: docker compose -f <file> up -d
            end
        end

        command run
            arg image
                type string
            end
            arg name
                type string
                default "app"
            end
            do
                run -d --name ${name} ${image}
            end
        end

        command fetch
            arg url
                type host                    # ← scope type: must match a declared host
            end
            do
                run --rm curlimages/curl -fsSL ${url}
            end
        end
    end

    read  "./deploy"
    write "./data"
    host  "registry.example.com"
end

command start
    do
        docker.up "./deploy/compose.yml"     # ✓ file ∈ read scope
        docker.up "/etc/evil.yml"            # ✗ read_not_declared — refused before spawn
        docker.run image:"nginx"             # name uses its default "app"
    end
end
```

Everything here already exists in perch: the `bin "…"` line, the `command NAME …
end` block, the `arg NAME … end` block with a `type` line, `default`,
`description`, and the `do … end` body. The only additions are **where** the
command block may appear (nested in a `bin`) and **three new `type` values**.

### Invocation — like a namespaced command

A declared call is invoked exactly like an imported, namespaced command
(`alias.command`): `BINALIAS.CALLNAME args…`. Arguments are positional or
`name:"value"` — the same call convention as commands and templates. The bin
alias comes from `bin "docker" as d` (defaults to the bare bin name, `docker`).

```perch
docker.run "nginx" "web"        # positional, by declared arg order
docker.run image:"nginx"        # named; name falls back to its default
```

## 3. The `type` vocabulary

The existing value types are unchanged; the proposal adds **scope types** that
carry an enforcement check:

| `type`            | Meaning                              | Check before spawn |
|-------------------|--------------------------------------|---------------------|
| `string`          | One literal argv token               | none (typed only)   |
| `int` / `float`   | Numeric token                        | parses as number    |
| `bool`            | `true` / `false`                     | parses as bool      |
| `enum "a" "b" …`  | One of a fixed set                   | value ∈ set         |
| **`read`**        | A filesystem path the call reads     | path ∈ `requires read`/`write` roots, else `read_not_declared` |
| **`write`**       | A filesystem path the call writes    | path ∈ `requires write` roots, else `write_not_declared` |
| **`host`** / **`url`** | A network endpoint              | host ∈ `requires host` set, else `host_not_declared` |

Every typed argument fills **exactly one argv token** — no word-splitting, no
shell metacharacters, no injection — identical to how `exec`/bare-bin argv already
works. So `docker.up "a b.yml"` passes one argument containing a space; it can
never become two tokens or inject a flag.

## 4. Enforcement semantics

1. **Lock-down (recommended default).** A bin that declares **one or more**
   commands is callable **only** through them. A bare `docker ps` (raw argv) for a
   bin that has a typed interface is a load error (`bin_interface_required`) — you
   declared a surface, so that's the whole surface. A bin with **no** declared
   commands keeps today's raw-argv behavior unchanged (full back-compat).
2. **Scope checks fire at call time**, after `${…}` interpolation, on every
   invocation — same model as the existing filesystem/host gates. A `read` arg
   whose interpolated value escapes every declared root refuses *before* the
   process is spawned.
3. **Argv is assembled structurally** from the `do` body: literal tokens are
   fixed, `${param}` tokens are filled from the typed arguments (one token each).
   The binary path is resolved from the `bin` line (alias / path / PATH lookup),
   exactly as today.
4. **The env scrub still applies** — a typed call's subprocess gets the same
   manifest-scrubbed environment as any other declared bin.

## 5. What `--check` can prove statically

Because the interface is typed and declared, `perch --check` (and `simulate`) gain
real reach into subprocess calls *without running them*:

- every `BINALIAS.call` resolves to a declared command (else "no such call");
- argument **count and types** match the signature;
- a **literal** `read`/`write`/`host` argument is checked against the scopes at
  check time (a `${var}` argument is deferred to runtime, like other interpolated
  gates);
- a bin with a typed interface is never invoked with raw argv.

## 6. The two bypasses — and the only real defense

Two honest objections, and what actually answers each. **Read this before
trusting the feature as a security control.**

### Bypass 1 — "I'll type a path as `string`, so it skips the scope check."

True, and **not fixable at perch's level.** perch can't know an arbitrary tool's
flag semantics, so it cannot *force* a path argument to be typed `read`/`write`.
That means **argument typing is a static-analysis + intent layer, not an
enforcement boundary.** The most perch can add on its own:

- **A `--check` path-shape lint** — warn when a `string` argument's *literal* value
  looks like a filesystem path (absolute, `./`, `~`, `..`, a known file extension)
  or a URL: *"this looks like a path — type it `read` to scope-check it."* This
  catches an honest author's mistake. It does **nothing** against a hostile author,
  and it can't see a path that arrives via `${var}` or a config file the tool reads.

### Bypass 2 — "The binary CRUDs dirs I never passed as arguments."

Also true, and worse: arguments don't even enter into it. Once `docker`/`git` is
running it can `open()` / `write()` / `unlink()` anywhere the OS user can, on its
own initiative — perch never sees those syscalls. **No amount of argument typing
touches this.**

### The one defense that answers both: OS-level confinement

The same `requires read / write / host` roots become an **OS sandbox profile**, and
the subprocess runs inside it:

- **macOS** — `sandbox-exec` with a generated seatbelt profile: deny `file*` by
  default, allow `file-read*` / `file-write*` only on the declared roots; deny
  `network*` unless a `host` is declared.
- **Linux** — **Landlock** (in-kernel, per-process filesystem allowlist, no helper
  binary) or **bubblewrap** (bind-mount only the declared roots) + a network
  namespace for the on/off bit.
- **Windows** — AppContainer / restricted token (best-effort).

Under that, the **kernel** enforces the scope on the *whole process* — every
syscall, every path, regardless of how an argument was typed or whether the binary
invented the path itself. A `string`-typed `/etc/passwd` is refused by the OS, and
so is the binary's own `open("/etc/shadow")`. **This is the enforcement boundary;
everything above it is intent.**

### So the layers compose — and only some enforce

| Layer | What it gives | Actually enforces? |
|---|---|---|
| `bin "…"` declaration | which binary may spawn | ✅ at spawn (`bin_not_declared`) |
| env scrub | no undeclared secrets in the subprocess env | ✅ |
| **typed interface** (this doc) | static-checkable call shape; scoped *intent*; a narrow, legible surface | ⚠️ honesty-dependent — **not** a boundary |
| **OS confinement** (roadmap) | the binary physically cannot touch undeclared paths/network | ✅ kernel-enforced |

Typed interfaces make a file's intent **legible and statically checkable**; OS
confinement makes it **true** even against a dishonest author or a binary acting on
its own. Ship them together. **Neither alone is a sandbox — and the typed interface
is the weaker, advisory half.** Anywhere this doc says "checked before spawn," read
it as "checked for the *declared* arguments"; the *process* is only truly bounded
once OS confinement lands.

- **Not every tool fits a small interface.** A bin you genuinely need to call many
  ways can stay a raw `bin "…"` (no commands) — you keep flexibility, you lose the
  arg-level *intent* checks. Under OS confinement that trade costs you nothing on
  the enforcement side: the kernel bounds the raw-args bin just the same.

## 7. Implementation sketch (when greenlit)

- **domain:** `BinReq` gains `Interface []BinCall`, where `BinCall` reuses the
  command shape (`Name`, `Args []ArgSpec`, `Ops []Op` — the `do` body is one argv
  op). `ArgSpec.Type` accepts the new scope types.
- **grammar (`lib.capy`):** allow a `command … end` block inside the `bin … end`
  block; add `read`/`write`/`host`/`url`/`enum` as recognized `type` values.
- **loader:** attach parsed `BinCall`s to their `BinReq`; register
  `BINALIAS.call` as a dispatchable namespaced name (reuse the import-namespacing
  path); enforce lock-down (raw argv on an interfaced bin → load error).
- **runtime:** a `BINALIAS.call` dispatch assembles argv from the `do` body +
  typed args, runs each scope type's existing check (`checkReadScope`,
  `checkWriteScope`, host gate) on the filled value, then spawns via the same
  `exec` path (env scrub included).
- **validate:** signature/arity/type checks + literal-scope checks at `--check`.
- **docs/tooling:** promote this page from proposal to reference; extend LSP /
  highlighter with the new `type` keywords.

## 8. Open questions

1. **Strictness default** — lock-down (a typed bin rejects raw argv) vs. additive
   (typed calls *and* raw argv coexist). Lock-down is the security win; additive is
   easier migration. Recommendation: lock-down, with raw-argv bins unchanged.
2. **Where the body's tokens may come from** — only literals + declared `${param}`?
   (Recommended: yes — no bare `${binding}` from outside the signature, so the
   argv shape is fully determined by the declared interface.)
3. **`enum` worth it for v1?** Small, high-value for flag-like args
   (`--format json|yaml`); could land later.
4. **Multiple bins, shared calls** — e.g. a `kubectl`/`oc` pair. Out of scope for
   v1; templates already cover cross-bin reuse at the command layer.

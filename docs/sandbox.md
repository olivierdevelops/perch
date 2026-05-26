# Sandboxing — a design document

**Status:** design proposal. Not yet implemented. Comments welcome on the [tracking issue](https://github.com/luowensheng/perch/issues).

This document is the spec for a capability-based sandbox layer on top of perch. The motivation is simple: the same `.perch` file we ship as a binary, expose as an MCP tool, or download from a stranger should have a way to declare *exactly* what it's allowed to touch — and we should be able to enforce that, both statically (via `--check`) and at runtime.

It also addresses the meta-question: **is perch a cross-platform shell?** Short answer: yes — and the sandbox makes that claim defensible.

---

## 0. What ships today: `--mode`

Before the full sandbox lands, perch ships **named modes** — opinionated subsets of the op catalog. One flag, no per-op declarations needed, no .perch-file changes. This is the Phase-0 mechanism.

```sh
perch --mode safe       run                  # no shell, no subprocess
perch --mode offline    run                  # no network ops
perch --mode read-only  run                  # no filesystem mutation
perch --mode pure       run                  # safe + offline + read-only
perch --modes                                # list all modes + what each blocks
```

When a blocked op is called you get a precise error:

```
op "shell" is disabled by --mode safe (see https://luowensheng.github.io/perch/sandbox/)
```

| Mode | What it blocks | When to use |
|---|---|---|
| `trusted` (default) | nothing | local dev, internal CLIs you wrote |
| `safe` | `shell`, `shell_output`, `shell_detached`, `shell_in`, `try_shell`, `pkg_install`, `pkg_uninstall`, `kill_by_name`, `process_running` | running a `.perch` from a teammate; serving to a non-engineer via `--server` |
| `offline` | every network-touching op (`http_*`, `download`, `dns_lookup`, `port_*`, `wait_for_*`, `public_ip`, `local_ip`, `mac_address`, `interfaces`) | airgapped CI; data-only scripts |
| `read-only` | every filesystem mutation op (`write_file`, `append_*`, `cp`, `mv`, `rm`, `mkdir`, `chmod`, `touch`, `copy_dir`, `symlink`, archive extract / create, `bundle_extract`/`dir`) | analysis scripts; report generators |
| `pure` | union of `safe` + `offline` + `read-only` | running a `.perch` from a stranger; the strictest preset |

Modes apply to **everything** the binary does — `perch --mode safe COMMAND`, `perch --mode safe --server`, `perch --mode safe --shell`. They also propagate into built binaries: `perch --mode safe -f my.perch CMD` runs `CMD` with shell denied; if you `--build` *with* shell calls, the build itself succeeds (build doesn't run the script), but `./mybinary --mode safe CMD` enforces the mode at run time.

Modes are the **80% answer**. The full sandbox below (env scopes, FS roots, network allowlists, resource ceilings) is the 100% answer.

---

## 1. Why we need this

### 1.1 The threat model

perch is interesting precisely because the same file drives many surfaces. Each surface has a different trust gradient:

| Surface | Caller | Trust | Risk |
|---|---|---|---|
| Local CLI | you | high | low — you wrote the file |
| Internal team CLI shipped as a binary | colleagues | high | low |
| Web UI (`--server`) | support team / non-engineers | medium | medium — clicks aren't audited |
| Recipient running a downloaded `--build` binary | strangers | low | high — they didn't write it |
| MCP server (`perch-mcp`) | AI agent | **none** | **high** — the agent is adversarial-by-construction |

Today, every surface gets the full op catalog. `shell "rm -rf ${HOME}"` works the same whether you invoke it or an LLM does. That's fine for case 1 and 2; it's unacceptable for cases 4 and 5.

### 1.2 What the existing safeguards already buy us

Before adding anything, let's be precise about what perch *already* enforces:

- **Op dispatch is the security boundary.** The interpreter calls Go handlers — there is no way to "escape" the op catalog. You can't `eval` a string into a new op.
- **Args are typed.** `arg port type int` means MCP / CLI / web all reject `port="; rm -rf /"`.
- **No verb undeclared = no verb callable.** An MCP agent calling `drop_database` when you never wrote that command gets a "not declared" error.
- **`--check` rejects unknown ops, unknown placeholders, malformed args, missing run targets, mismatched defaults.** Wired into pre-commit, it catches a class of bugs at PR time.

These are real properties. The sandbox layer extends them to cover the *contents* of each op call.

### 1.3 What we're missing

| Capability today | Caller can | Should be able to restrict |
|---|---|---|
| `shell "X"` | run any X | declare an allowlist of binaries |
| `http_get "URL"` | fetch any URL | declare an allowlist of hosts |
| `cp / rm / write_file PATH` | touch any path | declare read / write roots |
| `${ANY_ENV_VAR}` | read any process env | declare which env vars are visible |
| `kill_by_name N` | kill any process matching | turn off |
| `pkg_install N` | invoke `sudo apt …` | turn off (no privilege escalation) |
| Long-running shell | run forever | declare a max wall clock |
| Large downloads | fill disk | declare a max bytes-per-op |

The proposal below addresses each.

---

## 2. Is perch a cross-platform shell?

Yes — and the sandbox makes that claim sharper.

A "cross-platform shell" in this context means: **a language whose primitive operations are file / process / network / string actions, available identically on every host, that can be used to script tasks without falling back to `bash` or `cmd`.**

perch's ~110-op catalog already covers that surface: `cp`, `mkdir`, `rm`, `chmod`, `glob`, `read_file`, `write_file`, `append_line`, `download`, `http_get`, `regex_replace`, `sha256_file`, `tar_create`, `pkg_install`, `wait_for_port`, …. With `shell` op disabled (see §4.5), a `.perch` file is a *pure* cross-platform script — no bash, no cmd, no PowerShell. It runs identically on macOS / Linux / Windows.

What the sandbox adds:

- **`pure` mode** — a per-command modifier that forbids `shell`, `shell_output`, `shell_detached`, network ops, and writes outside an explicit root. The command is reduced to "structured cross-platform script."
- **`no_shell` sandbox option** — same idea at the file level.

So: yes, perch is a cross-platform shell. The sandbox lets you *prove* it is, for a given file.

---

## 3. The `sandbox` block — grammar

We add one new top-level block, parallel to `globals`. Sample:

```capy
name        "deploy"
about       "Roll out a release"
version     "0.5.0"

globals
    APP_NAME = "myapp"
end

sandbox
    # Op-level allowlist. If absent → all ops available.
    ops shell mkdir cp rm write_file http_get download sha256_file print

    # Env-var allowlist. If present, ${UNDECLARED} fails statically.
    env HOME PATH APP_VERSION RELEASE_TOKEN

    # Filesystem roots. Paths outside these are rejected.
    read  "./src" "${HOME}/.config/myapp"
    write "${cache_dir}/myapp"

    # Network allowlist. Patterns: host  or  host:port  or  *.example.com.
    net "api.github.com" "*.s3.amazonaws.com" "localhost:*"

    # Shell binary allowlist. The first word of every shell command must
    # match one of these. Combined with `no_shell_metachars`, this stops
    # bash one-liners doing arbitrary things.
    shell_bins go git docker
    no_shell_metachars      # forbid > | & ; $( ` in shell args

    # Resource ceilings.
    max_runtime 300         # seconds, wall-clock
    max_download 50MB
    max_file_size 100MB
    max_processes 8

    # Identity.
    no_sudo                 # fail if shell cmd starts with `sudo` (or pkg_install on linux)
end

command release
    description "Cut a release"
    do
        run build
        run publish
    end
end
```

Every field is **opt-in**. Omit `sandbox` entirely and behavior is unchanged from today. Include it and perch enforces every clause.

### 3.1 Field reference

| Field | Form | Effect |
|---|---|---|
| `ops X Y Z` | space-separated op kinds | Only these ops callable. Any other = static + runtime error. |
| `no_op X Y` | space-separated | Inverse — block these specific ops; everything else allowed. |
| `env A B C` | env var names | Only these env vars resolve via `${…}` fallthrough. Anything else = static error. |
| `read PATTERN …` | quoted paths, may contain `${var}` | Read-allowlist for file ops (read_file, exists, glob, file_size, cp src, …). |
| `write PATTERN …` | quoted paths | Write-allowlist (write_file, append_line, rm, mkdir, cp dst, …). `write` ⊆ `read` automatically. |
| `net "host[:port]" …` | host patterns | URL ops + dial ops check `URL.Host` / `host:port` against this. `*` wildcards permitted in a single label. |
| `shell_bins X Y` | binary names | First word of each `shell` command must match. |
| `no_shell_metachars` | (no args) | Reject shell args containing `|`, `>`, `>>`, `<`, `&`, `;`, `` ` ``, `$(`, `&&`, `\|\|`. |
| `no_shell` | (no args) | Forbid `shell`, `shell_output`, `shell_detached` entirely. |
| `no_subprocess` | (no args) | Same as `no_shell` plus `pkg_install` / `pkg_uninstall` / `kill_by_name`. |
| `no_sudo` | (no args) | Fail if a shell cmd begins with `sudo ` or a privilege-escalating pkg-mgr call. |
| `offline` | (no args) | Forbid all network ops (`http_*`, `download`, `dns_lookup`, `port_check`, `wait_for_url`, `wait_for_port`, `public_ip`). |
| `read_only` | (no args) | Forbid all write ops (`write_file`, `cp dst`, `rm`, `mkdir`, `mv dst`, `append_*`, `chmod`, `symlink`, `link_into_path`, `replace_in_file`, `backup_file`). |
| `max_runtime SECS` | int | Wall-clock budget for the invocation. Interpreter checks before each op. |
| `max_download BYTES` | int with K/M/G suffix | Cap per-call download size. |
| `max_file_size BYTES` | int with K/M/G suffix | Cap `write_file` / `append_file` output. |
| `max_processes N` | int | Cap concurrent spawned processes (`shell_detached`). |
| `private_ops` | (no args) | All ops accessing host facts (hostname, local_ip, mac_address, …) return empty rather than real values. Useful for hermetic tests. |

### 3.2 Per-command modifiers (further tightening)

Inside a command's config region (before `do`), you can add:

| Modifier | Effect |
|---|---|
| `pure` | `no_shell` + `offline` + `read_only` (most restrictive). |
| `offline` | Override the sandbox to make this command offline-only. |
| `read_only` | Override to make this command read-only. |
| `require_sandbox` | Fail to load if the file has no `sandbox` block. |

These can only **tighten**, never loosen. A command can declare `offline` even if the file's sandbox allows network. A command **cannot** add network access if the file forbids it.

### 3.3 Example: a minimal "untrusted" sandbox

For a `.perch` file you'd happily run from a stranger:

```capy
sandbox
    no_shell
    no_subprocess
    offline
    read  "${cwd}/input"
    write "${cwd}/output"
    env LANG TZ
    max_runtime 60
    max_file_size 10MB
end
```

This file can: read from `./input`, write to `./output`, use 60 seconds of wall clock, produce at most 10 MB per file. It cannot: shell out, hit the network, see env vars beyond LANG / TZ, touch anything outside `./output`. Run it without thinking.

---

## 4. Each restriction class in detail

### 4.1 Op-level allowlist (`ops` / `no_op`)

The simplest layer. The interpreter has a `Handlers map[string]Handler` that lists every op. With `sandbox ops X Y Z`, we mask the handler map at command-start time:

```go
if sb := prog.Sandbox; sb != nil {
    handlers = filterByAllow(handlers, sb.AllowedOps, sb.BlockedOps)
}
```

Unknown ops then return `"op X is not allowed by sandbox"`. `--check` catches them up front.

This is enough to neuter shell access (`no_op shell shell_output shell_detached`), neuter network (`no_op http_get http_post download …`), or whitelist a tiny safe set.

### 4.2 Env var visibility (`env`)

Today, `Bindings.Lookup` falls through to `os.LookupEnv` for any name not in bindings/env/globals. That means `${ANY_VAR}` reads the host process environment.

Under sandbox:
- The `env` clause declares the whitelist.
- The interpolation function is wrapped: `Lookup(name)` rejects un-whitelisted names with `"env var ${X} not declared in sandbox env"`.
- `--check` walks every literal `${…}` in op args + globals + on_signal handlers. Any name not in (autobound ∪ args ∪ globals ∪ env vars ∪ command env ∪ sandbox env) is flagged at validation time.
- Dynamic forms — `let n = format "${X}_${Y}" …; print "${n}"` — can't be statically caught fully, but the interpreter still enforces at runtime.

The autobound names (`os`, `arch`, `home`, `cache_dir`, `exe_path`, …) are unaffected — they're host facts, not env vars, and the sandbox controls them via `private_ops` separately.

### 4.3 Filesystem scope (`read` / `write`)

The hard case — paths are usually built dynamically.

Approach:

1. **Roots are interpolated once at startup** against the auto-bindings + globals + the host env vars that survived the `env` filter. So `read "${HOME}/.config/myapp"` is resolved to `/Users/you/.config/myapp` before any command runs.
2. **Each file-op handler wraps `resolve(path, b)` with `enforceFS(path, mode)`**. `mode` is `read` or `write`. The check resolves symlinks, then ensures the abs path is `filepath.Clean`-equal to or a descendant of at least one root.
3. **Reject `..` traversal at resolve time** so a user-supplied arg like `path:"../../etc/passwd"` is rejected even if the resolved abs path coincidentally falls inside a root.
4. **Static checks where possible** — if an op arg is a string literal with no `${…}`, `--check` evaluates the path now and reports any violation pre-runtime.

The `bundle_extract` op needs a special exception (or its destination must be inside `write`). Documenting both options.

### 4.4 Network scope (`net`)

Every network-touching op gets `URL.Host` (for `http_*`) or `host:port` (for `port_check`, `dial`, `wait_for_port`) extracted before sending bytes. The host is matched against the patterns:

- Exact match: `api.github.com`
- Single-label wildcard: `*.example.com` (matches `api.example.com`, not `a.b.example.com`)
- Host:port: `localhost:*`, `api.github.com:443`
- IP literal: `127.0.0.1`, `10.0.0.0/8`

`download URL DST` checks the URL host AND the destination path (against `write`).

`offline` is sugar for `net (nothing)` — every network op is blocked.

### 4.5 Shell access (`shell_bins` / `no_shell_metachars` / `no_shell`)

`shell` is the largest hole. We close it in layers:

- **`no_shell`** — the maximum restriction, op simply isn't available.
- **`no_shell_metachars`** — passes the shell command to the OS shell still, but pre-parses its tokens. If it sees pipes / redirects / command substitution / chains, we reject.
- **`shell_bins X Y`** — extracts the first token (after stripping leading env-var assignments like `GOOS=linux`) and checks against the list.

Combine all three for a tight policy: "you can call `git ARGS…` or `docker ARGS…`, no pipes, no redirects, no `$(…)`."

For the truly paranoid: `no_shell` + use only the structured cross-platform ops. This is "pure perch" mode.

### 4.6 Resource limits

Wall-clock and bytes are the two we care about.

- **`max_runtime`**: the interpreter starts a `context.WithTimeout`. Before each op dispatch, check `ctx.Err()`. For ops that block (shell, http_get), pass `ctx` so cancellation propagates.
- **`max_download`**: wrap the `http.Response.Body` reader in a `LimitReader`. Exceeding triggers a `download exceeds sandbox limit` error.
- **`max_file_size`**: similar wrap on `os.File.Write` path.
- **`max_processes`**: a semaphore in the interpreter. `shell_detached` acquires; long-running detached counts.

### 4.7 Identity / sudo

`no_sudo` rejects any shell command whose first non-env-assignment token is `sudo` or `doas`. It also rejects `pkg_install` on linux because the underlying invocation uses `sudo apt-get install …`. On macOS (`brew install` runs as user) it's allowed.

This is a soft fence — it isn't OS-level enforcement (the user could write `s=sudo; ${s} apt-get …`). The validator catches the easy cases; the sandbox isn't a kernel boundary.

### 4.8 Host fact privacy (`private_ops`)

Some ops leak host info: `hostname`, `local_ip`, `mac_address`, `public_ip`, `interfaces`, `user`, `uid`. With `private_ops`, these return empty strings / `false` instead of real values. Useful for hermetic builds and untrusted scripts that don't need this info.

---

## 5. Static enforcement via `--check`

The validator already walks every op's string args and resolves `${name}` placeholders against known names. The sandbox adds three new classes of static error:

1. **Undeclared env vars** — already described in §4.2.
2. **Forbidden ops** — every `Op.Kind` not in the allowlist (or in the blocklist) is flagged.
3. **Literal arg violations** — file ops with `_0 = "/etc/passwd"`, http ops with `URL = "https://attacker.com/…"`, shell with `cmd = "sudo …"`. When the arg is a string literal (no `${…}`), the validator can evaluate it now.

What the validator *can't* catch: paths/URLs/binaries built at runtime from interpolated strings. Those rely on the interpreter's runtime check. The validator should warn (not error) when it sees ops whose args contain `${…}` AND the file has a sandbox — "I can't statically verify this; the runtime will enforce."

---

## 6. Runtime enforcement

The interpreter currently does:

```go
func (i *Interpreter) RunOp(op domain.Op, b *Bindings) error {
    args, _ := InterpolateArgs(op.Args, b)
    h, ok := i.Handlers[op.Kind]
    ...
}
```

The sandbox wraps this:

```go
func (i *Interpreter) RunOp(op domain.Op, b *Bindings) error {
    if err := i.Sandbox.CheckBudget(); err != nil { return err }   // max_runtime
    if !i.Sandbox.OpAllowed(op.Kind) { return errOpDenied }
    args, _ := InterpolateArgs(op.Args, b)
    if err := i.Sandbox.CheckArgs(op.Kind, args); err != nil {
        return err  // FS scope, net scope, shell allowlist, env scope
    }
    h := i.Handlers[op.Kind]
    return h(i, b, args)
}
```

`CheckArgs` is a switch on `op.Kind` dispatching to per-op argument validators (one for file paths, one for URLs, one for shell cmds, …). All in `infra/sandbox/`.

**Interpolation hook.** `Bindings.Lookup` already exists; we attach a sandbox callback so unknown env vars fail with the sandbox error instead of an empty string.

---

## 7. Untrusted mode (`perch --untrusted`)

A CLI flag that:

1. **Refuses to run if the file has no `sandbox` block.** Forces explicit policy.
2. **Adds a permission preview before executing.** Prints, in human language, what the file will be able to do:

   ```
   This script wants to:
     • READ:  ./src, ~/.config/myapp
     • WRITE: ~/.cache/myapp
     • NETWORK: api.github.com, *.s3.amazonaws.com
     • SHELL: only `git`, `docker`
     • ENV: HOME, PATH, APP_VERSION
   Continue? [y/N]
   ```

3. **Caps unset limits at safe defaults**: `max_runtime 300`, `max_download 100MB`, `max_file_size 100MB`.

4. **Disables `private` commands** so the file can't run something the user can't see in `--help`.

The browser-permission analogy is intentional. perch becomes a safe runner for `.perch` files from strangers — the way `npx` should have worked.

---

## 8. CLI overrides

Following Deno's lead:

```sh
perch -f run.perch deploy \
    --allow-env=HOME,PATH \
    --allow-read=./src \
    --allow-write=./out \
    --allow-net=api.github.com \
    --no-shell
```

These are *additive* on top of the file's sandbox (you can never grant more than the file declared, but you can tighten further). Useful for CI: ship the file with a permissive sandbox; pin it tighter in CI.

`perch --allow-all` is the explicit opt-out (CI of trusted internal repos).

---

## 9. WASM — why not the primary lever

The user's intuition is reasonable — WASM is the modern sandbox primitive, and "compile `.perch` to WASM" sounds appealing. Three reasons not to make it the core mechanism:

1. **The security boundary is the op set, not the language runtime.** When a `.perch` file does `shell "rm -rf /"`, the *interpretation* of that op is in Go code that calls `os/exec`. Compiling the perch *script* to WASM doesn't fence the shell op — only fencing op dispatch does. To gain anything from WASM you'd have to compile the op handlers too, then deny them all but the safe ones — at which point you're back to op allowlisting, with extra steps.

2. **Most ops must cross the boundary anyway.** `shell`, `http_get`, `cp` all require host syscalls. WASI gives us capability-passed file descriptors, but that's the same model we're proposing here, expressed in a different language.

3. **The control flow is already structured.** The bytecode we'd compile to is already `[]domain.Op` walked by a Go interpreter. We don't gain isolation from re-expressing that as WASM bytes.

**Where WASM does help:**

- **User-defined ops via plugin modules.** If we let `commands.perch` declare `op my_widget` and load a `.wasm` module that exports it, WASI is a great way to give that module limited capabilities (read-only access to a directory; no network). The host (perch) acts as the runtime — it grants exactly the caps the sandbox block declares.
- **Distribution.** A perch file + a WASM op pack is portable in a way that a Go binary isn't.
- **Determinism.** WASM execution is deterministic in a way Go interpretation isn't (no goroutine scheduling, no GC timing).

So: WASM is on the roadmap, but for **user-defined ops**, not for the language runtime itself.

---

## 10. Migration / backward compatibility

- Files without a `sandbox` block continue to behave exactly as today. Zero breakage.
- `perch --untrusted` is a new flag, off by default.
- `perch --check` gains new error classes only when a sandbox block is present.
- The MCP server (`perch-mcp`) gains a `--require-sandbox` flag that refuses to serve files lacking one. (Recommended-on default in a future major version.)

---

## 11. Worked examples

### 11.1 A safe "downloaded from a stranger" script

```capy
name "format-photos"
about "Normalize a directory of photos"

sandbox
    no_shell
    no_subprocess
    offline
    read  "${cwd}/in"
    write "${cwd}/out"
    env LANG
    max_runtime 120
    max_file_size 20MB
end

command run
    do
        mkdir "${cwd}/out"
        # Loop, normalize, write. No shell, no network, no surprises.
        ...
    end
end
```

You receive this file from a colleague. `perch --untrusted -f format-photos.perch run` prints the permission preview, you say yes, it does its job. There is no version of this file that exfiltrates a secret or chmods your `~/.ssh`.

### 11.2 An MCP-served ops surface for an AI agent

```capy
name "ops"
about "Operations the AI agent can perform"

sandbox
    ops shell print eprintln http_get   # tiny op set
    shell_bins kubectl ssh
    no_shell_metachars
    net "*.k8s.internal" "*.our-company.com"
    env KUBE_CONFIG SSH_AGENT_AUTH_SOCK
    read  "${HOME}/.kube/config"
    write "/tmp/agent-${pid}"
    max_runtime 60
end

command restart_pod
    description "Restart a pod"
    arg ns      type string  description "namespace (matches ^[a-z0-9-]+$)" end
    arg pod     type string  description "pod name"  end
    do
        if not regex_match "${ns}" "^[a-z0-9-]+$"
            fail "invalid namespace"
        end
        shell "kubectl -n ${ns} delete pod ${pod}"
    end
end
```

Run as `perch-mcp -f ops.perch`. The agent has access to `restart_pod` (or anything else you declared); it has *no* access to anything outside the kubernetes scope. Even if it crafts an injection-style argument, the regex + shell allowlist + no-metachars combine to neutralize it.

### 11.3 An internal team CLI shipped as a binary

```capy
name "dev"
about "Our team's dev CLI"

sandbox
    ops shell print mkdir cp rm http_get download sha256_file pkg_install
    shell_bins go git docker make
    env HOME PATH GOPATH OPENAI_API_KEY
    read  "${HOME}/repo"
    write "${HOME}/repo" "${cache_dir}/dev-cli"
    net "api.github.com" "registry.docker.io" "*.docker.io"
    max_runtime 600
end

command up
    description "Start the dev stack"
    do
        shell "docker compose up -d"
    end
end

# … the rest of the CLI
```

This is a tight-but-realistic sandbox for an internal CLI. It can't `curl evil.com`. It can't read `~/.ssh`. It can't `chmod -R 777 /`. Reviewers can audit the sandbox block in one screen and know the upper bound of what the tool can do.

### 11.4 A pure cross-platform script (no shell at all)

```capy
name "extract-and-checksum"

sandbox
    pure
    read  "${cwd}/in"
    write "${cwd}/out"
end

command run
    arg input type string description "input archive" end
    do
        mkdir "${cwd}/out"
        tar_extract "${cwd}/in/${input}" "${cwd}/out"
        let files = list_dir "${cwd}/out"
        let h = sha256_file "${cwd}/in/${input}"
        write_file "${cwd}/out/SHA256SUMS" "${h}  ${input}\n"
        print "extracted: ${files}"
    end
end
```

`pure` means: no shell ops, no network ops, no writes outside `write` roots. This runs identically on macOS, Linux, Windows. No `bash` required. No `cmd` required. This is the "perch is a cross-platform shell" claim made operational.

---

## 12. Implementation sketch (not in scope of this doc)

For reviewers wanting to see where the code would live:

```
domain/sandbox.go              ← Policy struct + sub-policies
infra/sandbox/
    policy.go                  ← parse the `sandbox` block from capy events
    enforce.go                 ← OpAllowed, EnvAllowed, FSAllowed, NetAllowed
    interpolate_hook.go        ← wraps Bindings.Lookup
    runtime.go                 ← max_runtime ctx, byte counters, proc semaphore
    args.go                    ← per-op arg validators
infra/capyloader/lib.capy      ← new `sandbox` block function + sub-rules
usecases/validate/validate.go  ← static checks for env / ops / literal paths
io/cli/cli.go                  ← --untrusted, --allow-*, --no-shell flags
cmd/perch-mcp/main.go          ← --require-sandbox flag
```

Estimate: ~1500 lines + ~300 lines of tests. Not small, but well-bounded.

---

## 13. Open questions

- **`read` / `write` semantics for `cd`.** Should `cd "/tmp"` succeed if `/tmp` isn't in `read`? Probably yes — `cd` is metadata-only — but ops *inside* the new cwd then still need to pass the FS check against absolute paths.
- **Globs in `read` / `write`.** `read "${HOME}/*/.config"` is reasonable. The pattern matching should be eager (resolved at startup) and re-evaluated per op call (so newly-created dirs land in scope). Or static — both have trade-offs. Probably static + a `glob_extend` flag for the dynamic case.
- **Net allowlist for DNS-only ops.** `dns_lookup "example.com"` doesn't connect, but does leak the hostname to the resolver. Treat as a network op? I'd say yes — covered by `offline` / `net`.
- **Capability tokens.** Instead of `shell_bins git`, declare `cap git "git"` and require `shell.git "status"` syntax. More robust but more grammar to learn. Worth a follow-up RFC.
- **Effects in the type system.** `let h = sha256_file "X"` is pure; `let r = http_get "Y"` is impure. We could surface effects in op metadata so the sandbox classifies ops automatically. Probably overkill v1.
- **Per-arg policy.** `download URL DST` has different policy needs for URL (net) vs DST (write). Currently the enforce dispatch handles both; should arg-policy be declared per-arg in `infra/ops/<op>.go`? Probably yes.

---

## 14. Summary

| Layer | Mechanism | What it stops |
|---|---|---|
| **Op allowlist** | `sandbox ops …` / `no_op …` | Calling ops you didn't intend to expose |
| **Env scope** | `sandbox env …` | Reading host env vars you didn't declare |
| **Filesystem scope** | `sandbox read … write …` | Touching paths outside your project |
| **Network scope** | `sandbox net …` / `offline` | Talking to hosts outside an allowlist |
| **Shell scope** | `shell_bins`, `no_shell_metachars`, `no_shell` | Bash-injection style abuse via the `shell` op |
| **Resource ceilings** | `max_runtime`, `max_download`, `max_file_size`, `max_processes` | Resource exhaustion, runaway processes |
| **Identity** | `no_sudo` | Privilege escalation |
| **Privacy** | `private_ops` | Leaking host facts (hostname, IPs, MAC) |
| **Per-command** | `pure`, `offline`, `read_only` | Local tightening within an already-tight file |

With all of these:

- You can run a `.perch` from a stranger via `perch --untrusted` and know the upper bound on damage.
- You can serve a `.perch` to an AI agent via `perch-mcp` and the schema is genuinely the security boundary.
- You can ship a `.perch` to colleagues as a binary and the auditable sandbox block tells reviewers what the tool can do.
- **And perch becomes, defensibly, a cross-platform shell.**

Feedback welcome on the [tracking issue](https://github.com/luowensheng/perch/issues).

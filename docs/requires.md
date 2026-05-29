# `requires` — file-declared manifest

> **What this enables.** Your `.perch` file declares what the host machine must provide (bins, env vars, hosts, OS, arch). At startup perch verifies every required item up front; at runtime any *undeclared* shell binary, env-var read, or HTTP host is refused with a specific error kind. Static tools (`perch --check`, `perch simulate`) can prove a file is feasible on a target machine **without running a single user op**.

---

## TL;DR

```perch
requires
    bin "kubectl"
    bin "docker"  optional
    env "KUBECONFIG"
    env "DEBUG"   optional
    host "api.github.com"
    host "*.amazonaws.com"
    os   "linux"
    arch "amd64"
end

command deploy
    do
        exec kubectl apply -f manifest.yaml
        let body = http_get "https://api.github.com/repos/me/app"
    end
end
```

If you write `shell "curl ..."` without listing `curl` under `bin`, perch errors with `bin_not_declared` **before** the shell runs.

---

## Statement shapes

| Form                              | Meaning                                                                 |
|-----------------------------------|-------------------------------------------------------------------------|
| `bin "NAME"`                      | Required. Must exist on `PATH`.                                         |
| `bin "NAME" optional`             | Allowed; absence is not fatal.                                          |
| `bin "NAME" … hash … end`         | Required + a SHA-256 pin on the binary's bytes (see below).            |

### No version checking — pin the artifact instead

perch deliberately does **not** check binary versions. Verifying a version means *executing* the binary (or some probe) during preflight — before the sandbox is established — and a trojaned or PATH-shadowed binary will happily report whatever version satisfies your constraint. Asking a binary "are you safe?" and trusting its answer is not a security control.

The control that actually works is **content pinning**: declare the SHA-256 of the exact binary you trust. perch reads the bytes off disk and compares — **no execution, no self-reporting, pins the exact artifact**. A version you care about is really "the build I tested," and a hash names that build precisely.

```perch
requires
    bin "kubectl"
        hash "sha256:abc123…"      # the exact kubectl build you trust
    end
    bin "docker"                    # existence-only is fine for many tools
end
```

If you only need "a recent enough X" and can't pin a hash (e.g. the user installs their own), check the version *inside* a command with the normal ops — `let v = shell_output "kubectl version --client"` then `regex_match` / `assert_version` — where it runs under the declared `shell` capability and you can see and gate it explicitly. The manifest itself stays execution-free.

### Pinning the binary contents — `hash`

For supply-chain-sensitive workflows, pin the binary's SHA-256 directly. Preflight reads the resolved binary off disk and compares — no execution.

```perch
requires
    # Pin a specific kubectl build — defends against PATH-shadowing,
    # silently-upgraded package mirrors, and "rm /usr/local/bin/kubectl
    # && cp ./evil-kubectl /usr/local/bin/kubectl" supply-chain attacks.
    bin "kubectl"
        hash  "sha256:abc123def456..."
    end

    # Existence + hash is the whole contract — pin the exact build you trust.
    bin "internal-deploy-tool"
        hash "sha256:0123456789abcdef..."
    end
end
```

Format: `ALGO:HEXDIGEST`. Today only `sha256` is supported; the prefix is required so future algorithms (sha512, blake2b) can be added without ambiguity.

To compute a hash for an installed binary:

```sh
shasum -a 256 "$(which kubectl)"      # macOS / Linux (BSD shasum)
sha256sum "$(which kubectl)"          # GNU coreutils
```

A mismatch fires `requirement_unmet` with both expected and actual hashes in the message, so you can audit exactly what changed.

A bin may be existence-only (`bin "docker"`) or pinned (`bin "docker" / hash "sha256:…" / end`). The hash check reads the binary's bytes — it never executes it.

### Loading hashes from a file — `hash_file`

Hardcoding hashes in the `.perch` source works for small projects, but supply-chain audits usually want checksums in a separate, signable, machine-generated file. Use `hash_file` to load the hash from elsewhere:

```perch
requires
    # Load from a sibling file (resolved relative to the .perch source dir)
    bin "kubectl"
        hash_file "./checksums/kubectl-1.28.sha256"
    end
end
```

The file format is flexible — perch reads the first whitespace-delimited token:

```
# Bare hex (algo defaults to sha256)
abc123def456...

# Or with explicit algo prefix
sha256:abc123def456...

# Or raw `shasum -a 256` / `sha256sum` output — only the leading hex is used
abc123def456...  /usr/local/bin/kubectl
```

#### Embedding the hash file in the fat binary

The killer combo: declare the checksum file in `bundle`, reference it with `bundle:NAME` — now `perch --build` embeds the hash file in the output binary, and runtime preflight reads it without touching disk. Ship one tamper-evident artifact:

```perch
bundle
    include "./checksums/kubectl-1.28.sha256"
end

requires
    bin "kubectl"
        hash_file "bundle:kubectl-1.28.sha256"
    end
end
```

After `perch --build -o myapp`, the resulting `myapp` carries the checksum inside its tail. Running `myapp` on any host verifies the local `kubectl` against the embedded hash — no separate file to lose, no PATH-shadow surface for the checksum itself.

If both inline `hash` and `hash_file` are set, perch compares them and errors if they disagree — useful as a "did we update one but not the other?" guard.
| `env "NAME"`                      | Required. The env var must be set (non-empty).                          |
| `env "NAME" optional`             | Listed only so `get_env "NAME"` doesn't error with `env_not_declared`.  |
| `host "api.github.com"`           | Allow exact host for all HTTP ops.                                      |
| `host "*.amazonaws.com"`          | Wildcard suffix match (`*.X` matches any `Y.X`).                        |
| `host "..." optional`             | Same as above; future versions may treat optional hosts differently.     |
| `read "./src"`                    | A filesystem path the program may read. One per line, repeatable. Read ops outside every read (or write) root error with `read_not_declared`. |
| `write "./dist"`                  | A filesystem path the program may write. Write ops outside every write root error with `write_not_declared`. A write root implies read on the same tree. |
| `os "linux" / os "darwin"`        | Host OS allowlist. One value per line; multiple entries OR-combine. Special value `"unix"` matches any unix. Shares the bare `os` keyword with the `os "..." ... end` conditional block — disambiguated by indent lookahead. |
| `arch "amd64" / arch "arm64"`     | Host arch allowlist. One value per line; multiple entries OR-combine. |

### Filesystem scopes — `read` / `write`

The filesystem is an external resource like bins and hosts, so it's declared the same way. When the block is present, every filesystem op's path is checked against the declared roots — both statically by `perch --check` (for literal paths) and at runtime (after `${…}` interpolation):

```perch
requires
    read  "./config"           # read_file / stat / list_dir / … must stay inside
    write "./build" "${temp_dir}/myapp"   # mkdir / write_file / cp dst / … must stay inside
end
```

- A **write** op (`mkdir`, `write_file`, `append_file`, `cp` dst, `mv` dst, `rm`, `touch`, `chmod`, …) outside every `write` root → `write_not_declared`.
- A **read** op (`read_file`, `exists`, `list_dir`, `walk_dir`, `file_size`, `sha256_file`, `cp` src, …) outside every `read` *and* `write` root → `read_not_declared`.
- Paths are cleaned and made absolute (relative to the command's cwd) before matching; a root matches itself and anything nested under it. `..`/symlink escapes are a known sharp edge — the matcher is prefix-based, not a chroot.
- Interpolated paths (`mkdir "${BUILD_DIR}/${target}"`) are checked at **runtime** after resolution; literal paths are also flagged statically by `--check`.

---

## Enforcement model

When a `requires` block is present, three runtime checks fire on every command:

1. **Preflight** (once, before any op runs):
   - Every required `bin` must resolve via `exec.LookPath`.
   - Every `bin` with a `hash` / `hash_file` pin has its bytes read and SHA-256-compared (no execution).
   - Every required `env` must be set (non-empty).
   - Host `os` / `arch` must match the declared list.
2. **Per-shell-op**: the first token of every `shell` / `shell_output` / `shell_detached` command is checked against the declared `bin` list. Built-ins (`echo`, `cd`, `true`, `false`, `pwd`, `:`, `test`, `[`, `export`, `set`, `unset`) are always permitted.
3. **Per-HTTP-op**: the request URL's hostname is checked against the declared `host` list (exact or `*.suffix`).
4. **Per-`get_env`**: the requested env-var name must be declared.

Failures raise typed errors, matchable inside `try / rescue`:

| Kind                  | When it fires                                                  |
|-----------------------|----------------------------------------------------------------|
| `bin_not_declared`    | `shell "X ..."` where `X` isn't listed under `bin`.            |
| `env_not_declared`    | `get_env "Y"` where `Y` isn't listed under `env`.              |
| `host_not_declared`   | `http_* "https://Z/..."` where `Z` isn't listed under `host`.  |
| `requirement_unmet`   | Preflight failure (missing bin, hash mismatch, missing env, wrong OS/arch). |

---

## Capability flags vs. requirements

Declaring a requirement is **not** the same as granting capability:

- `requires bin "curl"` says "the file needs `curl` to work."
- `--allow-bin curl` says "this *invocation* lets perch shell out to `curl`."

If you run a `requires`-declared file inside a sandbox (`--no-shell`, `--allow-bin foo`), the sandbox still wins. Declarations are **promises about the program**; capability flags are **policies for the invocation**.

This split lets CI run a strict-sandboxed copy of the same file that the developer runs interactively.

---

## Static checking — catch undeclared use before running

Because the manifest is declarative, `perch --check` can verify usage **statically — without running a single op**. When a `requires` block is present, the validator walks every command body and flags any **literal** shell binary, HTTP host, or `get_env` name that isn't declared:

```sh
$ perch -f deploy.perch --check
error: deploy: shell uses bin "curl" which is not declared in `requires` (add `bin "curl"`)
error: deploy: http_get targets host "untrusted.org" which is not declared in `requires` (add `host "untrusted.org"`)
error: deploy: get_env reads "AWS_SECRET_ACCESS_KEY" which is not declared in `requires` (add `env "AWS_SECRET_ACCESS_KEY"`)
✗ deploy.perch: 3 errors, 0 warnings
```

This is the **static half** of the same enforcement the runtime does dynamically. Wire `perch --check` into CI and an undeclared `curl` fails the build the moment it's committed — before it ever reaches a machine that has secrets to leak.

**Dynamic args are deferred, not flagged.** A `shell "${cmd}"` or `http_get "https://${host}/x"` where the value is only known at runtime is **skipped** by the static check (no false positive) and enforced by the runtime guard instead. So you get the best of both: literal usage is caught at lint time; interpolated usage is caught at run time.

## Files without a `requires` block

Existing files (no manifest) keep their current behavior — undeclared shell bins are not blocked at runtime. The `requires` block is the opt-in switch.

A future release may flip the default to "strict-always" via a global flag; until then the manifest is your explicit signal that you want strict enforcement.

---

## See also

- [docs/errors.md](errors.md) — the `bin_not_declared` / `host_not_declared` / `env_not_declared` / `requirement_unmet` kinds in the full enum
- [docs/sandbox.md](sandbox.md) — capability flags (the complement of `requires`)

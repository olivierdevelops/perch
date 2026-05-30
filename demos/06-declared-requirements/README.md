# demo 06 — declared requirements

One `commands.perch` that **declares everything it needs from the host** — bins (with version floors and a custom version probe), env vars, and network hosts — in a single `requires` block. perch then enforces that manifest both statically (`--check`) and at runtime.

```sh
perch -f commands.perch --check     # prove the file is feasible — without running it
perch -f commands.perch versions    # git + go versions perch verified at preflight
perch -f commands.perch whoami       # reads the declared env var HOME
perch -f commands.perch fetch        # hits the declared host api.github.com
```

## Why this matters

In the AI-agent era, the bottleneck isn't "is this code correct" — it's "what can this thing touch?" A `requires` block answers that question in a form both a human and a machine can check in seconds. The trust unit shifts from *who wrote the code* to *what the code declares it needs*.

With the block present, perch runs in **strict mode**:

| Undeclared use | Result |
|---|---|
| `shell "curl …"` where `curl` isn't declared | `bin_not_declared` |
| `http_get "https://evil.com"` to an undeclared host | `host_not_declared` |
| `get_env "AWS_SECRET_ACCESS_KEY"` not declared | `env_not_declared` |
| Declared bin missing / hash mismatch / wrong OS | `requirement_unmet` |

## The manifest in this demo

```perch
requires
    bin "git"                      # existence verified at preflight
    bin "echo"
    bin "go"
    bin "jq" optional              # absence is not fatal
    env "HOME"                     # required env var (must be set)
    env "DEBUG" optional
    host "api.github.com"          # the only allowed HTTP destination
end
```

## Concepts

- **`requires … end`** — the file-declared manifest. See [docs/requires.md](https://olivierdevelops.github.io/perch/requires/).
- **No version checking** — perch does NOT run a binary to check its version. That would mean executing it before the sandbox exists, and a trojaned binary lies about its version. Pin the artifact instead (below).
- **Hash pinning** — add `hash "sha256:…"` (or `hash_file "bundle:checksums/git.sha256"`) inside a `bin … end` block to pin the binary's exact bytes. perch reads the file and SHA-256-compares — no execution. Not shown in the runnable file because hashes are host-specific; see [docs/requires.md](https://olivierdevelops.github.io/perch/requires/).
- **Static check** — `perch --check` flags undeclared *literal* usage before you run anything. Interpolated args (`shell "${cmd}"`) are deferred to the runtime guard, so there are no false positives.

## Try the failure modes

Add a command that reaches outside the manifest and watch `--check` refuse it:

```sh
# Append a command that does `shell "curl https://x"` and:
perch -f commands.perch --check
# → error: ...: shell uses bin "curl" which is not declared in `requires`
```

## See also

- [docs/requires.md](https://olivierdevelops.github.io/perch/requires/) — full reference
- [docs/trust-by-manifest.md](https://olivierdevelops.github.io/perch/trust-by-manifest/) — the roadmap for declared-capability WASM modules
- [docs/errors.md](https://olivierdevelops.github.io/perch/errors/) — the `*_not_declared` / `requirement_unmet` error kinds

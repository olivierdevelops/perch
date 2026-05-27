# `perch simulate` — what would this program do on *that* host?

> The missing third tool in perch's pre-flight suite.
>
> | Tool | Inputs | Output | What it answers |
> |---|---|---|---|
> | `perch --check` | source | pass/fail | "Is the syntax valid?" |
> | `perch --scan` | source | capability report | "What capabilities does it need overall?" |
> | **`perch simulate`** | source + **hypothetical env** | per-op outcome tree | "What would happen if I ran this on a host with THESE properties?" |
> | `perch --dry-run` | source + real env | op-list (no execution) | "What ops would fire right now?" |
> | `perch test` | source + real env | pass/fail per test | "Does behavior match assertions?" |
> | `perch <cmd>` | source + real env | the actual output | "What happens when I run it?" |

## TL;DR

```sh
$ perch simulate deploy --sim-os=linux --sim-have-bin=kubectl \
                        --sim-allow-host=api.github.com \
                        --sim-fs-write=/srv

── command deploy — Apply prod manifests
✓ print "==> deploy starting"
✓ if os eq linux
   ↳ condition os eq "linux" evaluates TRUE (sim os="linux") — body runs
   ✓ shell "kubectl apply -f manifest.yaml"
✓ if os eq darwin
   ↳ condition os eq "darwin" evaluates FALSE (sim os="linux") — body skipped
? http_get "https://api.github.com/repos/foo/bar"
   • server at "api.github.com" could redirect to any host
     perch re-checks every redirect against the allowlist;
     this op succeeds if redirects stay within the allowlist
✗ write_file "/etc/passwd"
   ↳ write path "/etc/passwd" is outside sim --allow-write roots (allowed: /srv)
? write_file "${HOME}/notes.txt"
   ↳ target path not statically determinable

summary: 5 will-run · 1 will-fail · 2 uncertain
1 op(s) would fail under the simulated environment
```

Exit code 0 if every op would run; non-zero if any op definitively fails. Drop into CI as a pre-merge gate.

## Why this exists

`--scan` is static: it tells you *what capabilities* the program needs, but not whether your target host actually has them. `--dry-run` requires you to be ON the target host. `simulate` is the missing piece: **answer "would this work?" without leaving your laptop and without running anything.**

Concrete uses:

- **Compliance reviewer**: "If this script runs in the prod environment (no `/etc` writes, only the corporate registry, only these env vars), what would it actually do?"
- **CI guard**: "Refuse to merge a PR if `simulate` reports any `WILL_FAIL` under our standard prod env."
- **Migration planning**: "We're moving from macOS to Linux runners — what breaks in our build pipeline?"
- **Plugin acceptance**: "Customer submitted a `.perch` extension; simulate it under our strictest sandbox before we accept it."
- **Onboarding**: "What env vars does a fresh dev's machine need? Run `simulate` with `--sim-env-only=HOME` and see what reports as missing."

## Outcome classification

Every op gets one of three verdicts:

| Glyph | Outcome | Meaning |
|---|---|---|
| **✓** | `WILL_RUN` | Every check the simulator can perform passes against the sim env. |
| **✗** | `WILL_FAIL` | At least one check definitively fails. Exit code reflects this. |
| **?** | `MIGHT_FAIL` | Outcome depends on runtime data the simulator can't statically know — values inside `${var}`, server-side HTTP redirects, computed shell argv, etc. Reasons + scenarios are listed. |

The summary line at the end reports the aggregate counts.

## CLI surface

```sh
perch simulate [COMMAND] [SIM FLAGS]
```

`COMMAND` is the command name to simulate. If omitted, simulates every public command.

### Sim flags

Each is independent. Omitting a flag means "no restriction in that dimension."

| Flag | Effect | Example |
|---|---|---|
| `--sim-os OS` | Pretend the host is OS | `--sim-os=linux` |
| `--sim-arch ARCH` | Pretend the host arch | `--sim-arch=arm64` |
| `--sim-env K=v,K=v,...` | Set sim host env vars | `--sim-env=HOME=/home/x,USER=x` |
| `--sim-env-only` | Use with `--sim-env`: restrict envs to ONLY listed names | with above: any `${OTHER}` fails |
| `--sim-fs-read PATH,...` | Sim has these paths readable | `--sim-fs-read=/srv,/etc` |
| `--sim-fs-write PATH,...` | Sim allows writes under these | `--sim-fs-write=/tmp,/srv/data` |
| `--sim-have-bin NAME,...` | Sim has these on PATH | `--sim-have-bin=docker,kubectl` |
| `--sim-allow-host HOST,...` | Sim network allowlist | `--sim-allow-host=api.github.com,*.s3.amazonaws.com` |
| `--sim-no-shell` | Sim equivalent of `--no-shell` | (boolean) |
| `--sim-no-subprocess` | Sim equivalent of `--no-subprocess` | (boolean) |
| `--sim-no-network` | Sim equivalent of `--no-network` | (boolean) |
| `--sim-no-write` | Sim equivalent of `--no-write` | (boolean) |
| `--sim-file PATH` | JSON fixture with capabilities + oracles + scenarios (see "Stateful simulation" below) | `--sim-file=fixtures/staging.json` |

All flags compose. They mirror perch's runtime `--no-*` / `--allow-*` / `--env` flags so you can simulate exactly the invocation you plan to run.

## What the simulator catches

### Capability mismatches

`shell "kubectl ..."` when the sim env doesn't have `kubectl` in `--sim-have-bin`:

```
✗ shell "kubectl apply -f manifest.yaml"
   ↳ shell binary "kubectl" not in sim --allow-bin allowlist (have: docker, git)
```

### Sandbox-style flags

`shell` when the sim env declares `--sim-no-shell`:

```
✗ shell "echo hello"
   ↳ shell capability denied by sim --no-shell
```

### Write outside allowed roots

```
✗ write_file "/etc/passwd"
   ↳ write path "/etc/passwd" is outside sim --allow-write roots (allowed: /srv)
```

### Network host violations

```
✗ http_get "https://attacker.com/exfil"
   ↳ HTTP host "attacker.com" not in sim --allow-host allowlist (have: api.github.com)
```

### Env var visibility

With `--sim-env-only` plus `--sim-env=HOME=/x`:

```
✗ shell "deploy --token=${API_TOKEN}"
   ↳ references ${API_TOKEN} but sim --env restricts host envs to HOME
```

### Conditional branches resolved against the sim env

```
✓ if os eq linux
   ↳ condition os eq "linux" evaluates TRUE (sim os="linux") — body runs
   ✓ shell "apt-get install jq"

✓ if os eq darwin
   ↳ condition os eq "darwin" evaluates FALSE (sim os="linux") — body skipped
```

The simulator **doesn't waste your time** showing failures inside branches that would never run.

### Predicate calls

`if exists "PATH"` is checked against `--sim-fs-read`; `if has_bin "X"` against `--sim-have-bin`. The body simulates only if the predicate would evaluate true.

### MIGHT_FAIL with reasons

When the simulator can't reach a definite verdict:

```
? shell "${BUILD_CMD}"
   • argv[0] = "${BUILD_CMD}" (contains unresolved interpolation)
     value depends on runtime bindings
```

```
? http_get "https://api.github.com/foo"
   • server at "api.github.com" could redirect to any host
     perch re-checks every redirect against the allowlist;
     this op succeeds if redirects stay within the allowlist or there are no redirects
```

## Composition — sandbox / cache / parallel blocks

Each block-op modifies the simulated environment for its body. `sandbox` narrows capabilities; `with_env` adds env vars; both compose with the outer sim env.

```
✓ sandbox "no_shell,no_network"
   ✗ shell "echo hi"
      ↳ shell capability denied by sim --no-shell (within sandbox block)
   ✓ print "still works — no shell needed"
```

## Cross-command dispatch

`run other_command` recurses — the simulator follows the call and simulates that command's body too, with the same sim env. Useful for catching that `run setup` depends on `--sim-have-bin=brew` even if the parent command looks fine.

## Stateful simulation, oracles, and scenarios

Pure capability-mode (`--sim-*` flags only) answers *"can each op run in this environment?"* It does NOT answer:

- *"What if the file exists after the previous step?"*
- *"What if HTTP returns 500?"*
- *"What if `git rev-parse HEAD` returns this specific value?"*
- *"What if the upstream redirects to a host I haven't allowlisted?"*

For those, point `simulate` at a JSON **fixture file** with `--sim-file FIXTURE.json`. The fixture declares capabilities + **oracles** (concrete simulated outputs for ops the static walker can't otherwise resolve) + named **scenarios** (override sets that branch the simulation).

### Fixture file shape

```json
{
  "os": "linux", "arch": "amd64",
  "env": {"HOME": "/h", "PATH": "/usr/bin"},
  "fs_write": ["/tmp"],
  "bins":    ["git", "curl"],
  "network": ["api.github.com"],

  "oracles": {
    "file_exists":  {"/tmp/manifest.yaml": true},
    "shell_output": {"git rev-parse HEAD": "abc123"},
    "http":         {"https://api.github.com/health": {"status": 200, "body": "OK"}},
    "has_bin":      {"kubectl": true}
  },

  "scenarios": [
    {"name": "happy",      "overrides": {}},
    {"name": "github-down","overrides": {
      "http": {"https://api.github.com/health": {"status": 500, "body": "down"}}
    }},
    {"name": "redirect-evil","overrides": {
      "http": {"https://api.github.com/health": {"status": 302, "redirect": "https://evil.com/payload"}}
    }}
  ]
}
```

### What the stateful walker does

- **State threads through ops.** `write_file "/tmp/x"` records the file as existing; downstream `if exists "/tmp/x"` evaluates true. `rm` flips it back. `cd /srv` shifts the cwd used to resolve relative paths.
- **`let` captures consult oracles.** `let rev = shell_output "git rev-parse HEAD"` looks up the post-interpolation command in `oracles.shell_output`. If present, `${rev}` resolves to the simulated value; if absent, `${rev}` is marked *symbolic* and downstream uses surface MIGHT_FAIL.
- **HTTP outcomes are oracled per URL.** Status 2xx → WILL_RUN; 4xx/5xx → WILL_FAIL with the simulated body; 3xx with a `redirect` field → MIGHT_FAIL, and if the redirect destination's host isn't in your network allowlist → WILL_FAIL (the practical answer to "what if upstream redirects me to evil.com?").
- **`has_bin` oracles override the capability list.** Lets you simulate "what if `kubectl` is suddenly missing?" without removing it from `bins`.

### Scenarios

Each entry in `scenarios` runs as one independent walk with its own report, sharing the top-level capabilities + oracles but overlaying the scenario's `overrides`. Empty `scenarios` is treated as one implicit `default` scenario. Per-scenario `env` overrides let you tweak the environment too — e.g. *"what if `GITHUB_TOKEN` is missing in this scenario?"*

### Running it

```sh
perch -f commands.perch simulate release --sim-file fixture.json
```

Each scenario produces a banner, then the per-op report. Exit code is non-zero if any scenario reports a failure — drop straight into CI as a multi-environment gate.

CLI `--sim-*` flags layer on top of the fixture (CLI wins on conflict), so you can combine `--sim-file env.json --sim-no-network` to force-disable network for a one-off without editing the fixture.

## What the simulator does NOT catch (yet — roadmap)

- **Symbolic branching on unknown conditions** — if a condition depends on a runtime value that has no oracle, the simulator presents the body as "MIGHT run." It doesn't enumerate "if X were Y, this branch fires; if X were Z, that branch fires."
- **`wasm_run` body deep analysis** — the simulator notes the module path but doesn't simulate the WASM module's behavior (the module sees a tighter capability surface by construction; that's `wasm_run`'s point).
- **Counterfactual suggestions** — e.g. "Add `api.github.com` to your `--allow-host` to make this pass" is a future idea.
- **Persistent state between scenarios** — each scenario starts from a fresh state seeded from the top-level fixture. Chained scenarios ("start in state from scenario A, then run B") are a future idea.

Each of these gracefully degrades to `MIGHT_FAIL` with a reason explaining what the simulator couldn't resolve. Honest, not lossy.

## How `simulate` differs from related tools

### vs `perch --scan`

`--scan` is **schema-shaped**: it produces a capability summary ("needs shell, hits these hosts, writes these roots, uses these env vars") plus a list of static risk findings. It doesn't take an env — it tells you what you'd need to provide.

`simulate` is **per-op**: it takes the env you'd provide and walks every op telling you exactly which would succeed, fail, or branch. Use `--scan` to know what's needed; use `simulate` to test specific scenarios.

### vs `perch --dry-run`

`--dry-run` shows the plan **on the current host**. It honors the real `os`, real `${HOME}`, real files-on-disk. Useful when you're at the keyboard of the target host.

`simulate` takes a **hypothetical** host. Useful when the target isn't your machine — CI, a customer environment, a future-state migration target.

### vs `perch test`

`perch test` actually runs commands marked `test` in a sandboxed cwd. It catches behavioral bugs.

`simulate` doesn't run anything. It catches capability/structural mismatches before you spend test cycles.

You want both: `simulate` as a pre-merge gate ("this won't break in prod"), `perch test` as a behavioral gate ("the logic is correct").

## CI integration

```yaml
# .github/workflows/predeploy.yml
- name: Simulate deploy against prod env
  run: |
    perch -f deploy.perch simulate deploy \
      --sim-os=linux --sim-arch=amd64 \
      --sim-env-only=KUBECONFIG,PATH \
      --sim-have-bin=kubectl,helm,docker \
      --sim-allow-host=*.acme.com \
      --sim-fs-write=/srv/deploy \
      --sim-no-subprocess
```

Exit non-zero on any `WILL_FAIL` → PR blocked.

For uncertainty: optionally fail on `?` outcomes too with `--strict` (roadmap — not in v1). Today, `?` is informational; only `✗` triggers a non-zero exit.

## See also

- [`docs/sandbox.md`](sandbox.md) — the capability model the simulator mirrors
- `perch --scan -f FILE` — the static cousin (no env input; produces a capability summary). Run it as a CLI command; there's no separate doc page yet.
- [`docs/execution-contexts.md`](execution-contexts.md) — the block ops (`sandbox`, `parallel`, etc.) the simulator recurses into

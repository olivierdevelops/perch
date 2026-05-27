# `wasm-policy-check` — deterministic compliance gate for CI

Pre-deploy policy enforcement compiled to WebAssembly. Five baked-in
policies for Kubernetes manifests; extending the catalog is one Go
function each. Replaces the 60-line bash + `grep` + `yq` + `jq`
pre-deploy gate every platform team writes from scratch.

## What it checks (out of the box)

| Policy | Catches |
|---|---|
| **`registry-allowlist`** | Containers from non-approved registries |
| **`resource-limits`** | Pods without explicit CPU + memory `limits` |
| **`no-latest-tag`** | Images pinned to `:latest` (or unpinned) |
| **`no-privileged`** | `securityContext.privileged: true` |
| **`required-labels`** | Missing `metadata.labels.team` / `.env` |

Extend by adding a `policy*` function to `main.go` — `runPolicies()`
is the central dispatch.

## Try it (no install — just perch)

```sh
$ perch -f commands.perch demo_good
✓ /ro/deploy/api-good.yaml: policies passed

$ perch -f commands.perch demo_bad
✗ [no-latest-tag] /ro/deploy/api-bad.yaml
  at  container "api"
  got docker.io/some-random-user/api:latest
  want a pinned tag (e.g. registry/image:v1.2.3)

✗ [no-privileged] /ro/deploy/api-bad.yaml
  at  container "api".securityContext.privileged
  got true
  want false or omitted

✗ [registry-allowlist] /ro/deploy/api-bad.yaml
  at  container "api"
  got docker.io/some-random-user/api:latest
  want one of: ghcr.io/, registry.company.internal/, docker.io/library/

✗ [required-labels] /ro/deploy/api-bad.yaml
  at  metadata.labels
  got missing
  want label team

…

6 violation(s) across 1 file(s)
```

Exit code 1 on any violation — drop into pre-deploy CI directly.

## Bundle into a single binary

```sh
$ perch --build -f commands.perch --include . -o policy-tool
$ ./policy-tool enforce
✓ /ro/deploy/api-good.yaml: policies passed
# (api-bad.yaml is in the demo set — see violations above)
```

One ~14 MB binary; recipients run it; no Python, Go, or perch install
required. The `.wasm` is inside the binary.

## Why deterministic policy enforcement is hard with shell

The shell version of these checks looks like:

```bash
# Naive "no privileged" check — misses YAML edge cases, multi-doc files
if grep -rE "privileged:\s*true" deploy/; then
    exit 1
fi

# Naive "registry allowlist" — breaks on multi-line image refs,
# misses spec.template.spec.containers vs spec.containers
yq '.spec.containers[].image' deploy/*.yaml | grep -v "^ghcr.io"
```

Issues:
- Each policy is its own bash idiom
- Edge cases (multi-doc YAML, comments, alternate spec shapes) accumulate
- `yq` / `jq` versions vary by machine
- No structured output for downstream pipelines

The WASM version:
- One `.wasm` artifact, sha256-pinnable
- Proper YAML parser (gopkg.in/yaml.v3) → multi-doc + comments work correctly
- Identical output on every host
- Each policy is ~10-20 lines of Go, trivially reviewable

## What the module sees (capability boundary)

When `wasm_run` instantiates this module:

```
argv:     ["policy-check.wasm", "/ro/deploy"]
env:      empty — no host env declared
fs read:  /ro/deploy → host's ./deploy directory
fs write: none — module cannot write anywhere
network:  none — cannot exfiltrate
shell:    none — cannot syscall
```

Even a deliberately malicious policy module (loaded from a community
registry, say) literally **cannot** call `~/.ssh/config` or HTTP-POST
your manifests to an attacker. The WASM runtime makes those operations
not exist in the module's reality.

## Commands available

| Verb | What |
|---|---|
| `enforce` | Run policies against ./deploy (CI gate) |
| `enforce_dir DIR` | Run policies against an arbitrary directory |
| `demo_good` | Pass case — every policy honored |
| `demo_bad` | Fail case — every policy violated |
| `rebuild` | Recompile policy-check.wasm |

## CI integration

```yaml
# .github/workflows/predeploy.yml
- run: ./policy-tool enforce_dir manifests/
```

The exit code propagates. PRs that violate policy can't merge.

## Extending the policy catalog

Add a function in `main.go`:

```go
func policyNoHostNetwork(file string, doc map[string]any) []Violation {
    var out []Violation
    for _, c := range containers(doc) {
        if hn, _ := nested(doc, "spec", "hostNetwork").(bool); hn {
            out = append(out, Violation{
                File: file, Policy: "no-host-network",
                Where: "spec.hostNetwork", Got: "true",
                Want:  "false or omitted",
            })
            break
        }
    }
    return out
}
```

Then register it in `runPolicies()`. Rebuild:

```sh
$ perch -f commands.perch rebuild
✓ rebuilt policy-check.wasm (4.2 MB)
```

Re-test:

```sh
$ perch -f commands.perch demo_bad
…now also reports no-host-network if applicable…
```

## See also

- [`docs/wasm.md`](../../docs/wasm.md) — `wasm_run` reference
- [`docs/wasm-walkthroughs.md`](../../docs/wasm-walkthroughs.md) — Walkthrough 5 covers this CI-gate pattern in depth
- [`demos/wasm-schema-validator/`](../wasm-schema-validator/) — sibling demo for JSON Schema validation
- [`recipes/scan-secrets.perch`](../../recipes/scan-secrets.perch) — non-WASM sibling for secret scanning via gitleaks

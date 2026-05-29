# Trust by manifest — design doc

> **The thesis.** In an era where most code is written by AI agents, the bottleneck is not "is this code correct" but "can anyone afford to review 10,000 generated functions a day?" The fix is to shift the unit of trust from **who wrote the code** to **what the code declares it needs**. Reviewers stop asking "is this safe?" and start asking the much smaller, machine-checkable question "are these declared capabilities acceptable?"
>
> This document is the roadmap for making perch the runtime where that shift happens — author writes code in any language, compiles to WASM, perch embeds the bytecode AND its capability manifest, and enforces the manifest at runtime. Code that doesn't declare can't run. Code that lies gets killed.

---

## 1. Why this matters now

Three forces collide:

1. **AI-generated code volume**. A team that shipped 50k lines of human code a year now ships 500k lines of AI-assisted code. The review economics broke; nobody can read it all.
2. **Regulatory pressure**. Banks, healthcare, defense — auditors keep asking "what can this thing touch?" That is a capability question, not a code-correctness question. The artifacts most code ships today (containers, binaries, scripts) don't answer it.
3. **Supply-chain attacks**. `event-stream`, `node-ipc`, the XZ backdoor. Once an attacker ships a malicious dependency, code review is the only defense. Capability declarations are the missing systemic control.

The right answer to all three is the same: ship artifacts that **declare their capabilities** in a machine-checkable form, and run them inside a runtime that **physically refuses** to grant anything outside the declaration.

Perch already has the runtime. This doc covers what else has to ship.

---

## 2. The framing — three sentences

```
Programs declare what they need.
The runtime enforces it.
Code that doesn't declare can't run. Code that lies gets killed.
```

Every feature below is in service of those three sentences.

---

## 3. What perch already does (today)

The architecture the design calls for is **already shipping** in perch — just incomplete in three specific ways (see §4). Today:

| Vision item | Where it lives in perch today |
|---|---|
| Code is delivered as bytecode, not source | `bundle ... include "./policy.wasm" as policy ... end` |
| WASM has no syscalls; can't escape | wazero (the WebAssembly runtime perch links against) |
| Author declares filesystem reach | `wasm_mount_read "/host" as "/guest"` / `wasm_mount_write` |
| Author declares env-var reach | `wasm_env "VAR_NAME"` (allowlist; nothing else visible) |
| Author declares network reach | `wasm_allow_host "api.github.com"` |
| Author declares argv | `wasm_arg "..."` |
| Refusal is a tagged error | `wasm_capability_denied`, `wasm_http_refused`, `wasm_module_exited` |
| Host can pile on more refusals | `--no-network`, `--no-shell`, `--no-write`, `--allow-host`, `--untrusted` |
| Outer-file manifest exists | `requires` block — declares host bins / env / hosts / OS / arch |
| Supply-chain pinning | `bin "X" hash "sha256:..."` and `hash_file "bundle:..."` |

A real perch program today already looks like:

```perch
bundle
    include "./policy.wasm" as policy
end

requires
    bin  "kubectl" >= "1.28.0"
    host "api.github.com"
end

command validate
    arg deploy string "deploy spec"

    do
        wasm_run policy
            wasm_arg deploy
            wasm_mount_read "./deploys" as "/ro"
            wasm_env "DEPLOY_TARGET"
            wasm_allow_host "api.github.com"
        end
    end
end
```

This **already** delivers:
- The wasm module sees `/ro/...` and **nothing else** of the host filesystem.
- The wasm module sees `DEPLOY_TARGET` and **no other** env vars.
- `perch.http_get` to `api.github.com` works; every other host returns `wasm_http_refused`.
- The module can't fork, can't open arbitrary sockets, can't read `/etc/passwd`. The boundary is **physical**, not policy.

What's missing isn't enforcement. Enforcement works. What's missing is the **trust-unit** — see §4.

---

## 4. What's missing — three gaps, in priority order

### 4.1 In-module manifest (the load-bearing gap)

**Problem.** Today, the capabilities a WASM module gets are declared by the **`.perch` file**, not the module author. The wasm bytes are "dumb" — they don't carry a declaration of what they need. So the trust unit is still the .perch author, not the wasm author.

In the vision, the trust unit must be the wasm. A bank should be able to:
1. Receive `risk-model.wasm` from a vendor.
2. Read its **embedded manifest** in 5 seconds: "this thing wants `net:risk-api.vendor.com:443`, `read:/inputs`, `env:RISK_THRESHOLD`, 256MB memory."
3. Decide *yes* or *no* without reading 200KB of WAT or trusting the .perch wrapping it.

**Proposed shape.** A WASM **custom section** named `perch.manifest` carries a canonical JSON or CBOR document. Module authors generate it via an SDK:

```rust
// in policy.rs (Rust SDK)
perch::manifest! {
    name        = "risk-model",
    version     = "1.4.2",
    needs_read  = ["/inputs"],
    needs_write = [],
    needs_net   = ["risk-api.vendor.com:443"],
    needs_env   = ["RISK_THRESHOLD"],
    memory_max  = 256 * 1024 * 1024,
}
```

```go
// in policy.go (Go SDK, via TinyGo)
//go:wasm-manifest
var manifest = perch.Manifest{
    Name:       "risk-model",
    Version:    "1.4.2",
    NeedsRead:  []string{"/inputs"},
    NeedsNet:   []string{"risk-api.vendor.com:443"},
    NeedsEnv:   []string{"RISK_THRESHOLD"},
    MemoryMax:  256 << 20,
}
```

At build time the SDK appends a `perch.manifest` custom section to the .wasm. Perch reads this section before instantiating the module and decides whether to grant.

**The .perch file becomes ACCEPTANCE, not declaration.**

```perch
wasm_run policy
    # The module already declared its manifest. We accept the lot:
    accept_manifest

    # ... OR override per-line, narrower:
    accept_manifest
    deny "wasm_allow_host"       # we refuse the network ask
    deny "wasm_env" "DEBUG"      # deny only this one env var
end
```

```perch
wasm_run policy
    # ... OR override broader (requires --untrusted-allow-broaden or similar)
    grant "wasm_allow_host" "second-host.com"
end
```

**Three new error kinds:**

| Kind | When it fires |
|---|---|
| `manifest_missing` | The .wasm has no `perch.manifest` section, and the .perch file used `accept_manifest`. (To run an unmanifested module you must declare capabilities the old way.) |
| `manifest_rejected` | The module declared something the .perch file (or host policy) refused. Surfaced at load time, before the module runs a single instruction. |
| `manifest_violation` | The module attempted an op that wasn't in its own declared manifest. (This should be impossible if the loader honors the manifest, but the runtime double-checks; tripping this is a perch bug.) |

**Why this is the load-bearing gap.** Without it, "what does this wasm need?" is answered by reading the .perch file — which the auditor doesn't trust because the .perch file is what's wrapping the untrusted vendor module. With it, the auditor inspects the .wasm directly: one command, no .perch involved.

```sh
perch wasm inspect risk-model.wasm
  name:       risk-model
  version:    1.4.2
  declared capabilities:
    read:    /inputs
    net:     risk-api.vendor.com:443
    env:     RISK_THRESHOLD
    memory:  256 MB
  signer:    (none)
  size:      218 KB
```

---

### 4.2 Manifest diff — the underrated piece

**Problem.** When a vendor ships version 1.4.2 of `risk-model.wasm` and then version 1.5.0, the security team's *only* question is "did the capabilities change?" Today they have no tool to ask that question machine-checkably. They re-read the changelog and hope it's honest.

**Proposed shape.**

```sh
perch wasm diff risk-model-1.4.2.wasm risk-model-1.5.0.wasm

  manifest diff: risk-model
  ─────────────────────────────────────────────────────────────
    version:    1.4.2 → 1.5.0
    name:       risk-model (unchanged)
    memory:     256 MB → 256 MB (unchanged)

    needs_read:
      /inputs                                            (unchanged)
    + /etc/keys                                          ★ NEW
    + /tmp/cache                                         ★ NEW

    needs_net:
      risk-api.vendor.com:443                            (unchanged)
    + api.openai.com:443                                 ★ NEW
    - legacy.vendor.com:443                              (removed)

    needs_env:
      RISK_THRESHOLD                                     (unchanged)
    + STRIPE_SECRET_KEY                                  ★★ NEW (HIGH RISK)
  ─────────────────────────────────────────────────────────────
  VERDICT: ELEVATED — 4 new capability requests
           1 marked HIGH RISK by host policy
           Review required before deployment
```

This is the **manifest diff as code-review primitive** the doc calls out. Nobody does this well today, and it's exactly what regulated industries want. Implementation is cheap — parse both manifests, set-diff each capability list, render — but the value is enormous because it turns "AI shipped 10,000 functions" into "AI shipped 0 new capability requests" or "AI shipped 1 new capability request, here it is."

CI integration is trivial: `perch wasm diff old new --fail-on-elevated` returns non-zero if anything new appeared. Bank security teams can wire this into the deploy pipeline directly.

---

### 4.3 Signed manifests — the trust-chain piece

**Problem.** A manifest is useless if anyone in the supply chain can rewrite it. If `risk-model.wasm` ships with `needs_net = ["risk-api.vendor.com"]` but a man-in-the-middle adds `needs_net = ["*"]` and re-emits the .wasm, the runtime grants `*` and the trust model collapses.

**Proposed shape.** A second custom section `perch.signature` holds:

```
{
  "algo":   "ed25519",
  "signer": "vendor-co-prod-2026",
  "pubkey": "<base64 ed25519 public key>",
  "sig":    "<base64 signature over: SHA256(wasm_without_signature_section || manifest_section)>"
}
```

Perch verifies the signature against:
1. A keyring loaded from `--trust-keys path/to/keyring.json`.
2. A signature-pinned reference: `wasm_run policy / require_signer "ed25519:vendor-co-prod-2026" / end`.

**Two policy postures the host can take:**

| Posture | Behavior |
|---|---|
| `--unsigned-allow` | Unsigned modules run if the manifest is otherwise acceptable. (Default for local dev.) |
| `--unsigned-deny` | Unsigned modules error with `manifest_unsigned`. (Default for production / CI.) |

New error kinds: `manifest_unsigned`, `manifest_signature_invalid`, `manifest_signer_untrusted`.

**Combined with `4.1` and `4.2`**, this gives the full trust chain:

```sh
perch wasm verify risk-model.wasm --trust-keys ./vendor-keys.json
  ✓ signature valid (signer: vendor-co-prod-2026)
  ✓ manifest hash matches signed payload
  ✓ all declared capabilities within host policy
  ✓ signer is in trust keyring

perch wasm diff risk-model-1.4.2.wasm risk-model-1.5.0.wasm
  ★ new capability: needs_net += api.openai.com:443
  → re-signing required for production deploy

perch run ./pipeline.perch
  ✓ verified policy.wasm before instantiation
  ✓ enforcing declared manifest
```

---

## 5. Two things explicitly NOT in scope

**(a) A new language.** The doc that prompted this argued this point and it's right: building a new language is a 3-year tarpit and never ships the thing that matters. Perch is the **language-agnostic** frontend. Authors write in Rust, Go (TinyGo), AssemblyScript, Zig, C — whatever compiles to clean WASM. The SDK per language is a 200-line crate/package. The shared work is the manifest format + runtime + diff tool.

**(b) Capability *metering*.** "This module ran for X seconds, made Y network calls, allocated Z MB." Useful for cost attribution and DoS defense, but it's a separate axis from "is this safe to run." Add it later as a per-module trace; don't entangle with the manifest design.

---

## 6. The honest risks

1. **The "permission fatigue" failure mode.** Every permission system in history has failed at the same place: developers grant `*` because the narrow path is harder than the broad path. **The design has to make the narrow path the easy path.** Concretely:
   - Generating a manifest from real usage must be a one-command operation: `perch wasm record ./policy.wasm` runs the module, watches what it actually touches, emits a tight manifest.
   - Declaring `needs_net = ["api.github.com"]` must be one line. Declaring `*` must require a signed exception AND trigger an audit log entry AND fail CI by default. The asymmetry is the design.

2. **The "small binary" pitch is partly misleading.** Yes, the deliverable is small (a .wasm + its manifest fits in tens of KB). But the host still needs perch installed to enforce. The win is *shared runtime instead of per-app container*, not *no runtime*. Density and startup time go from VM/container scale to function-call scale, but it's not magic.

3. **WASM is the enforcement boundary, not the answer to every safety question.** WASM gives you "this process cannot do anything outside the host imports I provided." It does *not* give you "this code is correct" or "this code doesn't have logic bugs that drain the budget the host approved." Those are different problems. Manifest enforcement bounds the *blast radius* of correctness failures; it doesn't prevent them.

---

## 7. Phased build plan

Each phase is self-contained and ships independently. None of them touch the existing `wasm_run` codepath in breaking ways — they extend it.

### Phase 1 — Manifest format + Rust/Go SDK + reader (4–6 weeks)

- Define the `perch.manifest` custom section format (CBOR, fixed schema).
- Write `perch-manifest-rs` and `perch-manifest-go` SDKs that emit the section at build time.
- Teach `infra/ops/wasm.go` to read the section before instantiation.
- New `perch wasm inspect ./mod.wasm` subcommand.
- New error: `manifest_missing` when `accept_manifest` is used and the section is absent.
- Documentation: replace the example in `docs/wasm.md` with the manifest-driven shape; keep the old explicit shape as the fallback for unmanifested third-party modules.

**Demo deliverable:** rebuild the existing `demos/wasm-plugin-host` so the plugins ship their own manifests instead of being granted by the .perch file. Document the workflow end-to-end: write plugin → emit manifest → ship .wasm → host imports → perch enforces.

### Phase 2 — `accept_manifest` and override sub-statements (1–2 weeks)

- Grammar: `accept_manifest` / `deny "<cap>"` / `grant "<cap>"` sub-lines inside `wasm_run`.
- Loader: merge declared manifest + .perch overrides into the effective capability set; error with `manifest_rejected` on conflict (declared want vs .perch deny).
- Existing explicit shape (`wasm_arg`, `wasm_mount_read`, etc.) continues to work — it's just sugar for "I'm declaring on behalf of the module."

### Phase 3 — `perch wasm diff` (1 week)

- Standalone subcommand. Reads both modules' manifest sections, set-diffs, renders.
- Risk scoring uses the same `riskBadge` style as `perch --scan` for visual consistency.
- `--fail-on-elevated` flag for CI.
- `--format json` for tooling integration.

### Phase 4 — Signing (3–4 weeks)

- `perch.signature` section format.
- `perch wasm sign --key ./signing.key ./mod.wasm` subcommand.
- `perch wasm verify --trust-keys ./keys.json ./mod.wasm` subcommand.
- `wasm_run` `require_signer` sub-statement.
- CLI flags `--unsigned-deny` / `--unsigned-allow` with the right defaults per mode (dev vs CI).
- New errors: `manifest_unsigned`, `manifest_signature_invalid`, `manifest_signer_untrusted`.

### Phase 5 — Host-level policy file (2 weeks)

- `~/.config/perch/policy.yaml` (or repo-local `.perch-policy.yaml`).
- Declares org-wide defaults: "no wasm in this environment may declare `needs_net = ['*']` unless signed by an approved key in this keyring." Etc.
- Composes AND-wise with per-invocation flags — neither side can grant more than the other allows. (Same model as the existing capability-flag intersection.)

Phases 1–3 alone are the **demo for the bank pitch**. Phase 4 is needed for **production**. Phase 5 is needed for **fleet** deployment.

---

## 8. What this is NOT — claims to avoid

So the project doesn't drift into hand-wave:

- **NOT** "this prevents all exploits in your code." It prevents the *capability* from being used; the code can still have logic bugs that misuse capabilities it legitimately holds.
- **NOT** "no runtime needed." Perch is the runtime. It must be present on the host.
- **NOT** "you don't need code review." Reviewing the *manifest* replaces reviewing the *implementation* for trust questions, not for correctness questions.
- **NOT** "compatible with arbitrary native code." Anything escaping the WASM boundary (FFI, `unsafe` in Rust compiled to native, system calls via libc) defeats the model. The constraint is real: code must compile to clean WASM with only the host imports perch grants.
- **NOT** a replacement for sandboxing OS-level tools the .perch file uses (the `shell "kubectl ..."` lines). For native binaries the file invokes from the *host* (vs. the embedded wasm), `requires` + `--allow-bin` + `hash` pinning is the model. WASM and `requires` are complementary — wasm is the boundary for the bundled bytecode; `requires` is the boundary for the host tooling.

---

## 9. The pitch (for the bank audience)

> Today your auditors read 50,000 lines of generated code per quarter and approve or reject based on guesswork. You ship that work in containers — opaque images that could touch anything the kernel allows. The capabilities each service has are implicit in code; there's no machine-checkable answer to "what can this service touch?"
>
> Perch lets your engineers (and their AI assistants) write code in Rust / Go / TypeScript that compiles to WASM. Every module ships with a **signed, declared manifest** of what it needs: which paths, which hosts, which env vars, how much memory. Perch enforces it at runtime — the module physically cannot reach anything outside its declaration.
>
> Your auditors stop reading code. They read manifests. They sign off on capabilities, not on implementations. When a vendor ships v1.5, your CI runs `perch wasm diff v1.4 v1.5` and tells you in 50ms what capabilities changed.
>
> The artifact is a 200KB .wasm + manifest, not a 1GB container. The runtime is one binary, deployed once per host. The trust model is auditable. The diff is machine-checkable. The blast radius is bounded by construction.

---

## 10. What we want feedback on

This is a roadmap doc, not a built feature. Two open design questions worth getting external opinion on:

1. **Should the in-module manifest be CBOR or JSON?** CBOR is the WASM-community standard (component model uses it), smaller on the wire, robust against parser quirks. JSON is debuggable with `cat`, faster to iterate on. We lean CBOR but the developer-experience cost matters.

2. **Should `accept_manifest` be the default, or should explicit declaration in the .perch file stay the default?** Default-accept reduces .perch ceremony for trusted-vendor modules. Default-explicit makes the .perch file the explicit policy point. We lean default-explicit (safer; matches the "narrow path = easy path" principle in §6.1), but ergonomically default-accept is much nicer once teams trust the manifest+signing chain.

If you have skin in this game — running AI-generated code in regulated environments, building plugin hosts, doing capability-based supply-chain work — those are the two questions where outside input would change the design.

---

## See also

- [docs/wasm.md](wasm.md) — current `wasm_run` reference (what's shipping today)
- [docs/wasm-walkthroughs.md](wasm-walkthroughs.md) — realistic workflows on the existing API
- [docs/requires.md](requires.md) — the file-declared manifest for the .perch outer layer (the OTHER half of "trust by manifest" — perch itself does this today for the .perch file's own host needs)
- [docs/sandbox.md](sandbox.md) — the existing capability model and CLI flag intersection

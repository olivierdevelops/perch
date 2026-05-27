# Changelog

All notable changes to perch are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows [SemVer](https://semver.org/) once it reaches v1.0.

## [Unreleased]

### Added

### Design notes (planned, not yet shipped)

- **Unified `with ... do ... end ... end` context block** — replace `os "X" ... end` / `with_env` / `with_cwd` / `sandbox` with a single block carrying any combination of context attributes (os, cwd, env, no_tty, allow_host, max_runtime, …). Target shape:

  ```perch
  with
      os "unix"
      cwd "./build"
      env DEBUG=1
      do
          shell "make"
      end
  end
  ```

  Same `do ... end` body opener every other block in perch uses (`command`, `catch`, `template`) — no new separator keyword, two `end`s mirror the standard `command ... do ... end ... end` shape. Composability win: adding a new context kind never requires a new block construct, just a new attribute line.

  **Status: blocked.** Attempt this session shipped a working OS-context block via `os "X" ... end` (below) but the unified `with` shape needs two capabilities the current capy engine doesn't have: (a) body-parser accepting attribute lines like `os "unix"` / `cwd "./build"` as valid content inside an outer `block_closer end` block (errored even with `arg literal "X" + arg capture target string` matching the wasm_arg precedent); (b) loader's `do_begin` state machine extended to accept `do` inside a `with` block (the current code hardcodes `stCommand` / `stCatch` / `stTemplate` only). Documented here as the design intent; in the meantime `os "X" ... end` covers the OS piece and existing `with_env` / `with_cwd` / `sandbox` blocks cover the rest.

### Added

- **`arch "ARCH" ... end` — architecture execution context block.** Sibling to `os "PLATFORM" ... end`, gates the body by `${arch}`. Targets are Go GOARCH values (`amd64`, `arm64`, `386`, `arm`, `riscv64`, …); exact match, no umbrellas (matrix builds want exact pinning). Composes cleanly with `os` for cross-product matrices:

  ```perch
  command release
      do
          os "linux"
              arch "amd64"
                  shell "GOOS=linux GOARCH=amd64 go build -o app-x64 ./cmd/app"
              end
              arch "arm64"
                  shell "GOOS=linux GOARCH=arm64 go build -o app-arm64 ./cmd/app"
              end
          end
          os "darwin"
              arch "arm64"
                  shell "GOOS=darwin GOARCH=arm64 go build -o app-mac-arm64 ./cmd/app"
              end
          end
      end
  end
  ```

  `perch simulate release --sim-os=linux --sim-arch=arm64` prunes everything except the matching leaf, reporting per-block whether the target matched. Wired into both simulator walkers (static + v2 state-threaded). When the unified `with ... do ... end ... end` block lands (deferred), `arch "X"` becomes an attribute inside it (`with arch "arm64" do … end end`); the standalone block remains as the dedicated arch-only shape.

- **`os "unix"` umbrella target** for execution-context blocks. Matches darwin, linux, freebsd, openbsd, netbsd — the same set the existing `${is_unix}` auto-bound var covers. Standard "any Unix; Windows is special" pattern is now one block:

  ```perch
  os "unix"
      shell "rm -rf ./build"
  end
  os "windows"
      shell "rmdir /S /Q .\\build"
  end
  ```

  Supported targets: `"darwin"`, `"linux"`, `"windows"`, `"freebsd"`, `"openbsd"`, `"netbsd"`, `"unix"`. Both the runtime handler (`infra/ops/flow.go:OsTargetMatches`) and the simulator (`usecases/simulate/simulate.go:osMatches`) apply the same matching rules, so `perch simulate ... --sim-os=freebsd` correctly runs an `os "unix"` body.

- **`os "PLATFORM" ... end` — execution-context blocks.** Declares which body is meant for which OS, making cross-platform intent first-class structure rather than hidden inside shell strings. Same runtime semantics as `if os == "X" ... end` (skip body if `${os}` doesn't match), but with three concrete wins downstream: (1) **simulate** prunes mismatched branches as known dead code on the target host — `simulate setup --sim-os=linux` reports "os \"darwin\" does NOT match sim-os \"linux\" — body skipped" instead of treating the darwin shell as uncertainty; (2) the **web UI** can flag "incompatible with your current OS" before the user clicks Run; (3) **agents** reasoning about a `.perch` file have explicit OS metadata to constrain their plan.

  ```perch
  command setup
      do
          os "darwin"
              shell "brew install jq"
          end
          os "linux"
              shell "apt-get install -y jq"
          end
          os "windows"
              shell "choco install jq -y"
          end
      end
  end
  ```

  `if os == "X"` still works for runtime branching mid-flight; `os "X" ... end` is the new shape for "this body's structural intent is OS-specific work." Wired into both the static and the v2 (state-threaded) simulator. Implementation: new `os` op kind + handler in `infra/ops/flow.go`, new grammar function in `lib.perch`, added to `domain.IsBlock`.

- **Risk score in `--scan` + `/api/scan`.** The capability audit now leads with a one-glance summary — 🟢 SAFE / 🟡 LOW / 🟠 MED / 🔴 HIGH — plus the concrete bullet list of why (executes shell, uses sudo, network access, catch forwards proxy_args, etc.). The web UI's Scan tab can pill this score; the JSON endpoint returns `risk: {score, reasons}` for downstream tools (CI gates, dashboards, Slack bots). Scoring rules in `usecases/scan/risk.go`: HIGH escalates on sudo, proxy_args forwarding to shell, or any HIGH-severity finding; MED on shell metacharacters or 2+ capabilities; LOW on exactly one capability; SAFE on pure ops. Pure UI affordance — explicitly NOT a security guarantee; it's "is this worth carefully reading before I run?" framing.

- **🛟 Error handling — `try / rescue / finally` + `match` + the error-kind enum.** Until now perch had no general error recovery: any op failure halted the command. `try_shell` covered the shell case, `retry` re-ran whole bodies on failure, but there was no way to discriminate on **what kind of error** happened. This ships full structured error handling:

  ```perch
  try
      let body = http_get "${url}"
  rescue err
      match "${err.kind}"
          case http_5xx
              throw "${err.message}"          # let an outer retry handle it
          case http_ssrf_blocked
              run alert msg:"security: ${err.detail}"
          case http_4xx
              print "bad request: ${err.code}"
          else
              throw "${err.message}"
      end
  finally
      rm "${tmpfile}"
  end
  ```

  Inside `rescue err`, five bindings are populated: `${err.kind}` (a value from a 30-member enum), `${err.message}`, `${err.code}` (op-specific, e.g. exit status for shell), `${err.op}` (which op failed), `${err.detail}` (structured extra info). `finally` runs unconditionally — both on success and failure; errors in `finally` override the original. `throw "msg"` re-raises (semantically same as `fail`, spelled differently for clarity).

  **The error-kind enum** is 30 values across 8 groups: shell (`shell_exit_nonzero`, `shell_metachars_denied`, `shell_bin_not_allowed`, `shell_signal_killed`), HTTP (`http_4xx`, `http_5xx`, `http_redirect_refused`, `http_ssrf_blocked`, `http_dns_failed`, `http_timeout`), file (`file_not_found`, `file_permission_denied`, `file_path_disallowed`, `file_already_exists`), capability (`cap_shell_denied`, `cap_network_denied`, `cap_subprocess_denied`, `cap_write_denied`), wasm (`wasm_compile_failed`, `wasm_module_exited`, `wasm_capability_denied`, `wasm_http_refused`), interpolation (`unresolved_var`, `unresolved_template`), runtime (`timeout_exceeded`, `signal_received`, `user_fail`, `assert_failed`), and lookup (`command_not_found`, `bin_not_found`). Plus `unclassified` as the catch-all for ops not yet migrated to tagged errors.

  **`match VALUE / case X / else / end`** is a new general value-driven dispatch block — used here for error-kind discrimination, but works on any string (auto-bound vars, `let` captures, arg values). Bare-identifier `case` values like `case user_fail` are captured as their string spelling, so the enum reads naturally.

  **Naming honesty:** perch uses Ruby-style `rescue` (not `catch`) because `catch unknown ... end` was already the file-scope keyword for catch-all CLI commands. They're structurally different concepts; distinct keywords keep both clear.

  Composes cleanly with `retry`, `parallel`, `timeout`, and every other block op — errors propagate UP through blocks until something catches them. See [docs/errors.md](docs/errors.md) for the full reference (every error kind documented, every composition rule, five worked patterns including retry-with-classified-transients, parallel-with-per-branch-recovery, multi-level match).

  Initial tagged ops: `shell` / `shell_output` (shell_exit_nonzero, shell_metachars_denied, shell_bin_not_allowed, shell_signal_killed), `fail` (user_fail), HTTP runner (http_ssrf_blocked, http_redirect_refused, http_dns_failed, http_timeout). Remaining ~85 ops fall through to `unclassified` for now — migration is mechanical and continues incrementally.

- **HTTP from inside a WASM module — `wasm_allow_host` + host-provided imports.** WASI Preview 1 has no network, which left every `wasm_run` module isolated from the world (good for plugin sandboxing; awkward when the module actually needs to fetch a remote spec/policy/manifest). This adds a `wasm_allow_host "HOST"` declaration inside `wasm_run` bodies plus a `perch` host module exposing `http_get` / `http_status` / `http_body_len` / `http_read_body` / `http_close`. Modules import them via a tiny Go SDK (`wasm-sdk/perchhttp`); call sites look like `body, status, err := perchhttp.Get(url)`. Every call goes through the **same secure HTTP client** the native `http_get` op uses — SSRF guard, redirect policy, `--allow-host` check, all enforced. The module's `wasm_allow_host` list intersects AND-wise with the outer `--allow-host` flag; empty intersection → all calls refused. Modules with NO `wasm_allow_host` declaration get `http_get → -1` for every URL (fail-closed). 32 MB response cap; per-call handles released on `http_close` or runtime teardown. Per-call state (policy + handle table) is threaded through `context.Context` so the shared wazero runtime can host concurrent `wasm_run` calls without cross-talk. See [docs/wasm.md → "HTTP from inside a module"](docs/wasm.md). Smoke-test: a TinyGo-built WASM module fetching `https://api.github.com/zen` and printing the body to stdout. Roadmap: `Post(url, body)`, custom headers, then sockets only if WASI Preview 2 reaches stable in wazero.

- **Declarative `bundle ... end` section — the `.perch` file is now the complete buildable spec.** Previously the only way to embed files into a fat binary was the CLI `--include PATH` flag at `perch --build` time. That meant: the build invocation lived outside the source file, CI configs duplicated the include list, and a fresh contributor had to read README to know what to pass. Now you declare the bundle directly in the `.perch`:

  ```perch
  bundle
      include "./modules"
      include "./policies/rules.json"
  end
  ```

  `perch --build` (no extra args) reads this and produces the right artifact. Paths in the `bundle` section resolve relative to the `.perch` file's directory. CLI `--include PATH` still works and is **additive** on top of the declarative set — useful for CI steps injecting a generated config. Combined with `wasm_run "bundle:PATH"` (below), the entire build + run story is **one file in, one binary out, zero CLI flags**.

- **Bundle aliases: `include "PATH" as NAME`, used as `wasm_run NAME` (bare identifier).** Embedded WASM modules now have a clean, name-based reference syntax. No magic URI strings (no `bundle:foo.wasm`); no separate `wasm_bundle` op. The `bundle ... end` section declares includes with optional `as NAME` aliases; `wasm_run` is grammar-overloaded to accept either a string literal (loads from disk) or a bare identifier (resolves against the bundle alias table and loads bytes straight from the embedded archive). One op, two source forms, zero disk reads at runtime when an alias is used.

  ```perch
  bundle
      include "./policy.wasm" as policy_wasm
      include "./schema.wasm" as schema
  end

  command run_plugin do
      wasm_run policy_wasm        # bare ident → bundle bytes
          wasm_arg "/ro/deploy"
      end
  end
  ```

  Compiled-module cache is keyed by `bundle:<bundleHash>:<entry>` so repeated calls (e.g. inside `parallel`) compile once. New `ops.BundleReadFile` + `ops.BundleHash` Go helpers expose the same capability to other op handlers. See [docs/wasm.md → "Embedded modules — declare once with `as NAME`"](docs/wasm.md). The practical shape: **ship a sandboxed plugin host as one artifact, no install steps on the target machine, no string-URI awkwardness in user code.**

- **🪟 Web UI feature-parity push — for non-devs.** `perch --server` was a thin form-per-command surface; this brings it close to the CLI's pre-flight + run experience. Five tabs (hash-routed): **▶ Run** (live command search/filter, type-aware form inputs — checkbox/number/textarea per arg type, defaults as placeholders so empty submits use runtime defaults, mod badges for `test`/`detached`/`proxy_args`, collapsible globals panel, **Copy as CLI** button that mirrors the form back as a shell command), **🧪 Simulate** (every `--sim-*` flag as a form field plus a JSON fixture textarea — paste v2 fixture with oracles + scenarios, see per-op outcomes per scenario), **🔍 Scan** (full capability + risk audit with severity pills + recommended hardened invocation), **✓ Check** (syntactic validation with issue counts), **ℹ About**. Dark mode toggle (auto-respects `prefers-color-scheme`, persists per browser). Four new JSON endpoints back the panels: `GET /api/program`, `POST /api/check`, `POST /api/scan`, `POST /api/simulate` — embeddable in your dashboard / Slack bot / Backstage plugin. Same interpreter as the CLI; same capability restrictions inherit from launch (`perch --no-shell --no-network --server`). New doc: [docs/web-ui.md](docs/web-ui.md). The framing: *"For teammates who don't live in a terminal."*

- **`perch simulate` v2 — state threading + oracles + scenarios + JSON fixture file.** The static walker now threads a mutable `SimState` through the op sequence: `write_file "/tmp/x"` makes downstream `if exists "/tmp/x"` evaluate true; `cd /srv` shifts relative paths; `let X = shell_output Y` binds `${X}` to a simulated value. Unresolvable ops can be pinned by **oracles** — concrete simulated outputs supplied per op key (`shell_output`, `http`, `file_exists`, `has_bin`). Multiple **scenarios** (named override sets) run from one fixture: a "happy" scenario with `https://api.github.com/health → 200` and a "github-down" scenario with `→ 500` execute back-to-back, each with its own report. Pass via `--sim-file FIXTURE.json` — the fixture's capabilities layer under CLI `--sim-*` flags (CLI wins on conflict); the fixture's `scenarios` field declares each branch. HTTP oracles support redirect destinations; a redirect to a host NOT in the network allowlist becomes WILL_FAIL. Exit code is non-zero if any scenario reports a failure. Answers the user-facing questions: "what if the file exists after the previous step?", "what if HTTP returns 500?", "what if the script modifies state?", "what if the upstream redirects?". See `docs/simulate.md` → "Stateful simulation, oracles, and scenarios."

- **`perch simulate` — hypothetical-env what-if analyzer.** The missing third tool alongside `--scan` (static caps, no env) and `--dry-run` (live env, skips execution). Takes a simulated environment via CLI flags (`--sim-os`, `--sim-arch`, `--sim-env K=v`, `--sim-env-only`, `--sim-fs-read`, `--sim-fs-write`, `--sim-have-bin`, `--sim-allow-host`, `--sim-no-shell`, `--sim-no-subprocess`, `--sim-no-network`, `--sim-no-write`) and walks every op in the target command (or every command if no name given), classifying outcomes as **WILL_RUN ✓** / **WILL_FAIL ✗** (with the specific capability that's missing) / **MIGHT_FAIL ?** (with the scenario that branches the outcome — e.g. "server at api.github.com could redirect to any host; succeeds if redirect stays in allowlist"). Recursively follows `if EXPR ... end` (skips branches that wouldn't fire under the sim env), `parallel` / `retry` / `cache` / `sandbox` / `with_env` (mutates sim env for the body), `run other_cmd` (follows the call). Exit code non-zero on any `WILL_FAIL` — drop into CI as a pre-merge gate. See `docs/simulate.md`. Roadmap (documented honestly): symbolic branching on unknown conditions, HTTP redirect destination enumeration, counterfactual suggestions ("add api.github.com to your allowlist to make this pass").

- **`perch-mcp` streams stdout/stderr live via MCP progress notifications.** Previously, a long-running verb (deploy, build, test suite) returned its output as a single blob after completion — the agent waited in silence. Now: when the MCP client sends `_meta.progressToken` in the `tools/call` request (per MCP spec), perch-mcp emits a `notifications/progress` event for every line of stdout / stderr as it's produced. The final `tools/call` response still arrives at completion with the accumulated output (so non-streaming clients still work). Concurrency-safe — `parallel` block goroutines emit notifications through a mutex-guarded encoder; each notification gets a monotonic `progress` counter. Closes the architectural asymmetry between the web UI (which streamed NDJSON via `flusher.Flush()`) and the MCP server (which buffered). Honest framing: this isn't a new MCP feature — the spec always supported progress notifications. perch-mcp just wasn't using them. See `docs/mcp.md` → "Streaming progress" section.

- **`wasm-plugin-host` demo — the architectural killer demo.** A pluggable runtime where every plugin is a WASM module conforming to a shared contract (read `/ro/data/input.json`, write JSON to stdout). Ships 4 legitimate plugins (tax / discount / shipping / format) and 1 **deliberately malicious** plugin that tries five different escape attempts: read `/etc/passwd`, read host env secrets, open a TCP socket, write outside the mount, exec `curl`. Every escape attempt fails — not because perch policy intercepts them, but because the WASI runtime literally doesn't provide those operations. The conceptual frame: *"We don't sandbox AI code. We give it a runtime where unsafe things don't exist."* This is the practical answer to the question "can we let AI write plugins for our system?" — yes, if the plugins are WASM and the runtime is perch's `wasm_run`. Composes with `parallel` (run all plugins concurrently) and `cache` (content-hash invalidation per `(plugin-bytes, input-bytes)` pair). See `demos/wasm-plugin-host/`.

- **Three runnable `wasm_run` demos** under `demos/`: `wasm-schema-validator` (200-line JSON Schema validator in Go → WASM, CI-shaped), `wasm-policy-check` (K8s manifest policy enforcement: registry / resource-limits / no-latest / no-privileged / required-labels), `wasm-diff-summary` (agent-safe git-diff summarizer with risk heuristics, designed for MCP). Each ships the Go source + pre-built `.wasm` + commands.perch + example data + README showing both direct-run and `perch --build`-to-single-binary flows. `bundle_dir` now falls back to `script_dir` when not running embedded, so one `commands.perch` works in both modes. New companion doc: **`docs/wasm-walkthroughs.md`** — 5 end-to-end real-world workflows (markdown frontmatter validation, JSON Schema with cache, AI-agent MCP surface, polyglot pipelines, CI hot loops).

- **`wasm_run` — the constrained execution lane.** New block op that loads a WebAssembly module via [wazero](https://github.com/tetratelabs/wazero) and runs its `_start` function under WASI Preview 1. Capabilities are declared in the body: `wasm_arg "VAL"` appends to argv; `wasm_env "K1,K2"` is the env allowlist (nothing else passes through, even if set on the host); `wasm_mount_read "PATH"` mounts a host dir read-only at `/ro/<basename>`; `wasm_mount_write "PATH"` does the same read-write at `/rw/<basename>`. Anything not declared **does not exist** in the module's environment — enforced by the WASM runtime by construction, not perch's policy layer. Composes with every execution context (`parallel`, `retry`, `timeout`, `cache`, `sandbox`) and with `--trace` / `--audit` / `--report`. Pure Go (no CGO); adds ~3 MB to the binary. Demo: [`demos/wasm-hello/`](demos/wasm-hello/). Full reference: [`docs/wasm.md`](docs/wasm.md). **Roadmap (called out in the docs): sockets/network (WASI Preview 2), URL-loaded modules with sha256 pinning, named-export typed calls, persistent on-disk cache, module signature verification.**

- **`recipes/` — 22 ready-to-run `.perch` files for real problems.** A curated library people can `curl` + `perch --scan` + run. Covers single-service local stacks (redis, postgres, mongodb, mysql, mailpit, minio, rabbitmq, localstack), 3-in-1 stacks via `parallel` (devstack = postgres+redis+minio, aistack = ollama+chromadb+open-webui, observe = prometheus+grafana+loki, kafka-stack), cross-platform tool installers (modern-unix, clouds, node-stack, python-stack), CLI workflow wrappers (gh-flow, docker-mgr, kube-helpers), and ops/security (mkcert-local, backup via restic, scan-secrets via gitleaks). Each recipe imports `recipes/_lib.perch` for shared templates (`require_docker`, `ok`, `say`, …) — showcases the import + template + parallel + sandbox systems in one place. See [docs/recipes.md](docs/recipes.md) and [recipes/README.md](recipes/README.md).

- **Imports now propagate templates.** When a file imports another that declares `template NAME ... end` blocks, those templates are visible at every `call NAME ...` site in the importer. The expansion pass runs twice: once in `parseEventStream` (resolves locally-defined templates), then again in `resolveImports` after merging (resolves imported ones). Recursion and unresolved calls still fail loudly at `--check` time.

- **`--trace` — live human-readable op stream while running.** Prints `▸ kind args…` to stderr the moment each op fires and `✓ (duration)` (or `✗ error`) when it completes. Block ops (`if`, `parallel`, `retry`, `cache`, `sandbox`, …) indent their children under the block header so the live tree shape mirrors what `--report` shows after the run. Bare `--trace` streams to stderr; `--trace=PATH` to a file (`--trace=-` for stdout). Pairs with the existing `--audit FILE.ndjson` (machine-readable) and `--report` (post-run human tree) — all three derive from the same hook order. Mutually exclusive with `--report` (they share the Tracer slot).

- **`--dry-run` now expands block bodies inline.** Previously `if` / `parallel` / `retry` / `cache` / `sandbox` printed as summary counts (`{5 body ops}`); now the body is rendered as an indented sub-tree so what you see is every op that could fire. The top-level args remain interpolated; nested ops show their literal `${var}` placeholders (since interpolation only runs at dispatch time).

- **`perch test` — discover and run behavior tests.** A test is a regular `command` with the `test` modifier; `perch test` discovers them, runs each in a sandboxed temp cwd with `--no-shell` / `--no-network` / `--no-subprocess` on by default, and reports pass/fail. Opt-out modifiers (`test_allow_shell`, `test_allow_network`, `test_allow_subprocess`, `test_keep_cwd`, `test_timeout N`) loosen the sandbox per-test. Hidden from `--help`, MCP, and `did you mean…?` suggestions. Seven new assertion ops (`assert_eq`, `assert_neq`, `assert_contains`, `assert_not_contains`, `assert_exists`, `assert_not_exists`, `assert_match`) make failure messages readable. CLI: `perch test [--filter PATTERN] [-v|--verbose]`. Exit code non-zero if any test failed — wire into pre-commit and CI. See [docs/testing.md](docs/testing.md). The framing: **a test is a command that fails when something's wrong.** No mocking framework, no expectations DSL — same ops, same templates, same execution contexts.

- **Templates — parse-time stamps for boilerplate elimination.** New `template NAME … end` declaration with the same `arg NAME … end` block syntax as `command`. Every `call NAME args…` site is expanded inline before the program reaches the interpreter; positional args are substituted as `${argname}` bindings in the spliced body. Templates can't recurse, can't define commands/imports, and don't appear in `--help` / MCP. **The framing: a template is a command that expands at parse time instead of running at execution time.** See [docs/language.md → Templates](docs/language.md#templates--parse-time-stamps).

- **Execution contexts — six block ops that wrap a body.** Each modifies *how* the inner ops run without changing what they can express; they compose by nesting.
  - **`parallel … end`** — every direct child runs concurrently in its own goroutine; block exits when all complete. Each branch gets its own Bindings copy.
  - **`timeout "30s" … end`** — wall-clock cap on the body. Inner deadline can only narrow the outer one.
  - **`retry N … end`** — retry the body up to N times with exponential backoff (capped at 5m).
  - **`with_env "K1=v,K2=v" … end`** — bracketed env overlay; auto-restored on exit.
  - **`with_cwd "./path" … end`** — bracketed `cd` that auto-restores even on error.
  - **`sandbox "no_shell,no_network" … end`** — narrows the active capability mask for the body. Intersection rule: masks only narrow, never widen. Runtime enforcement shipped; static enforcement at `--check` time is the follow-up.
  - **`cache "KEY" "TTL" … end`** — user-keyed body cache. On miss, runs body and persists captured `let` bindings. On hit, replays bindings and skips the body. Honest framing: perch does NOT content-address the body's inputs — the user picks the key.

- **`--report` — span-tree renderer for a run.** Block ops nest naturally (children's spans live between their block's Before and After), so a tree falls out for free. Each node shows status, kind, key arg, wall-clock, optional template provenance:

  ```
  ── perch trace ─────────────────────────────────
  ✓ nested (2ms)
  ├─ ✓ retry attempts=2 (65µs)
  │  └─ ✓ print "==> Attempt" (63µs) [from template log_step]
  └─ ✓ parallel (143µs)
     ├─ ✓ print "==> Branch A" (20µs) [from template log_step]
     └─ ✓ print "==> Branch B" (19µs) [from template log_step]
  ```

  `--report` writes to stderr; `--report=PATH` to a file (`--report=-` = stdout). The audit NDJSON stream remains the canonical machine artifact; `--report` is the human renderer over the same hook order.

- **Variadic args — `rest` modifier + `for_each` block op.** Declare the last positional arg with `rest` to make it capture every remaining argv:

  ```capy
  command pack
      arg out    type string  index 0  end
      arg files  type string  index 1  rest  end
      do
          print "${files_count} files"
          for_each "${files}" f
              print "  → ${f}"
          end
      end
  end
  ```

  `${files}` becomes a newline-joined string; `${files_count}` is the count (int). `for_each VALUE NAME ... end` iterates over any newline-separated value, restoring the previous binding for `NAME` after the loop. The validator enforces that `rest` is on the last arg, type `string`, no default, with a positional index — and treats `${NAME_count}` as known in body interpolation. Equivalent in shape to Go's `args ...string`.

- **`--allow-host HOST[,HOST...]`** — strict allowlist for HTTP destinations. When set, every URL hit by `http_get` / `http_post` / `download` / `http_status` (initial request AND every redirect destination) must match an allowlist entry. Patterns: exact (`api.github.com`), single-label wildcard (`*.s3.amazonaws.com` — TLS-SAN style), host:port (`localhost:8080`), IP literal. Multiple `--allow-host` flags accumulate. Composes AND-wise with the default SSRF guard: a host in the allowlist still has to pass the private-IP check unless `--allow-private-ips` is also set. Critical for the [LLM control plane](docs/llm-control-plane.md) — an agent that picks a URL can only reach what the operator declared, with redirects re-validated. Help entry: `perch help --allow-host`.
- **SSRF / redirect protection on `http_*` / `download` — default-on.** Closed the classic redirect-following attack surface (SSRF to AWS metadata, pivot to local services, https→http downgrade).
  - **Default behaviour, no flag needed:**
    - Requests + redirect destinations are validated. The host (or every A/AAAA record it resolves to) must not be loopback (`127.0.0.0/8`, `::1`), link-local (`169.254.0.0/16` — the AWS / GCP / Azure metadata service), RFC 1918 private (`10/8`, `172.16/12`, `192.168/16`), IPv6 ULA (`fc00::/7`), unspecified (`0.0.0.0`, `::`), or multicast. Closes SSRF.
    - `https://X` redirecting to `http://Y` is refused. Closes scheme downgrade.
    - Cap of **5** redirect hops.
  - **Escape-hatch flags** for when the script genuinely needs to reach a private service:
    - `--allow-private-ips` — opt out of the SSRF check.
    - `--allow-scheme-downgrade` — opt out of the https-only redirect rule.
    - `--max-redirects N` — change the cap (`0` ≡ `--no-redirects`).
    - `--no-redirects` — refuse all redirects.
  - DNS-rebinding defense: a hostname with multiple A records gets ALL records checked; any private record makes the host private.
  - Applies to `http_get`, `http_post`, `http_put`, `http_delete`, `download`, and `http_status` — all converge on a single `runHTTP` helper now.
- **`perch help` — auto-generated CLI reference.** Single source of truth for every CLI flag, subcommand, and concept, with three surfaces:
  - **`perch help`** — top-level index grouped by Execution / Authoring / Security / Build / Agents / Concepts. Each row is a one-line synopsis with the flag/subcommand name.
  - **`perch help <TOPIC>`** — detail on one item. Topic is matched by exact name first, then substring fuzzy match. `perch help --no-shell`, `perch help shebang`, `perch help interpolation` all work. Multiple matches show a refinement list.
  - **`perch help --json`** — full machine-readable catalog (~37 topics today) with name, kind, group, synopsis, description, examples, doc URL, and see-also. Designed for agents and tooling: an LLM can fetch the catalog once and have the entire CLI surface as context.
  - Each topic carries a `doc_url` field pointing at the canonical doc page on GH Pages, so the help system stays a navigation index rather than a duplicate of the prose docs.
- **Error messages now point at `perch help`.** A blocked op call says `op "shell" is disabled by --no-shell — run perch help --no-shell for details`. An undeclared env var says `env var ${X} is not in --env allowlist — run perch help --env`. An unresolved placeholder says `run perch help interpolation`. The hint pattern is consistent so users and agents both know where to look.
- **Stdin input is untrusted by default — explicit opt-in for capabilities.** When `-f -` is used, perch applies the strictest posture automatically (`--no-shell --no-subprocess --no-network --no-write` + empty env allowlist) and requires the user to grant capabilities with new positive flags:
  - **`--allow-shell`** — re-enable shell ops
  - **`--allow-subprocess`** — re-enable pkg_install/kill_by_name/etc.
  - **`--allow-network`** — re-enable network ops
  - **`--allow-write`** — re-enable FS-mutation ops
  - **`--env A,B,C`** — declare visible host env vars (already existed; now serves the additional role of overriding the empty stdin default)
  - **`--trust-stdin`** — skip the deny-by-default entirely (when piping a `.perch` file you wrote yourself)
  - Banner becomes `🔒 stdin (untrusted): ...` and prints the exact `--allow-*` flags the user could pass to unlock each capability.
  - **File input is unchanged.** `perch -f file.perch` retains the existing "all allowed unless explicitly restricted" posture. The deny-by-default only applies to stdin because that's where untrusted scripts arrive (curl, paste, sibling process). Same model Deno uses: explicit-grant for things the user didn't already save to disk.
- **Stdin support — `perch -f -` reads from stdin.** Pipe a `.perch` file straight from any source: `curl URL | perch -f - <cmd>`, `cat script.perch | perch -f -`, `git show HEAD:commands.perch | perch -f - --check`. Combines with all restriction flags, so `curl URL | perch -f - --no-shell --no-write run` is a one-liner for safely running an untrusted remote script — never lands on disk, never touches anything outside the declared op set. `${script_path}` is `"-"` in this mode. Single change in `infra/capyloader/loader.go`: when path is `"-"`, read from `os.Stdin` via `io.ReadAll`.
- **Shebang scripts — `#!/usr/bin/env perch` works.** `.perch` files can now run as standalone scripts on Linux / macOS / WSL the same way Python or bash scripts do. Three new behaviours wire this up:
  - **`perch --init`** writes the shebang line at the top, sets the file `+x`, and includes a `main` command so `./commands.perch` (no args) runs a sensible default. Prints the exact `chmod +x` and `./commands.perch` commands so the user sees the script idiom on day one.
  - **`perch FILE [CMD] [ARGS]`** auto-detects when the first arg is a path to a `.perch` file (path-shaped + exists + regular file) and treats it as `-f FILE`. This is what makes the kernel-invoked `perch /abs/path/script.perch ARGS…` form work after the shebang.
  - **Shebang invocation with no command name** dispatches to a `main` command if the file declares one (Python / bash convention), otherwise lists available commands cleanly. Implemented via a new `HasCommand` probe on the run use case so we don't print "unknown command" noise before the listing.
  - Net result: a `.perch` file is now simultaneously a structured CLI surface AND a standalone executable script. `./deploy.perch up`, `perch -f deploy.perch up`, `perch --build` + `./deploy up` all behave identically.
  - The shebang line itself is `#` comment text to capy's parser — no grammar change required.
  - SKILL.md updated so AI-assisted authoring suggests the shebang in new files.
- **`perch --scan FILE`** — static security audit. Walks the program (no execution), produces:
  - **Capabilities needed** — shell? subprocess? network? writes? — broken out with the actual binaries called, the hosts contacted, and the path roots written. Each row that's NOT used carries an "add `--no-X` for free" hint.
  - **Env vars referenced** — every `${UPPERCASE_NAME}` interpolated in any string arg, so the `--env A,B,C` allowlist becomes a copy-paste.
  - **Risk findings** — HIGH / MED / LOW / INFO, each with a concrete `→` fix. Today's heuristics: `sudo` in shell (HIGH, escalation), `catch` forwarding `${proxy_args}` to shell (MED, open passthrough), unvalidated `${var}` interpolation inside shell args (LOW, possible injection), `make_executable` without `verify_sha256` pairing (MED, supply-chain).
  - **Recommended invocation** — assembled automatically. The tightest set of `--no-*` / `--allow-bin` / `--env` / `--max-runtime` / `--audit` flags that should still let the script work, followed by a "compared to bare perch" diff so the reviewer sees what each flag bought.
  - Pure Go, no new dependencies. ~400 LOC in `usecases/scan/`. Distinct from `--check` (which validates syntax) and `--audit FILE.ndjson` (which records runtime activity).
- **`perch --import script.sh`** — best-effort bash → `.perch` translator. Produces a scaffold the user can immediately run (`--check` passes, `--help` works, every command callable) while preserving original semantics by routing most lines through `shell` ops. Recognised patterns:
  - `#!/bin/bash` and `# comments` → file-level / inline comments
  - `NAME=value` at top level → `globals` entry, with `$VAR → ${VAR}` rewrite
  - `function f { … }` or `f() { … }` → `command f ... do ... end end`
  - `echo "literal"` (no metachars) → `print`
  - `cmd &` (background) → `shell_detached`
  - top-level executable lines → wrapped in a default `main` command
  - everything else → `shell` op with the original line preserved verbatim
  - The "pick whichever delimiter doesn't appear in your content" rule is applied automatically per line, so JSON / SQL / mixed-quote bodies get the right wrapper without manual escaping. Three-or-more-quote-types lines get a `# TODO: fix quoting` flag.
- **New doc: [docs/migrating-from-shell.md](docs/migrating-from-shell.md)** — three-option migration guide (Wrap / Translate / Rewrite) with a rewriting checklist mapping common bash idioms to perch ops.
- **Import-path variable expansion.** `${name}` placeholders in import paths are substituted at load time. Vocabulary matches the runtime auto-bound vars:

  ```capy
  import "${file_dir}/shared/aws.perch" as aws    # directory of THIS file
  import "${HOME}/.perch/team-ops.perch"           # user home
  import "${cache_dir}/perch/lib.perch"            # OS cache dir
  import "${PERCH_LIB_PATH}/utils.perch"           # env-var fallthrough
  ```

  Recognised names: `file_dir` / `script_dir` (alias) → directory of the importing file; `home` / `HOME`; `cache_dir`; `config_dir`; `temp_dir`; `exe_dir`; `user` / `USER`; any other `${NAME}` falls through to `os.LookupEnv`. Unknown names error at load time (not silently expanded to empty), so typos surface as `unknown placeholder ${X} in import path` rather than a confusing "file not found" later.

  Why this matters: relative imports already worked, but explicit `${file_dir}` makes the anchor visible to readers, and absolute forms (via `${HOME}` / `${cache_dir}`) unlock the team-wide-shared-library pattern (e.g. `${HOME}/.perch/team-ops.perch`) without depending on the user's cwd at invocation time.

  Three new unit tests cover `${file_dir}`, env-var fallthrough, and unknown-placeholder errors. ~70 LOC added; same callback-shape expander pattern as the interpreter's `${...}` interpolation (no cross-package dependency).

- **Multi-file imports** — `.perch` files can now compose. The most-named missing feature across all third-party reviews; ships in this release.

  ```capy
  #!/usr/bin/env perch
  name "main"

  import "./shared.perch"               # flat: commands callable by bare names
  import "./aws.perch" as aws            # namespaced: `run aws.upload` etc.

  command deploy
      do
          run notify                    # from shared.perch
          run aws.upload                # from aws.perch
      end
  end
  ```

  Semantics, all enforced:

  - **Cycle detection.** `a imports b imports a` → "import cycle detected: /abs/a.perch already on the import stack". No infinite recursion.
  - **Conflict detection.** Two flat imports defining the same command, or a flat import + an importer-local command with the same name → static error from both `Load` and `--check`. No silent winner.
  - **Path resolution against the importing file.** `import "./x"` in `/a/b/main.perch` looks for `/a/b/x.perch`, not `cwd/x.perch`.
  - **Globals merge parent-wins.** Importer's value silently overrides the imported default — "default with override" without ceremony.
  - **`private` commands** hidden from flat import, accessible via aliased import (`alias.privatename`).
  - **Catch handlers** do NOT propagate — only the root file's catch is active.
  - **Namespacing via single-label dots.** `import "x" as ns` exposes `ns.cmd` callable from both the CLI (`perch ns.cmd`) and from other commands (`run ns.cmd`). New capy grammar variant `run NS.CMD` makes the cross-file call site readable.

  Implementation: ~190 LOC. `Load` becomes a recursive resolver with a `visited` set; `parseEventStream` collects import directives and returns them alongside the program; `mergeProgram` folds imported commands + globals into the importer with the rules above. Four new unit tests in `infra/capyloader/loader_test.go`.

  This is the structural feature that closes the "weak composability ceiling" critique flagged across the README review rounds. perch now scales to multi-file projects (within reason — see [README: Where it breaks down](README.md#where-it-breaks-down) for the new ceiling).

- **Three string delimiters documented** — `"..."`, `'...'`, `` `...` ``. All three are equivalent (raw, interpolation-active), so authors pick whichever doesn't appear in their content. Massive readability win for JSON / SQL / shell-with-quotes:

  ```capy
  # Painful before — escape every quote:
  let body = format "{\"order_id\":\"${order_id}\",\"amount\":${amount}}"

  # Now (single quotes, no escapes needed):
  let body = format '{"order_id":"${order_id}","amount":${amount}}'

  # SQL or shell with both " and ' — use backticks:
  shell `psql -c "SELECT * FROM t WHERE name='${name}'"`
  ```

  No code change — the three delimiters always worked but were undocumented. Removed an incorrect SKILL.md note claiming `\n` escapes are supported (they're not — capy strings are pure raw strings, no backslash escapes). Updated language.md / SKILL.md / applications.md / llm-control-plane.md to use the cleaner delimiters consistently.
- **`perch --dry-run <cmd>`** and **`perch --ask <cmd>`** — preview a command's ops before they execute.
  - `--dry-run` walks every op, prints its kind + interpolated args, and skips the handler. Capture variables get set to `""` so subsequent `${x}` interpolation still works.
  - `--ask` is the same plan, interactively. For each op the user chooses: `y` (run), `n` (skip), `a` (run this op then everything else without further asking), or `q` (stop immediately).
  - The interpolated args shown are exactly what the handler receives — no daydreaming. Block ops show `{N body ops}` and are entered (or skipped wholesale) by the user's choice.
  - Implementation is a `BeforeOp` hook on `interpreter.Interpreter` — zero overhead when unset, no change to existing handlers. Stacks with the restriction flags (e.g. `perch --no-shell --ask deploy` previews ops AND fences shell at runtime).
- **`--audit FILE.ndjson`** — structured trace of every op the interpreter dispatches. One line per op call: timestamp, command name, op kind, interpolated args, duration, ok/error. Plus session-start and session-end records. Path `-` means stdout; a file path appends. The OS-grade observability piece — pair with the security flags and you have a full forensic record of what an agent (or user, or CI) actually ran. Same shape across CLI / web / REPL / MCP / built binary, because they all go through the same op dispatch.
- **`--max-runtime SECS`** — wall-clock budget for the whole invocation. Checked before each op dispatch; the next op after the deadline returns `↪ stopped: --max-runtime exceeded` and exits non-zero. Best-effort (can't interrupt a long-running shell mid-call without context-aware exec wiring), but caps runaway scripts.
- **New positioning doc: [docs/os-in-a-program.md](docs/os-in-a-program.md)** — maps OS concepts (syscalls, process model, capability system, identity / env, resource limits, audit log, std lib, package manager, config / state, frontends) to perch features. Shipped column, designed column, not-in-scope column. The "what perch is *for*" page.
- **Closed the subprocess escape hatch.** Subprocess restrictions only fenced perch's *own* op dispatch — `shell "echo $SECRET"` happily inherited the full host env, and any shell op could spawn any binary. Three new defenses, composable:
  - **Subprocess env scrubbing (automatic with `--env`).** When `--env A,B,C` is set, spawned processes inherit *only* the named host env vars — no more leaking `$SECRET_KEY` to a `shell` subprocess that perch's own interpolation would have rejected.
  - **`--allow-bin NAME[,NAME…]`** — when shell is allowed but you want to bound *what* it spawns, declare the basenames permitted. First non-env-assignment token's basename must match. Skips `FOO=bar` leading assignments. Blocked calls cite the flag: `shell: binary "echo" is not in --allow-bin`.
  - **`--no-shell-metachars`** — rejects `|`, `>`, `<`, `&`, `;`, `` ` ``, `$(` in shell args. Stops shell-injection-style escapes inside an otherwise-allowed call.
  - **Reclassified `bin_version` and `os_version` as subprocess ops** — they fork-exec but were missed by `--no-subprocess`. Now correctly blocked under the same flag as `pkg_install` etc.
  - All four layers documented in [docs/sandbox.md §0c](docs/sandbox.md#0c-the-subprocess-escape-hatch--and-the-layered-defense), including an honest "what this still does NOT cover" subsection (an allowed binary reading FS / opening sockets — kernel-level sandboxing required for those, not in scope).
- **Composable restriction flags** + **`--env` host env-var allowlist** — the CLI side of the sandbox design at [sandbox.md](docs/sandbox.md). Each flag names exactly what it disables; flags compose.
  - **`--no-shell`** — disables `shell`, `shell_output`, `shell_detached`, `shell_in`, `try_shell`.
  - **`--no-subprocess`** — disables `pkg_install`, `pkg_uninstall`, `kill_by_name`, `process_running`.
  - **`--no-network`** — disables every network-touching op (`http_*`, `download`, `dns_lookup`, `port_*`, `wait_for_*`, `public_ip`, `local_ip`, `mac_address`, `interfaces`).
  - **`--no-write`** — disables every filesystem-mutation op (`write_file`, `append_*`, `cp`, `mv`, `rm`, `mkdir`, `chmod`, `touch`, archive create/extract, `symlink`, `bundle_extract`, `bundle_dir`, …).
  - **`--env A,B,C`** (or `--env=A,B,C`, or repeated `--env A --env B`) — restricts which host env vars resolve via `${NAME}` fallthrough. Bare `--env` = "no env vars visible." Auto-bound names (`home`, `cache_dir`, `exe_path`, `is_macos`, …) are NOT env vars and are unaffected.
  - Blocked op call returns `op "X" is disabled by --no-Y (see https://luowensheng.github.io/perch/sandbox/)`. Blocked env lookup returns `env var ${SECRET_KEY} is not in --env allowlist (declare with --env SECRET_KEY)`.
  - Whenever any restriction is active, perch prints a one-line `🔒 security: …` banner naming every active flag so the posture is visible in CI logs and code review.
  - **`perch --restrictions`** lists every flag with the exact ops it blocks.
  - **Replaces** the earlier `--mode safe|offline|read-only|pure` knob. The `--mode` flag name conveyed marketing intent rather than mechanism; `--no-shell` says what it does. Strictly more expressive, too: you can take `--no-shell --no-network` without taking `--no-write` along.
  - Applies to every surface uniformly: CLI, `--server`, `--shell`, embedded binaries built via `--build`.
- **~30 auto-bound variables** every command can use as `${name}` without declaration — the building blocks of cross-platform install / build / uninstall scripts:
  - **OS flags**: `is_windows`, `is_macos`, `is_linux`, `is_unix`, `is_arm64`, `is_amd64` — write `if is_windows ... end` instead of `if os == "windows"`.
  - **Path conventions**: `path_sep`, `path_list_sep`, `exe_ext`, `null_device`, `shell_name` — the things that differ per OS.
  - **Standard directories**: `home`, `config_dir`, `cache_dir`, `data_dir`, `temp_dir` — OS-correct (`%APPDATA%` on Windows, `~/Library/Application Support` on macOS, `~/.local/share` on Linux).
  - **The binary itself**: `exe_path`, `exe_dir`, `exe_name` — the running perch (or built) binary, with symlinks resolved.
  - **The script**: `script_path`, `script_dir` — the loaded `.perch` source (empty when embedded).
  - **Identity & system**: `user`, `uid`, `hostname`, `cpu_count`, `pid`, `now_unix`.
- **~40 new cross-platform ops** for install / build / uninstall workflows:
  - **Binary discovery**: `which`, `has_bin`, `bin_version`.
  - **Path manipulation**: `path_join`, `path_dir`, `path_base`, `path_ext`, `path_abs`, `path_clean`, `path_rel`, `path_with_ext`, `is_abs`, `to_slash`, `from_slash`, `expand_path` (handles `~` and env vars).
  - **PATH and shell rc**: `path_contains`, `shell_rc_path`, `add_to_path` (idempotent — edits `.zshrc` / `.bashrc` / fish config; prints `setx` instructions on Windows), `link_into_path`.
  - **Package manager**: `detect_pkg_mgr` (brew / apt / dnf / pacman / apk / zypper / winget / choco / scoop), `pkg_install`, `pkg_uninstall`, `pkg_installed`.
  - **System probes**: `is_admin`, `is_ci`, `is_tty`, `os_version`, `env_default`, `env_has`.
  - **Network**: `port_free`, `find_free_port`, `wait_for_port`, `wait_for_url`, `http_status`, `local_ip`, `public_ip`, `mac_address`, `interfaces`.
  - **Filesystem helpers**: `make_executable`, `ensure_dir`, `copy_dir`, `append_file`, `append_line`, `ensure_line_in_file` (idempotent), `replace_in_file`, `backup_file`, `glob`, `list_dir`, `symlink`, `read_link`, `mktemp_dir`, `mktemp_file`.
  - **Process**: `try_shell` (probe without erroring), `shell_in` (explicit cwd), `process_running`, `kill_by_name`.
  - **Integrity**: `verify_sha256` (file vs. expected hex).
- Static validator (`--check`) now knows every auto-bound name, so `${cache_dir}` / `${exe_dir}` / `${is_windows}` no longer trip "unknown placeholder."
- **`perch --build --include <path>`** — bundles an arbitrary file tree (file or directory) as a gzipped tarball inside the produced fat binary. Skips `.git`, `node_modules`, `__pycache__`, `.venv`, `venv`, `.tox`, `dist`, `.cache`, and `.DS_Store` by default.
- **Three new ops** for accessing the embedded archive at runtime:
  - **`bundle_hash`** — SHA-256 hex of the embedded archive. Used as a content-addressable version id.
  - **`bundle_extract DST`** — extract the archive to `DST` (idempotent).
  - **`bundle_dir`** — lazy-extract to an OS temp dir; cached for the process. Useful for read-only "run from the bundle" flows.
- **Fat-binary footer format v2** (`PRCHEMB2`). Adds an optional archive section between the program JSON and the footer. V1 binaries (`PRCHEMB1`) continue to load — existing fat binaries built with older perch keep working.
- **String-typed globals are now interpolated at seed time.** `${HOME}/.cache/foo` in a global expands to `/Users/.../cache/foo` before any command runs, so cross-referencing globals + host env in a global definition just works.
- **`perch --install-lsp`** — installs `perch-lsp` via `go install`. Prints the resolved install path. Fails with a clear actionable error if Go isn't on `$PATH`.
- **`perch --install-vscode`** — installs `perch-lsp` AND the perch VS Code extension. The extension files are **embedded in the `perch` binary** via `go:embed`, so no repo checkout is required. Requires `node`/`npm` and the VS Code `code` CLI.
- **`perch-lsp`** — Language Server Protocol implementation for `.perch` files. Provides diagnostics (parse + static `--check`), context-aware completion (top-level / command-config / arg-block / `do`-body), hover docs for every keyword and op, and document outline (commands + args). Spawned by the VS Code extension automatically; Neovim/Helix/Zed setup snippets in [`docs/lsp.md`](docs/lsp.md).
- **`perch --check`** (alias `--validate`) — statically check a `.perch` file without running anything. Catches: invalid arg types, default values that don't match the declared type, duplicate arg names, colliding positional indexes, missing `run TARGET` / `on_signal HANDLER` references, unknown op kinds, unresolved `${name}` placeholders.
- **`perch <command> --help`** — per-command help block with usage line, arguments table (with type / default / required / description), env vars, modifiers, examples, and the source file path.
- **"Did you mean…?"** — when an unknown command is typed and there's no `catch` handler, perch suggests the closest matches via Levenshtein distance.

### Changed (breaking)

- **Arg declarations are now blocks.** The old `arg NAME TYPE "desc"` + `arg_default NAME VALUE` + `arg_index NAME N` + `arg_optional NAME` flat statements are gone. Each arg is now a `arg NAME ... end` block with labelled inner fields:

  ```capy
  arg target
      type string
      default "darwin"
      description "Target OS"
  end
  ```

  Inner fields: `type` (required), `default`, `description`, `optional`, `index`. See [docs/language.md](docs/language.md#arg-blocks).

- **Runtime interpolation marker switched from `{{name}}` to `${name}`.** Capy-native; consistent with shell-style. Escape literal shell variables with `\${VAR}`.

## [0.1.0] — 2026-05-26

### Added

- Initial public release. User files use the `.perch` extension; perch's own grammar definition (consumed by capy) lives at `infra/capyloader/lib.capy`.
- DSL defined entirely via [capy](https://luowensheng.github.io/capy) (`infra/capyloader/lib.capy`).
- Four CLI modes:
  - default: run a named command from `commands.perch`
  - `--server`: HTTP UI with NDJSON-streaming `/api/exec` endpoint
  - `--shell`: interactive REPL with persistent bindings
  - `--build`: bundle the parsed program into a self-extracting fat binary
- Built-in op catalog (~70 ops): process / file system / strings / hashing / encoding / HTTP / compression / archives / time / regex / network / system.
- Block ops: `if os == "..."`, `if arch == "..."`, `if exists "..."`, `if A == B`, `if A != B`, `if A > B`, `if A < B`, `if not A`, `if A`.
- `let NAME = OP ARG` capture syntax (0, 1, and 2-arg forms).
- `catch NAME ... end` fallback for unknown commands.
- Cross-platform: tested on macOS, Linux, Windows via CI matrix.
- VHCO layout (domain / features / usecases / io / infra / orchestrator).
- Five-page docs site published via GitHub Pages.
- Four runnable demos under `demos/`.
- Claude Code skill at `skills/perch/SKILL.md`.
- Three tutorials under `docs/tutorials/` (Makefile conversion, ship a tool, cross-platform installer).
- Install scripts (`scripts/install.sh`, `scripts/install.ps1`).
- Homebrew formula at `Formula/perch.rb` (release-workflow fills in sha256s).
- SVG brand assets (`assets/logo.svg`, `assets/social.svg`).
- GitHub issue + PR templates.
- VS Code extension scaffold at `editors/vscode-perch/`.
- Tree-sitter grammar at `editors/tree-sitter-perch/`.
- Shell completions for bash/zsh/fish, emittable via `perch --completions SHELL`.
- `perch-mcp` MCP server at `cmd/perch-mcp/` for AI-agent integration.

[Unreleased]: https://github.com/luowensheng/perch/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/luowensheng/perch/releases/tag/v0.1.0

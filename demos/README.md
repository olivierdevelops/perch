# perch demos

Each subfolder is a self-contained `commands.perch` you can copy into your own project. Run any of them with `perch <command>` from inside the demo folder.

## Core demos (the perch story)

| Demo | What it shows | Try |
|---|---|---|
| [01-hello](01-hello/) | Globals, args, `let`, `print`. The 30-second intro. | `perch hello -name=World` |
| [02-cross-platform-setup](02-cross-platform-setup/) | `if os == "..."` branching that installs deps via brew/apt/choco. | `perch setup` |
| [03-go-project](03-go-project/) | Build, test, lint a Go project. Could replace your Makefile. | `perch build -target=linux` |
| [04-portable-cli](04-portable-cli/) | Bundle a `commands.perch` into a single portable binary via `perch --build`. The killer feature. | `perch --build -o greet && ./greet hello -name=Alice` |
| [05-python-installer](05-python-installer/) | Embed a Python project alongside perch with `--include`. One binary that installs itself. | See its README |
| [06-declared-requirements](06-declared-requirements/) | A `requires` manifest — declares every bin (with version floors + a custom version probe), env var, and host. Strict mode: undeclared use errors; `--check` proves feasibility without running. | `perch -f commands.perch --check` |

## WASM demos (`wasm_run` end-to-end)

Real Go programs compiled to WebAssembly, executed inside perch under
capability-gated WASI. Each ships the pre-built `.wasm`, the source, a
`commands.perch`, and example data so you can run them without a Go
toolchain. Each can also be **bundled into a single portable binary**
via `perch --build --include .` for shipping.

| Demo | What it shows | Killer use case |
|---|---|---|
| [wasm-hello](wasm-hello/) | The minimum — `wasm_run` + four capability declarations. Shows what's visible vs invisible. | First-look "what does this do?" |
| [wasm-schema-validator](wasm-schema-validator/) | A 200-line JSON Schema validator. Deterministic CI replacement for `yq`+`jq`+`ajv`. | Validate configs / fixtures / API responses in CI without Python or Node |
| [wasm-policy-check](wasm-policy-check/) | K8s manifest policy enforcement: registry allowlist, resource limits, no-latest, no-privileged, required labels. | Pre-deploy compliance gate |
| [wasm-diff-summary](wasm-diff-summary/) | Git-diff → structured JSON summary with risk heuristics. **Designed as an agent-callable tool** via MCP. | Safe LLM agent PR analysis without giving the agent a shell |
| [**wasm-plugin-host**](wasm-plugin-host/) | **Zero-trust plugin runtime — 4 legit plugins + 1 deliberately malicious one demonstrating EVERY escape attempt fails by construction**. The killer demo. | AI-generated plugins running safely without trusting the AI |

### Quick start for the WASM demos

```sh
# Run a demo directly (no install — just perch)
$ perch -f demos/wasm-schema-validator/commands.perch demo_good
/ro/fixtures/good-alice.json: ✓ valid

# Or bundle it into a one-file tool
$ perch --build -f demos/wasm-schema-validator/commands.perch \
    --include demos/wasm-schema-validator \
    -o validator-tool

$ ./validator-tool demo_good
/ro/fixtures/good-alice.json: ✓ valid
```

The pre-built `.wasm` artifacts are committed alongside their Go
source. Rebuild any of them with `perch <demo>/rebuild` (needs
Go 1.21+).

## Reading order

If you're new to perch:
1. Start with **01-hello** for the language basics.
2. Read **02-cross-platform-setup** + **03-go-project** for shell-based recipes.
3. Try **04-portable-cli** + **05-python-installer** for the `perch --build` story.
4. Read **06-declared-requirements** for the `requires` manifest — what the file needs, declared and enforced.
5. Then **wasm-hello** to see the capability boundary in action.
6. Then **wasm-schema-validator** for the "actually shippable CI tool" pattern.
7. Then **wasm-diff-summary** for the agent-safety pattern.
8. Then **wasm-plugin-host** for the architectural punch — AI-generated WASM plugins running under capability gates, with a deliberately malicious plugin demonstrating that every escape attempt fails *by construction*.

## See also

- [`docs/wasm.md`](../docs/wasm.md) — `wasm_run` reference
- [`docs/wasm-walkthroughs.md`](../docs/wasm-walkthroughs.md) — 5 end-to-end real-world workflows
- [`recipes/`](../recipes/) — 22 ready-to-run `.perch` files for common dev tools (Redis, Postgres, AI stack, etc.)

More demos welcome — open a PR adding a folder under `demos/` with a `commands.perch` and a short `README.md`.

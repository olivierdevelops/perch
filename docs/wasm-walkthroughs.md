# Real-world walkthroughs — `wasm_run` in production-shaped workflows

> **This page is the "how do I actually use this?" guide.** For the
> reference (every flag, every capability declaration), see
> [`wasm.md`](wasm.md). For the design rationale, see
> [`ideas/12-wasm-execution.md`](https://github.com/luowensheng/perch/blob/main/ideas/12-wasm-execution.md).
>
> Each walkthrough below is a complete, runnable example: the WASM
> source (Go), the build command, the `.perch` file, sample output,
> and the composition patterns (parallel / retry / cache / sandbox)
> that make it useful in practice.

---

## When to reach for `wasm_run`

Three honest mental categories:

| Reach for `shell` when… | Reach for `wasm_run` when… |
|---|---|
| Wrapping docker / kubectl / git / aws / brew — orchestrating *other* tools | Running validation / transformation / classification logic *on your own machine* |
| The bash one-liner is the natural form | Determinism + portability matter (same result on every dev's laptop AND in CI) |
| Glue work | Letting an AI agent execute computation safely |
| Migrating existing scripts | Loading third-party plugins (you can't trust their source but you can constrain their environment) |
| You need the tool the user already has installed | You can ship the `.wasm` artifact alongside the `.perch` file |

**Most real `.perch` files mix both.** The walkthroughs below all do.

## How data flows in and out of a WASI module

WASI is Unix-shaped. Pick whichever of these matches the data:

| Direction | Mechanism | Best for |
|---|---|---|
| **In: argv** | `wasm_arg "VALUE"` | Short flags, file paths, structured tokens |
| **In: env vars** | `wasm_env "K1,K2"` | Config, secrets you explicitly allow through |
| **In: files** | `wasm_mount_read "PATH"` mounted at `/ro/<basename>` | Input data: source code, schemas, source files |
| **In: stdin** | A pipe into the `perch` invocation | Arbitrary blobs the module reads with `bufio.NewReader(os.Stdin)` |
| **Out: stdout** | Module's `fmt.Println` / `os.Stdout.Write` | Results, logs, JSON output |
| **Out: stderr** | Module's `os.Stderr.Write` | Diagnostics, warnings |
| **Out: files** | `wasm_mount_write "PATH"` mounted at `/rw/<basename>` | Generated artifacts, reports, large outputs |
| **Out: exit code** | Module's `os.Exit(N)` | Pass/fail signal — perch treats non-zero as op failure |

These compose. A validator typically takes input via `mount_read` + argv flags, writes a JSON report via `mount_write`, and signals success via exit 0.

---

## Walkthrough 1 — Markdown frontmatter validator (deep)

**The problem.** A docs repo has 200+ markdown files with YAML frontmatter. We want CI to catch posts missing required fields (`title`, `date`, `tags`) before they ship. Today: a 60-line bash script that greps + awks frontmatter blocks, breaks on edge cases (multi-line values, escaped quotes), and is impossible to test.

**Why `wasm_run`.** A proper YAML parser is straight-forward to write in Go (4 imports, 30 lines). Compiled to WASM, it runs identically on every dev's laptop and in CI. No `yq` to install, no version-skew between machines.

### The module

`tools/frontmatter-validator/main.go`:

```go
// frontmatter-validator.wasm
//
// Reads a markdown file from /ro/in/<NAME>, parses its YAML
// frontmatter, and verifies the required fields are present.
// Exits 0 on success, 1 on missing fields, 2 on parse error.
//
// Build: GOOS=wasip1 GOARCH=wasm go build -o frontmatter.wasm .
package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"

	"gopkg.in/yaml.v3"
)

type frontmatter struct {
	Title string   `yaml:"title"`
	Date  string   `yaml:"date"`
	Tags  []string `yaml:"tags"`
}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: frontmatter <file>")
		os.Exit(2)
	}
	path := os.Args[1]
	f, err := os.Open(path)
	if err != nil {
		fmt.Fprintln(os.Stderr, "open:", err)
		os.Exit(2)
	}
	defer f.Close()

	// Walk the file to extract the frontmatter block.
	scan := bufio.NewScanner(f)
	var lines []string
	in := false
	for scan.Scan() {
		line := scan.Text()
		if line == "---" {
			if !in {
				in = true
				continue
			}
			break
		}
		if in {
			lines = append(lines, line)
		}
	}

	var fm frontmatter
	if err := yaml.Unmarshal([]byte(strings.Join(lines, "\n")), &fm); err != nil {
		fmt.Fprintf(os.Stderr, "%s: yaml: %v\n", path, err)
		os.Exit(2)
	}

	missing := []string{}
	if fm.Title == "" {
		missing = append(missing, "title")
	}
	if fm.Date == "" {
		missing = append(missing, "date")
	}
	if len(fm.Tags) == 0 {
		missing = append(missing, "tags")
	}
	if len(missing) > 0 {
		fmt.Fprintf(os.Stderr, "%s: missing: %s\n", path, strings.Join(missing, ", "))
		os.Exit(1)
	}
	fmt.Printf("%s: ok\n", path)
}
```

Build:

```sh
cd tools/frontmatter-validator
go mod init validator && go mod tidy
GOOS=wasip1 GOARCH=wasm go build -o ../../frontmatter.wasm .
```

Result: a ~3 MB `frontmatter.wasm` that takes a file path, prints `OK` or `MISSING …`, exits 0 or 1.

### The `.perch` file

`docs-ci.perch`:

```capy
name "docs-ci"
about "Validate every markdown file's frontmatter"

command validate_one
    description "Validate ONE file (used internally by validate_all)"
    arg path
        type string
        index 0
    end
    do
        wasm_run "${script_dir}/frontmatter.wasm"
            wasm_arg "/ro/posts/${path}"
            wasm_mount_read "${script_dir}/posts"
        end
    end
end

command validate_all
    description "Validate every .md under ./posts"
    do
        let files = glob "${script_dir}/posts/*.md"
        for_each "${files}" file
            wasm_run "${script_dir}/frontmatter.wasm"
                wasm_arg "/ro/posts/${file}"
                wasm_mount_read "${script_dir}/posts"
            end
        end
    end
end

command test_validator_catches_missing_title
    test
    test_allow_write
    do
        write_file "${script_dir}/posts/_bad.md" "---\ndate: 2026-01-01\ntags: [a]\n---\nbody"
        let r = try_shell "perch -f ${script_dir}/docs-ci.perch validate_one /ro/posts/_bad.md"
        rm "${script_dir}/posts/_bad.md"
        assert_neq "${r}" "0"
    end
end
```

### Running it

```sh
$ perch -f docs-ci.perch validate_all
/ro/posts/welcome.md: ok
/ro/posts/launch-2025.md: ok
/ro/posts/sketch.md: missing: title, tags
↪ exit status: 1
```

Exit code is non-zero on any failure — drop it into CI directly:

```yaml
# .github/workflows/docs.yml
- run: perch -f docs-ci.perch validate_all
```

### What makes this strong

- **Determinism.** The validator gives bit-identical output on every machine — no Python/Ruby/Node version drift.
- **Auditability.** The `.wasm` is a single artifact you can sha256-pin in CI. Reviewers know exactly which validator ran.
- **Capability boundary.** The validator literally cannot read anything outside `/ro/posts/`. A bug in the YAML parser can't accidentally exfiltrate `/etc/passwd` because `/etc/passwd` isn't reachable from the module.
- **Composable with everything else.** Add `parallel` around the `for_each` to validate 200 files concurrently. Add `cache "frontmatter-${sha256_file('frontmatter.wasm')}-${file}" "7d"` to skip re-validating files that haven't changed.

### Adding parallelism

```capy
command validate_all
    do
        let files = glob "${script_dir}/posts/*.md"
        parallel
            for_each "${files}" file
                wasm_run "${script_dir}/frontmatter.wasm"
                    wasm_arg "/ro/posts/${file}"
                    wasm_mount_read "${script_dir}/posts"
                end
            end
        end
    end
end
```

200 files validate in ~3 seconds instead of ~30. Module bytecode is compiled once (sha256-keyed in-process cache); only the per-file invocation cost.

---

## Walkthrough 2 — JSON Schema validator with caching

**The problem.** A monorepo has 500+ JSON files: config schemas, API fixtures, OpenAPI specs. We want them all validated against their respective schemas in CI — but only the changed files should re-validate.

**Why `wasm_run`.** Same determinism story plus the `cache` block becomes meaningful here. Each `(schema-hash, file-hash)` pair maps to a cached pass/fail.

### The module

`tools/jsonschema-validator/main.go` uses [`github.com/santhosh-tekuri/jsonschema/v6`](https://github.com/santhosh-tekuri/jsonschema):

```go
// jsonschema-validator.wasm
// Args: <schema.json> <document.json>
// Exit codes: 0 valid · 1 invalid · 2 parse/load error
package main

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/santhosh-tekuri/jsonschema/v6"
)

func main() {
	if len(os.Args) < 3 {
		fmt.Fprintln(os.Stderr, "usage: validator <schema> <document>")
		os.Exit(2)
	}
	schemaPath, docPath := os.Args[1], os.Args[2]
	c := jsonschema.NewCompiler()
	sch, err := c.Compile(schemaPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "compile schema:", err)
		os.Exit(2)
	}
	data, err := os.ReadFile(docPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "read doc:", err)
		os.Exit(2)
	}
	var v any
	if err := json.Unmarshal(data, &v); err != nil {
		fmt.Fprintln(os.Stderr, "parse doc:", err)
		os.Exit(2)
	}
	if err := sch.Validate(v); err != nil {
		fmt.Fprintf(os.Stderr, "%s: %v\n", docPath, err)
		os.Exit(1)
	}
	fmt.Printf("%s: valid\n", docPath)
}
```

Build:

```sh
GOOS=wasip1 GOARCH=wasm go build -o jsonschema.wasm .
```

### The `.perch` file with caching

```capy
name "schema-ci"

import "./_lib.perch"

command validate_dir
    description "Validate every JSON file in a dir against its schema"
    arg schema
        type string
        index 0
        description "Path to the JSON schema"
    end
    arg dir
        type string
        index 1
        description "Directory of JSON documents to validate"
    end
    do
        let schema_hash = sha256_file "${schema}"
        let docs = glob "${dir}/*.json"
        for_each "${docs}" doc
            let doc_hash = sha256_file "${doc}"
            cache "schema-${schema_hash}-${doc_hash}" "30d"
                wasm_run "${script_dir}/jsonschema.wasm"
                    wasm_arg "/ro/schema/${schema}"
                    wasm_arg "/ro/docs/${doc}"
                    wasm_mount_read "${schema}"
                    wasm_mount_read "${dir}"
                end
            end
        end
    end
end
```

### Running it

First run (cold cache):

```sh
$ perch validate_dir ./api.schema.json ./fixtures
/ro/docs/user.json: valid
/ro/docs/order.json: valid
/ro/docs/payment.json: valid
↪ ~2.1s wall-clock, 500 files validated
```

Second run (warm cache, nothing changed):

```sh
$ perch validate_dir ./api.schema.json ./fixtures
↪ cache hit: schema-3f9a2b…-c8d1e7… (replayed 0 bindings, 29d23h left)
↪ cache hit: schema-3f9a2b…-7b2c9d… (replayed 0 bindings, 29d23h left)
…
↪ ~120ms wall-clock, all hits
```

After changing one fixture file:

```sh
$ touch fixtures/user.json
$ perch validate_dir ./api.schema.json ./fixtures
/ro/docs/user.json: valid          # re-validated (hash changed)
↪ cache hit: order.json
↪ cache hit: payment.json
…
↪ ~145ms wall-clock — only the changed file actually re-ran
```

### Composition pattern: pipeline of validators

If you have multiple schemas applying to different file globs, use `parallel`:

```capy
command validate_all
    do
        parallel
            validate_dir "./schemas/user.json" "./fixtures/users"
            validate_dir "./schemas/order.json" "./fixtures/orders"
            validate_dir "./schemas/payment.json" "./fixtures/payments"
        end
    end
end
```

All three validations run concurrently; each one's cache layer is independent. Adding a fourth schema is one line.

---

## Walkthrough 3 — AI agent safe-execution surface (via MCP)

**The killer use case.** You're building an internal tool that lets an LLM agent process user-uploaded files. The agent decides what processing to apply (validate, extract, normalize, classify). You absolutely cannot let the agent shell out to arbitrary binaries.

**Why `wasm_run`.** The MCP server exposes verbs the agent can call. Each verb internally executes a WASM module with the user's data mounted in. The agent specifies *which* operation to apply (typed argument); perch dispatches to the right WASM module. The agent cannot escape the module's declared capabilities — no syscalls, no network (in v1), no host filesystem beyond the declared mount.

### The setup

- `ops.perch` declares verbs the agent can call: `validate_json`, `extract_emails`, `normalize_address`, `classify_intent`.
- Each verb runs a different `.wasm` module under tightly scoped capabilities.
- The user uploads a file; the agent decides which operation; perch routes.

### The `.perch` file

`ops.perch`:

```capy
name "agent-ops"
about "MCP-exposed verbs — every operation runs in a constrained WASM module"

command validate_json
    description "Validate a JSON file against a schema"
    arg file
        type string
        description "Path to a JSON document (under ./uploads)"
    end
    arg schema
        type string
        description "Schema name (must match a file in ./schemas)"
    end
    do
        # Per-call sandbox: even though we already use wasm_run,
        # belt-and-braces — the perch sandbox prevents any escape
        # ops in the body from shelling out anyway.
        sandbox "no_shell,no_network,no_subprocess"
            wasm_run "${script_dir}/wasm/jsonschema.wasm"
                wasm_arg "/ro/schemas/${schema}.json"
                wasm_arg "/ro/uploads/${file}"
                wasm_mount_read "${script_dir}/schemas"
                wasm_mount_read "${script_dir}/uploads"
            end
        end
    end
end

command extract_emails
    description "Extract email addresses from a text file"
    arg file
        type string
    end
    do
        sandbox "no_shell,no_network,no_subprocess"
            wasm_run "${script_dir}/wasm/extract-emails.wasm"
                wasm_arg "/ro/uploads/${file}"
                wasm_mount_read "${script_dir}/uploads"
            end
        end
    end
end

command normalize_address
    description "Normalize a postal address string"
    arg address
        type string
    end
    do
        sandbox "no_shell,no_network,no_subprocess"
            wasm_run "${script_dir}/wasm/normalize-address.wasm"
                wasm_arg "${address}"
            end
        end
    end
end

command classify_intent
    description "Classify a text snippet's intent"
    arg text
        type string
    end
    do
        sandbox "no_shell,no_network,no_subprocess"
            wasm_run "${script_dir}/wasm/classify-intent.wasm"
                wasm_arg "${text}"
                wasm_mount_read "${script_dir}/models"
            end
        end
    end
end
```

### Wire perch-mcp to the file

```sh
$ perch-mcp \
    --no-shell \
    --no-network \
    --no-subprocess \
    --env "" \
    --max-runtime 30 \
    -f ops.perch
```

Now an MCP-aware agent (Claude Desktop, Cursor, Zed, etc.) can call:

```
perch_run name="validate_json" file="user-upload.json" schema="user"
perch_run name="extract_emails" file="signup-form.txt"
perch_run name="normalize_address" address="123 Main St, NYC"
perch_run name="classify_intent" text="cancel my subscription"
```

Each call:

1. MCP validates the arg shape (typed args from the `arg` blocks — `file` is `string`, etc.)
2. perch dispatches the named verb
3. The verb's body runs the corresponding WASM module
4. The module sees ONLY what was declared: argv, the mount paths, no env, no network
5. The module's stdout is returned to the agent as the tool's result

### What the agent CANNOT do

- Invoke an undeclared verb (typed error: `command "X" not declared`)
- Shell out (the verb wraps `wasm_run` in a `sandbox "no_shell"` block; even if a future verb forgot the sandbox, `perch-mcp --no-shell` is the outer gate)
- Pass an undeclared arg type (typed error from MCP schema)
- Reach the network (Preview 1 has no socket import; `--no-network` is the outer gate too)
- Read files outside `./uploads` (the wasm mount is the only fs root the module sees)
- Inject shell metacharacters (the arg is passed to `wasm_arg`, not to `shell`)

### Layered defense

The story for compliance / security review:

1. **MCP schema** — agent's tool call must match the typed arg shape
2. **perch verb dispatch** — only declared verbs callable
3. **Per-verb `sandbox`** — even if a verb's body had non-wasm ops, they'd be denied
4. **`wasm_run` boundary** — module cannot syscall; sees only declared imports
5. **WASI capability declarations** — only mounted dirs reachable; only env-allowlist visible
6. **perch-mcp CLI flags** — outermost `--no-*` gates as belt-and-braces

Each layer is enforceable separately. Reviewers can sign off on the *file* (it's plain text); operators can sign off on the *invocation flags* (one line in a config). Auditing reduces to "do you trust the .wasm modules?" — and you can sha256-pin those.

### What this replaces

Without perch + WASM, you'd build:

- A FastAPI / Express service exposing typed endpoints
- A subprocess-spawning layer (or worse: in-process arbitrary Python)
- Custom rate limiting + audit logging
- A container per worker
- Trust boundary policy in code review

With perch + WASM:

- One `.perch` file (plain text, reviewable)
- One `--no-shell --no-network --no-subprocess` invocation
- Pre-built `.wasm` modules (sha256-pinnable, language-agnostic)
- Audit via the existing `--audit FILE.ndjson` stream

[`docs/llm-control-plane.md`](llm-control-plane.md) is the deeper version of this argument.

---

## Walkthrough 4 — Polyglot data pipeline (Rust + Go in one perch file)

**The problem.** A data pipeline has three stages with different language preferences: a Rust extractor (existing crate handles weird XML edge cases well), a Go transformer (the team's main language), and a Python classifier (sklearn model). Normally these run as three separate subprocesses, each with their own runtime dependencies, glued together by bash.

**Why `wasm_run`.** Each stage compiles to a `.wasm`. The pipeline becomes three `wasm_run` blocks in a single `.perch`. No Python/Rust/Go toolchains needed at runtime — just the pre-built modules.

> **Honest caveat:** Python is hard to compile to WASI today. Mature options exist for *running* a Python interpreter under WASI (e.g. WASI-built CPython), but the binary is large (~30 MB). For "compile a Python script directly to WASM," tools like [py2wasm](https://github.com/wasmerio/py2wasm) work for AOT compilation of simple modules. Practical reality: stick to Rust/Go/Zig/AssemblyScript/C++ for v1 pipelines.

### The pipeline

```
input.xml  →  extract.wasm (Rust)  →  records.jsonl
                                  →
                                  →  transform.wasm (Go)     →  enriched.jsonl
                                                              →
                                                              →  classify.wasm (Go)  →  output.jsonl
```

Each stage reads from one mount, writes to another. perch wires the mounts.

### The `.perch` file

```capy
name "pipeline"

command process
    description "Run the full extract → transform → classify pipeline"
    arg input
        type string
        index 0
        description "Path to input.xml"
    end
    arg out
        type string
        index 1
        description "Path to output.jsonl"
    end
    do
        # Each stage writes its output to a tmpdir; the next stage
        # reads from it. perch's mount_write convention puts files
        # under /rw/<basename> inside the module.
        let work = mktemp_dir
        cp "${input}" "${work}/input.xml"

        timeout "30s"
            wasm_run "${script_dir}/wasm/extract.wasm"
                wasm_arg "/ro/work/input.xml"
                wasm_arg "/rw/work/records.jsonl"
                wasm_mount_read  "${work}"
                wasm_mount_write "${work}"
            end
        end

        timeout "30s"
            wasm_run "${script_dir}/wasm/transform.wasm"
                wasm_arg "/ro/work/records.jsonl"
                wasm_arg "/rw/work/enriched.jsonl"
                wasm_mount_read  "${work}"
                wasm_mount_write "${work}"
            end
        end

        timeout "60s"
            wasm_run "${script_dir}/wasm/classify.wasm"
                wasm_arg "/ro/work/enriched.jsonl"
                wasm_arg "/rw/work/output.jsonl"
                wasm_mount_read  "${work}"
                wasm_mount_write "${work}"
            end
        end

        cp "${work}/output.jsonl" "${out}"
        rm "${work}"
    end
end

command process_batch
    description "Process every input under ./batch — three at a time"
    do
        let inputs = glob "${script_dir}/batch/*.xml"
        let n = 0
        for_each "${inputs}" f
            parallel
                process "${f}" "${script_dir}/out/$(basename ${f} .xml).jsonl"
            end
        end
    end
end
```

### The composition value

Three stages, three languages, **zero runtime dependencies**. The recipient of this pipeline has perch installed and the three `.wasm` files. They run `perch process input.xml output.jsonl`. That's it.

Compare to the shell version:

```sh
# Without perch
rustc extract.rs && ./extract input.xml > /tmp/records.jsonl    # needs Rust
go run transform.go /tmp/records.jsonl > /tmp/enriched.jsonl    # needs Go
python classify.py /tmp/enriched.jsonl > output.jsonl           # needs Python + sklearn + …
```

vs

```sh
# With perch + wasm_run
perch process input.xml output.jsonl
```

### Going further — `perch --build` bundles the whole pipeline

```sh
$ perch --build -f commands.perch --include ./wasm -o pipeline-tool
$ scp pipeline-tool prod:/usr/local/bin/
$ ssh prod 'pipeline-tool process /tmp/in.xml /tmp/out.jsonl'
```

The `.wasm` modules are embedded inside the perch binary; recipient doesn't need anything beyond the single file. Real "ship the pipeline as a product" story.

---

## Walkthrough 5 — CI hot loop with content-hash caching

**The problem.** A code linter that runs across 5,000 files in CI on every PR. Currently uses a Python tool that takes ~6 minutes. We rewrote the linter's hot path in Go; compiled to WASM it's ~40x faster per invocation, but the per-file invocation overhead is non-trivial. We want sub-30s CI.

**Why `wasm_run` + `cache`.** WASM gives us the fast per-file execution. perch's `cache` block keyed by `(linter-version-hash, file-content-hash)` skips files that haven't changed since the last green CI run.

### The `.perch` file

```capy
name "lint"

LINTER = "${script_dir}/wasm/golint.wasm"

command lint_all
    description "Lint every file under ./src — cache results per (linter, file)"
    do
        let linter_hash = sha256_file "${LINTER}"
        let files = glob "${script_dir}/src/**/*.go"
        parallel
            for_each "${files}" f
                let file_hash = sha256_file "${f}"
                cache "lint-${linter_hash}-${file_hash}" "30d"
                    wasm_run "${LINTER}"
                        wasm_arg "/ro/src/${f}"
                        wasm_mount_read "${script_dir}/src"
                    end
                end
            end
        end
    end
end
```

### CI integration

```yaml
# .github/workflows/lint.yml
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: go install github.com/luowensheng/perch@latest

      # Restore the perch cache between CI runs.
      - uses: actions/cache@v4
        with:
          path: ~/.cache/perch/blocks
          key: perch-lint-${{ runner.os }}-${{ hashFiles('wasm/golint.wasm') }}
          restore-keys: perch-lint-${{ runner.os }}-

      - run: perch lint_all
```

### What this gets you

| Scenario | Cold cache | Warm cache (e.g. PR touches 12 files) |
|---|---|---|
| Wall-clock | ~28s | ~0.8s |
| What ran | 5,000 lints | 12 lints + 4,988 cache hits |

The `cache` block honestly says "this is user-keyed, not content-addressed" — but here the user-supplied key IS the content hash, so we get content-addressed behavior in practice. The key is `(linter-hash, file-hash)`; either changing invalidates the entry; otherwise the cache hits and skips the lint.

### Why this is hard to get with shell-only

You'd be reimplementing perch's cache layer in bash (`mkdir -p $CACHE; KEY=$(sha256 file); if [ -f $CACHE/$KEY ]; then …`) and you'd be doing it per-tool, per-CI-pipeline. The `cache` block makes it a one-line wrap.

---

## Common patterns reference

### Pattern: pass small data via argv

```capy
wasm_run "./tool.wasm"
    wasm_arg "--mode=fast"
    wasm_arg "--limit=100"
    wasm_arg "${input_id}"
end
```

Cheap, structured, easy to log via `--trace`.

### Pattern: pass a config file via mount

```capy
write_file "${cwd}/.config.json" "${json_config}"
wasm_run "./tool.wasm"
    wasm_arg "/ro/config/.config.json"
    wasm_mount_read "${cwd}"
end
rm "${cwd}/.config.json"
```

For structured config larger than argv can comfortably hold.

### Pattern: stdin from a pipe

```sh
$ cat input.txt | perch -f pipeline.perch process
```

```capy
command process
    do
        wasm_run "./reader.wasm"
            wasm_arg "from-stdin"
        end
    end
end
```

Inside the module: `bufio.NewReader(os.Stdin)` works. perch's stdin is wired through.

### Pattern: capture stdout into a variable

`wasm_run` itself returns no value, but you can use `shell_output` to capture another perch op's view of the module's stdout:

```capy
let result = perch -f pipeline.perch invoke ${input}
# Now ${result} contains the module's stdout
```

Or — cleaner — have the module write to a known path:

```capy
wasm_run "./extractor.wasm"
    wasm_arg "/rw/out/result.json"
    wasm_mount_write "${cwd}/out"
end
let result = read_file "${cwd}/out/result.json"
```

### Pattern: composition with retry

```capy
retry 3
    wasm_run "./flaky-validator.wasm"
        wasm_arg "${input}"
    end
end
```

Useful if the module reads from a flaky external resource you mounted. The retry uses exponential backoff.

### Pattern: composition with parallel

```capy
parallel
    wasm_run "./check-a.wasm"
        wasm_mount_read "./data"
    end
    wasm_run "./check-b.wasm"
        wasm_mount_read "./data"
    end
    wasm_run "./check-c.wasm"
        wasm_mount_read "./data"
    end
end
```

Three modules running concurrently. Their stdouts interleave naturally; their exit codes are aggregated (any failure → block fails with the first error).

### Pattern: bundling the WASM with `--build`

```sh
perch --build -f pipeline.perch --include ./wasm -o pipeline
scp pipeline prod:/usr/local/bin/
ssh prod 'pipeline run'
```

The `.wasm` files travel inside the binary. Recipients need only the binary; no perch install, no toolchain.

---

## Anti-patterns

These shapes work but defeat the purpose of using `wasm_run`:

### ❌ Compiling a thin wrapper that immediately shells out

```go
// BAD: wraps a system tool inside WASM. WASM can't shell out anyway —
// this just doesn't compile.
exec.Command("docker", "ps").Run()
```

If you want to run docker, use `shell "docker ps"`. `wasm_run` is for *your own* logic.

### ❌ Loading a huge module just to do one tiny thing

A Go-compiled WASM is ~2-3 MB minimum (the Go runtime). For a 10-line script, that's overkill — use `shell` or a perch op. Reserve `wasm_run` for code where the per-invocation safety / determinism / portability benefits dominate.

### ❌ Trying to make the module call back to perch

```go
// BAD: WASM has no network in v1. Can't call any HTTP endpoint.
http.Get("http://localhost:8080/perch-rpc")
```

If a module needs data perch has, mount it as a file. If a module needs to call another verb, write the result to a file mount; the *next* perch op picks it up.

### ❌ Pre-WASM-compiling everything as a religion

`shell "kubectl apply -f m.yaml"` is right. Compiling kubectl to WASM is not happening. Keep `shell` for orchestrating real tools.

---

## Troubleshooting

### "wasm_run …: module not found"

The first arg of `wasm_run` is a host filesystem path. Use `${script_dir}/…` to make it portable across cwds:

```capy
wasm_run "${script_dir}/tools/validator.wasm"
```

### "wasm_run: malformed module" / "invalid section"

The file isn't valid WASM. Common causes:

- Built for the wrong target: `GOOS=wasip1 GOARCH=wasm` is correct for Go 1.21+ with WASI Preview 1.
- Truncated download. `sha256_file` the artifact and compare to your build host.

### Module starts then exits immediately with no output

WASI's `_start` is the entry point. If your `main()` returns immediately, no output. If you used `os.Exit(N)`, wazero translates that to a non-zero exit. Check the module's exit code:

```capy
# perch returns a process-shaped error when the module exits non-zero.
# Trace to see what fired:
perch --trace -f file.perch yourCmd
```

### "open /ro/foo: file does not exist" inside the module

You forgot to mount the directory. The path inside the module is `/ro/<basename>` (read-only) or `/rw/<basename>` (read-write), not the host path.

```capy
# WRONG: passes the host path; the module can't see it
wasm_arg "${HOME}/data/input.csv"

# RIGHT: mount the dir + pass the in-module path
wasm_mount_read "${HOME}/data"
wasm_arg "/ro/data/input.csv"
```

### "env X not set" inside the module despite being set on the host

Env vars are deny-by-default. Add the name to `wasm_env`:

```capy
wasm_env "GREETING,API_TOKEN,HOME"
```

This is by design — see the [capability gating rationale](wasm.md#why-wasm_run-is-a-different-kind-of-safety).

### "wasm_run: module compile failed"

The module imports a function perch's WASI implementation doesn't provide. Most often: a module built for **WASI Preview 2** (not yet supported in v1). Recompile against Preview 1:

```sh
# Go
GOOS=wasip1 GOARCH=wasm go build -o tool.wasm .

# Rust
rustup target add wasm32-wasi
cargo build --target wasm32-wasi --release

# TinyGo
tinygo build -target=wasi -o tool.wasm .
```

`wasm32-wasip2` builds will not work in v1 — see the [roadmap](wasm.md#status-whats-in-the-v1-whats-coming).

### Module hangs or runs forever

Wrap the call in a `timeout` block:

```capy
timeout "10s"
    wasm_run "./potentially-slow.wasm"
        wasm_arg "${input}"
    end
end
```

The timeout context is wired into wazero — execution cancels at the wall-clock boundary.

### Performance: cold start is slow

The first `wasm_run` in a session pays the wazero compilation cost (~200-500ms for a 3 MB module). Subsequent calls within the same `perch` invocation reuse the compiled bytecode (in-process cache, sha256-keyed). For CI where each perch invocation is fresh, the on-disk cache is on the roadmap; for now the cold cost is one-time per command.

---

## What's NOT in v1 (workarounds)

The full roadmap lives in [`wasm.md`](wasm.md#status-whats-in-the-v1-whats-coming). Quick workarounds:

| Want | Today's workaround |
|---|---|
| **Network from inside the module** | Use `http_get` *outside* `wasm_run`; write result to a file; mount the file into the next `wasm_run`. |
| **Load module from URL** | `download URL ./tool.wasm` then `wasm_run "./tool.wasm"`. Use `sha256_file` to verify before invoking. |
| **Call a specific named export** | Today only `_start` runs. Have your module branch on argv[1] to dispatch internally. |
| **Configure mount path** | Mounts always land at `/ro/<basename>` or `/rw/<basename>`. Reorganize your host directory or use intermediate symlinks if you need a specific in-module path. |
| **Persistent on-disk module cache** | Re-compilation per fresh `perch` invocation is the cost. For CI, use `actions/cache@v4` over `~/.cache/perch/blocks` (the `cache` block's storage covers most of the benefit). |
| **WASI Preview 2 / Component Model** | Use Preview 1 modules. Most current toolchains target it by default. |

---

## See also

- [`docs/wasm.md`](wasm.md) — the canonical reference (every flag, every capability declaration, full spec)
- [`docs/execution-contexts.md`](execution-contexts.md) — `parallel` / `retry` / `timeout` / `cache` / `sandbox` block ops that compose with `wasm_run`
- [`docs/testing.md`](testing.md) — `perch test` for verifying your WASM workflows in CI
- [`docs/llm-control-plane.md`](llm-control-plane.md) — the agent-safety story in depth
- [`demos/wasm-hello/`](https://github.com/luowensheng/perch/tree/main/demos/wasm-hello) — minimal end-to-end demo (Go source + pre-built `.wasm` + commands.perch)
- [`ideas/12-wasm-execution.md`](https://github.com/luowensheng/perch/blob/main/ideas/12-wasm-execution.md) — design rationale + what's intentionally NOT in v1

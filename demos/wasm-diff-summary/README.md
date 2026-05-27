# `wasm-diff-summary` — agent-safe PR analysis

The strongest demo for perch + WASM + AI agents. The module takes a
unified diff and returns structured JSON: per-file ± counts,
extension breakdown, risk heuristics with reasons. **Designed as a
tool an LLM agent can call safely** — no shell, no network, no host
filesystem access, deterministic output.

## Output

```json
{
  "files_changed": 3,
  "added_lines": 6,
  "removed_lines": 1,
  "by_extension": {
    ".go": 1,
    ".md": 1,
    ".sql": 1
  },
  "files": [
    {"path": "README.md",                   "added": 2, "removed": 0},
    {"path": "auth/handler.go",             "added": 1, "removed": 1},
    {"path": "migrations/0042_add_index.sql","added": 3, "removed": 0}
  ],
  "risk": "high",
  "risk_reasons": [
    "touches auth",
    "modifies schema",
    "modifies migrations"
  ]
}
```

Same input → same output, every time, on every machine.

## Try it

```sh
$ perch -f commands.perch demo
# … prints the JSON above

$ git diff origin/main...HEAD | perch -f commands.perch --trust-stdin summarize_stdin
# … prints summary of YOUR PR's diff
```

## Why this is the killer agent demo

An LLM agent that can analyze pull requests is hugely valuable —
summarizing diffs, flagging risky changes, drafting review comments.
But the moment you let it execute arbitrary `git diff` + analyze
the result via shell, you've opened the door to prompt injection
turning into a shell-execution exploit.

`wasm-diff-summary` is the **safe lane**:

| The agent can: | The agent CANNOT: |
|---|---|
| Call the `summarize_diff` MCP verb | Shell out — no syscalls available |
| Pass typed args (a file path or stdin) | Read anywhere except the mounted diff dir |
| Receive structured JSON | Make network requests — no socket imports |
| Be re-prompted with the summary | Spawn subprocesses — no `fork`/`exec` |

The `.wasm` module is sha256-pinnable. Reviewers can sign off on
"this is the diff analyzer we approved" once; auditors get a stable
artifact identity forever.

## Wire to an MCP server

```sh
$ perch-mcp \
    --no-shell --no-network --no-subprocess --env "" \
    --max-runtime 30 \
    -f commands.perch
```

The agent then sees one verb:

```
perch_run name="summarize_file" path="some.diff"
```

Or, if you write a streaming variant that takes diff text as an arg:

```capy
command summarize_text
    arg diff_text
        type string
    end
    do
        write_file "${cwd}/_in.diff" "${diff_text}"
        wasm_run "${script_dir}/diff-summary.wasm"
            wasm_arg "/ro/in/_in.diff"
            wasm_mount_read "${cwd}"
        end
        rm "${cwd}/_in.diff"
    end
end
```

The agent passes raw diff text; perch writes it to a tempfile; the
module reads it via the mount. **No shell escape possible anywhere
in this chain** — the diff text is data passed to a WASI mount-read,
not to shell.

## Risk heuristics (transparent, easy to tune)

`riskTouchpoints` in `main.go` is the configurable list. Default
patterns:

| Pattern | Reason |
|---|---|
| `auth` | touches auth |
| `crypt` | touches crypto |
| `secret` | touches secrets |
| `sandbox` | touches sandbox |
| `.sql` | modifies schema |
| `migration` | modifies migrations |
| `main.go` | touches entry point |
| `.tf` | modifies infrastructure |

Plus a size-based heuristic: 500+ ± lines → escalates to medium;
1000+ → high.

Final risk:
- **high** — 2+ touchpoints OR 1000+ ± lines
- **medium** — 1 touchpoint OR 200+ ± lines
- **low** — neither

Trivially extensible — add to `riskTouchpoints`, rebuild.

## Bundle into a single binary

```sh
$ perch --build -f commands.perch --include . -o diff-tool

# Recipients run one binary; no perch install, no Go toolchain
$ scp diff-tool ci-host:/usr/local/bin/
$ ssh ci-host 'git diff main...HEAD | diff-tool summarize_stdin'
```

## What the WASM module sees

```
argv:     ["diff-summary.wasm", "/ro/diffs/sample.diff"]
env:      none
fs read:  /ro/diffs → host's ./diffs directory only
fs write: none — module cannot write anywhere
network:  none
shell:    none
```

Worst-case (malicious .wasm slipped past code review): the module
can only read what it was given and write to stdout. There's no
syscall surface to escape through.

## Commands available

| Verb | What |
|---|---|
| `summarize_file PATH` | Summarize a diff file (under ./diffs) |
| `summarize_stdin` | Read a diff from stdin → JSON to stdout |
| `summarize_pr` | `git diff origin/main…HEAD` piped into the summarizer |
| `demo` | Summarize the bundled sample.diff |
| `rebuild` | Recompile diff-summary.wasm |

## See also

- [`docs/wasm.md`](../../docs/wasm.md) — `wasm_run` reference
- [`docs/wasm-walkthroughs.md`](../../docs/wasm-walkthroughs.md) — Walkthrough 3 covers this MCP integration in depth
- [`docs/llm-control-plane.md`](../../docs/llm-control-plane.md) — the broader agent-safety story
- [`demos/wasm-schema-validator/`](../wasm-schema-validator/) — sibling demo for JSON validation

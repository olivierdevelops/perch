# perch — Public Release Plan

> Goal: ship perch v0.1.0 on GitHub with enough polish that someone discovering it from a tweet, Hacker News post, or Reddit thread can — in under 5 minutes — understand what it is, install it, and run a real example.
>
> The bar: **someone clones the repo, runs one command, sees something impressive.** Everything else exists to support that moment.

---

## Success criteria

- `curl -fsSL <install>.sh | sh` works on macOS + Linux.
- A user with no prior context can read README.md and produce their first runnable `commands.perch` within 3 minutes.
- `./demos/<any>` is interesting on its own merits — not just "hello world × 12."
- CI is green; pre-built binaries are attached to the GitHub release for darwin/linux/windows × arm64/amd64.
- `perch --help` is enough to use 80% of the tool.
- At least one demo demonstrates the killer feature (`--build` producing a portable, self-contained binary).
- The README's first 30 lines could be a tweet.

---

## Tier 0 — Launch blockers

Cannot release without these. Everything here is mandatory for v0.1.0.

### 0.1 — README.md

**Goal:** the front page that closes deals.

```
[logo: capybara with tools perched on its back]

# perch
> a cross-platform command runner that compiles to a single binary

[badges: build, release, license, go reference]

## 60-second demo
[animated terminal GIF or a 12-line code block + output]

## Install
brew / curl-pipe / go install / binary download — all four lines.

## Why perch
- single config, three frontends (CLI / web UI / REPL)
- one --build away from a portable binary you can ship to users
- built-in cross-platform OS functions — no per-OS bash branches
- powered by [capy](https://github.com/luowensheng/capy)

## Three-line tour
[commands.perch with build/test/serve + matching `perch <cmd>` lines]

## Documentation
[reading order: getting-started → language → ops → embedding → faq]

## Status
Pre-1.0, surface stable but adding ops. SemVer kicks in at v1.0.

## License
[Apache-2.0 or MIT — see Tier 0.4]
```

**Files:** `README.md`

### 0.2 — Demos folder (at least 5)

Each demo is a self-contained subdir with `commands.perch` + `README.md` + (optionally) a screenshot/recording. Each must be runnable with one command.

Initial demos to ship:

| Folder | What it shows | One-liner to run |
|---|---|---|
| `demos/01-hello/` | Globals, args, `let`, `print`. The 30-second "wow this is simple" intro. | `perch hello -name=World` |
| `demos/02-cross-platform-setup/` | `if_os` branches that install dev dependencies via brew/apt/choco. Highlights the cross-platform thesis. | `perch setup` |
| `demos/03-go-project/` | Build, test, lint, release a Go project. The "this could replace your Makefile" demo. | `perch build -target=linux` |
| `demos/04-portable-cli/` | A tiny tool defined entirely in commands.perch, bundled into a single binary via `perch --build`. The killer-feature demo. | `perch --build -o greet && ./greet hello -name=Alice` |
| `demos/05-web-ui/` | A `commands.perch` whose `--server` mode gives non-engineers a friendly UI to run jobs (e.g. backup, deploy, rotate logs). | `perch --server` then browse |
| `demos/06-ci-runner/` | The same `commands.perch` used by both devs locally AND CI (paste into GitHub Actions). Shows that one file replaces both Makefile + workflow yaml. | `perch ci` |
| `demos/07-pipeline/` | Real-ish data pipeline: download → ungzip → transform → upload. Shows `let` chaining and the HTTP/compression op catalog. | `perch run` |

**Files:** `demos/*/{commands.perch,README.md}` + a top-level `demos/README.md` indexing them.

### 0.3 — Tests

Three layers:

**Unit tests** (per Go package):

```
domain/program_test.go              -- Op.IsBlock, marshaling round-trip
infra/capyloader/loader_test.go     -- parsing each event kind; nested if_os; catch; globals
infra/interpreter/interpolate_test.go -- ${x} substitution, escape, errors
infra/interpreter/bindings_test.go   -- ToStringValue for every value kind
infra/interpreter/interpreter_test.go -- arg parsing, defaults, required, ProxyArgs
infra/ops/*_test.go                 -- each handler with a tiny fixture
infra/embed/embed_test.go           -- write + read round-trip + idempotent rebuild
io/cli/cli_test.go                  -- argv → dispatch enum
```

**Integration tests** (`tests/integration/` with table-driven fixtures):

- For each demo, a `.expected` file recording the stdout/stderr/exit code. CI runs `perch <cmd>` and diffs.
- A `golden/` folder with a dozen `.perch` files paired with `.json` snapshots of the parsed `Program`. Catches regressions in lib.capy.
- A separate `tests/e2e_build_test.go` that builds a binary, runs it, asserts the embedded program is served.

**Smoke test in CI**: `go test ./... && go run ./.ignore/sample-runner.go` (a runner that exercises every demo headlessly).

**Files:** `*_test.go` in each package; `tests/integration/...`; `tests/golden/...`.

### 0.4 — LICENSE + minimal governance

- `LICENSE` — pick MIT or Apache-2.0. **Recommendation: MIT** (simplest, broadest compatibility for a tool that produces binaries others ship).
- `CONTRIBUTING.md` — even a 30-line "open an issue first; tests with PRs; one-feature-per-PR" is enough for v0.1.0.
- `SECURITY.md` — one-paragraph disclosure email + the supported-versions table.
- `CODE_OF_CONDUCT.md` — adopt Contributor Covenant 2.1 verbatim.

**Files:** `LICENSE`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`.

### 0.5 — Pre-built release binaries via GitHub Actions

`.github/workflows/release.yml`:

- Triggers on `v*` tag push.
- Matrix: `{darwin, linux, windows} × {amd64, arm64}` → 6 binaries.
- `go build -ldflags '-s -w' -trimpath -o perch-${OS}-${ARCH}${EXT}`.
- Uploads to the GitHub release.
- Computes sha256 for each; attaches `checksums.txt`.

`.github/workflows/ci.yml`:

- Runs on every push and PR.
- `go vet ./... && go test ./...` against the matrix.
- Builds the demos folder headlessly.

**Files:** `.github/workflows/{ci.yml,release.yml}`.

### 0.6 — Versioning + CHANGELOG

- `version.go` (or just a `Version` const in main) bumped on each tag.
- `CHANGELOG.md` in Keep-a-Changelog format. Start with the v0.1.0 entry.
- Git tags: `v0.1.0`, `v0.1.1`, … SemVer with the understanding that 0.x can break.

**Files:** `CHANGELOG.md`.

### 0.7 — `--help` quality pass

Today `--help` lists commands but is terse. Tier-0 sweep:

- Top-level `perch --help` → manpage-style sections: USAGE, COMMANDS, OPTIONS, EXAMPLES.
- `perch <cmd> --help` → per-command help: description, arg table with defaults, env table, "Defined at line N of …".
- Suggestion fuzzy match: typo'd command → "Did you mean `build`?"
- Color when stdout is a TTY; plain text otherwise.

**Files:** updates to `io/cli/cli.go`, possibly a new `infra/cliformat/`.

---

## Tier 1 — Launch quality

Should be done before announcing publicly. Stuff that turns "interesting" into "credible."

### 1.1 — Documentation site

`docs/` folder, served by `mkdocs-material` (matches capy's approach) and published to GitHub Pages via a workflow.

Reading order — six pages, each under 1,000 words:

1. **`docs/getting-started.md`** — install → `--init` → first command → run.
2. **`docs/language.md`** — every keyword and modifier in §2 of the original plan: command/do/end, globals, args/arg_default/etc., config modifiers, catch, the `${x}` interpolation rules. A cheat sheet at the bottom.
3. **`docs/op-reference.md`** — auto-generated from `infra/ops/` registrations: kind, signature, return value, example. The "stdlib" reference.
4. **`docs/embedding.md`** — `--build` and how the fat binary works. When to use it. The on-disk format spec (so people can audit security).
5. **`docs/ci-recipes.md`** — drop-in GitHub Actions / GitLab CI snippets calling `perch <cmd>`.
6. **`docs/faq.md`** — vs Make / Just / Task; "can I share commands across projects?"; "why capy?"; "how do I test my commands.perch?"

Plus `docs/architecture.md` for contributors — points at `vhco-architecture.md`.

**Files:** `docs/*.md`, `mkdocs.yml`, `.github/workflows/docs.yml`.

### 1.2 — Tutorial path

`docs/tutorials/` — three guided projects, each 10–15 minutes, building on each other:

1. **`01-replace-your-makefile.md`** — take a real (small) Go project's Makefile, convert it. Side-by-side before/after. End by `perch test` working.
2. **`02-ship-a-tool.md`** — write a tool entirely in commands.perch. Build it. Distribute the binary. Recipient runs it on a fresh machine.
3. **`03-cross-platform-installer.md`** — write one commands.perch that installs deps on darwin/linux/windows. Show the same source running on all three.

Each tutorial ends with a "what you learned" recap and a link to the next one.

**Files:** `docs/tutorials/*.md`.

### 1.3 — Installer scripts + Homebrew tap

- `scripts/install.sh` — detects OS/arch, downloads from latest release, verifies sha256, installs to `/usr/local/bin` (with sudo only if needed) or `~/.local/bin`. The `curl -fsSL <url> | sh` story.
- `scripts/install.ps1` — Windows equivalent.
- `Formula/perch.rb` in a `homebrew-tap` sibling repo (or `Homebrew/Formula/perch.rb` if going for homebrew/core later).
- Document `go install github.com/<you>/perch/cmd/perch@latest` in README.

**Files:** `scripts/install.{sh,ps1}`; separate `homebrew-perch` repo with the Formula.

### 1.4 — Error messages + diagnostics

Today's errors are mostly raw Go errors. Sweep to:

- Wrap every error path through `loader.go` with line number + file path + a hint ("expected `do` here").
- When an op fails, print the op kind + interpolated args + line in the source.
- Add `perch --debug` flag that dumps the parsed `Program` JSON before running (useful for our own debugging and user bug reports).
- Add structured error types so the `--server` UI can render them nicely.

**Files:** `infra/capyloader/errors.go`, `infra/interpreter/errors.go`, updates throughout.

### 1.5 — Brand assets

- **Logo:** capybara with a stack of tools/files perched on its back, leaning into the "perch" name. SVG + PNG @ 256/512/1024. Commission from Fiverr or generate via DALL-E and refine.
- **Social card:** 1200×630 PNG for OpenGraph/Twitter, embedded in README and docs site.
- **Favicon** for the docs site.
- **Demo GIFs:** record 2-3 terminal sessions with `vhs` or `asciinema → agg`. Pin to the top of the README.

**Files:** `assets/logo.{svg,png}`, `assets/social.png`, `assets/demo-*.gif`.

### 1.6 — Issue and PR templates

- `.github/ISSUE_TEMPLATE/bug.yml` — repro steps, perch version, OS, expected vs actual.
- `.github/ISSUE_TEMPLATE/feature.yml` — what + why + alternatives considered.
- `.github/ISSUE_TEMPLATE/op-request.yml` — name, signature, when you'd use it.
- `.github/PULL_REQUEST_TEMPLATE.md` — "what changed, what tests, screenshot if UI."
- `.github/DISCUSSION_TEMPLATE/show-and-tell.yml` — encourage users to share their commands.perch.

**Files:** `.github/ISSUE_TEMPLATE/*`, `.github/PULL_REQUEST_TEMPLATE.md`, `.github/DISCUSSION_TEMPLATE/*`.

---

## Tier 2 — Polish

Elevates a credible launch into a memorable one. Do these before public-facing announcement on HN/Reddit/Twitter.

### 2.1 — Claude Code skill

Drop `skills/perch/SKILL.md` modelled on the existing capy MCP/skill setup. When invoked, the skill:

- Knows the perch grammar (gives correct syntax, doesn't hallucinate ops)
- Has examples of common patterns (cross-platform, build pipelines)
- Includes the full op catalog so model can suggest the right op
- Has a `/perch-new <project description>` flow that generates a starter commands.perch

The skill is the single biggest thing that makes perch viral with AI-tool users — they can ask Claude "set up commands.perch for my Go project" and it works.

**Files:** `skills/perch/SKILL.md`, `skills/perch/scripts/*`.

### 2.2 — Optional MCP server

Build a tiny MCP server (`cmd/perch-mcp/`) so Claude Desktop / Cursor / Zed users can:

- Discover the commands in a project's commands.perch
- Run them via MCP tool calls
- Stream the output back to the chat

Pattern: copy `capy-mcp`'s skeleton. Reuse our HTTP server's NDJSON streaming for the MCP transport.

**Files:** `cmd/perch-mcp/main.go`, documented in `docs/mcp.md`.

### 2.3 — Editor support

- **VS Code extension** (`editors/vscode/perch/`): syntax highlighting for `*.perch` files. Recognises perch keywords (command, do, end, if_os, let, …) and the op catalog. Publish to the VS Code Marketplace.
- **Tree-sitter grammar** (`editors/tree-sitter-perch/`): the source of truth that powers VS Code, Neovim, and Helix.
- **Vim plugin** (`editors/vim/perch.vim`): same syntax file translated. Optional.

**Files:** as above. Publishing the VS Code extension is one-time effort + a CI job to release on tag.

### 2.4 — Shell completions

- `scripts/completions/perch.bash`, `.zsh`, `.fish` — generated from the command list in `cli.go`.
- Plus dynamic per-command completions (read commands.perch on tab, suggest command names + args).
- `perch --completions <shell>` prints the completion script to stdout (drop-in for `eval "$(perch --completions zsh)"`).

**Files:** `scripts/completions/*`, plus a hook in `io/cli/cli.go`.

### 2.5 — Manpages

Auto-generate from the command tree via `cmd/perch-gendoc/`. Ship `perch.1` in releases. Optional for v0.1.0, table-stakes for v1.

**Files:** `cmd/perch-gendoc/main.go`, `man/perch.1`.

### 2.6 — Playground (web)

Compile perch + its op handlers to WebAssembly and host a playground at `<your>.github.io/perch/playground/`. Six curated sample DSLs in a dropdown. Lets people *try* without installing.

Inspired by capy's playground; the architecture transfers — replace text rendering with op-trace rendering ("this op would run X, then Y…").

**Note:** the playground can't actually run `shell` ops in the browser, but it CAN parse the source and show the interpreted op tree, which is 80% of the educational value.

**Files:** `cmd/perch-wasm/`, `playground/index.html`, GitHub Pages workflow.

---

## Tier 3 — Post-launch backlog

Don't block v0.1.0 on these.

- **`perch fmt`** — opinionated formatter for commands.perch. Trees nicely, aligns arg columns.
- **`perch lint`** — surface unused args, dead commands, OS-specific shell calls outside if_os, …
- **`perch test`** — first-class testing of commands.perch (assertions on op output, env, side effects).
- **Cross-compile in `--build`** — embed per-target perch stubs so `perch --build -target=linux-amd64` works from macOS.
- **`perch share`** — upload a commands.perch + program JSON to a registry; `perch install <name>` to pull. (Like cargo-binstall but for perch programs.)
- **Templates** — `perch new go-cli`, `perch new node-monorepo`. Scaffolds a starter commands.perch from a template registry.
- **Watch mode** — `perch --watch <cmd>` reruns the command on file changes (like nodemon).
- **Op grouping/imports** — `import ./shared.perch` so multi-project setups can share commands.
- **TypeScript / Python op SDKs** — let users write op handlers in languages other than Go, expose them via plugin protocol.
- **Translations** of docs/README into Spanish, Japanese, Mandarin.
- **Migrate-from-make tool** — semi-automated Makefile → commands.perch converter.
- **Lockfile** — pin op catalog versions for reproducibility across perch upgrades.

---

## Workstreams (parallel-friendly)

If multiple people pitch in, here's how to slice it without conflicts:

| Workstream | Owns | Tier-0 + Tier-1 items |
|---|---|---|
| **Code** | `domain/`, `infra/`, `usecases/`, `io/`, `orchestrator/` | tests, error messages, --help quality, ci.yml |
| **DSL** | `infra/capyloader/lib.capy`, `infra/ops/*` | op catalog reference, golden tests, expanding the catalog |
| **Docs** | `docs/*` | getting-started, language, op-ref, embedding, FAQ, tutorials |
| **Demos** | `demos/*` | the seven demos; recording demo GIFs |
| **Brand** | `assets/*`, `README.md` top | logo, social card, README front matter |
| **Distribution** | `scripts/*`, `.github/workflows/*`, Homebrew tap | install scripts, release.yml, tap formula |
| **AI integration** | `skills/*`, `cmd/perch-mcp/` | Claude Code skill, MCP server |
| **Editor** | `editors/*` | tree-sitter, VS Code extension |

Most workstreams can advance in isolation; the only shared bottleneck is `lib.capy` (DSL workstream) which everything else builds on.

---

## Pre-launch checklist

The day before flipping the repo public:

- [ ] All Tier-0 items complete.
- [ ] CI green on main; release workflow tested via `v0.1.0-rc1` tag.
- [ ] Demos all run on a clean clone (`git clone && cd perch && ./demos/04-portable-cli/run.sh`).
- [ ] README renders correctly on github.com (check anchor links, image paths).
- [ ] LICENSE file is present and the year is correct.
- [ ] `go vet ./...` and `staticcheck ./...` are clean.
- [ ] Run `perch --build` on the project's own `commands.perch`; ship the resulting binary as part of the release.
- [ ] Spell-check pass over README and every doc page.
- [ ] Search-replace any TODOs, FIXMEs, "tk", "lorem" in shipped docs.
- [ ] Logo and social card final.
- [ ] Docs site builds and is reachable at the GitHub Pages URL.
- [ ] Discussions enabled on the repo with a pinned "Welcome / Show & Tell" thread.
- [ ] At least one external collaborator has reviewed README and run quickstart.

## Day-of-launch checklist

The day you announce:

- [ ] Flip repo from private to public (or push initial commit to public repo).
- [ ] Cut and publish the GitHub release (`v0.1.0`) — title, release notes, attached binaries with checksums.
- [ ] Add the perch repo to your GitHub profile pinned items.
- [ ] Submit to:
  - [ ] Hacker News (Show HN: perch — …)
  - [ ] r/golang
  - [ ] r/programming
  - [ ] Lobsters
  - [ ] Tweet / toot a 30-second screen recording
- [ ] Open a "Show and Tell" discussion seeded with the demos folder.
- [ ] Respond to the first 24 hours of issues / PRs / comments in real time.
- [ ] Don't announce on Monday or Friday. Tuesday–Thursday morning US-East tends to perform best.

---

## Open questions to resolve before Tier-0 work begins

These are decisions that, once made, unblock multiple workstreams. Suggest deciding them up front:

1. **License: MIT vs Apache-2.0?** MIT recommended; simpler and friendlier for a tool that produces shippable binaries.
2. **GitHub org or personal account?** A `perch-lang` (or similar) org gives room for `perch-mcp`, `homebrew-perch`, `perch-vscode`, `tree-sitter-perch` siblings.
3. **Brand asset commission source?** Fiverr ($30–50), an illustrator friend, or AI-generated + refined in Figma.
4. **First demo to record as the README hero GIF?** Recommend `demos/04-portable-cli/` — the "look, one command turns this into a binary" moment is the most viral.
5. **Are you open to upstreaming changes back into capy?** Some perch improvements (e.g. `top_level` / `depth` locals if those would help us) would benefit capy. Easier to coordinate if you OK with that ahead of time.

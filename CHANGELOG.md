# Changelog

All notable changes to perch are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows [SemVer](https://semver.org/) once it reaches v1.0.

## [Unreleased]

### Added

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

# Changelog

All notable changes to perch are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows [SemVer](https://semver.org/) once it reaches v1.0.

## [Unreleased]

### Added

- **`perch --mode NAME`** — opinionated security presets that disable groups of ops globally. Phase-0 of the capability sandbox design at [sandbox.md](docs/sandbox.md).
  - **`safe`** — disables `shell`, `shell_output`, `shell_detached`, `shell_in`, `try_shell`, `pkg_install`, `pkg_uninstall`, `kill_by_name`, `process_running`. No subprocess access.
  - **`offline`** — disables every network-touching op (`http_*`, `download`, `dns_lookup`, `port_*`, `wait_for_*`, `public_ip`, `local_ip`, `mac_address`, `interfaces`).
  - **`read-only`** — disables every filesystem-mutation op (`write_file`, `append_*`, `cp`, `mv`, `rm`, `mkdir`, `chmod`, `touch`, `copy_dir`, `symlink`, `tar_create/extract`, `gzip`/`ungzip`, `zip_create/extract`, `bundle_extract`, `bundle_dir`, …).
  - **`pure`** — union of all three. The strictest preset.
  - **`trusted`** (default; empty string) — full op catalog.
  - **`perch --modes`** lists every mode with the exact ops it blocks.
  - Blocked calls return `op "X" is disabled by --mode NAME` rather than a confusing "unknown op" — precise feedback for both humans and AI agents.
  - Applies to every surface uniformly: CLI, `--server`, `--shell`, embedded binaries.
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

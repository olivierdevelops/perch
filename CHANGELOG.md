# Changelog

All notable changes to perch are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows [SemVer](https://semver.org/) once it reaches v1.0.

## [Unreleased]

### Added

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

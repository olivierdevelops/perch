# perch as an OS-in-a-program

> **The goal:** get as close to an operating system as possible while still being just one program — maximum control over what runs, who can run it, with what resources, observed how, undoable when. No kernel work. No privileged install. No daemon.

This page is the manifesto. It maps the parts of an operating system to the parts of perch — which ones already ship, which ones are designed, and which ones are not in scope. If you want a reference for what perch *is*, read the [language guide](language.md). If you want to know what it's *for*, this is the page.

---

## The 10 things an OS gives a program

Pick any operating system. Strip away the hardware story and what's left is a small set of abstractions: a way to run things, a way to scope what they touch, a way to watch them, a way to bound them. perch implements those abstractions inside a single Go binary that you can `scp` to a server, embed inside another binary, or hand to an AI agent.

| # | OS concept | perch equivalent | Status |
|---|---|---|---|
| 1 | **System calls** (the API surface) | ~140 first-class ops (`shell`, `cp`, `http_get`, `tar_create`, `pkg_install`, …) | shipped |
| 2 | **Process model** (fork/exec, lifecycle) | `shell`, `shell_detached`, `run TARGET`, `on_signal HANDLER`, `kill_by_name` | shipped |
| 3 | **Capability system** (which calls a process may make) | `--no-shell`, `--no-subprocess`, `--no-network`, `--no-write`, `--allow-bin`, `--no-shell-metachars` | shipped |
| 4 | **Identity / environment** (whose env vars can be read) | `--env A,B,C` (with automatic subprocess scrubbing) | shipped |
| 5 | **Resource limits** (CPU, memory, wall clock) | `--max-runtime SECS` (more designed in [sandbox §3](sandbox.md#3-the-sandbox-block-grammar)) | **shipped (wall clock)**, rest designed |
| 6 | **Audit log** (what did the process do?) | `--audit FILE.ndjson` — one line per op, with args, duration, error | **shipped** |
| 7 | **Standard library** (CLI tools you can call) | the ~140 ops cover string / hashing / encoding / HTTP / archive / fs / regex / time / network / system | shipped |
| 8 | **Package manager** (install other software) | `pkg_install` + `detect_pkg_mgr` (brew / apt / dnf / pacman / apk / zypper / winget / choco / scoop) | shipped |
| 9 | **Configuration / state** (where things land on disk) | auto-bound `${config_dir}`, `${cache_dir}`, `${data_dir}`, `${home}`, `${temp_dir}`; `bundle_extract` for content-addressable install dirs | shipped |
| 10 | **Multiple frontends** (CLI / GUI / API) | same `.perch` file becomes a CLI, a web UI (`--server`), a REPL (`--shell`), an MCP tool surface (`perch-mcp`), and a portable binary (`--build`) | shipped |

That's the OS in 10 rows. Everything below details how each row works and what the limits are.

---

## 1. System calls — the op catalog as ABI

In Linux, the kernel's syscall table is the contract: `read()`, `write()`, `open()`, `socket()`, … ~330 syscalls. Above that, libc and `coreutils` build the userspace experience.

In perch, the **op handlers** are the contract: ~140 Go functions registered into a `map[string]Handler`. Above them, the capy DSL (`name`, `command`, `do`, `if`, `let`, `for_each`) is the userspace shape. Below them, Go's standard library does the syscalls.

This matters because:

- **The op set is *exactly* what a `.perch` file can do.** No FFI, no `eval`, no plugin escape. To extend the ABI you add a handler in Go and recompile. (User-defined ops via WASM modules are on the roadmap; see [sandbox §9](sandbox.md#9-wasm-why-not-the-primary-lever).)
- **Restricting the ABI is one map mutation.** `ops.ApplyRestrictions(handlers, r)` replaces blocked ops with deny-handlers. That's the whole capability mechanism.
- **The ABI is stable across surfaces.** A command run from the CLI, from the web UI, from the REPL, from MCP — all go through the same dispatch path. Audit one path, audit all of them.

The full op list is in [op-reference.md](op-reference.md).

---

## 2. Process model — `command`, `shell`, signals

A perch `command` is the unit of "something a user invokes." Inside `do … end` you compose ops; the ops include all the subprocess primitives:

- **`shell CMD`** — fork-execs a shell. The closest perch gets to a syscall, also its biggest attack surface (see §3).
- **`shell_detached CMD`** — fire-and-forget.
- **`shell_output CMD`** — capture stdout.
- **`run TARGET`** — call another command (including `private` ones not visible on the CLI).
- **`on_signal HANDLER`** — a per-command modifier; when SIGINT/SIGTERM arrives, the named command runs as cleanup. The init-system-style "trap and clean up" pattern, declarative.
- **`for_each VALUE NAME … end`** — iterate, like Make's pattern rules.

Combined with `proxy_args` (forward the full invocation as one string) and the `rest` arg modifier (typed variadic), this covers every shape a CLI tool needs to spawn or supervise children.

---

## 3. Capability system — `--no-*` and `--allow-bin`

Posix gives you `setuid`, `setgid`, capabilities (cap_net_bind_service, …), seccomp. macOS adds entitlements + sandbox-exec. Windows has integrity levels + AppContainer. The unifying idea: **what a process can ask the kernel for is policy, not code.**

perch's version is the same idea, scoped to the op catalog:

| Restriction | What disappears |
|---|---|
| `--no-shell` | `shell`, `shell_output`, `shell_detached`, `shell_in`, `try_shell` |
| `--no-subprocess` | `pkg_install`, `pkg_uninstall`, `kill_by_name`, `process_running`, `bin_version`, `os_version` |
| `--no-network` | every `http_*`, `download`, `dns_lookup`, `port_*`, `wait_for_*`, `public_ip`, `local_ip`, `mac_address`, `interfaces` |
| `--no-write` | every FS-mutation op (write_file, append_*, cp, mv, rm, mkdir, chmod, touch, copy_dir, archive create/extract, symlink, …) |
| `--allow-bin git,docker` | shell still works, but only with the listed binaries (basename-matched) |
| `--no-shell-metachars` | shell still works, but no `|`, `>`, `<`, `&`, `;`, `` ` ``, `$(` |

These compose freely. The "🔒 security:" banner names every active restriction so reviewers see the posture without having to dig:

```
🔒 security: --no-shell --no-network  --env HOME,PATH  --allow-bin git,docker
```

Full design + the upcoming author-declared sandbox block (`sandbox … end` inside a `.perch` file) is in [sandbox.md](sandbox.md).

---

## 4. Identity / environment — `--env`

Process inherits the parent's env unless explicitly scrubbed. Most programs leak everything to every subprocess.

In perch:

- `${NAME}` interpolation falls through to `os.LookupEnv` by default — convenient, every host var visible.
- `--env A,B,C` restricts the fallthrough to declared names. `${SECRET_KEY}` outside the allowlist becomes a runtime error: `env var ${SECRET_KEY} is not in --env allowlist (declare with --env SECRET_KEY)`.
- **Bonus: subprocesses inherit only the allowlist too.** `shell "echo $SECRET"` returns empty for any `$SECRET` not on the list. This closes the subprocess escape hatch for env exfiltration.

Auto-bound names (`${home}`, `${cache_dir}`, `${exe_path}`, `${is_macos}`, …) are *not* env vars — they're host facts perch maintains internally — so `--env` doesn't touch them. Globally declared `UPPERCASE` perch globals still propagate to subprocesses because the file's author chose to expose them.

---

## 5. Resource limits — `--max-runtime`

Linux gives you `ulimit` + `cgroups` + `setrlimit`. perch ships `--max-runtime SECS` today — a wall-clock cap on the whole invocation. The deadline is checked before each op dispatch, so:

- A 60-second budget with a `shell "sleep 120"` in the middle finishes the shell (Go's `exec.Cmd` doesn't respect deadlines unless you wire context cancellation), but the next op refuses to fire and the process exits with `↪ stopped: --max-runtime exceeded`.
- Tight loops of cheap ops get caught right away.

`--max-download BYTES`, `--max-file-size BYTES`, `--max-processes N` are designed (see [sandbox §4.6](sandbox.md#46-resource-limits)) and not yet implemented.

---

## 6. Audit log — `--audit FILE.ndjson`

Linux has `auditd`. Solaris has DTrace. macOS has `log show`. Windows has ETW. They all serve the same function: **a structured trace of what the process actually did.**

perch's version is `--audit PATH` — one line of NDJSON per op call, plus session-start and session-end records. Each line carries timestamp, command name, op kind, the interpolated args (so the agent's actual input is recorded), duration, and the error (or empty):

```json
{"event":"session_start","ts":"2024-…","cmd":"deploy","cli_args":["-target=prod"]}
{"event":"op","ts":"…","cmd":"deploy","kind":"shell","args":{"_0":"docker compose up -d"},"dur_ms":1842,"ok":true}
{"event":"op","ts":"…","cmd":"deploy","kind":"write_file","args":{"path":"/etc/x","content":"…"},"dur_ms":3,"ok":false,"error":"op \"write_file\" is disabled by --no-write"}
{"event":"session_end","ts":"…","cmd":"deploy","dur_ms":2104,"ok":false,"error":"…"}
```

`-` means stdout (for piping into other tools); a path appends so multiple invocations accumulate. Pair it with the security flags and you have a full forensic record of what an agent (or a user, or CI) actually ran:

```sh
perch-mcp --no-shell --no-network --env KUBECONFIG --audit /var/log/perch-agent.ndjson -f ops.perch
```

The audit stream is *the entire interesting thing* in compliance / supervision setups. Same shape across CLI / web / shell / MCP / built binary — same dispatch path, same recorder.

---

## 7. Standard library — the cross-platform layer

A Linux box ships with `coreutils`, `grep`, `awk`, `tar`, `curl`, `openssl`, `find`. Windows has its own (and is missing several). Bash scripts go non-portable the moment you reach for any of them.

perch's ops *are* the std library, identical on every host:

- **Strings**: `trim`, `upper`, `lower`, `replace`, `split`, `join`, `contains`, `has_prefix`, `has_suffix`, `regex_match`, `regex_replace`, …
- **Hashing**: `md5`, `sha1`, `sha256`, `*_file`, `crc32`, `verify_sha256`.
- **Encoding**: `base64_*`, `hex_*`, `url_*`, `json_parse`, `json_get`, `json_stringify`.
- **HTTP**: `http_get`, `http_post`, `http_put`, `http_delete`, `download`, `http_status`.
- **Archive**: `tar_create`, `tar_extract`, `gzip`, `ungzip`, `zip_create`, `zip_extract`.
- **Filesystem**: `cp`, `mv`, `rm`, `mkdir`, `chmod`, `touch`, `glob`, `list_dir`, `walk`, `symlink`, `read_link`, `make_executable`, `ensure_dir`, `copy_dir`, `append_*`, `replace_in_file`, `backup_file`, `ensure_line_in_file`, …
- **Path** (cross-platform, no `/` vs `\` headaches): `path_join`, `path_dir`, `path_base`, `path_ext`, `path_abs`, `path_clean`, `path_rel`, `expand_path`, `to_slash`, …
- **Network**: `port_check`, `port_free`, `find_free_port`, `wait_for_port`, `wait_for_url`, `local_ip`, `public_ip`, `mac_address`, `dns_lookup`, `interfaces`.
- **Process / system**: `which`, `has_bin`, `bin_version`, `is_admin`, `is_ci`, `is_tty`, `cpu_count`, `os_version`, `hostname`, `pid`, `user`, `uid`.
- **Time / regex**: `now`, `format_time`, `parse_time`, `time_diff`, `regex_match`, `regex_replace`, `regex_find_all`.

What this gets you: **a `.perch` file is a portable shell script.** Disable the `shell` op (`--no-shell`) and your file uses *only* these ops — guaranteed identical behavior on macOS / Linux / Windows.

---

## 8. Package manager — `pkg_install`

Linux distros each have their own; macOS has brew; Windows has three. perch papers over them:

```capy
let mgr = detect_pkg_mgr      # "brew" / "apt" / "dnf" / "pacman" / "apk" / "zypper" / "winget" / "choco" / "scoop" / ""
pkg_install "ripgrep"          # picks the right manager automatically
```

`pkg_uninstall` and `pkg_installed` round out the trio. `--no-subprocess` gates these (they spawn package-manager processes).

---

## 9. Configuration / state — auto-bound directory variables

Every command starts with these bound, no declaration needed:

```
${home}            user's home dir
${config_dir}      OS-correct user config dir (~/.config / %APPDATA% / ~/Library/Application Support)
${cache_dir}       OS-correct user cache dir
${data_dir}        OS-correct user data dir
${temp_dir}        OS temp dir
${exe_path}        absolute path of the running binary (symlinks resolved)
${exe_dir}         directory of that binary
${script_path}     absolute path of the loaded .perch file (empty when embedded)
${script_dir}      directory containing it
${path_sep}        / or \
${exe_ext}         "" or ".exe"
${null_device}     /dev/null or NUL
${cpu_count}       runtime.NumCPU()
${user}, ${hostname}, ${is_windows}, ${is_macos}, ${is_linux}, ${is_arm64}, ${is_amd64}
```

Plus content-addressable storage via the **bundle ops**: `bundle_hash` (SHA-256 of an embedded archive), `bundle_extract DST` (extract once), `bundle_dir` (lazy-extract to temp dir, cached per-process). These let a built binary install itself into `~/.cache/perch/<hash>/` and never need to know its own version number — the hash IS the version.

---

## 10. Multiple frontends — one program, four user interfaces

Linux has X11 / Wayland (GUI), tty (CLI), DBus (IPC), ssh (remote). They're separate stacks talking to the same kernel.

perch is one stack with five frontends to the same dispatcher:

- **CLI** — `perch <cmd> ARGS`
- **Web UI** — `perch --server`, NDJSON-streamed
- **REPL** — `perch --shell`, persistent bindings
- **MCP** — `perch-mcp`, JSON-RPC over stdio for AI agents
- **Embedded binary** — `perch --build -o myapp` produces a standalone executable

All five share the same command set, the same arg parsing, the same op dispatch, the same restrictions, the same audit log. *That's* the OS analogy: the kernel is the same; the frontend is the consumer's choice. The deep dive on the agent case is [LLM control plane](llm-control-plane.md).

---

## What perch is NOT

To be honest about the limits:

- **It is not a kernel.** It can't enforce filesystem or network restrictions on a subprocess that legitimately reads a file or opens a socket. For that you need real OS sandboxing — Linux mount/network namespaces (firejail, bubblewrap), macOS sandbox-exec, Windows AppContainer. perch documents how to layer with those (see [sandbox §0c](sandbox.md#0c-the-subprocess-escape-hatch-and-the-layered-defense)) but doesn't reimplement them.
- **It is not multi-user.** No login, no per-user identity beyond what the host already provides. Identity is "whoever invoked perch."
- **It is not an init system.** There's no `systemd`-style service supervision yet. Long-running processes go to `shell_detached`; cleanup is via `on_signal`. Real supervision (restart policy, health checks, dependency graph) is a future direction, not a current feature.
- **It is not a hypervisor.** It does not provide hardware isolation. Two `perch` instances on the same machine share the same OS view.

---

## The roadmap shape

Concrete next steps that move further toward the OS analogy:

1. **Author-side `sandbox` block** ([sandbox.md §3](sandbox.md#3-the-sandbox-block-grammar)) — the file declares its own intended permissions; the runtime enforces the intersection with the user's CLI flags.
2. **Filesystem and network *scope* allowlists** (not just on/off) — `read "./src" "${cache_dir}"`, `net "*.github.com"`. Designed in §4.3 / §4.4.
3. **`--max-download`, `--max-file-size`, `--max-processes`** — the remaining resource ceilings.
4. **`perch --untrusted`** — refuses files without a sandbox block, prints a permission preview, applies safe defaults. Designed in §7.
5. **Plugin ops via WASM** — let users add their own ops, contained by a WASI sandbox, granted only the capabilities the host declares. The right place for WASM in the design, discussed in §9.
6. **Long-running service supervision** — restart policies + health checks for `shell_detached`-style processes. Closer to an init system.

Each of these tightens the OS analogy. The current set already covers the cases that matter most — running a `.perch` from a stranger safely, serving one to an AI agent with a forensic audit trail, fencing what a CI invocation can touch.

---

## Summary table — every OS knob, ranked by status

| Capability | Today | Designed | Not in scope |
|---|---|---|---|
| Op-set restriction | ✅ `--no-*` flags | ✅ author-side `sandbox` block | — |
| Env-var scope | ✅ `--env` (with subprocess scrub) | ✅ author-side `env A B C` | — |
| Shell-binary allowlist | ✅ `--allow-bin` | ✅ author-side `shell_bins` | — |
| Shell-metachar filter | ✅ `--no-shell-metachars` | — | — |
| FS read/write scope | — | ✅ `read PATTERN`, `write PATTERN` | OS-level mount-ns enforcement |
| Network host allowlist | — | ✅ `net "host[:port]"` | OS-level network-ns enforcement |
| Wall-clock limit | ✅ `--max-runtime` | — | — |
| Bytes-out / file-size limit | — | ✅ `max_download`, `max_file_size` | — |
| Process count limit | — | ✅ `max_processes` | — |
| Audit log | ✅ `--audit FILE.ndjson` | — | — |
| Step-through / dry-run | ✅ `--ask`, `--dry-run` | ✅ `--untrusted` permission preview | — |
| Static validation | ✅ `perch --check` | — | — |
| Subprocess env scrubbing | ✅ automatic with `--env` | — | — |
| Cross-platform std lib | ✅ ~140 ops | — | — |
| Package manager | ✅ `pkg_install` (9 backends) | — | — |
| Standard dirs | ✅ auto-bound vars | — | — |
| Multiple frontends | ✅ CLI / web / REPL / MCP / binary | — | — |
| Multi-user / login | — | — | use the host's |
| Init / service supervision | — | (roadmap) | — |
| Hardware / hypervisor | — | — | use the host's |

---

## The pitch in one paragraph

**perch is the operating system you can `scp`.** One Go binary, no daemon, no root, no install ceremony. ~140 cross-platform ops that work identically on macOS / Linux / Windows. Layer in capabilities (`--no-shell`, `--no-network`, `--no-write`), env scoping (`--env`), shell-call restrictions (`--allow-bin`, `--no-shell-metachars`), wall-clock budgets (`--max-runtime`), and a structured audit log (`--audit`) — and you have a controlled execution surface that's small enough to read end-to-end in an afternoon, strong enough to give an AI agent without losing sleep, and portable enough to ship to a server with one `scp` command. That's the OS you can fit in a program.

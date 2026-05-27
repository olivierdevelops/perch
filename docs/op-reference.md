# Op catalog

The built-in "standard library" — every op the perch runtime can dispatch. Each op is implemented in [`infra/ops/`](https://github.com/luowensheng/perch/tree/main/infra/ops) and registered in [`infra/capyloader/lib.capy`](https://github.com/luowensheng/perch/blob/main/infra/capyloader/lib.capy).

Ops fall into two shapes:

- **Statement ops** — invoked as a body line for their side effects. e.g. `shell "go build"`.
- **Capturable ops** — invoked via `let NAME = OP ARGS` to capture their return value. e.g. `let h = sha256_file "./bin"`.

Most ops support both shapes (return value is discarded if you don't `let`).

## Process & I/O

| Op | Signature | Notes |
|---|---|---|
| `print MSG`              | `(string)` | Prints `MSG` + newline to stdout. |
| `println MSG`            | `(string)` | Alias for `print`. |
| `eprintln MSG`           | `(string)` | Prints to stderr. |
| `shell CMD`              | `(string)` | Runs `CMD` via bash (POSIX) or cmd.exe (Windows). Inherits stdout/stderr. |
| `shell_output CMD`       | `(string) → string` | Same, but captures stdout as the return value. Usually used in `let`. |
| `shell_detached CMD`     | `(string)` | Starts and returns immediately. Use with `detached` modifier. |
| `fail MSG`               | `(string)` | Exits non-zero with the message. |
| `exit N`                 | `(int)`    | Exits with code `N`. |
| `sleep SECS`             | `(any)`    | Sleeps for `SECS` seconds. Accepts float. |
| `run TARGET`             | `(ident)`  | Calls another command. Bindings persist into the called command. |
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

(Mostly used via `let`.)

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
| `get_env NAME`      | `(string) → string` |
| `set_env NAME VAL`  | `(string, string)` |
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

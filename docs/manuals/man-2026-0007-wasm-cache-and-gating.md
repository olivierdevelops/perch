---
document_id: MAN-2026-0007
title: "wasm_run - compile cache, capability gating, typed errors, TLS roots and the serve page"
document_type: manual
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [wasm, ops, http, httpserver]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [perch authors, operators]
scope: "Task chapter for follow-ups F02 (persistent wasm compile cache), F03 (gating of mounts and hosts), F04 (typed wasm error kinds), F06 (system TLS roots) and F07 (serve page fix) of PROP-2026-0002."
reason: "wasm_run was slow on every start, mounts and hosts were not gated by the file's manifest, wasm errors had no kinds, HTTP ignored the system trust store, and the serve page truncated."
related_documents: [MAN-2026-0001, PLAN-2026-0001, PROP-2026-0002]
supersedes: null
superseded_by: null
tags: [manual, wasm, cache, gating, errors, tls, web-ui]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# wasm_run - compile cache, capability gating, typed errors, TLS roots and the serve page

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** wasm runtime in the ops crate, HTTP ops, the web UI server
> **Verified:** F02, F03, F04 and F07 were run on macOS arm64 with the `rust-port` build (0.1.1). **F06 was not verified end to end** (it needs a locally trusted CA in the system store); see its section.

## Summary

| ID | Change | Where |
|---|---|---|
| F02 | Compiled wasm modules are cached on disk and reused by later runs | [below](#f02-persistent-compile-cache) |
| F03 | `wasm_mount_read`, `wasm_mount_write` and `wasm_allow_host` are checked against the file's `requires` roots and hosts, and against `--no-write` / `--no-network` | [below](#f03-mounts-and-hosts-are-gated) |
| F04 | wasm failures carry the documented error kinds | [below](#f04-typed-error-kinds) |
| F06 | `http_*` trusts the operating system's root certificates | [below](#f06-system-tls-roots) |
| F07 | The `/` page of `perch --server` no longer truncates for commands with arguments | [below](#f07-the-serve-page) |

For the `wasm_run` language itself see [wasm.md](../wasm.md).

## F02 Persistent compile cache

### Why

Compiling a 2.5 MB module with the bundled runtime (wasmtime) took about two seconds on the verification machine, every run. A serialized, already-compiled module loads in milliseconds.

### How it behaves

- The first `wasm_run` of a module compiles it and writes the compiled form to the cache directory. Later runs, in any perch process, load that file.
- Each entry is keyed by the SHA-256 of the module bytes, the wasmtime major version and an engine fingerprint, so a changed module, a runtime upgrade or a changed engine configuration produces a new entry instead of reusing a stale one. File name: `<sha256>-wt40-epoch1<engine hash>.cwasm`.
- Each file starts with a magic marker and a checksum of its payload; an entry is deserialized only after the checksum verifies. A damaged or foreign entry is ignored, the module is recompiled, and the entry is rewritten.
- Writes are atomic (temp file then rename) and best effort: a missing or read-only cache directory never fails a run.
- There is no eviction. Delete the directory to clear it.

### Location and switches

| Name | Kind | Values | Default | Effect |
|---|---|---|---|---|
| `PERCH_WASM_CACHE_DIR` | env var | a directory path | the user cache directory below | Overrides the cache directory (useful for tests and read-only homes). |
| `PERCH_WASM_CACHE` | env var | `off`, `0`, `false`, `no`, `disabled` (case-insensitive) | on | Disables reads and writes. Anything else leaves the cache on. |

Default directory, `<user cache dir>/perch/wasm`:

| OS | `<user cache dir>` |
|---|---|
| macOS | `$HOME/Library/Caches` (verified: with `HOME` pointed at a scratch directory the entry landed in `Library/Caches/perch/wasm/`) |
| Linux | `$XDG_CACHE_HOME`, else `$HOME/.cache` (from the source; not run on Linux) |
| Windows | `%LocalAppData%` (from the source; not run on Windows) |

If no user cache directory can be determined (no `HOME`), the cache is simply not used.

### Verified timings and behaviour

Same module (`demos/wasm-hello/hello.wasm`, 2.5 MB), same command, `PERCH_WASM_CACHE_DIR` pointing at an empty scratch directory (timings are machine specific; the ratio is the point):

```text
cache off (empty dir)              rc=0  2.03s entries=0
cold, cache on                     rc=0  2.05s entries=1
warm                               rc=0  0.07s entries=1
cache off while populated          rc=0  2.02s entries=1
corrupt entry (100 random bytes)   rc=0  2.09s entries=1
entry size after:                  8112400
warm again                         rc=0  0.08s entries=1
read-only cache dir                rc=0  2.05s entries=0
```

Reading the table: with the cache off nothing is written and nothing is read, even when an entry exists; a corrupted entry costs one recompile and is repaired (it went from 100 bytes back to 8112400); a read-only directory costs the compile every time and never errors. A different module gets its own entry (a second module made the directory hold two files).

An 8 MB `.cwasm` for a 2.5 MB module is normal: it is the machine code.

### Procedure

```sh
export PERCH_WASM_CACHE_DIR="$HOME/.cache/perch-wasm"   # optional
perch -f w.perch demo        # first run: compiles and stores
perch -f w.perch demo        # later runs: loads from the cache
PERCH_WASM_CACHE=off perch -f w.perch demo     # bypass entirely
rm -rf "$PERCH_WASM_CACHE_DIR"                 # clear
```

## F03 Mounts and hosts are gated

Before 0.2.0, `wasm_mount_*` and `wasm_allow_host` were not checked against the file's `requires` block (this was also true in the Go build). Now:

| Declaration | Allowed only if |
|---|---|
| `wasm_mount_read "P"` | `P` is inside a declared `read` or `write` root. |
| `wasm_mount_write "P"` | `P` is inside a declared `write` root, and `--no-write` is not active, and the enclosing `sandbox` mask allows writes. |
| `wasm_allow_host "H"` | `H` is a declared `host`, and `--no-network` is not active, and the enclosing `sandbox` mask allows network. |

As everywhere in perch, a file with no `requires` block is an empty manifest, so it can mount nothing and allow no host until the block says so. Matching is the same prefix matching perch's own file ops use, on the resolved absolute path.

`g.perch` (declares `read "./src"`, `write "./bin"`, `host "api.example.com"`):

```text
$ perch -f g.perch badread          # wasm_mount_read "./other"
wasm_capability_denied: wasm_mount_read: read of "/…/w/other" is outside every declared `read` root in `requires` (/…/w/other)
$ perch -f g.perch badwrite         # wasm_mount_write "./src"
wasm_capability_denied: wasm_mount_write: write to "/…/w/src" is outside every declared `write` root in `requires` (/…/w/src)
$ perch -f g.perch badhost          # wasm_allow_host "evil.example.org"
wasm_capability_denied: wasm_allow_host: host "evil.example.org" is not declared in `requires` (evil.example.org)
$ perch -f g.perch --no-write okmounts
🔒 security: --no-write
wasm_capability_denied: wasm_mount_write "/…/w/bin" forbidden by --no-write (/…/w/bin)
$ perch -f g.perch --no-network goodhost
🔒 security: --no-network
wasm_capability_denied: wasm_allow_host "api.example.com" forbidden by --no-network (api.example.com)
$ perch -f g.perch goodhost         # declared host, no restriction
── hello.wasm ────────────────────────────────
argv: [hello.wasm]
...
```

(`/…/` stands for the long scratch path.) The part in parentheses at the end is the error's detail (the path or host).

The outer `--allow-host` policy and the SSRF guard still apply on top of `wasm_allow_host`; see [wasm.md](../wasm.md).

## F04 Typed error kinds

wasm failures now carry the four documented kinds ([errors.md](../errors.md)), so `rescue` can branch on `${err.kind}`. The format is `kind: message`.

| Kind | Meaning | Verified output |
|---|---|---|
| `wasm_compile_failed` | Module missing or not valid wasm | `wasm_compile_failed: wasm_run: module "./nope.wasm": stat /…/w/nope.wasm: no such file or directory` and `wasm_compile_failed: wasm_run: compile "./g.perch": magic header not detected: bad magic number - expected=[ 0x0, 0x61, 0x73, 0x6d ] actual=[ 0x72, ... ] (at offset 0x0)` |
| `wasm_module_exited` | Non-zero exit, or the deadline expired inside the module | `wasm_module_exited: wasm_run "./exit2.wasm": module closed with exit_code(2)` |
| `wasm_capability_denied` | A mount or host was refused (F03) | see above |
| `wasm_http_refused` | The module called `perch.http_get` and perch refused; the reason is appended in parentheses | below |

The `wasm_http_refused` detail form: the module itself only sees the failure return code, so the module's exit is reported as a refusal and the host-side reason rides along in parentheses. Two real cases, with a module whose only job is to call `http_get "http://127.0.0.1:1/x"` and exit 1 on failure:

```text
$ perch -f k.perch nohost           # no wasm_allow_host in the block
wasm_http_refused: wasm_run "./http.wasm": module closed with exit_code(1) (host not allowed: no wasm_allow_host declared or none permitted by the outer policy)
$ perch -f k.perch withhost         # wasm_allow_host "127.0.0.1" declared, default HTTP policy
wasm_http_refused: wasm_run "./http.wasm": module closed with exit_code(1) (127.0.0.1 is a loopback address (use --allow-private-ips to permit))
```

A deadline that fires while a module is running is reported as `wasm_module_exited` with `module closed with context deadline exceeded`.

Recovery: match on the kind and read `${err.detail}` for the reason, or fix the declaration the message names.

## F06 System TLS roots

`http_get`, `http_post` and the other HTTP ops now trust the operating system's certificate store (macOS keychain, the Linux distribution's bundle, the Windows store) so corporate and locally issued CAs work. The bundled Mozilla roots are used only if the system store yields no usable certificate at all, so public sites keep working on a machine with an empty store.

**What was and was not verified.** The selection logic is in `infra/ops/src/group_b/http.rs` (`tls_config`), and a unit test checks that a client builds with either source. The end-to-end proof, a local CA trusted in the OS store with `openssl s_server`, is a manual procedure recorded as a comment in that file and as T-42 in the plan; it modifies the machine's trust store and was **not run** while writing this chapter. Treat F06 as implemented and unit-tested, not demonstrated.

## F07 The serve page

`perch --server` rendered the `/` page with a template that used the wrong variable inside the command loop, so any visible command that had an argument cut the page off right after `<label for="`. Fixed. Verified with a file whose first command has an argument:

```text
$ perch -f s.perch --server --port 18765 &
perch UI on http://127.0.0.1:18765
$ curl -s http://127.0.0.1:18765/ | grep -n 'label for=\|data-name="second"'
131:      <label for="name">-name</label>
149:  <div class="cmd" data-name="second" data-haystack="second Another">
$ curl -s http://127.0.0.1:18765/ | tail -3
</script>
</body>
</html>
```

The page contains the argument's label, the commands after it, and the closing tags. See [web-ui.md](../web-ui.md).

## Errors and recovery

| Symptom | Cause | Recovery |
|---|---|---|
| Every run takes seconds | Cache disabled, directory read-only or unwritable, or no `HOME` | Check `PERCH_WASM_CACHE`; set `PERCH_WASM_CACHE_DIR` to a writable path. |
| `wasm_capability_denied: wasm_mount_*` | Mount outside declared roots, or `--no-write` | Declare the root or remove the mount. |
| `wasm_capability_denied: wasm_allow_host` | Host not declared, or `--no-network` | Declare it under `requires host`. |
| `wasm_http_refused ... (reason)` | Host not allowed by the block or the outer policy, SSRF guard, redirect policy | Read the parenthesised reason. |

## Limitations

- No cache eviction or size limit.
- The cache stores machine code for this CPU and this wasmtime build; it is not portable between machines (the key includes the engine's compatibility hash, so a copied entry from a different build is simply not used).
- Only `GET` and no custom headers inside modules (unchanged, see [wasm.md](../wasm.md)).
- F06 is not demonstrated end to end (see above). Linux and Windows cache paths are from the source.

## Version applicability

| Feature | Introduced |
|---|---|
| In-process module cache | 0.1.0 |
| Persistent on-disk compile cache, `PERCH_WASM_CACHE`, `PERCH_WASM_CACHE_DIR` | 0.2.0 |
| Gating of mounts and hosts by `requires`, `--no-write`, `--no-network` | 0.2.0 |
| Typed wasm error kinds, `(reason)` detail form | 0.2.0 |
| System TLS roots with bundled fallback | 0.2.0 |
| `/` page fix | 0.2.0 |

## Related documents

[MAN-2026-0001](man-2026-0001-perch-manual-index.md) · [wasm.md](../wasm.md) · [errors.md](../errors.md) · [web-ui.md](../web-ui.md) · [requires.md](../requires.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · Verified demo: DEMO-2026-0001 (planned at `docs/demos/demo-2026-0001-perch-0-2-0-verification.md`).

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial chapter for F02, F03, F04, F06, F07. |

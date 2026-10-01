---
document_id: MAN-2026-0002
title: "Structured scan - read what a perch file declares and what it uses"
document_type: manual
status: draft
created_date: 2026-10-01
last_updated: 2026-10-01
document_revision: 1
authors: [Claude]
owner: Perch maintainers (proposed)
reviewers: [Perch language maintainer, Perch security maintainer, Perch release maintainer]
systems: [Perch]
components: [scan, CLI]
affected_versions:
  from: "0.2.0"
  to: null
applicable_environments: [development, CI, macOS, Linux, Windows]
audience: [tool authors, operators, reviewers]
scope: "Task chapter for `perch --scan --json` (requirement R01 of PROP-2026-0002) - output schema, declared versus inferred, risk, advice behaviour, and a consumer snippet."
reason: "A tool that wraps perch must compare what a file declares with what it uses without parsing prose."
related_documents: [MAN-2026-0001, PLAN-2026-0001, PROP-2026-0002, REF-2026-0003]
supersedes: null
superseded_by: null
tags: [manual, scan, json, trust, requires]
confidentiality: internal
review_cycle: on-release
next_review_date: 2026-11-01
---

# Structured scan - read what a perch file declares and what it uses

> **Status:** Draft · **Created:** 2026-10-01 · **Last Updated:** 2026-10-01
> **Affected Versions:** 0.2.0 and later
> **Owner:** Perch maintainers (proposed)
> **Affected Components:** scan use case, CLI flag parsing
> **Verified:** every command and output below was run on the `rust-port` branch build that reports `perch --version` = 0.1.1 (the 0.2.0 version bump happens at release).

## Summary

`perch -f FILE --scan --json` prints one JSON document that separates three things a wrapping tool needs:

- `declared`: what the file's `requires` block says it needs (bins with hash pins, env, hosts, read and write roots, os, arch).
- `inferred`: what perch can see the file's operations actually use (shell calls, spawned bins, hosts, paths, env names, env prefixes).
- `risk`: the same coarse classification the text report shows (`safe`, `low`, `med`, `high`) plus the reasons.

The human text report is unchanged except for one advice fix (see [Advice behaviour](#advice-behaviour)).

## Purpose and when to use it

Use it when a program, not a person, must decide whether a `.perch` file is acceptable to run: an agent runtime, a CI gate, a code-review bot, a launcher. It answers "what can this file reach?" before anything runs. `--scan` never executes an op.

Prerequisites: a `perch` binary (0.2.0 or later) and a `.perch` file that loads. For the underlying model see [requires.md](../requires.md) (the manifest) and [sandboxed-by-design.md](../sandboxed-by-design.md).

## Journey overview

```text
[caller] -> perch -f FILE --scan --json -> [load + analyze, no ops run] -> JSON on stdout, exit 0
                |                                   |
                |                                   +-> file does not load -> message on stdout, exit 1
                +-> bad --format value / missing value -> message on stderr, exit 2
```

## CLI procedure

### Flags

| Form | Meaning |
|---|---|
| `--scan` | Text report (default). |
| `--scan --json` | JSON report. |
| `--scan --format json` or `--scan --format=json` | Same as `--json`. |
| `--scan --format text` | Explicit text report. |

`-f FILE` selects the file (default `commands.perch`). The flags may be given in any order after `--scan`.

### A file with a manifest

`req.perch`:

```perch
name "scan-demo"
version "1.0.0"

requires
    bin "sh"
    env "HOME"
    host "api.example.com"
    read "./data"
    write "./allowed"
end

command build
    description "build"
    do
        mkdir "./allowed"
        sh -c "echo hi > ./allowed/out.txt"
        http_get "https://api.example.com/x"
    end
end
```

```text
$ perch -f req.perch --scan --json
{
  "declared": {
    "arch": [],
    "bin": [
      {
        "alias": "",
        "hash": "",
        "hash_file": "",
        "name": "sh",
        "optional": false
      }
    ],
    "declared": true,
    "env": [
      {
        "name": "HOME",
        "optional": false
      }
    ],
    "host": [
      {
        "name": "api.example.com",
        "optional": false
      }
    ],
    "os": [],
    "read": [
      "./data"
    ],
    "write": [
      "./allowed"
    ]
  },
  "file": "req.perch",
  "inferred": {
    "catch_forwards": false,
    "env": [],
    "env_prefix": [],
    "exec_bins": [
      "sh"
    ],
    "hosts": [
      "api.example.com"
    ],
    "network": true,
    "read": false,
    "read_roots": [],
    "shell": {
      "bins": [],
      "calls": 0,
      "metachars": false,
      "sudo": false
    },
    "subprocess": true,
    "subprocess_ops": [
      "exec"
    ],
    "write": true,
    "write_roots": [
      "./allowed"
    ]
  },
  "risk": "med",
  "risk_reasons": [
    "spawns subprocesses (pkg_install / kill / process_running)",
    "network access (1 host)",
    "writes the filesystem (1 root)"
  ],
  "schema": 1
}
```

### Failure examples

```text
$ perch -f req.perch --scan --format=yaml ; echo rc=$?
unknown --format value "yaml" for --scan (want text or json)
rc=2

$ perch -f req.perch --scan --format ; echo rc=$?
--format requires a value (text or json)
rc=2

$ perch -f nonexist.perch --scan --json ; echo rc=$?
read nonexist.perch: open nonexist.perch: no such file or directory
rc=1
```

The first two messages go to stderr; the load failure goes to stdout (this is how perch reports every load error). A file that fails to load or has a parse error produces no JSON, so a consumer must check the exit code before parsing.

## Schema (`"schema": 1`)

Keys are emitted alphabetically and every list is sorted or in declaration order (stated below), so the output is byte-stable for a given file and can be compared in golden tests. The `schema` field is the version of this document structure; any breaking change bumps it.

| Path | Type | Meaning |
|---|---|---|
| `schema` | integer | Always `1` in 0.2.0. |
| `file` | string | The path exactly as passed with `-f` (default `commands.perch`). |
| `risk` | string | `safe`, `low`, `med` or `high`. Same classification as the text badge. |
| `risk_reasons` | string[] | Human-readable reasons. Do not parse them: the wording is not a stable API. |
| `declared.declared` | boolean | Always `true` in 0.2.0. A file with no `requires` block is treated as an empty manifest (nothing declared, nothing allowed), so this field does not distinguish "no block" from "empty block". |
| `declared.bin[]` | objects | One per `bin` line, declaration order. Fields: `name`, `alias` (empty if none), `hash` (`sha256:...` or empty), `hash_file`, `optional`. |
| `declared.env[]`, `declared.host[]` | objects | `{name, optional}`, declaration order. Hosts keep wildcards such as `*.amazonaws.com`. |
| `declared.read[]`, `declared.write[]` | strings | Roots as written, declaration order. |
| `declared.os[]`, `declared.arch[]` | strings | Sorted. |
| `inferred.shell` | object | `calls` (count of `shell`-family ops), `bins` (first token of each shell string, sorted), `metachars` (a shell string uses pipes, redirects, `;`, `&&`, `$()`), `sudo`. |
| `inferred.subprocess`, `inferred.subprocess_ops[]` | boolean, string[] | Ops that spawn processes other than `shell` (for example `exec`). |
| `inferred.exec_bins[]` | string[] | Binaries called through a bare declared-bin call, `exec` or `pipe`. |
| `inferred.env_prefix[]` | objects | One per call that carries an inline `NAME=VALUE` prefix: `{bin, names[], op}` (`op` is always `"exec"`). Names only, never values. See [MAN-2026-0006](man-2026-0006-env-prefix.md). |
| `inferred.network`, `inferred.hosts[]` | boolean, string[] | Network ops and the literal hosts they target. Hosts built from `${var}` are not listed. |
| `inferred.write`, `inferred.write_roots[]` | boolean, string[] | Filesystem-mutating ops and the literal paths they touch, as written. |
| `inferred.read`, `inferred.read_roots[]` | boolean, string[] | Same for read ops. |
| `inferred.env[]` | string[] | Host env names referenced as `${UPPER_CASE}` or in a prefix. |
| `inferred.catch_forwards` | boolean | A `catch` block forwards unknown verbs to a shell. |

A second, richer example (a pinned bin, an optional path bin with an alias, a prefix, reads and writes):

```text
$ perch -f rich.perch --scan --json | jq -c '{risk, declared_bin: [.declared.bin[]|.name], env_prefix: .inferred.env_prefix, read_roots: .inferred.read_roots, write_roots: .inferred.write_roots, env: .inferred.env}'
{"risk":"med","declared_bin":["kubectl","./tools/lint"],"env_prefix":[{"bin":"kubectl","names":["KUBECONFIG"],"op":"exec"}],"read_roots":["./manifests/app.yaml"],"write_roots":["./out/zen.txt"],"env":["HOME","KUBECONFIG"]}
```

### A file with nothing declared

```text
$ perch -f plain.perch --scan --json | jq -c '{risk, risk_reasons, declared: .declared | del(.bin,.env,.host)}'
{"risk":"safe","risk_reasons":["no privileged operations — pure ops only"],"declared":{"arch":[],"declared":true,"os":[],"read":[],"write":[]}}
```

## Declared versus inferred

`declared` is the file's promise; `inferred` is what perch can observe in the operations. The gap is the useful signal:

| Situation | How it shows |
|---|---|
| The file uses a host it did not declare | `inferred.hosts` contains a name missing from `declared.host`. Scan still succeeds; `perch --check` reports it as an error. |
| The file declares a write root but its only writer is a spawned binary | `declared.write` is non-empty and `inferred.write` is `false`. Perch cannot see inside the binary; at run time the declared root is what confines it (see [MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md)). |
| The file declares nothing and uses a spawned bin | The file does not load: `bin_not_declared`. There is nothing to scan. |
| The file calls a bin through a prefix | `inferred.env_prefix` lists the variable names, so a reviewer sees which environment each call receives. |

Bins are checked when the file loads, so an undeclared bin never reaches scan. Hosts and paths are checked statically by `--check` and at run time.

## Consumer snippets

With `jq` (verified with jq 1.7):

```sh
perch -f un.perch --scan --json | jq -r '
  (.inferred.hosts - [.declared.host[].name]) as $h
| "undeclared hosts: \($h | join(","))"'
```

```text
undeclared hosts: untrusted.org
```

With Python 3 (no third-party packages), as a CI gate that fails on `high` risk or on any undeclared host:

```python
import json, subprocess, sys

def scan(path):
    out = subprocess.run(["perch", "-f", path, "--scan", "--json"], capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"scan failed: {out.stdout.strip() or out.stderr.strip()}")
    return json.loads(out.stdout)

r = scan(sys.argv[1])
assert r["schema"] == 1, "unknown schema version"
declared_hosts = {h["name"] for h in r["declared"]["host"]}
extra_hosts = sorted(set(r["inferred"]["hosts"]) - declared_hosts)
print("risk:", r["risk"])
print("hosts used but not declared:", extra_hosts)
sys.exit(1 if r["risk"] == "high" or extra_hosts else 0)
```

```text
$ python3 check_scan.py un.perch ; echo rc=$?
risk: low
hosts used but not declared: ['untrusted.org']
rc=1
$ python3 check_scan.py rich.perch ; echo rc=$?
risk: med
hosts used but not declared: []
rc=0
```

Note: wildcard hosts (`*.amazonaws.com`) are matched by perch with suffix rules; the snippet compares exact names, so a real gate should apply the same wildcard rule.

## Advice behaviour

The text report ends with a "recommended invocation" and each capability line may say "add `--no-write` for free". Before 0.2.0 that advice ignored the manifest: a file declaring `write "./allowed"` whose only writer was a spawned binary was told to add `--no-write`, which would break it.

From 0.2.0 a capability covered by a declared scope is not advised away:

```text
$ perch -f rq.perch --scan        # requires: bin "sh", write "./allowed"; body: shell "sh -c 'echo in > allowed/ok.txt'"
...
  CAPABILITIES NEEDED
    ✓ shell        (1 call(s), binaries: sh)  ⚠ pipes/redirects
    ✗ subprocess   — add `--no-subprocess` for free
    ✗ network      — add `--no-network` for free
    ✗ writes       — none seen in ops; declared `write` scope covers any use by spawned binaries

  RECOMMENDED INVOCATION
    perch \
      --no-subprocess \
      --no-network \
      --allow-bin sh \
      --max-runtime 600 \
      --audit /var/log/perch-rq.ndjson \
      -f rq.perch
```

The same rule applies to a declared `host` scope: the network line reads "none seen in ops; declared `host` scope covers any use by spawned binaries" and `--no-network` is not recommended. `--no-write` and `--no-network` remain in the advice when nothing is declared and nothing is used.

## Expected result and side effects

- Output only on stdout (JSON) with exit code 0. No file is written, no op runs, no network is touched.
- The scan reads the `.perch` file and its imports.

## Errors and recovery

| Symptom | Cause | Recovery |
|---|---|---|
| `unknown --format value "X" for --scan (want text or json)`, exit 2 | A `--format` value other than `text` or `json`. | Use `json` or `text`. |
| `--format requires a value (text or json)`, exit 2 | `--format` was last on the line. | Add the value, or use `--json`. |
| Load error text on stdout, exit 1, no JSON | The file does not parse or fails the load-time bin check. | Run `perch -f FILE --check` for the full list of problems. |

## Limitations

- Static analysis. Literal values only: a host, path or shell string built from `${var}` is not listed (the text report says so for hosts and write paths).
- `risk_reasons` wording and the thresholds behind `risk` are a coarse heuristic, not a security guarantee.
- `inferred.read_roots` and `inferred.write_roots` hold the literal paths as written (for example `./manifests/app.yaml`), not normalised directory roots.
- The `perch help` catalog (`perch help --scan`) still describes the text report only; it has not been updated for `--json` (known documentation gap in the help catalog, not in the scan behaviour).

## Version applicability

| Feature | Introduced | Notes |
|---|---|---|
| `--scan` text report | 0.1.0 | |
| `--scan --json`, `--format json|text` | 0.2.0 | `schema: 1`. |
| Advice accounts for declared `write` and `host` scopes | 0.2.0 | |
| `inferred.env_prefix`, `inferred.exec_bins` | 0.2.0 | Scan walks `exec` calls and chained clauses. |

## Related documents

[MAN-2026-0001](man-2026-0001-perch-manual-index.md) · [MAN-2026-0004](man-2026-0004-confining-spawned-binaries.md) · [MAN-2026-0006](man-2026-0006-env-prefix.md) · [requires.md](../requires.md) · [trust-by-manifest.md](../trust-by-manifest.md) · [PROP-2026-0002](../proposals/prop-2026-0002-perch-safe-service-layer-and-embeddable-runtime.md) · [PLAN-2026-0001](../plans/plan-2026-0001-perch-safe-embeddable-layer.md) · Verified demo: DEMO-2026-0001 (planned at `docs/demos/demo-2026-0001-perch-0-2-0-verification.md`).

## Change History

| Revision | Date | Author | Change |
|---|---|---|---|
| 1 | 2026-10-01 | Claude | Initial chapter for R01. |

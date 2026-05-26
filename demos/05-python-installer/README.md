# demo 05 — Python-installer

**Ship a Python project as one self-installing binary.** The recipient has no `pip`, no `venv`, no clone — just a single `stt_bin` file.

## Build

```sh
perch --build -f commands.perch --include ./src -o stt_bin
```

`--include ./src` tarballs the Python project (excluding `.git`, `__pycache__`, `.venv`, …) and embeds it inside the binary. Result: `stt_bin`, ~12 MB.

## Use

```sh
./stt_bin install     # extract → venv → pip install → drop launcher in ~/.local/bin/stt
./stt_bin status      # where everything landed
./stt_bin run example.wav --lang=es   # bypass the launcher
stt example.wav       # use the global launcher (after install)
./stt_bin uninstall   # remove install + launcher
```

## What it demonstrates

- **`bundle_hash`** — SHA-256 of the embedded archive. Used as a content-addressable install dir (`~/.cache/perch/stt_bin/<hash>/`). Reinstalling the same binary returns the same hash; upgrading to a new build gets a new hash → multiple versions coexist; pruning is `rm -rf <old_hash>`.
- **`bundle_extract DST`** — extracts the archive to `DST`. Idempotent given the same content.
- **`bundle_dir`** — lazy-extracts to an OS temp dir on first call; cached for the rest of the process. Used by `stt_bin run`, which doesn't need a permanent install.
- **`proxy_args`** — `stt_bin run example.wav --lang=es` forwards every argument verbatim into the spawned python.
- **`catch`** — friendly "did you mean…?" when someone types `stt_bin start` (a verb we don't have).

## The pattern

This is the **polyglot installer** pattern. The same shape works for:

- **JS / TS** — embed `package.json` + source, install does `npm install --omit=dev` + writes a Node launcher.
- **Ruby** — embed + `bundle install`.
- **Static binaries** — embed the pre-compiled binary itself, install just copies it into PATH.
- **A whole monorepo** — embed `src/`, install builds each language's piece (Go binary, Python venv, Node deps).

The user's machine needs only what your install command requires (in the Python case: `python3`). No package manager. No registry. No internet at install time (everything's in the binary).

## Distribution

Once `stt_bin` is built, distribution is whatever you'd do for any binary:

- `scp stt_bin user@server:/usr/local/bin/`
- Upload to GitHub Releases
- `curl -fsSL https://internal/stt_bin -o stt_bin && chmod +x stt_bin && ./stt_bin install`

The recipient runs **one binary**. No `pipx`, no venv setup, no python-version drama.

# Security Policy

## Supported versions

perch is pre-1.0. Only the latest release receives security patches.

| Version | Supported |
|---|---|
| 0.1.x   | ✅ |
| < 0.1.0 | ❌ |

## Reporting a vulnerability

If you discover a vulnerability in perch — particularly anything affecting:

- the `--build` fat-binary loader (`infra/embed/embed.go`)
- the capy → program loader (`infra/capyloader/loader.go`)
- arbitrary code execution paths in the op catalog (`infra/ops/`)
- the HTTP server endpoints (`infra/httpserver/server.go`)

please **do not** open a public GitHub issue.

Instead, email the maintainer or use GitHub's private security advisory feature:

→ <https://github.com/luowensheng/perch/security/advisories/new>

You can expect:

- Acknowledgement within 72 hours.
- A target fix window of 14 days for high-severity issues, 30 days for medium.
- Credit in the release notes once the patch lands (unless you prefer to remain anonymous).

## Security model

perch is a task runner. It executes whatever is in `commands.perch`. A malicious `commands.perch` can run arbitrary shell commands; **the runtime has no sandbox**. Treat `commands.perch` exactly as you'd treat a shell script.

Specifically:

- The embedded program in a fat binary built by `--build` is **not signed**. Use platform code signing (`codesign`, `signtool`) on the output binary if your distribution channel requires it.
- The HTTP server (`perch --server`) defaults to `127.0.0.1`. Binding to a public interface exposes every command to anyone who can reach the port.
- Op handlers like `http_post`, `download`, `shell` make outbound calls / write files / spawn processes. Audit `commands.perch` before running anything untrusted.

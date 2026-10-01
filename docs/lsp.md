# Language server (`perch-lsp`)

`perch-lsp` is a Language Server Protocol implementation for `.perch` files. Drop it into your editor and you get:

- **Diagnostics** — every parse error and every `perch --check` finding shown inline as you type.
- **Completion** — context-aware suggestions: top-level keywords, command-config statements, arg-block fields, and the full op catalog inside `do` blocks. Command names declared in the current file are also suggested for bare invocation.
- **Hover** — point at a keyword or op and read its signature + docstring.
- **Outline** — every command (and its args) appear in the editor's symbol picker.

### Keyword documentation (0.2.0)

Hover and completion know the 0.2.0 syntax:

- **`finally`** — hover: "Cleanup section that always runs. Command level: `do … finally … end` (sugar for wrapping the whole body in `try … finally … end`). Inside `try … rescue … finally … end` the same rules apply. A failing `finally` never hides the body error." Completion offers `finally` ("cleanup that always runs (`do … finally … end` or inside `try`)").
- **`do`** — hover now mentions the optional `finally` section, the `; additionally, finally failed: …` rule and the 5 s grace on `timeout`/`--max-runtime`.
- **`NAME=value`** — completion entry "inline env prefix: NAME=value binary verb --args (bins / exec only)", with hover text covering `$NAME` / `${NAME}` values, the `requires env` / `--env` gating, one-process scope, and that `name=value` after the binary is an ordinary argument.

(Verified by driving `perch-lsp` over stdio: `textDocument/hover` on `finally` and `do`, and `textDocument/completion` inside a command body returning both `finally` and `NAME=value`.) Diagnostics come from the same loader and validator as `perch --check`, so a misplaced `finally` or an env prefix on a built-in op is reported as the load error described in [manuals/man-2026-0003-cleanup-with-finally.md](manuals/man-2026-0003-cleanup-with-finally.md) and [manuals/man-2026-0006-env-prefix.md](manuals/man-2026-0006-env-prefix.md). The server currently identifies itself as version 0.1.0 in `initialize`.

## Install

The fastest path — let perch do it:

```sh
perch --install-lsp
```

`perch --install-lsp` downloads the `perch-lsp` release asset for your OS and architecture (`perch-lsp-<os>-<arch>[.exe]`) together with the release's `checksums.txt`, **verifies the asset's sha256** (it refuses to install on a mismatch or a missing entry), and installs it next to the running `perch` executable when that directory is writable, otherwise into `~/.local/bin` (macOS/Linux) or `%LOCALAPPDATA%\perch` (Windows). No Go toolchain is needed. It needs network access to GitHub; if it can't download, nothing is installed. (The download-and-verify logic is covered by the use case's unit tests against a fake server; it was not run against the live release for this page.) The installer prints the path it installed to — make sure that directory is on your `$PATH`.

Or build it from source (needs a Rust toolchain):

```sh
cargo install --path cmd/perch-lsp               # from a checkout
cargo install --git https://github.com/olivierdevelops/perch perch-lsp   # from the repository
```

## VS Code (one command)

```sh
perch --install-vscode
```

`perch` itself extracts the embedded extension files, runs `npm install` + `vsce package`, and `code --install-extension`s the resulting `.vsix`. No repo checkout needed; `perch` is the only binary you need.

Requirements: `node` + `npm` and VS Code's `code` CLI on `$PATH`. If `code` isn't on `$PATH`: open VS Code → Command Palette → "Shell Command: Install code command in PATH".

If you'd rather drive it from a perch checkout, the same logic is in [`scripts/install-vscode.sh`](https://github.com/olivierdevelops/perch/blob/main/scripts/install-vscode.sh).

Configurable via VS Code settings (the only setting):

```jsonc
{ "perch.lsp.path": "perch-lsp" }   // override if perch-lsp isn't on $PATH
```

If the server hiccups, run **perch: Restart Language Server** from the command palette.

### Manual install

If `code --install-extension` isn't available:

```sh
cd editors/vscode-perch
npm install
npx @vscode/vsce package
# → produces perch-0.1.0.vsix; install from VS Code's UI: Extensions panel → "..." → "Install from VSIX…"
```

## Neovim (built-in LSP)

Add to `init.lua`:

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

-- Register the perch language definition (file type + executable).
if not configs.perch_lsp then
  configs.perch_lsp = {
    default_config = {
      cmd = { "perch-lsp" },
      filetypes = { "perch" },
      root_dir = lspconfig.util.root_pattern("commands.perch", ".git"),
      settings = {},
    },
  }
end

vim.filetype.add({ extension = { perch = "perch" } })

lspconfig.perch_lsp.setup({
  -- optional: on_attach = function(client, buf) ... end,
})
```

Diagnostics, completion, hover, and outline (`:Telescope lsp_document_symbols` or `:lua vim.lsp.buf.document_symbol()`) all light up automatically.

## Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "perch"
scope = "source.perch"
file-types = ["perch"]
roots = ["commands.perch"]
comment-token = "#"
indent = { tab-width = 4, unit = "    " }
language-servers = ["perch-lsp"]

[language-server.perch-lsp]
command = "perch-lsp"
```

Helix picks it up after restart. Use `Space + s` to see the outline, `gh` to hover, `Ctrl-x` for completion.

## Zed

Zed's extension format is in flux; once stable, a `perch` extension will publish from `editors/zed-perch/`. In the meantime, Zed 0.140+ honours generic LSP entries — add to your settings:

```json
{
  "languages": {
    "perch": {
      "language_servers": ["perch-lsp"]
    }
  },
  "lsp": {
    "perch-lsp": {
      "binary": { "path": "perch-lsp" }
    }
  }
}
```

## What's not yet supported

These are on the roadmap (issues / PRs welcome):

- **Go-to-definition** — `foo` → jump to `command foo`. Requires source-position info from the capy parser.
- **Find references** — every site that mentions a given command or arg.
- **Rename symbol** — coordinated rename of a command or arg across its declaration + every usage.
- **Formatting** — `perch fmt` exists as a roadmap CLI; once it lands, `textDocument/formatting` will wrap it.
- **Code actions** — quick-fixes for the common validator findings (e.g. "add missing `type` field").
- **Inlay hints** — show the resolved value of `${name}` placeholders.

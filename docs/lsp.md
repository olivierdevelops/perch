# Language server (`perch-lsp`)

`perch-lsp` is a Language Server Protocol implementation for `.perch` files. Drop it into your editor and you get:

- **Diagnostics** — every parse error and every `perch --check` finding shown inline as you type.
- **Completion** — context-aware suggestions: top-level keywords, command-config statements, arg-block fields, and the full op catalog inside `do` blocks. Command names declared in the current file are also suggested for `run`.
- **Hover** — point at a keyword or op and read its signature + docstring.
- **Outline** — every command (and its args) appear in the editor's symbol picker.

## Install

```sh
go install github.com/luowensheng/perch/cmd/perch-lsp@latest
```

Then make sure `perch-lsp` is on your `$PATH` (Go puts it in `$(go env GOBIN)` or `$(go env GOPATH)/bin`).

## VS Code (one command)

From a perch repo checkout:

```sh
./scripts/install-vscode.sh
```

The script:

1. installs `perch-lsp` via `go install`
2. installs node deps inside `editors/vscode-perch/`
3. packages the extension into a `.vsix`
4. runs `code --install-extension perch.vsix`

Open any `.perch` file — the LSP boots automatically. The extension is plain JS, so **no TypeScript build step**.

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

- **Go-to-definition** — `run foo` → jump to `command foo`. Requires source-position info from the capy parser.
- **Find references** — every site that mentions a given command or arg.
- **Rename symbol** — coordinated rename of a command or arg across its declaration + every usage.
- **Formatting** — `perch fmt` exists as a roadmap CLI; once it lands, `textDocument/formatting` will wrap it.
- **Code actions** — quick-fixes for the common validator findings (e.g. "add missing `type` field").
- **Inlay hints** — show the resolved value of `${name}` placeholders.

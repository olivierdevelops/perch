# tree-sitter-perch

Incremental parser for the [perch](https://github.com/olivierdevelops/perch) command-file DSL.

## Generate the parser

```sh
cd editors/tree-sitter-perch
npm install
npx tree-sitter generate
npx tree-sitter test       # once tests are added
```

## Use in Neovim (nvim-treesitter)

Until this is upstreamed:

```lua
require'nvim-treesitter.parsers'.get_parser_configs().perch = {
  install_info = {
    url = "https://github.com/olivierdevelops/perch",
    files = { "editors/tree-sitter-perch/src/parser.c" },
    branch = "main",
  },
  filetype = "perch",
}
```

Then `:TSInstall perch`.

## Use in Helix

Add a language entry to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "perch"
scope = "source.perch"
file-types = ["perch"]
roots = []
indent = { tab-width = 4, unit = "    " }
comment-token = "#"

[[grammar]]
name = "perch"
source = { git = "https://github.com/olivierdevelops/perch", subpath = "editors/tree-sitter-perch", rev = "main" }
```

## Roadmap

- Highlighting queries (`queries/highlights.scm`)
- Indent queries (`queries/indents.scm`)
- Folding queries (`queries/folds.scm`)
- Injections for embedded shell strings

# perch — VS Code extension

Syntax highlighting for `.capy` files written against the [perch](https://github.com/luowensheng/perch) command-runner DSL.

## Features

- Highlights `command`, `do`, `end`, `globals`, `catch`, `let`, `run`
- Highlights config modifiers (`description`, `arg`, `private`, `require_os`, …)
- Highlights block ops (`if_os`, `if_arch`, `if_exists`, `if_eq`, `if_gt`, …)
- Highlights the built-in op catalog
- `${name}` placeholders highlighted as variables inside strings
- `#`-line comments
- Smart indent on `command`/`do`/`if_*`/`catch` openers

## Install — from source

```sh
cd editors/vscode-perch
npm install -g @vscode/vsce
vsce package
code --install-extension perch-0.1.0.vsix
```

## Install — from the marketplace

Coming soon. The extension will be published to the VS Code Marketplace as `luowensheng.perch`.

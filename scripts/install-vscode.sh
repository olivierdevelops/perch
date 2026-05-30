#!/usr/bin/env sh
# One-shot installer for the perch VS Code extension.
#
# What it does:
#   1. installs `perch-lsp` via `go install` (so the extension can spawn it)
#   2. installs node deps inside editors/vscode-perch
#   3. packages the extension into a .vsix
#   4. installs the .vsix into VS Code via `code --install-extension`
#
# Prereqs: go, node + npm, the `code` CLI on PATH.

set -eu

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXT_DIR="$REPO_ROOT/editors/vscode-perch"

echo "→ Installing perch-lsp via go"
go install github.com/olivierdevelops/perch/cmd/perch-lsp@latest

cd "$EXT_DIR"

echo "→ Installing node dependencies"
npm install --silent --no-audit --no-fund

echo "→ Packaging extension"
npx --yes @vscode/vsce package --no-dependencies --skip-license -o perch.vsix

echo "→ Installing into VS Code"
if ! command -v code >/dev/null 2>&1; then
    echo "  ⚠  'code' CLI not found. Add it from VS Code:"
    echo "      Cmd-Shift-P → 'Shell Command: Install code command in PATH'"
    echo "  Then run:    code --install-extension $EXT_DIR/perch.vsix"
    exit 1
fi
code --install-extension perch.vsix --force

echo
echo "✓ Installed. Open a .perch file to activate the extension."
echo "  If perch-lsp isn't on your PATH, set perch.lsp.path in VS Code settings."

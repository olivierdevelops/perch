#!/usr/bin/env sh
# perch installer for macOS and Linux.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/scripts/install.sh | sh -s -- --version v0.1.0
#
# Honors:
#   PERCH_INSTALL_DIR    — install destination (default: /usr/local/bin or ~/.local/bin)
#   PERCH_VERSION        — version tag to install (default: latest)

set -eu

REPO="luowensheng/perch"
DEFAULT_DIR="/usr/local/bin"
FALLBACK_DIR="$HOME/.local/bin"

# ── parse args ─────────────────────────────────────────────────────────────
VERSION="${PERCH_VERSION:-latest}"
while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --version=*) VERSION="${1#*=}"; shift ;;
        --dir) PERCH_INSTALL_DIR="$2"; shift 2 ;;
        --dir=*) PERCH_INSTALL_DIR="${1#*=}"; shift ;;
        --help|-h)
            sed -n '2,12p' "$0" | sed 's/^# //; s/^#//'
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done

# ── detect OS / arch ───────────────────────────────────────────────────────
case "$(uname -s)" in
    Darwin) OS=darwin ;;
    Linux)  OS=linux ;;
    *) echo "Unsupported OS: $(uname -s). Build from source: go install github.com/$REPO/cmd/perch@latest" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64|amd64) ARCH=amd64 ;;
    arm64|aarch64) ARCH=arm64 ;;
    *) echo "Unsupported arch: $(uname -m)." >&2; exit 1 ;;
esac

# ── resolve version ────────────────────────────────────────────────────────
if [ "$VERSION" = "latest" ]; then
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
    if [ -z "$VERSION" ]; then
        echo "Could not resolve latest version. Set PERCH_VERSION explicitly." >&2
        exit 1
    fi
fi

# ── pick install dir ───────────────────────────────────────────────────────
if [ -n "${PERCH_INSTALL_DIR:-}" ]; then
    DEST_DIR="$PERCH_INSTALL_DIR"
elif [ -w "$DEFAULT_DIR" ]; then
    DEST_DIR="$DEFAULT_DIR"
elif command -v sudo >/dev/null 2>&1; then
    DEST_DIR="$DEFAULT_DIR"
    USE_SUDO=1
else
    DEST_DIR="$FALLBACK_DIR"
    mkdir -p "$DEST_DIR"
fi

# ── download ───────────────────────────────────────────────────────────────
BIN="perch-$OS-$ARCH"
URL="https://github.com/$REPO/releases/download/$VERSION/$BIN"
SUM_URL="$URL.sha256"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "→ downloading $BIN ($VERSION)"
curl -fsSL "$URL" -o "$TMPDIR/$BIN"
curl -fsSL "$SUM_URL" -o "$TMPDIR/$BIN.sha256" || true

if [ -s "$TMPDIR/$BIN.sha256" ]; then
    echo "→ verifying sha256"
    (cd "$TMPDIR" && shasum -a 256 -c "$BIN.sha256" >/dev/null) || {
        echo "checksum failed" >&2
        exit 1
    }
fi

chmod +x "$TMPDIR/$BIN"

# ── install ────────────────────────────────────────────────────────────────
if [ "${USE_SUDO:-0}" = "1" ]; then
    echo "→ installing to $DEST_DIR/perch (sudo)"
    sudo mv "$TMPDIR/$BIN" "$DEST_DIR/perch"
else
    echo "→ installing to $DEST_DIR/perch"
    mv "$TMPDIR/$BIN" "$DEST_DIR/perch"
fi

echo
echo "✓ installed $("$DEST_DIR/perch" --version 2>/dev/null || echo "(version check failed)")"
echo "  path: $DEST_DIR/perch"

case ":$PATH:" in
    *":$DEST_DIR:"*) ;;
    *) echo
       echo "  ⚠  $DEST_DIR is not on your PATH. Add this to your shell rc:"
       echo "      export PATH=\"$DEST_DIR:\$PATH\""
       ;;
esac

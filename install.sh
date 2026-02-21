#!/bin/sh
set -e

REPO="Stephen-Collins-tech/hotspots"
BIN_NAME="hotspots"
INSTALL_DIR="$HOME/.local/bin"

# ── Detect OS and arch ───────────────────────────────────────────────────────

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  os="linux" ;;
  Darwin) os="darwin" ;;
  *)
    echo "error: unsupported OS: $OS" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64)          arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *)
    echo "error: unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

ASSET="hotspots-${os}-${arch}.tar.gz"

# ── Resolve download URL ──────────────────────────────────────────────────────

if [ -n "$HOTSPOTS_VERSION" ]; then
  URL="https://github.com/${REPO}/releases/download/${HOTSPOTS_VERSION}/${ASSET}"
else
  URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
fi

# ── Download and install ──────────────────────────────────────────────────────

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $ASSET..."
curl --fail --silent --show-error --location "$URL" | tar -xz -C "$TMP"

mkdir -p "$INSTALL_DIR"
mv "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

echo "Installed to $INSTALL_DIR/$BIN_NAME"

# ── PATH check ───────────────────────────────────────────────────────────────

case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    ;;
  *)
    echo ""
    echo "Note: $INSTALL_DIR is not in your PATH."
    echo "Add this to your ~/.zshrc or ~/.bashrc:"
    echo ""
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    ;;
esac

echo "Done. Run: hotspots --version"

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

# ── Resolve latest version (HEAD request only — no body, no API key) ─────────
#
# GitHub redirects /releases/latest to /releases/tag/<version>.
# A HEAD request reveals the tag in the Location header without downloading
# anything or sending any identifying information.

latest_version() {
  curl --silent --head --location \
    "https://github.com/${REPO}/releases/latest" \
    | grep -i "^location:" \
    | grep -o 'tag/[^[:space:]]*' \
    | sed 's|tag/||' \
    | tr -d '\r'
}

# ── Check for existing install ────────────────────────────────────────────────

INSTALLED_VERSION=""
if command -v "$BIN_NAME" >/dev/null 2>&1; then
  INSTALLED_VERSION="$("$BIN_NAME" --version 2>/dev/null | grep -o 'v[0-9][^ ]*' || true)"
fi

# ── Resolve download URL ──────────────────────────────────────────────────────

if [ -n "$HOTSPOTS_VERSION" ]; then
  TARGET_VERSION="$HOTSPOTS_VERSION"
else
  echo "Checking latest version..."
  TARGET_VERSION="$(latest_version)"
  if [ -z "$TARGET_VERSION" ]; then
    echo "error: could not determine latest version" >&2
    exit 1
  fi
fi

# ── Version comparison ────────────────────────────────────────────────────────

if [ -n "$INSTALLED_VERSION" ]; then
  if [ "$INSTALLED_VERSION" = "$TARGET_VERSION" ]; then
    echo "hotspots $INSTALLED_VERSION is already up to date."
    exit 0
  fi
  echo "Update available: $INSTALLED_VERSION → $TARGET_VERSION"
  printf "Install update? [y/N] "
  read -r answer
  case "$answer" in
    [yY]*) ;;
    *) echo "Aborted."; exit 0 ;;
  esac
else
  echo "Installing hotspots $TARGET_VERSION..."
fi

URL="https://github.com/${REPO}/releases/download/${TARGET_VERSION}/${ASSET}"

# ── Download and install ──────────────────────────────────────────────────────

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $ASSET..."
curl --fail --silent --show-error --location "$URL" | tar -xz -C "$TMP"

mkdir -p "$INSTALL_DIR"
mv "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

echo "Installed $TARGET_VERSION to $INSTALL_DIR/$BIN_NAME"

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

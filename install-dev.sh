#!/usr/bin/env bash
# Build and install dev version of faultline to system PATH
# Usage: ./install-dev.sh [install-dir]
#
# By default, installs to ~/.local/bin (which should be in PATH)
# Or specify a custom directory: ./install-dev.sh /usr/local/bin

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"

# Default install directory
INSTALL_DIR="${1:-${HOME}/.local/bin}"

echo "Building faultline (release mode)..."

cd "$REPO_ROOT"

# Build release binary
cargo build --release --package faultline-cli

# Binary location after build
BUILT_BINARY="$REPO_ROOT/target/release/faultline"

if [ ! -f "$BUILT_BINARY" ]; then
    echo "Error: Binary not found at $BUILT_BINARY" >&2
    exit 1
fi

# Create install directory if it doesn't exist
mkdir -p "$INSTALL_DIR"

# Install binary
INSTALL_PATH="$INSTALL_DIR/faultline"
echo "Installing to $INSTALL_PATH..."

cp "$BUILT_BINARY" "$INSTALL_PATH"
chmod +x "$INSTALL_PATH"

# Check if install directory is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "⚠️  Warning: $INSTALL_DIR is not in your PATH"
    echo "Add this to your shell profile (~/.zshrc or ~/.bashrc):"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    echo ""
fi

# Verify installation
if command -v faultline >/dev/null 2>&1; then
    INSTALLED_VERSION=$(faultline --version 2>&1 || echo "unknown")
    echo "✅ Installation successful!"
    echo "   Binary: $INSTALL_PATH"
    echo "   Version: $INSTALLED_VERSION"
    echo ""
    echo "You can now use 'faultline' from any directory."
else
    echo "✅ Binary installed to $INSTALL_PATH"
    echo "   (May need to restart shell or add $INSTALL_DIR to PATH)"
fi

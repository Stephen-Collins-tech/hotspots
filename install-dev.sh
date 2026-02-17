#!/bin/bash
# Install hotspots to user's local bin directory
# Usage: ./install-dev.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Determine user's local bin directory
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    LOCAL_BIN="${HOME}/.local/bin"
else
    # Linux and others
    LOCAL_BIN="${HOME}/.local/bin"
fi

# Create local bin directory if it doesn't exist
mkdir -p "$LOCAL_BIN"

echo "Building hotspots..."
cargo build --release

if [ ! -f "target/release/hotspots" ]; then
    echo "Error: Build failed - binary not found"
    exit 1
fi

echo "Installing hotspots to ${LOCAL_BIN}..."

# Remove existing binary if it exists
if [ -f "${LOCAL_BIN}/hotspots" ]; then
    rm -f "${LOCAL_BIN}/hotspots"
fi

cp target/release/hotspots "${LOCAL_BIN}/hotspots"
chmod +x "${LOCAL_BIN}/hotspots"

# Check if LOCAL_BIN is in PATH
if [[ ":$PATH:" != *":${LOCAL_BIN}:"* ]]; then
    echo ""
    echo "⚠️  Warning: ${LOCAL_BIN} is not in your PATH"
    echo ""
    echo "Add this to your shell configuration (~/.zshrc, ~/.bashrc, etc.):"
    echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
    echo ""
    echo "Then run: source ~/.zshrc  (or ~/.bashrc)"
    echo ""
else
    echo "✓ ${LOCAL_BIN} is already in your PATH"
fi

echo ""
echo "✓ Installation complete!"
echo ""

# Try to verify installation
if command -v hotspots >/dev/null 2>&1; then
    INSTALLED_PATH=$(which hotspots)
    echo "✓ hotspots is available at: ${INSTALLED_PATH}"
    echo ""
    echo "Verify installation:"
    echo "  hotspots --version"
else
    echo "⚠️  Note: hotspots installed but not in current PATH"
    echo "   Location: ${LOCAL_BIN}/hotspots"
    echo "   You may need to restart your shell or run: source ~/.zshrc"
fi

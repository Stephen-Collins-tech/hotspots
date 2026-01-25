#!/bin/bash
# Install faultline to user's local bin directory
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

echo "Building faultline..."
cargo build --release

if [ ! -f "target/release/faultline" ]; then
    echo "Error: Build failed - binary not found"
    exit 1
fi

echo "Installing faultline to ${LOCAL_BIN}..."

# Remove existing binary if it exists
if [ -f "${LOCAL_BIN}/faultline" ]; then
    rm -f "${LOCAL_BIN}/faultline"
fi

cp target/release/faultline "${LOCAL_BIN}/faultline"
chmod +x "${LOCAL_BIN}/faultline"

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
if command -v faultline >/dev/null 2>&1; then
    INSTALLED_PATH=$(which faultline)
    echo "✓ faultline is available at: ${INSTALLED_PATH}"
    echo ""
    echo "Verify installation:"
    echo "  faultline --version"
else
    echo "⚠️  Note: faultline installed but not in current PATH"
    echo "   Location: ${LOCAL_BIN}/faultline"
    echo "   You may need to restart your shell or run: source ~/.zshrc"
fi

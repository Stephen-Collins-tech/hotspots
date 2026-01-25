#!/bin/bash
# Local development wrapper for faultline
# Uses `cargo run` instead of installing globally
# Usage: ./dev.sh [faultline arguments...]
#
# This script uses Cargo workspace features to find and run the faultline binary.
# It preserves the original working directory so relative paths work correctly.

set -e

# Get the directory where this script is located (project root)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Preserve the original working directory for path resolution
ORIGINAL_DIR="$(pwd)"

# Run cargo from the workspace root using --manifest-path, but execute from original directory
# This allows:
# 1. Cargo to find the workspace correctly (from project root)
# 2. The binary to execute with original CWD (for relative path resolution)
(cd "$ORIGINAL_DIR" && cargo run --manifest-path "$SCRIPT_DIR/Cargo.toml" --release --bin faultline -- "$@")

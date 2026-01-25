#!/bin/bash
# Run all tests for Faultline
# Usage: ./scripts/run-tests.sh [unit|comprehensive|all]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && cd .. && pwd)"
cd "$SCRIPT_DIR"

case "${1:-all}" in
    unit)
        echo "Running unit tests..."
        cargo test --lib
        ;;
    comprehensive)
        echo "Building release binary..."
        cargo build --release
        echo "Running comprehensive tests..."
        python3 test_comprehensive.py
        ;;
    all)
        echo "Running all tests..."
        echo "1. Unit tests..."
        cargo test --lib
        echo ""
        echo "2. Building release binary..."
        cargo build --release
        echo ""
        echo "3. Comprehensive integration tests..."
        python3 test_comprehensive.py
        ;;
    *)
        echo "Usage: $0 [unit|comprehensive|all]"
        echo "  unit          - Run unit tests only"
        echo "  comprehensive - Run comprehensive integration tests only"
        echo "  all           - Run all tests (default)"
        exit 1
        ;;
esac

echo ""
echo "✓ All tests completed successfully"

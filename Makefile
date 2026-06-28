.PHONY: test test-comprehensive test-integration test-all build install install-hooks setup clean fmt lint help

# First-time contributor setup: activate tracked git hooks
setup: install-hooks

# Build the release binary
build:
	cargo build --release

# Install to user's local bin directory
install: build
	./install-dev.sh

# Activate tracked git hooks in .githooks/ (run once per clone)
install-hooks:
	git config core.hooksPath .githooks
	@echo "Git hooks activated (.githooks/pre-commit)"

# Run unit tests
test:
	cargo test --lib

# Run comprehensive integration tests
test-comprehensive: build
	@if command -v pytest >/dev/null 2>&1; then \
		pytest -q integration; \
	else \
		python3 integration/legacy/test_comprehensive.py; \
	fi

# Run integration tests (pytest suite)
test-integration: build
	pytest -q integration

# Run all tests (unit + comprehensive)
test-all: test test-comprehensive

# Clean build artifacts and test directories
clean:
	cargo clean
	rm -rf test-repo-comprehensive

# Convenience targets
fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Help target
help:
	@echo "Available targets:"
	@echo "  setup              - First-time setup: activate git hooks"
	@echo "  build              - Build release binary"
	@echo "  install            - Build and install to ~/.local/bin"
	@echo "  install-hooks      - Activate git hooks from .githooks/ (run once per clone)"
	@echo "  test               - Run unit tests"
	@echo "  test-comprehensive - Run comprehensive integration tests"
	@echo "  test-all           - Run all tests"
	@echo "  test-integration   - Run pytest integration suite"
	@echo "  fmt                - Format code with rustfmt"
	@echo "  lint               - Lint with clippy (deny warnings)"
	@echo "  clean              - Clean build artifacts and test directories"

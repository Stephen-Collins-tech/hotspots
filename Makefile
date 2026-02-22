.PHONY: test test-comprehensive test-integration test-all build install install-hooks clean fmt lint help

# Build the release binary
build:
	cargo build --release

# Install to user's local bin directory
install: build
	./install-dev.sh

# Install git hooks
install-hooks:
	git config core.hooksPath .githooks

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
	@echo "  build              - Build release binary"
	@echo "  install            - Build and install to ~/.local/bin"
	@echo "  install-hooks      - Install git hooks (pre-commit)"
	@echo "  test               - Run unit tests"
	@echo "  test-comprehensive - Run comprehensive integration tests"
	@echo "  test-all           - Run all tests"
	@echo "  test-integration   - Run pytest integration suite"
	@echo "  fmt                - Format code with rustfmt"
	@echo "  lint               - Lint with clippy (deny warnings)"
	@echo "  clean              - Clean build artifacts and test directories"

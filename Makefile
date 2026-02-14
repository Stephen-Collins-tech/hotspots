.PHONY: test test-comprehensive build install install-hooks clean

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
	python3 test_comprehensive.py

# Run all tests (unit + comprehensive)
test-all: test test-comprehensive

# Clean build artifacts and test directories
clean:
	cargo clean
	rm -rf test-repo-comprehensive

# Help target
help:
	@echo "Available targets:"
	@echo "  build              - Build release binary"
	@echo "  install            - Build and install to ~/.local/bin"
	@echo "  install-hooks      - Install git hooks (pre-commit)"
	@echo "  test               - Run unit tests"
	@echo "  test-comprehensive - Run comprehensive integration tests"
	@echo "  test-all           - Run all tests"
	@echo "  clean              - Clean build artifacts and test directories"

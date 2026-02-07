.PHONY: help build test fmt fmt-check lint clean check all

# Default target
help:
	@echo "Navin Smart Contracts - Available Commands"
	@echo ""
	@echo "  make build        - Build all contracts"
	@echo "  make test         - Run all tests"
	@echo "  make fmt          - Format all code"
	@echo "  make fmt-check    - Check code formatting (for CI)"
	@echo "  make lint         - Run clippy lints"
	@echo "  make check        - Run format check and lint (for CI)"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make all          - Run checks and tests"
	@echo ""

# Build all contracts for wasm
build:
	@echo "Building contracts..."
	@cargo build --target wasm32-unknown-unknown --release


# Run all tests
test:
	@echo "Running tests..."
	@cargo test

# Format all code
fmt:
	@echo "Formatting code..."
	@cargo fmt --all
	@echo "Done formatting code..."

# Check code formatting (CI)
fmt-check:
	@echo "Checking code formatting..."
	@cargo fmt --all -- --check
	@echo "Done formatting & checking..."

# Run clippy lints
lint:
	@echo "Running clippy lints..."
	@cargo clippy --all-targets --all-features -- -D warnings

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	@cargo clean

# Run all checks (format + lint)
check: fmt-check lint
	@echo "✓ All checks passed!"

# Run all checks and tests
all: check test build
	@echo "✓ All tasks completed successfully!"

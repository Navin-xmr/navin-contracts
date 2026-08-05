
# Minimum line-coverage percentage the `coverage` target enforces.
# This is a starting baseline recorded from the first cargo-llvm-cov run —
# raise it as coverage improves so regressions become visible.
COVERAGE_BASELINE := 40

.PHONY: help build test fmt fmt-check lint clean check all generate-schema-shipment coverage

# Default target
help:
	@echo "Navin Smart Contracts - Available Commands"
	@echo ""
	@echo "  make generate-schema-shipment - Generate shipment contract ABI schema"
	@echo "  make build        - Build all contracts"
	@echo "  make test         - Run all tests"
	@echo "  make coverage     - Measure test coverage with cargo-llvm-cov"
	@echo "  make fmt          - Format all code"
	@echo "  make fmt-check    - Check code formatting (for CI)"
	@echo "  make lint         - Run clippy lints"
	@echo "  make check        - Run format check and lint (for CI)"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make all          - Run checks and tests"
	@echo ""

# Generate shipment contract ABI schema
generate-schema-shipment: build
	@echo "Generating shipment contract schema..."
	@stellar contract info interface \
		--wasm target/wasm32-unknown-unknown/release/shipment.wasm \
		--output json-formatted \
		> docs/contract-schema.shipment.json
	@echo "Schema written to docs/contract-schema.shipment.json"

# Build all contracts for wasm
build:
	@echo "Building contracts..."
	@cargo build --target wasm32-unknown-unknown --release


# Run all tests
test:
	@echo "Running tests..."
	@cargo test

# Measure test coverage with cargo-llvm-cov.
# Requires: cargo install cargo-llvm-cov (and the llvm-tools rustup component).
# Produces an lcov report at target/llvm-cov/lcov.info and an HTML report
# under target/llvm-cov/html/, and fails if line coverage drops below
# COVERAGE_BASELINE.
coverage:
	@echo "Measuring test coverage (cargo-llvm-cov)..."
	@cargo llvm-cov --workspace --no-report
	@mkdir -p target/llvm-cov
	@cargo llvm-cov report --lcov --output-path target/llvm-cov/lcov.info
	@cargo llvm-cov report --html --output-dir target/llvm-cov/html
	@cargo llvm-cov report --fail-under-lines $(COVERAGE_BASELINE)

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
	@cargo clippy --all-targets --all-features

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


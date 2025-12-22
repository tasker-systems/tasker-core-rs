# Tasker TypeScript Worker Build
#
# Builds both the Rust FFI library (cdylib) and TypeScript package.
# The FFI library is loaded by Bun/Node/Deno at runtime via dlopen.
#
# Note: Build output location depends on CARGO_TARGET_DIR environment variable.
# - If set: outputs to $(CARGO_TARGET_DIR)/release/
# - If unset: outputs to ../../target/release/ (default)
# See ~/bin/development_cache_init.sh for local cache configuration.

.PHONY: all build build-ffi build-ts clean test lint typecheck check install help

# Respect CARGO_TARGET_DIR if set, otherwise use default
TARGET_DIR ?= $(or $(CARGO_TARGET_DIR),../../target)

# Default target
all: build

# Full build: Rust FFI + TypeScript
build: build-ffi build-ts

# Build Rust FFI library (cdylib)
# Output: $(TARGET_DIR)/release/libtasker_worker.{dylib,so,dll}
build-ffi:
	@echo "🔨 Building Rust FFI library..."
	cargo build -p tasker-worker-ts --release
	@echo "✅ Rust FFI build complete"
	@ls -lh $(TARGET_DIR)/release/libtasker_worker.* 2>/dev/null || echo "ℹ️  Library at: $(TARGET_DIR)/release/"

# Build Rust FFI library (debug mode, faster compilation)
build-ffi-debug:
	@echo "🔨 Building Rust FFI library (debug)..."
	cargo build -p tasker-worker-ts
	@echo "✅ Rust FFI debug build complete"

# Build TypeScript package
build-ts: node_modules
	@echo "📦 Building TypeScript package..."
	bun run build
	@echo "✅ TypeScript build complete"

# Install dependencies
install: node_modules

node_modules: package.json bun.lock
	bun install --frozen-lockfile
	@touch node_modules

# Run all tests
test: node_modules
	bun test

# Run tests with coverage
test-coverage: node_modules
	bun test --coverage

# Run linting
lint: node_modules
	bun run lint

# Run type checking
typecheck: node_modules
	bun run typecheck

# Run all checks (lint + typecheck + test)
check: lint typecheck test

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	rm -rf dist/
	rm -rf node_modules/
	cargo clean -p tasker-worker-ts
	@echo "✅ Clean complete"

# Clean only TypeScript artifacts (faster)
clean-ts:
	rm -rf dist/

# Development: watch mode for TypeScript
watch: node_modules
	bun run build:watch

# Help
help:
	@echo "Tasker TypeScript Worker Build Targets:"
	@echo ""
	@echo "  make build       - Build Rust FFI + TypeScript (default)"
	@echo "  make build-ffi   - Build Rust FFI library only (release)"
	@echo "  make build-ts    - Build TypeScript package only"
	@echo "  make install     - Install npm dependencies"
	@echo "  make test        - Run tests"
	@echo "  make lint        - Run linter"
	@echo "  make typecheck   - Run TypeScript type checker"
	@echo "  make check       - Run lint + typecheck + test"
	@echo "  make clean       - Clean all build artifacts"
	@echo "  make clean-ts    - Clean TypeScript artifacts only"
	@echo "  make help        - Show this help"

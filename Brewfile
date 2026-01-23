# =============================================================================
# Tasker Core - Brewfile
# =============================================================================
#
# System-level dependencies for tasker-core development on macOS.
#
# Usage:
#   brew bundle                    # Install all dependencies
#   brew bundle check              # Check if all dependencies are installed
#   brew bundle cleanup            # Remove dependencies not listed here
#
# After running brew bundle, run bin/setup-dev.sh to complete setup.
#
# =============================================================================

# -----------------------------------------------------------------------------
# Taps (External Homebrew Repositories)
# -----------------------------------------------------------------------------

# -----------------------------------------------------------------------------
# Core Infrastructure
# -----------------------------------------------------------------------------

# PostgreSQL database (required for PGMQ and sqlx)
brew "postgresql@18"

brew "docker"
brew "docker-compose"

# Container runtime (aliased to docker commands)
brew "podman"

# Podman Compose for docker-compose compatibility
brew "podman-compose"

# -----------------------------------------------------------------------------
# Rust Tooling
# -----------------------------------------------------------------------------

# Rust is installed via rustup (curl -sSf https://sh.rustup.rs | sh)
# The following are system dependencies for Rust compilation

# OpenSSL for cryptographic operations
brew "openssl@3"

# PostgreSQL client libraries (for sqlx native TLS)
brew "libpq"

# LLVM for coverage tools (cargo-llvm-cov)
brew "llvm"

# -----------------------------------------------------------------------------
# Python Tooling
# -----------------------------------------------------------------------------

# uv - Fast Python package and project manager (provides Python too)
brew "uv"

# ruff - Fast Python linter and formatter (also available via uv)
brew "ruff"

# mypy - Static type checker for Python
brew "mypy"

# -----------------------------------------------------------------------------
# Ruby Tooling
# -----------------------------------------------------------------------------

# chruby - Ruby version manager (lightweight, works well with ruby-install)
brew "chruby"

# ruby-install - Installs Ruby versions for use with chruby
brew "ruby-install"

# Note: After installation, add to your shell config:
#   source $(brew --prefix)/opt/chruby/share/chruby/chruby.sh
#   source $(brew --prefix)/opt/chruby/share/chruby/auto.sh
#
# Then install Ruby 3.4.4:
#   ruby-install ruby 3.4.4

# -----------------------------------------------------------------------------
# TypeScript/JavaScript Runtimes
# -----------------------------------------------------------------------------

# Bun - Fast JavaScript runtime (primary runtime for TypeScript worker)
brew "oven-sh/bun/bun"

# Node.js - Required for Node.js FFI support
brew "node"

# Deno - Required for Deno FFI support
brew "deno"

# -----------------------------------------------------------------------------
# Development Utilities
# -----------------------------------------------------------------------------

# jq - JSON processor (useful for parsing API responses)
brew "jq"

# yq - YAML processor (useful for config file manipulation)
brew "yq"

# watch - Execute a program periodically
brew "watch"

# bottom - process monitoring
brew "bottom"

# -----------------------------------------------------------------------------
# Performance & Profiling Tools (TAS-71)
# -----------------------------------------------------------------------------
#
# macOS Profiling Strategy:
#   - samply: CPU sampling profiler with Firefox Profiler UI (cargo install)
#   - cargo-flamegraph: Flamegraph generation via dtrace (cargo install)
#   - tokio-console: Async runtime introspection (cargo install, requires code changes)
#   - dhat-rs: Memory profiling (Cargo dev-dependency)
#
# Linux CI Note:
#   Linux would use perf-based tools (cargo-flamegraph, heaptrack).
#   Benchmarking and profiling are LOCAL ONLY for this project due to
#   GHA runner constraints. See docs/ticket-specs/TAS-71/profiling-tool-evaluation.md
#
# Usage:
#   cargo build --profile profiling    # Build with debug symbols
#   samply record ./target/profiling/binary
#   cargo flamegraph --profile profiling --bin binary
#
# Note: samply requires one-time setup: `samply setup` (self-signs binary)

# hyperfine - Command-line benchmarking tool
brew "hyperfine"


# -----------------------------------------------------------------------------
# Optional: GUI Applications (Casks)
# -----------------------------------------------------------------------------

# Uncomment if you want GUI applications installed via Homebrew

# cask "visual-studio-code"
cask "docker-desktop"
cask "tableplus"            # PostgreSQL GUI client
# cask "postico"              # Alternative PostgreSQL GUI

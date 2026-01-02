# =============================================================================
# Rust Worker Service - Test Dockerfile (LOCAL OPTIMIZED)
# =============================================================================
# Fast local testing build with optimizations:
# - cargo-binstall for fast binary installation (cargo-chef)
# - Pre-built binaries where available (5-10 min savings)
# - BuildKit cache mount support for incremental builds
# - Single-layer binary copy pattern
# Context: workers/rust/ directory
# Usage: DOCKER_BUILDKIT=1 docker build -f Dockerfile.test-local -t tasker-worker-rust:test .

# Verify BuildKit is enabled (will fail if not)
ARG DOCKER_BUILDKIT=1

FROM rust:1.90-bullseye AS chef

# Version pinning for reproducible builds
ARG CARGO_BINSTALL_VERSION=1.10.17

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    build-essential \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# OPTIMIZATION 1: Install cargo-binstall for fast binary installations
# Detect architecture and download appropriate binary
RUN ARCH=$(uname -m) && \
    if [ "$ARCH" = "aarch64" ]; then \
        BINSTALL_ARCH="aarch64-unknown-linux-musl"; \
    else \
        BINSTALL_ARCH="x86_64-unknown-linux-musl"; \
    fi && \
    curl -L --proto '=https' --tlsv1.2 -sSf \
    https://github.com/cargo-bins/cargo-binstall/releases/download/v${CARGO_BINSTALL_VERSION}/cargo-binstall-${BINSTALL_ARCH}.tgz \
    | tar xz -C /usr/local/bin

# OPTIMIZATION 2: Install cargo-chef using cargo-binstall
# Saves 5-10 minutes vs compiling from source
# Use sharing=locked to prevent concurrent access issues
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo binstall cargo-chef --no-confirm --no-symlinks

WORKDIR /app

# =============================================================================
# Planner - Generate recipe for dependency caching
# =============================================================================
FROM chef AS planner

# Copy workspace root files
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
COPY src/ ./src/

# Copy workspace crates needed by rust worker
COPY tasker-shared/ ./tasker-shared/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY tasker-worker/ ./tasker-worker/
COPY workers/rust/ ./workers/rust/
COPY migrations/ ./migrations/

# Copy minimal workspace structure for crates we don't actually need
# Uses shared stub script to reduce maintenance burden
COPY docker/scripts/create-workspace-stubs.sh /tmp/
RUN chmod +x /tmp/create-workspace-stubs.sh && \
    /tmp/create-workspace-stubs.sh tasker-orchestration workers/ruby workers/python workers/typescript
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/
COPY workers/python/Cargo.toml ./workers/python/
COPY workers/typescript/Cargo.toml ./workers/typescript/

# Generate dependency recipe
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Builder - Build dependencies and application
# =============================================================================
FROM chef AS builder

WORKDIR /app

# Copy recipe and build dependencies with cache mounts
COPY --from=planner /app/recipe.json recipe.json

# OPTIMIZATION 3: Build dependencies with cache mounts
# Note: We don't use --package here as cargo-chef works better with full workspace
# Use sharing=locked to prevent concurrent access issues
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo chef cook --recipe-path recipe.json

# Copy workspace root files and all source
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
COPY src/ ./src/

# Copy workspace crates needed by rust worker
COPY tasker-shared/ ./tasker-shared/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY tasker-worker/ ./tasker-worker/
COPY workers/rust/ ./workers/rust/
COPY migrations/ ./migrations/

# Copy minimal workspace structure for crates we don't actually need
COPY docker/scripts/create-workspace-stubs.sh /tmp/
RUN chmod +x /tmp/create-workspace-stubs.sh && \
    /tmp/create-workspace-stubs.sh tasker-orchestration workers/ruby workers/python workers/typescript
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/
COPY workers/python/Cargo.toml ./workers/python/
COPY workers/typescript/Cargo.toml ./workers/typescript/

# Set offline mode for SQLx
ENV SQLX_OFFLINE=true

# OPTIMIZATION 4: Build with cache mounts and copy binary in single layer
# This avoids the inefficient double-copy pattern
# Use sharing=locked to prevent concurrent access issues
# IMPORTANT: Use --locked to ensure Cargo.lock is respected (prevents serde version conflicts)
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --all-features --locked --bin rust-worker -p tasker-worker-rust && \
    cp /app/target/debug/rust-worker /app/rust-worker

# =============================================================================
# Runtime - Minimal runtime image
# =============================================================================
FROM debian:bullseye-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl1.1 \
    libpq5 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -g daemon -u 999 tasker

# Copy binary from builder
COPY --from=builder /app/rust-worker ./

# Create scripts directory and copy worker entrypoint script
RUN mkdir -p ./scripts
COPY docker/scripts/worker-entrypoint.sh ./scripts/worker-entrypoint.sh

# Make scripts executable before switching to non-root user
RUN chmod +x ./scripts/*.sh

# Set environment variables for the service
ENV APP_NAME=tasker-worker-rust

# Environment variables will be set by docker-compose

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

USER tasker

EXPOSE 8081

# Use worker-specific entrypoint
ENTRYPOINT ["./scripts/worker-entrypoint.sh"]
CMD ["./rust-worker"]

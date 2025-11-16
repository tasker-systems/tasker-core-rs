# =============================================================================
# Rust Worker Service - Test Dockerfile (LOCAL OPTIMIZED)
# =============================================================================
# Fast local testing build with optimizations:
# - Pre-built binaries for cargo-chef (5-10 min savings)
# - BuildKit cache mount support for incremental builds
# - Single-layer binary copy pattern
# Context: workers/rust/ directory
# Usage: DOCKER_BUILDKIT=1 docker build -f Dockerfile.test-local -t tasker-worker-rust:test .

# Verify BuildKit is enabled (will fail if not)
ARG DOCKER_BUILDKIT=1

FROM rust:1.90-bullseye AS chef

# Version pinning for reproducible builds
ARG CARGO_CHEF_VERSION=0.1.68

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    build-essential \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# OPTIMIZATION 1: Install pre-built cargo-chef binary
# Saves 5-10 minutes vs compiling from source
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://github.com/LukeMathWalker/cargo-chef/releases/download/v${CARGO_CHEF_VERSION}/cargo-chef-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin

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
RUN mkdir -p tasker-orchestration/src && \
    echo "pub fn stub() {}" > tasker-orchestration/src/lib.rs
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/

RUN mkdir -p workers/ruby/ext/tasker_core/src && \
    echo "pub fn stub() {}" > workers/ruby/ext/tasker_core/src/lib.rs
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/

# Generate dependency recipe
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Builder - Build dependencies and application
# =============================================================================
FROM chef AS builder

WORKDIR /app

# Copy recipe and build dependencies with cache mounts
COPY --from=planner /app/recipe.json recipe.json

# OPTIMIZATION 2: Build dependencies with cache mounts
# Note: We don't use --package here as cargo-chef works better with full workspace
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
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
RUN mkdir -p tasker-orchestration/src && \
    echo "pub fn stub() {}" > tasker-orchestration/src/lib.rs
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/

RUN mkdir -p workers/ruby/ext/tasker_core/src && \
    echo "pub fn stub() {}" > workers/ruby/ext/tasker_core/src/lib.rs
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/

# Set offline mode for SQLx
ENV SQLX_OFFLINE=true

# OPTIMIZATION 3: Build with cache mounts and copy binary in single layer
# This avoids the inefficient double-copy pattern
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --all-features --bin rust-worker -p tasker-worker-rust && \
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

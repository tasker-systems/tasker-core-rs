# =============================================================================
# Orchestration Service - Test Dockerfile (LOCAL OPTIMIZED)
# =============================================================================
# Fast local testing build with optimizations:
# - Pre-built binaries for cargo-chef (5-10 min savings)
# - cargo-binstall for sqlx-cli installation (3-5 min savings)
# - Optimized cargo chef cook with targeted packages
# - BuildKit cache mount support for incremental builds
# Context: tasker-orchestration/ directory
# Usage: DOCKER_BUILDKIT=1 docker build -f Dockerfile.test-local -t tasker-orchestration:test .

# Verify BuildKit is enabled (will fail if not)
ARG DOCKER_BUILDKIT=1

FROM rust:1.90-bullseye AS chef

# Version pinning for reproducible builds
ARG CARGO_CHEF_VERSION=0.1.68
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

# OPTIMIZATION 1: Install pre-built cargo-chef binary
# Saves 5-10 minutes vs compiling from source
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://github.com/LukeMathWalker/cargo-chef/releases/download/v${CARGO_CHEF_VERSION}/cargo-chef-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin

# OPTIMIZATION 2: Install cargo-binstall for fast binary installations
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://github.com/cargo-bins/cargo-binstall/releases/download/v${CARGO_BINSTALL_VERSION}/cargo-binstall-x86_64-unknown-linux-musl.tgz \
    | tar xz -C /usr/local/bin

# OPTIMIZATION 3: Install sqlx-cli using cargo-binstall (much faster than compiling)
# This tries to download a pre-built binary first, falls back to building if needed
# Note: cargo-binstall doesn't support --features flag, it will install the full version
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo binstall sqlx-cli --no-confirm --no-symlinks

WORKDIR /app

# =============================================================================
# Planner - Generate recipe for dependency caching
# =============================================================================
FROM chef AS planner

# Copy workspace root files
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
COPY src/ ./src/

# Copy workspace crates needed by orchestration
# Note: Each package contains its own .sqlx/ directory with query metadata
COPY tasker-orchestration/ ./tasker-orchestration/
COPY tasker-shared/ ./tasker-shared/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY migrations/ ./migrations/

# Copy minimal workspace structure for crates we don't actually need
# Cargo validates ALL workspace members even if unused, so we need their Cargo.toml files
RUN mkdir -p tasker-worker/src && \
    echo "pub fn stub() {}" > tasker-worker/src/lib.rs
COPY tasker-worker/Cargo.toml ./tasker-worker/

RUN mkdir -p workers/ruby/ext/tasker_core/src && \
    echo "pub fn stub() {}" > workers/ruby/ext/tasker_core/src/lib.rs
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/

RUN mkdir -p workers/rust/src && \
    echo "pub fn stub() {}" > workers/rust/src/lib.rs
COPY workers/rust/Cargo.toml ./workers/rust/

# Generate dependency recipe
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Builder - Build dependencies and application
# =============================================================================
FROM chef AS builder

# Copy recipe and build dependencies with cache mounts
COPY --from=planner /app/recipe.json recipe.json

# OPTIMIZATION 4: Build dependencies with cache mounts
# Note: We don't use --package here as cargo-chef works better with full workspace
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo chef cook --recipe-path recipe.json

# Copy workspace root files and all source
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
COPY src/ ./src/

# Copy workspace crates needed by orchestration
# Note: Each package contains its own .sqlx/ directory with query metadata
COPY tasker-orchestration/ ./tasker-orchestration/
COPY tasker-shared/ ./tasker-shared/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY migrations/ ./migrations/

# Copy minimal workspace structure for crates we don't actually need
RUN mkdir -p tasker-worker/src && \
    echo "pub fn stub() {}" > tasker-worker/src/lib.rs
COPY tasker-worker/Cargo.toml ./tasker-worker/

RUN mkdir -p workers/ruby/ext/tasker_core/src && \
    echo "pub fn stub() {}" > workers/ruby/ext/tasker_core/src/lib.rs
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/

RUN mkdir -p workers/rust/src && \
    echo "pub fn stub() {}" > workers/rust/src/lib.rs
COPY workers/rust/Cargo.toml ./workers/rust/

# Set offline mode for SQLx
ENV SQLX_OFFLINE=true

# OPTIMIZATION 5: Build with cache mounts and copy binary in single layer
# This avoids the inefficient double-copy pattern
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --all-features --bin tasker-server -p tasker-orchestration && \
    cp /app/target/debug/tasker-server /app/tasker-server

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
    bash \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -g daemon -u 999 tasker

# Copy binary from builder
COPY --from=builder /app/tasker-server ./tasker-orchestration

# Copy SQLx CLI from builder
COPY --from=builder /usr/local/bin/sqlx /usr/local/bin/sqlx

# Copy migration scripts and migrations
COPY docker/scripts/ ./scripts/
COPY migrations/ ./migrations/

# Make scripts executable before switching to non-root user
RUN chmod +x ./scripts/*.sh

# Set environment variables for the service
ENV APP_NAME=tasker-orchestration

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

USER tasker

EXPOSE 8080

# Use orchestration-specific entrypoint that handles migrations
ENTRYPOINT ["./scripts/orchestration-entrypoint.sh"]
CMD ["./tasker-orchestration"]

# =============================================================================
# Common Rust Builder Base for Tasker Core
# =============================================================================
# This provides a shared builder foundation with cargo-chef optimization
# and sccache support for all Rust components in the Tasker ecosystem.
#
# Components using this base:
# - tasker-orchestration
# - tasker-worker (foundation)
# - workers/rust (comprehensive)
#
# Build optimization features:
# - cargo-chef for Docker layer caching
# - sccache for distributed compilation caching in CI
# - Rust 1.89 for latest performance and features

# =============================================================================
# Stage 1: Chef (cargo-chef installation)
# =============================================================================
FROM rust:1.89-bullseye AS chef
WORKDIR /app

# Install cargo-chef for build optimization
RUN cargo install cargo-chef

# Install system dependencies needed for Rust compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# =============================================================================
# Stage 2: Planner (cargo-chef recipe generation)
# =============================================================================
FROM chef AS planner

# Copy workspace configuration files
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/

# Copy all source code for dependency analysis
COPY tasker-shared/ ./tasker-shared/
COPY tasker-worker/ ./tasker-worker/
COPY tasker-orchestration/ ./tasker-orchestration/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY workers/ ./workers/

# Generate cargo-chef recipe for dependency caching
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 3: Builder (optimized dependency and source compilation)
# =============================================================================
FROM chef AS builder

# Copy recipe from planner stage
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies first (this layer will be cached if dependencies don't change)
# This significantly speeds up rebuilds when only source code changes
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code (this happens after dependency build for optimal caching)
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
COPY tasker-shared/ ./tasker-shared/
COPY tasker-worker/ ./tasker-worker/
COPY tasker-orchestration/ ./tasker-orchestration/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY workers/ ./workers/

# =============================================================================
# Common environment variables for all builds
# =============================================================================
ENV RUST_LOG=info
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_integration
ENV TASKER_ENV=docker
ENV WORKSPACE_PATH=/app
ENV TASKER_CONFIG_ROOT=/app/config

# =============================================================================
# Notes for sccache CI integration:
# =============================================================================
# To enable sccache in GitHub Actions, add these environment variables
# to the builder stage in specific Dockerfiles:
#
# ENV RUSTC_WRAPPER=sccache
# ENV SCCACHE_GHA_ENABLED=true
# ENV SCCACHE_CACHE_SIZE=2G
#
# And use the Mozilla sccache action in your workflow:
# - name: Run sccache-cache
#   uses: mozilla-actions/sccache-action@v0.0.4

# This stage provides the compiled dependencies and build environment.
# Specific Dockerfiles should copy binaries from this stage and create
# their own runtime stages with minimal dependencies.
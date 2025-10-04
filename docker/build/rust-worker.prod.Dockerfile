# =============================================================================
# Rust Worker Service - Production Dockerfile
# =============================================================================
# Optimized for production deployment with minimal size and maximum security
# Context: workers/rust/ directory
# Usage: docker build -f Dockerfile.prod -t tasker-worker-rust:prod .

FROM rust:1.90-bullseye AS chef

# Install cargo-chef for dependency layer caching
RUN cargo install cargo-chef

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# =============================================================================
# Planner - Generate recipe for dependency caching
# =============================================================================
FROM chef AS planner

WORKDIR /app

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
COPY .sqlx/ ./.sqlx/

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

# Copy recipe and build dependencies (cached layer)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

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
COPY .sqlx/ ./.sqlx/

# Copy minimal workspace structure for crates we don't actually need
RUN mkdir -p tasker-orchestration/src && \
    echo "pub fn stub() {}" > tasker-orchestration/src/lib.rs
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/

RUN mkdir -p workers/ruby/ext/tasker_core/src && \
    echo "pub fn stub() {}" > workers/ruby/ext/tasker_core/src/lib.rs
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/

# Set offline mode for SQLx
ENV SQLX_OFFLINE=true

# Build optimized release binary
RUN cargo build --release --all-features --bin rust-worker -p tasker-worker-rust

# Strip binary for minimal size
RUN strip target/release/rust-worker

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
COPY --from=builder /app/target/release/rust-worker ./

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

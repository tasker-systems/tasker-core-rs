# =============================================================================
# Orchestration Service - Production Dockerfile
# =============================================================================
# Optimized for production deployment with minimal size and maximum security
# Context: tasker-orchestration/ directory
# Usage: docker build -f Dockerfile.prod -t tasker-orchestration:prod .

FROM rust:1.89-bullseye AS chef

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

# Copy all workspace member crates
COPY tasker-orchestration/ ./tasker-orchestration/
COPY tasker-shared/ ./tasker-shared/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY tasker-worker/ ./tasker-worker/
COPY workers/ ./workers/
COPY migrations/ ./migrations/
COPY .sqlx/ ./.sqlx/

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

# Copy all workspace member crates
COPY tasker-orchestration/ ./tasker-orchestration/
COPY tasker-shared/ ./tasker-shared/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY tasker-worker/ ./tasker-worker/
COPY workers/ ./workers/
COPY migrations/ ./migrations/
COPY .sqlx/ ./.sqlx/

# Set offline mode for SQLx
ENV SQLX_OFFLINE=true

# Build optimized release binary
RUN cargo build --release --all-features --bin tasker-server -p tasker-orchestration

# Strip binary for minimal size
RUN strip target/release/tasker-server

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

WORKDIR /app

# Copy binary from builder (workspace target directory)
COPY --from=builder /app/target/release/tasker-server ./tasker-orchestration

# Environment variables will be set by docker-compose

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

USER tasker

EXPOSE 8080

CMD ["./tasker-orchestration"]

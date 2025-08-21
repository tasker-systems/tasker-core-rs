# Tasker Server Dockerfile
#
# Multi-stage build for efficient caching and minimal runtime image.
# Builds the tasker-server binary with web-api features for standalone deployment.

# Build stage
FROM rust:1.80-slim AS builder

# Install system dependencies needed for compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy dependency manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo

# Create a dummy src/main.rs to build dependencies (cache optimization)
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached if Cargo.toml doesn't change)
RUN cargo build --release --bin tasker-server --features web-api
RUN rm src/main.rs

# Copy source code
COPY src/ src/
COPY bindings/ bindings/
COPY config/ config/
COPY migrations/ migrations/

# Build the actual application
RUN cargo build --release --bin tasker-server --features web-api

# Runtime stage - minimal image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 tasker

# Create directories with proper permissions
RUN mkdir -p /app/config /app/log && \
    chown -R tasker:tasker /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/tasker-server /app/tasker-server

# Copy configuration (can be overridden with volumes)
COPY --from=builder /app/config /app/config

# Set ownership
RUN chown -R tasker:tasker /app

# Switch to non-root user
USER tasker

# Set working directory
WORKDIR /app

# Environment variables with defaults
ENV TASKER_ENV=production
ENV TASKER_PROJECT_ROOT=/app
ENV WORKSPACE_PATH=/app
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_production

# Health check endpoint
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Expose web API port (default from web.toml configuration)
EXPOSE 8080

# Run the server
CMD ["./tasker-server"]

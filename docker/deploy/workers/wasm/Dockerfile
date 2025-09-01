# =============================================================================
# Production-Optimized Tasker WASM Worker
# =============================================================================
# This creates a WASM worker that integrates with the Rust core via WASI:
# - WASM-based step handlers with Rust core coordination
# - Full integration with tasker-core via WASI runtime
# - Namespace-specific worker for WASM-implemented workflows
# - Wasmtime runtime for high-performance WASM execution
#
# Production optimizations:
# - Wasmtime runtime for optimal WASM performance
# - Minimal runtime footprint and security hardening
# - Sandboxed execution environment for enhanced security
#
# Built for production deployment with WASM ecosystem integration

# =============================================================================
# Stage 1: WASM Builder with Rust
# =============================================================================
FROM rust:1.89-bullseye AS wasm-builder

# Install system dependencies needed for WASM compilation
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Install WASM target and tools
RUN rustup target add wasm32-wasi
RUN cargo install wasm-pack

# Install wasmtime for runtime execution
RUN curl -sSL https://github.com/bytecodealliance/wasmtime/releases/download/v13.0.0/wasmtime-v13.0.0-x86_64-linux.tar.xz | tar -xJ && \
    cp wasmtime-v13.0.0-x86_64-linux/wasmtime /usr/local/bin/ && \
    rm -rf wasmtime-v13.0.0-x86_64-linux

WORKDIR /app

# Copy WASM worker source
COPY workers/wasm/ ./workers/wasm/

# Copy Rust source needed for WASM compilation
COPY tasker-shared/ ./tasker-shared/
COPY pgmq-notify/ ./pgmq-notify/
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/

# Build WASM modules and runtime
WORKDIR /app/workers/wasm

# Build the WASM step handlers
RUN wasm-pack build --target wasm32-wasi --release step-handlers/

# Build the WASM runtime coordinator
RUN cargo build --release --target wasm32-wasi --bin wasm-worker

# Verify WASM modules were built
RUN ls -la target/wasm32-wasi/release/

# =============================================================================
# Stage 2: Production WASM Runtime
# =============================================================================
FROM debian:bullseye-slim AS runtime

# Install only essential runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get purge -y --auto-remove

# Install wasmtime runtime
COPY --from=wasm-builder /usr/local/bin/wasmtime /usr/local/bin/wasmtime

# Create non-root user for security
RUN useradd -r -u 1001 -m -s /bin/false tasker

# Create application directories with minimal permissions
RUN mkdir -p /app/config /app/logs /app/workers/wasm /app/wasm-modules \
    && chown -R tasker:tasker /app \
    && chmod 755 /app \
    && chmod 750 /app/config /app/logs

# Copy WASM application and modules from builder stage
COPY --from=wasm-builder --chown=tasker:tasker /app/workers/wasm/target/wasm32-wasi/release/*.wasm /app/wasm-modules/
COPY --from=wasm-builder --chown=tasker:tasker /app/workers/wasm/runtime/ /app/workers/wasm/runtime/

# Copy configuration files
COPY --chown=tasker:tasker config/ /app/config/

# Create WASM worker startup script
COPY --chown=tasker:tasker <<'EOF' /app/workers/wasm/start-wasm-worker.sh
#!/bin/bash
set -e

echo "Starting Tasker WASM Worker..."
echo "WASM Modules: $(ls -1 /app/wasm-modules/)"
echo "Worker ID: $WORKER_ID"
echo "Supported Namespaces: $SUPPORTED_NAMESPACES"

# Start the WASM worker runtime
exec wasmtime run \
    --allow-unknown-exports \
    --wasm-features=all \
    --wasi-modules=wasi_snapshot_preview1 \
    --dir /app/config \
    --dir /app/logs \
    --env DATABASE_URL="$DATABASE_URL" \
    --env TASKER_ENV="$TASKER_ENV" \
    --env WORKER_ID="$WORKER_ID" \
    --env SUPPORTED_NAMESPACES="$SUPPORTED_NAMESPACES" \
    /app/wasm-modules/wasm-worker.wasm
EOF

RUN chmod +x /app/workers/wasm/start-wasm-worker.sh

# Switch to non-root user
USER tasker
WORKDIR /app/workers/wasm

# Production environment configuration
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_production
ENV TASKER_ENV=production
ENV WORKSPACE_PATH=/app
ENV TASKER_CONFIG_ROOT=/app/config

# WASM worker identification
ENV WORKER_ID=wasm-worker-production
ENV WORKER_TYPE=wasm-wasi
ENV WORKER_LANGUAGE=wasm

# Namespace configuration - WASM worker handles sandboxed workflows
ENV SUPPORTED_NAMESPACES=wasm_workflows,sandboxed_execution,secure_computation

# WASM runtime configuration
ENV WASM_RUNTIME=wasmtime
ENV WASM_FEATURES=all
ENV WASM_SECURITY_SANDBOX=true

# Orchestration client configuration
ENV ORCHESTRATION_CLIENT_ENABLED=true
ENV ORCHESTRATION_API_URL=http://orchestration:8080

# PGMQ configuration for WASM worker
ENV PGMQ_ENABLED=true
ENV PGMQ_POLL_INTERVAL_MS=750
ENV PGMQ_MAX_MESSAGES_PER_POLL=15

# Resource limits optimized for WASM
ENV MAX_CONCURRENT_TASKS=10
ENV DATABASE_POOL_SIZE=5
ENV DATABASE_CONNECTION_TIMEOUT_SECONDS=30

# Health monitoring (simplified for WASM)
ENV HEALTH_CHECK_INTERVAL_SECONDS=20
ENV HEALTH_CHECK_TIMEOUT_SECONDS=10

# WASM-specific health check using wasmtime
HEALTHCHECK --interval=20s --timeout=10s --start-period=60s --retries=3 \
    CMD wasmtime --version && ps aux | grep wasm-worker.wasm || exit 1

# Production signal handling
STOPSIGNAL SIGTERM

# Run the WASM worker via wasmtime runtime
CMD ["./start-wasm-worker.sh"]
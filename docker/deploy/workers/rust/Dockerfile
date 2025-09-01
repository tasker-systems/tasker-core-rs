# =============================================================================
# Production-Optimized Tasker Rust Worker
# =============================================================================
# This creates a comprehensive Rust worker that handles ALL namespaces:
# - linear_workflow, diamond_workflow, tree_workflow, mixed_dag_workflow
# - order_fulfillment, inventory, notifications, payments
#
# Production optimizations:
# - Full compiler optimizations for maximum performance
# - Minimal runtime footprint and security hardening
# - Comprehensive health monitoring and metrics collection
# - Auto-scaling and resource management
#
# Built for production deployment with high performance requirements

# =============================================================================
# Stage 1: Build from common base
# =============================================================================
FROM jcoletaylor/tasker-builder-base AS rust-worker-builder

# Build the comprehensive Rust worker with all optimizations
# - Release mode for maximum performance
# - All features enabled for comprehensive step handler support
# - Link-time optimization for smaller binaries
RUN cargo build --release --package tasker-worker-rust --all-features

# Verify the binary was created and is optimized
RUN ls -la target/release/tasker-worker-rust && \
    strip target/release/tasker-worker-rust

# =============================================================================
# Stage 2: Minimal runtime (production hardened)
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

# Create non-root user for security
RUN useradd -r -u 1001 -m -s /bin/false tasker

# Create application directories with minimal permissions
RUN mkdir -p /app/config /app/logs \
    && chown -R tasker:tasker /app \
    && chmod 755 /app \
    && chmod 750 /app/config /app/logs

# Copy configuration files
COPY --chown=tasker:tasker config/ /app/config/

# Copy optimized Rust worker binary from builder stage
COPY --from=rust-worker-builder --chown=tasker:tasker /app/target/release/tasker-worker-rust /app/tasker-worker-rust

# Switch to non-root user
USER tasker
WORKDIR /app

# Production environment configuration
ENV RUST_LOG=info
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_production
ENV TASKER_ENV=production
ENV WORKSPACE_PATH=/app
ENV TASKER_CONFIG_ROOT=/app/config

# Rust worker identification
ENV WORKER_ID=rust-worker-production
ENV WORKER_TYPE=comprehensive

# Namespace configuration - this Rust worker handles ALL namespaces
ENV SUPPORTED_NAMESPACES=linear_workflow,diamond_workflow,tree_workflow,mixed_dag_workflow,order_fulfillment,inventory,notifications,payments

# Web API configuration for production monitoring
ENV WEB_API_ENABLED=true
ENV WEB_API_BIND_ADDRESS=0.0.0.0:8081
ENV WEB_API_REQUEST_TIMEOUT_MS=30000

# Orchestration client configuration for production
ENV ORCHESTRATION_CLIENT_ENABLED=true
ENV ORCHESTRATION_API_URL=http://orchestration:8080

# PGMQ configuration optimized for production load
ENV PGMQ_ENABLED=true
ENV PGMQ_POLL_INTERVAL_MS=250
ENV PGMQ_MAX_MESSAGES_PER_POLL=50

# Event system configuration for high throughput
ENV EVENT_SYSTEM_ENABLED=true
ENV EVENT_SYSTEM_BATCH_SIZE=100

# Resource limits optimized for production
ENV MAX_CONCURRENT_TASKS=50
ENV DATABASE_POOL_SIZE=15
ENV DATABASE_CONNECTION_TIMEOUT_SECONDS=30

# Health monitoring configuration
ENV HEALTH_CHECK_INTERVAL_SECONDS=15
ENV HEALTH_CHECK_TIMEOUT_SECONDS=5

# Production health check with shorter intervals
HEALTHCHECK --interval=15s --timeout=5s --start-period=45s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

# Expose the worker web API port
EXPOSE 8081

# Production signal handling
STOPSIGNAL SIGTERM

# Run the comprehensive Rust worker
CMD ["./tasker-worker-rust"]

# =============================================================================
# Development-Optimized Tasker Rust Worker
# =============================================================================
# This creates a comprehensive Rust worker for development that handles ALL namespaces:
# - linear_workflow, diamond_workflow, tree_workflow, mixed_dag_workflow
# - order_fulfillment, inventory, notifications, payments
#
# Development optimizations:
# - Fast build times (debug mode, minimal optimizations)
# - Development tools and debugging capabilities
# - Relaxed security for easier development
# - Enhanced logging and debugging output
#
# Built for local development with fast iteration requirements

# =============================================================================
# Stage 1: Build from common base (debug mode)
# =============================================================================
FROM jcoletaylor/tasker-builder-base AS rust-worker-dev-builder

# Build the comprehensive Rust worker in debug mode for faster compilation
# - Debug mode for faster builds and better debugging
# - All features enabled for comprehensive step handler support
# - No optimizations to speed up development builds
RUN cargo build --package tasker-worker-rust --all-features

# Verify the debug binary was created
RUN ls -la target/debug/tasker-worker-rust

# =============================================================================
# Stage 2: Development runtime
# =============================================================================
FROM debian:bullseye-slim AS dev-runtime

# Install runtime dependencies plus development tools
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    curl \
    netcat-traditional \
    procps \
    htop \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user (relaxed for development)
RUN useradd -m -u 1001 tasker

# Create application directories with development permissions
RUN mkdir -p /app/config /app/logs /app/tmp /app/debug \
    && chown -R tasker:tasker /app

# Copy configuration files
COPY --chown=tasker:tasker config/ /app/config/

# Copy debug Rust worker binary from builder stage
COPY --from=rust-worker-dev-builder --chown=tasker:tasker /app/target/debug/tasker-worker-rust /app/tasker-worker-rust

# Create development helper scripts
COPY --chown=tasker:tasker <<'EOF' /app/debug/worker-status.sh
#!/bin/bash
echo "=== Rust Worker Status ==="
echo "Worker Health: $(curl -s http://localhost:8081/health | jq -r '.status // "unhealthy"')"
echo "Worker Info: $(curl -s http://localhost:8081/info | jq -r '.worker_id // "unknown"')"
echo "Database: $(nc -z postgres 5432 && echo 'connected' || echo 'disconnected')"
echo "Orchestration: $(nc -z orchestration 8080 && echo 'connected' || echo 'disconnected')"
echo "Memory: $(free -h | grep Mem)"
echo "Processes: $(ps aux | grep tasker-worker-rust)"
echo "Supported Namespaces: $SUPPORTED_NAMESPACES"
EOF

RUN chmod +x /app/debug/worker-status.sh

# Switch to non-root user
USER tasker
WORKDIR /app

# Development environment configuration
ENV RUST_LOG=debug
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_development
ENV TASKER_ENV=development
ENV WORKSPACE_PATH=/app
ENV TASKER_CONFIG_ROOT=/app/config

# Development debugging flags
ENV RUST_BACKTRACE=full
ENV TOKIO_CONSOLE_BIND=0.0.0.0:6669

# Rust worker identification for development
ENV WORKER_ID=rust-worker-development
ENV WORKER_TYPE=comprehensive-dev

# Namespace configuration - this Rust worker handles ALL namespaces
ENV SUPPORTED_NAMESPACES=linear_workflow,diamond_workflow,tree_workflow,mixed_dag_workflow,order_fulfillment,inventory,notifications,payments

# Web API configuration for development
ENV WEB_API_ENABLED=true
ENV WEB_API_BIND_ADDRESS=0.0.0.0:8081
ENV WEB_API_REQUEST_TIMEOUT_MS=60000

# Orchestration client configuration for development
ENV ORCHESTRATION_CLIENT_ENABLED=true
ENV ORCHESTRATION_API_URL=http://orchestration:8080

# PGMQ configuration optimized for development
ENV PGMQ_ENABLED=true
ENV PGMQ_POLL_INTERVAL_MS=1000
ENV PGMQ_MAX_MESSAGES_PER_POLL=10

# Event system configuration for development
ENV EVENT_SYSTEM_ENABLED=true
ENV EVENT_SYSTEM_BATCH_SIZE=20

# Resource limits for development
ENV MAX_CONCURRENT_TASKS=10
ENV DATABASE_POOL_SIZE=5
ENV DATABASE_CONNECTION_TIMEOUT_SECONDS=30

# Health monitoring configuration (relaxed for development)
ENV HEALTH_CHECK_INTERVAL_SECONDS=30
ENV HEALTH_CHECK_TIMEOUT_SECONDS=10

# Development health check with longer intervals and more retries
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=5 \
    CMD /app/debug/worker-status.sh

# Expose the worker web API port and Tokio console port
EXPOSE 8081 6669

# Development signal handling (allow debugging)
STOPSIGNAL SIGINT

# Run the comprehensive Rust worker with development settings
CMD ["./tasker-worker-rust"]

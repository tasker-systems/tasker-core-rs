# =============================================================================
# Development-Optimized Tasker Orchestration Service
# =============================================================================
# This creates a development-friendly orchestration service with:
# - Fast build times (debug mode, minimal optimizations)
# - Development tools and debugging capabilities
# - Relaxed security for easier development
# - Enhanced logging and debugging output
#
# Built for local development with fast iteration requirements

# =============================================================================
# Stage 1: Build from common base (debug mode)
# =============================================================================
FROM jcoletaylor/tasker-builder-base AS orchestration-dev-builder

# Build the orchestration service in debug mode for faster compilation
# - Debug mode for faster builds and better debugging
# - All features enabled for comprehensive API coverage
# - No optimizations to speed up development builds
RUN cargo build --package tasker-orchestration --all-features

# Verify the debug binary was created
RUN ls -la target/debug/tasker-orchestration

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
    && rm -rf /var/lib/apt/lists/*

# Create non-root user (relaxed for development)
RUN useradd -m -u 1001 tasker

# Create application directories with development permissions
RUN mkdir -p /app/config /app/logs /app/tmp /app/debug \
    && chown -R tasker:tasker /app

# Copy configuration files
COPY --chown=tasker:tasker config/ /app/config/

# Copy debug binary from builder stage
COPY --from=orchestration-dev-builder --chown=tasker:tasker /app/target/debug/tasker-orchestration /app/tasker-orchestration

# Create development helper scripts
COPY --chown=tasker:tasker <<'EOF' /app/debug/health-check.sh
#!/bin/bash
echo "=== Orchestration Health Check ==="
echo "Service: $(curl -s http://localhost:8080/health | jq -r '.status // "unhealthy"')"
echo "Database: $(nc -z postgres 5432 && echo 'connected' || echo 'disconnected')"
echo "Memory: $(free -h | grep Mem)"
echo "Processes: $(ps aux | grep tasker-orchestration)"
EOF

RUN chmod +x /app/debug/health-check.sh

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

# Web API configuration for development
ENV WEB_API_ENABLED=true
ENV WEB_API_BIND_ADDRESS=0.0.0.0:8080
ENV WEB_API_REQUEST_TIMEOUT_MS=60000

# Orchestration configuration optimized for development
ENV ORCHESTRATION_ENABLED=true
ENV ORCHESTRATION_POLL_INTERVAL_MS=2000
ENV ORCHESTRATION_MAX_CONCURRENT_TASKS=10

# Database pool configuration for development
ENV DATABASE_POOL_SIZE=5
ENV DATABASE_MAX_CONNECTIONS=10
ENV DATABASE_CONNECTION_TIMEOUT_SECONDS=30

# Circuit breaker configuration (relaxed for development)
ENV CIRCUIT_BREAKER_ENABLED=false
ENV CIRCUIT_BREAKER_FAILURE_THRESHOLD=10
ENV CIRCUIT_BREAKER_RECOVERY_TIMEOUT_SECONDS=30

# Development health check with longer intervals and more retries
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=5 \
    CMD /app/debug/health-check.sh

# Expose the orchestration API port and Tokio console port
EXPOSE 8080 6669

# Development signal handling (allow debugging)
STOPSIGNAL SIGINT

# Run the orchestration service with development settings
CMD ["./tasker-orchestration"]

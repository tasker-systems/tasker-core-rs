# =============================================================================
# Production-Optimized Tasker Orchestration Service
# =============================================================================
# This creates a production-ready orchestration service with:
# - Full compiler optimizations for maximum performance
# - Minimal runtime footprint and security hardening
# - Comprehensive health monitoring and metrics collection
# - Complete API endpoints for task coordination
#
# Built for production deployment with high performance requirements

# =============================================================================
# Stage 1: Build from common base
# =============================================================================
FROM jcoletaylor/tasker-builder-base AS orchestration-builder

# Build the orchestration service with all optimizations
# - Release mode for maximum performance
# - All features enabled for comprehensive API coverage
# - Link-time optimization for smaller binaries
ENV SQLX_OFFLINE=true
RUN cargo build --release --package tasker-orchestration --all-features

# Verify the binary was created and is optimized
RUN ls -la target/release/tasker-server && \
    strip target/release/tasker-server

# =============================================================================
# Stage 2: Minimal runtime (production hardened)
# =============================================================================
FROM debian:bullseye-slim AS runtime

# Install only essential runtime dependencies
RUN apt-get update && apt-get install -y \
    debian-archive-keyring \
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

# Copy optimized binary from builder stage
COPY --from=orchestration-builder --chown=tasker:tasker /app/target/release/tasker-server /app/tasker-server

# Switch to non-root user
USER tasker
WORKDIR /app

# Production environment configuration
ENV RUST_LOG=info
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_production
ENV TASKER_ENV=production
ENV WORKSPACE_PATH=/app
ENV TASKER_CONFIG_ROOT=/app/config

# Web API configuration for production load
ENV WEB_API_ENABLED=true
ENV WEB_API_BIND_ADDRESS=0.0.0.0:8080
ENV WEB_API_REQUEST_TIMEOUT_MS=30000

# Orchestration configuration optimized for production
ENV ORCHESTRATION_ENABLED=true
ENV ORCHESTRATION_POLL_INTERVAL_MS=500
ENV ORCHESTRATION_MAX_CONCURRENT_TASKS=100

# Database pool configuration for production load
ENV DATABASE_POOL_SIZE=20
ENV DATABASE_MAX_CONNECTIONS=50
ENV DATABASE_CONNECTION_TIMEOUT_SECONDS=30

# Circuit breaker configuration for production resilience
ENV CIRCUIT_BREAKER_ENABLED=true
ENV CIRCUIT_BREAKER_FAILURE_THRESHOLD=5
ENV CIRCUIT_BREAKER_RECOVERY_TIMEOUT_SECONDS=60

# Production health check with shorter intervals
HEALTHCHECK --interval=15s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Expose the orchestration API port
EXPOSE 8080

# Production signal handling
STOPSIGNAL SIGTERM

# Run the orchestration service
CMD ["./tasker-server"]

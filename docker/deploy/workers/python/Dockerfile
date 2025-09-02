# =============================================================================
# Production-Optimized Tasker Python Worker
# =============================================================================
# This creates a Python worker that integrates with the Rust core via PyO3:
# - Python-based step handlers with Rust core coordination
# - Full integration with tasker-core via PyO3 bindings
# - Namespace-specific worker for Python-implemented workflows
# - PyO3 bridge for high-performance orchestration
#
# Production optimizations:
# - Python 3.12 for latest performance improvements
# - Minimal runtime footprint and security hardening
# - Comprehensive health monitoring via FastAPI
#
# Built for production deployment with Python ecosystem integration

# =============================================================================
# Stage 1: Python Builder with Rust
# =============================================================================
FROM python:3.12-bullseye AS python-builder

# Install system dependencies needed for PyO3 compilation
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    libffi-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust for PyO3 bindings compilation
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install maturin for Python wheel building
RUN pip install maturin

WORKDIR /app

# Copy Python worker source
COPY workers/python/ ./workers/python/

# Copy Rust source needed for PyO3 bindings
COPY tasker-shared/ ./tasker-shared/
COPY pgmq-notify/ ./pgmq-notify/
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/

# Install Python dependencies
WORKDIR /app/workers/python
COPY workers/python/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Build PyO3 extension wheel
RUN maturin build --release --out dist/ --find-interpreter

# Install the built wheel
RUN pip install dist/*.whl

# =============================================================================
# Stage 2: Production Python Runtime
# =============================================================================
FROM python:3.12-slim-bullseye AS runtime

# Install only essential runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    libffi7 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get purge -y --auto-remove

# Create non-root user for security
RUN useradd -r -u 1001 -m -s /bin/false tasker

# Create application directories with minimal permissions
RUN mkdir -p /app/config /app/logs /app/workers/python \
    && chown -R tasker:tasker /app \
    && chmod 755 /app \
    && chmod 750 /app/config /app/logs

# Copy Python application and dependencies from builder stage
COPY --from=python-builder /usr/local/lib/python3.12/site-packages/ /usr/local/lib/python3.12/site-packages/
COPY --from=python-builder /usr/local/bin/ /usr/local/bin/
COPY --from=python-builder --chown=tasker:tasker /app/workers/python /app/workers/python

# Copy configuration files
COPY --chown=tasker:tasker config/ /app/config/

# Switch to non-root user
USER tasker
WORKDIR /app/workers/python

# Production environment configuration
ENV PYTHONPATH=/app/workers/python
ENV PYTHONUNBUFFERED=1
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_production
ENV TASKER_ENV=production
ENV WORKSPACE_PATH=/app
ENV TASKER_CONFIG_ROOT=/app/config

# Python worker identification
ENV WORKER_ID=python-worker-production
ENV WORKER_TYPE=python-pyo3
ENV WORKER_LANGUAGE=python

# Namespace configuration - Python worker handles Python-specific workflows
ENV SUPPORTED_NAMESPACES=python_workflows,data_processing,ml_pipelines,analytics

# FastAPI server configuration for health monitoring
ENV WEB_SERVER_ENABLED=true
ENV WEB_SERVER_HOST=0.0.0.0
ENV WEB_SERVER_PORT=8083

# Orchestration client configuration
ENV ORCHESTRATION_CLIENT_ENABLED=true
ENV ORCHESTRATION_API_URL=http://orchestration:8080

# PGMQ configuration for Python worker
ENV PGMQ_ENABLED=true
ENV PGMQ_POLL_INTERVAL_MS=500
ENV PGMQ_MAX_MESSAGES_PER_POLL=25

# Resource limits optimized for Python
ENV MAX_CONCURRENT_TASKS=15
ENV DATABASE_POOL_SIZE=8
ENV DATABASE_CONNECTION_TIMEOUT_SECONDS=30

# Health monitoring
ENV HEALTH_CHECK_INTERVAL_SECONDS=15
ENV HEALTH_CHECK_TIMEOUT_SECONDS=5

# Production health check
HEALTHCHECK --interval=15s --timeout=5s --start-period=45s --retries=3 \
    CMD curl -f http://localhost:8083/health || exit 1

# Expose the Python worker FastAPI port
EXPOSE 8083

# Production signal handling
STOPSIGNAL SIGTERM

# Run the Python worker with PyO3 integration
CMD ["python", "-m", "tasker_python_worker"]

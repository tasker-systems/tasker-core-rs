# =============================================================================
# Python Worker Service - Test Dockerfile (LOCAL OPTIMIZED)
# =============================================================================
# Python-driven worker that bootstraps Rust foundation via FFI (PyO3)
# Optimizations:
# - Single-stage Rust build using maturin for PyO3 compilation
# - BuildKit cache mount support for incremental builds
# - UV for fast Python package management (using official Astral images)
# Context: tasker-core/ directory (workspace root)
# Usage: DOCKER_BUILDKIT=1 docker build -f docker/build/python-worker.test.Dockerfile -t tasker-python-worker:test .
#
# References:
# - UV Docker Guide: https://docs.astral.sh/uv/guides/integration/docker/

# Verify BuildKit is enabled (will fail if not)
ARG DOCKER_BUILDKIT=1

# =============================================================================
# Python Builder - Compile PyO3 extensions with both Python and Rust available
# =============================================================================
FROM python:3.10-bullseye AS python_builder

# Install system dependencies for PyO3 compilation
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libffi-dev \
    libssl-dev \
    libpq-dev \
    libclang-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:$PATH"

# Set libclang path for bindgen (Debian Bullseye uses LLVM 11)
ENV LIBCLANG_PATH=/usr/lib/llvm-11/lib

# Install UV using official Astral image (recommended approach)
# See: https://docs.astral.sh/uv/guides/integration/docker/
# Pin to specific version for reproducible builds
COPY --from=ghcr.io/astral-sh/uv:0.9.17 /uv /uvx /bin/

# UV configuration for Docker builds
ENV UV_LINK_MODE=copy
ENV UV_COMPILE_BYTECODE=1
ENV UV_PYTHON_DOWNLOADS=never

WORKDIR /app

# Copy workspace root files for Cargo workspace resolution
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
# Copy src/ directory (even if empty, needed for workspace structure)
COPY src/ ./src/

# Copy workspace crates needed by Python FFI extension
COPY tasker-shared/ ./tasker-shared/
COPY tasker-worker/ ./tasker-worker/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/

# Copy minimal workspace structure for crates we don't actually need
# Cargo validates ALL workspace members even if unused, so we need their Cargo.toml files
# Uses shared stub script to reduce maintenance burden
COPY docker/scripts/create-workspace-stubs.sh /tmp/
RUN chmod +x /tmp/create-workspace-stubs.sh && \
    /tmp/create-workspace-stubs.sh tasker-orchestration workers/rust workers/ruby workers/typescript
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/
COPY workers/rust/Cargo.toml ./workers/rust/
COPY workers/ruby/ext/tasker_core/Cargo.toml ./workers/ruby/ext/tasker_core/
COPY workers/typescript/Cargo.toml ./workers/typescript/

# Copy Python worker source code to proper workspace location
COPY workers/python/ ./workers/python/
COPY migrations/ ./migrations/

# Set working directory and environment for Python worker
ENV SQLX_OFFLINE=true
WORKDIR /app/workers/python

# Create virtual environment using UV
RUN uv venv /app/.venv
ENV VIRTUAL_ENV=/app/.venv
ENV PATH="/app/.venv/bin:$PATH"

# Install Python dependencies using UV with cache mount
# Using sync with pyproject.toml for reproducible builds
# --active tells uv to use the venv specified by VIRTUAL_ENV rather than .venv in project dir
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --no-dev --locked --active

# Install maturin for PyO3 compilation (build dependency)
RUN --mount=type=cache,target=/root/.cache/uv \
    uv pip install maturin>=1.7

# Compile Python FFI extensions
# NOTE: Removed BuildKit cache mounts due to stale artifact issues with pythonize/serde.
# The cache can retain broken compilation artifacts that cause "can't find crate for serde" errors.
# For faster local builds, consider using docker build caching via DOCKER_BUILDKIT layers instead.
# IMPORTANT: Use --locked to ensure Cargo.lock is respected (prevents version conflicts)
RUN maturin develop --release --locked

# =============================================================================
# Runtime - Python-driven worker image
# =============================================================================
FROM python:3.10-slim-bullseye AS runtime

WORKDIR /app

# Install runtime dependencies only (no build tools)
RUN apt-get update && apt-get install -y \
    libssl1.1 \
    libpq5 \
    postgresql-client \
    libffi7 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -g daemon -u 999 tasker

# Copy virtual environment from builder (includes compiled bytecode)
COPY --from=python_builder /app/.venv /app/.venv
ENV VIRTUAL_ENV=/app/.venv
ENV PATH="/app/.venv/bin:$PATH"

# Copy Python worker source code
COPY --from=python_builder /app/workers/python/python ./python_worker/python
COPY --from=python_builder /app/workers/python/bin ./python_worker/bin

# Copy test handlers for E2E testing
COPY --from=python_builder /app/workers/python/tests/handlers ./python_handlers

# Ensure all Python files are readable
RUN chmod -R 755 ./python_worker/bin && \
    chmod -R 644 ./python_worker/python && find ./python_worker/python -type d -exec chmod 755 {} \; && \
    chmod -R 644 ./python_handlers && find ./python_handlers -type d -exec chmod 755 {} \;

# Copy Python worker entrypoint script
COPY docker/scripts/python-worker-entrypoint.sh /app/python_worker_entrypoint.sh
RUN chmod 755 /app/python_worker_entrypoint.sh

# Set environment variables for Python worker
ENV APP_NAME=tasker-python-worker
ENV PYTHON_WORKER_ENABLED=true
ENV PYTHONPATH=/app/python_worker/python

# Python-specific environment
ENV PYTHON_VERSION=3.10
ENV PYTHONUNBUFFERED=1
ENV PYTHONDONTWRITEBYTECODE=1

# Template discovery paths for Python handlers
ENV TASKER_TEMPLATE_PATH=/app/python_templates
ENV PYTHON_HANDLER_PATH=/app/python_handlers

# Python worker will expose its own health check via the bootstrap system
# Note: Internal port is 8081, Docker Compose maps to external port 8083
HEALTHCHECK --interval=15s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

USER tasker

EXPOSE 8081

WORKDIR /app/python_worker

# Run Python worker entrypoint
ENTRYPOINT ["/app/python_worker_entrypoint.sh"]

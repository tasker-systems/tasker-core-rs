# =============================================================================
# Ruby Worker Service - Test Dockerfile (LOCAL OPTIMIZED)
# =============================================================================
# Ruby-driven worker that bootstraps Rust foundation via FFI
# Optimizations:
# - Single-stage Rust build (removed duplicate rust_builder stage)
# - BuildKit cache mount support for incremental builds
# - Fixed binary copy pattern in single layer
# Context: tasker-core/ directory (workspace root)
# Usage: DOCKER_BUILDKIT=1 docker build -f docker/build/ruby-worker.test-local.Dockerfile -t tasker-ruby-worker:test .

# Verify BuildKit is enabled (will fail if not)
ARG DOCKER_BUILDKIT=1

# =============================================================================
# Ruby Builder - Compile Ruby FFI extensions with both Ruby and Rust available
# =============================================================================
FROM ruby:3.4.4-bullseye AS ruby_builder

# Install system dependencies for Ruby FFI compilation
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libffi-dev \
    libssl-dev \
    libpq-dev \
    libclang-dev \
    libyaml-dev \
    zlib1g-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# OPTIMIZATION 1: Install Rust toolchain once (removed separate rust_builder stage)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:$PATH"

# Set libclang path for bindgen (Debian Bullseye uses LLVM 11)
ENV LIBCLANG_PATH=/usr/lib/llvm-11/lib

WORKDIR /app

# Copy workspace root files for Cargo workspace resolution
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
# Copy src/ directory (even if empty, needed for workspace structure)
COPY src/ ./src/

# Copy workspace crates needed by Ruby FFI extension
COPY tasker-shared/ ./tasker-shared/
COPY tasker-worker/ ./tasker-worker/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/

# Copy minimal workspace structure for crates we don't actually need
# Cargo validates ALL workspace members even if unused, so we need their Cargo.toml files
# Uses shared stub script to reduce maintenance burden
COPY docker/scripts/create-workspace-stubs.sh /tmp/
RUN chmod +x /tmp/create-workspace-stubs.sh && \
    /tmp/create-workspace-stubs.sh tasker-orchestration workers/rust workers/python workers/typescript
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/
COPY workers/rust/Cargo.toml ./workers/rust/
COPY workers/python/Cargo.toml ./workers/python/
COPY workers/typescript/Cargo.toml ./workers/typescript/

# Copy Ruby worker source code to proper workspace location
COPY workers/ruby/ ./workers/ruby/
COPY migrations/ ./migrations/

# Set working directory and environment for Ruby worker
ENV SQLX_OFFLINE=true
WORKDIR /app/workers/ruby

# Install Ruby dependencies
# Remove deployment mode for test builds - we're testing, not deploying
RUN bundle config set --local without 'development'
RUN bundle install

# Compile Ruby FFI extensions with cache mounts for incremental builds
# rb_sys will handle all Rust compilation via bundle exec rake compile
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    bundle exec rake compile

# =============================================================================
# Runtime - Ruby-driven worker image (OPTIMIZED - using slim base)
# =============================================================================
FROM ruby:3.4.4-slim-bullseye AS runtime

WORKDIR /app

# Install runtime dependencies only (no build tools)
RUN apt-get update && apt-get install -y \
    libssl1.1 \
    libpq5 \
    postgresql-client \
    libffi7 \
    libyaml-0-2 \
    zlib1g \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -g daemon -u 999 tasker

# OPTIMIZATION: Copy only necessary Ruby worker files (exclude tmp/, spec/, doc/, etc.)
# This avoids copying 1.3GB of Rust build artifacts from tmp/ directory
WORKDIR /app/ruby_worker
COPY --from=ruby_builder /app/workers/ruby/bin ./bin
COPY --from=ruby_builder /app/workers/ruby/lib ./lib
COPY --from=ruby_builder /app/workers/ruby/Gemfile* ./
COPY --from=ruby_builder /app/workers/ruby/*.gemspec ./
COPY --from=ruby_builder /app/workers/ruby/Rakefile ./

# Copy bundled gems from builder (includes compiled extensions and all gems)
# This preserves the compiled FFI extension from ruby_builder stage
# Gems install to /usr/local/bundle by default in Ruby Docker images
COPY --from=ruby_builder /usr/local/bundle /usr/local/bundle

# Configure bundler environment (gems already installed via COPY above)
ENV BUNDLE_WITHOUT=development
ENV BUNDLE_APP_CONFIG=/app/ruby_worker/.bundle

# Extensions and gems are already compiled/installed from ruby_builder stage

# Copy Ruby worker entrypoint script
COPY docker/scripts/ruby-worker-entrypoint.sh /app/ruby_worker_entrypoint.sh
RUN chmod +x /app/ruby_worker_entrypoint.sh

# Set environment variables for Ruby worker
ENV APP_NAME=tasker-ruby-worker
ENV RUBY_WORKER_ENABLED=true
ENV BUNDLE_GEMFILE=/app/ruby_worker/Gemfile

# Ruby-specific environment
ENV RUBY_VERSION=3.4.4

# Template discovery paths for Ruby handlers
ENV TASKER_TEMPLATE_PATH=/app/ruby_templates
ENV RUBY_HANDLER_PATH=/app/ruby_handlers

# Ruby worker will expose its own health check via the bootstrap system
# Note: Health check port will be determined by Ruby bootstrap configuration
HEALTHCHECK --interval=15s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

USER tasker

EXPOSE 8081

# Run Ruby worker entrypoint (not Rust binary)
ENTRYPOINT ["/app/ruby_worker_entrypoint.sh"]

# =============================================================================
# Ruby Worker Service - Test Dockerfile
# =============================================================================
# Ruby-driven worker that bootstraps Rust foundation via FFI
# Context: tasker-core/ directory (workspace root)
# Usage: docker build -f docker/build/ruby-worker.test.Dockerfile -t tasker-ruby-worker:test .

FROM rust:1.90-bullseye AS rust_builder

# Install system dependencies for Rust compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace for Rust FFI extension compilation
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
COPY src/ ./src/
COPY tasker-orchestration/ ./tasker-orchestration/
COPY tasker-shared/ ./tasker-shared/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/
COPY tasker-worker/ ./tasker-worker/
COPY workers/ ./workers/
COPY migrations/ ./migrations/
COPY .sqlx/ ./.sqlx/

# Build Ruby FFI extensions (not the binary worker)
WORKDIR /app/workers/ruby
ENV SQLX_OFFLINE=true
RUN cargo build --all-features  # Build FFI libraries for Ruby integration

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
    libyaml-dev \
    zlib1g-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain for FFI compilation
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app/ruby_worker

# Copy Ruby worker source code
COPY workers/ruby/ ./

# Copy compiled Rust FFI libraries from rust_builder
COPY --from=rust_builder /app/workers/ruby/target ./target/

# Install Ruby dependencies
RUN bundle config set --local deployment 'true'
RUN bundle config set --local without 'development'
RUN bundle install

# Compile Ruby FFI extensions (links against pre-built Rust libraries)
# This stage has both Ruby and Rust toolchain available
RUN bundle exec rake compile

# =============================================================================
# Runtime - Ruby-driven worker image
# =============================================================================
FROM ruby:3.4.4-bullseye AS runtime

WORKDIR /app

# Install runtime dependencies only (no build tools)
RUN apt-get update && apt-get install -y \
    libssl1.1 \
    libpq5 \
    libffi8 \
    libyaml-0-2 \
    zlib1g \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -g daemon -u 999 tasker

# Copy Ruby worker source code and compiled extensions from ruby_builder
COPY --from=ruby_builder /app/ruby_worker ./ruby_worker/
WORKDIR /app/ruby_worker

# Extensions are already compiled in ruby_builder stage

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
ENV TASK_TEMPLATE_PATH=/app/ruby_templates
ENV RUBY_HANDLER_PATH=/app/ruby_handlers

# Ruby worker will expose its own health check via the bootstrap system
# Note: Health check port will be determined by Ruby bootstrap configuration
HEALTHCHECK --interval=15s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

USER tasker

EXPOSE 8081

# Run Ruby worker entrypoint (not Rust binary)
ENTRYPOINT ["/app/ruby_worker_entrypoint.sh"]

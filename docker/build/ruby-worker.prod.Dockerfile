# =============================================================================
# Ruby Worker Service - Production Dockerfile
# =============================================================================
# Ruby-driven worker that bootstraps Rust foundation via FFI
# Context: tasker-core/ directory (workspace root)
# Usage: docker build -f docker/build/ruby-worker.prod.Dockerfile -t tasker-ruby-worker:prod .

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
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain for FFI compilation
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
# Set libclang path for bindgen (Debian Bullseye uses LLVM 11)
ENV LIBCLANG_PATH=/usr/lib/llvm-11/lib

WORKDIR /app

# Copy workspace root files for Cargo workspace resolution
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/
# Note: src/ is empty now (no library code), but keep for workspace structure

# Copy workspace crates needed by Ruby FFI extension
COPY tasker-shared/ ./tasker-shared/
COPY tasker-worker/ ./tasker-worker/
COPY tasker-client/ ./tasker-client/
COPY pgmq-notify/ ./pgmq-notify/

# Copy minimal workspace structure for crates we don't actually need
# Cargo validates ALL workspace members even if unused, so we need their Cargo.toml files
# We don't copy source code - just enough to satisfy workspace validation
RUN mkdir -p tasker-orchestration/src && \
    echo "pub fn main() {}" > tasker-orchestration/src/lib.rs
COPY tasker-orchestration/Cargo.toml ./tasker-orchestration/

RUN mkdir -p workers/rust/src && \
    echo "pub fn main() {}" > workers/rust/src/lib.rs
COPY workers/rust/Cargo.toml ./workers/rust/

# Copy Ruby worker source code to proper workspace location
COPY workers/ruby/ ./workers/ruby/
COPY migrations/ ./migrations/

# Install Ruby dependencies
WORKDIR /app/workers/ruby
RUN bundle config set --local deployment 'true'
RUN bundle config set --local without 'development'
RUN bundle install

ENV SQLX_OFFLINE=true
ENV RB_SYS_CARGO_PROFILE=release
# Compile Ruby FFI extensions
# rb_sys will handle all Rust compilation via bundle exec rake compile
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
    postgresql-client \
    libffi7 \
    libyaml-0-2 \
    zlib1g \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -g daemon -u 999 tasker

# Copy Ruby worker source code and compiled extensions from ruby_builder
COPY --from=ruby_builder /app/workers/ruby ./ruby_worker/
WORKDIR /app/ruby_worker

# Copy bundled gems from builder (includes compiled extensions and all gems)
# Gems install to /usr/local/bundle by default in Ruby Docker images
COPY --from=ruby_builder /usr/local/bundle /usr/local/bundle

# Extensions are already compiled in ruby_builder stage

# Copy Ruby worker entrypoint script
COPY docker/scripts/ruby-worker-entrypoint.sh /app/ruby_worker_entrypoint.sh
RUN chmod +x /app/ruby_worker_entrypoint.sh

# Set environment variables for Ruby worker (production)
ENV APP_NAME=tasker-ruby-worker
ENV RUBY_WORKER_ENABLED=true
ENV BUNDLE_GEMFILE=/app/ruby_worker/Gemfile

# Ruby-specific environment
ENV RUBY_VERSION=3.4.4

# Production environment settings
ENV TASKER_ENV=production
ENV RUBY_ENV=production

# Template discovery paths for Ruby handlers
ENV TASKER_TEMPLATE_PATH=/app/ruby_templates
ENV RUBY_HANDLER_PATH=/app/ruby_handlers

# Production performance optimizations
ENV RUBY_GC_HEAP_GROWTH_FACTOR=1.1
ENV RUBY_GC_HEAP_GROWTH_MAX_SLOTS=100000
ENV RUBY_GC_HEAP_INIT_SLOTS=600000

# Ruby worker will expose its own health check via the bootstrap system
HEALTHCHECK --interval=15s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

USER tasker

EXPOSE 8081

# Run Ruby worker entrypoint (not Rust binary)
ENTRYPOINT ["/app/ruby_worker_entrypoint.sh"]

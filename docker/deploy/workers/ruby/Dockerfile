# =============================================================================
# Production-Optimized Tasker Ruby Worker
# =============================================================================
# This creates a Ruby worker that integrates with the Rust core via FFI:
# - Ruby-based step handlers with Rust core coordination
# - Full integration with tasker-core via Ruby extension
# - Namespace-specific worker for Ruby-implemented workflows
# - FFI bridge for high-performance orchestration
#
# Production optimizations:
# - Ruby 3.3 for latest performance improvements
# - Minimal runtime footprint and security hardening
# - Comprehensive health monitoring via Ruby web framework
#
# Built for production deployment with Ruby ecosystem integration

# =============================================================================
# Stage 1: Ruby Builder
# =============================================================================
FROM ruby:3.3-bullseye AS ruby-builder

# Install system dependencies needed for Ruby extension compilation
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    libffi-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust for FFI extension compilation
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

# Copy Ruby worker source and extension
COPY workers/ruby/ ./workers/ruby/
COPY Gemfile* ./workers/ruby/

# Copy Rust source needed for extension compilation
COPY tasker-shared/ ./tasker-shared/
COPY pgmq-notify/ ./pgmq-notify/
COPY Cargo.toml Cargo.lock ./
COPY .cargo/ ./.cargo/

# Build Ruby extension with Rust integration
WORKDIR /app/workers/ruby
RUN bundle config set --local deployment 'true' && \
    bundle config set --local without 'development test' && \
    bundle install

# Compile the Ruby extension with Rust FFI
RUN bundle exec rake compile

# =============================================================================
# Stage 2: Production Ruby Runtime
# =============================================================================
FROM ruby:3.3-slim-bullseye AS runtime

# Install only essential runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    libffi8 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get purge -y --auto-remove

# Create non-root user for security
RUN useradd -r -u 1001 -m -s /bin/false tasker

# Create application directories with minimal permissions
RUN mkdir -p /app/config /app/logs /app/workers/ruby \
    && chown -R tasker:tasker /app \
    && chmod 755 /app \
    && chmod 750 /app/config /app/logs

# Copy Ruby application from builder stage
COPY --from=ruby-builder --chown=tasker:tasker /app/workers/ruby /app/workers/ruby

# Copy configuration files
COPY --chown=tasker:tasker config/ /app/config/

# Switch to non-root user
USER tasker
WORKDIR /app/workers/ruby

# Production environment configuration
ENV RAILS_ENV=production
ENV RACK_ENV=production
ENV DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_production
ENV TASKER_ENV=production
ENV WORKSPACE_PATH=/app
ENV TASKER_CONFIG_ROOT=/app/config

# Ruby worker identification
ENV WORKER_ID=ruby-worker-production
ENV WORKER_TYPE=ruby-ffi
ENV WORKER_LANGUAGE=ruby

# Namespace configuration - Ruby worker handles Ruby-specific workflows
ENV SUPPORTED_NAMESPACES=ruby_workflows,custom_handlers,legacy_integration

# Web server configuration for health monitoring
ENV WEB_SERVER_ENABLED=true
ENV WEB_SERVER_BIND_ADDRESS=0.0.0.0
ENV WEB_SERVER_PORT=8082

# Orchestration client configuration
ENV ORCHESTRATION_CLIENT_ENABLED=true
ENV ORCHESTRATION_API_URL=http://orchestration:8080

# PGMQ configuration for Ruby worker
ENV PGMQ_ENABLED=true
ENV PGMQ_POLL_INTERVAL_MS=500
ENV PGMQ_MAX_MESSAGES_PER_POLL=25

# Resource limits optimized for Ruby
ENV MAX_CONCURRENT_TASKS=20
ENV DATABASE_POOL_SIZE=10
ENV DATABASE_CONNECTION_TIMEOUT_SECONDS=30

# Health monitoring
ENV HEALTH_CHECK_INTERVAL_SECONDS=15
ENV HEALTH_CHECK_TIMEOUT_SECONDS=5

# Production health check
HEALTHCHECK --interval=15s --timeout=5s --start-period=45s --retries=3 \
    CMD curl -f http://localhost:8082/health || exit 1

# Expose the Ruby worker web server port
EXPOSE 8082

# Production signal handling
STOPSIGNAL SIGTERM

# Run the Ruby worker with FFI integration
CMD ["bundle", "exec", "ruby", "bin/tasker-ruby-worker"]
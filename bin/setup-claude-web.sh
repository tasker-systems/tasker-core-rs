#!/usr/bin/env bash
# =============================================================================
# Tasker Core - Claude Code on the Web Environment Setup
# =============================================================================
#
# Companion to bin/setup-dev.sh (macOS). This script sets up the Claude Code
# on the web (remote) environment with the tools needed for Rust development,
# PostgreSQL, and environment configuration.
#
# This script is invoked automatically via .claude/settings.json SessionStart
# hook when running in a Claude Code remote environment. It is idempotent and
# safe to run on session resume/compact events.
#
# What it installs:
#   - System libraries (libssl-dev, libpq-dev, pkg-config, cmake)
#   - Protocol Buffers compiler (protoc)
#   - Rust toolchain (if not present)
#   - cargo-make, sqlx-cli, cargo-nextest
#   - PostgreSQL database (via Docker or native)
#   - Project environment variables
#
# What it skips (to minimize overhead):
#   - Profiling tools (samply, flamegraph, tokio-console)
#   - Code quality tools (cargo-audit, cargo-machete, cargo-llvm-cov)
#   - Ruby/Python/TypeScript runtimes (focus on Rust core)
#   - Telemetry stack (Grafana, OTLP)
#
# Usage:
#   ./bin/setup-claude-web.sh           # Auto-invoked by SessionStart hook
#   FORCE_SETUP=1 ./bin/setup-claude-web.sh  # Run even outside remote env
#
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Environment Guard
# ---------------------------------------------------------------------------
# Only run in Claude Code remote environments unless FORCE_SETUP is set
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ] && [ "${FORCE_SETUP:-}" != "1" ]; then
  exit 0
fi

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

# ---------------------------------------------------------------------------
# Helper Functions
# ---------------------------------------------------------------------------

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

persist_env() {
  # Write an export statement to CLAUDE_ENV_FILE for session-wide persistence
  if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
    echo "$1" >> "$CLAUDE_ENV_FILE"
  fi
}

log_section() {
  echo ""
  echo "==> $1"
}

log_ok() {
  echo "  [ok] $1"
}

log_skip() {
  echo "  [skip] $1"
}

log_warn() {
  echo "  [warn] $1"
}

log_install() {
  echo "  [install] $1"
}

# ---------------------------------------------------------------------------
# PATH Setup
# ---------------------------------------------------------------------------
# Ensure common tool directories are in PATH from the start
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
persist_env 'export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"'

log_section "Setting up tasker-core for Claude Code on the web"
echo "  Project: $PROJECT_DIR"

# ---------------------------------------------------------------------------
# System Dependencies (Linux/apt)
# ---------------------------------------------------------------------------
log_section "System dependencies"

if command_exists apt-get; then
  PKGS_NEEDED=""

  # Check each package individually
  for pkg in libssl-dev libpq-dev pkg-config cmake jq unzip curl; do
    if ! dpkg -l "$pkg" >/dev/null 2>&1; then
      PKGS_NEEDED="$PKGS_NEEDED $pkg"
    fi
  done

  if [ -n "$PKGS_NEEDED" ]; then
    log_install "apt packages:$PKGS_NEEDED"
    sudo apt-get update -qq 2>/dev/null
    sudo apt-get install -y -qq $PKGS_NEEDED 2>/dev/null
  else
    log_ok "all system packages present"
  fi
else
  log_skip "apt-get not available (non-Debian system)"
fi

# ---------------------------------------------------------------------------
# Protocol Buffers Compiler
# ---------------------------------------------------------------------------
log_section "Protocol Buffers compiler (protoc)"

if command_exists protoc; then
  log_ok "protoc $(protoc --version 2>/dev/null | head -1 || echo 'installed')"
else
  PROTOC_VERSION="28.3"
  PROTOC_ZIP="protoc-${PROTOC_VERSION}-linux-x86_64.zip"
  log_install "protoc v${PROTOC_VERSION}"

  curl -sSL -o "/tmp/${PROTOC_ZIP}" \
    "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/${PROTOC_ZIP}"
  mkdir -p "$HOME/.local"
  unzip -qo "/tmp/${PROTOC_ZIP}" -d "$HOME/.local"
  rm -f "/tmp/${PROTOC_ZIP}"

  log_ok "protoc installed to ~/.local/bin"
fi

# ---------------------------------------------------------------------------
# Rust Toolchain
# ---------------------------------------------------------------------------
log_section "Rust toolchain"

if command_exists cargo; then
  log_ok "Rust $(rustc --version 2>/dev/null | awk '{print $2}' || echo 'installed')"
else
  log_install "Rust (stable, minimal profile)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain stable \
    --profile minimal \
    --no-modify-path

  persist_env 'source "$HOME/.cargo/env"'
  log_ok "Rust installed"
fi

# Ensure cargo env is loaded for this script
if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
fi

# Add rustfmt and clippy components (needed for cargo make check)
if command_exists rustup; then
  rustup component add rustfmt clippy 2>/dev/null || true
fi

# ---------------------------------------------------------------------------
# Cargo Tools
# ---------------------------------------------------------------------------
log_section "Cargo tools"

install_cargo_tool() {
  local binary="$1"
  local crate="$2"
  shift 2
  local extra_args=("$@")

  if command_exists "$binary" || cargo install --list 2>/dev/null | grep -q "^${crate} "; then
    log_ok "$crate"
  else
    log_install "$crate"
    cargo install --quiet "$crate" "${extra_args[@]}"
  fi
}

install_cargo_tool cargo-make cargo-make --locked
install_cargo_tool sqlx        sqlx-cli  --no-default-features --features postgres,rustls
install_cargo_tool cargo-nextest cargo-nextest --locked

# ---------------------------------------------------------------------------
# grpcurl (optional, for gRPC API testing)
# ---------------------------------------------------------------------------
log_section "gRPC tooling"

if command_exists grpcurl; then
  log_ok "grpcurl"
else
  GRPCURL_VERSION="1.9.1"
  GRPCURL_TAR="grpcurl_${GRPCURL_VERSION}_linux_x86_64.tar.gz"
  log_install "grpcurl v${GRPCURL_VERSION}"

  curl -sSL -o "/tmp/${GRPCURL_TAR}" \
    "https://github.com/fullstorydev/grpcurl/releases/download/v${GRPCURL_VERSION}/${GRPCURL_TAR}" 2>/dev/null && \
  mkdir -p "$HOME/.local/bin" && \
  tar -xzf "/tmp/${GRPCURL_TAR}" -C "$HOME/.local/bin" grpcurl 2>/dev/null && \
  chmod +x "$HOME/.local/bin/grpcurl" && \
  rm -f "/tmp/${GRPCURL_TAR}" && \
  log_ok "grpcurl installed to ~/.local/bin" || \
  log_warn "grpcurl installation failed (non-critical)"
fi

# ---------------------------------------------------------------------------
# PostgreSQL Setup
# ---------------------------------------------------------------------------
log_section "PostgreSQL database"

PG_READY=false

# Strategy 1: Try Docker (gives us pg18 + PGMQ from our Dockerfile)
setup_postgres_docker() {
  if ! command_exists docker; then
    return 1
  fi

  if ! docker info >/dev/null 2>&1; then
    return 1
  fi

  echo "  Docker available, starting PostgreSQL with PGMQ via docker-compose..."

  local compose_file="$PROJECT_DIR/docker/docker-compose.test.yml"
  if [ ! -f "$compose_file" ]; then
    return 1
  fi

  # Start only the postgres service (skip observability, rabbitmq, dragonfly)
  docker compose -f "$compose_file" up -d postgres 2>/dev/null || \
    docker-compose -f "$compose_file" up -d postgres 2>/dev/null || \
    return 1

  # Wait for readiness (up to 30 seconds)
  echo "  Waiting for PostgreSQL..."
  local retries=30
  while [ $retries -gt 0 ]; do
    if pg_isready -h localhost -p 5432 -U tasker -q 2>/dev/null; then
      log_ok "PostgreSQL ready (Docker, pg18 + PGMQ)"
      return 0
    fi
    retries=$((retries - 1))
    sleep 1
  done

  log_warn "PostgreSQL Docker container started but not ready"
  return 1
}

# Strategy 2: Use pre-installed PostgreSQL (web environment typically has pg16)
setup_postgres_native() {
  if ! command_exists psql; then
    return 1
  fi

  # Try to start PostgreSQL if not running
  if ! pg_isready -q 2>/dev/null; then
    # Try common service management commands
    sudo service postgresql start 2>/dev/null || \
      sudo systemctl start postgresql 2>/dev/null || \
      true

    # Wait for readiness
    local retries=10
    while [ $retries -gt 0 ]; do
      pg_isready -q 2>/dev/null && break
      retries=$((retries - 1))
      sleep 1
    done
  fi

  if ! pg_isready -q 2>/dev/null; then
    return 1
  fi

  echo "  Native PostgreSQL is running"

  # Determine how to connect (try multiple methods)
  local PSQL_SUPER=""
  if sudo -u postgres psql -c "SELECT 1" >/dev/null 2>&1; then
    PSQL_SUPER="sudo -u postgres psql"
  elif psql -U postgres -c "SELECT 1" >/dev/null 2>&1; then
    PSQL_SUPER="psql -U postgres"
  elif psql -c "SELECT 1" >/dev/null 2>&1; then
    PSQL_SUPER="psql"
  else
    log_warn "Cannot connect to PostgreSQL as superuser"
    return 1
  fi

  # Create tasker role if it doesn't exist
  if ! $PSQL_SUPER -tAc "SELECT 1 FROM pg_roles WHERE rolname='tasker'" 2>/dev/null | grep -q 1; then
    $PSQL_SUPER -c "CREATE ROLE tasker WITH LOGIN PASSWORD 'tasker' SUPERUSER;" 2>/dev/null || true
  fi

  # Create database if it doesn't exist
  if ! $PSQL_SUPER -tAc "SELECT 1 FROM pg_database WHERE datname='tasker_rust_test'" 2>/dev/null | grep -q 1; then
    $PSQL_SUPER -c "CREATE DATABASE tasker_rust_test OWNER tasker;" 2>/dev/null || true
  fi

  # Try to install PGMQ extension
  if $PSQL_SUPER -d tasker_rust_test -c "CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;" 2>/dev/null; then
    log_ok "PGMQ extension installed"
  else
    log_warn "PGMQ extension not available - some messaging tests will fail"
    log_warn "PGMQ requires the pgmq PostgreSQL extension to be installed on the system"
  fi

  # Create uuid_generate_v7() compatibility function
  # pg18 has native uuidv7(), older versions need a PL/pgSQL implementation
  $PSQL_SUPER -d tasker_rust_test -c "
    DO \$\$
    BEGIN
      -- Try pg18 native uuidv7 first
      PERFORM uuidv7();
      -- If available, create a simple alias
      CREATE OR REPLACE FUNCTION uuid_generate_v7() RETURNS uuid
        AS 'SELECT uuidv7();'
        LANGUAGE SQL VOLATILE PARALLEL SAFE;
      RAISE NOTICE 'uuid_generate_v7: using pg18 native uuidv7()';
    EXCEPTION WHEN undefined_function THEN
      -- Fallback: PL/pgSQL UUID v7 implementation for pg16/pg17
      CREATE OR REPLACE FUNCTION uuid_generate_v7() RETURNS uuid AS '
        DECLARE
          timestamp_ms bigint;
          uuid_bytes bytea;
        BEGIN
          timestamp_ms := (extract(epoch FROM clock_timestamp()) * 1000)::bigint;
          uuid_bytes := decode(lpad(to_hex(timestamp_ms), 12, ''0''), ''hex'') || gen_random_bytes(10);
          -- Set version to 7 (0111 in bits 48-51)
          uuid_bytes := set_byte(uuid_bytes, 6, (get_byte(uuid_bytes, 6) & 15) | 112);
          -- Set variant to RFC 4122 (10xx in bits 64-65)
          uuid_bytes := set_byte(uuid_bytes, 8, (get_byte(uuid_bytes, 8) & 63) | 128);
          RETURN encode(uuid_bytes, ''hex'')::uuid;
        END
      ' LANGUAGE plpgsql VOLATILE PARALLEL SAFE;
      RAISE NOTICE 'uuid_generate_v7: using PL/pgSQL fallback';
    END
    \$\$;
  " 2>/dev/null || log_warn "Could not create uuid_generate_v7 function"

  log_ok "PostgreSQL configured (native)"
  return 0
}

# Try Docker first (preferred: gives us pg18 + PGMQ), then native
if setup_postgres_docker; then
  PG_READY=true
elif setup_postgres_native; then
  PG_READY=true
else
  log_warn "PostgreSQL not available - database tests will fail"
  log_warn "Compilation will still work using the SQLx offline query cache (.sqlx/)"
fi

# ---------------------------------------------------------------------------
# Environment Variables
# ---------------------------------------------------------------------------
log_section "Environment configuration"

# Core project paths (these use the actual project directory, not the
# hardcoded WORKSPACE_PATH from base.env)
persist_env "export WORKSPACE_PATH=\"$PROJECT_DIR\""
persist_env "export TASKER_CONFIG_ROOT=\"$PROJECT_DIR/config\""
persist_env "export TASKER_FIXTURE_PATH=\"$PROJECT_DIR/tests/fixtures\""
persist_env 'export TASKER_ENV="test"'
persist_env 'export RUST_LOG="info"'
persist_env 'export LOG_LEVEL="info"'

# Database
persist_env 'export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"'
persist_env 'export TEST_DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"'

# Messaging: default to pgmq (simpler, no RabbitMQ needed)
# The test.env file defaults to rabbitmq but that requires a separate service
persist_env 'export TASKER_MESSAGING_BACKEND="pgmq"'

# Config and template paths
persist_env "export TASKER_CONFIG_PATH=\"$PROJECT_DIR/config/tasker/generated/complete-test.toml\""
persist_env "export TASKER_TEMPLATE_PATH=\"$PROJECT_DIR/config/tasks\""

# Web API
persist_env 'export WEB_API_ENABLED="true"'
persist_env 'export WEB_API_TLS_ENABLED="false"'
persist_env "export WEB_API_TLS_CERT_PATH=\"$PROJECT_DIR/tests/web/certs/server.crt\""
persist_env "export WEB_API_TLS_KEY_PATH=\"$PROJECT_DIR/tests/web/certs/server.key\""

# Authentication
persist_env 'export WEB_AUTH_ENABLED="true"'
persist_env 'export WEB_JWT_ISSUER="tasker-core-test"'
persist_env 'export WEB_JWT_AUDIENCE="tasker-api-test"'
persist_env 'export WEB_JWT_TOKEN_EXPIRY_HOURS="1"'

# CORS
persist_env 'export WEB_CORS_ENABLED="true"'
persist_env 'export WEB_CORS_ALLOWED_ORIGINS="http://localhost:3000,https://localhost:3000"'

# Rate limiting (disabled for test)
persist_env 'export WEB_RATE_LIMITING_ENABLED="false"'

# Circuit breaker
persist_env 'export WEB_CIRCUIT_BREAKER_ENABLED="true"'

# Resource monitoring
persist_env 'export WEB_RESOURCE_MONITORING_ENABLED="true"'

# Redis/Dragonfly (may not be available)
persist_env 'export REDIS_URL="redis://localhost:6379"'

log_ok "environment variables persisted to session"

# ---------------------------------------------------------------------------
# Generate .env File (via cargo make)
# ---------------------------------------------------------------------------
log_section "Generating .env file"

# Export WORKSPACE_PATH for the setup-env script's variable expansion
export WORKSPACE_PATH="$PROJECT_DIR"

if command_exists cargo-make; then
  cd "$PROJECT_DIR"
  if cargo make setup-env 2>/dev/null; then
    log_ok ".env generated via cargo make setup-env"
  else
    log_warn "cargo make setup-env failed - creating minimal .env"
    # Fallback: create a minimal .env with the essentials
    cat > "$PROJECT_DIR/.env" << ENVEOF
# Generated by setup-claude-web.sh (fallback)
WORKSPACE_PATH=$PROJECT_DIR
TASKER_CONFIG_ROOT=$PROJECT_DIR/config
TASKER_FIXTURE_PATH=$PROJECT_DIR/tests/fixtures
TASKER_ENV=test
RUST_LOG=info
LOG_LEVEL=info
DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test
TEST_DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test
TASKER_MESSAGING_BACKEND=pgmq
TASKER_CONFIG_PATH=$PROJECT_DIR/config/tasker/generated/complete-test.toml
TASKER_TEMPLATE_PATH=$PROJECT_DIR/config/tasks
WEB_API_ENABLED=true
WEB_API_TLS_ENABLED=false
WEB_AUTH_ENABLED=true
WEB_JWT_ISSUER=tasker-core-test
WEB_JWT_AUDIENCE=tasker-api-test
WEB_JWT_TOKEN_EXPIRY_HOURS=1
WEB_CORS_ENABLED=true
WEB_RATE_LIMITING_ENABLED=false
WEB_CIRCUIT_BREAKER_ENABLED=true
WEB_RESOURCE_MONITORING_ENABLED=true
REDIS_URL=redis://localhost:6379
ENVEOF
    log_ok "minimal .env created"
  fi
else
  log_warn "cargo-make not available - .env not generated"
fi

# ---------------------------------------------------------------------------
# Database Migrations (if PostgreSQL is ready)
# ---------------------------------------------------------------------------
if [ "$PG_READY" = true ] && command_exists sqlx; then
  log_section "Database migrations"
  cd "$PROJECT_DIR"
  export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"

  if sqlx migrate run 2>/dev/null; then
    log_ok "migrations applied"
  else
    log_warn "migrations failed - you may need to run them manually"
    log_warn "try: cargo make db-migrate"
  fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
log_section "Setup complete"

echo ""
echo "  Tools installed:"
command_exists protoc     && echo "    protoc:       $(protoc --version 2>/dev/null | head -1 || echo 'yes')"
command_exists cargo      && echo "    cargo:        $(cargo --version 2>/dev/null | awk '{print $2}' || echo 'yes')"
command_exists cargo-make && echo "    cargo-make:   yes"
command_exists sqlx       && echo "    sqlx-cli:     yes"
command_exists cargo-nextest && echo "    cargo-nextest: yes"
command_exists grpcurl    && echo "    grpcurl:      yes"
echo ""

if [ "$PG_READY" = true ]; then
  echo "  PostgreSQL: ready"
else
  echo "  PostgreSQL: NOT available (compilation will use .sqlx/ cache)"
fi

echo ""
echo "  Quick start:"
echo "    cargo make check    # Run all quality checks"
echo "    cargo make build    # Build everything"
if [ "$PG_READY" = true ]; then
  echo "    cargo make test     # Run tests"
fi
echo ""

#!/bin/bash
# =============================================================================
# run-orchestration.sh
# =============================================================================
# Start the orchestration service with proper environment configuration.
# Supports both single-database and split-database modes.
#
# NOTE: This script is called via cargo-make which sets cwd to project root.
#
# Environment:
#   DATABASE_URL          - Required: Tasker database connection
#   PGMQ_DATABASE_URL     - Optional: PGMQ database (split mode)
#   TASKER_ENV            - Optional: Environment name (default: test)
#   TASKER_CONFIG_PATH    - Optional: Config file path
#   PORT                  - Optional: HTTP port (default: 8080)
# =============================================================================

set -euo pipefail

# cargo-make sets cwd to project root
PROJECT_ROOT="$(pwd)"
SCRIPTS_DIR="${PROJECT_ROOT}/cargo-make/scripts"

# TAS-78: Preserve database URLs if already set (for split-database mode)
_SAVED_DATABASE_URL="${DATABASE_URL:-}"
_SAVED_PGMQ_DATABASE_URL="${PGMQ_DATABASE_URL:-}"

# Source .env for additional configuration
if [[ -f ".env" ]]; then
    set -a
    source ".env"
    set +a
fi

# Restore preserved URLs (split-db mode takes precedence)
[[ -n "$_SAVED_DATABASE_URL" ]] && export DATABASE_URL="$_SAVED_DATABASE_URL"
[[ -n "$_SAVED_PGMQ_DATABASE_URL" ]] && export PGMQ_DATABASE_URL="$_SAVED_PGMQ_DATABASE_URL"

export TASKER_ENV="${TASKER_ENV:-test}"
export PORT="${PORT:-8080}"
export TASKER_CONFIG_PATH="${TASKER_CONFIG_PATH:-${PROJECT_ROOT}/config/tasker/complete-test.toml}"

echo "Starting orchestration service..."
echo "   Port: ${PORT}"
echo "   Config: ${TASKER_CONFIG_PATH}"
echo "   Database: ${DATABASE_URL}"

# Use service-start.sh for process management
PROJECT_ROOT="${PROJECT_ROOT}" "${SCRIPTS_DIR}/service-start.sh" orchestration \
    cargo run -p tasker-orchestration --bin tasker-server

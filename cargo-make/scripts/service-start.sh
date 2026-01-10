#!/bin/bash
# =============================================================================
# Service Start Script
# =============================================================================
# Generic script to start a service in the background and record its PID.
#
# Usage: service-start.sh <service-name> <command> [args...]
#
# Environment:
#   PROJECT_ROOT - Project root directory (defaults to pwd)
#   SERVICE_LOG_DIR - Log directory (defaults to $PROJECT_ROOT/.logs)
#
# Example:
#   service-start.sh orchestration cargo run -p tasker-orchestration
#   service-start.sh python-worker cargo run -p tasker-worker --features python
# =============================================================================

set -e

SERVICE_NAME="${1:?Service name required}"
shift
COMMAND="$@"

if [ -z "$COMMAND" ]; then
    echo "❌ Error: Command required"
    echo "Usage: service-start.sh <service-name> <command> [args...]"
    exit 1
fi

# TAS-78: Enable SQLX_OFFLINE in split-database mode
# (SQLx compile-time checking requires single database)
if [ -n "${PGMQ_DATABASE_URL:-}" ] && [ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]; then
    echo "🔀 Split database mode - using SQLX_OFFLINE=true"
    export SQLX_OFFLINE=true
fi

# Setup directories
PROJECT_ROOT="${PROJECT_ROOT:-$(pwd)}"
PID_DIR="${PROJECT_ROOT}/.pids"
LOG_DIR="${SERVICE_LOG_DIR:-${PROJECT_ROOT}/.logs}"

mkdir -p "$PID_DIR"
mkdir -p "$LOG_DIR"

PID_FILE="${PID_DIR}/${SERVICE_NAME}.pid"
LOG_FILE="${LOG_DIR}/${SERVICE_NAME}.log"

# Check if already running
if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        echo "⚠️  ${SERVICE_NAME} is already running (PID: ${OLD_PID})"
        echo "   Use 'cargo make stop-${SERVICE_NAME}' to stop it first"
        exit 1
    else
        # Stale PID file
        rm -f "$PID_FILE"
    fi
fi

echo "🚀 Starting ${SERVICE_NAME}..."
echo "   Command: ${COMMAND}"
echo "   Log: ${LOG_FILE}"

# Start the service in the background
nohup $COMMAND > "$LOG_FILE" 2>&1 &
PID=$!

# Record the PID
echo "$PID" > "$PID_FILE"

# Wait a moment and verify it started
sleep 2
if kill -0 "$PID" 2>/dev/null; then
    echo "✅ ${SERVICE_NAME} started (PID: ${PID})"
else
    echo "❌ ${SERVICE_NAME} failed to start"
    echo "   Check logs: tail -f ${LOG_FILE}"
    rm -f "$PID_FILE"
    exit 1
fi

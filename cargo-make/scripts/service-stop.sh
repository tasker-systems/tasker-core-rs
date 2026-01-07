#!/bin/bash
# =============================================================================
# Service Stop Script
# =============================================================================
# Generic script to stop a running service by its recorded PID.
#
# Usage: service-stop.sh <service-name>
#
# Environment:
#   PROJECT_ROOT - Project root directory (defaults to pwd)
#
# Example:
#   service-stop.sh orchestration
#   service-stop.sh python-worker
# =============================================================================

set -e

SERVICE_NAME="${1:?Service name required}"

# Setup directories
PROJECT_ROOT="${PROJECT_ROOT:-$(pwd)}"
PID_DIR="${PROJECT_ROOT}/.pids"
PID_FILE="${PID_DIR}/${SERVICE_NAME}.pid"

if [ ! -f "$PID_FILE" ]; then
    echo "⚠️  ${SERVICE_NAME} is not running (no PID file)"
    exit 0
fi

PID=$(cat "$PID_FILE")

if ! kill -0 "$PID" 2>/dev/null; then
    echo "⚠️  ${SERVICE_NAME} is not running (stale PID: ${PID})"
    rm -f "$PID_FILE"
    exit 0
fi

echo "🛑 Stopping ${SERVICE_NAME} (PID: ${PID})..."

# Send SIGTERM for graceful shutdown
kill -TERM "$PID" 2>/dev/null || true

# Wait for process to terminate (max 10 seconds)
COUNTER=0
while kill -0 "$PID" 2>/dev/null && [ $COUNTER -lt 10 ]; do
    sleep 1
    COUNTER=$((COUNTER + 1))
    echo "   Waiting for shutdown... (${COUNTER}s)"
done

# Force kill if still running
if kill -0 "$PID" 2>/dev/null; then
    echo "⚠️  Force killing ${SERVICE_NAME}..."
    kill -9 "$PID" 2>/dev/null || true
    sleep 1
fi

# Clean up PID file
rm -f "$PID_FILE"

if kill -0 "$PID" 2>/dev/null; then
    echo "❌ Failed to stop ${SERVICE_NAME}"
    exit 1
else
    echo "✅ ${SERVICE_NAME} stopped"
fi

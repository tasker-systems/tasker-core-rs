#!/bin/bash
# =============================================================================
# TAS-73: Multi-Instance Cluster Stop Script
# =============================================================================
# Stop cluster instances gracefully.
#
# Usage: stop-cluster.sh [service-type]
#
# Arguments:
#   service-type - Optional filter (orchestration, worker-rust, etc.)
#                  If not specified, stops ALL cluster instances.
#
# Examples:
#   stop-cluster.sh                # Stop all instances
#   stop-cluster.sh orchestration  # Stop only orchestration instances
#   stop-cluster.sh worker-rust    # Stop only rust worker instances
# =============================================================================

set -euo pipefail

SERVICE_FILTER="${1:-}"

# Determine project root from script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"

PID_DIR="${PROJECT_ROOT}/.pids"

if [ ! -d "$PID_DIR" ]; then
    echo "No instances to stop (no .pids directory)"
    exit 0
fi

# Collect matching PID files
shopt -s nullglob
PID_FILES=("$PID_DIR"/*.pid)
shopt -u nullglob

if [ ${#PID_FILES[@]} -eq 0 ]; then
    echo "No instances to stop"
    exit 0
fi

STOPPED=0
ALREADY_STOPPED=0

echo "🛑 Stopping cluster instances..."
echo ""

for pid_file in "${PID_FILES[@]}"; do
    INSTANCE=$(basename "$pid_file" .pid)

    # Apply filter if specified (match prefix)
    if [ -n "$SERVICE_FILTER" ] && [[ ! "$INSTANCE" =~ ^${SERVICE_FILTER} ]]; then
        continue
    fi

    PID=$(cat "$pid_file")

    if kill -0 "$PID" 2>/dev/null; then
        echo "   Stopping $INSTANCE (PID $PID)..."

        # Send SIGTERM for graceful shutdown
        kill -TERM "$PID" 2>/dev/null || true

        # Wait up to 10 seconds for graceful shutdown
        WAIT_COUNT=0
        while [ $WAIT_COUNT -lt 20 ]; do
            if ! kill -0 "$PID" 2>/dev/null; then
                break
            fi
            sleep 0.5
            ((WAIT_COUNT++))
        done

        # Force kill if still running
        if kill -0 "$PID" 2>/dev/null; then
            echo "   ⚠️  Force killing $INSTANCE..."
            kill -9 "$PID" 2>/dev/null || true
            sleep 0.5
        fi

        echo "   ✅ $INSTANCE stopped"
        ((STOPPED++))
    else
        echo "   ⚪ $INSTANCE was not running"
        ((ALREADY_STOPPED++))
    fi

    rm -f "$pid_file"
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Stopped: $STOPPED  Already stopped: $ALREADY_STOPPED"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

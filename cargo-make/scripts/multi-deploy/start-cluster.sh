#!/bin/bash
# =============================================================================
# TAS-73: Multi-Instance Cluster Start Script
# =============================================================================
# Start N instances of a service type for multi-instance testing.
#
# Usage: start-cluster.sh <service-type> <count> [base-port]
#
# Service types:
#   orchestration  - Orchestration service instances (default base: 8080)
#   worker-rust    - Rust worker instances (default base: 8100)
#   worker-ruby    - Ruby worker instances (default base: 8200)
#   worker-python  - Python worker instances (default base: 8300)
#   worker-ts      - TypeScript worker instances (default base: 8400)
#
# Environment:
#   PROJECT_ROOT - Project root directory (defaults to script location)
#   DATABASE_URL - PostgreSQL connection URL
#   PGMQ_DATABASE_URL - PGMQ database URL (optional, enables split mode)
#
# Examples:
#   start-cluster.sh orchestration 3          # Start 3 orchestration instances
#   start-cluster.sh worker-rust 2            # Start 2 rust worker instances
#   start-cluster.sh orchestration 2 9000     # Start 2 on ports 9000, 9001
# =============================================================================

set -euo pipefail

SERVICE_TYPE="${1:?Service type required (orchestration, worker-rust, etc.)}"
COUNT="${2:?Instance count required}"
BASE_PORT="${3:-}"

# Determine project root from script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"

# Setup directories
PID_DIR="${PROJECT_ROOT}/.pids"
LOG_DIR="${PROJECT_ROOT}/.logs"
mkdir -p "$PID_DIR" "$LOG_DIR"

# TAS-78: Enable SQLX_OFFLINE in split-database mode
if [ -n "${PGMQ_DATABASE_URL:-}" ] && [ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]; then
    echo "🔀 Split database mode detected - using SQLX_OFFLINE=true"
    export SQLX_OFFLINE=true
fi

# Determine base port and binary/package from service type
case "$SERVICE_TYPE" in
    orchestration)
        BASE_PORT="${BASE_PORT:-8080}"
        PACKAGE="tasker-orchestration"
        BINARY_ARGS="--bin tasker-server"
        ;;
    worker-rust)
        BASE_PORT="${BASE_PORT:-8100}"
        PACKAGE="tasker-worker-rust"
        BINARY_ARGS=""
        ;;
    worker-ruby)
        BASE_PORT="${BASE_PORT:-8200}"
        PACKAGE="workers/ruby"
        BINARY_ARGS=""
        # Ruby worker has different startup mechanism
        echo "❌ Ruby worker cluster not yet implemented"
        exit 1
        ;;
    worker-python)
        BASE_PORT="${BASE_PORT:-8300}"
        PACKAGE="workers/python"
        BINARY_ARGS=""
        # Python worker has different startup mechanism
        echo "❌ Python worker cluster not yet implemented"
        exit 1
        ;;
    worker-ts)
        BASE_PORT="${BASE_PORT:-8400}"
        PACKAGE="workers/typescript"
        BINARY_ARGS=""
        # TypeScript worker has different startup mechanism
        echo "❌ TypeScript worker cluster not yet implemented"
        exit 1
        ;;
    *)
        echo "❌ Unknown service type: $SERVICE_TYPE"
        echo "   Valid types: orchestration, worker-rust, worker-ruby, worker-python, worker-ts"
        exit 1
        ;;
esac

echo "🚀 Starting $COUNT $SERVICE_TYPE instance(s) from base port $BASE_PORT..."
echo ""

STARTED=0
FAILED=0

for i in $(seq 1 "$COUNT"); do
    INSTANCE_ID="${SERVICE_TYPE}-${i}"
    PORT=$((BASE_PORT + i - 1))
    PID_FILE="${PID_DIR}/${INSTANCE_ID}.pid"
    LOG_FILE="${LOG_DIR}/${INSTANCE_ID}.log"

    # Check if instance already running
    if [ -f "$PID_FILE" ]; then
        OLD_PID=$(cat "$PID_FILE")
        if kill -0 "$OLD_PID" 2>/dev/null; then
            echo "⚠️  $INSTANCE_ID: already running (PID $OLD_PID, port $PORT)"
            continue
        fi
        # Stale PID file
        rm -f "$PID_FILE"
    fi

    echo "   Starting $INSTANCE_ID on port $PORT..."

    # Start the instance with instance-specific environment
    TASKER_INSTANCE_ID="$INSTANCE_ID" \
    TASKER_INSTANCE_PORT="$PORT" \
    TASKER_WEB_BIND_ADDRESS="0.0.0.0:$PORT" \
    TASKER_WORKER_ID="$INSTANCE_ID" \
    nohup cargo run --release -p "$PACKAGE" $BINARY_ARGS \
        >> "$LOG_FILE" 2>&1 &

    PID=$!
    echo "$PID" > "$PID_FILE"

    # Brief wait to check if process started
    sleep 1
    if kill -0 "$PID" 2>/dev/null; then
        echo "   ✅ $INSTANCE_ID: started (PID $PID, port $PORT)"
        echo "      Log: $LOG_FILE"
        ((STARTED++))
    else
        echo "   ❌ $INSTANCE_ID: failed to start"
        echo "      Check log: $LOG_FILE"
        rm -f "$PID_FILE"
        ((FAILED++))
    fi
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Started: $STARTED  Failed: $FAILED"
echo ""
echo "Commands:"
echo "  cargo make cluster-status     # Check instance health"
echo "  cargo make cluster-logs       # Tail all instance logs"
echo "  cargo make cluster-stop       # Stop all instances"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

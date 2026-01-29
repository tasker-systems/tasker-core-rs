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

# Determine base port, working directory, and start command from service type
WORKER_DIR=""
START_CMD=""
RUNTIME_TYPE="cargo"  # cargo, bundle, uv, or bun

case "$SERVICE_TYPE" in
    orchestration)
        BASE_PORT="${BASE_PORT:-8080}"
        GRPC_BASE_PORT="${TASKER_ORCHESTRATION_GRPC_BASE_PORT:-9090}"
        START_CMD="cargo run --release -p tasker-orchestration --bin tasker-server"
        RUNTIME_TYPE="cargo"
        ;;
    worker-rust)
        BASE_PORT="${BASE_PORT:-8100}"
        GRPC_BASE_PORT="${TASKER_WORKER_RUST_GRPC_BASE_PORT:-9100}"
        START_CMD="cargo run --release -p tasker-worker-rust"
        RUNTIME_TYPE="cargo"
        ;;
    worker-ruby)
        BASE_PORT="${BASE_PORT:-8200}"
        WORKER_DIR="${PROJECT_ROOT}/workers/ruby"
        START_CMD="bundle exec ruby bin/server.rb"
        RUNTIME_TYPE="bundle"
        ;;
    worker-python)
        BASE_PORT="${BASE_PORT:-8300}"
        WORKER_DIR="${PROJECT_ROOT}/workers/python"
        START_CMD="uv run python bin/server.py"
        RUNTIME_TYPE="uv"
        ;;
    worker-ts)
        BASE_PORT="${BASE_PORT:-8400}"
        WORKER_DIR="${PROJECT_ROOT}/workers/typescript"
        START_CMD="bun run bin/server.ts"
        RUNTIME_TYPE="bun"
        ;;
    *)
        echo "❌ Unknown service type: $SERVICE_TYPE"
        echo "   Valid types: orchestration, worker-rust, worker-ruby, worker-python, worker-ts"
        exit 1
        ;;
esac

# Verify worker directory exists for non-Rust workers
if [ -n "$WORKER_DIR" ] && [ ! -d "$WORKER_DIR" ]; then
    echo "❌ Worker directory not found: $WORKER_DIR"
    exit 1
fi

# Check runtime availability for non-Rust workers
case "$RUNTIME_TYPE" in
    bundle)
        if ! command -v bundle &>/dev/null; then
            echo "❌ bundle not found. Install Ruby and Bundler first."
            exit 1
        fi
        # Ensure dependencies are installed
        echo "📦 Checking Ruby dependencies..."
        (cd "$WORKER_DIR" && bundle check &>/dev/null) || {
            echo "   Installing Ruby dependencies..."
            (cd "$WORKER_DIR" && bundle install)
        }
        # Ensure extension is compiled
        echo "🔨 Checking Ruby extension..."
        (cd "$WORKER_DIR" && bundle exec rake compile 2>/dev/null) || {
            echo "   Compiling Ruby extension..."
            (cd "$WORKER_DIR" && bundle exec rake compile)
        }
        ;;
    uv)
        if ! command -v uv &>/dev/null; then
            echo "❌ uv not found. Install uv (https://github.com/astral-sh/uv) first."
            exit 1
        fi
        # Ensure virtual environment and dependencies
        echo "📦 Checking Python dependencies..."
        (cd "$WORKER_DIR" && uv sync --group dev 2>/dev/null) || {
            echo "   Setting up Python environment..."
            (cd "$WORKER_DIR" && uv venv && uv sync --group dev)
        }
        # Build the maturin extension
        echo "🔨 Checking Python extension..."
        (cd "$WORKER_DIR" && uv run maturin develop 2>/dev/null) || {
            echo "   Building Python extension..."
            (cd "$WORKER_DIR" && uv run maturin develop)
        }
        ;;
    bun)
        if ! command -v bun &>/dev/null; then
            echo "❌ bun not found. Install Bun (https://bun.sh) first."
            exit 1
        fi
        # Ensure dependencies are installed
        echo "📦 Checking TypeScript dependencies..."
        if [ ! -d "$WORKER_DIR/node_modules" ]; then
            echo "   Installing TypeScript dependencies..."
            (cd "$WORKER_DIR" && bun install --frozen-lockfile)
        fi
        # Build the FFI library
        echo "🔨 Checking TypeScript FFI library..."
        (cd "$PROJECT_ROOT" && cargo build -p tasker-worker-ts 2>/dev/null) || {
            echo "   Building TypeScript FFI library..."
            (cd "$PROJECT_ROOT" && cargo build -p tasker-worker-ts)
        }
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
    if [ -n "$WORKER_DIR" ]; then
        # Non-Rust worker: run from worker directory
        (
            cd "$WORKER_DIR"
            # Source worker-specific .env if it exists
            if [ -f ".env" ]; then
                set -a
                source .env
                set +a
            fi
            # Export instance-specific environment
            export TASKER_INSTANCE_ID="$INSTANCE_ID"
            export TASKER_INSTANCE_PORT="$PORT"
            export PORT="$PORT"
            export TASKER_WEB_BIND_ADDRESS="0.0.0.0:$PORT"
            export TASKER_WORKER_ID="$INSTANCE_ID"

            nohup $START_CMD >> "$LOG_FILE" 2>&1 &
            echo $! > "$PID_FILE"
        )
    else
        # Rust service: run from project root with service-specific config
        # Set appropriate config/template paths based on service type
        case "$SERVICE_TYPE" in
            orchestration)
                CONFIG_PATH="${PROJECT_ROOT}/config/tasker/generated/orchestration-test.toml"
                TEMPLATE_PATH="${PROJECT_ROOT}/tests/fixtures/task_templates"
                ;;
            worker-rust)
                CONFIG_PATH="${PROJECT_ROOT}/config/tasker/generated/worker-test.toml"
                TEMPLATE_PATH="${PROJECT_ROOT}/tests/fixtures/task_templates/rust"
                ;;
            *)
                # Default to complete-test for any other rust service
                CONFIG_PATH="${PROJECT_ROOT}/config/tasker/generated/complete-test.toml"
                TEMPLATE_PATH="${PROJECT_ROOT}/tests/fixtures/task_templates"
                ;;
        esac

        # TAS-177: Calculate gRPC port for services that have gRPC enabled
        GRPC_BIND_ENV=""
        if [ "$SERVICE_TYPE" = "orchestration" ]; then
            GRPC_PORT=$((GRPC_BASE_PORT + i - 1))
            export TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS="0.0.0.0:$GRPC_PORT"
            GRPC_BIND_ENV="TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS"
        elif [ "$SERVICE_TYPE" = "worker-rust" ]; then
            GRPC_PORT=$((GRPC_BASE_PORT + i - 1))
            export TASKER_WORKER_GRPC_BIND_ADDRESS="0.0.0.0:$GRPC_PORT"
            GRPC_BIND_ENV="TASKER_WORKER_GRPC_BIND_ADDRESS"
        fi

        TASKER_CONFIG_PATH="$CONFIG_PATH" \
        TASKER_TEMPLATE_PATH="$TEMPLATE_PATH" \
        TASKER_INSTANCE_ID="$INSTANCE_ID" \
        TASKER_INSTANCE_PORT="$PORT" \
        TASKER_WEB_BIND_ADDRESS="0.0.0.0:$PORT" \
        TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS="${TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS:-}" \
        TASKER_WORKER_GRPC_BIND_ADDRESS="${TASKER_WORKER_GRPC_BIND_ADDRESS:-}" \
        TASKER_WORKER_ID="$INSTANCE_ID" \
        nohup $START_CMD >> "$LOG_FILE" 2>&1 &

        PID=$!
        echo "$PID" > "$PID_FILE"
    fi

    # Get PID from file (for non-Rust workers started in subshell)
    PID=$(cat "$PID_FILE" 2>/dev/null || echo "")

    # Brief wait to check if process started
    sleep 1
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        if [ -n "${GRPC_PORT:-}" ]; then
            echo "   ✅ $INSTANCE_ID: started (PID $PID, REST port $PORT, gRPC port $GRPC_PORT)"
        else
            echo "   ✅ $INSTANCE_ID: started (PID $PID, port $PORT)"
        fi
        echo "      Log: $LOG_FILE"
        ((STARTED++)) || true
    else
        echo "   ❌ $INSTANCE_ID: failed to start"
        echo "      Check log: $LOG_FILE"
        rm -f "$PID_FILE"
        ((FAILED++)) || true
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

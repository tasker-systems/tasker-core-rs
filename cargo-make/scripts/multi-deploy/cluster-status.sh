#!/bin/bash
# =============================================================================
# TAS-73: Multi-Instance Cluster Status Script
# =============================================================================
# Display status of all running cluster instances including health checks.
#
# Usage: cluster-status.sh [--no-health]
#
# Options:
#   --no-health  Skip HTTP health checks (faster, just checks if process running)
#
# Output includes:
#   - Instance ID
#   - Process status (running/stopped)
#   - PID
#   - Port
#   - Health status (healthy/starting/unhealthy)
# =============================================================================

set -euo pipefail

SKIP_HEALTH=false
if [ "${1:-}" = "--no-health" ]; then
    SKIP_HEALTH=true
fi

# Determine project root from script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"

PID_DIR="${PROJECT_ROOT}/.pids"

if [ ! -d "$PID_DIR" ]; then
    echo "No instances found (no .pids directory)"
    echo ""
    echo "Start instances with: cargo make cluster-start"
    exit 0
fi

# Collect PID files
shopt -s nullglob
PID_FILES=("$PID_DIR"/*.pid)
shopt -u nullglob

if [ ${#PID_FILES[@]} -eq 0 ]; then
    echo "No instances found"
    echo ""
    echo "Start instances with: cargo make cluster-start"
    exit 0
fi

echo ""
echo "┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓"
echo "┃                        Cluster Instance Status                          ┃"
echo "┣━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━┳━━━━━━━━━━┳━━━━━━━━┳━━━━━━━━━━━━━━━━━┫"
printf "┃ %-20s ┃ %-10s ┃ %-8s ┃ %-6s ┃ %-15s ┃\n" "INSTANCE" "STATUS" "PID" "PORT" "HEALTH"
echo "┣━━━━━━━━━━━━━━━━━━━━━━╋━━━━━━━━━━━━╋━━━━━━━━━━╋━━━━━━━━╋━━━━━━━━━━━━━━━━━┫"

RUNNING=0
STOPPED=0
HEALTHY=0
UNHEALTHY=0

for pid_file in "${PID_FILES[@]}"; do
    INSTANCE=$(basename "$pid_file" .pid)
    PID=$(cat "$pid_file")

    # Derive port from instance ID
    # Format: service-type-N where N is 1-based
    # Port = BASE_PORT + (N - 1)
    case "$INSTANCE" in
        orchestration-*)
            BASE=8080
            NUM="${INSTANCE##*-}"
            PORT=$((BASE + NUM - 1))
            ;;
        worker-rust-*)
            BASE=8100
            NUM="${INSTANCE##*-}"
            PORT=$((BASE + NUM - 1))
            ;;
        worker-ruby-*)
            BASE=8200
            NUM="${INSTANCE##*-}"
            PORT=$((BASE + NUM - 1))
            ;;
        worker-python-*)
            BASE=8300
            NUM="${INSTANCE##*-}"
            PORT=$((BASE + NUM - 1))
            ;;
        worker-ts-*)
            BASE=8400
            NUM="${INSTANCE##*-}"
            PORT=$((BASE + NUM - 1))
            ;;
        # Legacy single-instance names (backward compatibility)
        orchestration)
            PORT=8080
            ;;
        rust-worker)
            PORT=8081
            ;;
        ruby-worker)
            PORT=8082
            ;;
        python-worker)
            PORT=8083
            ;;
        ts-worker|typescript-worker)
            PORT=8085
            ;;
        *)
            PORT="?"
            ;;
    esac

    # Check process status
    if kill -0 "$PID" 2>/dev/null; then
        STATUS="🟢 running"
        ((RUNNING++))

        # Check health endpoint if enabled
        if [ "$SKIP_HEALTH" = false ] && [ "$PORT" != "?" ]; then
            HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
                --connect-timeout 2 --max-time 5 \
                "http://localhost:$PORT/health" 2>/dev/null || echo "000")

            if [ "$HTTP_CODE" = "200" ]; then
                HEALTH="🟢 healthy"
                ((HEALTHY++))
            elif [ "$HTTP_CODE" = "000" ]; then
                HEALTH="🟡 starting"
            else
                HEALTH="🔴 unhealthy"
                ((UNHEALTHY++))
            fi
        else
            HEALTH="-"
        fi
    else
        STATUS="🔴 stopped"
        HEALTH="-"
        ((STOPPED++))
        # Clean up stale PID file
        rm -f "$pid_file"
    fi

    printf "┃ %-20s ┃ %-10s ┃ %-8s ┃ %-6s ┃ %-15s ┃\n" \
        "$INSTANCE" "$STATUS" "$PID" "$PORT" "$HEALTH"
done

echo "┗━━━━━━━━━━━━━━━━━━━━━━┻━━━━━━━━━━━━┻━━━━━━━━━━┻━━━━━━━━┻━━━━━━━━━━━━━━━━━┛"
echo ""
echo "Summary: $RUNNING running, $STOPPED stopped"
if [ "$SKIP_HEALTH" = false ]; then
    echo "Health:  $HEALTHY healthy, $UNHEALTHY unhealthy"
fi
echo ""

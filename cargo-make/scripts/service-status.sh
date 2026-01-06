#!/bin/bash
# =============================================================================
# Service Status Script
# =============================================================================
# Generic script to check the status of a running service.
#
# Usage: service-status.sh <service-name>
#        service-status.sh --all
#
# Environment:
#   PROJECT_ROOT - Project root directory (defaults to pwd)
#
# Example:
#   service-status.sh orchestration
#   service-status.sh --all
# =============================================================================

SERVICE_NAME="$1"

# Setup directories
PROJECT_ROOT="${PROJECT_ROOT:-$(pwd)}"
PID_DIR="${PROJECT_ROOT}/.pids"
LOG_DIR="${PROJECT_ROOT}/.logs"

# Function to check a single service
check_service() {
    local name="$1"
    local pid_file="${PID_DIR}/${name}.pid"
    local log_file="${LOG_DIR}/${name}.log"

    if [ ! -f "$pid_file" ]; then
        printf "%-20s ⚪ not running\n" "${name}:"
        return 1
    fi

    local pid=$(cat "$pid_file")

    if kill -0 "$pid" 2>/dev/null; then
        # Try to get port from logs or process
        local port=""
        if [ -f "$log_file" ]; then
            port=$(grep -oE "listening on.*:([0-9]+)" "$log_file" 2>/dev/null | tail -1 | grep -oE "[0-9]+" | tail -1 || true)
        fi

        if [ -n "$port" ]; then
            printf "%-20s 🟢 running (PID: %s, Port: %s)\n" "${name}:" "$pid" "$port"
        else
            printf "%-20s 🟢 running (PID: %s)\n" "${name}:" "$pid"
        fi
        return 0
    else
        printf "%-20s 🔴 dead (stale PID: %s)\n" "${name}:" "$pid"
        return 1
    fi
}

# Check all services
if [ "$SERVICE_NAME" = "--all" ] || [ -z "$SERVICE_NAME" ]; then
    echo "=== Tasker Service Status ==="
    echo ""

    # Define known services
    SERVICES="orchestration rust-worker ruby-worker python-worker typescript-worker"

    RUNNING=0
    STOPPED=0

    for service in $SERVICES; do
        if check_service "$service"; then
            RUNNING=$((RUNNING + 1))
        else
            STOPPED=$((STOPPED + 1))
        fi
    done

    echo ""
    echo "Summary: ${RUNNING} running, ${STOPPED} stopped"

    # Also check Docker services
    echo ""
    echo "=== Docker Services ==="
    if command -v docker &> /dev/null; then
        docker ps --format "table {{.Names}}\t{{.Status}}" 2>/dev/null | grep -E "postgres|observability|tasker" || echo "No Docker containers running"
    else
        echo "Docker not available"
    fi
else
    check_service "$SERVICE_NAME"
fi

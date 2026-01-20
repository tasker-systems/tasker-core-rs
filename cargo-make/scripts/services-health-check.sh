#!/bin/bash
# =============================================================================
# services-health-check.sh
# =============================================================================
# Check health endpoints for all Tasker services.
# Uses port configuration from worker .env files when available.
#
# Usage:
#   ./services-health-check.sh           # Check all services
#   ./services-health-check.sh --quiet   # Return exit code only (no output)
#
# Exit codes:
#   0 - All services healthy
#   1 - One or more services unhealthy
# =============================================================================

set -euo pipefail

QUIET=false
if [[ "${1:-}" == "--quiet" ]]; then
    QUIET=true
fi

log() {
    if [[ "$QUIET" == "false" ]]; then
        echo "$@"
    fi
}

log "Checking service health endpoints..."
log ""

# Service definitions: name|default_port|.env path
services=(
    "Orchestration|8080|"
    "Rust Worker|8081|workers/rust/.env"
    "Ruby Worker|8082|workers/ruby/.env"
    "Python Worker|8083|workers/python/.env"
    "TypeScript Worker|8085|workers/typescript/.env"
)

all_healthy=true
failed_count=0
success_count=0

for service in "${services[@]}"; do
    IFS='|' read -r name port env_file <<< "$service"

    # Try to get port from .env if specified
    if [[ -n "$env_file" ]] && [[ -f "$env_file" ]]; then
        env_port=$(grep "^PORT=" "$env_file" 2>/dev/null | cut -d'=' -f2 | tr -d '"' | tr -d "'" || true)
        if [[ -n "$env_port" ]]; then
            port="$env_port"
        fi
    fi

    url="http://localhost:${port}/health"

    if [[ "$QUIET" == "false" ]]; then
        printf "%-20s " "$name (${port}):"
    fi

    # Check health endpoint with timeout
    response=$(curl -s --connect-timeout 2 --max-time 5 "$url" 2>/dev/null || true)

    if [[ -n "$response" ]]; then
        # Parse status using jq if available, otherwise grep
        if command -v jq &> /dev/null; then
            status=$(echo "$response" | jq -r '.status // "unknown"' 2>/dev/null || echo "unknown")
        else
            status=$(echo "$response" | grep -o '"status":"[^"]*"' | cut -d'"' -f4 || echo "unknown")
        fi

        if [[ "$status" == "healthy" ]]; then
            log "healthy"
            ((success_count++))
        else
            log "status: $status"
            all_healthy=false
            ((failed_count++))
        fi
    else
        log "not responding"
        all_healthy=false
        ((failed_count++))
    fi
done

log ""
log "────────────────────────────────"
log "Summary: $success_count healthy, $failed_count failed"

if [[ "$all_healthy" == "true" ]]; then
    log "All services healthy"
    exit 0
else
    log "Some services not healthy"
    exit 1
fi

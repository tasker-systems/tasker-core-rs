#!/bin/bash
# =============================================================================
# check-db-split.sh
# =============================================================================
# Check connectivity to both Tasker and PGMQ databases in split-db mode.
#
# Usage:
#   ./check-db-split.sh
#
# Environment:
#   DATABASE_URL      - Required: Tasker database connection
#   PGMQ_DATABASE_URL - Optional: PGMQ database (if split mode)
#
# Exit codes:
#   0 - All databases reachable
#   1 - One or more databases unreachable
# =============================================================================

set -euo pipefail

echo "Checking database connectivity..."

# Check Tasker database
echo -n "Tasker DB: "
if pg_isready -d "${DATABASE_URL}" > /dev/null 2>&1; then
    echo "connected"
else
    echo "not reachable"
    exit 1
fi

# Check PGMQ database (if set)
if [[ -n "${PGMQ_DATABASE_URL:-}" ]]; then
    echo -n "PGMQ DB:   "
    if pg_isready -d "${PGMQ_DATABASE_URL}" > /dev/null 2>&1; then
        echo "connected"
    else
        echo "not reachable"
        exit 1
    fi
fi

echo ""
echo "All databases reachable"

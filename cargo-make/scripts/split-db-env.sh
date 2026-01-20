#!/bin/bash
# =============================================================================
# split-db-env.sh
# =============================================================================
# Shared utility for split-database mode detection and SQLX_OFFLINE setup.
# Source this file to automatically configure SQLX_OFFLINE in split-db mode.
#
# Usage (source for side effects):
#   source "cargo-make/scripts/split-db-env.sh"
#   # SQLX_OFFLINE will be set if split-db mode detected
#
# Usage (check only):
#   if "cargo-make/scripts/split-db-env.sh" --check; then
#       echo "Split-db mode active"
#   fi
#
# Split-database mode is detected when:
#   1. PGMQ_DATABASE_URL is set AND
#   2. PGMQ_DATABASE_URL differs from DATABASE_URL
#
# See: docs/ticket-specs/TAS-78/split-database-design.md
# =============================================================================

is_split_db_mode() {
    [[ -n "${PGMQ_DATABASE_URL:-}" ]] && [[ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]]
}

# If called with --check, just return exit code
if [[ "${1:-}" == "--check" ]]; then
    if is_split_db_mode; then
        exit 0
    else
        exit 1
    fi
fi

# When sourced, configure environment
if is_split_db_mode; then
    echo "Split database mode detected - using SQLX_OFFLINE=true"
    export SQLX_OFFLINE=true
fi

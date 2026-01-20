#!/bin/bash
# =============================================================================
# migrate-db-split.sh
# =============================================================================
# Run migrations in split database mode (requires PGMQ_DATABASE_URL).
#
# NOTE: This script is called via cargo-make which sets cwd to project root.
#
# Environment:
#   DATABASE_URL      - Required: Tasker database connection
#   PGMQ_DATABASE_URL - Required: PGMQ database connection
#
# Exit codes:
#   0 - Migrations successful
#   1 - PGMQ_DATABASE_URL not set or migrations failed
# =============================================================================

set -euo pipefail

# cargo-make sets cwd to project root
SCRIPTS_DIR="$(pwd)/cargo-make/scripts"

if [[ -z "${PGMQ_DATABASE_URL:-}" ]]; then
    echo "PGMQ_DATABASE_URL not set"
    echo ""
    echo "Usage:"
    echo "  PGMQ_DATABASE_URL=postgresql://... cargo make db-migrate-split"
    echo ""
    echo "Or use docker-compose for dual-database testing:"
    echo "  docker compose -f docker/docker-compose.dual-pg.test.yml up -d"
    echo "  source docker/dual-pg.env"
    echo "  cargo make db-migrate-split"
    exit 1
fi

echo "Running split database migrations..."
"${SCRIPTS_DIR}/run-migrations.sh"

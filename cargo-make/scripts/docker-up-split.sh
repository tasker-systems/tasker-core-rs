#!/usr/bin/env bash
# TAS-78: Dual-Database Docker Setup
#
# Starts the dual-database Docker configuration for split-database testing.
# Creates two databases:
#   - tasker_split_test: Tasker tables (NO pgmq extension)
#   - pgmq_split_test: PGMQ queues only
#
# Usage:
#   cargo make docker-up-split

set -euo pipefail

# Source container runtime detection (cargo-make runs from project root)
source "cargo-make/scripts/docker-env.sh"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}🐳 Starting dual-database Docker services...${NC}"
$COMPOSE_CMD -f docker/docker-compose.dual-pg.test.yml up -d

echo "⏳ Waiting for PostgreSQL to be ready..."
sleep 8

# Source the environment if not already set
if [ -z "${PGMQ_DATABASE_URL:-}" ]; then
    echo "📋 Loading dual-pg environment..."
    # shellcheck source=/dev/null
    source docker/dual-pg.env
fi

cargo make db-check-split

echo ""
echo -e "${GREEN}✓ Dual-database setup ready!${NC}"
echo ""
echo "To configure your shell, run:"
echo "  source docker/dual-pg.env"

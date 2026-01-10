#!/usr/bin/env bash
# TAS-78: Docker Services Startup
#
# Auto-detects split-database mode when PGMQ_DATABASE_URL is set
# and different from DATABASE_URL.
#
# Usage:
#   cargo make docker-up           # Auto-detects mode
#   cargo make docker-up-split     # Explicit split mode

set -euo pipefail

# Source container runtime detection (cargo-make runs from project root)
source "cargo-make/scripts/docker-env.sh"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Auto-detect split mode
if [ -n "${PGMQ_DATABASE_URL:-}" ] && [ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]; then
    echo -e "${YELLOW}🐳 Starting Docker services (split-database mode)...${NC}"
    $COMPOSE_CMD -f docker/docker-compose.dual-pg.test.yml up -d
    echo "⏳ Waiting for PostgreSQL..."
    sleep 5
    cargo make db-check-split
else
    echo -e "${YELLOW}🐳 Starting Docker services (single-database mode)...${NC}"
    $COMPOSE_CMD up -d postgres
    echo "⏳ Waiting for PostgreSQL..."
    sleep 3
    cargo make db-check
fi

echo -e "${GREEN}✓ Docker services started${NC}"

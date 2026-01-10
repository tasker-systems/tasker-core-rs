#!/usr/bin/env bash
# TAS-78: Docker Services Shutdown
#
# Auto-detects split-database mode when PGMQ_DATABASE_URL is set
# and different from DATABASE_URL.

set -euo pipefail

# Source container runtime detection (cargo-make runs from project root)
source "cargo-make/scripts/docker-env.sh"

# Colors for output
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Auto-detect split mode
if [ -n "${PGMQ_DATABASE_URL:-}" ] && [ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]; then
    echo -e "${YELLOW}🐳 Stopping Docker services (split-database mode)...${NC}"
    $COMPOSE_CMD -f docker/docker-compose.dual-pg.test.yml down
else
    echo -e "${YELLOW}🐳 Stopping Docker services (single-database mode)...${NC}"
    $COMPOSE_CMD down
fi

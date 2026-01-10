#!/usr/bin/env bash
# TAS-78: Database Migration Script
#
# Supports both single-database and split-database modes:
#
# Single Database Mode (default):
#   DATABASE_URL=postgresql://... cargo make db-migrate
#
# Split Database Mode:
#   DATABASE_URL=postgresql://tasker_db/...
#   PGMQ_DATABASE_URL=postgresql://pgmq_db/...
#   cargo make db-migrate
#
# The script automatically detects split mode when PGMQ_DATABASE_URL is set
# and different from DATABASE_URL.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}📦 Running database migrations...${NC}"

# Check if we're in split database mode
if [ -n "${PGMQ_DATABASE_URL:-}" ] && [ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]; then
    echo -e "${YELLOW}🔀 Split database mode detected${NC}"
    echo "   Tasker DB: ${DATABASE_URL:-<not set>}"
    echo "   PGMQ DB:   ${PGMQ_DATABASE_URL}"
    echo ""

    # Run PGMQ migrations first (extensions needed by Tasker)
    echo -e "${BLUE}📦 Running PGMQ migrations...${NC}"
    DATABASE_URL="${PGMQ_DATABASE_URL}" sqlx migrate run --source migrations/pgmq
    echo -e "${GREEN}✓ PGMQ migrations complete${NC}"
    echo ""

    # Run Tasker migrations
    echo -e "${BLUE}📦 Running Tasker migrations...${NC}"
    sqlx migrate run --source migrations/tasker
    echo -e "${GREEN}✓ Tasker migrations complete${NC}"
else
    echo -e "${YELLOW}📦 Single database mode${NC}"
    echo "   Database: ${DATABASE_URL:-<not set>}"
    echo ""

    # Run combined migrations
    sqlx migrate run
    echo -e "${GREEN}✓ Migrations complete${NC}"
fi

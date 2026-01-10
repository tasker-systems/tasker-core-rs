#!/usr/bin/env bash
# TAS-78: Database Connectivity Check
#
# Supports both single-database and split-database modes:
# - Single mode: Checks DATABASE_URL
# - Split mode: Checks both DATABASE_URL and PGMQ_DATABASE_URL
#
# Auto-detects split mode when PGMQ_DATABASE_URL is set and different.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

echo -e "🔍 Checking database connectivity..."

# Check if we're in split database mode
if [ -n "${PGMQ_DATABASE_URL:-}" ] && [ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]; then
    echo -e "${YELLOW}🔀 Split database mode detected${NC}"

    # Check Tasker database
    echo -n "Tasker DB: "
    if pg_isready -d "${DATABASE_URL}" > /dev/null 2>&1; then
        echo -e "${GREEN}✓ connected${NC}"
    else
        echo -e "${RED}✗ not reachable${NC}"
        echo "  DATABASE_URL: ${DATABASE_URL:-<not set>}"
        exit 1
    fi

    # Check PGMQ database
    echo -n "PGMQ DB:   "
    if pg_isready -d "${PGMQ_DATABASE_URL}" > /dev/null 2>&1; then
        echo -e "${GREEN}✓ connected${NC}"
    else
        echo -e "${RED}✗ not reachable${NC}"
        echo "  PGMQ_DATABASE_URL: ${PGMQ_DATABASE_URL}"
        exit 1
    fi

    echo ""
    echo -e "${GREEN}✓ All databases reachable${NC}"
else
    echo -e "${YELLOW}📦 Single database mode${NC}"

    # Use DATABASE_URL if set, otherwise fall back to pg_isready defaults
    if [ -n "${DATABASE_URL:-}" ]; then
        echo -n "Database: "
        if pg_isready -d "${DATABASE_URL}" > /dev/null 2>&1; then
            echo -e "${GREEN}✓ connected${NC}"
        else
            echo -e "${RED}✗ not reachable${NC}"
            echo "  DATABASE_URL: ${DATABASE_URL}"
            echo "  Start it with: docker-compose up -d postgres"
            exit 1
        fi
    else
        # Fall back to localhost check
        if pg_isready -h localhost -p "${PGPORT:-5432}" -U "${PGUSER:-tasker}" > /dev/null 2>&1; then
            echo -e "${GREEN}✓ Database is ready${NC}"
        else
            echo -e "${RED}✗ Database is not available${NC}"
            echo "  Start it with: docker-compose up -d postgres"
            exit 1
        fi
    fi
fi

#!/usr/bin/env bash
# TAS-78: Full Split-Database Development Setup
#
# Sets up a complete split-database development environment:
# 1. Starts dual-database Docker containers
# 2. Runs migrations on both databases
# 3. Sets up workers
#
# Usage:
#   cargo make setup-split

set -euo pipefail

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🔧 Setting up split-database development environment...${NC}"
echo ""

# Start dual-pg docker
cargo make docker-up-split

# Source environment
echo -e "${YELLOW}📋 Loading dual-pg environment...${NC}"
# shellcheck source=/dev/null
source docker/dual-pg.env

# Run migrations
cargo make db-migrate-split

# Setup workers
cargo make setup-workers

echo ""
echo -e "${GREEN}✓ Split-database development environment ready!${NC}"
echo ""
echo "Remember to source the environment in your shell:"
echo "  source docker/dual-pg.env"

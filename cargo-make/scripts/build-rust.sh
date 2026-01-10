#!/usr/bin/env bash
# TAS-78: Rust Build Script
#
# Handles split-database mode by enabling SQLX_OFFLINE when PGMQ uses
# a separate database (compile-time verification can only use one DB).
#
# Usage:
#   cargo make build-rust

set -euo pipefail

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Check if we're in split database mode
if [ -n "${PGMQ_DATABASE_URL:-}" ] && [ "${PGMQ_DATABASE_URL}" != "${DATABASE_URL:-}" ]; then
    echo -e "${YELLOW}🔀 Split database mode detected - using SQLX_OFFLINE=true${NC}"
    echo "   (SQLx compile-time checking requires single database)"
    export SQLX_OFFLINE=true
fi

echo -e "${YELLOW}🔨 Building Rust crates...${NC}"
cargo build --all-features

echo -e "${GREEN}✓ Rust build complete${NC}"

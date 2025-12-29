#!/usr/bin/env bash
set -euo pipefail

# Setup environment variables for local development
# Mirrors .github/actions/setup-env but for local use

echo "📦 Setting up environment..."

# Load .env if exists
if [ -f ".env" ]; then
    echo "  Loading .env file..."
    set -a
    source .env
    set +a
fi

# Set defaults if not already set
export DATABASE_URL="${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_rust_test}"
export TASKER_ENV="${TASKER_ENV:-test}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export RUST_LOG="${RUST_LOG:-warn}"

echo "  DATABASE_URL=${DATABASE_URL}"
echo "  TASKER_ENV=${TASKER_ENV}"
echo "✓ Environment configured"

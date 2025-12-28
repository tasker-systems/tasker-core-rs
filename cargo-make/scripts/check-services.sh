#!/usr/bin/env bash
set -euo pipefail

# Check required services for testing
# Ports align with docker-compose and start-native-services.sh

POSTGRES_PORT="${POSTGRES_PORT:-5432}"
ORCHESTRATION_PORT="${ORCHESTRATION_PORT:-8080}"
RUST_WORKER_PORT="${RUST_WORKER_PORT:-8081}"
RUBY_WORKER_PORT="${RUBY_WORKER_PORT:-8082}"
PYTHON_WORKER_PORT="${PYTHON_WORKER_PORT:-8083}"
TYPESCRIPT_WORKER_PORT="${TYPESCRIPT_WORKER_PORT:-8085}"

echo "🔍 Checking services..."

# PostgreSQL (required)
if pg_isready -h localhost -p "$POSTGRES_PORT" > /dev/null 2>&1; then
    echo "✓ PostgreSQL is running on port $POSTGRES_PORT"
else
    echo "✗ PostgreSQL is not running on port $POSTGRES_PORT"
    echo "  Start with: docker-compose up -d postgres"
    exit 1
fi

# Optional service checks
check_service() {
    local name="$1"
    local port="$2"
    if curl -sf "http://localhost:$port/health" > /dev/null 2>&1; then
        echo "✓ $name is running on port $port"
    else
        echo "⚠ $name is not running on port $port (optional)"
    fi
}

if [ "${CHECK_ALL_SERVICES:-false}" = "true" ]; then
    check_service "Orchestration" "$ORCHESTRATION_PORT"
    check_service "Rust Worker" "$RUST_WORKER_PORT"
    check_service "Ruby Worker" "$RUBY_WORKER_PORT"
    check_service "Python Worker" "$PYTHON_WORKER_PORT"
    check_service "TypeScript Worker" "$TYPESCRIPT_WORKER_PORT"
fi

echo "✓ Service check complete"

#!/usr/bin/env bash
set -euo pipefail

echo "🔍 Checking database connectivity..."
if pg_isready -h localhost -p "${PGPORT:-5432}" -U "${PGUSER:-tasker}"; then
    echo "✓ Database is ready"
else
    echo "✗ Database is not available"
    echo "  Start it with: docker-compose up -d postgres"
    exit 1
fi

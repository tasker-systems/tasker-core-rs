#!/usr/bin/env bash
set -euo pipefail

echo "⚠️  Resetting database..."
sqlx database drop -y 2>/dev/null || true
sqlx database create
sqlx migrate run
echo "✓ Database reset complete"

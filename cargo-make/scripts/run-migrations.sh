#!/usr/bin/env bash
set -euo pipefail

echo "📦 Running migrations..."
sqlx migrate run
echo "✓ Migrations complete"

#!/usr/bin/env bash
set -euo pipefail

echo "📦 Preparing SQLX cache..."
cargo sqlx prepare --workspace -- --all-targets --all-features
echo "✓ SQLX cache prepared"
echo ""
echo "Don't forget to commit the .sqlx directories!"

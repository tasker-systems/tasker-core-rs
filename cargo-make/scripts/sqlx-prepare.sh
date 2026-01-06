#!/usr/bin/env bash
set -euo pipefail

echo "📦 Preparing SQLX cache for all crates..."
echo ""

# List of crates that use SQLX and need cache generation
CRATES=(
  "."
  "tasker-shared"
  "tasker-orchestration"
  "tasker-worker"
  "tasker-client"
  "pgmq-notify"
  "workers/ruby/ext/tasker_core"
  "workers/rust"
  "workers/python"
  "workers/typescript"
)

# Store the workspace root
WORKSPACE_ROOT=$(pwd)

for crate in "${CRATES[@]}"; do
  if [ "$crate" = "." ]; then
    echo "📦 Preparing SQLX cache for workspace root..."
    cargo sqlx prepare -- --all-targets --all-features
  else
    echo "📦 Preparing SQLX cache for $crate..."
    (cd "$crate" && cargo sqlx prepare -- --all-targets --all-features)
  fi
  echo "✓ SQLX cache prepared for $crate"
  echo ""
done

echo "✅ All SQLX caches prepared"
echo ""
echo "Don't forget to commit the .sqlx directories!"

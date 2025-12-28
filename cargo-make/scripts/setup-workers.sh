#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "📦 Setting up workers..."

echo "  → Python worker..."
(cd "$WORKSPACE_ROOT/workers/python" && cargo make setup)

echo "  → Ruby worker..."
(cd "$WORKSPACE_ROOT/workers/ruby" && cargo make setup)

echo "  → TypeScript worker..."
(cd "$WORKSPACE_ROOT/workers/typescript" && cargo make setup)

echo "✓ All workers setup"

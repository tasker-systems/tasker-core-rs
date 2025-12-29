#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "🧹 Cleaning workers..."

(cd "$WORKSPACE_ROOT/workers/python" && cargo make clean) 2>/dev/null || true
(cd "$WORKSPACE_ROOT/workers/ruby" && cargo make clean) 2>/dev/null || true
(cd "$WORKSPACE_ROOT/workers/typescript" && cargo make clean) 2>/dev/null || true

echo "✓ Workers cleaned"

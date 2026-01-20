#!/bin/bash
# =============================================================================
# services-start-all.sh
# =============================================================================
# Start all Tasker services (orchestration + all workers).
#
# Usage:
#   ./services-start-all.sh
#
# Port Mapping:
#   Orchestration:     8080
#   Rust Worker:       8081
#   Ruby Worker:       8082
#   Python Worker:     8083
#   TypeScript Worker: 8085
#
# Prerequisites:
#   - PostgreSQL running (via docker-compose or locally)
#   - Run 'cargo make build' first to build all services
# =============================================================================

set -euo pipefail

echo "Starting all Tasker services..."
echo ""

# Start orchestration first
echo "=== Starting Orchestration ==="
cargo make run-orchestration
echo ""

# Start workers in parallel by launching them sequentially
# (each returns quickly after backgrounding)
echo "=== Starting Workers ==="
cargo make run-worker-rust
cargo make run-worker-python
cargo make run-worker-ruby
cargo make run-worker-typescript

echo ""
echo "Waiting for services to initialize..."
sleep 5

echo ""
cargo make services-status

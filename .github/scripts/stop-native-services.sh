#!/usr/bin/env bash
set -euo pipefail

echo "🛑 Stopping native services..."

if [ -f .pids/orchestration.pid ]; then
  kill $(cat .pids/orchestration.pid) 2>/dev/null || true
  echo "Stopped orchestration service"
fi

if [ -f .pids/worker.pid ]; then
  kill $(cat .pids/worker.pid) 2>/dev/null || true
  echo "Stopped Rust worker"
fi

if [ -f .pids/ruby-worker.pid ]; then
  kill $(cat .pids/ruby-worker.pid) 2>/dev/null || true
  echo "Stopped Ruby worker"
fi

rm -rf .pids
echo "✅ All services stopped"

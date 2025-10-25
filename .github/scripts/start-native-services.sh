#!/usr/bin/env bash
set -euo pipefail

# Configuration
POSTGRES_URL="${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_rust_test}"
CONFIG_PATH="${TASKER_CONFIG_PATH:-$(pwd)/config/tasker/complete-test.toml}"
ORCHESTRATION_PORT="${ORCHESTRATION_PORT:-8080}"
WORKER_PORT="${WORKER_PORT:-8081}"
RUBY_WORKER_PORT="${RUBY_WORKER_PORT:-8082}"

echo "🚀 Starting native services..."

# 1. Run database migrations
echo "📊 Running database migrations..."
DATABASE_URL="$POSTGRES_URL" cargo sqlx migrate run

# 2. Start orchestration service in background
echo "🎯 Starting orchestration service on port $ORCHESTRATION_PORT..."
TASKER_CONFIG_PATH="$CONFIG_PATH" \
  DATABASE_URL="$POSTGRES_URL" \
  RUST_LOG=info \
  target/debug/tasker-server \
  --port "$ORCHESTRATION_PORT" \
  > orchestration.log 2>&1 &
ORCHESTRATION_PID=$!
echo "Orchestration PID: $ORCHESTRATION_PID"

# 3. Start Rust worker in background
echo "⚙️  Starting Rust worker on port $WORKER_PORT..."
TASKER_CONFIG_PATH="$CONFIG_PATH" \
  DATABASE_URL="$POSTGRES_URL" \
  RUST_LOG=info \
  target/debug/rust-worker \
  --port "$WORKER_PORT" \
  > worker.log 2>&1 &
WORKER_PID=$!
echo "Worker PID: $WORKER_PID"

# 4. Start Ruby FFI worker in background
echo "💎 Starting Ruby FFI worker on port $RUBY_WORKER_PORT..."
cd workers/ruby
TASKER_CONFIG_PATH="$CONFIG_PATH" \
  DATABASE_URL="$POSTGRES_URL" \
  TASKER_ENV=test \
  bundle exec bin/server.rb \
  --port "$RUBY_WORKER_PORT" \
  > ../../ruby-worker.log 2>&1 &
RUBY_WORKER_PID=$!
echo "Ruby worker PID: $RUBY_WORKER_PID"
cd ../..

# 5. Wait for health checks
echo "🏥 Waiting for services to be healthy..."
timeout 60 bash -c "
  until curl -sf http://localhost:$ORCHESTRATION_PORT/health > /dev/null; do
    echo '⏳ Waiting for orchestration...'
    sleep 2
  done
  echo '✅ Orchestration ready'

  until curl -sf http://localhost:$WORKER_PORT/health > /dev/null; do
    echo '⏳ Waiting for Rust worker...'
    sleep 2
  done
  echo '✅ Rust worker ready'

  until curl -sf http://localhost:$RUBY_WORKER_PORT/health > /dev/null; do
    echo '⏳ Waiting for Ruby worker...'
    sleep 2
  done
  echo '✅ Ruby worker ready'
"

# 6. Save PIDs for cleanup
mkdir -p .pids
echo "$ORCHESTRATION_PID" > .pids/orchestration.pid
echo "$WORKER_PID" > .pids/worker.pid
echo "$RUBY_WORKER_PID" > .pids/ruby-worker.pid

echo "✅ All services started and healthy!"
echo "   - Orchestration: http://localhost:$ORCHESTRATION_PORT (PID $ORCHESTRATION_PID)"
echo "   - Rust Worker:   http://localhost:$WORKER_PORT (PID $WORKER_PID)"
echo "   - Ruby Worker:   http://localhost:$RUBY_WORKER_PORT (PID $RUBY_WORKER_PID)"

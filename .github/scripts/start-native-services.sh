#!/usr/bin/env bash
set -euo pipefail

# Configuration
POSTGRES_URL="${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_rust_test}"
ORCHESTRATION_CONFIG="${ORCHESTRATION_CONFIG:-$(pwd)/config/tasker/orchestration-test.toml}"
WORKER_CONFIG="${WORKER_CONFIG:-$(pwd)/config/tasker/worker-test.toml}"
RUST_TEMPLATE_PATH="$(pwd)/tests/fixtures/task_templates/rust"
RUBY_TEMPLATE_PATH="$(pwd)/tests/fixtures/task_templates/ruby"
PYTHON_TEMPLATE_PATH="$(pwd)/tests/fixtures/task_templates/python"
TYPESCRIPT_TEMPLATE_PATH="$(pwd)/tests/fixtures/task_templates/typescript"
PYTHON_HANDLER_PATH="$(pwd)/workers/python/tests/handlers"
FIXTURE_PATH="$(pwd)/tests/fixtures"
ORCHESTRATION_PORT="${ORCHESTRATION_PORT:-8080}"
WORKER_PORT="${WORKER_PORT:-8081}"
RUBY_WORKER_PORT="${RUBY_WORKER_PORT:-8082}"
PYTHON_WORKER_PORT="${PYTHON_WORKER_PORT:-8083}"
TYPESCRIPT_WORKER_PORT="${TYPESCRIPT_WORKER_PORT:-8084}"

# Export fixture path for E2E tests
export TASKER_FIXTURE_PATH="$FIXTURE_PATH"

echo "🚀 Starting native services..."

# 1. Run database migrations (idempotent)
echo "📊 Running database migrations..."
DATABASE_URL="$POSTGRES_URL" cargo sqlx migrate run

# 2. Start orchestration service in background
echo "🎯 Starting orchestration service on port $ORCHESTRATION_PORT..."
TASKER_CONFIG_PATH="$ORCHESTRATION_CONFIG" \
  DATABASE_URL="$POSTGRES_URL" \
  TASKER_WEB_BIND_ADDRESS="0.0.0.0:$ORCHESTRATION_PORT" \
  RUST_LOG=info \
  target/debug/tasker-server \
  > orchestration.log 2>&1 &
ORCHESTRATION_PID=$!
echo "Orchestration PID: $ORCHESTRATION_PID"

# 3. Start Rust worker in background
echo "⚙️  Starting Rust worker on port $WORKER_PORT..."
TASKER_CONFIG_PATH="$WORKER_CONFIG" \
  DATABASE_URL="$POSTGRES_URL" \
  TASKER_TEMPLATE_PATH="$RUST_TEMPLATE_PATH" \
  TASKER_WEB_BIND_ADDRESS="0.0.0.0:$WORKER_PORT" \
  RUST_LOG=info \
  target/debug/rust-worker \
  > worker.log 2>&1 &
WORKER_PID=$!
echo "Worker PID: $WORKER_PID"

# 4. Start Ruby FFI worker in background
echo "💎 Starting Ruby FFI worker on port $RUBY_WORKER_PORT..."
echo "   WORKER_CONFIG=$WORKER_CONFIG"
echo "   TASKER_WEB_BIND_ADDRESS=0.0.0.0:$RUBY_WORKER_PORT"
echo "   Checking worker config file bind_address:"
grep -A 1 "\[worker.web\]" "$WORKER_CONFIG" | grep bind_address || echo "   bind_address not found in config"
cd workers/ruby
TASKER_CONFIG_PATH="$WORKER_CONFIG" \
  DATABASE_URL="$POSTGRES_URL" \
  TASKER_ENV=test \
  TASKER_TEMPLATE_PATH="$RUBY_TEMPLATE_PATH" \
  TASKER_WEB_BIND_ADDRESS="0.0.0.0:$RUBY_WORKER_PORT" \
  RUST_LOG=info \
  bundle exec bin/server.rb \
  > ../../ruby-worker.log 2>&1 &
RUBY_WORKER_PID=$!
echo "Ruby worker PID: $RUBY_WORKER_PID"
cd ../..

# 5. Start Python FFI worker in background
echo "🐍 Starting Python FFI worker on port $PYTHON_WORKER_PORT..."
cd workers/python
TASKER_CONFIG_PATH="$WORKER_CONFIG" \
  DATABASE_URL="$POSTGRES_URL" \
  TASKER_ENV=test \
  TASKER_TEMPLATE_PATH="$PYTHON_TEMPLATE_PATH" \
  PYTHON_HANDLER_PATH="$PYTHON_HANDLER_PATH" \
  TASKER_WEB_BIND_ADDRESS="0.0.0.0:$PYTHON_WORKER_PORT" \
  RUST_LOG=info \
  uv run python bin/server.py \
  > ../../python-worker.log 2>&1 &
PYTHON_WORKER_PID=$!
echo "Python worker PID: $PYTHON_WORKER_PID"
cd ../..

# 6. Start TypeScript FFI worker in background
echo "📜 Starting TypeScript FFI worker on port $TYPESCRIPT_WORKER_PORT..."
cd workers/typescript
TASKER_CONFIG_PATH="$WORKER_CONFIG" \
  DATABASE_URL="$POSTGRES_URL" \
  TASKER_ENV=test \
  TASKER_TEMPLATE_PATH="$TYPESCRIPT_TEMPLATE_PATH" \
  TASKER_WEB_BIND_ADDRESS="0.0.0.0:$TYPESCRIPT_WORKER_PORT" \
  RUST_LOG=info \
  bun run bin/server.ts \
  > ../../typescript-worker.log 2>&1 &
TYPESCRIPT_WORKER_PID=$!
echo "TypeScript worker PID: $TYPESCRIPT_WORKER_PID"
cd ../..

# 7. Wait for health checks
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

  until curl -sf http://localhost:$PYTHON_WORKER_PORT/health > /dev/null; do
    echo '⏳ Waiting for Python worker...'
    sleep 2
  done
  echo '✅ Python worker ready'

  until curl -sf http://localhost:$TYPESCRIPT_WORKER_PORT/health > /dev/null; do
    echo '⏳ Waiting for TypeScript worker...'
    sleep 2
  done
  echo '✅ TypeScript worker ready'
"

# 8. Save PIDs for cleanup
mkdir -p .pids
echo "$ORCHESTRATION_PID" > .pids/orchestration.pid
echo "$WORKER_PID" > .pids/worker.pid
echo "$RUBY_WORKER_PID" > .pids/ruby-worker.pid
echo "$PYTHON_WORKER_PID" > .pids/python-worker.pid
echo "$TYPESCRIPT_WORKER_PID" > .pids/typescript-worker.pid

echo "✅ All services started and healthy!"
echo "   - Orchestration:      http://localhost:$ORCHESTRATION_PORT (PID $ORCHESTRATION_PID)"
echo "   - Rust Worker:        http://localhost:$WORKER_PORT (PID $WORKER_PID)"
echo "   - Ruby Worker:        http://localhost:$RUBY_WORKER_PORT (PID $RUBY_WORKER_PID)"
echo "   - Python Worker:      http://localhost:$PYTHON_WORKER_PORT (PID $PYTHON_WORKER_PID)"
echo "   - TypeScript Worker:  http://localhost:$TYPESCRIPT_WORKER_PORT (PID $TYPESCRIPT_WORKER_PID)"

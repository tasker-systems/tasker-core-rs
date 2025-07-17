#!/bin/bash

# System timeout wrapper for running RSpec tests
# Prevents tests from hanging indefinitely due to database pool issues

set -e

# Configuration
TIMEOUT_MINUTES=${1:-5}  # Default 5 minute timeout, can be overridden
TIMEOUT_SECONDS=$((TIMEOUT_MINUTES * 60))
RSPEC_ARGS=${2:-"-f d"}  # Default to documentation format

echo "🔍 TIMEOUT WRAPPER: Starting RSpec with ${TIMEOUT_MINUTES} minute timeout"
echo "🔍 TIMEOUT WRAPPER: RSpec args: ${RSPEC_ARGS}"
echo "🔍 TIMEOUT WRAPPER: Current directory: $(pwd)"

# Function to cleanup background processes
cleanup() {
    echo "🔍 TIMEOUT WRAPPER: Cleanup called"
    if [ ! -z "$RSPEC_PID" ]; then
        echo "🔍 TIMEOUT WRAPPER: Killing RSpec process $RSPEC_PID"
        kill -TERM $RSPEC_PID 2>/dev/null || true
        sleep 2
        kill -KILL $RSPEC_PID 2>/dev/null || true
    fi
}

# Set up signal handlers
trap cleanup EXIT
trap cleanup SIGINT
trap cleanup SIGTERM

# Start RSpec in background
echo "🔍 TIMEOUT WRAPPER: Starting bundle exec rspec ${RSPEC_ARGS}"
bundle exec rspec ${RSPEC_ARGS} &
RSPEC_PID=$!

echo "🔍 TIMEOUT WRAPPER: RSpec PID: $RSPEC_PID"

# Wait for RSpec to complete or timeout
timeout ${TIMEOUT_SECONDS} wait $RSPEC_PID
EXIT_CODE=$?

echo "🔍 TIMEOUT WRAPPER: RSpec exit code: $EXIT_CODE"

if [ $EXIT_CODE -eq 124 ]; then
    echo "🔍 TIMEOUT WRAPPER: ❌ RSpec timed out after ${TIMEOUT_MINUTES} minutes"
    echo "🔍 TIMEOUT WRAPPER: This suggests database pool timeout issues"
    exit 1
elif [ $EXIT_CODE -eq 0 ]; then
    echo "🔍 TIMEOUT WRAPPER: ✅ RSpec completed successfully"
else
    echo "🔍 TIMEOUT WRAPPER: ⚠️  RSpec completed with failures (exit code: $EXIT_CODE)"
fi

exit $EXIT_CODE
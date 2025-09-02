#!/bin/bash
# =============================================================================
# Tasker Core Integration Testing Runner
# =============================================================================
# Starts the complete integration testing environment with Docker Compose
# Includes PostgreSQL with PGMQ, orchestration service, and comprehensive worker
#
# Usage:
#   ./run-integration.sh start          # Start with cached builds
#   ./run-integration.sh start --rebuild # Force rebuild without cache
#   ./run-integration.sh stop           # Stop all services

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$DOCKER_DIR")"

# Parse command line arguments
COMMAND="${1:-start}"
REBUILD="${2:-}"

echo "🚀 Starting Tasker Core Integration Environment..."
echo "Project root: $PROJECT_ROOT"
echo "Docker directory: $DOCKER_DIR"

cd "$DOCKER_DIR"

if [[ "$COMMAND" == "stop" ]]; then
    echo "🛑 Stopping integration services..."
    docker-compose -f docker-compose.integration.yml down
    exit 0
fi

# Determine build flags
BUILD_FLAGS="--build"
if [[ "$REBUILD" == "--rebuild" ]]; then
    echo "🔄 Force rebuilding all images without cache..."
    BUILD_FLAGS="--build"
fi

# Build the common base first
echo "📦 Building common Rust dependencies with cargo-chef..."
if [[ "$REBUILD" == "--rebuild" ]]; then
    docker-compose -f docker-compose.integration.yml --profile build build --no-cache builder-base
    docker-compose -f docker-compose.integration.yml --profile build up builder-base
else
    docker-compose -f docker-compose.integration.yml --profile build up builder-base
fi

# Start all services
echo "🐳 Starting integration services..."
docker-compose -f docker-compose.integration.yml up $BUILD_FLAGS

echo "✅ Integration environment started!"
echo ""
echo "🔗 Service endpoints:"
echo "  • PostgreSQL:     localhost:5432 (tasker/tasker)"
echo "  • Orchestration:  http://localhost:8080"
echo "  • Rust Worker:    http://localhost:8081"
echo ""
echo "📊 Health checks:"
echo "  • curl http://localhost:8080/health"
echo "  • curl http://localhost:8081/health"
echo ""
echo "🧪 Run integration tests with:"
echo "  cd $PROJECT_ROOT && cargo test --all-features -- docker_integration"
echo ""
echo "🛑 Stop with: docker-compose -f docker-compose.integration.yml down"
#!/bin/bash
# =============================================================================
# Tasker Core Integration Testing Runner
# =============================================================================
# Starts the complete integration testing environment with Docker Compose
# Includes PostgreSQL with PGMQ, orchestration service, and comprehensive worker

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$DOCKER_DIR")"

echo "🚀 Starting Tasker Core Integration Environment..."
echo "Project root: $PROJECT_ROOT"
echo "Docker directory: $DOCKER_DIR"

cd "$DOCKER_DIR"

# Build the common base first
echo "📦 Building common Rust dependencies with cargo-chef..."
docker-compose -f docker-compose.integration.yml --profile build up builder-base

# Start all services
echo "🐳 Starting integration services..."
docker-compose -f docker-compose.integration.yml up --build

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
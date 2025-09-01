#!/bin/bash
# =============================================================================
# Tasker Core Development Environment Runner
# =============================================================================
# Starts the development environment with debugging tools enabled
# Includes hot-reloading, Tokio console, and enhanced logging

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$DOCKER_DIR")"

echo "🛠️  Starting Tasker Core Development Environment..."
echo "Project root: $PROJECT_ROOT"
echo "Docker directory: $DOCKER_DIR"

cd "$DOCKER_DIR"

# Build the common base first
echo "📦 Building common Rust dependencies with cargo-chef..."
docker-compose -f docker-compose.dev.yml --profile build up builder-base

# Start all services with development configuration
echo "🐳 Starting development services..."
docker-compose -f docker-compose.dev.yml up --build

echo "✅ Development environment started!"
echo ""
echo "🔗 Service endpoints:"
echo "  • PostgreSQL:     localhost:5432 (tasker/tasker)"
echo "  • Orchestration:  http://localhost:8080"
echo "  • Rust Worker:    http://localhost:8081"
echo ""
echo "🐛 Debugging tools:"
echo "  • Tokio Console (Orchestration): http://localhost:6669"
echo "  • Tokio Console (Worker):        http://localhost:6670"
echo ""
echo "📊 Health checks:"
echo "  • curl http://localhost:8080/health"
echo "  • curl http://localhost:8081/health"
echo ""
echo "🔧 Development features:"
echo "  • Config hot-reloading enabled"
echo "  • Enhanced debug logging (RUST_LOG=debug)"
echo "  • Full backtraces enabled"
echo "  • Development helper scripts in containers"
echo ""
echo "🛑 Stop with: docker-compose -f docker-compose.dev.yml down"
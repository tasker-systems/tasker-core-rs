#!/bin/bash
# =============================================================================
# Tasker Core Production Deployment Runner
# =============================================================================
# Starts the production environment with optimized builds and resource limits
# Includes security hardening and production monitoring

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$DOCKER_DIR")"

# Check for required environment variables
if [[ -z "${POSTGRES_PASSWORD}" ]]; then
    echo "⚠️  WARNING: POSTGRES_PASSWORD not set. Using default (not recommended for production)."
    echo "   Set with: export POSTGRES_PASSWORD=your_secure_password"
fi

echo "🚀 Starting Tasker Core Production Environment..."
echo "Project root: $PROJECT_ROOT"
echo "Docker directory: $DOCKER_DIR"

cd "$DOCKER_DIR"

# Build the common base first
echo "📦 Building common Rust dependencies with cargo-chef..."
docker-compose -f docker-compose.deploy.yml --profile build up builder-base

# Start production services in detached mode
echo "🐳 Starting production services..."
docker-compose -f docker-compose.deploy.yml up --build -d

echo "✅ Production environment started!"
echo ""
echo "🔗 Service endpoints:"
echo "  • PostgreSQL:     localhost:${POSTGRES_PORT:-5432}"
echo "  • Orchestration:  http://localhost:${ORCHESTRATION_PORT:-8080}"
echo "  • Rust Worker:    http://localhost:${WORKER_PORT:-8081}"
echo ""
echo "📊 Health monitoring:"
echo "  • curl http://localhost:${ORCHESTRATION_PORT:-8080}/health"
echo "  • curl http://localhost:${WORKER_PORT:-8081}/health"
echo ""
echo "🏭 Production features:"
echo "  • Release-optimized binaries"
echo "  • Resource limits enabled"
echo "  • Security hardened containers"
echo "  • Automatic restart policies"
echo ""
echo "📈 Scaling examples:"
echo "  • Scale workers: docker-compose -f docker-compose.deploy.yml up --scale worker=3 -d"
echo "  • View logs:     docker-compose -f docker-compose.deploy.yml logs -f"
echo "  • Monitor:       docker-compose -f docker-compose.deploy.yml ps"
echo ""
echo "🛑 Stop with: docker-compose -f docker-compose.deploy.yml down"
echo ""
echo "⚡ Production environment variables you can set:"
echo "  • POSTGRES_PASSWORD     - Database password (required for security)"
echo "  • POSTGRES_PORT         - Database port (default: 5432)"
echo "  • ORCHESTRATION_PORT    - Orchestration API port (default: 8080)"
echo "  • WORKER_PORT           - Worker API port (default: 8081)"
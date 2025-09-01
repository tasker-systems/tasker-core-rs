#!/bin/bash
# =============================================================================
# Tasker Core Docker Cleanup Script
# =============================================================================
# Cleans up Docker resources, images, containers, and volumes
# Provides different cleanup levels for different scenarios

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"

# Default cleanup level
CLEANUP_LEVEL="${1:-basic}"

echo "🧹 Tasker Core Docker Cleanup"
echo "Cleanup level: $CLEANUP_LEVEL"
echo ""

cd "$DOCKER_DIR"

# Stop and remove containers
stop_containers() {
    echo "🛑 Stopping Tasker containers..."

    # Stop integration environment
    if docker-compose -f docker-compose.integration.yml ps -q 2>/dev/null | grep -q .; then
        docker-compose -f docker-compose.integration.yml down
    fi

    # Stop development environment
    if docker-compose -f docker-compose.dev.yml ps -q 2>/dev/null | grep -q .; then
        docker-compose -f docker-compose.dev.yml down
    fi

    # Stop production environment
    if docker-compose -f docker-compose.deploy.yml ps -q 2>/dev/null | grep -q .; then
        docker-compose -f docker-compose.deploy.yml down
    fi

    echo "✅ Containers stopped"
}

# Remove volumes
remove_volumes() {
    echo "🗂️  Removing Tasker volumes..."

    # Remove named volumes
    docker volume rm tasker-integration-postgres-data 2>/dev/null || true
    docker volume rm tasker-dev-postgres-data 2>/dev/null || true
    docker volume rm tasker-prod-postgres-data 2>/dev/null || true

    echo "✅ Volumes removed"
}

# Remove networks
remove_networks() {
    echo "🔗 Removing Tasker networks..."

    docker network rm tasker-integration-network 2>/dev/null || true
    docker network rm tasker-development-network 2>/dev/null || true
    docker network rm tasker-production-network 2>/dev/null || true

    echo "✅ Networks removed"
}

# Remove images
remove_images() {
    echo "🖼️  Removing Tasker images..."

    # Remove all tasker images
    docker images | grep tasker | awk '{print $3}' | xargs -r docker rmi -f 2>/dev/null || true

    # Remove builder base
    docker rmi jcoletaylor/tasker-builder-base:latest 2>/dev/null || true
    docker rmi jcoletaylor/docker-build:latest 2>/dev/null || true

    echo "✅ Images removed"
}

# Clean build cache
clean_build_cache() {
    echo "💾 Cleaning Docker build cache..."

    docker builder prune -f
    docker system prune -f

    echo "✅ Build cache cleaned"
}

# Deep clean - removes everything including unused Docker resources
deep_clean() {
    echo "🔥 Performing deep clean..."

    # Remove all stopped containers
    docker container prune -f

    # Remove all unused images
    docker image prune -a -f

    # Remove all unused volumes
    docker volume prune -f

    # Remove all unused networks
    docker network prune -f

    # Clean build cache
    docker builder prune -a -f
    docker system prune -a -f

    echo "✅ Deep clean complete"
}

# Main cleanup logic
case "$CLEANUP_LEVEL" in
    "containers")
        stop_containers
        ;;
    "basic")
        stop_containers
        remove_volumes
        remove_networks
        ;;
    "images")
        stop_containers
        remove_volumes
        remove_networks
        remove_images
        ;;
    "cache")
        clean_build_cache
        ;;
    "deep")
        stop_containers
        remove_volumes
        remove_networks
        remove_images
        deep_clean
        ;;
    *)
        echo "❌ Invalid cleanup level: $CLEANUP_LEVEL"
        echo ""
        echo "Available levels:"
        echo "  • containers  - Stop containers only"
        echo "  • basic       - Stop containers, remove volumes and networks"
        echo "  • images      - Basic + remove Tasker images"
        echo "  • cache       - Clean Docker build cache"
        echo "  • deep        - Remove everything including unused Docker resources"
        exit 1
        ;;
esac

echo ""
echo "✅ Cleanup complete!"
echo ""
echo "📊 Current Docker usage:"
echo "Containers: $(docker ps -a | wc -l) total"
echo "Images: $(docker images | wc -l) total"
echo "Volumes: $(docker volume ls | wc -l) total"
echo "Networks: $(docker network ls | wc -l) total"
echo ""
echo "Usage examples:"
echo "  • ./cleanup.sh containers  - Stop containers only"
echo "  • ./cleanup.sh basic       - Standard cleanup"
echo "  • ./cleanup.sh images      - Remove Tasker images"
echo "  • ./cleanup.sh deep        - Full cleanup (careful!)"

#!/bin/bash
# =============================================================================
# Tasker Core Docker Images Builder
# =============================================================================
# Builds all Docker images with proper tagging and optimization
# Uses cargo-chef for efficient layer caching

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$DOCKER_DIR")"

# Default values
BUILD_TYPE="${1:-all}"
TAG="${2:-latest}"
PUSH="${3:-false}"

echo "🔨 Building Tasker Core Docker Images..."
echo "Build type: $BUILD_TYPE"
echo "Tag: $TAG"
echo "Push to registry: $PUSH"
echo ""

cd "$DOCKER_DIR"

# Build common base first
build_base() {
    echo "📦 Building common Rust base with cargo-chef..."
    docker build -f build/Dockerfile -t jcoletaylor/tasker-builder-base:$TAG ..
    echo "✅ Base image built: jcoletaylor/tasker-builder-base:$TAG"
}

# Build orchestration images
build_orchestration() {
    echo "🎯 Building orchestration images..."

    # Production orchestration
    docker build --no-cache -f deploy/orchestration/Dockerfile \
        -t jcoletaylor/tasker-orchestration:$TAG \
        -t jcoletaylor/tasker-orchestration:deploy-$TAG ..

    # Development orchestration
    docker build --no-cache -f dev/orchestration/Dockerfile \
        -t jcoletaylor/tasker-orchestration:dev-$TAG ..

    echo "✅ Orchestration images built"
}

# Build worker images
build_workers() {
    echo "👷 Building worker images..."

    # Rust workers (production and development)
    docker build --no-cache -f deploy/workers/rust/Dockerfile \
        -t jcoletaylor/tasker-worker-rust:$TAG \
        -t jcoletaylor/tasker-worker-rust:deploy-$TAG ..

    docker build --no-cache -f dev/workers/rust/Dockerfile \
        -t jcoletaylor/tasker-worker-rust:dev-$TAG ..

    # # Other worker types (optional)
    # if [[ "$BUILD_TYPE" == "all" || "$BUILD_TYPE" == "ruby" ]]; then
    #     echo "💎 Building Ruby worker..."
    #     docker build --no-cache -f deploy/workers/ruby/Dockerfile \
    #         -t jcoletaylor/tasker-worker-ruby:$TAG ..
    # fi

    # if [[ "$BUILD_TYPE" == "all" || "$BUILD_TYPE" == "python" ]]; then
    #     echo "🐍 Building Python worker..."
    #     docker build --no-cache -f deploy/workers/python/Dockerfile \
    #         -t jcoletaylor/tasker-worker-python:$TAG ..
    # fi

    # if [[ "$BUILD_TYPE" == "all" || "$BUILD_TYPE" == "wasm" ]]; then
    #     echo "🕸️  Building WASM worker..."
    #     docker build --no-cache -f deploy/workers/wasm/Dockerfile \
    #         -t jcoletaylor/tasker-worker-wasm:$TAG ..
    # fi

    echo "✅ Worker images built"
}

# Build PostgreSQL with extensions
build_postgres() {
    echo "🐘 Building PostgreSQL with PGMQ and UUID v7..."
    docker build -f db/Dockerfile \
        -t jcoletaylor/tasker-pgmq:$TAG .
    echo "✅ PostgreSQL image built"
}

# Push images to registry
push_images() {
    if [[ "$PUSH" == "true" ]]; then
        echo "📤 Pushing images to registry..."

        docker push jcoletaylor/tasker-builder-base:$TAG
        docker push jcoletaylor/tasker-orchestration:$TAG
        docker push jcoletaylor/tasker-orchestration:deploy-$TAG
        docker push jcoletaylor/tasker-orchestration:dev-$TAG
        docker push jcoletaylor/tasker-worker-rust:$TAG
        docker push jcoletaylor/tasker-worker-rust:deploy-$TAG
        docker push jcoletaylor/tasker-worker-rust:dev-$TAG
        docker push jcoletaylor/tasker-pgmq:$TAG

        # if docker images | grep -q "tasker-worker-ruby:$TAG"; then
        #     docker push jcoletaylor/tasker-worker-ruby:$TAG
        # fi

        # if docker images | grep -q "tasker-worker-python:$TAG"; then
        #     docker push jcoletaylor/tasker-worker-python:$TAG
        # fi

        # if docker images | grep -q "tasker-worker-wasm:$TAG"; then
        #     docker push jcoletaylor/tasker-worker-wasm:$TAG
        # fi

        echo "✅ Images pushed to registry"
    fi
}

# Main build logic
case "$BUILD_TYPE" in
    "base")
        build_base
        ;;
    "orchestration")
        build_base
        build_orchestration
        ;;
    "workers")
        build_base
        build_workers
        ;;
    "rust")
        build_base
        build_workers
        ;;
    # "ruby")
    #     build_base
    #     BUILD_TYPE="ruby" build_workers
    #     ;;
    # "python")
    #     build_base
    #     BUILD_TYPE="python" build_workers
    #     ;;
    # "wasm")
    #     build_base
    #     BUILD_TYPE="wasm" build_workers
    #     ;;
    "postgres")
        build_postgres
        ;;
    "all"|*)
        build_base
        build_orchestration
        build_workers
        build_postgres
        ;;
esac

push_images

echo ""
echo "🎉 Build complete!"
echo ""
echo "📋 Built images:"
docker images | grep tasker | head -20
echo ""
echo "Usage examples:"
echo "  • ./build-images.sh base           - Build only base image"
echo "  • ./build-images.sh orchestration  - Build orchestration images"
echo "  • ./build-images.sh workers        - Build all worker images"
echo "  • ./build-images.sh rust           - Build only Rust worker"
echo "  • ./build-images.sh all v1.0.0     - Build all with custom tag"
echo "  • ./build-images.sh all latest true - Build all and push to registry"

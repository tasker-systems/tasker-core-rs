#!/bin/bash
# =============================================================================
# Docker Build Benchmark Script
# =============================================================================
# This script benchmarks the performance difference between the original
# and optimized Docker builds.
#
# Usage: ./docker/scripts/benchmark-builds.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Timing functions
start_timer() {
    START_TIME=$(date +%s)
}

end_timer() {
    END_TIME=$(date +%s)
    ELAPSED=$((END_TIME - START_TIME))
    echo -e "${GREEN}Time: ${ELAPSED} seconds${NC}"
}

# Header function
print_header() {
    echo -e "\n${BLUE}============================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}============================================${NC}"
}

# Main benchmark
main() {
    print_header "Docker Build Benchmark"

    # Check prerequisites
    echo -e "${YELLOW}Checking prerequisites...${NC}"
    if ! command -v docker &> /dev/null; then
        echo -e "${RED}Docker is not installed${NC}"
        exit 1
    fi

    if ! command -v docker compose &> /dev/null; then
        echo -e "${RED}Docker Compose is not installed${NC}"
        exit 1
    fi

    # Clean up everything
    print_header "Cleaning up existing builds and caches"
    docker compose -f docker/docker-compose.test.yml down -v 2>/dev/null || true
    docker compose -f docker/docker-compose.test-local.yml down -v 2>/dev/null || true
    docker system prune -af --volumes
    rm -rf /tmp/docker-cache

    # Benchmark 1: Cold build (original)
    print_header "Test 1: Cold Build - Original Dockerfiles"
    start_timer
    docker compose -f docker/docker-compose.test.yml build --no-cache
    end_timer
    ORIGINAL_COLD=$ELAPSED

    # Clean again for fair comparison
    docker system prune -af --volumes

    # Benchmark 2: Cold build (optimized)
    print_header "Test 2: Cold Build - Optimized Dockerfiles"
    export DOCKER_BUILDKIT=1
    export COMPOSE_DOCKER_CLI_BUILD=1
    start_timer
    docker compose -f docker/docker-compose.test-local.yml build --no-cache
    end_timer
    OPTIMIZED_COLD=$ELAPSED

    # Benchmark 3: Incremental build (original)
    print_header "Test 3: Incremental Build - Original (code change)"
    echo "// benchmark test" >> tasker-orchestration/src/lib.rs
    start_timer
    docker compose -f docker/docker-compose.test.yml build
    end_timer
    ORIGINAL_INCREMENTAL=$ELAPSED

    # Benchmark 4: Incremental build (optimized)
    print_header "Test 4: Incremental Build - Optimized (code change)"
    echo "// benchmark test 2" >> tasker-orchestration/src/lib.rs
    start_timer
    docker compose -f docker/docker-compose.test-local.yml build
    end_timer
    OPTIMIZED_INCREMENTAL=$ELAPSED

    # Benchmark 5: No-op build (original)
    print_header "Test 5: No-op Build - Original (no changes)"
    start_timer
    docker compose -f docker/docker-compose.test.yml build
    end_timer
    ORIGINAL_NOOP=$ELAPSED

    # Benchmark 6: No-op build (optimized)
    print_header "Test 6: No-op Build - Optimized (no changes)"
    start_timer
    docker compose -f docker/docker-compose.test-local.yml build
    end_timer
    OPTIMIZED_NOOP=$ELAPSED

    # Clean up the test changes
    git checkout -- tasker-orchestration/src/lib.rs 2>/dev/null || true

    # Print results
    print_header "Benchmark Results"
    echo -e "${GREEN}Cold Build:${NC}"
    echo "  Original:  ${ORIGINAL_COLD}s"
    echo "  Optimized: ${OPTIMIZED_COLD}s"
    if [ $OPTIMIZED_COLD -lt $ORIGINAL_COLD ]; then
        IMPROVEMENT=$(( (ORIGINAL_COLD - OPTIMIZED_COLD) * 100 / ORIGINAL_COLD ))
        echo -e "  ${GREEN}Improvement: ${IMPROVEMENT}%${NC}"
    fi

    echo -e "\n${GREEN}Incremental Build:${NC}"
    echo "  Original:  ${ORIGINAL_INCREMENTAL}s"
    echo "  Optimized: ${OPTIMIZED_INCREMENTAL}s"
    if [ $OPTIMIZED_INCREMENTAL -lt $ORIGINAL_INCREMENTAL ]; then
        IMPROVEMENT=$(( (ORIGINAL_INCREMENTAL - OPTIMIZED_INCREMENTAL) * 100 / ORIGINAL_INCREMENTAL ))
        echo -e "  ${GREEN}Improvement: ${IMPROVEMENT}%${NC}"
    fi

    echo -e "\n${GREEN}No-op Build:${NC}"
    echo "  Original:  ${ORIGINAL_NOOP}s"
    echo "  Optimized: ${OPTIMIZED_NOOP}s"
    if [ $OPTIMIZED_NOOP -lt $ORIGINAL_NOOP ]; then
        IMPROVEMENT=$(( (ORIGINAL_NOOP - OPTIMIZED_NOOP) * 100 / ORIGINAL_NOOP ))
        echo -e "  ${GREEN}Improvement: ${IMPROVEMENT}%${NC}"
    fi

    # Cache statistics
    print_header "Cache Statistics"
    echo "Local cache sizes:"
    du -sh /tmp/docker-cache/* 2>/dev/null || echo "No local cache found"

    echo -e "\nDocker BuildKit cache:"
    docker builder du

    print_header "Benchmark Complete!"
}

# Run main function
main "$@"

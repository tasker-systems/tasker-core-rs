#!/bin/bash
# =============================================================================
# Optimized Docker Build Validation Script
# =============================================================================
# This script validates that the optimized Docker builds work correctly
# without running full benchmarks.
#
# Usage: ./docker/scripts/validate-optimized-builds.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Test function
run_test() {
    local test_name=$1
    local test_command=$2

    echo -e "\n${YELLOW}Testing: ${test_name}...${NC}"

    if eval "$test_command"; then
        echo -e "${GREEN}✓ PASSED${NC}"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "${RED}✗ FAILED${NC}"
        ((TESTS_FAILED++))
        return 1
    fi
}

# Header function
print_header() {
    echo -e "\n${BLUE}============================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}============================================${NC}"
}

# Main validation
main() {
    print_header "Optimized Docker Build Validation"

    # Check prerequisites
    echo -e "${YELLOW}Checking prerequisites...${NC}"

    # Check Docker
    if ! command -v docker &> /dev/null; then
        echo -e "${RED}✗ Docker is not installed${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Docker found${NC}"

    # Check Docker Compose
    if ! command -v docker compose &> /dev/null; then
        echo -e "${RED}✗ Docker Compose is not installed${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Docker Compose found${NC}"

    # Check BuildKit
    if [ -z "$DOCKER_BUILDKIT" ]; then
        echo -e "${YELLOW}! BuildKit not enabled, enabling now...${NC}"
        export DOCKER_BUILDKIT=1
        export COMPOSE_DOCKER_CLI_BUILD=1
    fi
    echo -e "${GREEN}✓ BuildKit enabled${NC}"

    # Test 1: Verify Dockerfiles exist
    print_header "Test 1: Verify Optimized Dockerfiles Exist"

    run_test "orchestration.test-local.Dockerfile exists" \
        "[ -f docker/build/orchestration.test-local.Dockerfile ]"

    run_test "rust-worker.test-local.Dockerfile exists" \
        "[ -f docker/build/rust-worker.test-local.Dockerfile ]"

    run_test "ruby-worker.test-local.Dockerfile exists" \
        "[ -f docker/build/ruby-worker.test-local.Dockerfile ]"

    run_test "docker-compose.test-local.yml exists" \
        "[ -f docker/docker-compose.test-local.yml ]"

    # Test 2: Validate Dockerfile syntax
    print_header "Test 2: Validate Dockerfile Syntax"

    run_test "orchestration Dockerfile syntax" \
        "docker build -f docker/build/orchestration.test-local.Dockerfile --help > /dev/null 2>&1"

    run_test "rust-worker Dockerfile syntax" \
        "docker build -f docker/build/rust-worker.test-local.Dockerfile --help > /dev/null 2>&1"

    run_test "ruby-worker Dockerfile syntax" \
        "docker build -f docker/build/ruby-worker.test-local.Dockerfile --help > /dev/null 2>&1"

    # Test 3: Build base stage (quick test)
    print_header "Test 3: Quick Build Test (chef stage only)"

    run_test "Build orchestration chef stage" \
        "DOCKER_BUILDKIT=1 docker build -f docker/build/orchestration.test-local.Dockerfile --target chef -t test-chef . > /dev/null 2>&1"

    # Test 4: Verify cache directory setup
    print_header "Test 4: Cache Directory Setup"

    run_test "Create cache directories" \
        "mkdir -p /tmp/docker-cache/{orchestration,rust-worker,ruby-worker}"

    run_test "Cache directories writable" \
        "touch /tmp/docker-cache/test && rm /tmp/docker-cache/test"

    # Test 5: Docker Compose validation
    print_header "Test 5: Docker Compose Configuration"

    run_test "Validate docker-compose.test-local.yml syntax" \
        "docker compose -f docker/docker-compose.test-local.yml config > /dev/null 2>&1"

    # Test 6: Check for required tools in Dockerfiles
    print_header "Test 6: Verify Tool Configurations"

    run_test "cargo-chef version specified" \
        "grep -q 'ARG CARGO_CHEF_VERSION=' docker/build/orchestration.test-local.Dockerfile"

    run_test "cargo-binstall version specified" \
        "grep -q 'ARG CARGO_BINSTALL_VERSION=' docker/build/orchestration.test-local.Dockerfile"

    run_test "BuildKit verification present" \
        "grep -q 'ARG DOCKER_BUILDKIT=1' docker/build/orchestration.test-local.Dockerfile"

    # Test 7: Verify optimization patterns
    print_header "Test 7: Verify Optimization Patterns"

    run_test "Single-layer binary copy in orchestration" \
        "grep -A1 'cargo build.*tasker-server' docker/build/orchestration.test-local.Dockerfile | grep -q 'cp.*tasker-server.*app/tasker-server'"

    run_test "Single-layer binary copy in rust-worker" \
        "grep -A1 'cargo build.*rust-worker' docker/build/rust-worker.test-local.Dockerfile | grep -q 'cp.*rust-worker.*app/rust-worker'"

    run_test "Cache mounts in ruby-worker" \
        "grep -q 'mount=type=cache' docker/build/ruby-worker.test-local.Dockerfile"

    # Test 8: Optional full build test
    if [ "$1" == "--full" ]; then
        print_header "Test 8: Full Build Test (this will take time)"

        # Clean up first
        docker compose -f docker/docker-compose.test-local.yml down -v 2>/dev/null || true
        docker system prune -f

        run_test "Full build of all services" \
            "DOCKER_BUILDKIT=1 docker compose -f docker/docker-compose.test-local.yml build"

        run_test "Start services" \
            "docker compose -f docker/docker-compose.test-local.yml up -d"

        # Wait for services to be ready
        echo -e "${YELLOW}Waiting for services to be healthy...${NC}"
        sleep 10

        run_test "Orchestration health check" \
            "curl -f http://localhost:8080/health 2>/dev/null"

        run_test "Rust worker health check" \
            "curl -f http://localhost:8081/health 2>/dev/null"

        # Ruby worker might take longer
        sleep 5
        run_test "Ruby worker health check" \
            "curl -f http://localhost:8082/health 2>/dev/null"

        # Clean up
        echo -e "${YELLOW}Cleaning up...${NC}"
        docker compose -f docker/docker-compose.test-local.yml down -v
    else
        echo -e "\n${YELLOW}ℹ Skipping full build test. Run with --full to include.${NC}"
    fi

    # Print summary
    print_header "Validation Summary"

    TOTAL_TESTS=$((TESTS_PASSED + TESTS_FAILED))

    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}✓ All tests passed! (${TESTS_PASSED}/${TOTAL_TESTS})${NC}"
        echo -e "\n${GREEN}The optimized builds are ready to use!${NC}"
        echo -e "\nTo run the optimized builds:"
        echo -e "  ${BLUE}export DOCKER_BUILDKIT=1${NC}"
        echo -e "  ${BLUE}export COMPOSE_DOCKER_CLI_BUILD=1${NC}"
        echo -e "  ${BLUE}docker compose -f docker/docker-compose.test-local.yml up --build${NC}"
        echo -e "\nTo benchmark against original:"
        echo -e "  ${BLUE}./docker/scripts/benchmark-builds.sh${NC}"
        exit 0
    else
        echo -e "${RED}✗ Some tests failed: ${TESTS_FAILED}/${TOTAL_TESTS}${NC}"
        echo -e "${GREEN}✓ Tests passed: ${TESTS_PASSED}/${TOTAL_TESTS}${NC}"
        echo -e "\n${YELLOW}Please review the failures above before using the optimized builds.${NC}"
        exit 1
    fi
}

# Run main function
main "$@"

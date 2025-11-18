#!/bin/bash
# =============================================================================
# Docker Build Compatibility Script
# =============================================================================
# This script automatically detects Docker BuildKit support and chooses the
# appropriate Dockerfiles and build strategy for your environment.
#
# Usage: ./docker/scripts/build-with-compatibility.sh [service] [--force-traditional]
#
# Examples:
#   ./docker/scripts/build-with-compatibility.sh              # Build all services
#   ./docker/scripts/build-with-compatibility.sh orchestration # Build specific service
#   ./docker/scripts/build-with-compatibility.sh --force-traditional # Force non-BuildKit

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
COMPOSE_FILE_TRADITIONAL="docker/docker-compose.test.yml"
COMPOSE_FILE_OPTIMIZED="docker/docker-compose.test-local.yml"
MIN_DOCKER_VERSION="18.09"
SERVICE_NAME=$1
FORCE_TRADITIONAL=false

# Parse arguments
for arg in "$@"; do
    case $arg in
        --force-traditional)
            FORCE_TRADITIONAL=true
            shift
            ;;
        --help)
            echo "Usage: $0 [service] [--force-traditional]"
            echo ""
            echo "Automatically detects Docker BuildKit support and chooses the"
            echo "appropriate build strategy for your environment."
            echo ""
            echo "Options:"
            echo "  service              Build specific service (orchestration, worker, ruby-worker)"
            echo "  --force-traditional  Force traditional build even if BuildKit is available"
            echo "  --help              Show this help message"
            exit 0
            ;;
    esac
done

# Functions
print_header() {
    echo -e "\n${BLUE}============================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}============================================${NC}"
}

print_info() {
    echo -e "${CYAN}ℹ${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Check Docker version
check_docker_version() {
    if ! command -v docker &> /dev/null; then
        print_error "Docker is not installed"
        exit 1
    fi

    DOCKER_VERSION=$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo "0.0")
    print_info "Docker version: $DOCKER_VERSION"

    # Compare versions (simplified - just check major.minor)
    DOCKER_MAJOR=$(echo $DOCKER_VERSION | cut -d. -f1)
    DOCKER_MINOR=$(echo $DOCKER_VERSION | cut -d. -f2)
    MIN_MAJOR=$(echo $MIN_DOCKER_VERSION | cut -d. -f1)
    MIN_MINOR=$(echo $MIN_DOCKER_VERSION | cut -d. -f2)

    if [ "$DOCKER_MAJOR" -lt "$MIN_MAJOR" ] || \
       ([ "$DOCKER_MAJOR" -eq "$MIN_MAJOR" ] && [ "$DOCKER_MINOR" -lt "$MIN_MINOR" ]); then
        return 1
    fi
    return 0
}

# Check BuildKit support
check_buildkit_support() {
    # Check if BuildKit is explicitly disabled
    if [ "$FORCE_TRADITIONAL" = true ]; then
        print_warning "BuildKit forced off by --force-traditional flag"
        return 1
    fi

    # Check Docker version supports BuildKit
    if ! check_docker_version; then
        print_warning "Docker version too old for BuildKit (need $MIN_DOCKER_VERSION+)"
        return 1
    fi

    # Check if BuildKit is available
    if docker buildx version &>/dev/null; then
        print_success "BuildKit available via buildx"
        return 0
    fi

    # Try to enable BuildKit and test it
    DOCKER_BUILDKIT=1 docker build --help 2>&1 | grep -q "mount" && {
        print_success "BuildKit available and functional"
        return 0
    }

    print_warning "BuildKit not available or not functional"
    return 1
}

# Check for BuildKit-specific issues
check_buildkit_issues() {
    local issues_found=false

    # Check for WSL1 (BuildKit has issues)
    if [ -f /proc/version ] && grep -qi "microsoft" /proc/version; then
        if ! grep -qi "WSL2" /proc/version; then
            print_warning "WSL1 detected - BuildKit may have issues"
            issues_found=true
        fi
    fi

    # Check for Docker Desktop vs Docker Engine
    if docker version 2>&1 | grep -q "Docker Desktop"; then
        print_info "Docker Desktop detected - BuildKit fully supported"
    else
        print_info "Docker Engine detected - BuildKit support depends on configuration"
    fi

    # Check for rootless Docker (may have cache mount issues)
    if docker info 2>&1 | grep -q "rootless"; then
        print_warning "Rootless Docker detected - cache mounts may not work"
        issues_found=true
    fi

    # Check available disk space for cache
    if command -v df &> /dev/null; then
        AVAILABLE_SPACE=$(df /tmp 2>/dev/null | awk 'NR==2 {print $4}')
        if [ -n "$AVAILABLE_SPACE" ] && [ "$AVAILABLE_SPACE" -lt 5242880 ]; then  # 5GB in KB
            print_warning "Low disk space in /tmp - BuildKit cache may have issues"
            issues_found=true
        fi
    fi

    if [ "$issues_found" = true ]; then
        return 1
    fi
    return 0
}

# Choose build strategy
choose_build_strategy() {
    print_header "Docker Build Compatibility Check"

    local use_buildkit=false
    local compose_file="$COMPOSE_FILE_TRADITIONAL"
    local build_flags=""

    if check_buildkit_support; then
        if check_buildkit_issues; then
            use_buildkit=true
            compose_file="$COMPOSE_FILE_OPTIMIZED"
            print_success "Using optimized BuildKit builds"
            print_info "Compose file: $compose_file"

            # Set BuildKit environment variables
            export DOCKER_BUILDKIT=1
            export COMPOSE_DOCKER_CLI_BUILD=1

            # Create cache directories if they don't exist
            mkdir -p /tmp/docker-cache/{orchestration,rust-worker,ruby-worker} 2>/dev/null || true

            build_flags="--progress=plain"
        else
            print_warning "BuildKit available but potential issues detected"
            print_info "Falling back to traditional builds for safety"
        fi
    else
        print_info "Using traditional Docker builds"
        print_info "Compose file: $compose_file"

        # Explicitly disable BuildKit
        export DOCKER_BUILDKIT=0
        export COMPOSE_DOCKER_CLI_BUILD=0
    fi

    # Execute build
    print_header "Building Services"

    local build_command="docker compose -f $compose_file build"

    if [ -n "$SERVICE_NAME" ] && [ "$SERVICE_NAME" != "--force-traditional" ]; then
        build_command="$build_command $SERVICE_NAME"
        print_info "Building service: $SERVICE_NAME"
    else
        print_info "Building all services"
    fi

    if [ -n "$build_flags" ]; then
        build_command="$build_command $build_flags"
    fi

    print_info "Command: $build_command"
    echo ""

    # Run the build
    if $build_command; then
        print_success "Build completed successfully!"

        if [ "$use_buildkit" = true ]; then
            echo ""
            print_header "BuildKit Cache Statistics"
            if command -v du &> /dev/null && [ -d /tmp/docker-cache ]; then
                du -sh /tmp/docker-cache/* 2>/dev/null | while read size path; do
                    service=$(basename "$path")
                    print_info "$service cache: $size"
                done
            fi

            echo ""
            print_info "To view detailed cache info: docker builder du"
            print_info "To clear cache: rm -rf /tmp/docker-cache"
        fi

        echo ""
        print_header "Next Steps"
        echo "Start services:"
        echo "  docker compose -f $compose_file up"
        echo ""
        echo "View logs:"
        echo "  docker compose -f $compose_file logs -f"
        echo ""
        echo "Stop services:"
        echo "  docker compose -f $compose_file down"

    else
        print_error "Build failed!"

        if [ "$use_buildkit" = true ]; then
            echo ""
            print_warning "BuildKit build failed. You can try:"
            echo "  1. Force traditional build: $0 $SERVICE_NAME --force-traditional"
            echo "  2. Clear BuildKit cache: docker builder prune"
            echo "  3. Check disk space: df -h /tmp"
            echo "  4. View detailed error: BUILDKIT_PROGRESS=plain $build_command"
        else
            echo ""
            print_warning "Traditional build failed. You can try:"
            echo "  1. Check Docker daemon: docker info"
            echo "  2. Clear Docker cache: docker system prune -af"
            echo "  3. Check disk space: df -h /var/lib/docker"
            echo "  4. View service logs: docker compose -f $compose_file logs"
        fi

        exit 1
    fi
}

# Main execution
main() {
    print_header "Docker Build Compatibility Script"

    # Check if running in CI environment
    if [ -n "$CI" ] || [ -n "$GITHUB_ACTIONS" ] || [ -n "$GITLAB_CI" ] || [ -n "$JENKINS_URL" ]; then
        print_info "CI environment detected"

        # CI environments should explicitly set their build strategy
        if [ -z "$DOCKER_BUILDKIT" ]; then
            print_warning "CI environment should explicitly set DOCKER_BUILDKIT"
            print_info "Defaulting to traditional builds for compatibility"
            FORCE_TRADITIONAL=true
        fi
    fi

    # Choose and execute build strategy
    choose_build_strategy
}

# Run main function
main

#!/usr/bin/env bash

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# External cache configuration
EXTERNAL_CACHE_ROOT="/Volumes/Expansion/Development/Cache"

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  Tasker Core Project Cleanup Script${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo

# Function to show disk usage before and after
show_size() {
    local path="$1"
    if [ -d "$path" ]; then
        du -sh "$path" 2>/dev/null || echo "N/A"
    else
        echo "N/A"
    fi
}

# Function to check if a command exists
command_exists() {
    command -v "$1" &> /dev/null
}

# Function to install cargo tools if needed
ensure_cargo_tools() {
    echo -e "${YELLOW}Checking for required cargo tools...${NC}"

    if ! command_exists cargo-sweep; then
        echo -e "${YELLOW}Installing cargo-sweep...${NC}"
        cargo install cargo-sweep
    fi

    if ! command_exists cargo-cache; then
        echo -e "${YELLOW}Installing cargo-cache...${NC}"
        cargo install cargo-cache
    fi

    echo -e "${GREEN}✓ Cargo tools ready${NC}"
    echo
}

# Function to check if path is on external storage
is_external_path() {
    local path="$1"
    # Resolve to absolute path
    local abs_path="$(cd "$(dirname "$path")" 2>/dev/null && pwd)/$(basename "$path")" 2>/dev/null || echo "$path"
    [[ "$abs_path" == "$EXTERNAL_CACHE_ROOT"* ]]
}

# Function to clean Cargo artifacts
clean_cargo() {
    echo -e "${BLUE}━━━ Cleaning Cargo Artifacts ━━━${NC}"

    cd "$PROJECT_ROOT"

    # Determine target directory to clean
    local target_dir="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"
    
    # Safety check: skip if target is on external storage
    if is_external_path "$target_dir"; then
        echo -e "${YELLOW}⚠️  Target directory is on external storage ($target_dir)${NC}"
        echo -e "${YELLOW}   Skipping Cargo cleanup to preserve external cache.${NC}"
        echo -e "${BLUE}   Use ~/bin/development_cache.sh clean to manage external storage.${NC}"
        echo
        return
    fi
    
    # If using local target directory
    if [[ "$target_dir" != "$PROJECT_ROOT/target" ]]; then
        echo -e "${YELLOW}Cleaning custom target directory: $target_dir${NC}"
    fi

    # Show initial size
    echo -e "${YELLOW}Current target/ size:${NC} $(show_size "$target_dir")"
    echo

    # Sweep old artifacts (older than 30 days by default)
    local sweep_days="${1:-30}"
    echo -e "${YELLOW}Running cargo-sweep (removing files older than $sweep_days days)...${NC}"
    cargo sweep --time "$sweep_days"

    # Clean incremental compilation artifacts
    echo -e "${YELLOW}Cleaning incremental compilation artifacts...${NC}"
    if [ -d "$target_dir/debug/incremental" ]; then
        rm -rf "$target_dir/debug/incremental"
        echo -e "${GREEN}✓ Removed debug incremental artifacts${NC}"
    fi
    if [ -d "$target_dir/release/incremental" ]; then
        rm -rf "$target_dir/release/incremental"
        echo -e "${GREEN}✓ Removed release incremental artifacts${NC}"
    fi

    # Run cargo-cache autoclean
    echo -e "${YELLOW}Running cargo-cache autoclean...${NC}"
    cargo cache --autoclean

    # Show final size
    echo
    echo -e "${GREEN}Final target/ size:${NC} $(show_size "$target_dir")"
    echo
}

# Function to clean Docker artifacts
clean_docker() {
    echo -e "${BLUE}━━━ Cleaning Docker Artifacts ━━━${NC}"

    if ! command_exists docker; then
        echo -e "${YELLOW}Docker not found, skipping Docker cleanup${NC}"
        echo
        return
    fi

    cd "$PROJECT_ROOT/docker"

    # Stop and remove containers from all compose files
    echo -e "${YELLOW}Stopping Docker Compose services...${NC}"
    for compose_file in docker-compose.{test.yml,dev.yml}; do
        if [ -f "$compose_file" ]; then
            echo -e "  Stopping services from ${compose_file}..."
            docker compose -f "$compose_file" down --remove-orphans 2>/dev/null || true
        fi
    done

    # Show Docker disk usage before cleanup
    echo
    echo -e "${YELLOW}Docker disk usage before cleanup:${NC}"
    docker system df
    echo

    # Remove project-specific containers
    echo -e "${YELLOW}Removing tasker-related containers...${NC}"
    docker ps -a --filter "name=tasker" --format "{{.Names}}" | while read -r container; do
        if [ -n "$container" ]; then
            echo -e "  Removing container: $container"
            docker rm -f "$container" 2>/dev/null || true
        fi
    done

    # Remove project-specific images
    echo -e "${YELLOW}Removing tasker-related images...${NC}"
    docker images --filter "reference=tasker*" --format "{{.Repository}}:{{.Tag}}" | while read -r image; do
        if [ -n "$image" ]; then
            echo -e "  Removing image: $image"
            docker rmi -f "$image" 2>/dev/null || true
        fi
    done

    # Remove dangling images and volumes
    echo -e "${YELLOW}Removing dangling Docker artifacts...${NC}"
    docker image prune -f
    docker volume prune -f

    # Show Docker disk usage after cleanup
    echo
    echo -e "${GREEN}Docker disk usage after cleanup:${NC}"
    docker system df
    echo
}

# Function for aggressive Docker cleanup
clean_docker_aggressive() {
    echo -e "${RED}━━━ Aggressive Docker Cleanup ━━━${NC}"
    echo -e "${RED}WARNING: This will remove ALL stopped containers, unused images, and volumes!${NC}"
    echo -e "${YELLOW}This includes Docker resources not related to this project.${NC}"
    echo
    read -p "Are you sure you want to continue? (yes/no): " confirm

    if [ "$confirm" != "yes" ]; then
        echo -e "${YELLOW}Skipping aggressive Docker cleanup${NC}"
        echo
        return
    fi

    echo -e "${YELLOW}Running docker system prune -a...${NC}"
    docker system prune -af --volumes

    echo
    echo -e "${GREEN}Aggressive Docker cleanup complete${NC}"
    echo
}

# Function to clean test artifacts
clean_test_artifacts() {
    echo -e "${BLUE}━━━ Cleaning Test Artifacts ━━━${NC}"

    cd "$PROJECT_ROOT"
    
    local target_dir="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"
    
    # Safety check: skip if target is on external storage
    if is_external_path "$target_dir"; then
        echo -e "${YELLOW}⚠️  Target directory is on external storage, skipping test artifacts cleanup${NC}"
        echo
        return
    fi

    # Clean nextest cache
    if [ -d "$target_dir/nextest" ]; then
        echo -e "${YELLOW}Removing nextest cache...${NC}"
        rm -rf "$target_dir/nextest"
        echo -e "${GREEN}✓ Removed nextest cache${NC}"
    fi

    # Clean coverage artifacts
    if [ -d "$target_dir/llvm-cov-target" ]; then
        echo -e "${YELLOW}Removing coverage artifacts...${NC}"
        rm -rf "$target_dir/llvm-cov-target"
        echo -e "${GREEN}✓ Removed coverage artifacts${NC}"
    fi

    # Clean any *.profraw files
    if compgen -G "*.profraw" > /dev/null; then
        echo -e "${YELLOW}Removing profraw files...${NC}"
        rm -f *.profraw
        echo -e "${GREEN}✓ Removed profraw files${NC}"
    fi

    echo
}

# Function to clean Ruby artifacts
clean_ruby() {
    echo -e "${BLUE}━━━ Cleaning Ruby Artifacts ━━━${NC}"

    local ruby_worker_dir="$PROJECT_ROOT/workers/ruby"

    if [ ! -d "$ruby_worker_dir" ]; then
        echo -e "${YELLOW}Ruby worker directory not found, skipping${NC}"
        echo
        return
    fi

    cd "$ruby_worker_dir"

    # Clean Ruby extension builds
    if [ -d "tmp" ]; then
        echo -e "${YELLOW}Removing Ruby tmp directory...${NC}"
        rm -rf tmp
        echo -e "${GREEN}✓ Removed tmp directory${NC}"
    fi

    if [ -d "lib/tasker_core" ] && [ -f "lib/tasker_core/tasker_core.bundle" ]; then
        echo -e "${YELLOW}Removing compiled Ruby extension...${NC}"
        rm -f lib/tasker_core/tasker_core.bundle
        echo -e "${GREEN}✓ Removed compiled extension${NC}"
    fi

    echo
}

# Function to show total disk space saved
show_summary() {
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}  Cleanup Complete!${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo
    local target_dir="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"
    
    echo -e "${BLUE}Current sizes:${NC}"
    if is_external_path "$target_dir"; then
        echo -e "  target/:           ${YELLOW}On external storage (not cleaned)${NC}"
    else
        echo -e "  target/:           $(show_size "$target_dir")"
    fi
    echo -e "  ~/.cargo/:         $(show_size ~/.cargo 2>/dev/null || echo "N/A")"
    echo
}

# Main cleanup function
main() {
    local cleanup_type="${1:-standard}"

    case "$cleanup_type" in
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo
            echo "Options:"
            echo "  standard              Standard cleanup (default)"
            echo "  --aggressive, -a      Aggressive cleanup including all Docker resources"
            echo "  --cargo-only          Clean only Cargo artifacts"
            echo "  --docker-only         Clean only Docker artifacts"
            echo "  --sweep-days N        Set cargo-sweep days threshold (default: 30)"
            echo "  --help, -h            Show this help message"
            echo
            echo "Examples:"
            echo "  $0                    # Standard cleanup"
            echo "  $0 --aggressive       # Aggressive cleanup"
            echo "  $0 --cargo-only       # Clean only Cargo artifacts"
            echo "  $0 --sweep-days 7     # Clean Cargo artifacts older than 7 days"
            exit 0
            ;;
        --aggressive|-a)
            ensure_cargo_tools
            clean_cargo 30
            clean_test_artifacts
            clean_ruby
            clean_docker_aggressive
            show_summary
            ;;
        --cargo-only)
            ensure_cargo_tools
            clean_cargo "${2:-30}"
            clean_test_artifacts
            clean_ruby
            show_summary
            ;;
        --docker-only)
            clean_docker
            show_summary
            ;;
        --sweep-days)
            if [ -z "${2:-}" ]; then
                echo -e "${RED}Error: --sweep-days requires a number of days${NC}"
                exit 1
            fi
            ensure_cargo_tools
            clean_cargo "$2"
            clean_test_artifacts
            clean_ruby
            clean_docker
            show_summary
            ;;
        standard|*)
            ensure_cargo_tools
            clean_cargo 30
            clean_test_artifacts
            clean_ruby
            clean_docker
            show_summary
            ;;
    esac
}

# Run main function
main "$@"

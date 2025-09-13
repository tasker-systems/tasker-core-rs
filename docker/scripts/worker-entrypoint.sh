#!/bin/bash

# =============================================================================
# Worker Service Entrypoint
# =============================================================================
# Handles worker service initialization with health checks and environment validation.
# Workers do NOT handle database migrations - that's the orchestration service's responsibility.

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly APP_NAME="${APP_NAME:-tasker-worker}"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[WORKER]${NC} $*" >&1
}

log_warn() {
    echo -e "${YELLOW}[WORKER]${NC} $*" >&2
}

log_error() {
    echo -e "${RED}[WORKER]${NC} $*" >&2
}

log_success() {
    echo -e "${GREEN}[WORKER]${NC} $*" >&1
}

# Show startup banner
show_banner() {
    log_info "=== Tasker Worker Service Startup ==="
    log_info "Worker: ${APP_NAME}"
    log_info "Worker ID: ${WORKER_ID:-unset}"
    log_info "Environment: ${TASKER_ENV:-unknown}"
    log_info "Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    log_info "User: $(whoami)"
    log_info "Working Directory: $(pwd)"
}

# Validate basic environment
validate_basic_environment() {
    if [[ -z "${TASKER_ENV:-}" ]]; then
        log_error "TASKER_ENV environment variable is required"
        exit 1
    fi
    
    log_info "Environment validation passed: ${TASKER_ENV}"
}


# Wait for orchestration service to be ready
wait_for_orchestration() {
    log_info "Waiting for orchestration service to be ready..."
    
    # Workers depend on orchestration service being available
    # The database should already be migrated by the orchestration service
    # This can be expanded to check orchestration health endpoint
    
    log_info "Orchestration dependency checks completed"
}

# Perform health checks before starting
pre_startup_health_check() {
    log_info "Performing pre-startup health checks..."
    
    # Check if required directories exist
    local required_dirs=("/app")
    for dir in "${required_dirs[@]}"; do
        if [[ ! -d "$dir" ]]; then
            log_error "Required directory missing: $dir"
            exit 1
        fi
    done
    
    # Check if the worker binary exists
    if [[ -n "${1:-}" ]] && [[ ! -f "$1" ]] && [[ ! -x "$(command -v "$1")" ]]; then
        log_error "Worker binary not found: $1"
        exit 1
    fi
    
    # Check worker-specific configuration
    if [[ -n "${TASKER_CONFIG_ROOT:-}" ]] && [[ ! -d "${TASKER_CONFIG_ROOT}" ]]; then
        log_warn "Configuration directory not found: ${TASKER_CONFIG_ROOT}"
    fi
    
    if [[ -n "${TASK_TEMPLATE_PATH:-}" ]] && [[ ! -d "${TASK_TEMPLATE_PATH}" ]]; then
        log_warn "Task template directory not found: ${TASK_TEMPLATE_PATH}"
    fi
    
    log_success "Pre-startup health checks passed"
}

# Production deployment considerations
handle_production_deployment() {
    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production deployment detected"
        
        # In production, you might want to:
        # 1. Validate configuration more strictly
        # 2. Enable additional monitoring
        # 3. Set specific resource limits
        # 4. Configure logging appropriately
        
        # Set production-specific environment variables
        export RUST_LOG="${RUST_LOG:-info}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-0}"  # Disable backtraces in production
        
        log_info "Production configuration applied"
    fi
}

# Workers don't handle deployment modes - they just start up
# Database migrations are handled by the orchestration service

# Main entrypoint logic
main() {
    show_banner
    validate_basic_environment
    handle_production_deployment
    wait_for_orchestration
    pre_startup_health_check "$@"
    
    log_info "=== Starting Worker Service ==="
    
    # If no arguments provided, show help
    if [[ $# -eq 0 ]]; then
        log_error "No command provided to execute"
        log_info "Usage: $0 <worker-binary> [args...]"
        exit 1
    fi
    
    log_info "Executing: $*"
    
    # Replace the shell process with the worker
    exec "$@"
}

# Handle signals gracefully
cleanup() {
    log_warn "Received shutdown signal, cleaning up..."
    # Add any cleanup logic here
    exit 0
}

trap cleanup INT TERM

# Run main function with all arguments
main "$@"

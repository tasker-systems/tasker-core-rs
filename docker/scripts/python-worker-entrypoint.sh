#!/bin/bash

# =============================================================================
# Python Worker Service Entrypoint
# =============================================================================
# Python-driven worker that bootstraps Rust foundation via FFI (PyO3)
# Python handles handler discovery and execution, Rust provides core infrastructure

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly APP_NAME="${APP_NAME:-tasker-python-worker}"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[PYTHON-WORKER]${NC} $*" >&1
}

log_warn() {
    echo -e "${YELLOW}[PYTHON-WORKER]${NC} $*" >&2
}

log_error() {
    echo -e "${RED}[PYTHON-WORKER]${NC} $*" >&2
}

log_success() {
    echo -e "${GREEN}[PYTHON-WORKER]${NC} $*" >&1
}

# Show startup banner
show_banner() {
    log_info "=== Tasker Python Worker Service Startup ==="
    log_info "Worker: ${APP_NAME}"
    log_info "Environment: ${TASKER_ENV:-development}"
    log_info "Python Version: ${PYTHON_VERSION:-unknown}"
    log_info "Database URL: ${DATABASE_URL:-unset}"
    log_info "Template Path: ${TASKER_TEMPLATE_PATH:-unset}"
    log_info "Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    log_info "User: $(whoami)"
    log_info "Working Directory: $(pwd)"
}

# Validate Python worker environment
validate_python_environment() {
    if [[ -z "${TASKER_ENV:-}" ]]; then
        log_error "TASKER_ENV environment variable is required"
        exit 1
    fi

    if [[ -z "${DATABASE_URL:-}" ]]; then
        log_error "DATABASE_URL environment variable is required"
        exit 1
    fi

    log_info "Environment validation passed: ${TASKER_ENV}"
}

# Wait for database to be ready
wait_for_database() {
    log_info "Waiting for database connection..."

    local timeout=${TASKER_DB_TIMEOUT:-60}
    local retry_interval=2

    # Extract database host and port from DATABASE_URL
    local db_host=$(echo "$DATABASE_URL" | cut -d@ -f2 | cut -d: -f1)
    local db_port=$(echo "$DATABASE_URL" | grep -o ":[0-9]*/" | tr -d ":/")
    [[ -z "$db_port" ]] && db_port=5432

    timeout "$timeout" bash -c "
        until pg_isready -h \"$db_host\" -p \"$db_port\" > /dev/null 2>&1; do
            sleep $retry_interval
        done
    " || {
        log_error "Database failed to become ready within $timeout seconds"
        exit 1
    }

    log_success "Database connection ready"
}

# Validate Python worker specific environment
validate_python_worker_environment() {
    log_info "Validating Python worker environment..."

    # Check if Python source exists
    if [[ ! -d "python" ]]; then
        log_error "Python source directory not found"
        exit 1
    fi

    # Check if tasker_core module exists
    if [[ ! -d "python/tasker_core" ]]; then
        log_error "TaskerCore module not found at python/tasker_core"
        exit 1
    fi

    # Check if server script exists
    if [[ ! -f "bin/server.py" ]]; then
        log_error "Server script not found at bin/server.py"
        exit 1
    fi

    # Check template path if provided
    if [[ -n "${TASKER_TEMPLATE_PATH:-}" ]] && [[ ! -d "${TASKER_TEMPLATE_PATH}" ]]; then
        log_warn "Task template directory not found: ${TASKER_TEMPLATE_PATH}"
        log_warn "Python handlers may not be discoverable"
    fi

    # Check handler path if provided
    if [[ -n "${PYTHON_HANDLER_PATH:-}" ]] && [[ ! -d "${PYTHON_HANDLER_PATH}" ]]; then
        log_warn "Python handler directory not found: ${PYTHON_HANDLER_PATH}"
        log_warn "Python handlers may not be discoverable"
    fi

    log_success "Python worker environment validation passed"
}

# Handle production-specific Python configuration
handle_production_deployment() {
    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production deployment detected"

        # Python production optimizations
        export PYTHONOPTIMIZE="${PYTHONOPTIMIZE:-2}"
        export PYTHONHASHSEED="${PYTHONHASHSEED:-random}"

        # Rust logging for production
        export RUST_LOG="${RUST_LOG:-info}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-0}"

        log_info "Production Python and Rust configuration applied"
    else
        # Development/test configuration
        export RUST_LOG="${RUST_LOG:-debug}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

        log_info "Development Python and Rust configuration applied"
    fi
}

# Start Python worker bootstrap system
start_python_worker() {
    log_info "Starting Python worker bootstrap process..."
    log_info "This will bootstrap Rust foundation via FFI and start worker processing"

    log_info "Python will initialize Rust foundation via FFI (PyO3)"
    log_info "Worker will process tasks by calling Python handlers"

    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production optimizations: Enabled"
    fi

    # Start the Python worker server script
    log_info "Executing Python worker server: bin/server.py"
    exec python bin/server.py
}

# Main entrypoint logic
main() {
    show_banner
    validate_python_environment
    handle_production_deployment
    wait_for_database
    validate_python_worker_environment

    log_info "=== Starting Python Worker Service ==="
    log_info "Python worker will bootstrap Rust foundation via FFI (PyO3)"

    start_python_worker
}

# Handle signals gracefully
cleanup() {
    log_warn "Received shutdown signal, cleaning up Python worker..."
    # Python bootstrap will handle cleanup via signal handlers
    exit 0
}

trap cleanup INT TERM

# Run main function
main "$@"

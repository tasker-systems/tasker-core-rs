#!/bin/bash

# =============================================================================
# TypeScript Worker Service Entrypoint
# =============================================================================
# TypeScript/Bun-driven worker that bootstraps Rust foundation via FFI
# TypeScript handles handler discovery and execution, Rust provides core infrastructure

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly APP_NAME="${APP_NAME:-tasker-typescript-worker}"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[TYPESCRIPT-WORKER]${NC} $*" >&1
}

log_warn() {
    echo -e "${YELLOW}[TYPESCRIPT-WORKER]${NC} $*" >&2
}

log_error() {
    echo -e "${RED}[TYPESCRIPT-WORKER]${NC} $*" >&2
}

log_success() {
    echo -e "${GREEN}[TYPESCRIPT-WORKER]${NC} $*" >&1
}

# Show startup banner
show_banner() {
    log_info "=== Tasker TypeScript Worker Service Startup ==="
    log_info "Worker: ${APP_NAME}"
    log_info "Environment: ${TASKER_ENV:-development}"
    log_info "Bun Version: ${BUN_VERSION:-unknown}"
    log_info "Database URL: ${DATABASE_URL:-unset}"
    log_info "Template Path: ${TASKER_TEMPLATE_PATH:-unset}"
    log_info "Handler Path: ${TYPESCRIPT_HANDLER_PATH:-unset}"
    log_info "FFI Library: ${TASKER_FFI_LIBRARY_PATH:-unset}"
    log_info "Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    log_info "User: $(whoami)"
    log_info "Working Directory: $(pwd)"
}

# Validate TypeScript worker environment
validate_typescript_environment() {
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

# Validate TypeScript worker specific environment
validate_typescript_worker_environment() {
    log_info "Validating TypeScript worker environment..."

    # Check if server script exists
    if [[ ! -f "bin/server.ts" ]]; then
        log_error "Server script not found at bin/server.ts"
        exit 1
    fi

    # Check FFI library
    if [[ -n "${TASKER_FFI_LIBRARY_PATH:-}" ]]; then
        if [[ -f "${TASKER_FFI_LIBRARY_PATH}" ]]; then
            log_info "FFI library found: ${TASKER_FFI_LIBRARY_PATH}"
        else
            log_error "FFI library not found at: ${TASKER_FFI_LIBRARY_PATH}"
            exit 1
        fi
    else
        # Try to find the library in default locations
        if [[ -f "/app/lib/libtasker_worker.so" ]]; then
            export TASKER_FFI_LIBRARY_PATH="/app/lib/libtasker_worker.so"
            log_info "FFI library found: ${TASKER_FFI_LIBRARY_PATH}"
        elif [[ -f "/app/lib/libtasker_worker.dylib" ]]; then
            export TASKER_FFI_LIBRARY_PATH="/app/lib/libtasker_worker.dylib"
            log_info "FFI library found: ${TASKER_FFI_LIBRARY_PATH}"
        else
            log_error "FFI library not found. Set TASKER_FFI_LIBRARY_PATH or ensure library is in /app/lib/"
            exit 1
        fi
    fi

    # Check template path if provided
    if [[ -n "${TASKER_TEMPLATE_PATH:-}" ]] && [[ ! -d "${TASKER_TEMPLATE_PATH}" ]]; then
        log_warn "Task template directory not found: ${TASKER_TEMPLATE_PATH}"
        log_warn "TypeScript handlers may not be discoverable"
    fi

    # Check handler path if provided
    if [[ -n "${TYPESCRIPT_HANDLER_PATH:-}" ]] && [[ ! -d "${TYPESCRIPT_HANDLER_PATH}" ]]; then
        log_warn "TypeScript handler directory not found: ${TYPESCRIPT_HANDLER_PATH}"
        log_warn "TypeScript handlers may not be discoverable"
    fi

    log_success "TypeScript worker environment validation passed"
}

# Handle production-specific TypeScript configuration
handle_production_deployment() {
    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production deployment detected"

        # Rust logging for production
        export RUST_LOG="${RUST_LOG:-info}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-0}"

        log_info "Production TypeScript and Rust configuration applied"
    else
        # Development/test configuration
        export RUST_LOG="${RUST_LOG:-debug}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

        log_info "Development TypeScript and Rust configuration applied"
    fi
}

# Start TypeScript worker bootstrap system
start_typescript_worker() {
    log_info "Starting TypeScript worker bootstrap process..."
    log_info "This will bootstrap Rust foundation via FFI and start worker processing"

    log_info "TypeScript will initialize Rust foundation via FFI"
    log_info "Worker will process tasks by calling TypeScript handlers"

    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production optimizations: Enabled"
    fi

    # Start the TypeScript worker server script
    log_info "Executing TypeScript worker server: bin/server.ts"
    exec bun run bin/server.ts
}

# Main entrypoint logic
main() {
    show_banner
    validate_typescript_environment
    handle_production_deployment
    wait_for_database
    validate_typescript_worker_environment

    log_info "=== Starting TypeScript Worker Service ==="
    log_info "TypeScript worker will bootstrap Rust foundation via FFI"

    start_typescript_worker
}

# Handle signals gracefully
cleanup() {
    log_warn "Received shutdown signal, cleaning up TypeScript worker..."
    # TypeScript bootstrap will handle cleanup via signal handlers
    exit 0
}

trap cleanup INT TERM

# Run main function
main "$@"

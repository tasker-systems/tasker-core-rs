#!/bin/bash

# =============================================================================
# Ruby Worker Service Entrypoint
# =============================================================================
# Ruby-driven worker that bootstraps Rust foundation via FFI
# Ruby handles handler discovery and execution, Rust provides core infrastructure

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly APP_NAME="${APP_NAME:-tasker-ruby-worker}"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[RUBY-WORKER]${NC} $*" >&1
}

log_warn() {
    echo -e "${YELLOW}[RUBY-WORKER]${NC} $*" >&2
}

log_error() {
    echo -e "${RED}[RUBY-WORKER]${NC} $*" >&2
}

log_success() {
    echo -e "${GREEN}[RUBY-WORKER]${NC} $*" >&1
}

# Show startup banner
show_banner() {
    log_info "=== Tasker Ruby Worker Service Startup ==="
    log_info "Worker: ${APP_NAME}"
    log_info "Environment: ${TASKER_ENV:-development}"
    log_info "Ruby Version: ${RUBY_VERSION:-unknown}"
    log_info "Database URL: ${DATABASE_URL:-unset}"
    log_info "Template Path: ${TASKER_TEMPLATE_PATH:-unset}"
    log_info "Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    log_info "User: $(whoami)"
    log_info "Working Directory: $(pwd)"
}

# Validate Ruby worker environment
validate_ruby_environment() {
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

# Validate Ruby worker specific environment
validate_ruby_worker_environment() {
    log_info "Validating Ruby worker environment..."

    # Check if Gemfile exists
    if [[ ! -f "Gemfile" ]]; then
        log_error "Gemfile not found in current directory"
        exit 1
    fi

    # Check if TaskerCore lib exists
    if [[ ! -f "lib/tasker_core.rb" ]]; then
        log_error "TaskerCore library not found at lib/tasker_core.rb"
        exit 1
    fi

    # Check Ruby FFI extensions compiled
    if [[ ! -d "lib/tasker_core" ]]; then
        log_error "TaskerCore extensions not found. Run 'bundle exec rake compile' first"
        exit 1
    fi

    # Check template path if provided
    if [[ -n "${TASKER_TEMPLATE_PATH:-}" ]] && [[ ! -d "${TASKER_TEMPLATE_PATH}" ]]; then
        log_warn "Task template directory not found: ${TASKER_TEMPLATE_PATH}"
        log_warn "Ruby handlers may not be discoverable"
    fi

    # Check handler path if provided
    if [[ -n "${RUBY_HANDLER_PATH:-}" ]] && [[ ! -d "${RUBY_HANDLER_PATH}" ]]; then
        log_warn "Ruby handler directory not found: ${RUBY_HANDLER_PATH}"
        log_warn "Ruby handlers may not be discoverable"
    fi

    log_success "Ruby worker environment validation passed"
}

# Handle production-specific Ruby configuration
handle_production_deployment() {
    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production deployment detected"

        # Ruby production optimizations
        export RUBY_GC_HEAP_GROWTH_FACTOR="${RUBY_GC_HEAP_GROWTH_FACTOR:-1.1}"
        export RUBY_GC_HEAP_GROWTH_MAX_SLOTS="${RUBY_GC_HEAP_GROWTH_MAX_SLOTS:-100000}"
        export RUBY_GC_HEAP_INIT_SLOTS="${RUBY_GC_HEAP_INIT_SLOTS:-600000}"

        # Rust logging for production
        export RUST_LOG="${RUST_LOG:-info}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-0}"

        log_info "Production Ruby and Rust configuration applied"
    else
        # Development/test configuration
        export RUST_LOG="${RUST_LOG:-debug}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

        log_info "Development Ruby and Rust configuration applied"
    fi
}

# Start Ruby worker bootstrap system
start_ruby_worker() {
    log_info "Starting Ruby worker bootstrap process..."
    log_info "This will bootstrap Rust foundation via FFI and start worker processing"

    # Set up Ruby environment
    export BUNDLE_GEMFILE="${BUNDLE_GEMFILE:-$(pwd)/Gemfile}"

    log_info "Ruby will initialize Rust foundation via FFI"
    log_info "Worker will process tasks by calling Ruby handlers"

    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production optimizations: Enabled"
    fi

    # Start the Ruby worker server script
    log_info "Executing Ruby worker server: bin/server.rb"
    exec bundle exec ruby bin/server.rb
}

# Main entrypoint logic
main() {
    show_banner
    validate_ruby_environment
    handle_production_deployment
    wait_for_database
    validate_ruby_worker_environment

    log_info "=== Starting Ruby Worker Service ==="
    log_info "Ruby worker will bootstrap Rust foundation via FFI"

    start_ruby_worker
}

# Handle signals gracefully
cleanup() {
    log_warn "Received shutdown signal, cleaning up Ruby worker..."
    # Ruby bootstrap will handle cleanup via signal handlers
    exit 0
}

trap cleanup INT TERM

# Run main function
main "$@"

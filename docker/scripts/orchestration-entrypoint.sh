#!/bin/bash

# =============================================================================
# Orchestration Service Entrypoint
# =============================================================================
# Handles orchestration service initialization including database migrations.
# This is only used by orchestration services - workers have no migration responsibilities.

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[ORCHESTRATION]${NC} $*" >&1
}

log_warn() {
    echo -e "${YELLOW}[ORCHESTRATION]${NC} $*" >&2
}

log_error() {
    echo -e "${RED}[ORCHESTRATION]${NC} $*" >&2
}

log_success() {
    echo -e "${GREEN}[ORCHESTRATION]${NC} $*" >&1
}

# Show startup banner
show_banner() {
    log_info "=== Tasker Orchestration Service Startup ==="
    log_info "Environment: ${TASKER_ENV:-unknown}"
    log_info "Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    log_info "User: $(whoami)"
    log_info "Working Directory: $(pwd)"
}

# Validate basic environment
validate_environment() {
    local missing_vars=()
    
    [[ -z "${TASKER_ENV:-}" ]] && missing_vars+=("TASKER_ENV")
    [[ -z "${DATABASE_URL:-}" ]] && missing_vars+=("DATABASE_URL")
    
    if [[ ${#missing_vars[@]} -gt 0 ]]; then
        log_error "Missing required environment variables: ${missing_vars[*]}"
        exit 1
    fi
    
    log_info "Environment validation passed: ${TASKER_ENV}"
}

# Run database migrations (orchestration responsibility)
run_database_migrations() {
    local migration_script="${SCRIPT_DIR}/migrate.sh"
    local deployment_mode="${DEPLOYMENT_MODE:-standard}"
    
    case "$deployment_mode" in
        "migrate-only")
            log_info "Running in migrate-only mode - will exit after migrations"
            ;;
        "no-migrate")
            log_info "Migration disabled by deployment mode"
            return 0
            ;;
        "standard"|*)
            log_info "Running database migrations (standard deployment mode)"
            ;;
    esac
    
    if [[ "${RUN_MIGRATIONS:-true}" == "false" ]]; then
        log_info "Database migrations disabled (RUN_MIGRATIONS=false)"
        return 0
    fi
    
    if [[ ! -f "$migration_script" ]]; then
        log_error "Migration script not found: $migration_script"
        exit 1
    fi
    
    if [[ ! -x "$migration_script" ]]; then
        log_error "Migration script is not executable: $migration_script"
        exit 1
    fi
    
    log_info "Executing database migrations..."
    
    if "$migration_script"; then
        log_success "Database migrations completed successfully"
        
        # Handle migrate-only mode
        if [[ "$deployment_mode" == "migrate-only" ]]; then
            log_success "Migration-only deployment completed - exiting"
            exit 0
        fi
    else
        log_error "Database migrations failed"
        exit 1
    fi
}

# Production deployment considerations
handle_production_config() {
    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production environment - applying production configuration"
        
        # Set production-specific defaults if not already set
        export RUST_LOG="${RUST_LOG:-info}"
        export RUST_BACKTRACE="${RUST_BACKTRACE:-0}"
        
        log_info "Production configuration applied"
    fi
}

# Health check before starting
pre_startup_checks() {
    log_info "Performing pre-startup checks..."
    
    # Verify the orchestration binary exists
    if [[ -n "${1:-}" ]] && [[ ! -f "$1" ]] && [[ ! -x "$(command -v "$1")" ]]; then
        log_error "Orchestration binary not found: $1"
        exit 1
    fi
    
    # Check configuration directory
    if [[ -n "${TASKER_CONFIG_ROOT:-}" ]] && [[ ! -d "${TASKER_CONFIG_ROOT}" ]]; then
        log_warn "Configuration directory not found: ${TASKER_CONFIG_ROOT}"
    fi
    
    log_success "Pre-startup checks passed"
}

# Main entrypoint logic
main() {
    show_banner
    validate_environment
    handle_production_config
    run_database_migrations
    pre_startup_checks "$@"
    
    log_info "=== Starting Orchestration Service ==="
    
    if [[ $# -eq 0 ]]; then
        log_error "No command provided to execute"
        log_info "Usage: $0 <orchestration-binary> [args...]"
        exit 1
    fi
    
    log_info "Executing: $*"
    
    # Replace shell with orchestration service
    exec "$@"
}

# Handle signals gracefully
cleanup() {
    log_warn "Received shutdown signal, cleaning up..."
    exit 0
}

trap cleanup INT TERM

# Run main function with all arguments
main "$@"

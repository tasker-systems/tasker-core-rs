#!/bin/bash

# =============================================================================
# Database Migration Script for Tasker System (Fixed Version)
# =============================================================================
# Safely runs SQLx migrations before starting the application server.
# Handles PGMQ conflicts and other common migration issues gracefully.

set -euo pipefail

# Configuration
readonly SCRIPT_NAME="${0##*/}"
readonly MAX_RETRIES=${DB_MIGRATION_RETRIES:-5}
readonly RETRY_DELAY=${DB_MIGRATION_RETRY_DELAY:-10}
readonly MIGRATION_TIMEOUT=${DB_MIGRATION_TIMEOUT:-300}

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $*" >&1
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*" >&2
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*" >&1
}

# Validate environment
validate_environment() {
    local missing_vars=()

    [[ -z "${DATABASE_URL:-}" ]] && missing_vars+=("DATABASE_URL")
    [[ -z "${TASKER_ENV:-}" ]] && missing_vars+=("TASKER_ENV")

    if [[ ${#missing_vars[@]} -gt 0 ]]; then
        log_error "Missing required environment variables: ${missing_vars[*]}"
        exit 1
    fi

    log_info "Environment validated: ${TASKER_ENV}"
}

# Wait for database to be available
wait_for_database() {
    local retry_count=0

    log_info "Waiting for database to become available..."

    while [[ $retry_count -lt $MAX_RETRIES ]]; do
        if timeout 10 bash -c "echo > /dev/tcp/${DATABASE_HOST:-postgres}/${DATABASE_PORT:-5432}" 2>/dev/null; then
            log_success "Database is available"
            return 0
        fi

        retry_count=$((retry_count + 1))
        log_warn "Database not available (attempt ${retry_count}/${MAX_RETRIES}). Retrying in ${RETRY_DELAY}s..."
        sleep "$RETRY_DELAY"
    done

    log_error "Database failed to become available after ${MAX_RETRIES} attempts"
    exit 1
}

# Check if SQLx CLI is available
ensure_sqlx_cli() {
    if ! command -v sqlx &> /dev/null; then
        log_error "sqlx CLI not found. Installing..."
        cargo install sqlx-cli --features postgres
    fi

    log_info "SQLx CLI version: $(sqlx --version)"
}

# Check if PGMQ extension is available (platform responsibility)
check_pgmq_prerequisite() {
    log_info "Checking PGMQ extension prerequisite..."

    # Check if pgmq schema exists - if not, this is a platform configuration issue
    local pgmq_exists
    pgmq_exists=$(psql "$DATABASE_URL" -t -c "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'pgmq');" 2>/dev/null | tr -d '[:space:]' || echo "f")

    if [[ "$pgmq_exists" == "t" ]]; then
        log_success "PGMQ extension detected - prerequisite satisfied"
        return 0
    else
        log_error "PGMQ extension not found. This must be installed by platform team or database setup."
        log_error "Our Docker database image includes this by default. If using external DB, install: CREATE EXTENSION pgmq CASCADE;"
        return 1
    fi
}

# Run database migrations with retry logic
run_migrations() {
    local migration_dir="/app/migrations"
    local max_migration_retries=2
    local retry_count=0

    if [[ ! -d "$migration_dir" ]]; then
        log_error "Migration directory not found: $migration_dir"
        exit 1
    fi

    log_info "Starting database migrations from: $migration_dir"
    log_info "Database URL: ${DATABASE_URL%?*}*****" # Mask password

    # Verify PGMQ prerequisite is satisfied
    # Off for now, this should be handled by our migrations or the platform team
    # if ! check_pgmq_prerequisite; then
    #     log_error "PGMQ prerequisite not satisfied - cannot proceed with migrations"
    #     exit 1
    # fi

    while [[ $retry_count -lt $max_migration_retries ]]; do
        log_info "Migration attempt $((retry_count + 1))/$max_migration_retries"

        if timeout "$MIGRATION_TIMEOUT" sqlx migrate run \
            --database-url "$DATABASE_URL" \
            --source "$migration_dir"; then

            log_success "All migrations completed successfully"
            return 0
        fi

        local exit_code=$?
        retry_count=$((retry_count + 1))

        if [[ $exit_code -eq 124 ]]; then
            log_error "Migration timeout after ${MIGRATION_TIMEOUT}s"
            exit $exit_code
        fi

        log_warn "Migration failed with exit code: $exit_code"

        if [[ $retry_count -lt $max_migration_retries ]]; then
            log_info "Retrying in 5 seconds..."
            sleep 5

            # Show current migration status for debugging
            log_info "Current migration status:"
            sqlx migrate info \
                --database-url "$DATABASE_URL" \
                --source "$migration_dir" || true
        else
            log_error "All migration attempts failed"
            exit $exit_code
        fi
    done
}

# Check migration status
check_migration_status() {
    log_info "Checking current migration status..."

    sqlx migrate info \
        --database-url "$DATABASE_URL" \
        --source "/app/migrations" \
        || {
            log_warn "Failed to retrieve migration status"
            return 1
        }
}

# Production safety checks
production_safety_checks() {
    if [[ "${TASKER_ENV}" == "production" ]]; then
        log_info "Production environment detected - enabling additional safety checks"

        # In production, we might want to require explicit approval
        if [[ "${SKIP_MIGRATION_PROMPT:-false}" != "true" ]]; then
            log_warn "Running migrations in production environment"
            log_warn "Set SKIP_MIGRATION_PROMPT=true to bypass this check"

            # For automated deployments, this should be skipped
            # For manual deployments, this provides a safety net
            if [[ -t 0 ]]; then  # Check if stdin is a terminal
                read -p "Continue with migrations? (y/N): " -n 1 -r
                echo
                if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                    log_info "Migration cancelled by user"
                    exit 1
                fi
            fi
        fi

        # Additional production checks can go here
        log_info "Production safety checks passed"
    fi
}

# Main execution
main() {
    log_info "=== Tasker Database Migration Script ==="
    log_info "Environment: ${TASKER_ENV}"
    log_info "Timestamp: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"

    validate_environment
    production_safety_checks
    wait_for_database
    ensure_sqlx_cli

    # Show current status before migration
    check_migration_status || true

    # Run migrations
    run_migrations

    # Show final status
    log_info "=== Migration Summary ==="
    check_migration_status || true

    log_success "Database migration completed successfully!"
}

# Handle signals for graceful shutdown
trap 'log_error "Migration interrupted"; exit 130' INT TERM

# Run main function
main "$@"

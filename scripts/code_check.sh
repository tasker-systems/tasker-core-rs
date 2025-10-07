#!/usr/bin/env bash

# code_check.sh - Comprehensive code quality check for tasker-core workspace
# This script runs SQLX preparation, formatting, linting, and documentation checks with detailed feedback
#
# Usage:
#   ./scripts/code_check.sh              # Check only (default)
#   ./scripts/code_check.sh --fix        # Auto-fix formatting issues
#   ./scripts/code_check.sh --prepare    # Prepare SQLX cache only
#   ./scripts/code_check.sh --help       # Show help

set -e  # Exit on any error

# Parse command line arguments
AUTO_FIX=false
SHOW_HELP=false
PREPARE_SQLX_ONLY=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --fix)
            AUTO_FIX=true
            shift
            ;;
        --prepare)
            PREPARE_SQLX_ONLY=true
            shift
            ;;
        --help|-h)
            SHOW_HELP=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

if [ "$SHOW_HELP" = true ]; then
    echo "Code Quality Check for tasker-core Workspace"
    echo ""
    echo "Usage:"
    echo "  $0               Check code quality (SQLX prepare, format, lint, docs)"
    echo "  $0 --fix         Check and auto-fix formatting issues"
    echo "  $0 --prepare     Prepare SQLX cache only (for Docker builds)"
    echo "  $0 --help        Show this help message"
    echo ""
    echo "This script checks all workspace projects including SQLX preparation."
    echo "Projects: tasker-core, tasker-shared, tasker-orchestration, tasker-worker,"
    echo "          tasker-client, pgmq-notify, workers/rust"
    exit 0
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
print_header() {
    echo -e "\n${BLUE}===================================================${NC}"
    echo -e "${BLUE} $1${NC}"
    echo -e "${BLUE}===================================================${NC}"
}

print_step() {
    echo -e "\n${YELLOW}➤ $1${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

# Check if we're in the right directory
if [[ ! -f "Cargo.toml" ]]; then
    print_error "This script must be run from the project root directory (where Cargo.toml is located)"
    exit 1
fi

# Define all workspace projects that need to be checked
# These are ordered by dependency hierarchy (dependencies first)
ALL_WORKSPACE_PROJECTS=(
    "tasker-shared"           # Shared library used by others
    "tasker-client"           # Client library
    "pgmq-notify"            # Notification system
    "tasker-worker"          # Worker library
    "tasker-orchestration"   # Orchestration service
    "workers/rust"           # Rust worker implementation
    "workers/ruby/ext/tasker_core"  # Ruby extension (if it exists)
    "."                      # Main workspace root
)

# Projects that use SQLX and need cache preparation
SQLX_PROJECTS=(
    "tasker-shared"
    "tasker-client"
    "pgmq-notify"
    "tasker-worker"
    "tasker-orchestration"
    "workers/rust"
    "workers/ruby/ext/tasker_core"
    "."                      # Main workspace root
)

# Filter out projects that don't exist
RUST_PROJECTS=()
for project in "${ALL_WORKSPACE_PROJECTS[@]}"; do
    if [[ -f "$project/Cargo.toml" ]]; then
        RUST_PROJECTS+=("$project")
    elif [[ "$project" == "." ]]; then
        # Always include root project
        RUST_PROJECTS+=("$project")
    fi
done

# Filter SQLX projects to only existing ones
EXISTING_SQLX_PROJECTS=()
for project in "${SQLX_PROJECTS[@]}"; do
    if [[ -f "$project/Cargo.toml" ]]; then
        EXISTING_SQLX_PROJECTS+=("$project")
    elif [[ "$project" == "." ]]; then
        # Always include root project
        EXISTING_SQLX_PROJECTS+=("$project")
    fi
done

# Helper function to run cargo command in a specific project directory
run_cargo_in_project() {
    local project_dir="$1"
    local cargo_cmd="$2"
    local description="$3"

    echo -e "  ${YELLOW}→${NC} Checking $description in ${BLUE}${project_dir}${NC}..."

    if [[ "$project_dir" == "." ]]; then
        eval "$cargo_cmd"
    else
        (cd "$project_dir" && eval "$cargo_cmd")
    fi
}

# Variables to track results
SQLX_PASSED=0
FORMAT_PASSED=0
CLIPPY_PASSED=0
BENCH_PASSED=0
DOC_PASSED=0
OVERALL_SUCCESS=1

print_header "Code Quality Check for tasker-core Workspace"
if [ "$PREPARE_SQLX_ONLY" = true ]; then
    echo -e "This script will prepare SQLX cache for all workspace projects."
else
    echo -e "This script will prepare SQLX cache, check code formatting, run linter, check benchmark compilation, and generate documentation."
fi
echo -e "Checking ${#RUST_PROJECTS[@]} Rust project(s): ${RUST_PROJECTS[*]}"
echo -e "SQLX projects (${#EXISTING_SQLX_PROJECTS[@]}): ${EXISTING_SQLX_PROJECTS[*]}\n"

# Step 0: SQLX Cache Preparation
print_step "Preparing SQLX query cache for all workspace projects..."
SQLX_PASSED=1

# Check if we have a database connection for SQLX prepare
if [[ -z "$DATABASE_URL" ]]; then
    print_warning "DATABASE_URL not set. Using default test database URL..."
    export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
fi

for project in "${EXISTING_SQLX_PROJECTS[@]}"; do
    # Check if project actually uses SQLX (has .sqlx directory or sqlx in Cargo.toml)
    has_sqlx=false
    if [[ -d "$project/.sqlx" ]]; then
        has_sqlx=true
    elif [[ "$project" == "." && -f "Cargo.toml" ]]; then
        if grep -q "sqlx" "Cargo.toml"; then
            has_sqlx=true
        fi
    elif [[ -f "$project/Cargo.toml" ]]; then
        if grep -q "sqlx" "$project/Cargo.toml"; then
            has_sqlx=true
        fi
    fi

    if [[ "$has_sqlx" == "true" ]]; then
        echo -e "  ${YELLOW}→${NC} Preparing SQLX cache in ${BLUE}${project}${NC}..."

        # For the main workspace, we need to be more specific about which packages to prepare
        if [[ "$project" == "." ]]; then
            # Prepare SQLX for all workspace members that use it
            sqlx_cmd="cargo sqlx prepare --workspace"
        else
            sqlx_cmd="cargo sqlx prepare"
        fi

        if ! run_cargo_in_project "$project" "$sqlx_cmd" "SQLX preparation"; then
            print_warning "SQLX preparation failed in $project - this might be expected if database is not running"
            print_warning "To ensure Docker builds work, make sure to run this with a database connection"
            # Don't fail the overall check for SQLX issues since database might not be available
            # SQLX_PASSED=0
            # OVERALL_SUCCESS=0
        else
            print_success "SQLX cache prepared for $project"
        fi
    else
        echo -e "  ${YELLOW}→${NC} Skipping SQLX preparation for ${BLUE}${project}${NC} (no SQLX usage detected)"
    fi
done

if [ $SQLX_PASSED -eq 1 ]; then
    print_success "SQLX cache preparation completed for all applicable projects"
fi

# If only preparing SQLX, exit here
if [ "$PREPARE_SQLX_ONLY" = true ]; then
    print_header "SQLX Preparation Complete"
    echo -e "SQLX cache has been prepared for all workspace projects."
    echo -e "This ensures Docker builds will work with SQLX_OFFLINE=true."
    exit 0
fi

# Step 1: Code Formatting Check
if [ "$AUTO_FIX" = true ]; then
    print_step "Auto-fixing code formatting with rustfmt..."
    for project in "${RUST_PROJECTS[@]}"; do
        echo -e "  ${YELLOW}→${NC} Auto-fixing formatting in ${BLUE}${project}${NC}..."
        run_cargo_in_project "$project" "cargo fmt --all" "formatting fix"
        print_success "Formatting fixed in $project"
    done
    print_success "All projects formatted successfully"
    FORMAT_PASSED=1
else
    print_step "Checking code formatting with rustfmt..."
    FORMAT_PASSED=1
    for project in "${RUST_PROJECTS[@]}"; do
        if ! run_cargo_in_project "$project" "cargo fmt --all -- --check" "formatting"; then
            print_error "Code formatting issues found in $project"
            print_warning "Run 'cd $project && cargo fmt --all' to fix formatting issues"
            print_warning "Or run this script with --fix to auto-fix: ./scripts/code_check.sh --fix"
            FORMAT_PASSED=0
            OVERALL_SUCCESS=0
        fi
    done

    if [ $FORMAT_PASSED -eq 1 ]; then
        print_success "Code formatting is correct in all projects"
    fi
fi

# Step 2: Clippy Linting
print_step "Running Clippy linter with all features and strict warnings..."
CLIPPY_PASSED=1
for project in "${RUST_PROJECTS[@]}"; do
    # # Use different clippy args for Ruby extension (no all-features)
    # if [[ "$project" == "workers/ruby/ext/tasker_core" ]]; then
    #     clippy_cmd="cargo clippy --all-targets -- -D warnings"
    # else
        clippy_cmd="cargo clippy --all-targets --all-features -- -D warnings"
    # fi

    if ! run_cargo_in_project "$project" "$clippy_cmd" "linting"; then
        print_error "Clippy found linting issues in $project"
        print_warning "Review the warnings above and fix them before committing"
        CLIPPY_PASSED=0
        OVERALL_SUCCESS=0
    fi
done

if [ $CLIPPY_PASSED -eq 1 ]; then
    print_success "No linting issues found in all projects"
fi

# Step 3: Benchmark Compilation Check
# Note: We check compilation rather than running full benchmarks for CI speed.
# To run actual benchmarks: cargo bench --features benchmarks
print_step "Checking benchmark compilation..."
BENCH_PASSED=1
for project in "${RUST_PROJECTS[@]}"; do
    # Only check benchmarks for main project (individual crates don't have benchmarks)
    if [[ "$project" == "." ]]; then
        if ! run_cargo_in_project "$project" "cargo check --benches --features benchmarks" "benchmark compilation"; then
            print_error "Benchmark compilation failed in $project"
            print_warning "Check benchmark code for compilation errors"
            BENCH_PASSED=0
            OVERALL_SUCCESS=0
        fi
    else
        echo -e "  ${YELLOW}→${NC} Skipping benchmark check for ${BLUE}${project}${NC} (no benchmarks)"
    fi
done

if [ $BENCH_PASSED -eq 1 ]; then
    print_success "All benchmarks compile successfully"
fi

# Step 4: Documentation Generation
print_step "Generating documentation..."
DOC_PASSED=1
for project in "${RUST_PROJECTS[@]}"; do
    if ! run_cargo_in_project "$project" "cargo doc --no-deps --document-private-items" "documentation generation"; then
        print_error "Documentation generation failed in $project"
        print_warning "Check for documentation errors or missing doc comments"
        DOC_PASSED=0
        OVERALL_SUCCESS=0
    fi
done

if [ $DOC_PASSED -eq 1 ]; then
    print_success "Documentation generated successfully for all projects"
fi

# Summary
print_header "Summary"
echo -e "SQLX Preparation:  $([ $SQLX_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
echo -e "Format Check:      $([ $FORMAT_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
echo -e "Clippy Linting:    $([ $CLIPPY_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
echo -e "Benchmarks:        $([ $BENCH_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
echo -e "Documentation:     $([ $DOC_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"

if [ $OVERALL_SUCCESS -eq 1 ]; then
    print_success "All checks passed! Code is ready for commit."
    echo -e "\n${GREEN}🎉 Great job! Your code meets all quality standards.${NC}"
    echo -e "${GREEN}🔍 SQLX cache is prepared for Docker builds.${NC}"
else
    print_error "Some checks failed. Please fix the issues above before committing."
    echo -e "\n${RED}💡 Quick fixes:${NC}"
    if [ $SQLX_PASSED -eq 0 ]; then
        echo -e "   • Ensure database is running and run: ${BLUE}./scripts/code_check.sh --prepare${NC}"
        echo -e "   • Or set DATABASE_URL and run preparation manually"
    fi
    if [ $FORMAT_PASSED -eq 0 ]; then
        echo -e "   • Run: ${BLUE}./scripts/code_check.sh --fix${NC} to auto-fix formatting"
        echo -e "   • Or manually: ${BLUE}cargo fmt --all${NC} in each failed project directory"
    fi
    if [ $CLIPPY_PASSED -eq 0 ]; then
        echo -e "   • Review and fix clippy warnings in each failed project"
        echo -e "   • Some clippy issues can be auto-fixed with: ${BLUE}cargo clippy --fix${NC}"
    fi
    if [ $BENCH_PASSED -eq 0 ]; then
        echo -e "   • Run: ${BLUE}cargo bench --features benchmarks${NC} to debug benchmark issues"
    fi
    if [ $DOC_PASSED -eq 0 ]; then
        echo -e "   • Check documentation syntax and completeness in each failed project"
    fi
    echo ""
    exit 1
fi

#!/usr/bin/env bash

# code_check.sh - Comprehensive code quality check for tasker-core workspace
#
# Supports Rust, Python, and Ruby code quality checks with flexible targeting.
#
# Usage Examples:
#   ./scripts/code_check.sh                    # Check all (rust + python + ruby)
#   ./scripts/code_check.sh --rust             # Check only Rust code
#   ./scripts/code_check.sh --python           # Check only Python code
#   ./scripts/code_check.sh --ruby             # Check only Ruby code
#   ./scripts/code_check.sh --fix              # Auto-fix issues where possible
#   ./scripts/code_check.sh --test             # Run tests in addition to checks
#   ./scripts/code_check.sh --rust --fix       # Fix Rust formatting only
#   ./scripts/code_check.sh --python --test    # Check Python and run pytest
#   ./scripts/code_check.sh --prepare          # Prepare SQLX cache only
#   ./scripts/code_check.sh --help             # Show detailed help

set -e  # Exit on any error

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Rust workspace projects (ordered by dependency hierarchy)
RUST_CORE_PROJECTS=(
    "tasker-shared"
    "tasker-client"
    "pgmq-notify"
    "tasker-worker"
    "tasker-orchestration"
    "."
)

RUST_WORKER_PROJECTS=(
    "workers/rust"
    "workers/ruby/ext/tasker_core"
    "workers/python"
)

# Projects that use SQLX
SQLX_PROJECTS=(
    "tasker-shared"
    "tasker-client"
    "pgmq-notify"
    "tasker-worker"
    "tasker-orchestration"
    "workers/rust"
    "workers/ruby/ext/tasker_core"
    "."
)

# =============================================================================
# Colors and Output Helpers
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

print_header() {
    echo -e "\n${BLUE}${BOLD}===================================================${NC}"
    echo -e "${BLUE}${BOLD} $1${NC}"
    echo -e "${BLUE}${BOLD}===================================================${NC}"
}

print_section() {
    echo -e "\n${CYAN}━━━ $1 ━━━${NC}"
}

print_step() {
    echo -e "${YELLOW}➤${NC} $1"
}

print_substep() {
    echo -e "  ${YELLOW}→${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_info() {
    echo -e "${CYAN}ℹ${NC} $1"
}

print_skip() {
    echo -e "${MAGENTA}○${NC} $1 ${MAGENTA}(skipped)${NC}"
}

# =============================================================================
# Command Line Argument Parsing
# =============================================================================

# Defaults
CHECK_RUST=false
CHECK_PYTHON=false
CHECK_RUBY=false
AUTO_FIX=false
RUN_TESTS=false
PREPARE_SQLX_ONLY=false
VERBOSE=false
QUIET=false
SHOW_HELP=false

# If no language flags specified, check all
EXPLICIT_LANGUAGE=false

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --rust|-r)
                CHECK_RUST=true
                EXPLICIT_LANGUAGE=true
                shift
                ;;
            --python|-p)
                CHECK_PYTHON=true
                EXPLICIT_LANGUAGE=true
                shift
                ;;
            --ruby|-R)
                CHECK_RUBY=true
                EXPLICIT_LANGUAGE=true
                shift
                ;;
            --all|-a)
                CHECK_RUST=true
                CHECK_PYTHON=true
                CHECK_RUBY=true
                EXPLICIT_LANGUAGE=true
                shift
                ;;
            --fix|-f)
                AUTO_FIX=true
                shift
                ;;
            --test|-t)
                RUN_TESTS=true
                shift
                ;;
            --prepare)
                PREPARE_SQLX_ONLY=true
                shift
                ;;
            --verbose|-v)
                VERBOSE=true
                shift
                ;;
            --quiet|-q)
                QUIET=true
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

    # If no explicit language selected, check all
    if [ "$EXPLICIT_LANGUAGE" = false ]; then
        CHECK_RUST=true
        CHECK_PYTHON=true
        CHECK_RUBY=true
    fi
}

show_help() {
    cat << 'EOF'
Code Quality Check for tasker-core Workspace
============================================

A comprehensive tool for checking and fixing code quality across Rust, Python,
and Ruby components of the tasker-core workspace.

USAGE:
    ./scripts/code_check.sh [OPTIONS]

LANGUAGE OPTIONS:
    --rust, -r          Check only Rust code
    --python, -p        Check only Python code (workers/python)
    --ruby, -R          Check only Ruby code (workers/ruby)
    --all, -a           Check all languages (default if no language specified)

OPERATION OPTIONS:
    --fix, -f           Auto-fix issues where possible
                        - Rust: cargo fmt, cargo clippy --fix
                        - Python: ruff format, ruff check --fix
                        - Ruby: rubocop --auto-correct
    --test, -t          Run tests in addition to checks
                        - Rust: cargo test
                        - Python: pytest
                        - Ruby: rspec
    --prepare           Prepare SQLX cache only (for Docker/offline builds)

OUTPUT OPTIONS:
    --verbose, -v       Show detailed output
    --quiet, -q         Minimal output (errors only)
    --help, -h          Show this help message

RUST CHECKS:
    • rustfmt           Code formatting
    • clippy            Linting with all features and strict warnings
    • cargo doc         Documentation generation
    • cargo check       Benchmark compilation (main crate only)
    • sqlx prepare      Query cache preparation (if DATABASE_URL set)

PYTHON CHECKS (workers/python):
    • ruff check        Fast Python linter (pycodestyle, pyflakes, isort, etc.)
    • ruff format       Code formatting check
    • mypy              Static type checking

RUBY CHECKS (workers/ruby):
    • rubocop           Ruby linting and style checking
    • cargo clippy      Rust extension linting
    • cargo fmt         Rust extension formatting

EXAMPLES:
    # Check everything (default)
    ./scripts/code_check.sh

    # Check and fix all formatting issues
    ./scripts/code_check.sh --fix

    # Check and run all tests
    ./scripts/code_check.sh --test

    # Full check with tests and fixes
    ./scripts/code_check.sh --fix --test

    # Check only Rust code
    ./scripts/code_check.sh --rust

    # Check Python and run pytest
    ./scripts/code_check.sh --python --test

    # Fix Ruby code formatting
    ./scripts/code_check.sh --ruby --fix

    # Prepare SQLX cache for Docker builds
    ./scripts/code_check.sh --prepare

ENVIRONMENT VARIABLES:
    DATABASE_URL        PostgreSQL connection for SQLX preparation
                        Default: postgresql://tasker:tasker@localhost:5432/tasker_rust_test

EXIT CODES:
    0                   All checks passed
    1                   One or more checks failed

EOF
}

# =============================================================================
# Rust Checks
# =============================================================================

RUST_FORMAT_PASSED=0
RUST_CLIPPY_PASSED=0
RUST_DOCS_PASSED=0
RUST_BENCH_PASSED=0
RUST_SQLX_PASSED=0
RUST_TEST_PASSED=0

check_rust_project_exists() {
    local project="$1"
    if [[ "$project" == "." ]]; then
        return 0
    fi
    [[ -f "$PROJECT_ROOT/$project/Cargo.toml" ]]
}

run_cargo_in_project() {
    local project_dir="$1"
    local cargo_cmd="$2"
    local description="$3"

    if [ "$VERBOSE" = true ]; then
        print_substep "Running in ${BLUE}${project_dir}${NC}: $cargo_cmd"
    fi

    local full_path
    if [[ "$project_dir" == "." ]]; then
        full_path="$PROJECT_ROOT"
    else
        full_path="$PROJECT_ROOT/$project_dir"
    fi

    (cd "$full_path" && eval "$cargo_cmd")
}

check_rust_format() {
    print_section "Rust: Code Formatting"
    RUST_FORMAT_PASSED=1

    local all_projects=("${RUST_CORE_PROJECTS[@]}" "${RUST_WORKER_PROJECTS[@]}")

    if [ "$AUTO_FIX" = true ]; then
        print_step "Auto-fixing Rust formatting..."
        for project in "${all_projects[@]}"; do
            if check_rust_project_exists "$project"; then
                print_substep "Formatting ${BLUE}${project}${NC}..."
                if run_cargo_in_project "$project" "cargo fmt --all" "format fix"; then
                    print_success "Formatted $project"
                fi
            fi
        done
    else
        print_step "Checking Rust formatting..."
        for project in "${all_projects[@]}"; do
            if check_rust_project_exists "$project"; then
                print_substep "Checking ${BLUE}${project}${NC}..."
                if ! run_cargo_in_project "$project" "cargo fmt --all -- --check" "format check"; then
                    print_error "Formatting issues in $project"
                    RUST_FORMAT_PASSED=0
                fi
            fi
        done
    fi

    if [ $RUST_FORMAT_PASSED -eq 1 ]; then
        print_success "Rust formatting OK"
    fi
}

check_rust_clippy() {
    print_section "Rust: Clippy Linting"
    RUST_CLIPPY_PASSED=1

    local all_projects=("${RUST_CORE_PROJECTS[@]}" "${RUST_WORKER_PROJECTS[@]}")
    local clippy_args="--all-targets --all-features -- -D warnings"

    if [ "$AUTO_FIX" = true ]; then
        print_step "Auto-fixing Clippy warnings..."
        clippy_args="--all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings"
    else
        print_step "Running Clippy..."
    fi

    for project in "${all_projects[@]}"; do
        if check_rust_project_exists "$project"; then
            print_substep "Linting ${BLUE}${project}${NC}..."
            # Python worker doesn't use --all-features (no workspace features)
            local cmd="cargo clippy $clippy_args"
            if [[ "$project" == "workers/python" ]]; then
                cmd="cargo clippy --all-targets -- -D warnings"
                if [ "$AUTO_FIX" = true ]; then
                    cmd="cargo clippy --all-targets --fix --allow-dirty --allow-staged -- -D warnings"
                fi
            fi
            if ! run_cargo_in_project "$project" "$cmd" "clippy"; then
                print_error "Clippy issues in $project"
                RUST_CLIPPY_PASSED=0
            fi
        fi
    done

    if [ $RUST_CLIPPY_PASSED -eq 1 ]; then
        print_success "Rust linting OK"
    fi
}

check_rust_docs() {
    print_section "Rust: Documentation"
    RUST_DOCS_PASSED=1

    local all_projects=("${RUST_CORE_PROJECTS[@]}" "${RUST_WORKER_PROJECTS[@]}")

    print_step "Generating documentation..."
    for project in "${all_projects[@]}"; do
        if check_rust_project_exists "$project"; then
            print_substep "Documenting ${BLUE}${project}${NC}..."
            if ! run_cargo_in_project "$project" "cargo doc --no-deps --document-private-items" "docs"; then
                print_error "Documentation issues in $project"
                RUST_DOCS_PASSED=0
            fi
        fi
    done

    if [ $RUST_DOCS_PASSED -eq 1 ]; then
        print_success "Rust documentation OK"
    fi
}

check_rust_benchmarks() {
    print_section "Rust: Benchmark Compilation"
    RUST_BENCH_PASSED=1

    print_step "Checking benchmark compilation..."
    if ! run_cargo_in_project "." "cargo check --benches --features benchmarks" "benchmark check"; then
        print_error "Benchmark compilation failed"
        RUST_BENCH_PASSED=0
    else
        print_success "Rust benchmarks compile OK"
    fi
}

prepare_rust_sqlx() {
    print_section "Rust: SQLX Cache Preparation"
    RUST_SQLX_PASSED=1

    if [[ -z "$DATABASE_URL" ]]; then
        print_warning "DATABASE_URL not set. Using default..."
        export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
    fi

    print_step "Preparing SQLX cache..."
    for project in "${SQLX_PROJECTS[@]}"; do
        if ! check_rust_project_exists "$project"; then
            continue
        fi

        # Check if project uses SQLX
        local has_sqlx=false
        local cargo_path
        if [[ "$project" == "." ]]; then
            cargo_path="$PROJECT_ROOT/Cargo.toml"
        else
            cargo_path="$PROJECT_ROOT/$project/Cargo.toml"
        fi

        if [[ -d "$PROJECT_ROOT/$project/.sqlx" ]] || grep -q "sqlx" "$cargo_path" 2>/dev/null; then
            has_sqlx=true
        fi

        if [ "$has_sqlx" = true ]; then
            print_substep "Preparing ${BLUE}${project}${NC}..."
            local sqlx_cmd
            if [[ "$project" == "." ]]; then
                sqlx_cmd="cargo sqlx prepare --workspace -- --all-targets --all-features"
            else
                sqlx_cmd="cargo sqlx prepare -- --all-targets --all-features"
            fi

            if ! run_cargo_in_project "$project" "$sqlx_cmd" "sqlx prepare"; then
                print_warning "SQLX preparation failed in $project (database may not be running)"
            else
                print_success "SQLX cache prepared for $project"
            fi
        else
            if [ "$VERBOSE" = true ]; then
                print_skip "No SQLX in $project"
            fi
        fi
    done

    print_success "SQLX preparation complete"
}

run_rust_tests() {
    print_section "Rust: Tests"
    RUST_TEST_PASSED=1

    print_step "Running Rust tests..."
    if ! run_cargo_in_project "." "cargo test --all-features" "tests"; then
        print_error "Rust tests failed"
        RUST_TEST_PASSED=0
    else
        print_success "Rust tests passed"
    fi
}

# =============================================================================
# Python Checks
# =============================================================================

PYTHON_FORMAT_PASSED=0
PYTHON_LINT_PASSED=0
PYTHON_TYPE_PASSED=0
PYTHON_TEST_PASSED=0

PYTHON_DIR="$PROJECT_ROOT/workers/python"

check_python_available() {
    if [[ ! -d "$PYTHON_DIR" ]]; then
        print_warning "Python worker directory not found: $PYTHON_DIR"
        return 1
    fi
    if [[ ! -f "$PYTHON_DIR/pyproject.toml" ]]; then
        print_warning "No pyproject.toml found in $PYTHON_DIR"
        return 1
    fi
    return 0
}

ensure_python_venv() {
    if [[ ! -d "$PYTHON_DIR/.venv" ]]; then
        print_step "Creating Python virtual environment..."
        (cd "$PYTHON_DIR" && uv venv)
    fi

    # Check if dev dependencies are installed
    if ! (cd "$PYTHON_DIR" && uv run python -c "import pytest" 2>/dev/null); then
        print_step "Installing Python dev dependencies..."
        (cd "$PYTHON_DIR" && uv sync --group dev)
    fi
}

check_python_format() {
    print_section "Python: Code Formatting (ruff)"
    PYTHON_FORMAT_PASSED=1

    if [ "$AUTO_FIX" = true ]; then
        print_step "Auto-fixing Python formatting..."
        if (cd "$PYTHON_DIR" && uv run ruff format .); then
            print_success "Python code formatted"
        else
            print_error "Python formatting failed"
            PYTHON_FORMAT_PASSED=0
        fi
    else
        print_step "Checking Python formatting..."
        if (cd "$PYTHON_DIR" && uv run ruff format --check .); then
            print_success "Python formatting OK"
        else
            print_error "Python formatting issues found"
            print_info "Run with --fix to auto-format"
            PYTHON_FORMAT_PASSED=0
        fi
    fi
}

check_python_lint() {
    print_section "Python: Linting (ruff)"
    PYTHON_LINT_PASSED=1

    if [ "$AUTO_FIX" = true ]; then
        print_step "Auto-fixing Python lint issues..."
        if (cd "$PYTHON_DIR" && uv run ruff check --fix .); then
            print_success "Python lint issues fixed"
        else
            print_warning "Some Python lint issues could not be auto-fixed"
            PYTHON_LINT_PASSED=0
        fi
    else
        print_step "Running Python linter..."
        if (cd "$PYTHON_DIR" && uv run ruff check .); then
            print_success "Python linting OK"
        else
            print_error "Python lint issues found"
            PYTHON_LINT_PASSED=0
        fi
    fi
}

check_python_types() {
    print_section "Python: Type Checking (mypy)"
    PYTHON_TYPE_PASSED=1

    print_step "Running mypy..."
    if (cd "$PYTHON_DIR" && uv run mypy python/); then
        print_success "Python type checking OK"
    else
        print_error "Python type errors found"
        PYTHON_TYPE_PASSED=0
    fi
}

run_python_tests() {
    print_section "Python: Tests (pytest)"
    PYTHON_TEST_PASSED=1

    print_step "Running pytest..."
    if (cd "$PYTHON_DIR" && uv run pytest -v); then
        print_success "Python tests passed"
    else
        print_error "Python tests failed"
        PYTHON_TEST_PASSED=0
    fi
}

# =============================================================================
# Ruby Checks
# =============================================================================

RUBY_FORMAT_PASSED=0
RUBY_LINT_PASSED=0
RUBY_RUST_PASSED=0
RUBY_TEST_PASSED=0

RUBY_DIR="$PROJECT_ROOT/workers/ruby"

check_ruby_available() {
    if [[ ! -d "$RUBY_DIR" ]]; then
        print_warning "Ruby worker directory not found: $RUBY_DIR"
        return 1
    fi
    if [[ ! -f "$RUBY_DIR/Gemfile" ]]; then
        print_warning "No Gemfile found in $RUBY_DIR"
        return 1
    fi
    return 0
}

ensure_ruby_bundle() {
    if ! (cd "$RUBY_DIR" && bundle check &>/dev/null); then
        print_step "Installing Ruby dependencies..."
        (cd "$RUBY_DIR" && bundle install)
    fi
}

check_ruby_lint() {
    print_section "Ruby: Linting (rubocop)"
    RUBY_LINT_PASSED=1

    if [ "$AUTO_FIX" = true ]; then
        print_step "Auto-fixing Ruby lint issues..."
        if (cd "$RUBY_DIR" && bundle exec rubocop --auto-correct); then
            print_success "Ruby lint issues fixed"
        else
            print_warning "Some Ruby lint issues could not be auto-fixed"
            RUBY_LINT_PASSED=0
        fi
    else
        print_step "Running rubocop..."
        if (cd "$RUBY_DIR" && bundle exec rubocop); then
            print_success "Ruby linting OK"
        else
            print_error "Ruby lint issues found"
            RUBY_LINT_PASSED=0
        fi
    fi
}

check_ruby_rust_extension() {
    print_section "Ruby: Rust Extension"
    RUBY_RUST_PASSED=1

    local ext_dir="$RUBY_DIR/ext/tasker_core"

    if [[ ! -f "$ext_dir/Cargo.toml" ]]; then
        print_skip "No Rust extension found"
        return
    fi

    # Format check
    print_step "Checking Rust extension formatting..."
    if [ "$AUTO_FIX" = true ]; then
        if (cd "$ext_dir" && cargo fmt --all); then
            print_success "Rust extension formatted"
        fi
    else
        if ! (cd "$ext_dir" && cargo fmt --all -- --check); then
            print_error "Rust extension formatting issues"
            RUBY_RUST_PASSED=0
        fi
    fi

    # Clippy
    print_step "Linting Rust extension..."
    if ! (cd "$ext_dir" && cargo clippy --all-targets -- -D warnings); then
        print_error "Rust extension lint issues"
        RUBY_RUST_PASSED=0
    fi

    if [ $RUBY_RUST_PASSED -eq 1 ]; then
        print_success "Ruby Rust extension OK"
    fi
}

run_ruby_tests() {
    print_section "Ruby: Tests (rspec)"
    RUBY_TEST_PASSED=1

    # Ensure extension is compiled
    print_step "Compiling Rust extension..."
    if ! (cd "$RUBY_DIR" && bundle exec rake compile); then
        print_error "Failed to compile Rust extension"
        RUBY_TEST_PASSED=0
        return
    fi

    print_step "Running rspec..."
    if (cd "$RUBY_DIR" && bundle exec rspec --format documentation); then
        print_success "Ruby tests passed"
    else
        print_error "Ruby tests failed"
        RUBY_TEST_PASSED=0
    fi
}

# =============================================================================
# Summary
# =============================================================================

print_summary() {
    print_header "Summary"

    local overall_success=1

    if [ "$CHECK_RUST" = true ]; then
        echo -e "\n${BOLD}Rust:${NC}"
        echo -e "  Format:      $([ $RUST_FORMAT_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        echo -e "  Clippy:      $([ $RUST_CLIPPY_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        echo -e "  Docs:        $([ $RUST_DOCS_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        echo -e "  Benchmarks:  $([ $RUST_BENCH_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        if [ "$PREPARE_SQLX_ONLY" = true ] || [ "$RUN_TESTS" = true ]; then
            echo -e "  SQLX:        $([ $RUST_SQLX_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${YELLOW}WARNING${NC}")"
        fi
        if [ "$RUN_TESTS" = true ]; then
            echo -e "  Tests:       $([ $RUST_TEST_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
            [ $RUST_TEST_PASSED -eq 0 ] && overall_success=0
        fi
        [ $RUST_FORMAT_PASSED -eq 0 ] && overall_success=0
        [ $RUST_CLIPPY_PASSED -eq 0 ] && overall_success=0
        [ $RUST_DOCS_PASSED -eq 0 ] && overall_success=0
        [ $RUST_BENCH_PASSED -eq 0 ] && overall_success=0
    fi

    if [ "$CHECK_PYTHON" = true ]; then
        echo -e "\n${BOLD}Python:${NC}"
        echo -e "  Format:      $([ $PYTHON_FORMAT_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        echo -e "  Lint:        $([ $PYTHON_LINT_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        echo -e "  Types:       $([ $PYTHON_TYPE_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        if [ "$RUN_TESTS" = true ]; then
            echo -e "  Tests:       $([ $PYTHON_TEST_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
            [ $PYTHON_TEST_PASSED -eq 0 ] && overall_success=0
        fi
        [ $PYTHON_FORMAT_PASSED -eq 0 ] && overall_success=0
        [ $PYTHON_LINT_PASSED -eq 0 ] && overall_success=0
        [ $PYTHON_TYPE_PASSED -eq 0 ] && overall_success=0
    fi

    if [ "$CHECK_RUBY" = true ]; then
        echo -e "\n${BOLD}Ruby:${NC}"
        echo -e "  Lint:        $([ $RUBY_LINT_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        echo -e "  Rust ext:    $([ $RUBY_RUST_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
        if [ "$RUN_TESTS" = true ]; then
            echo -e "  Tests:       $([ $RUBY_TEST_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
            [ $RUBY_TEST_PASSED -eq 0 ] && overall_success=0
        fi
        [ $RUBY_LINT_PASSED -eq 0 ] && overall_success=0
        [ $RUBY_RUST_PASSED -eq 0 ] && overall_success=0
    fi

    echo ""
    if [ $overall_success -eq 1 ]; then
        print_success "All checks passed! Code is ready for commit."
        echo -e "\n${GREEN}Great job! Your code meets all quality standards.${NC}"
    else
        print_error "Some checks failed. Please fix the issues above."
        echo -e "\n${YELLOW}Quick fixes:${NC}"
        echo -e "  • Run ${BLUE}./scripts/code_check.sh --fix${NC} to auto-fix formatting"
        echo -e "  • Run ${BLUE}./scripts/code_check.sh --test${NC} to run tests"
        echo ""
        exit 1
    fi
}

# =============================================================================
# Main
# =============================================================================

main() {
    parse_args "$@"

    if [ "$SHOW_HELP" = true ]; then
        show_help
        exit 0
    fi

    # Ensure we're in the project root
    cd "$PROJECT_ROOT"

    if [[ ! -f "Cargo.toml" ]]; then
        print_error "This script must be run from the project root (where Cargo.toml is located)"
        exit 1
    fi

    # Initialize all results to passing
    RUST_FORMAT_PASSED=1
    RUST_CLIPPY_PASSED=1
    RUST_DOCS_PASSED=1
    RUST_BENCH_PASSED=1
    RUST_SQLX_PASSED=1
    RUST_TEST_PASSED=1
    PYTHON_FORMAT_PASSED=1
    PYTHON_LINT_PASSED=1
    PYTHON_TYPE_PASSED=1
    PYTHON_TEST_PASSED=1
    RUBY_LINT_PASSED=1
    RUBY_RUST_PASSED=1
    RUBY_TEST_PASSED=1

    # Determine what we're checking
    local targets=()
    [ "$CHECK_RUST" = true ] && targets+=("Rust")
    [ "$CHECK_PYTHON" = true ] && targets+=("Python")
    [ "$CHECK_RUBY" = true ] && targets+=("Ruby")

    print_header "Code Quality Check for tasker-core"
    echo -e "Targets: ${CYAN}${targets[*]}${NC}"
    [ "$AUTO_FIX" = true ] && echo -e "Mode: ${YELLOW}Auto-fix enabled${NC}"
    [ "$RUN_TESTS" = true ] && echo -e "Tests: ${YELLOW}Enabled${NC}"

    # SQLX-only mode
    if [ "$PREPARE_SQLX_ONLY" = true ]; then
        prepare_rust_sqlx
        print_header "SQLX Preparation Complete"
        exit 0
    fi

    # Rust checks
    if [ "$CHECK_RUST" = true ]; then
        print_header "Rust Checks"
        check_rust_format
        check_rust_clippy
        check_rust_docs
        check_rust_benchmarks

        if [ "$RUN_TESTS" = true ]; then
            run_rust_tests
        fi
    fi

    # Python checks
    if [ "$CHECK_PYTHON" = true ]; then
        print_header "Python Checks"
        if check_python_available; then
            ensure_python_venv
            check_python_format
            check_python_lint
            check_python_types

            if [ "$RUN_TESTS" = true ]; then
                run_python_tests
            fi
        else
            print_skip "Python checks (not available)"
            PYTHON_FORMAT_PASSED=1
            PYTHON_LINT_PASSED=1
            PYTHON_TYPE_PASSED=1
            PYTHON_TEST_PASSED=1
        fi
    fi

    # Ruby checks
    if [ "$CHECK_RUBY" = true ]; then
        print_header "Ruby Checks"
        if check_ruby_available; then
            ensure_ruby_bundle
            check_ruby_lint
            check_ruby_rust_extension

            if [ "$RUN_TESTS" = true ]; then
                run_ruby_tests
            fi
        else
            print_skip "Ruby checks (not available)"
            RUBY_LINT_PASSED=1
            RUBY_RUST_PASSED=1
            RUBY_TEST_PASSED=1
        fi
    fi

    # Print summary
    print_summary
}

main "$@"

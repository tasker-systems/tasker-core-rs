#!/usr/bin/env bash

# code_check.sh - Comprehensive code quality check for tasker-core
# This script runs formatting, linting, and documentation checks with detailed feedback
#
# Usage:
#   ./scripts/code_check.sh          # Check only (default)
#   ./scripts/code_check.sh --fix    # Auto-fix formatting issues
#   ./scripts/code_check.sh --help   # Show help

set -e  # Exit on any error

# Parse command line arguments
AUTO_FIX=false
SHOW_HELP=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --fix)
            AUTO_FIX=true
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
    echo "Code Quality Check for tasker-core"
    echo ""
    echo "Usage:"
    echo "  $0              Check code quality (format, lint, docs)"
    echo "  $0 --fix        Check and auto-fix formatting issues"
    echo "  $0 --help       Show this help message"
    echo ""
    echo "This script checks both the main Rust project and the Ruby extension."
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

# Define Rust project paths in our monorepo
MAIN_RUST_PROJECT="."
RUBY_EXT_RUST_PROJECT="workers/ruby/ext/tasker_core"
RUST_PROJECTS=("$MAIN_RUST_PROJECT")

# Check if Ruby extension exists and add it to projects list
if [[ -f "$RUBY_EXT_RUST_PROJECT/Cargo.toml" ]]; then
    RUST_PROJECTS+=("$RUBY_EXT_RUST_PROJECT")
fi

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
FORMAT_PASSED=0
CLIPPY_PASSED=0
BENCH_PASSED=0
DOC_PASSED=0
OVERALL_SUCCESS=1

print_header "Code Quality Check for tasker-core Monorepo"
echo -e "This script will check code formatting, run linter, check benchmark compilation, and generate documentation."
echo -e "Checking ${#RUST_PROJECTS[@]} Rust project(s): ${RUST_PROJECTS[*]}\n"

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
    # Use different clippy args for Ruby extension (no benchmarks)
    if [[ "$project" == "$RUBY_EXT_RUST_PROJECT" ]]; then
        clippy_cmd="cargo clippy --all-targets -- -D warnings"
    else
        clippy_cmd="cargo clippy --all-targets --all-features -- -D warnings"
    fi

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
    # Only check benchmarks for main project (Ruby extension doesn't have benchmarks)
    if [[ "$project" == "$MAIN_RUST_PROJECT" ]]; then
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
echo -e "Format Check:      $([ $FORMAT_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
echo -e "Clippy Linting:    $([ $CLIPPY_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
echo -e "Benchmarks:        $([ $BENCH_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"
echo -e "Documentation:     $([ $DOC_PASSED -eq 1 ] && echo -e "${GREEN}PASSED${NC}" || echo -e "${RED}FAILED${NC}")"

if [ $OVERALL_SUCCESS -eq 1 ]; then
    print_success "All checks passed! Code is ready for commit."
    echo -e "\n${GREEN}🎉 Great job! Your code meets all quality standards.${NC}"
else
    print_error "Some checks failed. Please fix the issues above before committing."
    echo -e "\n${RED}💡 Quick fixes:${NC}"
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

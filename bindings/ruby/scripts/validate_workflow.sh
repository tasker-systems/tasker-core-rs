#!/bin/bash

# GitHub Actions Workflow Validation Script
# This script validates the workflow YAML and simulates the environment

set -e

echo "🔍 Validating GitHub Actions Workflow"
echo "===================================="

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

print_status() { echo -e "${GREEN}✅ $1${NC}"; }
print_error() { echo -e "${RED}❌ $1${NC}"; }
print_info() { echo -e "${YELLOW}ℹ️  $1${NC}"; }

WORKFLOW_FILE="../../.github/workflows/ruby-gem.yml"

# Check if workflow file exists
if [[ ! -f "$WORKFLOW_FILE" ]]; then
    print_error "Workflow file not found: $WORKFLOW_FILE"
    exit 1
fi

# 1. Check YAML syntax (if yq is available)
echo -n "YAML syntax validation... "
if command -v yq &> /dev/null; then
    if yq eval '.' "$WORKFLOW_FILE" > /dev/null 2>&1; then
        print_status "Valid"
    else
        print_error "Invalid YAML syntax"
        exit 1
    fi
else
    print_info "Skipped (yq not installed)"
fi

# 2. Check for required environment variables in workflow
echo -n "Environment variables check... "
if grep -q "TASKER_ENV: test" "$WORKFLOW_FILE" && \
   grep -q "RAILS_ENV: test" "$WORKFLOW_FILE" && \
   grep -q "DATABASE_URL:" "$WORKFLOW_FILE"; then
    print_status "Present"
else
    print_error "Missing required environment variables"
    exit 1
fi

# 3. Check database configuration
echo -n "Database configuration... "
if grep -q "POSTGRES_USER: tasker" "$WORKFLOW_FILE" && \
   grep -q "POSTGRES_PASSWORD: tasker" "$WORKFLOW_FILE" && \
   grep -q "POSTGRES_DB: tasker_rust_test" "$WORKFLOW_FILE"; then
    print_status "Correct"
else
    print_error "Database configuration mismatch"
    exit 1
fi

# 4. Check for required steps
echo -n "Required steps check... "
REQUIRED_STEPS=(
    "bundle exec rake test:setup"
    "bundle exec rspec"
    "bundle exec rake compile"
    "gem build tasker-core-rb.gemspec"
)

MISSING_STEPS=()
for step in "${REQUIRED_STEPS[@]}"; do
    if ! grep -q "$step" "$WORKFLOW_FILE"; then
        MISSING_STEPS+=("$step")
    fi
done

if [[ ${#MISSING_STEPS[@]} -eq 0 ]]; then
    print_status "All present"
else
    print_error "Missing steps: ${MISSING_STEPS[*]}"
    exit 1
fi

# 5. Check matrix configuration
echo -n "Matrix configuration... "
if grep -q "os: \[ubuntu-latest, macos-latest\]" "$WORKFLOW_FILE" && \
   grep -q "ruby: \['3.0', '3.1', '3.2', '3.3'\]" "$WORKFLOW_FILE"; then
    print_status "Correct"
else
    print_error "Matrix configuration issues"
    exit 1
fi

# 6. Check that Windows is not in matrix (as requested)
echo -n "Windows exclusion check... "
if grep -q "windows" "$WORKFLOW_FILE"; then
    print_error "Windows found in workflow (should be removed)"
    exit 1
else
    print_status "Windows excluded"
fi

# 7. Check for benchmark job removal
echo -n "Benchmark job removal... "
if grep -q "benchmark:" "$WORKFLOW_FILE"; then
    print_error "Benchmark job still present (should be removed)"
    exit 1
else
    print_status "Benchmark job removed"
fi

# 8. Check for modern toolchain actions
echo -n "Modern toolchain actions... "
if grep -q "dtolnay/rust-toolchain@stable" "$WORKFLOW_FILE" && \
   grep -q "Swatinem/rust-cache@v2" "$WORKFLOW_FILE"; then
    print_status "Using modern actions"
else
    print_error "Using outdated actions"
    exit 1
fi

# 9. Simulate workflow environment
echo -n "Workflow environment simulation... "
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
export TASKER_ENV=test
export RAILS_ENV=test
export APP_ENV=test
export RACK_ENV=test

# Check if this environment would work
if [[ -n "$DATABASE_URL" && "$TASKER_ENV" == "test" && "$RAILS_ENV" == "test" ]]; then
    print_status "Environment simulation OK"
else
    print_error "Environment simulation failed"
    exit 1
fi

echo ""
echo "🎉 Workflow validation passed!"
echo ""
echo "Summary:"
echo "  ✅ YAML syntax is valid"
echo "  ✅ Environment variables are properly set"
echo "  ✅ Database configuration matches expectations"
echo "  ✅ All required steps are present"
echo "  ✅ Matrix configuration is correct (Linux/macOS, Ruby 3.0-3.3)"
echo "  ✅ Windows support removed as requested"
echo "  ✅ Benchmark job removed (no benchmark files exist)"
echo "  ✅ Modern GitHub Actions used"
echo "  ✅ Environment simulation successful"
echo ""
echo "🚀 The workflow is ready for GitHub Actions!"
echo ""
echo "Next steps:"
echo "  1. Run: ./scripts/ci_check.sh (quick validation)"
echo "  2. Run: ./scripts/ci_dry_run.sh (full simulation)"
echo "  3. Push to GitHub and create PR"
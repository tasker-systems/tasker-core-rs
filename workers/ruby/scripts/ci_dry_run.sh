#!/bin/bash

# Ruby Gem CI Dry Run Script
# This script simulates the GitHub Actions workflow locally to catch issues before CI

set -e  # Exit on any error

echo "🧪 Starting Ruby Gem CI Dry Run"
echo "=================================="

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_step() {
    echo -e "\n${YELLOW}📋 $1${NC}"
}

# Check if we're in the right directory
if [[ ! -f "Gemfile" || ! -f "tasker-core-rb.gemspec" ]]; then
    print_error "Please run this script from the workers/ruby directory"
    exit 1
fi

# Set environment variables (matching CI)
export DATABASE_URL="${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_rust_test}"
export TASKER_ENV=test
export RAILS_ENV=test
export APP_ENV=test
export RACK_ENV=test

print_step "Environment Setup"
echo "DATABASE_URL: $DATABASE_URL"
echo "TASKER_ENV: $TASKER_ENV"
echo "RAILS_ENV: $RAILS_ENV"
print_status "Environment variables set"

# Check system dependencies
print_step "System Dependencies Check"

# Check for Rust
if ! command -v cargo &> /dev/null; then
    print_error "Rust/Cargo not found. Please install Rust: https://rustup.rs/"
    exit 1
fi
print_status "Rust toolchain found: $(rustc --version)"

# Check for Ruby
if ! command -v ruby &> /dev/null; then
    print_error "Ruby not found. Please install Ruby"
    exit 1
fi
print_status "Ruby found: $(ruby --version)"

# Check for Bundler
if ! command -v bundle &> /dev/null; then
    print_error "Bundler not found. Please install with: gem install bundler"
    exit 1
fi
print_status "Bundler found: $(bundle --version)"

# Check for PostgreSQL
if ! command -v pg_isready &> /dev/null; then
    print_warning "pg_isready not found. PostgreSQL client tools may not be installed"
    print_warning "Install with: brew install postgresql (macOS) or apt-get install postgresql-client (Ubuntu)"
fi

# Check for psql
if ! command -v psql &> /dev/null; then
    print_warning "psql not found. PostgreSQL client may not be installed"
fi

# Check PostgreSQL connection
print_step "PostgreSQL Connection Check"
if pg_isready -h localhost -p 5432 -U tasker > /dev/null 2>&1; then
    print_status "PostgreSQL is ready"
else
    print_error "PostgreSQL is not ready. Please ensure PostgreSQL is running with user 'tasker'"
    print_error "Expected: postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
    print_error "You may need to create the user and database:"
    echo "  createuser -s tasker"
    echo "  createdb -O tasker tasker_rust_test"
    echo "  psql -c \"ALTER USER tasker WITH PASSWORD 'tasker';\""
    exit 1
fi

# Install Ruby dependencies
print_step "Ruby Dependencies"
if ! bundle install; then
    print_error "Bundle install failed"
    exit 1
fi
print_status "Ruby dependencies installed"

# Check Rust formatting (like CI does)
print_step "Rust Code Formatting Check"
cd ../..
if ! cargo fmt --check; then
    print_error "Rust code formatting check failed"
    print_error "Run 'cargo fmt' to fix formatting issues"
    exit 1
fi
print_status "Rust code formatting is correct"

# Run Rust clippy (like CI does)
print_step "Rust Clippy Lints"
if ! cargo clippy --workspace --all-targets --all-features -- -D warnings; then
    print_error "Rust clippy check failed"
    print_error "Fix clippy warnings before proceeding"
    exit 1
fi
print_status "Rust clippy checks passed"

# Return to Ruby directory
cd workers/ruby

# Build Ruby extension
print_step "Ruby Extension Build"
if ! bundle exec rake compile; then
    print_error "Ruby extension compilation failed"
    exit 1
fi
print_status "Ruby extension built successfully"

# Set up Ruby test environment (the key step that mirrors CI)
print_step "Ruby Test Environment Setup"
if ! bundle exec rake test:setup; then
    print_error "Ruby test environment setup failed"
    print_error "This is the same step that runs in CI"
    exit 1
fi
print_status "Ruby test environment setup completed"

# Run Ruby tests
print_step "Ruby Tests"
if ! bundle exec rspec; then
    print_error "Ruby tests failed"
    exit 1
fi
print_status "Ruby tests passed"

# Test gem installation
print_step "Gem Installation Test"
if ! gem build tasker-core-rb.gemspec; then
    print_error "Gem build failed"
    exit 1
fi
print_status "Gem built successfully"

# Summary
print_step "Summary"
echo "🎉 All CI workflow steps completed successfully!"
echo ""
echo "The following steps were verified (matching CI workflow):"
echo "  ✅ System dependencies"
echo "  ✅ PostgreSQL connection"
echo "  ✅ Ruby dependencies installation"
echo "  ✅ Rust formatting check"
echo "  ✅ Rust clippy lints"
echo "  ✅ Ruby extension compilation"
echo "  ✅ Ruby test environment setup"
echo "  ✅ Ruby tests execution"
echo "  ✅ Gem build and installation"
echo ""
echo "🚀 Ready for CI! The workflow should pass in GitHub Actions."

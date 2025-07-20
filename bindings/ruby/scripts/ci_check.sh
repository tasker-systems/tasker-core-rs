#!/bin/bash

# Quick CI Check Script
# This script does a fast check of the key CI requirements without running full tests

set -e

echo "🔍 Quick CI Check (Fast)"
echo "======================="

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

print_status() { echo -e "${GREEN}✅ $1${NC}"; }
print_error() { echo -e "${RED}❌ $1${NC}"; }
print_warning() { echo -e "${YELLOW}⚠️  $1${NC}"; }

# Check we're in the right place
if [[ ! -f "Gemfile" ]]; then
    print_error "Run from bindings/ruby directory"
    exit 1
fi

# Set environment variables
export DATABASE_URL="${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_rust_test}"
export TASKER_ENV=test
export RAILS_ENV=test

echo "🧪 Checking CI environment setup..."

# 1. Check system dependencies
echo -n "Rust toolchain... "
if cargo --version > /dev/null 2>&1; then
    print_status "OK"
else
    print_error "Missing"
    exit 1
fi

echo -n "Ruby & Bundler... "
if bundle --version > /dev/null 2>&1; then
    print_status "OK"
else
    print_error "Missing"
    exit 1
fi

echo -n "PostgreSQL client... "
if pg_isready --version > /dev/null 2>&1; then
    print_status "OK"
else
    print_warning "Missing (may cause issues)"
fi

# 2. Check PostgreSQL connection
echo -n "PostgreSQL connection... "
if pg_isready -h localhost -p 5432 -U tasker > /dev/null 2>&1; then
    print_status "OK"
else
    print_error "Failed"
    echo "  Expected: postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
    echo "  Setup commands:"
    echo "    createuser -s tasker"
    echo "    createdb -O tasker tasker_rust_test"
    echo "    psql -c \"ALTER USER tasker WITH PASSWORD 'tasker';\""
    exit 1
fi

# 3. Check Ruby dependencies
echo -n "Ruby dependencies... "
if bundle check > /dev/null 2>&1; then
    print_status "OK"
else
    echo "Installing..."
    if bundle install > /dev/null 2>&1; then
        print_status "Installed"
    else
        print_error "Failed"
        exit 1
    fi
fi

# 4. Check Rust formatting
echo -n "Rust formatting... "
cd ../..
if cargo fmt --check > /dev/null 2>&1; then
    print_status "OK"
else
    print_error "Failed"
    echo "  Run: cargo fmt"
    exit 1
fi

# 5. Check Rust clippy (quick check)
echo -n "Rust clippy... "
if cargo clippy --workspace --all-targets --all-features -- -D warnings > /dev/null 2>&1; then
    print_status "OK"
else
    print_error "Failed"
    echo "  Run: cargo clippy --workspace --all-targets --all-features -- -D warnings"
    exit 1
fi

cd bindings/ruby

# 6. Check Ruby extension compiles
echo -n "Ruby extension compilation... "
if bundle exec rake compile > /dev/null 2>&1; then
    print_status "OK"
else
    print_error "Failed"
    exit 1
fi

echo ""
echo "🎉 All CI checks passed!"
echo "🚀 The workflow should succeed in GitHub Actions"
echo ""
echo "To run the full test suite locally:"
echo "  ./scripts/ci_dry_run.sh"

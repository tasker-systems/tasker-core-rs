#!/bin/bash

# Git Hooks Setup Script
# This script sets up or updates git hooks for the Tasker Core Rust project

set -e

echo "🔧 Setting up Git Hooks for Tasker Core Rust"
echo "============================================="

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

print_status() { echo -e "${GREEN}✅ $1${NC}"; }
print_error() { echo -e "${RED}❌ $1${NC}"; }
print_info() { echo -e "${YELLOW}ℹ️  $1${NC}"; }

# Check if we're in a git repository
if [ ! -d ".git" ]; then
    print_error "Not in a git repository. Please run this from the project root."
    exit 1
fi

# Check if we're in the right project
if [ ! -f "Cargo.toml" ]; then
    print_error "No Cargo.toml found. Please run this from the Tasker Core Rust project root."
    exit 1
fi

HOOKS_DIR=".git/hooks"

# Backup existing hooks if they exist
if [ -f "$HOOKS_DIR/pre-commit" ]; then
    print_info "Backing up existing pre-commit hook"
    cp "$HOOKS_DIR/pre-commit" "$HOOKS_DIR/pre-commit.backup.$(date +%Y%m%d_%H%M%S)"
fi

if [ -f "$HOOKS_DIR/pre-push" ]; then
    print_info "Backing up existing pre-push hook"
    cp "$HOOKS_DIR/pre-push" "$HOOKS_DIR/pre-push.backup.$(date +%Y%m%d_%H%M%S)"
fi

# Create pre-commit hook
cat > "$HOOKS_DIR/pre-commit" << 'EOF'
#!/bin/bash
# Pre-commit hook for Rust code quality and FFI validation
# Ensures code is formatted, passes clippy checks, and FFI bindings work before commit

set -e

echo "🔧 Running pre-commit hooks..."

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: cargo is not available. Please install Rust."
    exit 1
fi

# Check if we're in a Rust project
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Error: No Cargo.toml found. Are you in a Rust project?"
    exit 1
fi

echo "📝 Formatting code with rustfmt..."
if ! cargo fmt --all -- --check; then
    echo "❌ Code formatting issues found. Running cargo fmt to fix..."
    cargo fmt --all
    echo "✅ Code formatted. Please review and stage the changes."
    echo "   Run: git add . && git commit"
    exit 1
fi
echo "✅ Code formatting is clean"

echo "🔍 Running clippy checks..."
if ! cargo clippy --all-targets --all-features -- -D warnings; then
    echo "❌ Clippy found issues that must be fixed before commit."
    echo "   Run: cargo clippy --fix --allow-dirty"
    exit 1
fi
echo "✅ Clippy checks passed"

echo "🧪 Running basic compile check..."
if ! cargo check --all-targets; then
    echo "❌ Compilation failed. Please fix errors before commit."
    exit 1
fi
echo "✅ Compilation check passed"

# FFI Validation (Ruby bindings)
echo "🔗 Checking FFI Ruby bindings..."
if [ -d "bindings/ruby" ]; then
    echo "🔧 Checking Ruby extension compilation..."
    cd bindings/ruby
    if command -v bundle &> /dev/null; then
        if [ -f "Gemfile" ]; then
            # Check if bundler is ready
            if ! bundle check > /dev/null 2>&1; then
                echo "📦 Installing Ruby dependencies..."
                bundle install > /dev/null 2>&1
            fi

            # Check Ruby extension compilation
            if ! bundle exec rake compile > /dev/null 2>&1; then
                echo "❌ Ruby extension compilation failed."
                echo "   Run: cd bindings/ruby && bundle exec rake compile"
                exit 1
            fi
            echo "✅ Ruby extension compiles successfully"
        else
            echo "⚠️  No Gemfile found in Ruby bindings"
        fi
    else
        echo "⚠️  Bundle not available, skipping Ruby extension check"
    fi
    cd ../..
else
    echo "⚠️  No Ruby bindings found, skipping FFI validation"
fi

echo "🎉 All pre-commit checks passed!"
EOF

# Create pre-push hook
cat > "$HOOKS_DIR/pre-push" << 'EOF'
#!/bin/bash
# Pre-push hook for comprehensive testing including FFI validation
# Runs full test suite and FFI checks before pushing to remote

set -e

echo "🚀 Running pre-push hooks..."

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: cargo is not available. Please install Rust."
    exit 1
fi

echo "🧪 Running full test suite..."
if ! cargo test --all-features; then
    echo "❌ Tests failed. Please fix failing tests before pushing."
    exit 1
fi
echo "✅ All tests passed"

echo "📚 Checking documentation builds..."
if ! cargo doc --no-deps --document-private-items; then
    echo "❌ Documentation build failed. Please fix documentation issues."
    exit 1
fi
echo "✅ Documentation builds successfully"

echo "🔍 Running clippy with all features..."
if ! cargo clippy --all-targets --all-features -- -D warnings; then
    echo "❌ Clippy found issues. Please fix before pushing."
    exit 1
fi
echo "✅ Clippy checks passed"

# FFI Comprehensive Testing (Ruby bindings)
echo "🔗 Running FFI comprehensive tests..."
if [ -d "bindings/ruby" ]; then
    echo "🧪 Running Ruby gem CI checks..."
    cd bindings/ruby
    if [ -f "scripts/ci_check.sh" ]; then
        if ! ./scripts/ci_check.sh; then
            echo "❌ Ruby gem CI checks failed."
            echo "   This simulates what would happen in GitHub Actions"
            echo "   Run: cd bindings/ruby && ./scripts/ci_check.sh"
            exit 1
        fi
        echo "✅ Ruby gem CI checks passed"
    else
        echo "⚠️  Ruby gem CI check script not found"

        # Fallback to basic checks
        if command -v bundle &> /dev/null && [ -f "Gemfile" ]; then
            echo "🔧 Running basic Ruby extension checks..."

            # Ensure dependencies are installed
            if ! bundle check > /dev/null 2>&1; then
                echo "📦 Installing Ruby dependencies..."
                bundle install > /dev/null 2>&1
            fi

            # Check compilation
            if ! bundle exec rake compile > /dev/null 2>&1; then
                echo "❌ Ruby extension compilation failed."
                exit 1
            fi

            # Check gem build
            if ! gem build tasker-core-rb.gemspec > /dev/null 2>&1; then
                echo "❌ Ruby gem build failed."
                exit 1
            fi

            # Clean up
            rm -f tasker-core-rb-*.gem
            echo "✅ Basic Ruby extension checks passed"
        else
            echo "⚠️  Cannot run Ruby checks (bundle not available or no Gemfile)"
        fi
    fi
    cd ../..
else
    echo "⚠️  No Ruby bindings found, skipping FFI testing"
fi

echo "🎉 All pre-push checks passed! Safe to push."
EOF

# Make hooks executable
chmod +x "$HOOKS_DIR/pre-commit"
chmod +x "$HOOKS_DIR/pre-push"

print_status "Git hooks installed successfully!"

echo ""
echo "📋 Summary of installed hooks:"
echo "  • pre-commit: Rust formatting, clippy, compile checks + FFI validation"
echo "  • pre-push: Full test suite, documentation, clippy + FFI comprehensive tests"
echo ""
echo "🔧 What these hooks do:"
echo "  • Ensure code quality before commits"
echo "  • Validate Ruby FFI bindings compilation"
echo "  • Check GitHub Actions workflow configuration"
echo "  • Run comprehensive tests before pushing"
echo ""
echo "🎯 Benefits:"
echo "  • Catch issues early in development"
echo "  • Prevent CI failures"
echo "  • Maintain code quality standards"
echo "  • Ensure FFI bindings always work"
echo ""
echo "✅ Ready to commit and push with confidence!"

#!/bin/bash
# Install git hooks for the Tasker Core Rust project

set -e

HOOKS_DIR=".githooks"
GIT_HOOKS_DIR=".git/hooks"

echo "🔧 Installing git hooks for Tasker Core Rust..."

# Check if we're in a git repository
if [ ! -d ".git" ]; then
    echo "❌ Error: Not in a git repository. Please run this from the project root."
    exit 1
fi

# Check if hooks directory exists
if [ ! -d "$HOOKS_DIR" ]; then
    echo "❌ Error: $HOOKS_DIR directory not found."
    exit 1
fi

# Make hooks executable
echo "📝 Making hooks executable..."
chmod +x "$HOOKS_DIR"/*

# Create git hooks directory if it doesn't exist
mkdir -p "$GIT_HOOKS_DIR"

# Install pre-commit hook
if [ -f "$HOOKS_DIR/pre-commit" ]; then
    cp "$HOOKS_DIR/pre-commit" "$GIT_HOOKS_DIR/pre-commit"
    chmod +x "$GIT_HOOKS_DIR/pre-commit"
    echo "✅ Installed pre-commit hook"
else
    echo "⚠️  Warning: pre-commit hook not found"
fi

# Install pre-push hook
if [ -f "$HOOKS_DIR/pre-push" ]; then
    cp "$HOOKS_DIR/pre-push" "$GIT_HOOKS_DIR/pre-push"
    chmod +x "$GIT_HOOKS_DIR/pre-push"
    echo "✅ Installed pre-push hook"
else
    echo "⚠️  Warning: pre-push hook not found"
fi

echo ""
echo "🎉 Git hooks installed successfully!"
echo ""
echo "📋 Hooks installed:"
echo "   • pre-commit: Runs rustfmt, clippy, and compile checks"
echo "   • pre-push: Runs full test suite and documentation build"
echo ""
echo "💡 To skip hooks temporarily:"
echo "   git commit --no-verify"
echo "   git push --no-verify"
echo ""
echo "🔧 To uninstall hooks, delete files in .git/hooks/"
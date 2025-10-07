#!/bin/bash
# Git Hooks Installation Script
# Installs version-controlled git hooks by creating symlinks

set -e

HOOKS_DIR="$(git rev-parse --show-toplevel)/hooks"
GIT_HOOKS_DIR="$(git rev-parse --show-toplevel)/.git/hooks"

echo "🔧 Installing git hooks..."
echo ""

# Check if hooks directory exists
if [ ! -d "$HOOKS_DIR" ]; then
    echo "❌ Error: hooks directory not found at $HOOKS_DIR"
    exit 1
fi

# Install each hook
installed=0
for hook in pre-commit pre-push; do
    if [ -f "$HOOKS_DIR/$hook" ]; then
        # Remove existing hook if it's not a symlink
        if [ -f "$GIT_HOOKS_DIR/$hook" ] && [ ! -L "$GIT_HOOKS_DIR/$hook" ]; then
            echo "⚠️  Backing up existing $hook hook..."
            mv "$GIT_HOOKS_DIR/$hook" "$GIT_HOOKS_DIR/$hook.backup.$(date +%Y%m%d_%H%M%S)"
        fi

        # Create symlink
        ln -sf "../../hooks/$hook" "$GIT_HOOKS_DIR/$hook"
        echo "✅ Installed $hook hook"
        ((installed++))
    else
        echo "⚠️  Hook not found: $hook"
    fi
done

echo ""
if [ $installed -gt 0 ]; then
    echo "🎉 Successfully installed $installed git hook(s)!"
    echo ""
    echo "Installed hooks will:"
    echo "  • Run rustfmt before commits"
    echo "  • Run clippy checks before commits and pushes"
    echo "  • Verify compilation before commits"
    echo "  • Build documentation before pushes"
    echo ""
    echo "All checks use SQLX_OFFLINE=true for database-independent validation."
    echo ""
    echo "To bypass hooks (emergency only): git commit --no-verify"
else
    echo "❌ No hooks were installed"
    exit 1
fi

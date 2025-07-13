#!/bin/bash
# Multi-workspace validation script
# Run this to check all code in both workspaces

set -e

echo "🔍 Checking main workspace..."
echo "================================"

# Main workspace checks
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test

echo ""
echo "🦀 Checking Ruby extension workspace..."
echo "======================================="

# Ruby extension workspace checks  
cd bindings/ruby/ext/tasker_core
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test

echo ""
echo "✅ All workspace checks passed!"
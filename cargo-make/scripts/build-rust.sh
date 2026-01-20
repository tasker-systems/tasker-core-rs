#!/usr/bin/env bash
# TAS-78: Rust Build Script
#
# Handles split-database mode by enabling SQLX_OFFLINE when PGMQ uses
# a separate database (compile-time verification can only use one DB).
#
# NOTE: This script is called via cargo-make which sets cwd to project root.

set -euo pipefail

# cargo-make sets cwd to project root
SCRIPTS_DIR="$(pwd)/cargo-make/scripts"

# Source split-db detection utility
source "${SCRIPTS_DIR}/split-db-env.sh"

echo "Building Rust crates..."
cargo build --all-features

echo "Rust build complete"

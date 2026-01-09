#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Restore Rust worker binary from CI build
# =============================================================================
# Restores the rust-worker binary from the rust-worker-artifact artifact
# produced by build-workers.yml
#
# Environment variables:
#   ARTIFACTS_DIR - Directory where artifacts were downloaded (default: artifacts/rust-worker)
#
# Usage:
#   ./ci-restore-rust-worker.sh
#   ARTIFACTS_DIR=/path/to/artifacts ./ci-restore-rust-worker.sh
# =============================================================================

ARTIFACTS_DIR="${ARTIFACTS_DIR:-artifacts/rust-worker}"

echo "Restoring Rust worker from ${ARTIFACTS_DIR}..."

# Create target directory
mkdir -p target/debug

if [ -d "${ARTIFACTS_DIR}" ]; then
    # Look for rust-worker binary
    if [ -f "${ARTIFACTS_DIR}/rust-worker" ]; then
        cp -f "${ARTIFACTS_DIR}/rust-worker" target/debug/
        chmod +x target/debug/rust-worker
        echo "  Restored rust-worker"
    else
        # Try find in case it's nested
        found=$(find "${ARTIFACTS_DIR}" -name "rust-worker" -type f 2>/dev/null | head -1)
        if [ -n "$found" ]; then
            cp -f "$found" target/debug/
            chmod +x target/debug/rust-worker
            echo "  Restored rust-worker (found at $found)"
        else
            echo "  Warning: rust-worker binary not found in artifacts"
            echo "  Rust worker will need to be built from source"
        fi
    fi

    # Verify restoration
    if [ -f target/debug/rust-worker ]; then
        echo ""
        echo "Rust worker in target/debug/:"
        ls -lh target/debug/rust-worker 2>/dev/null || true
    fi
else
    echo "  Warning: Artifacts directory not found: ${ARTIFACTS_DIR}"
    echo "  Rust worker will need to be built from source"
    exit 0
fi

echo "Rust worker restoration complete"

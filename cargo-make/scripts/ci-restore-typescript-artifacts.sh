#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Restore TypeScript artifacts from CI build
# =============================================================================
# Restores the TypeScript dist folder and FFI library from the
# typescript-artifacts artifact produced by build-workers.yml
#
# Environment variables:
#   ARTIFACTS_DIR - Directory where artifacts were downloaded (default: artifacts/typescript)
#
# Usage:
#   ./ci-restore-typescript-artifacts.sh
#   ARTIFACTS_DIR=/path/to/artifacts ./ci-restore-typescript-artifacts.sh
# =============================================================================

ARTIFACTS_DIR="${ARTIFACTS_DIR:-artifacts/typescript}"

echo "Restoring TypeScript artifacts from ${ARTIFACTS_DIR}..."

if [ -d "${ARTIFACTS_DIR}" ]; then
    # Restore FFI library to target/release
    mkdir -p target/release

    for lib in libtasker_worker.so libtasker_worker.dylib; do
        # Try flat structure first
        if [ -f "${ARTIFACTS_DIR}/${lib}" ]; then
            cp -f "${ARTIFACTS_DIR}/${lib}" target/release/
            echo "  Restored ${lib} (flat)"
        # Try nested structure
        elif [ -f "${ARTIFACTS_DIR}/target/release/${lib}" ]; then
            cp -f "${ARTIFACTS_DIR}/target/release/${lib}" target/release/
            echo "  Restored ${lib} (nested)"
        fi
    done

    # Verify FFI library
    if [ -f target/release/libtasker_worker.so ] || [ -f target/release/libtasker_worker.dylib ]; then
        echo ""
        echo "FFI library in target/release/:"
        ls -lh target/release/libtasker_worker.* 2>/dev/null || true
    else
        echo "  Warning: FFI library not found in artifacts"
    fi

    # Restore TypeScript dist folder
    mkdir -p workers/typescript/dist

    if [ -d "${ARTIFACTS_DIR}/dist" ]; then
        cp -r "${ARTIFACTS_DIR}/dist/"* workers/typescript/dist/ 2>/dev/null || true
        echo "  Restored TypeScript dist (flat)"
    elif [ -d "${ARTIFACTS_DIR}/workers/typescript/dist" ]; then
        cp -r "${ARTIFACTS_DIR}/workers/typescript/dist/"* workers/typescript/dist/ 2>/dev/null || true
        echo "  Restored TypeScript dist (nested)"
    else
        echo "  Note: No dist folder found - will be built fresh"
    fi
else
    echo "  Warning: Artifacts directory not found: ${ARTIFACTS_DIR}"
    echo "  TypeScript artifacts will need to be built from source"
    exit 0
fi

echo "TypeScript artifacts restoration complete"

#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Restore core artifacts from CI build
# =============================================================================
# Downloads/restores core binaries (tasker-server, tasker-worker, tasker-cli)
# from the core-artifacts artifact produced by build-workers.yml
#
# Environment variables:
#   ARTIFACTS_DIR - Directory where artifacts were downloaded (default: artifacts/core)
#
# Usage:
#   ./ci-restore-core-artifacts.sh
#   ARTIFACTS_DIR=/path/to/artifacts ./ci-restore-core-artifacts.sh
# =============================================================================

ARTIFACTS_DIR="${ARTIFACTS_DIR:-artifacts/core}"

echo "Restoring core artifacts from ${ARTIFACTS_DIR}..."

# Create target directory
mkdir -p target/debug

if [ -d "${ARTIFACTS_DIR}" ]; then
    # Copy core binaries
    for binary in tasker-server tasker-worker tasker-cli; do
        if [ -f "${ARTIFACTS_DIR}/${binary}" ]; then
            cp -f "${ARTIFACTS_DIR}/${binary}" target/debug/
            chmod +x "target/debug/${binary}"
            echo "  Restored ${binary}"
        fi
    done

    # Verify restoration
    echo ""
    echo "Core binaries in target/debug/:"
    ls -lh target/debug/tasker-server target/debug/tasker-worker target/debug/tasker-cli 2>/dev/null || echo "  Warning: Some binaries missing"
else
    echo "  Warning: Artifacts directory not found: ${ARTIFACTS_DIR}"
    echo "  Core binaries will need to be built from source"
    exit 0
fi

echo "Core artifacts restored successfully"

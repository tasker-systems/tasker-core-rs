#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Restore all CI artifacts
# =============================================================================
# Convenience wrapper that restores all artifact types from CI builds.
# Calls individual restoration scripts with appropriate ARTIFACTS_DIR.
#
# Environment variables:
#   ARTIFACTS_BASE - Base directory for all artifacts (default: artifacts)
#
# Expected artifact structure:
#   ${ARTIFACTS_BASE}/
#     core/           -> core-artifacts (tasker-server, tasker-worker, tasker-cli)
#     ruby/           -> ruby-extension (tasker_worker_rb.bundle/.so)
#     typescript/     -> typescript-artifacts (dist/, libtasker_worker.so/.dylib)
#     rust-worker/    -> rust-worker-artifact (rust-worker binary)
#
# Usage:
#   ./ci-restore-all-artifacts.sh
#   ARTIFACTS_BASE=/path/to/artifacts ./ci-restore-all-artifacts.sh
# =============================================================================

ARTIFACTS_BASE="${ARTIFACTS_BASE:-artifacts}"
SCRIPTS_DIR="$(dirname "$0")"

echo "============================================"
echo "Restoring all CI artifacts from ${ARTIFACTS_BASE}"
echo "============================================"
echo ""

# Restore core artifacts
if [ -d "${ARTIFACTS_BASE}/core" ]; then
    ARTIFACTS_DIR="${ARTIFACTS_BASE}/core" "${SCRIPTS_DIR}/ci-restore-core-artifacts.sh"
    echo ""
fi

# Restore Ruby extension
if [ -d "${ARTIFACTS_BASE}/ruby" ]; then
    ARTIFACTS_DIR="${ARTIFACTS_BASE}/ruby" "${SCRIPTS_DIR}/ci-restore-ruby-extension.sh"
    echo ""
fi

# Restore TypeScript artifacts
if [ -d "${ARTIFACTS_BASE}/typescript" ]; then
    ARTIFACTS_DIR="${ARTIFACTS_BASE}/typescript" "${SCRIPTS_DIR}/ci-restore-typescript-artifacts.sh"
    echo ""
fi

# Restore Rust worker
if [ -d "${ARTIFACTS_BASE}/rust-worker" ]; then
    ARTIFACTS_DIR="${ARTIFACTS_BASE}/rust-worker" "${SCRIPTS_DIR}/ci-restore-rust-worker.sh"
    echo ""
fi

echo "============================================"
echo "All artifact restoration complete"
echo "============================================"

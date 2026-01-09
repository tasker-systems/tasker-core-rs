#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Restore Ruby FFI extension from CI build
# =============================================================================
# Restores the Ruby FFI extension (tasker_worker_rb.bundle/.so) from the
# ruby-extension artifact produced by build-workers.yml
#
# Environment variables:
#   ARTIFACTS_DIR - Directory where artifacts were downloaded (default: artifacts/ruby)
#
# Usage:
#   ./ci-restore-ruby-extension.sh
#   ARTIFACTS_DIR=/path/to/artifacts ./ci-restore-ruby-extension.sh
# =============================================================================

ARTIFACTS_DIR="${ARTIFACTS_DIR:-artifacts/ruby}"

echo "Restoring Ruby extension from ${ARTIFACTS_DIR}..."

# Create target directory
mkdir -p workers/ruby/lib/tasker_core

if [ -d "${ARTIFACTS_DIR}" ]; then
    restored=false

    # Look for .bundle (macOS) or .so (Linux) files
    for ext in bundle so; do
        if ls "${ARTIFACTS_DIR}"/*.${ext} 2>/dev/null; then
            cp -f "${ARTIFACTS_DIR}"/*.${ext} workers/ruby/lib/tasker_core/
            echo "  Restored .${ext} files"
            restored=true
        fi
    done

    if [ "$restored" = true ]; then
        echo ""
        echo "Ruby extension in workers/ruby/lib/tasker_core/:"
        ls -lh workers/ruby/lib/tasker_core/ 2>/dev/null || true
    else
        echo "  Warning: No extension files found in artifacts"
        echo "  Ruby extension will need to be built from source"
    fi
else
    echo "  Warning: Artifacts directory not found: ${ARTIFACTS_DIR}"
    echo "  Ruby extension will need to be built from source"
    exit 0
fi

echo "Ruby extension restoration complete"

#!/bin/bash
# =============================================================================
# services-stop-all.sh
# =============================================================================
# Stop all running Tasker services.
#
# NOTE: This script is called via cargo-make which sets cwd to project root.
# =============================================================================

set -euo pipefail

# cargo-make sets cwd to project root
PROJECT_ROOT="$(pwd)"
SCRIPTS_DIR="${PROJECT_ROOT}/cargo-make/scripts"

echo "Stopping all services..."

# Stop each service (ignore errors for services not running)
PROJECT_ROOT="${PROJECT_ROOT}" "${SCRIPTS_DIR}/service-stop.sh" orchestration || true
PROJECT_ROOT="${PROJECT_ROOT}" "${SCRIPTS_DIR}/service-stop.sh" rust-worker || true
PROJECT_ROOT="${PROJECT_ROOT}" "${SCRIPTS_DIR}/service-stop.sh" ruby-worker || true
PROJECT_ROOT="${PROJECT_ROOT}" "${SCRIPTS_DIR}/service-stop.sh" python-worker || true
PROJECT_ROOT="${PROJECT_ROOT}" "${SCRIPTS_DIR}/service-stop.sh" typescript-worker || true

echo "All services stopped"

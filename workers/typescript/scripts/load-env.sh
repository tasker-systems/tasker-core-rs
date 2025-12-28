#!/usr/bin/env bash
# Load test environment variables if .env.test exists
#
# Usage: source scripts/load-env.sh
#
# This script is designed to be sourced (not executed) so that
# environment variables are exported to the calling shell.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

ENV_FILE="$PROJECT_ROOT/.env.test"

if [[ -f "$ENV_FILE" ]]; then
    set -a
    # shellcheck source=/dev/null
    source "$ENV_FILE"
    set +a
fi

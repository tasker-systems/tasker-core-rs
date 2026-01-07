#!/usr/bin/env bash
# Load and export environment variables from .env files
#
# Usage:
#   source scripts/load-env.sh [env_file]
#   source scripts/load-env.sh              # defaults to .env.test
#   source scripts/load-env.sh .env.dev     # load specific file
#
# This script properly exports variables (unlike plain `source .env`)

set -a  # Auto-export all variables that are set

ENV_FILE="${1:-.env.test}"

if [[ ! -f "$ENV_FILE" ]]; then
    echo "Warning: $ENV_FILE not found" >&2
    return 1 2>/dev/null || exit 1
fi

# Source the env file (set -a means all vars get exported)
source "$ENV_FILE"

set +a  # Disable auto-export

echo "Loaded environment from $ENV_FILE"

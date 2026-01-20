#!/usr/bin/env bash
#
# setup-env.sh - Assemble environment files for Tasker services
#
# Usage:
#   ./cargo-make/scripts/setup-env.sh [OPTIONS]
#
# Options:
#   --target=TARGET    Target: root, orchestration, rust-worker, ruby-worker,
#                      python-worker, typescript-worker (default: root)
#   --mode=MODE        Mode: test, test-split, test-cluster, test-cluster-split
#                      (default: test)
#   --output=FILE      Output file (default: .env for target directory)
#   --dry-run          Print what would be generated without writing
#   --help             Show this help message
#
# Examples:
#   ./cargo-make/scripts/setup-env.sh                           # Root .env for standard test
#   ./cargo-make/scripts/setup-env.sh --mode=test-split         # Root .env for split-db test
#   ./cargo-make/scripts/setup-env.sh --mode=test-cluster       # Root .env for cluster test
#   ./cargo-make/scripts/setup-env.sh --mode=test-cluster-split # Root .env for cluster + split-db
#   ./cargo-make/scripts/setup-env.sh --target=rust-worker      # workers/rust/.env
#   ./cargo-make/scripts/setup-env.sh --target=orchestration    # tasker-orchestration/.env

set -euo pipefail

# Resolve script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DOTENV_DIR="${PROJECT_ROOT}/config/dotenv"

# Defaults
TARGET="root"
MODE="test"
OUTPUT=""
DRY_RUN=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --target=*)
            TARGET="${1#*=}"
            shift
            ;;
        --mode=*)
            MODE="${1#*=}"
            shift
            ;;
        --output=*)
            OUTPUT="${1#*=}"
            shift
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help|-h)
            head -27 "$0" | tail -24
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Determine output file based on target
determine_output() {
    if [[ -n "$OUTPUT" ]]; then
        echo "$OUTPUT"
        return
    fi

    case $TARGET in
        root)
            echo "${PROJECT_ROOT}/.env"
            ;;
        orchestration)
            echo "${PROJECT_ROOT}/tasker-orchestration/.env"
            ;;
        rust-worker)
            echo "${PROJECT_ROOT}/workers/rust/.env"
            ;;
        ruby-worker)
            echo "${PROJECT_ROOT}/workers/ruby/.env"
            ;;
        python-worker)
            echo "${PROJECT_ROOT}/workers/python/.env"
            ;;
        typescript-worker)
            echo "${PROJECT_ROOT}/workers/typescript/.env"
            ;;
        *)
            echo "Unknown target: $TARGET" >&2
            exit 1
            ;;
    esac
}

# Get list of source files to concatenate
get_source_files() {
    local files=()

    # Always start with base
    files+=("${DOTENV_DIR}/base.env")

    # Add test.env for all test modes
    case "$MODE" in
        test|test-split|test-cluster|test-cluster-split)
            files+=("${DOTENV_DIR}/test.env")
            ;;
    esac

    # Add split overrides if requested (before cluster to allow cluster to override)
    case "$MODE" in
        test-split|test-cluster-split)
            files+=("${DOTENV_DIR}/test-split.env")
            ;;
    esac

    # Add cluster overrides if requested
    case "$MODE" in
        test-cluster|test-cluster-split)
            files+=("${DOTENV_DIR}/cluster.env")
            ;;
    esac

    # Add target-specific file
    case $TARGET in
        root)
            # No additional file for root
            ;;
        orchestration)
            files+=("${DOTENV_DIR}/orchestration.env")
            ;;
        rust-worker)
            files+=("${DOTENV_DIR}/rust-worker.env")
            ;;
        ruby-worker)
            files+=("${DOTENV_DIR}/ruby-worker.env")
            ;;
        python-worker)
            files+=("${DOTENV_DIR}/python-worker.env")
            ;;
        typescript-worker)
            files+=("${DOTENV_DIR}/typescript-worker.env")
            ;;
    esac

    echo "${files[@]}"
}

# Expand variables in content (simple expansion for WORKSPACE_PATH)
expand_variables() {
    local content="$1"
    # Replace ${WORKSPACE_PATH} with actual path
    echo "$content" | sed "s|\${WORKSPACE_PATH}|${PROJECT_ROOT}|g"
}

# Deduplicate env vars - keep only the last occurrence of each key
# TAS-78: This ensures override files (like test-split.env) properly replace
# base values, making the output compatible with cargo-make's env_files feature
# Uses awk for portability (works with bash 3.x)
deduplicate_env() {
    awk '
    BEGIN { FS="="; OFS="=" }
    # Skip comments and empty lines
    /^[[:space:]]*#/ { next }
    /^[[:space:]]*$/ { next }
    # Parse KEY=value
    /^[A-Za-z_][A-Za-z0-9_]*=/ {
        # Get the key (first field)
        key = $1
        # Get the value (everything after first =)
        value = $0
        sub(/^[^=]*=/, "", value)
        # Track first occurrence order
        if (!(key in seen)) {
            order[++count] = key
            seen[key] = 1
        }
        # Store last value
        values[key] = value
    }
    END {
        for (i = 1; i <= count; i++) {
            key = order[i]
            print key "=" values[key]
        }
    }
    '
}

# Main
main() {
    local output_file
    output_file=$(determine_output)

    local -a source_files
    read -ra source_files <<< "$(get_source_files)"

    # Validate source files exist
    for f in "${source_files[@]}"; do
        if [[ ! -f "$f" ]]; then
            echo "Error: Source file not found: $f" >&2
            exit 1
        fi
    done

    # Generate header
    local header="# Generated by setup-env.sh
# Target: ${TARGET}
# Mode: ${MODE}
# Generated: $(date -Iseconds)
#
# Sources:
$(printf '#   - %s\n' "${source_files[@]}")
#
# DO NOT EDIT - Regenerate with: cargo make setup-env-${TARGET}
#
# Note: Variables are deduplicated - later source files override earlier ones.
# This ensures compatibility with cargo-make env_files loading.
"

    # Collect and expand all content, then deduplicate
    local combined_content=""
    for f in "${source_files[@]}"; do
        combined_content+="$(expand_variables "$(cat "$f")")"$'\n'
    done
    local deduplicated
    deduplicated=$(echo "$combined_content" | deduplicate_env)

    if $DRY_RUN; then
        echo "=== Would write to: $output_file ==="
        echo "$header"
        echo ""
        echo "$deduplicated"
        return
    fi

    # Write output file
    {
        echo "$header"
        echo ""
        echo "$deduplicated"
    } > "$output_file"

    echo "✅ Generated: $output_file"
    echo "   Target: $TARGET"
    echo "   Mode: $MODE"
    echo "   Sources: ${#source_files[@]} files"
}

main

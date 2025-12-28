#!/bin/bash
# =============================================================================
# Create Workspace Stubs
# =============================================================================
# Creates minimal stub files for Cargo workspace members that aren't needed
# by a particular Docker build. This satisfies Cargo's workspace validation
# without copying full source code.
#
# Usage: create-workspace-stubs.sh [crate1] [crate2] ...
#   Arguments are the crates that need stubs (not the ones being built)
#
# Example:
#   # For orchestration build (needs stubs for worker crates)
#   ./create-workspace-stubs.sh tasker-worker workers/rust workers/ruby workers/python workers/typescript
#
# Each stub consists of:
#   - A minimal src/lib.rs with a stub function
#   - The actual Cargo.toml (must be COPY'd separately)

set -e

STUB_CONTENT='pub fn stub() {}'

# Map of crate names to their paths in the workspace
declare -A CRATE_PATHS=(
    ["tasker-orchestration"]="tasker-orchestration"
    ["tasker-worker"]="tasker-worker"
    ["tasker-shared"]="tasker-shared"
    ["tasker-client"]="tasker-client"
    ["pgmq-notify"]="pgmq-notify"
    ["workers/rust"]="workers/rust"
    ["workers/ruby"]="workers/ruby/ext/tasker_core"
    ["workers/python"]="workers/python"
    ["workers/typescript"]="workers/typescript"
)

create_stub() {
    local crate_key="$1"
    local crate_path="${CRATE_PATHS[$crate_key]}"

    if [ -z "$crate_path" ]; then
        echo "Warning: Unknown crate '$crate_key', using as literal path"
        crate_path="$crate_key"
    fi

    echo "Creating stub for: $crate_path"
    mkdir -p "$crate_path/src"
    echo "$STUB_CONTENT" > "$crate_path/src/lib.rs"
}

# Process all arguments as crates needing stubs
for crate in "$@"; do
    create_stub "$crate"
done

echo "Workspace stubs created successfully"

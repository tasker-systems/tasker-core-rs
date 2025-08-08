#!/bin/bash
# Shared utilities for multi-workspace validation
# Provides reusable functions for git hooks and scripts

# Global variables for workspace management
ORIGINAL_PWD=""
WORKSPACE_VALIDATION_FAILED=0

# Initialize workspace utilities
init_workspace_utils() {
    ORIGINAL_PWD="$(pwd)"
    WORKSPACE_VALIDATION_FAILED=0
}

# Reset to original directory (cleanup function)
reset_to_original_directory() {
    if [[ -n "$ORIGINAL_PWD" ]]; then
        cd "$ORIGINAL_PWD" || {
            echo "❌ CRITICAL: Failed to return to original directory: $ORIGINAL_PWD" >&2
            exit 1
        }
    fi
}

# Trap to ensure we always return to original directory
setup_directory_trap() {
    trap reset_to_original_directory EXIT INT TERM
}

# Validate a single workspace with comprehensive checks
# Usage: validate_workspace <workspace_path> <workspace_name> <check_type>
# check_type: "pre-commit" | "pre-push" | "full"
validate_workspace() {
    local workspace_path="$1"
    local workspace_name="$2"
    local check_type="${3:-pre-commit}"

    # Validate arguments
    if [[ -z "$workspace_path" || -z "$workspace_name" ]]; then
        echo "❌ Error: validate_workspace requires workspace_path and workspace_name" >&2
        return 1
    fi

    echo ""
    echo "🔍 Validating $workspace_name workspace..."
    echo "======================================="
    echo "📁 Path: $workspace_path"

    # Store current directory for restoration
    local original_dir="$(pwd)"

    # Navigate to workspace (with error handling)
    if ! cd "$workspace_path" 2>/dev/null; then
        echo "❌ Error: Cannot access workspace directory: $workspace_path" >&2
        return 1
    fi

    # Verify this is a valid Rust workspace
    if [[ ! -f "Cargo.toml" ]]; then
        echo "❌ Error: No Cargo.toml found in workspace: $workspace_path" >&2
        cd "$original_dir" || exit 1
        return 1
    fi

    # Track success/failure for this workspace
    local workspace_success=1

    # Function to run command with proper error handling
    run_workspace_command() {
        local cmd_name="$1"
        local cmd_icon="$2"
        shift 2
        local cmd=("$@")

        echo "$cmd_icon Running $cmd_name in $workspace_name..."
        if "${cmd[@]}"; then
            echo "✅ $cmd_name passed in $workspace_name"
        else
            echo "❌ $cmd_name failed in $workspace_name" >&2
            workspace_success=0
            WORKSPACE_VALIDATION_FAILED=1
        fi
    }

    # Core checks (always run)
    run_workspace_command "code formatting check" "📝" cargo fmt --check
    run_workspace_command "clippy linting" "🔍" cargo clippy --all-targets --all-features -- -D warnings
    run_workspace_command "compilation check" "🔧" cargo check --all-targets --all-features

    # Additional checks based on type
    case "$check_type" in
        "pre-push"|"full")
            run_workspace_command "test suite" "🧪" cargo test --all-features
            run_workspace_command "documentation build" "📚" cargo doc --no-deps --document-private-items --quiet
            ;;
        "pre-commit")
            # Pre-commit only runs basic checks (already done above)
            ;;
        *)
            echo "⚠️  Warning: Unknown check type '$check_type', running pre-commit checks" >&2
            ;;
    esac

    # Always return to original directory
    cd "$original_dir" || {
        echo "❌ CRITICAL: Failed to return to original directory: $original_dir" >&2
        exit 1
    }

    # Report workspace result
    if [[ $workspace_success -eq 1 ]]; then
        echo "✅ $workspace_name workspace validation passed"
    else
        echo "❌ $workspace_name workspace validation failed"
    fi

    return $((1 - workspace_success))
}

# Validate all workspaces in the project
# Usage: validate_all_workspaces <check_type>
validate_all_workspaces() {
    local check_type="${1:-pre-commit}"

    # Initialize utilities
    init_workspace_utils
    setup_directory_trap

    echo "🚀 Running multi-workspace validation ($check_type)"
    echo "=================================================="

    # Define workspace configurations
    # Format: "path:name"
    local workspaces=(
        ".:Main Core"
        "bindings/ruby/ext/tasker_core:Ruby Extension"
    )

    # Validate each workspace
    for workspace_config in "${workspaces[@]}"; do
        IFS=':' read -r workspace_path workspace_name <<< "$workspace_config"

        # Skip if workspace doesn't exist
        if [[ ! -d "$workspace_path" ]]; then
            echo "⚠️  Skipping $workspace_name: directory $workspace_path not found"
            continue
        fi

        # Skip if no Cargo.toml in workspace
        if [[ ! -f "$workspace_path/Cargo.toml" ]]; then
            echo "⚠️  Skipping $workspace_name: no Cargo.toml in $workspace_path"
            continue
        fi

        validate_workspace "$workspace_path" "$workspace_name" "$check_type"
    done

    # Final result
    echo ""
    echo "=================================================="
    if [[ $WORKSPACE_VALIDATION_FAILED -eq 0 ]]; then
        echo "🎉 All workspace validations passed!"
        return 0
    else
        echo "❌ One or more workspace validations failed!"
        return 1
    fi
}

# Check if cargo is available (utility function)
check_cargo_available() {
    if ! command -v cargo &> /dev/null; then
        echo "❌ Error: cargo is not available. Please install Rust." >&2
        return 1
    fi
    return 0
}

# Auto-fix formatting issues across all workspaces
auto_fix_formatting() {
    init_workspace_utils
    setup_directory_trap

    echo "🔧 Auto-fixing formatting across all workspaces..."

    local workspaces=(
        ".:Main Core"
        "bindings/ruby/ext/tasker_core:Ruby Extension"
    )

    for workspace_config in "${workspaces[@]}"; do
        IFS=':' read -r workspace_path workspace_name <<< "$workspace_config"

        if [[ -d "$workspace_path" && -f "$workspace_path/Cargo.toml" ]]; then
            echo "📝 Formatting $workspace_name..."
            local original_dir="$(pwd)"

            if cd "$workspace_path" 2>/dev/null; then
                cargo fmt
                cd "$original_dir" || exit 1
                echo "✅ $workspace_name formatted"
            else
                echo "❌ Failed to access $workspace_name at $workspace_path" >&2
            fi
        fi
    done

    echo "✅ Formatting complete across all workspaces"
}

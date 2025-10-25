#!/usr/bin/env bash
#
# Remove unused dependencies from Cargo.toml files
#
# Usage:
#   ./scripts/remove-unused-deps.sh --verified-only    # Remove only verified unused deps
#   ./scripts/remove-unused-deps.sh --with-verification # Remove after cargo tree verification
#   ./scripts/remove-unused-deps.sh --dry-run          # Show what would be removed
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Dependencies verified as completely unused (not in cargo tree)
VERIFIED_UNUSED=(
    "factori"
    "mockall"
    "proptest"
    "insta"
)

# Dependencies likely unused (need cargo tree verification)
LIKELY_UNUSED=(
    "cargo-llvm-cov"
    "rust_decimal"
    "tokio-test"
    "sysinfo"
)

DRY_RUN=false
MODE="help"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --verified-only)
            MODE="verified"
            shift
            ;;
        --with-verification)
            MODE="verify"
            shift
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help|-h)
            MODE="help"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            MODE="help"
            shift
            ;;
    esac
done

show_help() {
    cat << EOF
Remove unused dependencies from Cargo.toml files

Usage:
    $0 --verified-only        Remove only verified unused deps (safe)
    $0 --with-verification    Remove after verifying with cargo tree
    $0 --dry-run              Show what would be removed (combine with other flags)
    $0 --help                 Show this help

Modes:
    --verified-only:    Remove factori, mockall, proptest, insta
                        These are verified as not in the dependency tree.

    --with-verification: Remove cargo-llvm-cov, rust_decimal, tokio-test, sysinfo
                        After verifying each is not in the dependency tree.

Example:
    $0 --verified-only --dry-run    # Preview safe removals
    $0 --verified-only              # Actually remove verified deps
EOF
}

remove_dep_from_file() {
    local dep="$1"
    local file="$2"

    if ! grep -q "^${dep} *=" "$file"; then
        return 1
    fi

    if [ "$DRY_RUN" = true ]; then
        echo -e "  ${YELLOW}Would remove${NC} from $file"
        return 0
    fi

    # Create backup
    cp "$file" "${file}.bak"

    # Remove the dependency line
    sed -i.tmp "/^${dep} *= */d" "$file"
    rm "${file}.tmp"

    echo -e "  ${GREEN}Removed${NC} from $file"
    return 0
}

remove_dependency() {
    local dep="$1"
    local verify="${2:-false}"

    echo -e "${BLUE}Processing: ${dep}${NC}"

    # If verification requested, check cargo tree first
    if [ "$verify" = true ]; then
        if cargo tree -i "$dep" &>/dev/null; then
            echo -e "  ${YELLOW}⚠️  Still in dependency tree, skipping${NC}"
            cargo tree -i "$dep" | head -5
            return
        else
            echo -e "  ${GREEN}✓${NC} Not in dependency tree"
        fi
    fi

    # Find all Cargo.toml files containing this dependency
    local found=false
    while IFS= read -r cargo_file; do
        if grep -q "^${dep} *=" "$cargo_file"; then
            found=true
            remove_dep_from_file "$dep" "$cargo_file"
        fi
    done < <(find . -name "Cargo.toml" -not -path "*/target/*")

    if [ "$found" = false ]; then
        echo -e "  ${YELLOW}Not found in any Cargo.toml${NC}"
    fi

    echo ""
}

# Main execution
case "$MODE" in
    help)
        show_help
        exit 0
        ;;

    verified)
        echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo -e "${BLUE}Removing Verified Unused Dependencies${NC}"
        echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""

        if [ "$DRY_RUN" = true ]; then
            echo -e "${YELLOW}DRY RUN MODE - No changes will be made${NC}"
            echo ""
        fi

        for dep in "${VERIFIED_UNUSED[@]}"; do
            remove_dependency "$dep" false
        done

        if [ "$DRY_RUN" = false ]; then
            echo -e "${GREEN}✅ Removed ${#VERIFIED_UNUSED[@]} verified unused dependencies${NC}"
            echo ""
            echo -e "${BLUE}Next steps:${NC}"
            echo "1. Run: cargo check --all-features"
            echo "2. Run: cargo test --all-features"
            echo "3. Review and commit changes"
            echo ""
            echo -e "${YELLOW}Note: Backup files created as *.bak${NC}"
        fi
        ;;

    verify)
        echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo -e "${BLUE}Removing Dependencies with Verification${NC}"
        echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""

        if [ "$DRY_RUN" = true ]; then
            echo -e "${YELLOW}DRY RUN MODE - No changes will be made${NC}"
            echo ""
        fi

        local removed=0
        for dep in "${LIKELY_UNUSED[@]}"; do
            # Check if it's in the tree
            if cargo tree -i "$dep" &>/dev/null; then
                echo -e "${BLUE}Processing: ${dep}${NC}"
                echo -e "  ${YELLOW}⚠️  Still in dependency tree, skipping${NC}"
                echo ""
            else
                remove_dependency "$dep" true
                removed=$((removed + 1))
            fi
        done

        if [ "$DRY_RUN" = false ]; then
            echo -e "${GREEN}✅ Verified and removed ${removed} dependencies${NC}"
            echo ""
            echo -e "${BLUE}Next steps:${NC}"
            echo "1. Run: cargo check --all-features"
            echo "2. Run: cargo test --all-features"
            echo "3. Review and commit changes"
            echo ""
            echo -e "${YELLOW}Note: Backup files created as *.bak${NC}"
        fi
        ;;
esac

exit 0

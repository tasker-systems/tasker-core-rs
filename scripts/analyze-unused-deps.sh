#!/usr/bin/env bash
#
# Analyze potentially unused dependencies in the workspace
#
# This script does a superficial analysis by checking if dependency crates
# have any `use` statements in the codebase. It's not definitive (some crates
# might be used via re-exports, macros, or derive attributes), but it identifies
# good candidates for investigation.
#
# Usage: ./scripts/analyze-unused-deps.sh

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
total_deps=0
unused_count=0
used_count=0

echo -e "${BLUE}🔍 Analyzing dependencies for unused crates...${NC}"
echo ""

# Temporary files
DEPS_FILE=$(mktemp)
USES_FILE=$(mktemp)
RESULTS_FILE=$(mktemp)

# Clean up on exit
trap "rm -f $DEPS_FILE $USES_FILE $RESULTS_FILE" EXIT

# Function to extract dependencies from a Cargo.toml file using grep/sed
extract_deps() {
    local cargo_file="$1"

    # Extract dependencies section and parse dependency names
    sed -n '/^\[dependencies\]/,/^\[/p' "$cargo_file" | \
        grep -E '^[a-zA-Z0-9_-]+ *=' | \
        sed -E 's/^([a-zA-Z0-9_-]+) *=.*/\1/' | \
        grep -v '^\[' || true

    sed -n '/^\[dev-dependencies\]/,/^\[/p' "$cargo_file" | \
        grep -E '^[a-zA-Z0-9_-]+ *=' | \
        sed -E 's/^([a-zA-Z0-9_-]+) *=.*/\1/' | \
        grep -v '^\[' || true
}

# Function to convert crate name to potential Rust module name
crate_to_module() {
    echo "$1" | tr '-' '_'
}

echo -e "${BLUE}📦 Collecting dependencies from Cargo.toml files...${NC}"

# Find all Cargo.toml files and extract dependencies
find . -name "Cargo.toml" -not -path "*/target/*" | while read -r cargo_file; do
    extract_deps "$cargo_file" >> "$DEPS_FILE"
done

# Remove duplicates and sort
sort -u "$DEPS_FILE" -o "$DEPS_FILE"

total_deps=$(wc -l < "$DEPS_FILE" | tr -d ' ')
echo -e "${BLUE}Found ${total_deps} unique dependencies${NC}"

echo -e "${BLUE}🔎 Collecting all use statements from source code...${NC}"

# Single pass through all Rust files to collect use statements
# Much faster than grepping once per dependency
grep -rh \
    --include="*.rs" \
    --exclude-dir=target \
    -E "^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]+[a-zA-Z0-9_]+" . \
    2>/dev/null | \
    sed -E 's/^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]+([a-zA-Z0-9_]+).*/\2/' | \
    sort -u > "$USES_FILE"

echo -e "${BLUE}Collected $(wc -l < "$USES_FILE" | tr -d ' ') unique module names from use statements${NC}"
echo ""

echo -e "${BLUE}🔎 Matching dependencies against source code...${NC}"
echo ""

while IFS= read -r dep; do
    # Skip empty lines
    [ -z "$dep" ] && continue

    # Convert to module name (kebab-case to snake_case)
    module_name=$(crate_to_module "$dep")

    # Check if this module name appears in our use statements
    if grep -q "^${module_name}$" "$USES_FILE"; then
        echo -e "${GREEN}✓${NC}  ${dep} (module: ${module_name})"
        used_count=$((used_count + 1))
    else
        echo -e "${YELLOW}⚠️  ${dep}${NC} (module: ${module_name}) - ${RED}no use statements found${NC}"
        echo "${dep} (${module_name})" >> "$RESULTS_FILE"
        unused_count=$((unused_count + 1))
    fi
done < "$DEPS_FILE"

# Summary
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📊 Summary${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "Total dependencies analyzed: ${BLUE}${total_deps}${NC}"
echo -e "Dependencies with uses:      ${GREEN}${used_count}${NC}"
echo -e "Potentially unused:          ${YELLOW}${unused_count}${NC}"
echo ""

if [ "$unused_count" -gt 0 ]; then
    echo -e "${YELLOW}⚠️  Potentially Unused Dependencies:${NC}"
    echo ""
    cat "$RESULTS_FILE" | while IFS= read -r line; do
        echo "  • $line"
    done
    echo ""
    echo -e "${YELLOW}Note:${NC} These dependencies might still be used via:"
    echo "  - Derive macros (e.g., #[derive(Serialize)], #[derive(Debug)])"
    echo "  - Re-exports from other crates"
    echo "  - Trait implementations without explicit 'use'"
    echo "  - Build dependencies (build.rs)"
    echo "  - Required by other dependencies (transitive)"
    echo "  - Feature flags that enable optional functionality"
    echo ""
    echo -e "${BLUE}💡 Recommendation:${NC} Review each flagged dependency manually before removing."
    echo "   Use 'cargo tree -i <package>' to see what depends on it."
    echo ""

    # Calculate potential compilation time savings estimate
    if [ "$total_deps" -gt 0 ]; then
        savings_percent=$((unused_count * 100 / total_deps))
        echo -e "${BLUE}💰 Potential Impact:${NC}"
        echo "  - ${unused_count}/${total_deps} dependencies flagged (${savings_percent}%)"
        echo "  - Could reduce initial compilation time"
        echo "  - May reduce incremental rebuild times"
        echo "  - Smaller binary size"
        echo ""
    fi
fi

exit 0

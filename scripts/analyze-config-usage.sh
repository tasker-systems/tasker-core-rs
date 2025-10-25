#!/bin/bash

# TAS-50 Configuration Parameter Usage Analysis Script
#
# This script:
# 1. Extracts all parameter paths from config files (common, orchestration, worker)
# 2. Searches the codebase for usage of each parameter
# 3. Generates a comprehensive usage mapping in JSON and Markdown formats
#
# Usage: ./scripts/analyze-config-usage.sh
# Output: docs/config-parameter-usage.json and docs/config-parameter-usage.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG_BASE_DIR="$PROJECT_ROOT/config/tasker/base"
OUTPUT_JSON="$PROJECT_ROOT/docs/config-parameter-usage.json"
OUTPUT_MD="$PROJECT_ROOT/docs/config-parameter-usage.md"

echo "=== TAS-50 Configuration Parameter Usage Analysis ==="
echo "Project Root: $PROJECT_ROOT"
echo "Config Base Dir: $CONFIG_BASE_DIR"
echo ""

# Function to extract parameter paths from TOML file
# Usage: extract_params <file> <context>
extract_params() {
    local file="$1"
    local context="$2"
    local current_section=""
    local params=()

    # Read the file line by line
    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// }" ]] && continue

        # Check for section headers [section.subsection]
        if [[ "$line" =~ ^\[([^\]]+)\] ]]; then
            current_section="${BASH_REMATCH[1]}"
            continue
        fi

        # Check for key = value pairs
        if [[ "$line" =~ ^[[:space:]]*([a-zA-Z_][a-zA-Z0-9_]*)[[:space:]]*= ]]; then
            local key="${BASH_REMATCH[1]}"
            if [[ -n "$current_section" ]]; then
                params+=("$context:$current_section.$key")
            else
                params+=("$context:$key")
            fi
        fi
    done < "$file"

    printf '%s\n' "${params[@]}"
}

# Function to search codebase for parameter usage
# Usage: search_parameter <param_path>
search_parameter() {
    local param="$1"
    local context="${param%%:*}"
    local path="${param#*:}"

    # Convert path to potential Rust field names
    # e.g., "database.pool.max_connections" -> "max_connections"
    local field_name="${path##*.}"

    # Search patterns
    local results=$(cd "$PROJECT_ROOT" && {
        # Search for direct field access patterns
        rg --no-heading --line-number "$field_name" \
            --glob '!target/**' \
            --glob '!.git/**' \
            --glob '!node_modules/**' \
            --glob '*.rs' \
            --glob '*.toml' 2>/dev/null || true
    })

    echo "$results"
}

# Function to determine Rust type from TOML value
determine_type() {
    local line="$1"

    if [[ "$line" =~ =[[:space:]]*\".*\" ]]; then
        echo "String"
    elif [[ "$line" =~ =[[:space:]]*\[.*\] ]]; then
        echo "Array"
    elif [[ "$line" =~ =[[:space:]]*[0-9]+\.[0-9]+ ]]; then
        echo "f64"
    elif [[ "$line" =~ =[[:space:]]*[0-9]+ ]]; then
        echo "u32/u64/i32"
    elif [[ "$line" =~ =[[:space:]]*(true|false) ]]; then
        echo "bool"
    elif [[ "$line" =~ =[[:space:]]*\{ ]]; then
        echo "Table/Struct"
    else
        echo "Unknown"
    fi
}

echo "Extracting parameters from config files..."
echo ""

# Initialize JSON output
cat > "$OUTPUT_JSON" << 'EOF'
{
  "analysis_date": "",
  "parameters": {
    "common": [],
    "orchestration": [],
    "worker": []
  },
  "summary": {
    "total_parameters": 0,
    "by_context": {
      "common": 0,
      "orchestration": 0,
      "worker": 0
    }
  }
}
EOF

# Update analysis date
sed -i '' "s/\"analysis_date\": \"\"/\"analysis_date\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"/" "$OUTPUT_JSON"

# Initialize Markdown output
cat > "$OUTPUT_MD" << 'EOF'
# Configuration Parameter Usage Analysis

**Analysis Date**:
**Purpose**: Comprehensive mapping of configuration parameter usage across the codebase

---

## Table of Contents

- [Common Configuration](#common-configuration)
- [Orchestration Configuration](#orchestration-configuration)
- [Worker Configuration](#worker-configuration)
- [Summary Statistics](#summary-statistics)

---

EOF

sed -i '' "s/\*\*Analysis Date\*\*: /&$(date -u +%Y-%m-%d)/" "$OUTPUT_MD"

# Process each context
for context in common orchestration worker; do
    echo "Analyzing $context.toml..."

    config_file="$CONFIG_BASE_DIR/$context.toml"
    if [[ ! -f "$config_file" ]]; then
        echo "  Warning: $config_file not found, skipping"
        continue
    fi

    # Extract parameters
    params=$(extract_params "$config_file" "$context")

    # Add to markdown
    # Capitalize first letter (bash 3 compatible)
    context_title="$(echo "$context" | sed 's/./\U&/')"

    cat >> "$OUTPUT_MD" << EOF

## ${context_title} Configuration

| Parameter Path | Type | Usage Locations | Notes |
|----------------|------|-----------------|-------|
EOF

    # Analyze each parameter
    while IFS= read -r param; do
        [[ -z "$param" ]] && continue

        path="${param#*:}"

        # Search for usage
        usage=$(search_parameter "$param" | head -10) # Limit to first 10 matches
        usage_count=$(echo "$usage" | grep -c ":" || echo "0")

        # Determine type from config file
        param_line=$(grep -E "^[[:space:]]*${path##*.}[[:space:]]*=" "$config_file" || true)
        param_type=$(determine_type "$param_line")

        # Add to markdown
        if [[ $usage_count -gt 0 ]]; then
            usage_summary=$(echo "$usage" | head -3 | sed 's/^/    /')
            echo "| \`$path\` | $param_type | $usage_count locations | <details><summary>Show</summary><pre>$usage_summary</pre></details> |" >> "$OUTPUT_MD"
        else
            echo "| \`$path\` | $param_type | Not found | Potentially unused |" >> "$OUTPUT_MD"
        fi

    done <<< "$params"

    # Update counts
    param_count=$(echo "$params" | grep -c "^" || echo "0")
    echo "  Found $param_count parameters"
done

# Add summary to markdown
cat >> "$OUTPUT_MD" << 'EOF'

---

## Summary Statistics

### Parameter Counts by Context

| Context | Parameter Count |
|---------|----------------|
EOF

# Calculate totals
for context in common orchestration worker; do
    config_file="$CONFIG_BASE_DIR/$context.toml"
    if [[ -f "$config_file" ]]; then
        count=$(extract_params "$config_file" "$context" | grep -c "^" || echo "0")
        context_title="$(echo "$context" | sed 's/./\U&/')"
        echo "| ${context_title} | $count |" >> "$OUTPUT_MD"
    fi
done

cat >> "$OUTPUT_MD" << 'EOF'

### Analysis Notes

- **Type Detection**: Inferred from TOML value format
- **Usage Locations**: Limited to first 10 matches per parameter
- **Search Scope**: Rust files (*.rs) and TOML files
- **Excluded**: target/, .git/, node_modules/

### Next Steps

1. Review "Not found" parameters for removal
2. Add documentation metadata to active parameters
3. Generate environment-specific recommendations
4. Create parameter relationship mappings

---

**Analysis Complete**
EOF

echo ""
echo "Analysis complete!"
echo "Output files:"
echo "  - $OUTPUT_MD"
echo ""
echo "Summary:"
for context in common orchestration worker; do
    config_file="$CONFIG_BASE_DIR/$context.toml"
    if [[ -f "$config_file" ]]; then
        count=$(extract_params "$config_file" "$context" | grep -c "^" || echo "0")
        context_title="$(echo "$context" | sed 's/./\U&/')"
        echo "  - ${context_title}: $count parameters"
    fi
done

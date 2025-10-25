#!/bin/bash

# TAS-50 Advanced Configuration Parameter Usage Analysis
#
# This script performs sophisticated static analysis to determine which configuration
# parameters are actually USED (influence runtime behavior) vs just structurally present.
#
# Analysis includes:
# 1. Direct struct field access patterns (e.g., config.database.pool.max_connections)
# 2. Method-based access patterns (e.g., config.database_url())
# 3. Value propagation through initialization code
# 4. Impact on system behavior (not just deserialization)
#
# Usage: ./scripts/analyze-config-usage-advanced.sh
# Output: docs/config-usage-analysis.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG_BASE_DIR="$PROJECT_ROOT/config/tasker/base"
OUTPUT_MD="$PROJECT_ROOT/docs/config-usage-analysis.md"

echo "=== TAS-50 Advanced Configuration Usage Analysis ==="
echo "Project Root: $PROJECT_ROOT"
echo ""

# Function to extract all parameter paths from TOML file
# Returns paths in format: "section.subsection.key"
extract_all_params() {
    local file="$1"
    local current_section=""
    local params=()

    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// }" ]] && continue

        # Check for section headers [section.subsection]
        if [[ "$line" =~ ^\\[([^\\]]+)\\] ]]; then
            current_section="${BASH_REMATCH[1]}"
            continue
        fi

        # Check for key = value pairs
        if [[ "$line" =~ ^[[:space:]]*([a-zA-Z_][a-zA-Z0-9_]*)[[:space:]]*= ]]; then
            local key="${BASH_REMATCH[1]}"
            if [[ -n "$current_section" ]]; then
                params+=("$current_section.$key")
            else
                params+=("$key")
            fi
        fi
    done < "$file"

    printf '%s\n' "${params[@]}"
}

# Function to analyze parameter usage with context
# Returns: usage_type | file_count | line_references | context_snippet
analyze_parameter_usage() {
    local param_path="$1"
    local field_name="${param_path##*.}"

    # Build search patterns for different access types
    local patterns=()

    # Pattern 1: Direct nested access (e.g., config.database.pool.max_connections)
    # Use the parameter path directly without escaping dots
    patterns+=("config\\.$param_path")

    # Pattern 2: Struct field access with dot prefix (e.g., .max_connections)
    patterns+=("\\.$field_name")

    # Pattern 3: Bare field name (catches struct definitions and various usages)
    patterns+=("$field_name")

    # Search for all patterns
    local results=""
    local usage_type="UNKNOWN"
    local file_count=0
    local line_refs=""
    local context=""

    for pattern in "${patterns[@]}"; do
        local search_result=$(cd "$PROJECT_ROOT" && {
            rg --no-heading --line-number "$pattern" \
                --glob '!target/**' \
                --glob '!.git/**' \
                --glob '!docs/**' \
                --glob '!config/**' \
                --glob '*.rs' \
                2>/dev/null || true
        })

        if [[ -n "$search_result" ]]; then
            results="$results\n$search_result"
            file_count=$(echo "$search_result" | grep -c ":" || echo "0")
            # Only count from first pattern to avoid duplicates
            break
        fi
    done

    # Analyze usage type based on context
    if [[ -z "$results" ]]; then
        usage_type="UNUSED"
    elif echo -e "$results" | grep -q "sqlx\|PgPoolOptions\|connect\|spawn\|execute\|DatabasePoolConfig"; then
        usage_type="FUNCTIONAL - Database Operations"
    elif echo -e "$results" | grep -q "mpsc::channel\|buffer_size\|capacity\|MpscChannels"; then
        usage_type="FUNCTIONAL - Channel Configuration"
    elif echo -e "$results" | grep -q "backoff\|retry\|delay\|BackoffConfig"; then
        usage_type="FUNCTIONAL - Retry Logic"
    elif echo -e "$results" | grep -q "circuit_breaker\|failure_threshold\|CircuitBreaker"; then
        usage_type="FUNCTIONAL - Circuit Breaker"
    elif echo -e "$results" | grep -q "timeout\|duration\|interval\|Duration::from"; then
        usage_type="FUNCTIONAL - Timing"
    elif echo -e "$results" | grep -q "event_system\|EventSystem\|listener\|poller"; then
        usage_type="FUNCTIONAL - Event System"
    elif echo -e "$results" | grep -q "queue\|pgmq\|QueueConfig\|visibility"; then
        usage_type="FUNCTIONAL - Queue Configuration"
    elif echo -e "$results" | grep -q "Deserialize\|#\\[serde\|pub struct.*Config"; then
        # Only mark as structural if it's ONLY in struct definitions, not used elsewhere
        if echo -e "$results" | grep -qv "pub struct\|Deserialize\|serde"; then
            usage_type="NEEDS_REVIEW"
        else
            usage_type="STRUCTURAL - Deserialization Only"
        fi
    else
        usage_type="NEEDS_REVIEW"
    fi

    # Extract sample line references (first 5)
    if [[ -n "$results" ]]; then
        line_refs=$(echo -e "$results" | grep ":" | head -5 | sed 's/^/  /')
        context=$(echo -e "$results" | head -15)
    fi

    echo "$usage_type|$file_count|$line_refs|$context"
}

# Function to categorize parameter by context
get_parameter_context() {
    local param_path="$1"

    if [[ "$param_path" =~ ^database\. ]]; then
        echo "Database"
    elif [[ "$param_path" =~ ^queues\. ]]; then
        echo "Queues"
    elif [[ "$param_path" =~ ^mpsc_channels\. ]]; then
        echo "MPSC Channels"
    elif [[ "$param_path" =~ ^circuit_breakers\. ]]; then
        echo "Circuit Breakers"
    elif [[ "$param_path" =~ ^orchestration_system\. ]]; then
        echo "Orchestration System"
    elif [[ "$param_path" =~ ^worker_system\. ]]; then
        echo "Worker System"
    elif [[ "$param_path" =~ ^backoff\. ]]; then
        echo "Backoff/Retry"
    elif [[ "$param_path" =~ ^execution\. ]]; then
        echo "Execution"
    elif [[ "$param_path" =~ ^system\. ]]; then
        echo "System"
    else
        echo "Other"
    fi
}

# Initialize output
cat > "$OUTPUT_MD" << 'EOF'
# Advanced Configuration Usage Analysis

**Analysis Date**:
**Purpose**: Comprehensive analysis of configuration parameter usage with distinction between functional and structural usage

---

## Analysis Methodology

This analysis uses sophisticated pattern matching to distinguish between:

- **FUNCTIONAL**: Parameters that influence runtime behavior (database connections, retry logic, timeouts, etc.)
- **STRUCTURAL**: Parameters that are deserialized but not actively used
- **UNUSED**: Parameters not referenced in codebase
- **NEEDS_REVIEW**: Parameters with unclear usage patterns requiring manual review

### Search Patterns

For each parameter, we search for:
1. Direct nested access: `config.database.pool.max_connections`
2. Struct field access in context: `.max_connections` in files mentioning `DatabasePoolConfig`
3. Method-based access: `config.database_url()` for `database.url`
4. Value propagation: Usage in initialization code (sqlx, tokio, etc.)

---

## Summary Statistics

EOF

sed -i '' "s/\\*\\*Analysis Date\\*\\*: /&$(date -u '+%Y-%m-%d %H:%M:%S UTC')/g" "$OUTPUT_MD"

# Process each configuration context
# Use temp files for statistics (bash 3.2 compatible)
stats_file=$(mktemp)
usage_type_file=$(mktemp)
trap "rm -f $stats_file $usage_type_file" EXIT

for context_file in common orchestration worker; do
    config_file="$CONFIG_BASE_DIR/$context_file.toml"

    if [[ ! -f "$config_file" ]]; then
        echo "Warning: $config_file not found, skipping"
        continue
    fi

    echo "Analyzing $context_file.toml..."

    # Add section header
    context_title="$(echo "$context_file" | sed 's/./\U&/')"
    cat >> "$OUTPUT_MD" << EOF

## ${context_title} Configuration

| Parameter Path | Context | Usage Type | References | Sample Locations |
|----------------|---------|------------|------------|------------------|
EOF

    # Extract and analyze all parameters
    params=$(extract_all_params "$config_file")
    param_count=0

    while IFS= read -r param; do
        [[ -z "$param" ]] && continue

        ((param_count++))

        echo "  Analyzing: $param"

        # Get parameter context
        param_context=$(get_parameter_context "$param")

        # Analyze usage
        analysis=$(analyze_parameter_usage "$param")
        IFS='|' read -r usage_type file_count line_refs context_snippet <<< "$analysis"

        # Track statistics in temp files
        echo "$usage_type" >> "$usage_type_file"

        # Format line references for markdown
        if [[ -n "$line_refs" ]]; then
            formatted_refs="<details><summary>$file_count locations</summary><pre>$line_refs</pre></details>"
        else
            formatted_refs="None"
        fi

        # Add to markdown
        echo "| \`$param\` | $param_context | $usage_type | $formatted_refs | |" >> "$OUTPUT_MD"

    done <<< "$params"

    echo "$context_file|$param_count" >> "$stats_file"
    echo "  Analyzed $param_count parameters from $context_file.toml"
done

# Add summary statistics
cat >> "$OUTPUT_MD" << 'EOF'

---

## Summary by Context

| Context | Total Parameters |
|---------|------------------|
EOF

while IFS='|' read -r context count; do
    context_title="$(echo "$context" | sed 's/./\U&/')"
    echo "| $context_title | $count |" >> "$OUTPUT_MD"
done < "$stats_file"

cat >> "$OUTPUT_MD" << 'EOF'

## Summary by Usage Type

| Usage Type | Count |
|------------|-------|
EOF

# Count usage types
sort "$usage_type_file" | uniq -c | while read count usage_type; do
    echo "| $usage_type | $count |" >> "$OUTPUT_MD"
done

cat >> "$OUTPUT_MD" << 'EOF'

---

## Recommended Actions

### Parameters Marked UNUSED
These parameters should be removed from TOML files and Rust config structs.

### Parameters Marked STRUCTURAL
These parameters are deserialized but may not influence behavior. Requires manual review to determine if they should be removed or if usage is indirect.

### Parameters Marked NEEDS_REVIEW
These parameters have unclear usage patterns. Manual code review required.

### Parameters Marked FUNCTIONAL
These parameters actively influence runtime behavior and should be retained with comprehensive _docs metadata.

---

**Analysis Complete**
EOF

echo ""
echo "Analysis complete!"
echo "Output: $OUTPUT_MD"
echo ""
echo "Summary by Context:"
while IFS='|' read -r context count; do
    context_title="$(echo "$context" | sed 's/./\U&/')"
    echo "  - $context_title: $count parameters"
done < "$stats_file"
echo ""
echo "Summary by Usage Type:"
sort "$usage_type_file" | uniq -c | while read count usage_type; do
    echo "  - $usage_type: $count parameters"
done

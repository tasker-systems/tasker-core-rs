#!/bin/bash
# TAS-29 Phase 1.5: Logging Standardization Audit Script
# Analyzes all tracing calls and generates a report of issues

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${WORKSPACE_ROOT}/tmp/logging-audit"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Create output directory
mkdir -p "${OUTPUT_DIR}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}TAS-29 Logging Standardization Audit${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Workspace: ${WORKSPACE_ROOT}"
echo "Output: ${OUTPUT_DIR}"
echo ""

# Target directories
CRATES=(
    "tasker-orchestration/src"
    "tasker-worker/src"
    "tasker-shared/src"
    "tasker-client/src"
    "pgmq-notify/src"
)

# Build file list
TEMP_FILES=$(mktemp)
for crate in "${CRATES[@]}"; do
    if [ -d "${WORKSPACE_ROOT}/${crate}" ]; then
        find "${WORKSPACE_ROOT}/${crate}" -name "*.rs" -type f >> "${TEMP_FILES}"
    fi
done

TOTAL_FILES=$(wc -l < "${TEMP_FILES}")
echo -e "${GREEN}Found ${TOTAL_FILES} Rust source files${NC}"
echo ""

# Initialize output files
EMOJI_OUTPUT="${OUTPUT_DIR}/emoji-issues-${TIMESTAMP}.csv"
ALLCAPS_OUTPUT="${OUTPUT_DIR}/allcaps-issues-${TIMESTAMP}.csv"
TAS_OUTPUT="${OUTPUT_DIR}/tas-ref-issues-${TIMESTAMP}.csv"
MISSING_CORR_OUTPUT="${OUTPUT_DIR}/missing-correlation-${TIMESTAMP}.csv"
SUMMARY_OUTPUT="${OUTPUT_DIR}/audit-summary-${TIMESTAMP}.txt"

# CSV headers
echo "file,line,level,issue_type,line_content" > "${EMOJI_OUTPUT}"
echo "file,line,level,prefix,line_content" > "${ALLCAPS_OUTPUT}"
echo "file,line,context,tas_ref,line_content" > "${TAS_OUTPUT}"
echo "file,line,level,context,line_content" > "${MISSING_CORR_OUTPUT}"

# Counters
EMOJI_COUNT=0
ALLCAPS_COUNT=0
TAS_COUNT=0
MISSING_CORR_COUNT=0
TOTAL_TRACING_CALLS=0

echo -e "${YELLOW}Analyzing tracing calls...${NC}"

# Pattern for emojis (common ones used in codebase)
EMOJI_PATTERN='[🔧✅🚀❌⚠️📊🔍🎉🛡️⏱️📝🏗️🎯🔄💡📦🧪🌉🔌⏳🛑]'

# Analyze each file
while IFS= read -r file; do
    # Get relative path
    rel_file="${file#${WORKSPACE_ROOT}/}"

    # Count tracing calls in this file (macros are imported, not qualified)
    file_calls=$(grep -cE "(^|[^:])\\<(info|debug|warn|error|trace)!" "$file" 2>/dev/null || echo 0)
    TOTAL_TRACING_CALLS=$((TOTAL_TRACING_CALLS + file_calls))

    # Find emoji usage
    while IFS=: read -r line_num line_content; do
        if [ -n "$line_content" ]; then
            # Extract log level (macros are imported directly)
            level=$(echo "$line_content" | grep -oE "\\<(info|debug|warn|error|trace)!" | sed 's/!//')
            # Escape commas and quotes for CSV
            escaped_content=$(echo "$line_content" | sed 's/"/\"\"/g' | tr '\n' ' ')
            echo "\"${rel_file}\",${line_num},\"${level}\",\"emoji\",\"${escaped_content}\"" >> "${EMOJI_OUTPUT}"
            EMOJI_COUNT=$((EMOJI_COUNT + 1))
        fi
    done < <(grep -nE "${EMOJI_PATTERN}" "$file" 2>/dev/null || true)

    # Find all-caps prefixes in log messages
    while IFS=: read -r line_num line_content; do
        if [[ "$line_content" =~ \"[A-Z_]{3,}: ]]; then
            level=$(echo "$line_content" | grep -oE "\\<(info|debug|warn|error|trace)!" | sed 's/!//')
            prefix=$(echo "$line_content" | grep -oE '"[A-Z_]{3,}:' | sed 's/"//; s/://')
            escaped_content=$(echo "$line_content" | sed 's/"/\"\"/g' | tr '\n' ' ')
            echo "\"${rel_file}\",${line_num},\"${level}\",\"${prefix}\",\"${escaped_content}\"" >> "${ALLCAPS_OUTPUT}"
            ALLCAPS_COUNT=$((ALLCAPS_COUNT + 1))
        fi
    done < <(grep -nE "\\<(info|debug|warn|error|trace)!" "$file" 2>/dev/null || true)

    # Find TAS-XX references
    while IFS=: read -r line_num line_content; do
        tas_ref=$(echo "$line_content" | grep -oE "TAS-[0-9]{2,3}")
        context="unknown"
        if echo "$line_content" | grep -qE "(info|debug|warn|error|trace)!"; then
            context="log"
        elif echo "$line_content" | grep -qE "^(\s*)//"; then
            context="comment"
        elif echo "$line_content" | grep -qE "^(\s*)(///|//!)"; then
            context="doc"
        fi
        escaped_content=$(echo "$line_content" | sed 's/"/\"\"/g' | tr '\n' ' ')
        echo "\"${rel_file}\",${line_num},\"${context}\",\"${tas_ref}\",\"${escaped_content}\"" >> "${TAS_OUTPUT}"
        TAS_COUNT=$((TAS_COUNT + 1))
    done < <(grep -nE "TAS-[0-9]{2,3}" "$file" 2>/dev/null || true)

    # Find task/step operations missing correlation_id
    # Look for logs with task_uuid or step_uuid but no correlation_id
    while IFS=: read -r line_num line_content; do
        if echo "$line_content" | grep -qE "(task_uuid|step_uuid)" && ! echo "$line_content" | grep -qE "correlation_id"; then
            level=$(echo "$line_content" | grep -oE "\\<(info|debug|warn|error|trace)!" | sed 's/!//')
            context="task_context"
            if echo "$line_content" | grep -qE "step_uuid"; then
                context="step_context"
            fi
            escaped_content=$(echo "$line_content" | sed 's/"/\"\"/g' | tr '\n' ' ')
            echo "\"${rel_file}\",${line_num},\"${level}\",\"${context}\",\"${escaped_content}\"" >> "${MISSING_CORR_OUTPUT}"
            MISSING_CORR_COUNT=$((MISSING_CORR_COUNT + 1))
        fi
    done < <(grep -nE "\\<(info|debug|warn|error|trace)!" "$file" 2>/dev/null || true)

done < "${TEMP_FILES}"

# Generate summary report
{
    echo "========================================="
    echo "TAS-29 Logging Standardization Audit"
    echo "Generated: $(date)"
    echo "========================================="
    echo ""
    echo "Scope:"
    echo "  Total files analyzed: ${TOTAL_FILES}"
    echo "  Total tracing calls: ${TOTAL_TRACING_CALLS}"
    echo ""
    echo "Issues Found:"
    echo "  Emoji usage:            ${EMOJI_COUNT} instances"
    echo "  All-caps prefixes:      ${ALLCAPS_COUNT} instances"
    echo "  TAS-XX references:      ${TAS_COUNT} instances"
    echo "  Missing correlation_id: ${MISSING_CORR_COUNT} instances"
    echo ""
    echo "Issue Breakdown:"
    echo "-----------------"
    echo ""
    if [ ${EMOJI_COUNT} -gt 0 ]; then
        echo "Emoji Usage (${EMOJI_COUNT} total):"
        awk -F',' 'NR>1 {print $1}' "${EMOJI_OUTPUT}" | sort | uniq -c | sort -rn | head -10
        echo ""
    fi

    if [ ${ALLCAPS_COUNT} -gt 0 ]; then
        echo "All-Caps Prefixes (${ALLCAPS_COUNT} total):"
        awk -F',' 'NR>1 {print $4}' "${ALLCAPS_OUTPUT}" | sed 's/"//g' | sort | uniq -c | sort -rn
        echo ""
    fi

    if [ ${TAS_COUNT} -gt 0 ]; then
        echo "TAS-XX References (${TAS_COUNT} total):"
        echo "  In logs:     $(awk -F',' 'NR>1 && $3 ~ /log/ {count++} END {print count+0}' "${TAS_OUTPUT}")"
        echo "  In comments: $(awk -F',' 'NR>1 && $3 ~ /comment/ {count++} END {print count+0}' "${TAS_OUTPUT}")"
        echo "  In docs:     $(awk -F',' 'NR>1 && $3 ~ /doc/ {count++} END {print count+0}' "${TAS_OUTPUT}")"
        echo ""
    fi

    if [ ${MISSING_CORR_COUNT} -gt 0 ]; then
        echo "Missing correlation_id (${MISSING_CORR_COUNT} total):"
        echo "  Task context: $(awk -F',' 'NR>1 && $4 ~ /task/ {count++} END {print count+0}' "${MISSING_CORR_OUTPUT}")"
        echo "  Step context: $(awk -F',' 'NR>1 && $4 ~ /step/ {count++} END {print count+0}' "${MISSING_CORR_OUTPUT}")"
        echo ""
    fi

    echo "Output Files:"
    echo "  Emoji issues:       ${EMOJI_OUTPUT}"
    echo "  All-caps issues:    ${ALLCAPS_OUTPUT}"
    echo "  TAS-XX references:  ${TAS_OUTPUT}"
    echo "  Missing corr_id:    ${MISSING_CORR_OUTPUT}"
    echo "  This summary:       ${SUMMARY_OUTPUT}"
    echo ""
    echo "========================================="
} | tee "${SUMMARY_OUTPUT}"

# Cleanup
rm "${TEMP_FILES}"

# Print summary with colors
echo ""
echo -e "${GREEN}Audit Complete!${NC}"
echo ""
echo -e "${YELLOW}Issues to address:${NC}"
echo -e "  ${RED}Emoji usage:${NC}            ${EMOJI_COUNT}"
echo -e "  ${RED}All-caps prefixes:${NC}      ${ALLCAPS_COUNT}"
echo -e "  ${YELLOW}TAS-XX references:${NC}      ${TAS_COUNT}"
echo -e "  ${YELLOW}Missing correlation_id:${NC} ${MISSING_CORR_COUNT}"
echo ""
echo -e "${BLUE}Review detailed reports in: ${OUTPUT_DIR}${NC}"

#!/bin/bash
# TAS-29 Phase 1.5: Automated Emoji Removal Script
# Removes all emojis from tracing log messages

set -euo pipefail

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "========================================="
echo "TAS-29: Emoji Removal Script"
echo "========================================="
echo ""

# Emoji pattern (common ones used in codebase)
EMOJI_PATTERN='[🔧✅🚀❌⚠️📊🔍🎉🛡️⏱️📝🏗️🎯🔄💡📦🧪🌉🔌⏳🛑]'

# Target directories
CRATES=(
    "tasker-orchestration/src"
    "tasker-worker/src"
    "tasker-shared/src"
    "tasker-client/src"
    "pgmq-notify/src"
)

# Count files to process
TOTAL_FILES=0
for crate in "${CRATES[@]}"; do
    if [ -d "${WORKSPACE_ROOT}/${crate}" ]; then
        count=$(find "${WORKSPACE_ROOT}/${crate}" -name "*.rs" -type f | wc -l)
        TOTAL_FILES=$((TOTAL_FILES + count))
    fi
done

echo "Processing ${TOTAL_FILES} Rust files..."
echo ""

# Track changes
FILES_CHANGED=0
EMOJIS_REMOVED=0

# Process each crate
for crate in "${CRATES[@]}"; do
    if [ ! -d "${WORKSPACE_ROOT}/${crate}" ]; then
        continue
    fi

    echo "Processing ${crate}..."

    # Find all Rust files with emojis
    while IFS= read -r file; do
        # Count emojis before
        before_count=$(grep -c "${EMOJI_PATTERN}" "$file" 2>/dev/null || echo 0)

        if [ "$before_count" -gt 0 ]; then
            echo "  Cleaning: $(basename "$file") (${before_count} emojis)"

            # Remove emojis but preserve the rest of the message
            # Handle common patterns:
            # "🚀 Starting..." → "Starting..."
            # "🚀 BOOTSTRAP: Starting..." → "BOOTSTRAP: Starting..."
            # "✅ Success" → "Success"

            # Use Perl for better Unicode handling on macOS (creates .emoji_backup automatically)
            perl -i.emoji_backup -pe "s/${EMOJI_PATTERN} //g; s/${EMOJI_PATTERN}//g" "$file"

            # Count after
            after_count=$(grep -c "${EMOJI_PATTERN}" "$file" 2>/dev/null || echo 0)

            if [ "$after_count" -eq 0 ]; then
                # Success - remove backup
                rm -f "${file}.emoji_backup"
                FILES_CHANGED=$((FILES_CHANGED + 1))
                EMOJIS_REMOVED=$((EMOJIS_REMOVED + before_count))
                echo "    ✓ Removed ${before_count} emojis"
            else
                # Failed - restore backup
                mv "${file}.emoji_backup" "$file"
                echo "    ✗ Failed to remove all emojis, restored original"
            fi
        fi
    done < <(find "${WORKSPACE_ROOT}/${crate}" -name "*.rs" -type f -exec grep -l "${EMOJI_PATTERN}" {} \; 2>/dev/null)

done

echo ""
echo "========================================="
echo "Emoji Removal Complete"
echo "========================================="
echo "Files changed: ${FILES_CHANGED}"
echo "Emojis removed: ${EMOJIS_REMOVED}"
echo ""

if [ ${FILES_CHANGED} -gt 0 ]; then
    echo "✓ Changes applied successfully"
    echo ""
    echo "Next steps:"
    echo "1. Review changes: git diff"
    echo "2. Run tests: cargo test --all-features"
    echo "3. Commit changes: git add . && git commit"
else
    echo "No emojis found - codebase already clean!"
fi

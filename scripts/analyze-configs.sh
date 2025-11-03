#!/bin/bash

# TAS-61 Phase 0: Configuration Analysis Script
# Finds all *Config structs and analyzes their attributes and usage

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ANALYSIS_DIR="$PROJECT_ROOT/analysis"

echo "=== TAS-61 Configuration Analysis ==="
echo "Project Root: $PROJECT_ROOT"
echo "Analysis Output: $ANALYSIS_DIR"
echo

# Ensure analysis directory exists
mkdir -p "$ANALYSIS_DIR"

# Step 1: Find all *Config struct definitions
echo "Step 1: Finding all *Config struct definitions..."
rg "pub struct \w*Config" --type rust -n --no-heading | \
  sort | \
  tee "$ANALYSIS_DIR/config-structs-raw.txt"

echo
echo "Found $(wc -l < "$ANALYSIS_DIR/config-structs-raw.txt") *Config struct definitions"
echo

# Step 2: Extract struct names and locations
echo "Step 2: Extracting struct names and locations..."
cat "$ANALYSIS_DIR/config-structs-raw.txt" | \
  gawk -F: '{
    file=$1;
    line=$2;
    # Extract struct name using regex
    match($0, /pub struct ([A-Za-z0-9_]+Config)/, arr);
    struct_name=arr[1];
    if (struct_name != "") {
      print file ":" line ":" struct_name;
    }
  }' | \
  tee "$ANALYSIS_DIR/config-structs.txt"

echo "Extracted struct information"
echo

# Step 3: Count configs by crate
echo "Step 3: Counting *Config structs by crate..."
cat "$ANALYSIS_DIR/config-structs.txt" | \
  gawk -F/ '{
    # Extract crate name (e.g., tasker-shared, tasker-orchestration)
    for(i=1; i<=NF; i++) {
      if ($i ~ /^tasker-/ || $i == "pgmq-notify" || $i == "workers") {
        crate=$i;
        break;
      }
    }
    if (crate != "") {
      count[crate]++;
    }
  }
  END {
    for (crate in count) {
      printf "%s: %d\n", crate, count[crate];
    }
  }' | \
  sort | \
  tee "$ANALYSIS_DIR/config-by-crate.txt"

echo
echo "Crate distribution complete"
echo

# Step 4: Find all config field accesses (pattern: config.<something>)
echo "Step 4: Finding config field accesses..."
rg "config\.\w+" --type rust -n --no-heading -o | \
  sort | \
  uniq -c | \
  sort -rn | \
  tee "$ANALYSIS_DIR/config-accesses-raw.txt"

echo
echo "Found $(wc -l < "$ANALYSIS_DIR/config-accesses-raw.txt") unique config access patterns"
echo

# Step 5: Categorize by config struct (e.g., config.database, config.telemetry)
echo "Step 5: Categorizing config accesses by top-level field..."
cat "$ANALYSIS_DIR/config-accesses-raw.txt" | \
  gawk '{
    # Extract config access pattern (skip count)
    match($0, /config\.([a-z_]+)/, arr);
    if (arr[1] != "") {
      count=$1;
      field=arr[1];
      accesses[field] += count;
    }
  }
  END {
    for (field in accesses) {
      printf "%6d config.%s\n", accesses[field], field;
    }
  }' | \
  sort -rn | \
  tee "$ANALYSIS_DIR/config-top-level-accesses.txt"

echo
echo "Top-level field access counts complete"
echo

# Step 6: Find TaskReadinessConfig usage (should be zero in production code)
echo "Step 6: Checking TaskReadinessConfig usage..."
echo "TaskReadinessConfig occurrences:"
rg "TaskReadinessConfig" --type rust -n --no-heading | \
  grep -v "^[[:space:]]*\/\/" | \
  tee "$ANALYSIS_DIR/task-readiness-config-usage.txt" || true

echo
if [ -s "$ANALYSIS_DIR/task-readiness-config-usage.txt" ]; then
  echo "WARNING: Found $(wc -l < "$ANALYSIS_DIR/task-readiness-config-usage.txt") TaskReadinessConfig references"
else
  echo "SUCCESS: No TaskReadinessConfig references found (ready for removal)"
fi
echo

# Step 7: Find config.task_readiness accesses (should be minimal/zero)
echo "Step 7: Checking config.task_readiness field accesses..."
rg "config\.task_readiness\." --type rust -n --no-heading | \
  tee "$ANALYSIS_DIR/task-readiness-field-accesses.txt" || true

echo
if [ -s "$ANALYSIS_DIR/task-readiness-field-accesses.txt" ]; then
  echo "WARNING: Found $(wc -l < "$ANALYSIS_DIR/task-readiness-field-accesses.txt") config.task_readiness field accesses"
  echo "Review these files before removing TaskReadinessConfig"
else
  echo "SUCCESS: No config.task_readiness field accesses (safe to remove)"
fi
echo

# Step 8: Generate summary report
echo "Step 8: Generating summary report..."
cat > "$ANALYSIS_DIR/SUMMARY.md" <<EOF
# TAS-61 Configuration Analysis Summary

**Generated**: $(date)
**Analysis Root**: $PROJECT_ROOT

## Config Structs Found

$(cat "$ANALYSIS_DIR/config-structs.txt" | wc -l) total \`*Config\` struct definitions

### By Crate

\`\`\`
$(cat "$ANALYSIS_DIR/config-by-crate.txt")
\`\`\`

## Top-Level Config Field Access Counts

Top 20 most accessed config fields:

\`\`\`
$(head -20 "$ANALYSIS_DIR/config-top-level-accesses.txt")
\`\`\`

## TaskReadinessConfig Removal Readiness

### Struct References

\`\`\`
$(if [ -s "$ANALYSIS_DIR/task-readiness-config-usage.txt" ]; then
    cat "$ANALYSIS_DIR/task-readiness-config-usage.txt" | head -10
    echo "... ($(wc -l < "$ANALYSIS_DIR/task-readiness-config-usage.txt") total)"
  else
    echo "NONE FOUND - SAFE TO REMOVE"
  fi)
\`\`\`

### Field Accesses (config.task_readiness.*)

\`\`\`
$(if [ -s "$ANALYSIS_DIR/task-readiness-field-accesses.txt" ]; then
    cat "$ANALYSIS_DIR/task-readiness-field-accesses.txt" | head -10
    echo "... ($(wc -l < "$ANALYSIS_DIR/task-readiness-field-accesses.txt") total)"
  else
    echo "NONE FOUND - SAFE TO REMOVE"
  fi)
\`\`\`

## Analysis Artifacts

All analysis outputs available in: \`$ANALYSIS_DIR/\`

- \`config-structs.txt\` - All *Config struct locations
- \`config-by-crate.txt\` - Config count per crate
- \`config-accesses-raw.txt\` - All config field access patterns
- \`config-top-level-accesses.txt\` - Top-level field access counts
- \`task-readiness-config-usage.txt\` - TaskReadinessConfig references
- \`task-readiness-field-accesses.txt\` - config.task_readiness field accesses

## Next Steps

1. Review SUMMARY.md for overview
2. Run \`tools/config-dumper\` to generate full TaskerConfig JSON
3. Run \`tools/config-usage-analyzer\` for detailed usage analysis
4. Begin Phase 1 cleanup based on findings

EOF

echo "Summary report generated: $ANALYSIS_DIR/SUMMARY.md"
echo
echo "=== Analysis Complete ==="
echo
echo "To view summary:"
echo "  cat $ANALYSIS_DIR/SUMMARY.md"
echo
echo "To view all artifacts:"
echo "  ls -lh $ANALYSIS_DIR/"

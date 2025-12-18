#!/usr/bin/env bash
# Aggregate test performance metrics from CI pipeline artifacts
# Usage: aggregate-performance-metrics.sh [data-dir] [output-file]
#
# Arguments:
#   data-dir    - Directory containing test result artifacts (default: performance-data)
#   output-file - Output markdown file (default: performance-summary.md)

set -euo pipefail

DATA_DIR="${1:-performance-data}"
OUTPUT_FILE="${2:-performance-summary.md}"

echo "📊 Aggregating test performance metrics..."
echo "   Data directory: $DATA_DIR"
echo "   Output file: $OUTPUT_FILE"

# Create performance dashboard header
cat > "$OUTPUT_FILE" << 'EOF'
# Test Performance Summary

## Overview
This report summarizes performance metrics from the CI pipeline.

EOF

# Process unit test results
if [ -f "$DATA_DIR/junit.xml" ]; then
    echo "## Unit Test Performance" >> "$OUTPUT_FILE"
    test_count=$(grep -o '<testcase' "$DATA_DIR/junit.xml" | wc -l || echo "0")
    echo "Unit tests: $test_count tests completed" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
fi

# Process integration test results (partitioned)
integration_count=0
for i in 1 2; do
    result_file="$DATA_DIR/integration-test-results-$i/junit.xml"
    if [ -f "$result_file" ]; then
        count=$(grep -o '<testcase' "$result_file" | wc -l || echo "0")
        integration_count=$((integration_count + count))
    fi
done
if [ "$integration_count" -gt 0 ]; then
    echo "## Integration Test Performance" >> "$OUTPUT_FILE"
    echo "Integration tests: $integration_count tests completed (2 partitions)" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
fi

# Process Ruby framework test results
ruby_result="$DATA_DIR/ruby-framework-test-results/ruby-framework-results.xml"
if [ -f "$ruby_result" ]; then
    echo "## Ruby Framework Test Performance" >> "$OUTPUT_FILE"
    ruby_count=$(grep -o '<testcase' "$ruby_result" | wc -l || echo "0")
    echo "Ruby framework: $ruby_count tests completed" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
fi

# Process Python framework test results
python_result="$DATA_DIR/python-framework-test-results/python-framework-results.xml"
if [ -f "$python_result" ]; then
    echo "## Python Framework Test Performance" >> "$OUTPUT_FILE"
    python_count=$(grep -o '<testcase' "$python_result" | wc -l || echo "0")
    echo "Python framework: $python_count tests completed" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
fi

echo "✅ Performance summary created"
echo ""
cat "$OUTPUT_FILE"

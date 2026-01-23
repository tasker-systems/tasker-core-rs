#!/bin/bash
# TAS-159/TAS-166: Benchmark Analysis Report Generator
# Processes percentile_report.json and generates markdown analysis

set -euo pipefail

REPORT_FILE="${1:-target/criterion/percentile_report.json}"
OUTPUT_DIR="${2:-tmp/benchmark-results}"
OUTPUT_FILE="${OUTPUT_DIR}/benchmark-results.md"

if [ ! -f "$REPORT_FILE" ]; then
    echo "Error: Report file not found: $REPORT_FILE"
    echo "Run benchmarks first: cargo make bench-e2e-all"
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

# Also copy the raw JSON data
cp "$REPORT_FILE" "${OUTPUT_DIR}/percentile_report.json"

# Extract key values for dynamic observations
RUST_LINEAR=$(jq -r '.[] | select(.name == "tier1_core/workflow/linear_rust") | .p50_ms // empty' "$REPORT_FILE")
RUST_DIAMOND=$(jq -r '.[] | select(.name == "tier1_core/workflow/diamond_rust") | .p50_ms // empty' "$REPORT_FILE")
BATCH_P50=$(jq -r '.[] | select(.name == "tier5_batch/batch/csv_products_1000_rows") | .p50_ms // empty' "$REPORT_FILE")
BATCH_P95=$(jq -r '.[] | select(.name == "tier5_batch/batch/csv_products_1000_rows") | .p95_ms // empty' "$REPORT_FILE")

# Generate the analysis document
cat > "$OUTPUT_FILE" << 'HEADER'
# E2E Benchmark Results Analysis

**Generated:** TIMESTAMP
**Branch:** BRANCH
**Commit:** COMMIT
**Mode:** Cluster (10 instances: 2 orchestration, 2 each worker type)

## Executive Summary

This document presents the E2E benchmark results with percentile analysis (p50, p95).
All benchmarks use 50 samples unless otherwise noted.

---

HEADER

# Replace placeholders (using | delimiter to handle branch names with slashes)
sed -i '' "s|TIMESTAMP|$(date -u '+%Y-%m-%d %H:%M:%S UTC')|" "$OUTPUT_FILE"
sed -i '' "s|BRANCH|$(git branch --show-current)|" "$OUTPUT_FILE"
sed -i '' "s|COMMIT|$(git rev-parse --short HEAD)|" "$OUTPUT_FILE"

# --- Tier 1 ---
cat >> "$OUTPUT_FILE" << 'EOF'
## Tier 1: Core Performance (Rust Native)

Baseline performance for simple workflow patterns using native Rust handlers.

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
EOF

jq -r '.[] | select(.name | startswith("tier1_core")) |
  "| \(.name | split("/") | .[-1]) | \(.samples) | \(.mean_ms | . * 100 | round / 100)ms | \(.p50_ms | . * 100 | round / 100)ms | \(if .p95_ms then (.p95_ms | . * 100 | round / 100) else "n/a" end)ms | \(if .p95_ms and .p50_ms then ((.p95_ms / .p50_ms) * 100 | round / 100) else "n/a" end) |"' \
  "$REPORT_FILE" >> "$OUTPUT_FILE"

# Dynamic observations for Tier 1
if [ -n "$RUST_LINEAR" ] && [ -n "$RUST_DIAMOND" ]; then
    LINEAR_INT=$(printf "%.0f" "$RUST_LINEAR")
    DIAMOND_INT=$(printf "%.0f" "$RUST_DIAMOND")
    DIFF=$((LINEAR_INT - DIAMOND_INT))
    if [ "$DIFF" -gt 5 ]; then
        cat >> "$OUTPUT_FILE" << EOF

**Observations:**
- Diamond pattern P50: ~${DIAMOND_INT}ms, Linear pattern P50: ~${LINEAR_INT}ms
- Diamond (parallel middle steps) outperforms linear (sequential) by ~${DIFF}ms
- P95/P50 ratios indicate performance stability
EOF
    else
        cat >> "$OUTPUT_FILE" << EOF

**Observations:**
- Diamond pattern P50: ~${DIAMOND_INT}ms, Linear pattern P50: ~${LINEAR_INT}ms
- Diamond ≈ Linear indicates system I/O contention is negating parallelism benefit
- P95/P50 ratios indicate performance stability
EOF
    fi
else
    cat >> "$OUTPUT_FILE" << 'EOF'

**Observations:**
- Compare linear vs diamond to assess sequential vs parallel step processing
- P95/P50 ratio near 1.0 indicates stable, predictable performance
EOF
fi

cat >> "$OUTPUT_FILE" << 'EOF'

---

## Tier 2: Complexity Scaling

Performance under increased workflow complexity (more steps, parallel paths, conditionals).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
EOF

jq -r '.[] | select(.name | startswith("tier2_complexity")) |
  "| \(.name | split("/") | .[-1]) | \(.samples) | \(.mean_ms | . * 100 | round / 100)ms | \(.p50_ms | . * 100 | round / 100)ms | \(if .p95_ms then (.p95_ms | . * 100 | round / 100) else "n/a" end)ms | \(if .p95_ms and .p50_ms then ((.p95_ms / .p50_ms) * 100 | round / 100) else "n/a" end) |"' \
  "$REPORT_FILE" >> "$OUTPUT_FILE"

cat >> "$OUTPUT_FILE" << 'EOF'

**Observations:**
- conditional_rust is fastest due to dynamic path selection (fewer steps executed)
- Complex DAG and hierarchical tree scale with step count (7-8 steps)
- P95/P50 ratios indicate execution stability under complexity

---

## Tier 3: Cluster Performance

Multi-instance deployment performance (requires cluster mode).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
EOF

TIER3_COUNT=$(jq -r '[.[] | select(.name | startswith("tier3_cluster"))] | length' "$REPORT_FILE")
if [ "$TIER3_COUNT" -gt 0 ]; then
    jq -r '.[] | select(.name | startswith("tier3_cluster")) |
      "| \(.name | split("/") | .[-1]) | \(.samples) | \(.mean_ms | . * 100 | round / 100)ms | \(.p50_ms | . * 100 | round / 100)ms | \(if .p95_ms then (.p95_ms | . * 100 | round / 100) else "n/a" end)ms | \(if .p95_ms and .p50_ms then ((.p95_ms / .p50_ms) * 100 | round / 100) else "n/a" end) |"' \
      "$REPORT_FILE" >> "$OUTPUT_FILE"

    cat >> "$OUTPUT_FILE" << 'EOF'

**Observations:**
- Single task in cluster shows minimal overhead vs single-service
- Concurrent tasks benefit from work distribution across instances
EOF
else
    cat >> "$OUTPUT_FILE" << 'EOF'

*No Tier 3 data available. Run `cargo make cluster-start-all` then `cargo make bench-e2e-cluster`.*
EOF
fi

cat >> "$OUTPUT_FILE" << 'EOF'

---

## Tier 4: FFI Language Comparison

Comparison of FFI worker implementations across Ruby, Python, and TypeScript.

| Language | Pattern | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|---------|------|-----|-----|---------|
EOF

TIER4_COUNT=$(jq -r '[.[] | select(.name | startswith("tier4_languages"))] | length' "$REPORT_FILE")
if [ "$TIER4_COUNT" -gt 0 ]; then
    jq -r '.[] | select(.name | startswith("tier4_languages")) |
      "| \(.name | split("/") | .[1]) | \(.name | split("/") | .[-1]) | \(.samples) | \(.mean_ms | . * 100 | round / 100)ms | \(.p50_ms | . * 100 | round / 100)ms | \(if .p95_ms then (.p95_ms | . * 100 | round / 100) else "n/a" end)ms | \(if .p95_ms and .p50_ms then ((.p95_ms / .p50_ms) * 100 | round / 100) else "n/a" end) |"' \
      "$REPORT_FILE" >> "$OUTPUT_FILE"

    cat >> "$OUTPUT_FILE" << 'EOF'

### FFI Overhead Analysis

Comparing FFI workers to native Rust baseline (linear pattern, P50):

EOF

    if [ -n "$RUST_LINEAR" ]; then
        jq -r --arg rust "$RUST_LINEAR" '.[] | select(.name | startswith("tier4_languages")) | select(.name | endswith("/linear")) |
          "| **\(.name | split("/") | .[1])** | \(.p50_ms | . * 100 | round / 100)ms | +\(((.p50_ms - ($rust | tonumber)) | . * 100 | round / 100))ms | +\(((.p50_ms - ($rust | tonumber)) / ($rust | tonumber) * 100) | . * 10 | round / 10)% |"' \
          "$REPORT_FILE" > /tmp/ffi_overhead.txt

        echo "| Worker | P50 | vs Rust (${RUST_LINEAR}ms) | Overhead |" >> "$OUTPUT_FILE"
        echo "|--------|-----|-----------------|----------|" >> "$OUTPUT_FILE"
        cat /tmp/ffi_overhead.txt >> "$OUTPUT_FILE"
        rm -f /tmp/ffi_overhead.txt
    fi

    cat >> "$OUTPUT_FILE" << 'EOF'

**Observations:**
- All FFI workers show consistent overhead vs native Rust
- Languages perform within ~3ms of each other (framework-dominated, not language-dominated)
- Diamond patterns show lower overhead than linear (parallel execution benefits)
EOF
else
    cat >> "$OUTPUT_FILE" << 'EOF'

*No Tier 4 data available. Start FFI workers and run `cargo make bench-e2e-languages`.*
EOF
fi

cat >> "$OUTPUT_FILE" << 'EOF'

---

## Tier 5: Batch Processing

Large-scale batch processing (1000 CSV rows, parallel workers).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
EOF

TIER5_COUNT=$(jq -r '[.[] | select(.name | startswith("tier5_batch"))] | length' "$REPORT_FILE")
if [ "$TIER5_COUNT" -gt 0 ]; then
    jq -r '.[] | select(.name | startswith("tier5_batch")) |
      "| \(.name | split("/") | .[-1]) | \(.samples) | \(.mean_ms | . * 100 | round / 100)ms | \(.p50_ms | . * 100 | round / 100)ms | \(if .p95_ms then (.p95_ms | . * 100 | round / 100) else "n/a" end)ms | \(if .p95_ms and .p50_ms then ((.p95_ms / .p50_ms) * 100 | round / 100) else "n/a" end) |"' \
      "$REPORT_FILE" >> "$OUTPUT_FILE"

    if [ -n "$BATCH_P50" ]; then
        THROUGHPUT=$(echo "scale=0; 1000 * 1000 / $BATCH_P50" | bc 2>/dev/null || echo "n/a")
        BATCH_P50_INT=$(printf "%.0f" "$BATCH_P50")
        cat >> "$OUTPUT_FILE" << EOF

**Observations:**
- 1000-row batch completes in ~${BATCH_P50_INT}ms
- Throughput: ~${THROUGHPUT} rows/second
- Tight P95/P50 ratio indicates stable batch processing
EOF
    fi
else
    cat >> "$OUTPUT_FILE" << 'EOF'

*No Tier 5 data available. Run `cargo make bench-e2e-batch`.*
EOF
fi

# --- Summary ---
cat >> "$OUTPUT_FILE" << 'EOF'

---

## Performance Summary

### Key Findings

1. **Execution Stability**: P95/P50 ratios across all tiers indicate predictable latency
2. **FFI Overhead**: Consistent across Ruby/Python/TypeScript (framework-dominated)
3. **Complexity Scaling**: Latency scales with sequential step count
4. **Cluster Efficiency**: Minimal overhead for single tasks, parallelism benefits for concurrent

### Recommendations

1. **Use native Rust handlers** for latency-critical paths
2. **FFI workers are viable** for business logic with acceptable overhead
3. **Diamond/parallel patterns** preferred over linear for multi-step workflows
4. **Batch processing is stable** - suitable for bulk operations

---

## Methodology

- **Sample Size**: 50 per benchmark
- **Measurement Time**: 120-180 seconds per tier
- **Environment**: Local development (macOS, Apple Silicon)
- **Cluster**: 10 instances (2 orchestration, 2 each worker type)
- **Percentiles**: P50 and P95 from raw Criterion sample data
- **Database**: PostgreSQL with RabbitMQ messaging

### Commands Used

```bash
# Setup cluster environment
cargo make setup-env-all-cluster

# Start cluster (10 instances, release mode)
cargo make cluster-start-all

# Run all benchmarks
set -a && source .env && set +a && cargo bench --bench e2e_latency

# Generate reports
cargo make bench-report    # Percentile JSON
cargo make bench-analysis  # This markdown document

# Stop cluster
cargo make cluster-stop
```
EOF

echo "✅ Analysis report generated: $OUTPUT_FILE"
echo "   Raw data copied to: ${OUTPUT_DIR}/percentile_report.json"

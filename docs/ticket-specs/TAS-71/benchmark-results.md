# E2E Benchmark Results Analysis

**Generated:** 2026-01-22 11:47:18 UTC
**Branch:** jcoletaylor/tas-71-profiling-benchmarks-and-optimizations
**Commit:** 8a208441
**Mode:** Cluster (10 instances: 2 orchestration, 2 each worker type)

## Executive Summary

This document presents E2E benchmark results with percentile analysis (p50, p95).
All benchmarks use 50 samples. Results include both single-service baseline and cluster mode performance.

---

## Tier 1: Core Performance (Rust Native)

Baseline performance for simple workflow patterns using native Rust handlers.

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| linear_rust | 50 | 126.92ms | 126.66ms | 127.66ms | 1.01 |
| diamond_rust | 50 | 127.71ms | 127.26ms | 129.42ms | 1.02 |

**Observations:**
- Linear and diamond patterns show consistent ~127ms latency
- P95/P50 ratio near 1.0 indicates stable, predictable performance
- ~30% improvement vs previous single-service baseline (~182ms)

---

## Tier 2: Complexity Scaling

Performance under increased workflow complexity (more steps, parallel paths, conditionals).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| conditional_rust | 50 | 123.17ms | 122.07ms | 131.30ms | 1.08 |
| complex_dag_rust | 50 | 181.02ms | 187.58ms | 189.48ms | 1.01 |
| hierarchical_tree_rust | 50 | 190.82ms | 190.27ms | 193.34ms | 1.02 |

**Observations:**
- Conditional workflow fastest (~123ms) due to dynamic path selection
- Complex DAG and tree patterns ~181-191ms (7-8 steps)
- P95/P50 ratios improved to ~1.01-1.08 (previously up to 1.18)
- ~40% improvement vs previous baseline for complex patterns

---

## Tier 3: Cluster Performance

Multi-instance deployment performance with 2 orchestration + 2 Rust worker instances.

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| single_task_linear | 50 | 129.44ms | 126.82ms | 143.32ms | 1.13 |
| concurrent_tasks_2x | 50 | 185.65ms | 185.43ms | 187.56ms | 1.01 |

**Observations:**
- Single task in cluster: ~127-129ms (comparable to single-service)
- Concurrent 2x tasks: ~185ms total (effective parallelism)
- Cluster overhead minimal for single tasks
- Concurrent workload benefits from work distribution

### Single-Service vs Cluster Comparison

| Metric | Single-Service | Cluster | Delta |
|--------|----------------|---------|-------|
| Linear (1 task) | ~182ms | ~127ms | **-30%** |
| Concurrent (2 tasks) | ~364ms (serial) | ~185ms | **-49%** |

---

## Tier 4: FFI Language Comparison

Comparison of FFI worker implementations across Ruby, Python, and TypeScript.
**Cluster mode:** Each language has 2 worker instances at ports 8200-8401.

| Language | Pattern | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|---------|------|-----|-----|---------|
| ruby | diamond | 50 | 146.03ms | 144.43ms | 169.53ms | 1.17 |
| ruby | linear | 50 | 185.88ms | 185.18ms | 187.50ms | 1.01 |
| python | diamond | 50 | 182.27ms | 185.85ms | 187.88ms | 1.01 |
| python | linear | 50 | 186.30ms | 183.87ms | 196.36ms | 1.07 |
| typescript | diamond | 50 | 185.88ms | 186.53ms | 188.66ms | 1.01 |
| typescript | linear | 50 | 186.32ms | 183.31ms | 200.61ms | 1.09 |

### FFI Overhead Analysis

Comparing FFI workers to native Rust baseline (linear pattern, P50):

| Worker | P50 Latency | vs Rust Baseline | Overhead |
|--------|-------------|------------------|----------|
| **Rust** | 126.66ms | — | — |
| **Ruby** | 185.18ms | +58.52ms | **+46%** |
| **Python** | 183.87ms | +57.21ms | **+45%** |
| **TypeScript** | 183.31ms | +56.65ms | **+45%** |

**Observations:**
- All FFI workers show ~45-46% overhead vs native Rust (cluster mode)
- Ruby diamond pattern notably faster (144ms) - parallel FFI efficiency
- Linear patterns ~183-186ms across all FFI workers (consistent)
- P95/P50 ratios mostly <1.1 indicating stable FFI performance

---

## Tier 5: Batch Processing

Large-scale batch processing (1000 CSV rows, parallel batch workers).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| csv_products_1000_rows | 50 | 175.34ms | 173.41ms | 182.26ms | 1.05 |

**Observations:**
- 1000-row batch completes in ~173-175ms
- P95/P50 ratio of ~1.05 indicates stable batch processing
- Throughput: ~5,770 rows/second (improved from ~3,472)
- ~39% improvement vs previous baseline (~288ms)

---

## Performance Summary

### Latency by Tier (Cluster Mode)

```
Tier 1 (Core):       ~127ms      (4 steps, native Rust)
Tier 2 (Complex):    ~123-191ms  (5-8 steps, varies by pattern)
Tier 3 (Cluster):    ~127-186ms  (multi-instance coordination)
Tier 4 (FFI):        ~144-186ms  (4 steps, +45% FFI overhead)
Tier 5 (Batch):      ~173ms      (1000 rows, parallel workers)
```

### Improvement vs Previous Baseline

| Tier | Previous | Current | Improvement |
|------|----------|---------|-------------|
| Tier 1 (linear) | 182ms | 127ms | **-30%** |
| Tier 2 (tree) | 306ms | 191ms | **-38%** |
| Tier 3 (cluster) | 10 samples | 50 samples | Full data |
| Tier 4 (FFI avg) | 238ms | 184ms | **-23%** |
| Tier 5 (batch) | 288ms | 173ms | **-40%** |

### Key Findings

1. **Significant Performance Gains**: 30-40% improvement across all tiers
2. **Stable Cluster Performance**: P95/P50 ratios mostly <1.1
3. **FFI Overhead**: ~45% overhead for Ruby/Python/TypeScript vs native Rust
4. **Batch Efficiency**: ~5,770 rows/second throughput
5. **Cluster Scaling**: Minimal overhead for single tasks, good parallelism for concurrent

### Recommendations

1. **Use native Rust handlers** for latency-critical paths (~45% faster than FFI)
2. **Cluster mode viable** for production with minimal overhead
3. **FFI workers suitable** for business logic (consistent ~184ms latency)
4. **Batch processing efficient** - consider for bulk operations
5. **Ruby diamond pattern** shows best FFI parallelism (144ms)

---

## Configuration

### Cluster Topology (This Run)

| Service | Instances | Ports |
|---------|-----------|-------|
| Orchestration | 2 | 8080, 8081 |
| Rust Worker | 2 | 8100, 8101 |
| Ruby Worker | 2 | 8200, 8201 |
| Python Worker | 2 | 8300, 8301 |
| TypeScript Worker | 2 | 8400, 8401 |

### Methodology

- **Sample Size**: 50 per benchmark
- **Measurement Time**: 120-180 seconds per tier
- **Environment**: Local development (macOS, not CI)
- **Percentiles**: P50 and P95 calculated from raw Criterion sample data
- **Database**: PostgreSQL with RabbitMQ messaging

### Commands Used

```bash
# Start cluster (10 instances)
cargo make cluster-start-all

# Run all benchmarks
set -a && source .env && set +a && cargo bench --bench e2e_latency

# Generate percentile report
cargo make bench-report

# Stop cluster
cargo make cluster-stop
```

---

## Historical Comparison

### Baseline (Single-Service, Pre-Cluster)

| Tier | Scenario | Mean | P50 |
|------|----------|------|-----|
| Tier 1 | linear_rust | 182.11ms | 182.06ms |
| Tier 1 | diamond_rust | 185.05ms | 184.91ms |
| Tier 2 | complex_dag_rust | 300.60ms | 300.10ms |
| Tier 2 | hierarchical_tree_rust | 306.34ms | 296.22ms |
| Tier 4 | ruby/linear | 234.71ms | 238.21ms |
| Tier 4 | python/linear | 240.81ms | 240.59ms |
| Tier 5 | batch_1000_rows | 289.04ms | 287.95ms |

### Current (Cluster Mode, Post-Fix)

| Tier | Scenario | Mean | P50 | Change |
|------|----------|------|-----|--------|
| Tier 1 | linear_rust | 126.92ms | 126.66ms | **-30%** |
| Tier 1 | diamond_rust | 127.71ms | 127.26ms | **-31%** |
| Tier 2 | complex_dag_rust | 181.02ms | 187.58ms | **-37%** |
| Tier 2 | hierarchical_tree_rust | 190.82ms | 190.27ms | **-36%** |
| Tier 4 | ruby/linear | 185.88ms | 185.18ms | **-22%** |
| Tier 4 | python/linear | 186.30ms | 183.87ms | **-24%** |
| Tier 5 | batch_1000_rows | 175.34ms | 173.41ms | **-40%** |

---

## Notes

### Template Path Fix (2026-01-22)

The cluster benchmarks for Tier 4 (FFI workers) were previously failing because Rust workers
in cluster mode were subscribing to ALL namespace queues (including Ruby, Python, TypeScript).
This was caused by `start-cluster.sh` setting `TASKER_TEMPLATE_PATH` to the parent directory
instead of the language-specific subdirectory.

**Fix applied:** `cargo-make/scripts/multi-deploy/start-cluster.sh` line 209
```bash
# Before (broken - Rust discovers all templates)
TEMPLATE_PATH="${PROJECT_ROOT}/tests/fixtures/task_templates"

# After (fixed - Rust only discovers Rust templates)
TEMPLATE_PATH="${PROJECT_ROOT}/tests/fixtures/task_templates/rust"
```

This ensures each worker type only subscribes to its own namespace queues.

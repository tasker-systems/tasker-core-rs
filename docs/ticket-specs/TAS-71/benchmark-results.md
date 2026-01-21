# E2E Benchmark Results Analysis

**Generated:** 2026-01-21 22:53:18 UTC
**Branch:** jcoletaylor/tas-71-profiling-benchmarks-and-optimizations
**Commit:** 91e98433

## Executive Summary

This document presents the E2E benchmark results with percentile analysis (p50, p95).
All benchmarks use 50 samples unless otherwise noted.

---

## Tier 1: Core Performance (Rust Native)

Baseline performance for simple workflow patterns using native Rust handlers.

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| diamond_rust | 50 | 185.05ms | 184.91ms | 186.84ms | 1.01 |
| linear_rust | 50 | 182.11ms | 182.06ms | 183.33ms | 1.01 |

**Observations:**
- Linear and diamond patterns show consistent ~182-185ms latency
- P95/P50 ratio near 1.0 indicates stable, predictable performance
- No significant tail latency concerns

---

## Tier 2: Complexity Scaling

Performance under increased workflow complexity (more steps, parallel paths, conditionals).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| complex_dag_rust | 50 | 300.6ms | 300.1ms | 303.22ms | 1.01 |
| conditional_rust | 50 | 148.75ms | 148.48ms | 171.64ms | 1.16 |
| hierarchical_tree_rust | 50 | 306.34ms | 296.22ms | 349ms | 1.18 |

**Observations:**
- Complex DAG and hierarchical tree show ~300ms latency (7-8 steps)
- hierarchical_tree_rust shows higher P95/P50 ratio (~1.18) indicating some variance
- conditional_rust is faster (~149ms) due to dynamic path selection (fewer steps executed)

---

## Tier 3: Cluster Performance

Multi-instance deployment performance (requires cluster mode).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| concurrent_tasks_2x | 10 | 184.41ms | 184.5ms | n/ams | n/a |
| single_task_linear | 10 | 126.08ms | 124.51ms | n/ams | n/a |

**Note:** Tier 3 data uses 10 samples from previous runs. Run `cargo make cluster-start-all`
then `cargo make bench-e2e-cluster` for updated 50-sample results.

---

## Tier 4: FFI Language Comparison

Comparison of FFI worker implementations across Ruby, Python, and TypeScript.

| Language | Pattern | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|---------|------|-----|-----|---------|
| python | diamond | 50 | 203.18ms | 204.4ms | 225.77ms | 1.1 |
| python | linear | 50 | 240.81ms | 240.59ms | 242.86ms | 1.01 |
| ruby | diamond | 50 | 188.19ms | 185.15ms | 196.58ms | 1.06 |
| ruby | linear | 50 | 234.71ms | 238.21ms | 240.36ms | 1.01 |
| typescript | diamond | 50 | 211ms | 207.95ms | 232.28ms | 1.12 |
| typescript | linear | 50 | 242.01ms | 241.34ms | 244.66ms | 1.01 |

### FFI Overhead Analysis

Comparing FFI workers to native Rust baseline (linear pattern):

- **PYTHON**: 240.59ms (+32.2% overhead)
- **RUBY**: 238.21ms (+30.8% overhead)
- **TYPESCRIPT**: 241.34ms (+32.6% overhead)

**Observations:**
- All FFI workers add ~30-35% overhead vs native Rust
- Ruby shows lowest overhead for diamond pattern
- TypeScript shows slightly higher P95/P50 ratio indicating more variance
- Linear patterns consistently slower than diamond (more sequential steps)

---

## Tier 5: Batch Processing

Large-scale batch processing (1000 CSV rows, 5 parallel workers).

| Scenario | Samples | Mean | P50 | P95 | P95/P50 |
|----------|---------|------|-----|-----|---------|
| csv_products_1000_rows | 50 | 289.04ms | 287.95ms | 298.68ms | 1.04 |

**Observations:**
- 1000-row batch completes in ~288ms
- P95/P50 ratio of ~1.04 indicates stable batch processing
- Throughput: ~3,472 rows/second

---

## Performance Summary

### Latency by Tier

```
Tier 1 (Core):       ~182-185ms  (4 steps, native Rust)
Tier 2 (Complex):    ~149-306ms  (5-8 steps, varies by pattern)
Tier 4 (FFI):        ~185-242ms  (4 steps, +30-35% FFI overhead)
Tier 5 (Batch):      ~288ms      (1000 rows, 5 workers)
```

### Key Findings

1. **Stable Performance**: P95/P50 ratios mostly <1.1, indicating predictable latency
2. **FFI Overhead**: ~30-35% overhead for Ruby/Python/TypeScript vs native Rust
3. **Complexity Scaling**: Linear increase with step count (~40-45ms per step)
4. **Batch Efficiency**: High throughput with minimal variance

### Recommendations

1. **Use native Rust handlers** for latency-critical paths
2. **FFI workers are viable** for business logic (30-35% overhead acceptable)
3. **Monitor hierarchical patterns** - higher variance may indicate optimization opportunity
4. **Batch processing is efficient** - consider for bulk operations

---

## Methodology

- **Sample Size**: 50 per benchmark (except Tier 3 which uses legacy 10-sample data)
- **Measurement Time**: 120-180 seconds per tier
- **Environment**: Local development (not CI)
- **Percentiles**: P50 and P95 calculated from raw Criterion data

### Commands Used

```bash
cargo make services-start-release   # Start orchestration + rust worker
cargo make run-worker-ruby &        # Ruby worker (port 8082)
cargo make run-worker-python &      # Python worker (port 8083)
cargo make run-worker-typescript &  # TypeScript worker (port 8084)
cargo make bench-e2e-all            # Run all benchmarks
cargo make bench-report             # Generate percentile report
```

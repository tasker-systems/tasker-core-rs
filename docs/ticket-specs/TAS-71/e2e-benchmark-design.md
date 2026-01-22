# TAS-71: E2E Workflow Benchmark Design

**Date:** 2026-01-20
**Status:** Design Phase
**Depends On:** Research Areas A (profiling tools), B (benchmark audit)

---

## Executive Summary

This document designs end-to-end workflow benchmarks that measure complete task lifecycle performance across different complexity levels, service configurations, and worker languages. The goal is to establish baseline measurements and identify performance characteristics of the Tasker system.

---

## Available Workflow Patterns

Based on analysis of `tests/fixtures/task_templates/`, we have the following patterns available for benchmarking:

### Pattern Inventory

| Pattern | Steps | Parallelism | Convergence | Complexity | Template Example |
|---------|-------|-------------|-------------|------------|------------------|
| **Linear** | 4 | None | None | Low | `mathematical_sequence` |
| **Diamond** | 4 | 2-way | 2→1 | Medium | `diamond_pattern` |
| **Complex DAG** | 7 | Multi-path | 3→1 | High | `complex_dag` |
| **Hierarchical Tree** | 8 | 4-way leaves | 4→1 | High | `hierarchical_tree` |
| **Decision Conditional** | 3-6 | Variable | Deferred | Variable | `approval_routing` |
| **Batch Processing** | 3+N | N workers | N→1 | Scale-dependent | `large_dataset_processor` |

### Pattern Details

#### 1. Linear (Baseline)
```
A → B → C → D
```
- **Template:** `rust/mathematical_sequence.yaml`
- **Namespace:** `rust_e2e_linear`
- **Steps:** 4 sequential
- **Purpose:** Baseline measurement, minimal coordination overhead
- **Key metric:** Per-step overhead in isolation

#### 2. Diamond (Fan-out/Fan-in)
```
    ┌→ B ─┐
A ──┤     ├→ D
    └→ C ─┘
```
- **Template:** `rust/diamond_pattern.yaml`
- **Namespace:** `rust_e2e_diamond`
- **Steps:** 4 (1 start + 2 parallel + 1 convergence)
- **Purpose:** Measure parallel execution and convergence
- **Key metric:** Parallel speedup vs linear, convergence coordination cost

#### 3. Complex DAG (Multi-path)
```
        ┌→ Left ──→ Validate ─┐
Init ──┤                      ├→ Finalize
        └→ Right ─┬→ Transform┘
                  └→ Analyze ─┘
```
- **Template:** `rust/complex_dag.yaml`
- **Namespace:** `rust_e2e_mixed_dag`
- **Steps:** 7
- **Purpose:** Test complex dependency resolution
- **Key metric:** Multi-way convergence overhead, path selection efficiency

#### 4. Hierarchical Tree
```
                    Root
                   /    \
            BranchL    BranchR
            /    \      /    \
         LeafD LeafE LeafF LeafG
                   \      /
              FinalConvergence
```
- **Template:** `rust/hierarchical_tree.yaml`
- **Namespace:** `rust_e2e_tree`
- **Steps:** 8
- **Purpose:** Deep parallel fan-out with wide convergence
- **Key metric:** Maximum parallel utilization, deep dependency traversal

#### 5. Decision Conditional (Dynamic)
```
Validate → Decision ─┬→ AutoApprove ───────┐
                     ├→ ManagerApproval ──┼→ Finalize
                     └→ FinanceReview ────┘
```
- **Template:** `rust/conditional_approval_rust.yaml`
- **Namespace:** `conditional_approval_rust`
- **Steps:** 3-6 (depends on path taken)
- **Purpose:** Test dynamic step creation overhead
- **Key metric:** Decision evaluation time, deferred convergence cost

#### 6. Batch Processing (N-way)
```
Analyze → [Worker_001, Worker_002, ..., Worker_N] → Aggregate
```
- **Template:** `rust/batch_processing_example.yaml`
- **Namespace:** `data_processing`
- **Steps:** 2 + N (dynamic worker count)
- **Purpose:** Test massive parallelism and aggregation
- **Key metric:** Throughput scaling with batch size

---

## Proposed Benchmark Scenarios

### Tier 1: Core Performance (Always Run)

These benchmarks establish baseline performance and should run on every PR/commit.

| Scenario | Pattern | Config | Sample Size | Timeout |
|----------|---------|--------|-------------|---------|
| `linear_rust` | Linear | Single orch + 1 worker | 50 | 5s |
| `diamond_rust` | Diamond | Single orch + 1 worker | 50 | 5s |
| `linear_ruby` | Linear | Single orch + 1 worker | 50 | 8s |
| `diamond_ruby` | Diamond | Single orch + 1 worker | 50 | 8s |

**Note:** These already exist in `tests/benches/e2e_latency.rs`.

### Tier 2: Complexity Scaling (Weekly/On-demand)

These benchmarks test how complexity affects performance.

| Scenario | Pattern | Steps | Expected Time |
|----------|---------|-------|---------------|
| `complex_dag_rust` | Complex DAG | 7 | 150-300ms |
| `hierarchical_tree_rust` | Tree | 8 | 200-400ms |
| `conditional_approve_rust` | Decision | 3-6 | 100-250ms |

### Tier 3: Horizontal Scaling (Local Development Only)

These benchmarks require cluster infrastructure (from TAS-73).

| Scenario | Orchestration | Workers | Pattern |
|----------|---------------|---------|---------|
| `scale_1x1` | 1 | 1 Rust | Linear |
| `scale_1x2` | 1 | 2 Rust | Linear |
| `scale_1x4` | 1 | 4 Rust | Linear |
| `scale_2x2` | 2 | 2 Rust | Linear |
| `scale_2x4` | 2 | 4 Rust | Linear |

### Tier 4: Language Comparison (On-demand)

Compare FFI overhead across worker languages.

| Scenario | Pattern | Workers | Purpose |
|----------|---------|---------|---------|
| `linear_rust` | Linear | 1 Rust | Baseline |
| `linear_ruby` | Linear | 1 Ruby | FFI overhead |
| `linear_python` | Linear | 1 Python | FFI overhead |
| `linear_typescript` | Linear | 1 TypeScript | FFI overhead |

### Tier 5: Batch Scaling (On-demand)

Test throughput under batch workloads.

| Scenario | Batch Size | Workers | Total Items |
|----------|------------|---------|-------------|
| `batch_10` | 100 | 10 | 1,000 |
| `batch_50` | 100 | 50 | 5,000 |
| `batch_100` | 100 | 100 | 10,000 |

---

## Metrics to Capture

### Primary Metrics

| Metric | Unit | Description |
|--------|------|-------------|
| `e2e_latency_p50` | ms | Median task completion time |
| `e2e_latency_p95` | ms | 95th percentile completion time |
| `e2e_latency_p99` | ms | 99th percentile completion time |
| `e2e_latency_mean` | ms | Mean completion time |
| `e2e_latency_stddev` | ms | Standard deviation (variance indicator) |

### Derived Metrics

| Metric | Formula | Purpose |
|--------|---------|---------|
| `per_step_overhead` | `e2e_latency / step_count` | Coordination cost per step |
| `parallelism_efficiency` | `linear_time / (parallel_time * branches)` | How well parallelism is utilized |
| `ffi_overhead` | `ruby_time - rust_time` | FFI cost per workflow |

### System Metrics (During Benchmark)

| Metric | Source | Purpose |
|--------|--------|---------|
| `cpu_usage` | Process | Resource utilization |
| `memory_usage` | Process | Memory efficiency |
| `db_connections` | PG stats | Connection pool pressure |
| `queue_depth` | PGMQ/RabbitMQ | Backpressure indicator |

---

## Benchmark Infrastructure Requirements

### 1. Task Creation Helper

```rust
/// Create a benchmark task with unique identity per iteration
fn create_benchmark_task(
    namespace: &str,
    name: &str,
    context: serde_json::Value,
) -> TaskRequest {
    // Inject _test_run_id to ensure unique identity hash
    let mut ctx = context.as_object().cloned().unwrap_or_default();
    ctx.insert("_test_run_id".to_string(), Uuid::now_v7().to_string().into());

    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        context: serde_json::Value::Object(ctx),
        correlation_id: Uuid::now_v7(),
        initiator: "benchmark".to_string(),
        source_system: "tas-71-benchmarks".to_string(),
        ..Default::default()
    }
}
```

### 2. Completion Polling

Current implementation polls every 50ms with 10s timeout. Consider:
- Configurable poll interval (50ms default, 10ms for fast tests)
- Configurable timeout per pattern (5s for linear, 30s for batch)
- Failure detection (blocked_by_failures status)

### 3. Cluster Configuration Abstraction

```rust
struct BenchmarkCluster {
    orchestration_urls: Vec<String>,
    worker_counts: HashMap<WorkerType, u32>,
}

impl BenchmarkCluster {
    /// Start cluster with specified configuration
    async fn start(config: ClusterConfig) -> Self;

    /// Get round-robin client for task submission
    fn get_client(&self) -> OrchestrationApiClient;

    /// Stop all cluster instances
    async fn stop(&self);
}
```

### 4. Results Collection

```rust
struct BenchmarkResult {
    scenario: String,
    pattern: String,
    cluster_config: ClusterConfig,

    // Timing
    latencies_ms: Vec<f64>,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    mean_ms: f64,
    stddev_ms: f64,

    // Context
    step_count: u32,
    sample_count: u32,
    timestamp: DateTime<Utc>,
}
```

---

## Implementation Plan

### Phase 1: Extend Existing E2E Benchmarks

1. Add Complex DAG, Tree, and Decision benchmarks to `tests/benches/e2e_latency.rs`
2. Add Python and TypeScript worker scenarios
3. Improve output formatting with pattern metadata

### Phase 2: Create Cluster Benchmark Infrastructure

1. Create `tests/benches/cluster_benchmarks.rs`
2. Implement `BenchmarkCluster` abstraction
3. Add scaling scenarios (1x1 → 2x4)

### Phase 3: Add Batch Benchmarks

1. Create `tests/benches/batch_benchmarks.rs`
2. Implement dynamic batch size scenarios
3. Add throughput (tasks/sec) measurement

### Phase 4: Results Dashboard

1. JSON output format for criterion results
2. Historical comparison tooling
3. Regression detection alerts

---

## Cargo-make Integration

```toml
# New benchmark tasks
[tasks.bench-e2e]
description = "Run E2E workflow benchmarks (requires services)"
script = '''
#!/bin/bash
set -a; source .env; set +a
cargo bench --bench e2e_latency
'''

[tasks.bench-e2e-full]
description = "Run all E2E benchmarks including complex patterns"
script = '''
#!/bin/bash
set -a; source .env; set +a
cargo bench --bench e2e_latency --bench e2e_complexity
'''

[tasks.bench-cluster]
description = "Run cluster scaling benchmarks (requires cluster-start)"
dependencies = ["cluster-status"]
script = '''
#!/bin/bash
echo "⚠️  Ensure cluster is running: cargo make cluster-start"
set -a; source .env; set +a
cargo bench --features test-cluster --bench cluster_benchmarks
'''

[tasks.bench-all]
description = "Run ALL benchmarks (services + cluster)"
dependencies = ["services-health-check"]
script = '''
cargo make bench-e2e-full
cargo make bench-cluster
'''
```

---

## Expected Results Baseline

Based on existing benchmark data and system characteristics:

### Single Instance (1 orch + 1 worker)

| Pattern | Steps | Rust (p95) | Ruby (p95) | FFI Overhead |
|---------|-------|------------|------------|--------------|
| Linear | 4 | ~150ms | ~200ms | ~50ms |
| Diamond | 4 | ~160ms | ~220ms | ~60ms |
| Complex DAG | 7 | ~250ms | ~350ms | ~100ms |
| Tree | 8 | ~300ms | ~420ms | ~120ms |

### Cluster (2 orch + 4 workers)

| Pattern | Steps | Expected (p95) | Notes |
|---------|-------|----------------|-------|
| Linear | 4 | ~140ms | Limited parallelism |
| Diamond | 4 | ~120ms | Parallel branches |
| Tree | 8 | ~200ms | Full parallelism |

---

## Success Criteria

1. **Baseline established**: P50/P95/P99 for all Tier 1 and Tier 2 scenarios
2. **Reproducible**: < 15% variance between runs
3. **Automated**: cargo-make tasks for all benchmark tiers
4. **Documented**: Performance characteristics documented
5. **Regression detection**: Criterion baseline comparison enabled

---

## Open Questions

1. **Should we benchmark against RabbitMQ as well as PGMQ?**
   - Adds complexity but important for messaging backend comparison
   - Recommendation: Add as Tier 3 scenario variant

2. **How do we handle test data setup for batch benchmarks?**
   - Need realistic dataset sizes
   - Consider fixture generation script

3. **Should cluster benchmarks run in CI?**
   - Resource constraints on GHA free tier
   - Recommendation: Local-only initially, consider self-hosted runners later

---

## References

- Existing E2E benchmarks: `tests/benches/e2e_latency.rs`
- Benchmark documentation: `docs/benchmarks/README.md`
- Cluster testing guide: `docs/testing/cluster-testing-guide.md`
- TAS-73 cluster infrastructure: `cargo-make/scripts/multi-deploy/`

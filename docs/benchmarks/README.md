# Tasker Core Benchmarks

**Last Updated**: 2026-01-23
**Audience**: Architects, Developers
**Status**: Active
**Related Docs**: [Documentation Hub](../README.md) | [Observability](../observability/README.md) | [Deployment Patterns](../deployment-patterns.md)

<- Back to [Documentation Hub](../README.md)

---

This directory contains documentation for all performance benchmarks in the tasker-core workspace.

---

## Quick Reference

```bash
# E2E benchmarks (cluster mode, all tiers)
cargo make setup-env-all-cluster
cargo make cluster-start-all
set -a && source .env && set +a && cargo bench --bench e2e_latency
cargo make bench-report     # Percentile JSON
cargo make bench-analysis   # Markdown analysis
cargo make cluster-stop

# Component benchmarks (requires Docker services)
docker-compose -f docker/docker-compose.test.yml up -d
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
cargo bench --package tasker-client --features benchmarks   # API benchmarks
cargo bench --package tasker-shared --features benchmarks   # SQL + Event benchmarks
```

---

## Benchmark Categories

### 1. End-to-End Latency (`tests/benches`)

**Location**: `tests/benches/e2e_latency.rs`
**Documentation**: [e2e-benchmarks.md](./e2e-benchmarks.md)

Measures complete workflow execution from API call through orchestration, message queue, worker execution, result processing, and dependency resolution — across all distributed components in a 10-instance cluster.

| Tier | Benchmark | Steps | Parallelism | P50 | Target (p99) |
|------|-----------|-------|-------------|-----|--------------|
| 1 | Linear Rust | 4 sequential | none | 255-258ms | < 500ms |
| 1 | Diamond Rust | 4 (2 parallel) | 2-way | 200-259ms | < 500ms |
| 2 | Complex DAG | 7 (mixed) | 2+3-way | 382ms | < 800ms |
| 2 | Hierarchical Tree | 8 (4 parallel) | 4-way | 389-426ms | < 800ms |
| 2 | Conditional | 5 (3 executed) | dynamic | 251-262ms | < 500ms |
| 3 | Cluster single task | 4 sequential | none | 261ms | < 500ms |
| 3 | Cluster concurrent 2x | 4+4 | distributed | 332-384ms | < 800ms |
| 4 | FFI linear (Ruby/Python/TS) | 4 sequential | none | 312-316ms | < 800ms |
| 4 | FFI diamond (Ruby/Python/TS) | 4 (2 parallel) | 2-way | 260-275ms | < 800ms |
| 5 | Batch 1000 rows | 7 (5 parallel) | 5-way | 358-368ms | < 1000ms |

Each step involves ~19 database operations, 2 message queue round-trips, 4+ state transitions, and dependency graph evaluation. See [e2e-benchmarks.md](./e2e-benchmarks.md) for the detailed per-step lifecycle.

**Key Characteristics**:
- FFI overhead: ~23% vs native Rust (all languages within 3ms of each other)
- Linear patterns: highly reproducible (<2% variance between runs)
- Parallel patterns: environment-sensitive (I/O contention affects parallelism)
- Batch processing: 2,700-2,800 rows/second with tight P95/P50 ratios

**Run Commands**:
```bash
cargo make bench-e2e           # Tier 1: Rust core
cargo make bench-e2e-full      # Tier 1+2: + complexity
cargo make bench-e2e-cluster   # Tier 3: Multi-instance
cargo make bench-e2e-languages # Tier 4: FFI comparison
cargo make bench-e2e-batch     # Tier 5: Batch processing
cargo make bench-e2e-all       # All tiers
```

---

### 2. API Performance (`tasker-client`)

**Location**: `tasker-client/benches/task_initialization.rs`

Measures orchestration API response times for task creation (HTTP round-trip + DB insert + step initialization).

| Benchmark | Target | Current | Status |
|-----------|--------|---------|--------|
| Linear task init | < 50ms | 17.7ms | 2.8x better |
| Diamond task init | < 75ms | 20.8ms | 3.6x better |

```bash
cargo bench --package tasker-client --features benchmarks
```

---

### 3. SQL Function Performance (`tasker-shared`)

**Location**: `tasker-shared/benches/sql_functions.rs`

Measures critical PostgreSQL function performance for orchestration polling.

| Function | Target | Current (5K tasks) | Status |
|----------|--------|-------------------|--------|
| get_next_ready_tasks | < 3ms | 1.75-2.93ms | Pass |
| get_step_readiness_status | < 1ms | 440-603us | Pass |
| get_task_execution_context | < 1ms | 380-460us | Pass |

```bash
DATABASE_URL="..." cargo bench --package tasker-shared --features benchmarks sql_functions
```

---

### 4. Event Propagation (`tasker-shared`)

**Location**: `tasker-shared/benches/event_propagation.rs`

Measures PostgreSQL LISTEN/NOTIFY round-trip latency for real-time coordination.

| Metric | Target (p95) | Current | Status |
|--------|-------------|---------|--------|
| Notify round-trip | < 10ms | 14.1ms | Slightly above, p99 < 20ms |

```bash
DATABASE_URL="..." cargo bench --package tasker-shared --features benchmarks event_propagation
```

---

## Performance Targets

### System-Wide Goals

| Category | Metric | Target | Rationale |
|----------|--------|--------|-----------|
| API Latency | p99 | < 100ms | User-facing responsiveness |
| SQL Functions | mean | < 3ms | Orchestration polling efficiency |
| Event Propagation | p95 | < 10ms | Real-time coordination overhead |
| E2E Linear (4 steps) | p99 | < 500ms | End-user task completion |
| E2E Complex (7-8 steps) | p99 | < 800ms | Complex workflow completion |
| E2E Batch (1000 rows) | p99 | < 1000ms | Bulk operation completion |

### Scaling Targets

| Dataset Size | get_next_ready_tasks | Notes |
|--------------|---------------------|-------|
| 1K tasks | < 2ms | Initial implementation |
| 5K tasks | < 3ms | Current verified |
| 10K tasks | < 5ms | Target |
| 100K tasks | < 10ms | Production scale |

---

## Cluster Topology (E2E Benchmarks)

| Service | Instances | Ports | Build |
|---------|-----------|-------|-------|
| Orchestration | 2 | 8080, 8081 | Release |
| Rust Worker | 2 | 8100, 8101 | Release |
| Ruby Worker | 2 | 8200, 8201 | Release extension |
| Python Worker | 2 | 8300, 8301 | Maturin develop |
| TypeScript Worker | 2 | 8400, 8401 | Bun FFI |

**Deployment Mode**: Hybrid (event-driven with polling fallback)
**Database**: PostgreSQL (with PGMQ extension available)
**Messaging**: RabbitMQ (via MessagingService provider abstraction; PGMQ also supported)
**Sample Size**: 50 per benchmark

---

## Running Benchmarks

### E2E Benchmarks (Full Suite)

```bash
# 1. Setup cluster environment
cargo make setup-env-all-cluster

# 2. Start 10-instance cluster
cargo make cluster-start-all

# 3. Verify cluster health
cargo make cluster-status

# 4. Run benchmarks
set -a && source .env && set +a && cargo bench --bench e2e_latency

# 5. Generate reports
cargo make bench-report    # → target/criterion/percentile_report.json
cargo make bench-analysis  # → tmp/benchmark-results/benchmark-results.md

# 6. Stop cluster
cargo make cluster-stop
```

### Component Benchmarks

```bash
# Start database
docker-compose -f docker/docker-compose.test.yml up -d
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"

# Run individual suites
cargo bench --package tasker-client --features benchmarks     # API
cargo bench --package tasker-shared --features benchmarks     # SQL + Events

# Run all at once
cargo bench --all-features
```

### Baseline Comparison

```bash
# Save current performance as baseline
cargo bench --all-features -- --save-baseline main

# After changes, compare
cargo bench --all-features -- --baseline main

# View report
open target/criterion/report/index.html
```

---

## Interpreting Results

### Stable Metrics (Reliable for Regression Detection)

These metrics show <2% variance between runs:
- **Linear pattern P50** (sequential execution baseline)
- **FFI linear P50** (framework overhead measurement)
- **Single task in cluster** (cluster overhead measurement)
- **Batch P50** (parallel I/O throughput)

### Environment-Sensitive Metrics

These metrics vary 10-30% depending on system load:
- **Diamond pattern P50** (parallelism benefit depends on I/O capacity)
- **Concurrent 2x** (scheduling contention varies)
- **Hierarchical tree** (deep dependency chains amplify I/O latency)

### Key Ratios (Always Valid)

- **FFI overhead %**: ~23% for all languages (framework-dominated)
- **P95/P50 ratio**: 1.01-1.12 (execution stability indicator)
- **Cluster vs single overhead**: <3ms (negligible cluster tax)
- **FFI language spread**: <3ms (language runtime is not the bottleneck)

---

## Design Principles

### Natural Measurement
Benchmarks measure real system behavior without artificial test harnesses:
- API benchmarks hit actual HTTP endpoints
- SQL benchmarks use real database with realistic data volumes
- E2E benchmarks execute complete workflows through all distributed components

### Distributed System Focus
All benchmarks account for distributed system characteristics:
- Network latency included (HTTP, PostgreSQL, message queues)
- Database transaction timing considered
- Message queue delivery overhead measured
- Worker coordination and scheduling included

### Load-Based Validation
Benchmarks serve dual purpose:
- **Performance measurement**: Track regressions and improvements
- **Load testing**: Expose race conditions and timing bugs

E2E benchmark warmup has historically discovered critical race conditions that manual testing never revealed.

### Statistical Rigor
- 50 samples per benchmark for P50/P95 validity
- Criterion framework with statistical regression detection
- Multiple independent runs recommended for absolute comparisons
- Relative metrics (ratios, overhead %) preferred over absolute milliseconds

---

## Troubleshooting

### "Services must be running"
```bash
cargo make cluster-status          # Check cluster health
cargo make cluster-start-all       # Restart cluster
```

### Tier 3/4 benchmarks skipped
```bash
# Ensure cluster env is configured (not single-service)
cargo make setup-env-all-cluster   # Generates .env with cluster URLs
```

### High variance between runs
- Close resource-intensive applications (browsers, IDEs)
- Ensure machine is plugged in (not throttling)
- Focus on stable metrics (linear P50, FFI overhead %) for comparisons
- Run benchmarks twice and compare for reproducibility

### Benchmark takes too long
```bash
# Reduce sample size (default: 50)
cargo bench -- --sample-size 10

# Run single tier
cargo make bench-e2e               # Only Tier 1
```

---

## CI Integration

```yaml
# Example: .github/workflows/benchmarks.yml
name: Performance Benchmarks

on:
  pull_request:
    paths:
      - 'tasker-*/src/**'
      - 'migrations/**'

jobs:
  benchmark:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: ghcr.io/pgmq/pg18-pgmq:v1.8.1
        env:
          POSTGRES_DB: tasker_rust_test
          POSTGRES_USER: tasker
          POSTGRES_PASSWORD: tasker

    steps:
      - uses: actions/checkout@v3
      - run: cargo bench --all-features -- --save-baseline pr
      - uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'criterion'
          output-file-path: target/criterion/report/index.html
```

Criterion automatically detects performance regressions with statistical comparison to baselines and alerts on >5% slowdowns.

---

## Contributing

When adding new benchmarks:

1. **Follow naming convention**: `<tier>_<category>/<group>/<scenario>`
2. **Include targets**: Document expected performance in this README
3. **Add fixture**: Create workflow template YAML in `tests/fixtures/task_templates/`
4. **Document shape**: Update [e2e-benchmarks.md](./e2e-benchmarks.md) with topology
5. **Consider variance**: Account for distributed system characteristics
6. **Use 50 samples**: Minimum for P50/P95 statistical validity

### Benchmark Template
```rust
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

fn bench_my_scenario(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_my_tier");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(30));

    group.bench_function(BenchmarkId::new("workflow", "my_scenario"), |b| {
        b.iter(|| {
            runtime.block_on(async {
                execute_benchmark_scenario(&client, namespace, handler, context, timeout).await
            })
        });
    });

    group.finish();
}
```

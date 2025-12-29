# Tasker Core Benchmarks

**Last Updated**: 2025-10-10
**Audience**: Architects, Developers
**Status**: Active - Phase 5.4 Complete
**Related Docs**: [Documentation Hub](../README.md) | [Observability](../observability/README.md) | [Deployment Patterns](../deployment-patterns.md)

← Back to [Documentation Hub](../README.md)

---

This directory contains documentation for all performance benchmarks in the tasker-core workspace.

---

## Quick Reference

```bash
# Complete benchmark suite (requires Docker services running)
docker-compose -f docker/docker-compose.test.yml up -d
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"

# Run all benchmarks
cargo bench --all-features

# Individual categories
cargo bench --package tasker-client --features benchmarks        # API benchmarks
cargo bench --package tasker-shared --features benchmarks         # SQL + Event benchmarks
cargo bench --bench e2e_latency                                   # E2E latency (Rust + Ruby)
```

---

## Benchmark Categories

### 1. **API Performance** (`tasker-client`)
**Location**: `tasker-client/benches/task_initialization.rs`
**Status**: ✅ Complete
**Documentation**: [api-benchmarks.md](./api-benchmarks.md)

Measures orchestration API response times for task creation operations.

| Benchmark | Target | Current | Status |
|-----------|--------|---------|--------|
| Linear task init | < 50ms | 17.7ms | ✅ 2.8x better |
| Diamond task init | < 75ms | 20.8ms | ✅ 3.6x better |

**Run Command**:
```bash
cargo bench --package tasker-client --features benchmarks
```

---

### 2. **SQL Function Performance** (`tasker-shared`)
**Location**: `tasker-shared/benches/sql_functions.rs`
**Status**: ✅ Complete
**Documentation**: [sql-benchmarks.md](./sql-benchmarks.md)

Measures critical PostgreSQL function performance for orchestration operations.

| Function | Target | Current (5K tasks) | Status |
|----------|--------|-------------------|--------|
| get_next_ready_tasks | < 3ms | 1.75-2.93ms | ✅ Pass |
| get_step_readiness_status | < 1ms | 440-603µs | ✅ Pass |
| get_task_execution_context | < 1ms | 380-460µs | ✅ Pass |

**Run Command**:
```bash
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --package tasker-shared --features benchmarks sql_functions
```

**Features**:
- Query plan analysis with EXPLAIN ANALYZE
- Buffer hit ratio tracking
- Scaling tests with large datasets (5K+ tasks, 21K+ steps)

---

### 3. **Event Propagation** (`tasker-shared`)
**Location**: `tasker-shared/benches/event_propagation.rs`
**Status**: ✅ Complete
**Documentation**: [event-benchmarks.md](./event-benchmarks.md)

Measures PostgreSQL LISTEN/NOTIFY round-trip latency for real-time coordination.

| Metric | Target (p95) | Current | Status |
|--------|-------------|---------|--------|
| Notify round-trip | < 10ms | 14.1ms | ⚠️ Slightly above, p99 < 20ms |

**Run Command**:
```bash
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --package tasker-shared --features benchmarks event_propagation
```

**Note**: PGMQ notifications are fast enough to expose microsecond-level transaction timing (see TAS-29-high-concurrency-bug.md).

---

### 4. **End-to-End Latency** (`tests/benches`)
**Location**: `tests/benches/e2e_latency.rs`
**Status**: ✅ Complete (TAS-29 Phase 5.4)
**Documentation**: [e2e-benchmarks.md](./e2e-benchmarks.md)

Measures complete workflow execution from API call to task completion across all distributed components.

| Workflow | Worker | Target (p99) | Current (mean) | Status |
|----------|--------|--------------|----------------|--------|
| Linear (4 steps) | Rust | < 500ms | 133.5ms | ✅ 3.7x better |
| Diamond (4 steps) | Rust | < 800ms | 140.1ms | ✅ 5.7x better |
| Linear (4 steps) | Ruby FFI | < 800ms | TBD | 🚧 Re-enabled |
| Diamond (4 steps) | Ruby FFI | < 1000ms | TBD | 🚧 Re-enabled |

**Run Command**:
```bash
# Requires all Docker services (orchestration + workers)
docker-compose -f docker/docker-compose.test.yml up -d

# Verify services healthy
curl http://localhost:8080/health  # Orchestration
curl http://localhost:8081/health  # Rust worker
curl http://localhost:8082/health  # Ruby worker

# Run benchmark
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --bench e2e_latency
```

**Features**:
- Measures complete user-facing latency
- Tests both Rust native and Ruby FFI handlers
- Includes all system overhead (API, DB, queue, worker, events)
- Worker breakdown available via OpenTelemetry traces

**Important Notes**:
- Slow by design (measures actual workflow execution)
- Sample size: 10 (vs 50 default) due to duration
- Higher variance expected (network, scheduling, DB state)
- Race conditions fixed in TAS-29 Phase 5.4 (see [TAS-29](https://linear.app/tasker-systems/issue/TAS-29))

---

## Benchmark Design Principles

### 1. **Natural Measurement**
Benchmarks measure real system behavior without artificial test harnesses:
- API benchmarks hit actual HTTP endpoints
- SQL benchmarks use real database with realistic data volumes
- E2E benchmarks execute complete workflows through all components

### 2. **Distributed System Focus**
All benchmarks account for distributed system characteristics:
- Network latency included
- Database transaction timing considered
- Message queue overhead measured
- Worker coordination included

### 3. **Load-Based Validation**
Benchmarks serve dual purpose:
- **Performance measurement**: Track regression and improvements
- **Load testing**: Expose race conditions and timing bugs

Example: E2E benchmark warmup discovered two critical race conditions that manual testing never revealed.

### 4. **Realistic Data Volumes**
SQL benchmarks run against realistic dataset sizes:
- **Current**: 5,244 tasks, 20,999 steps, 61,482 transitions
- **Target**: 100K+ tasks for production simulation

---

## Prerequisites

### Docker Services
```bash
# Start all required services
docker-compose -f docker/docker-compose.test.yml up -d

# Verify all services healthy
docker-compose -f docker/docker-compose.test.yml ps

# Services required:
# - postgres (with PGMQ extension)
# - orchestration (API server on :8080)
# - worker (Rust worker on :8081)
# - ruby-worker (Ruby FFI worker on :8082)
```

### Environment Variables
```bash
# Required for SQL and E2E benchmarks
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"

# Optional: Skip health checks in CI
export TASKER_TEST_SKIP_HEALTH_CHECK="true"

# Optional: Save query plans
export SAVE_QUERY_PLANS="1"
```

### Build Requirements
```bash
# Ensure all features enabled for consistency
cargo build --all-features

# Benchmark-specific features
cargo bench --all-features  # Includes criterion dependency
```

---

## Running Benchmarks

### Individual Benchmarks
```bash
# API benchmarks only
cargo bench --package tasker-client --features benchmarks

# SQL benchmarks only
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --package tasker-shared --features benchmarks sql_functions

# Event propagation only
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --package tasker-shared --features benchmarks event_propagation

# E2E latency only
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --bench e2e_latency
```

### Complete Benchmark Suite
```bash
# Run ALL benchmarks (takes ~5-10 minutes)
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --all-features
```

### Baseline Comparison
```bash
# Save current performance as baseline
cargo bench --all-features -- --save-baseline main

# After making changes, compare to baseline
cargo bench --all-features -- --baseline main

# Show comparison results
open target/criterion/report/index.html
```

---

## Output and Results

### Criterion Reports
```bash
# HTML reports (most detailed)
open target/criterion/report/index.html

# Individual benchmark data
ls target/criterion/

# Example structure:
# target/criterion/
#   ├── api_task_creation/
#   ├── sql_get_next_ready_tasks/
#   ├── event_notify_round_trip/
#   └── e2e_task_completion/
```

### Console Output
Benchmarks provide structured console output:
- Header with benchmark description
- Real-time progress during warmup/measurement
- Statistical summaries (mean, median, std dev)
- Comparison to baselines (if available)

### Query Plan Analysis (SQL Benchmarks)
When running SQL benchmarks, query plans are analyzed and displayed:
- Execution time
- Planning time
- Buffer hit ratio
- Cost estimates

Enable JSON export with `SAVE_QUERY_PLANS=1`.

---

## Performance Targets

### System-Wide Goals

| Category | Metric | Target | Rationale |
|----------|--------|--------|-----------|
| API Latency | p99 | < 100ms | User-facing responsiveness |
| SQL Functions | mean | < 3ms | Orchestration polling efficiency |
| Event Propagation | p95 | < 10ms | Real-time coordination overhead |
| E2E Complete (4 steps) | p99 | < 500ms | End-user task completion |

### Scaling Targets

| Dataset Size | get_next_ready_tasks | Notes |
|--------------|---------------------|-------|
| 1K tasks | < 2ms | Initial implementation |
| 5K tasks | < 3ms | Current verified ✅ |
| 10K tasks | < 5ms | Target |
| 100K tasks | < 10ms | Production scale |

---

## CI Integration

### GitHub Actions Workflow
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
        image: tembo-io/pgmq-pg:latest
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

### Regression Detection
Criterion automatically detects performance regressions:
- Statistical comparison to baseline
- Alerts on significant slowdowns (>5% with statistical confidence)
- Color-coded output (green = improved, red = regressed)

---

## Troubleshooting

### "Services must be running"
```bash
# Ensure Docker services are healthy
docker-compose -f docker/docker-compose.test.yml ps
curl http://localhost:8080/health

# Restart if needed
docker-compose -f docker/docker-compose.test.yml restart
```

### "DATABASE_URL must be set"
```bash
# Check environment variable
echo $DATABASE_URL

# Set if missing
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
```

### "Task template not found" (E2E benchmarks)
```bash
# Worker services register templates on startup
docker-compose -f docker/docker-compose.test.yml restart worker ruby-worker

# Verify templates registered
curl -s http://localhost:8080/v1/handlers | jq
```

### High Variance in E2E Benchmarks
E2E benchmarks naturally have higher variance than micro-benchmarks:
- **Expected**: 5-15% standard deviation
- **Acceptable**: 20% std dev
- **Concerning**: >30% std dev (check for resource contention)

Factors affecting variance:
- Network latency fluctuations
- Worker scheduling
- Database cache state
- Background system activity

### Benchmark Takes Too Long
```bash
# Reduce sample size (default: 50, E2E uses 10)
cargo bench -- --sample-size 5

# Reduce measurement time (default: 5s for micro, 30s for E2E)
cargo bench -- --measurement-time 10
```

---

## Related Documentation

- **Architecture**: [TAS-29](https://linear.app/tasker-systems/issue/TAS-29)
- **Race Conditions**: [TAS-29](https://linear.app/tasker-systems/issue/TAS-29)
- **Observability**: [../observability/](../observability/)
- **SQL Functions**: [../task-and-step-readiness-and-execution.md](../task-and-step-readiness-and-execution.md)

---

## Future Enhancements

### Planned Benchmarks

1. **Worker Execution Overhead** (TAS-29 Phase 6)
   - Measure worker claim → execute → submit cycle
   - Compare Rust vs Ruby FFI overhead
   - Target: < 60ms total worker overhead

2. **Step Enqueueing** (TAS-29 Phase 6)
   - Measure orchestration step discovery → enqueue latency
   - Test with varying step counts (1, 3, 10 steps)
   - Target: < 50ms for 3 steps

3. **Memory Benchmarks**
   - Track memory usage under sustained load
   - Detect memory leaks
   - Target: < 100MB for orchestration, < 50MB per worker

4. **Throughput Benchmarks**
   - Measure tasks/second at saturation
   - Test with multiple workers
   - Target: > 100 tasks/second

### OpenTelemetry Integration

Future enhancement: Export benchmark results to OpenTelemetry for:
- Distributed tracing of benchmark runs
- Historical trend analysis
- Correlation with system metrics
- Production comparison

---

## Contributing

When adding new benchmarks:

1. **Follow naming convention**: `<category>_<operation>`
2. **Include targets**: Document expected performance
3. **Add to this README**: Keep centralized documentation updated
4. **Test under load**: Ensure benchmarks don't expose race conditions
5. **Consider variance**: Account for distributed system characteristics

### Benchmark Template
```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_my_operation(c: &mut Criterion) {
    c.bench_function("my_operation", |b| {
        b.iter(|| {
            // Operation to measure
        });
    });
}

criterion_group!(benches, bench_my_operation);
criterion_main!(benches);
```

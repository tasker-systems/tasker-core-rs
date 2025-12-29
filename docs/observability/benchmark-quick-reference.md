# Benchmark Quick Reference Guide

**Last Updated**: 2025-10-08

Quick commands for running all benchmarks in the TAS-29 Phase 5.4 suite.

---

## Prerequisites

```bash
# Start all Docker services
docker-compose -f docker/docker-compose.test.yml up -d

# Verify services are healthy
curl http://localhost:8080/health  # Orchestration
curl http://localhost:8081/health  # Rust Worker
curl http://localhost:8082/health  # Ruby Worker (optional)

# Set database URL (for SQL benchmarks)
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
```

---

## Individual Benchmarks

### ✅ Implemented Benchmarks

```bash
# 1. API Task Creation (COMPLETE - 17.7-20.8ms)
cargo bench --package tasker-client --features benchmarks

# 2. SQL Function Performance (COMPLETE - 380µs-2.93ms)
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo bench --package tasker-shared --features benchmarks sql_functions
```

### 🚧 Placeholder Benchmarks

```bash
# 3. Event Propagation (placeholder)
cargo bench --package tasker-shared --features benchmarks event_propagation

# 4. Worker Execution (placeholder)
cargo bench --package tasker-worker --features benchmarks worker_execution

# 5. Handler Overhead (placeholder)
cargo bench --package tasker-worker --features benchmarks handler_overhead

# 6. Step Enqueueing (placeholder)
cargo bench --package tasker-orchestration --features benchmarks step_enqueueing

# 7. End-to-End Latency (placeholder)
cargo bench --test e2e_latency
```

---

## Run All Benchmarks

```bash
# Run ALL benchmarks (implemented + placeholders)
cargo bench --all-features

# Run only SQL benchmarks
cargo bench --package tasker-shared --features benchmarks

# Run only worker benchmarks
cargo bench --package tasker-worker --features benchmarks
```

---

## Benchmark Categories

| Category | Package | Benchmark Name | Status | Run Command |
|----------|---------|----------------|--------|-------------|
| **API** | tasker-client | task_initialization | ✅ Complete | `cargo bench -p tasker-client --features benchmarks` |
| **SQL** | tasker-shared | sql_functions | ✅ Complete | `DATABASE_URL=... cargo bench -p tasker-shared --features benchmarks sql_functions` |
| **Events** | tasker-shared | event_propagation | 🚧 Placeholder | `cargo bench -p tasker-shared --features benchmarks event_propagation` |
| **Worker** | tasker-worker | worker_execution | 🚧 Placeholder | `cargo bench -p tasker-worker --features benchmarks worker_execution` |
| **Worker** | tasker-worker | handler_overhead | 🚧 Placeholder | `cargo bench -p tasker-worker --features benchmarks handler_overhead` |
| **Orchestration** | tasker-orchestration | step_enqueueing | 🚧 Placeholder | `cargo bench -p tasker-orchestration --features benchmarks` |
| **E2E** | tests | e2e_latency | 🚧 Placeholder | `cargo bench --test e2e_latency` |

---

## Benchmark Output Locations

```bash
# Criterion HTML reports
open target/criterion/report/index.html

# Individual benchmark data
ls target/criterion/

# Proposed: Structured logs (not yet implemented)
# tmp/benchmarks/YYYY-MM-DD-benchmark-name.log
```

---

## Common Options

```bash
# Save baseline for comparison
cargo bench --features benchmarks -- --save-baseline main

# Compare to baseline
cargo bench --features benchmarks -- --baseline main

# Verbose output
cargo bench --features benchmarks -- --verbose

# Run specific benchmark
cargo bench --package tasker-client --features benchmarks task_creation_api

# Skip health checks (CI mode)
TASKER_TEST_SKIP_HEALTH_CHECK=true cargo bench --features benchmarks
```

---

## Troubleshooting

### "Services must be running"
```bash
# Start Docker services
docker-compose -f docker/docker-compose.test.yml up -d

# Check service health
curl http://localhost:8080/health
```

### "DATABASE_URL must be set"
```bash
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
```

### "Task template not found"
```bash
# Ensure worker services are running (they register templates)
docker-compose -f docker/docker-compose.test.yml ps

# Check registered templates
curl -s http://localhost:8080/v1/handlers | jq
```

### Compilation errors
```bash
# Clean and rebuild
cargo clean
cargo build --all-features
```

---

## Performance Targets

| Benchmark | Metric | Target | Current | Status |
|-----------|--------|--------|---------|--------|
| Task Init (linear) | mean | < 50ms | 17.7ms | ✅ 3x better |
| Task Init (diamond) | mean | < 75ms | 20.8ms | ✅ 3.6x better |
| SQL Task Discovery | mean | < 3ms | 1.75-2.93ms | ✅ Pass |
| SQL Step Readiness | mean | < 1ms | 440-603µs | ✅ Pass |
| Worker Total Overhead | mean | < 60ms | TBD | 🚧 |
| Event Notify (p95) | p95 | < 10ms | TBD | 🚧 |
| Step Enqueue (3 steps) | mean | < 50ms | TBD | 🚧 |
| E2E Complete (3 steps) | p99 | < 500ms | TBD | 🚧 |

---

## Documentation

- **Full Strategy**: [benchmark-strategy-summary.md](./benchmark-strategy-summary.md)
- **Implementation Plan**: [phase-5.4-distributed-benchmarks-plan.md](./phase-5.4-distributed-benchmarks-plan.md)
- **SQL Benchmark Guide**: [benchmarking-guide.md](./benchmarking-guide.md)
- **Ticket**: [TAS-29](https://linear.app/tasker-systems/issue/TAS-29)

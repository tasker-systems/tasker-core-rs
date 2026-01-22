# TAS-71: Benchmark Audit Report

**Created**: 2026-01-20
**Status**: Complete
**Audience**: Developers, Performance Engineers
**Related**: [TAS-29 Phase 5.4](https://linear.app/tasker-systems/issue/TAS-29) | [docs/benchmarks/README.md](../../benchmarks/README.md)

---

## Executive Summary

This document provides a comprehensive audit of all benchmark files in the tasker-core codebase. The audit examines 9 benchmark files across 5 packages, evaluating their implementation status, methodology, and coverage.

### Key Findings

- **4 benchmarks fully implemented and maintained** (44%)
- **3 benchmarks are documented placeholders** (33%)
- **2 benchmarks are empty stubs** (22%)
- **Feature flag consistency**: All benchmarks use `benchmarks` feature flag with criterion
- **Criterion configuration**: Per-benchmark configuration with no shared config file
- **Coverage gaps identified**: Memory benchmarks, throughput benchmarks, and profiling integration

---

## Summary Table

| # | File | Package | Purpose | Status | Prerequisites |
|---|------|---------|---------|--------|---------------|
| 1 | `sql_functions.rs` | tasker-shared | SQL function performance | **Implemented** | DATABASE_URL |
| 2 | `event_propagation.rs` | tasker-shared | LISTEN/NOTIFY latency | **Implemented** | DATABASE_URL |
| 3 | `task_initialization.rs` | tasker-client | API task creation latency | **Implemented** | Docker services |
| 4 | `orchestration_benchmarks.rs` | tasker-orchestration | General orchestration | **Empty Stub** | N/A |
| 5 | `step_enqueueing.rs` | tasker-orchestration | Step enqueueing latency | **Placeholder** | Docker services |
| 6 | `handler_overhead.rs` | tasker-worker | FFI/framework overhead | **Placeholder** | N/A |
| 7 | `worker_benchmarks.rs` | tasker-worker | General worker | **Empty Stub** | N/A |
| 8 | `worker_execution.rs` | tasker-worker | Worker processing cycle | **Placeholder** | Docker services |
| 9 | `e2e_latency.rs` | tests | End-to-end workflow | **Implemented** | Docker services |

---

## Detailed Findings

### 1. SQL Functions Benchmark

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-shared/benches/sql_functions.rs`

**Purpose**: Measures critical PostgreSQL function performance for orchestration operations.

**What It Measures**:
- `get_next_ready_tasks()` - Task discovery (4 batch sizes: 1, 10, 50, 100)
- `get_step_readiness_status()` - Step readiness calculation (5 task samples)
- `get_task_execution_context()` - Task orchestration status (5 task samples)
- `get_step_transitive_dependencies()` - Recursive CTE dependency traversal (10 step samples)
- EXPLAIN ANALYZE query plan capture and analysis

**Methodology**:
- Criterion benchmark framework
- Sample size: 50 iterations
- Measurement time: 10 seconds per benchmark
- Noise threshold: 15% (higher than default 5% for database variance)
- Intelligent stratified sampling for diverse task/step selection
- Graceful degradation when no test data exists

**Prerequisites**:
- `DATABASE_URL` environment variable set
- PostgreSQL running with test data populated
- Feature flag: `--features benchmarks`

**Status**: **Fully Implemented and Maintained**
- Well-documented with comprehensive module-level docs
- Includes query plan analysis with buffer hit ratios
- Supports `SAVE_QUERY_PLANS=1` for detailed JSON export
- Current results documented in `docs/benchmarks/README.md`

**Issues/Observations**:
- Removed benchmarks for `transition_task_state_atomic()` (TAS-54) and `claim_task_for_finalization()` (no longer in schema) - appropriately deprecated with comments
- No benchmark for `execute_pending_step_atomic()` which is called frequently

---

### 2. Event Propagation Benchmark

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-shared/benches/event_propagation.rs`

**Purpose**: Measures PostgreSQL LISTEN/NOTIFY event propagation latency for real-time coordination.

**What It Measures**:
- PGMQ `pgmq_send_with_notify` round-trip latency
- PostgreSQL LISTEN/NOTIFY notification mechanism overhead
- Real event system latency for worker/orchestration coordination

**Methodology**:
- Criterion benchmark framework
- Sample size: 20 iterations (moderate for network operations)
- Measurement time: 15 seconds
- Creates test queue (`benchmark_queue`)
- Spawns listener task, sends notification, measures round-trip time
- Uses tokio mpsc channels for cross-task timing

**Prerequisites**:
- `DATABASE_URL` environment variable set
- PostgreSQL running with PGMQ extension
- Feature flag: `--features benchmarks`

**Status**: **Fully Implemented and Maintained**
- Well-documented with target performance expectations
- Creates its own test queue (idempotent)
- Good timeout handling (1 second per iteration)

**Issues/Observations**:
- 10ms sleep before sending to allow listener setup - could add variance
- Listener task is aborted (not cleanly shutdown) after each iteration
- No cleanup of benchmark messages from queue after test

---

### 3. Task Initialization Benchmark

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-client/benches/task_initialization.rs`

**Purpose**: Measures end-to-end API latency for task creation operations.

**What It Measures**:
- HTTP request/response overhead
- Task record creation in PostgreSQL
- Initial step discovery from template
- Response generation and serialization

**Methodology**:
- Criterion benchmark framework
- Sample size: 20 iterations (fewer due to network overhead)
- Measurement time: 15 seconds
- Tests two workflow patterns:
  - Linear workflow (3 steps): `mathematical_sequence`
  - Diamond workflow (4 steps): `diamond_pattern`
- Pre-verifies service health before benchmarking

**Prerequisites**:
- Docker Compose services running (`docker-compose.test.yml`)
- Orchestration service healthy at `http://localhost:8080`
- Feature flag: `--features benchmarks`

**Status**: **Fully Implemented and Maintained**
- Comprehensive documentation with troubleshooting guide
- Health check verification before running
- Results: Linear ~17.7ms, Diamond ~20.8ms (documented)

**Issues/Observations**:
- Creates new tokio runtime per benchmark iteration (minor inefficiency)
- No cleanup of created tasks after benchmark
- Minimal retries (1) which is appropriate for benchmarks

---

### 4. Orchestration Benchmarks

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-orchestration/benches/orchestration_benchmarks.rs`

**Purpose**: General orchestration benchmark placeholder.

**What It Measures**: Nothing currently.

**Status**: **Empty Stub**

```rust
// Empty benchmark file for now
fn main() {
    println!("tasker-orchestration benchmarks - not yet implemented");
}
```

**Issues/Observations**:
- File exists but has no implementation
- Not registered in Cargo.toml `[[bench]]` section (confirmed - no bench entries for orchestration)
- Comment in Cargo.toml: "Benchmark placeholders removed - use E2E benchmarks and OpenTelemetry traces"
- Should either be implemented or removed

---

### 5. Step Enqueueing Benchmark

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-orchestration/benches/step_enqueueing.rs`

**Purpose**: Measure orchestration coordination latency for step discovery and enqueueing.

**What It Would Measure** (if implemented):
- Ready step discovery time
- Queue publishing time
- LISTEN/NOTIFY notification overhead
- Total orchestration coordination cycle

**Status**: **Documented Placeholder**
- Extensive documentation explaining why not implemented
- References existing coverage from E2E benchmarks, SQL benchmarks, and OpenTelemetry traces
- Includes implementation guidance for future use

**Current Code**:
```rust
fn main() {
    eprintln!("STEP ENQUEUEING BENCHMARK - PLACEHOLDER");
    // Prints explanation of existing coverage
}
```

**Prerequisites** (if implemented):
- Docker Compose services
- Pre-created tasks with dependency chains

**Issues/Observations**:
- Justified decision to defer - coverage exists elsewhere
- Well-documented rationale
- Not registered in Cargo.toml (no `[[bench]]` entry)

---

### 6. Handler Overhead Benchmark

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-worker/benches/handler_overhead.rs`

**Purpose**: Measure framework overhead vs pure handler execution time.

**What It Would Measure** (if implemented):
- Pure Rust handler (baseline - direct function call)
- Rust handler via framework (dispatch overhead)
- Ruby handler via FFI (FFI boundary cost)

**Status**: **Documented Placeholder**
- Extensive documentation explaining existing coverage
- References E2E benchmarks (Ruby FFI vs Rust native) and profiling tools

**Current Code**:
```rust
fn main() {
    eprintln!("HANDLER OVERHEAD BENCHMARK - PLACEHOLDER");
    // Prints explanation of existing coverage
}
```

**Issues/Observations**:
- Coverage claimed via E2E benchmarks and profiling
- Would be valuable for micro-level overhead analysis if implemented
- Not registered in Cargo.toml

---

### 7. Worker Benchmarks

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-worker/benches/worker_benchmarks.rs`

**Purpose**: General worker benchmark placeholder.

**What It Measures**: Nothing currently.

**Status**: **Empty Stub**

```rust
// Empty benchmark file for now
fn main() {
    println!("tasker-worker benchmarks - not yet implemented");
}
```

**Issues/Observations**:
- File exists but has no implementation or documentation
- Not registered in Cargo.toml
- Should either be implemented or removed

---

### 8. Worker Execution Benchmark

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-worker/benches/worker_execution.rs`

**Purpose**: Measure complete worker execution cycle latency.

**What It Would Measure** (if implemented):
- Step claim (PGMQ read + atomic claim)
- Handler execution (framework overhead)
- Result submission (serialization + HTTP)
- Total overhead target: < 60ms

**Status**: **Documented Placeholder**
- Extensive documentation explaining existing coverage
- References E2E benchmarks, OpenTelemetry traces, and PGMQ metrics

**Current Code**:
```rust
fn main() {
    eprintln!("WORKER EXECUTION BENCHMARK - PLACEHOLDER");
    // Prints explanation of existing coverage
}
```

**Issues/Observations**:
- Coverage claimed via E2E benchmarks and observability tools
- Would be valuable for isolated worker performance analysis
- Not registered in Cargo.toml

---

### 9. E2E Latency Benchmark

**Location**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tests/benches/e2e_latency.rs`

**Purpose**: Measures complete workflow execution latency across the entire distributed system.

**What It Measures**:
- Complete workflow execution (API call to task completion)
- All system components: API, Database, Message Queue, Worker, Events
- Real network overhead
- Comparison of Ruby FFI vs Rust native handlers

**Methodology**:
- Criterion benchmark framework
- Sample size: 10 iterations (very few - workflows are slow)
- Measurement time: 30 seconds
- Tests four scenarios:
  - `linear_ruby` - Linear workflow with Ruby FFI
  - `diamond_ruby` - Diamond workflow with Ruby FFI
  - `linear_rust` - Linear workflow with native Rust
  - `diamond_rust` - Diamond workflow with native Rust
- Polls task status every 50ms until completion
- 10 second timeout per workflow

**Prerequisites**:
- All Docker Compose services running
- Orchestration service at `http://localhost:8080`
- Rust worker at `http://localhost:8081`
- Ruby worker at `http://localhost:8082` (for Ruby scenarios)

**Status**: **Fully Implemented and Maintained**
- Well-documented with expected performance targets
- TAS-154 integration: Injects unique `_test_run_id` for identity hash uniqueness
- Includes debug output during first 1.1 seconds of each iteration
- Documented race condition fixes (TAS-29 Phase 5.4)

**Issues/Observations**:
- Creates new task per iteration (appropriate for E2E)
- No cleanup of test tasks after benchmark
- Debug output may add slight overhead
- Good timeout and error handling

---

## Gap Analysis

### What We Are NOT Benchmarking (But Should Consider)

1. **Memory Benchmarks**
   - Memory usage under sustained load
   - Memory leak detection
   - Per-component memory footprint
   - Mentioned in `docs/benchmarks/README.md` as "Future Enhancement"

2. **Throughput Benchmarks**
   - Tasks/second at saturation
   - Concurrent task handling capacity
   - Worker scaling characteristics
   - Mentioned in `docs/benchmarks/README.md` as "Future Enhancement"

3. **Connection Pool Performance**
   - Database connection pool efficiency
   - Connection acquisition latency under load
   - Pool exhaustion scenarios

4. **Message Queue Throughput**
   - PGMQ message throughput (messages/second)
   - Queue depth impact on performance
   - Batch read/write efficiency

5. **State Machine Transitions**
   - `TaskStateMachine` transition performance (Rust-side)
   - State validation overhead
   - Concurrent transition handling

6. **Profiling Integration**
   - flamegraph/samply integration is mentioned but not automated
   - No criterion profiling hooks configured

7. **Large Dataset Scaling**
   - Current SQL benchmarks use ~5K tasks
   - No automated tests with 10K, 50K, or 100K+ tasks
   - Scaling targets documented but not verified

---

## Consistency Analysis

### Criterion Configuration

| Benchmark | Sample Size | Measurement Time | Noise Threshold | Custom Config |
|-----------|-------------|------------------|-----------------|---------------|
| sql_functions | 50 | 10s | 15% | Yes |
| event_propagation | 20 | 15s | Default (5%) | No |
| task_initialization | 20 | 15s | Default (5%) | No |
| e2e_latency | 10 | 30s | Default (5%) | No |

**Observation**: Sample sizes and measurement times vary appropriately based on operation type, but noise thresholds are inconsistent. SQL benchmarks use 15% threshold (justified in comments), while others use default 5%.

### Feature Flag Pattern

All benchmarks consistently use:
- Feature flag: `benchmarks`
- Dependency: `criterion = { workspace = true, optional = true }`
- Cargo.toml entry: `required-features = ["benchmarks"]`

**Configuration in Cargo.toml**:
```toml
[[bench]]
harness = false
name = "<benchmark_name>"
required-features = ["benchmarks"]

[features]
benchmarks = ["criterion"]
```

### No Shared Criterion Configuration

There is no `criterion.toml` or shared configuration file. Each benchmark configures criterion independently in code:

```rust
let mut group = c.benchmark_group("group_name");
group.sample_size(N);
group.measurement_time(Duration::from_secs(X));
```

**Recommendation**: Consider creating a shared benchmark configuration module for consistency.

### Runtime Setup Pattern

All implemented benchmarks follow a similar pattern:
```rust
fn setup_runtime() -> (tokio::runtime::Runtime, Pool) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let pool = runtime.block_on(async { /* setup */ });
    (runtime, pool)
}
```

This is consistent but creates a new runtime per benchmark group (not per iteration).

---

## Feature Flag Gating

### Current Implementation

| Package | Feature | Enables |
|---------|---------|---------|
| tasker-shared | `benchmarks` | criterion dependency |
| tasker-client | `benchmarks` | criterion dependency |
| tasker-orchestration | `benchmarks` | criterion dependency |
| tasker-worker | `benchmarks` | criterion dependency |
| tasker-core (root) | `benchmarks` | Empty (commented out) |

**Root Cargo.toml**:
```toml
[features]
benchmarks = [
  # "tasker-orchestration/benchmarks",
  # "tasker-shared/benchmarks",
  # "tasker-worker/benchmarks",
]
```

**Observation**: The root `benchmarks` feature is empty. To run all benchmarks, users must enable features per-package or use `--all-features`.

### Test Infrastructure Feature Flags (TAS-73)

Benchmarks do not currently use the TAS-73 test infrastructure levels:
- `test-db` - Database available
- `test-messaging` - + Messaging backend
- `test-services` - + Services running
- `test-cluster` - + Multi-instance cluster

**Recommendation**: Consider gating E2E benchmarks with `test-services` feature for consistency with test infrastructure.

---

## Recommendations

### Immediate Actions

1. **Remove or Document Empty Stubs**
   - `/tasker-orchestration/benches/orchestration_benchmarks.rs` - Remove (redundant with step_enqueueing placeholder)
   - `/tasker-worker/benches/worker_benchmarks.rs` - Remove (redundant with worker_execution placeholder)

2. **Clean Up Benchmark Queue**
   - `event_propagation.rs` should clean up messages after benchmark runs

3. **Add Task Cleanup**
   - Both `task_initialization.rs` and `e2e_latency.rs` create tasks but don't clean them up
   - Consider cleanup hooks or database truncation

### Short-Term Improvements

1. **Standardize Noise Thresholds**
   - Document or standardize noise threshold choices
   - Consider 15% for all database-dependent benchmarks

2. **Add Shared Benchmark Utilities**
   - Create `tasker-shared/src/benchmarks/mod.rs` with:
     - Common runtime setup
     - Health check utilities
     - Cleanup utilities
     - Consistent logging format

3. **Update Quick Reference Documentation**
   - `docs/observability/benchmark-quick-reference.md` shows some benchmarks as "placeholder" that are now implemented
   - Update status for event_propagation and e2e_latency

### Medium-Term Enhancements

1. **Implement Missing Benchmarks** (if needed)
   - Handler overhead (micro-level FFI analysis)
   - Worker execution cycle (isolated worker performance)
   - Step enqueueing (orchestration coordination)

2. **Add Memory Benchmarks**
   - Use `criterion-perf-events` or similar for memory tracking
   - Document baseline memory footprints

3. **Add Throughput Benchmarks**
   - Measure tasks/second under load
   - Test with configurable concurrency levels

4. **Profiling Integration**
   - Add cargo-make tasks for flamegraph generation
   - Document profiling workflow

### Long-Term Vision

1. **CI Benchmark Regression Detection**
   - Configure GitHub Actions to run benchmarks on PR
   - Alert on >10% regression

2. **Historical Trending**
   - Store benchmark results in structured format
   - Generate trend reports

3. **Large Scale Testing**
   - Automated tests with 10K, 50K, 100K+ tasks
   - Validate scaling targets

---

## Appendix: Benchmark File Locations

```
tasker-core/
├── tasker-shared/benches/
│   ├── sql_functions.rs          # Implemented - SQL performance
│   └── event_propagation.rs      # Implemented - LISTEN/NOTIFY
├── tasker-client/benches/
│   └── task_initialization.rs    # Implemented - API latency
├── tasker-orchestration/benches/
│   ├── orchestration_benchmarks.rs  # Empty stub - consider removal
│   └── step_enqueueing.rs           # Placeholder - documented deferral
├── tasker-worker/benches/
│   ├── handler_overhead.rs       # Placeholder - documented deferral
│   ├── worker_benchmarks.rs      # Empty stub - consider removal
│   └── worker_execution.rs       # Placeholder - documented deferral
└── tests/benches/
    └── e2e_latency.rs            # Implemented - E2E workflow
```

---

## References

- [docs/benchmarks/README.md](../../benchmarks/README.md) - Main benchmark documentation
- [docs/observability/benchmarking-guide.md](../../observability/benchmarking-guide.md) - SQL benchmark guide
- [docs/observability/benchmark-strategy-summary.md](../../observability/benchmark-strategy-summary.md) - TAS-29 Phase 5.4 strategy
- [docs/observability/benchmark-quick-reference.md](../../observability/benchmark-quick-reference.md) - Quick command reference

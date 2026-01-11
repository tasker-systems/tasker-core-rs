# SQL Function Benchmarking Guide (TAS-29 Phase 5.2)

**Created**: 2025-10-08
**Status**: ✅ Complete
**Location**: `tasker-shared/benches/sql_functions.rs`

---

## Overview

The SQL function benchmark suite measures performance of critical database operations that form the hot paths in the Tasker orchestration system. These benchmarks provide:

1. **Baseline Performance Metrics**: Establish expected performance ranges
2. **Regression Detection**: Identify performance degradations in code changes
3. **Optimization Guidance**: Use EXPLAIN ANALYZE output to guide index/query improvements
4. **Capacity Planning**: Understand scaling characteristics with data volume

---

## Quick Start

### Prerequisites

```bash
# 1. Ensure PostgreSQL is running
pg_isready

# 2. Set up test database
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
cargo sqlx migrate run

# 3. Populate with test data - REQUIRED for representative benchmarks
cargo test --all-features
```

**Important**: The benchmarks use intelligent sampling to test diverse task/step complexities. Running integration tests first ensures the database contains various workflow patterns (linear, diamond, parallel) for representative benchmarking.

### Running Benchmarks

```bash
# Run all SQL benchmarks
cargo bench --package tasker-shared --features benchmarks

# Run specific benchmark group
cargo bench --package tasker-shared --features benchmarks get_next_ready_tasks

# Run with baseline comparison
cargo bench --package tasker-shared --features benchmarks -- --save-baseline main
# ... make changes ...
cargo bench --package tasker-shared --features benchmarks -- --baseline main
```

---

## Sampling Strategy

The benchmarks use **intelligent sampling** to ensure representative results:

### Task Sampling
- Samples 5 diverse tasks from different `named_task_uuid` types
- Distributes samples across different workflow patterns
- Maintains deterministic ordering (same UUIDs in same order each run)
- Provides consistent results while capturing complexity variance

### Step Sampling
- Samples 10 diverse steps from different tasks
- Selects up to 2 steps per task for variety
- Captures different DAG depths and dependency patterns
- Helps identify performance variance in recursive queries

### Benefits
1. **Representativeness**: No bias from single task/step selection
2. **Consistency**: Same samples = comparable baseline comparisons
3. **Variance Detection**: Criterion can measure performance across complexities
4. **Real-world Accuracy**: Reflects actual production workload diversity

**Example Output**:
```
step_readiness_status/calculate_readiness/0    2.345 ms
step_readiness_status/calculate_readiness/1    1.234 ms  (simple linear task)
step_readiness_status/calculate_readiness/2    5.678 ms  (complex diamond DAG)
step_readiness_status/calculate_readiness/3    3.456 ms
step_readiness_status/calculate_readiness/4    2.789 ms
```

---

## Benchmark Categories

### 1. Task Discovery (`get_next_ready_tasks`)

**What it measures**: Time to discover ready tasks for orchestration

**Hot path**: Orchestration coordinator → Task discovery

**Test parameters**:
- Batch size: 1, 10, 50, 100 tasks
- Measures function overhead even with empty database

**Expected performance**:
- **Empty DB**: < 5ms for any batch size (function overhead)
- **With data**: Should scale linearly, not exponentially

**Optimization targets**:
- Index on task state
- Index on namespace for filtering
- Efficient processor ownership checks

**Example output**:
```
get_next_ready_tasks/batch_size/1
                        time:   [2.1234 ms 2.1567 ms 2.1845 ms]
get_next_ready_tasks/batch_size/10
                        time:   [2.2156 ms 2.2489 ms 2.2756 ms]
get_next_ready_tasks/batch_size/50
                        time:   [2.5123 ms 2.5456 ms 2.5789 ms]
get_next_ready_tasks/batch_size/100
                        time:   [3.0234 ms 3.0567 ms 3.0890 ms]
```

**Analysis**: Near-constant time across batch sizes indicates efficient query planning.

---

### 2. Step Readiness (`get_step_readiness_status`)

**What it measures**: Time to calculate if a step is ready to execute

**Hot path**: Step enqueuer → Dependency resolution

**Dependencies**: Requires test data (tasks with steps)

**Expected performance**:
- **Simple linear**: < 10ms
- **Diamond pattern**: < 20ms
- **Complex DAG**: < 50ms

**Optimization targets**:
- Parent step completion checks
- Dependency graph traversal
- Retry backoff calculations

**Graceful degradation**:
```
⚠️  Skipping step_readiness_status benchmark - no test data found
    Run integration tests first to populate test data
```

---

### 3. State Transitions (`transition_task_state_atomic`)

**What it measures**: Time for atomic state transitions with processor ownership

**Hot path**: All orchestration operations (initialization, enqueuing, finalization)

**Expected performance**:
- **Successful transition**: < 15ms
- **Failed transition (wrong state)**: < 10ms (faster path)
- **Contention scenario**: < 50ms with backoff

**Optimization targets**:
- Atomic compare-and-swap efficiency
- Index on task_uuid + processor_uuid
- Transition history table size

---

### 4. Task Execution Context (`get_task_execution_context`)

**What it measures**: Time to retrieve comprehensive task orchestration status

**Hot path**: Orchestration coordinator → Status checking

**Dependencies**: Requires test data (tasks in database)

**Expected performance**:
- **Simple tasks**: < 10ms
- **Complex tasks**: < 25ms
- **With many steps**: < 50ms

**Optimization targets**:
- Step aggregation queries
- State calculation efficiency
- Join optimization for step counts

---

### 5. Transitive Dependencies (`get_step_transitive_dependencies`)

**What it measures**: Time to resolve complete dependency tree for a step

**Hot path**: Worker → Step execution preparation (once per step lifecycle)

**Dependencies**: Requires test data (steps with dependencies)

**Expected performance**:
- **Linear dependencies**: < 5ms
- **Diamond pattern**: < 10ms
- **Complex DAG (10+ levels)**: < 25ms

**Optimization targets**:
- Recursive CTE performance
- Index on step dependencies
- Materialized dependency graphs (future)

**Why it matters**: Called once per step on worker side when populating step data. While not in orchestration hot path, it affects worker step initialization latency. Recursive CTEs can be expensive with deep dependency trees.

---

### 6. EXPLAIN ANALYZE (`explain_analyze`)

**What it measures**: Query execution plans, not just timing

**How it works**: Runs EXPLAIN ANALYZE **once per function** (no repeated iterations since query plans don't change between executions)

**Functions analyzed**:
- `get_next_ready_tasks()` - Task discovery query plans
- `get_task_execution_context()` - Task status aggregation plans
- `get_step_transitive_dependencies()` - Recursive CTE dependency traversal plans

**Purpose**: Identify optimization opportunities:
- Sequential scans (need indexes)
- Nested loop performance
- Buffer hit ratios
- Index usage patterns
- Recursive CTE efficiency

**Automatic Query Plan Logging**: Captures each query plan once and analyzes, printing:
- ⏱️ Execution Time: Actual query execution duration
- 📋 Planning Time: Time spent planning the query
- 📦 Node Type: Primary operation type (Aggregate, Index Scan, etc.)
- 💰 Total Cost: PostgreSQL's cost estimate
- ⚠️ Sequential Scan Warning: Alerts for potential missing indexes
- 📊 Buffer Hit Ratio: Cache efficiency (higher is better)

**Example output**:
```
════════════════════════════════════════════════════════════════════════════════
📊 QUERY PLAN ANALYSIS
════════════════════════════════════════════════════════════════════════════════

🔍 Function: get_next_ready_tasks
────────────────────────────────────────────────────────────────────────────────
⏱️  Execution Time: 2.345 ms
📋 Planning Time: 0.123 ms
📦 Node Type: Aggregate
💰 Total Cost: 45.67
📊 Buffer Hit Ratio: 98.5% (197/200 blocks)
────────────────────────────────────────────────────────────────────────────────
```

**Saving Full Plans**:
```bash
# Save complete JSON plans to target/query_plan_*.json
SAVE_QUERY_PLANS=1 cargo bench --package tasker-shared --features benchmarks
```

**Red flags to investigate**:
- "Seq Scan" on large tables → Add index
- "Nested Loop" with high iteration count → Optimize join strategy
- "Sort" operations on large datasets → Add index for ORDER BY
- Low buffer hit ratio (< 90%) → Increase shared_buffers or investigate I/O

---

## Interpreting Results

### Criterion Statistics

Criterion provides comprehensive statistics for each benchmark:

```
get_next_ready_tasks/batch_size/10
                        time:   [2.2156 ms 2.2489 ms 2.2756 ms]
                        change: [-1.5% +0.2% +1.9%] (p = 0.31 > 0.05)
                        No change in performance detected.
Found 3 outliers among 50 measurements (6.00%)
  2 (4.00%) high mild
  1 (2.00%) high severe
```

**Key metrics**:
- **[2.2156 ms 2.2489 ms 2.2756 ms]**: Lower bound, mean, upper bound (95% confidence)
- **change**: Comparison to baseline (if available)
- **p-value**: Statistical significance (p < 0.05 = significant)
- **Outliers**: Measurements far from median (cache effects, GC, etc.)

### Performance Expectations

Based on Phase 3 metrics verification (26 tasks executed):

| Metric | Expected | Warning | Critical |
|--------|----------|---------|----------|
| Task initialization | < 50ms | 50-100ms | > 100ms |
| Step readiness | < 20ms | 20-50ms | > 50ms |
| State transition | < 15ms | 15-30ms | > 30ms |
| Finalization claim | < 10ms | 10-25ms | > 25ms |

**Note**: These are function-level times, not end-to-end latencies.

---

## Using Benchmarks for Optimization

### Workflow

1. **Establish Baseline**
   ```bash
   cargo bench --package tasker-shared --features benchmarks -- --save-baseline main
   ```

2. **Make Changes** (e.g., add index, optimize query)

3. **Compare**
   ```bash
   cargo bench --package tasker-shared --features benchmarks -- --baseline main
   ```

4. **Review Output**
   ```
   get_next_ready_tasks/batch_size/100
                        time:   [2.0123 ms 2.0456 ms 2.0789 ms]
                        change: [-34.5% -32.1% -29.7%] (p = 0.00 < 0.05)
                        Performance has improved.
   ```

5. **Analyze EXPLAIN Plans** (if improvement isn't clear)

---

## Common Optimization Patterns

### Pattern 1: Missing Index

**Symptom**: Exponential scaling with data volume

**EXPLAIN shows**: `Seq Scan on tasks`

**Solution**:
```sql
CREATE INDEX idx_tasks_state ON tasker.tasks(current_state)
WHERE complete = false;
```

### Pattern 2: Inefficient Join

**Symptom**: High latency with complex DAGs

**EXPLAIN shows**: `Nested Loop` with high row counts

**Solution**: Use CTE or adjust join strategy
```sql
WITH parent_status AS (
  SELECT ... -- Pre-compute parent completions
)
SELECT ... FROM tasker.workflow_steps s
JOIN parent_status ps ON ...
```

### Pattern 3: Large Transaction History

**Symptom**: State transition slowing over time

**EXPLAIN shows**: Large scan of `task_transitions`

**Solution**: Partition by date or archive old transitions
```sql
CREATE TABLE tasker.task_transitions_archive (LIKE tasker.task_transitions);
-- Move old data periodically
```

---

## Integration with Metrics

The benchmark results should correlate with production metrics:

**From metrics-reference.md**:
- `tasker_task_initialization_duration_milliseconds` → Benchmark: task discovery + initialization
- `tasker_step_result_processing_duration_milliseconds` → Benchmark: step readiness + state transitions
- `tasker_task_finalization_duration_milliseconds` → Benchmark: finalization claiming

**Validation approach**:
1. Run benchmarks: Get ~2ms for task discovery
2. Check metrics: `tasker_task_initialization_duration` P95 = ~45ms
3. Calculate overhead: 45ms - 2ms = 43ms (business logic + framework)

This helps identify where optimization efforts should focus:
- If benchmark is slow → Optimize SQL/indexes
- If benchmark is fast but metrics slow → Optimize Rust code

---

## Continuous Integration

### Recommended CI Workflow

```yaml
# .github/workflows/benchmarks.yml
name: Performance Benchmarks

on:
  pull_request:
    branches: [main]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: tasker
        options: >-
          --health-cmd pg_isready
          --health-interval 10s

    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable

      - name: Run migrations
        run: cargo sqlx migrate run
        env:
          DATABASE_URL: postgresql://postgres:tasker@localhost/test

      - name: Run benchmarks
        run: cargo bench --package tasker-shared --features benchmarks

      - name: Check for regressions
        run: |
          # Parse Criterion output and fail if P95 > threshold
          # This is left as an exercise for CI implementation
```

---

## Future Enhancements

### Phase 5.3: Data Generation (Deferred)

The current benchmarks work with existing test data. Future work could add:

1. **Realistic Data Generation**
   - Create 100/1,000/10,000 task datasets
   - Various DAG complexities (linear, diamond, tree)
   - State distribution (60% complete, 20% in-progress, etc.)

2. **Contention Testing**
   - Multiple processors competing for same tasks
   - Race condition scenarios
   - Deadlock detection

3. **Long-Running Benchmarks**
   - Memory leak detection
   - Connection pool exhaustion
   - Query plan cache effects

---

## Troubleshooting

### Benchmark fails with "DATABASE_URL must be set"

```bash
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
```

### All benchmarks show "no test data found"

```bash
# Run integration tests to populate database
cargo test --all-features

# Or run specific test suite
cargo test --package tasker-shared --all-features
```

### Benchmarks are inconsistent/noisy

- Close other applications
- Ensure PostgreSQL isn't under load
- Run benchmarks multiple times
- Increase `sample_size` in benchmark code

### Results don't match production metrics

- Production has different data volumes
- Network latency in production
- Different PostgreSQL version/configuration
- Connection pool overhead in production

---

## References

- **Criterion Documentation**: https://bheisler.github.io/criterion.rs/book/
- **PostgreSQL EXPLAIN**: https://www.postgresql.org/docs/current/sql-explain.html
- **Phase 3 Metrics**: `docs/observability/metrics-reference.md`
- **Verification Results**: `docs/observability/VERIFICATION_RESULTS.md`

---

## Sign-Off

**Phase 5.2 Status**: ✅ **COMPLETE**

**Benchmarks Implemented**:
- ✅ `get_next_ready_tasks()` - 4 batch sizes
- ✅ `get_step_readiness_status()` - with graceful skip
- ✅ `transition_task_state_atomic()` - atomic operations
- ✅ `get_task_execution_context()` - orchestration status retrieval
- ✅ `get_step_transitive_dependencies()` - recursive dependency traversal
- ✅ `EXPLAIN ANALYZE` - query plan capture with automatic analysis

**Documentation Complete**:
- ✅ Quick start guide
- ✅ Interpretation guidance
- ✅ Optimization patterns
- ✅ Integration with metrics
- ✅ CI recommendations

**Next Steps**: Run benchmarks with real data and establish baseline performance targets.

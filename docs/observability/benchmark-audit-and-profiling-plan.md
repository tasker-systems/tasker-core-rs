# Benchmark Audit & Profiling Plan (TAS-29)

**Created**: 2025-10-09
**Status**: 📋 Planning
**Purpose**: Audit existing benchmarks, establish profiling tooling, baseline before Actor/Services refactor

---

## Executive Summary

Before refactoring `tasker-orchestration/src/orchestration/lifecycle/` to Actor/Services pattern, we need to:

1. **Audit Benchmarks**: Review which benchmarks are implemented vs placeholders
2. **Clean Up**: Remove or complete placeholder benchmarks
3. **Establish Profiling**: Set up flamegraph/samply tooling
4. **Baseline Profiles**: Capture performance profiles for comparison post-refactor

**Current Status**: We have **working SQL and E2E benchmarks** but several **placeholder component benchmarks** that need decisions.

---

## Benchmark Inventory

### ✅ Working & Complete Benchmarks

#### 1. SQL Function Benchmarks
- **Location**: `tasker-shared/benches/sql_functions.rs`
- **Status**: ✅ Complete, Compiles, Well-documented
- **Coverage**:
  - `get_next_ready_tasks()` (4 batch sizes)
  - `get_step_readiness_status()` (5 diverse samples)
  - `transition_task_state_atomic()` (5 samples)
  - `get_task_execution_context()` (5 samples)
  - `get_step_transitive_dependencies()` (10 samples)
- **Documentation**: `docs/observability/benchmarking-guide.md`
- **Run Command**:
  ```bash
  cargo bench --package tasker-shared --features benchmarks
  ```

#### 2. Event Propagation Benchmarks
- **Location**: `tasker-shared/benches/event_propagation.rs`
- **Status**: ✅ Complete, Compiles
- **Coverage**: PostgreSQL LISTEN/NOTIFY event propagation
- **Run Command**:
  ```bash
  cargo bench --package tasker-shared --features benchmarks event_propagation
  ```

#### 3. Task Initialization Benchmarks
- **Location**: `tasker-client/benches/task_initialization.rs`
- **Status**: ✅ Complete, Compiles
- **Coverage**: API task creation latency
- **Run Command**:
  ```bash
  export SQLX_OFFLINE=true
  cargo bench --package tasker-client --features benchmarks task_initialization
  ```

#### 4. End-to-End Workflow Latency Benchmarks
- **Location**: `tests/benches/e2e_latency.rs`
- **Status**: ✅ Complete, Compiles
- **Coverage**: Complete workflow execution (API → Result)
  - Linear workflow (Ruby FFI)
  - Diamond workflow (Ruby FFI)
  - Linear workflow (Rust native)
  - Diamond workflow (Rust native)
- **Prerequisites**: Docker Compose services running
- **Run Command**:
  ```bash
  export SQLX_OFFLINE=true
  cargo bench --bench e2e_latency
  ```

### ⚠️ Placeholder Benchmarks (Need Decision)

#### 5. Orchestration Benchmarks
- **Location**: `tasker-orchestration/benches/`
- **Files**:
  - `orchestration_benchmarks.rs` - Empty placeholder
  - `step_enqueueing.rs` - Placeholder with documentation
- **Status**: Not implemented
- **Documented Intent**: Measure orchestration coordination latency
- **Challenges**:
  - Requires triggering orchestration cycle without full execution
  - Need step discovery measurement isolation
  - Queue publishing and notification overhead breakdown

#### 6. Worker Benchmarks
- **Location**: `tasker-worker/benches/`
- **Files**:
  - `worker_benchmarks.rs` - Empty placeholder
  - `worker_execution.rs` - Placeholder with documentation
  - `handler_overhead.rs` - Placeholder with documentation
- **Status**: Not implemented
- **Documented Intent**:
  - Worker processing cycle (claim, execute, submit)
  - Framework overhead vs pure handler execution
  - Ruby FFI overhead measurement
- **Challenges**:
  - Need pre-enqueued steps in test queues
  - Noop handler implementations for baseline
  - Breakdown metrics for each phase

---

## Recommendations

### Option 1: Keep Placeholders for Future Work ✅ RECOMMENDED

**Rationale**:
- Phase 5.4 distributed benchmarks are **documented** but complex to implement
- E2E benchmarks (`e2e_latency.rs`) already provide full workflow metrics
- SQL benchmarks provide component-level detail
- Actor/Services refactor is more urgent than distributed component benchmarks

**Action**:
- Keep placeholder files with clear "NOT IMPLEMENTED" status
- Update comments to reference this audit document
- Future ticket (post-refactor) can implement if needed

### Option 2: Remove Placeholders

**Rationale**:
- Reduce confusion about benchmark status
- E2E benchmarks already cover end-to-end latency
- SQL benchmarks cover database hot paths

**Action**:
- Delete placeholder bench files
- Document decision in this file
- Can recreate later if specific component isolation needed

### Option 3: Implement Placeholders Now

**Rationale**:
- Complete benchmark suite before refactor
- Better baseline data for Actor/Services comparison

**Concerns**:
- 2-3 days implementation effort
- Delays Actor/Services refactor
- May need re-implementation post-refactor anyway

---

## Decision: Option 1 (Keep Placeholders, Document Status)

We have **sufficient benchmarking coverage**:
1. ✅ SQL functions (hot path queries)
2. ✅ E2E workflows (user-facing latency)
3. ✅ Event propagation (LISTEN/NOTIFY)
4. ✅ Task initialization (API latency)

**What's Missing**:
- Component-level orchestration breakdown (not critical for refactor)
- Worker cycle breakdown (available via OpenTelemetry traces)
- Framework overhead measurement (nice-to-have, not blocking)

**Action Items**:
1. Update placeholder comments with "Status: Planned for future implementation"
2. Reference this document for implementation guidance
3. Move forward with profiling and refactor

---

## Profiling Tooling Setup

### Goals

1. **Identify Inefficiencies**: Find hot spots in lifecycle code
2. **Establish Baseline**: Profile before Actor/Services refactor
3. **Compare Post-Refactor**: Validate performance impact of refactor
4. **Continuous Profiling**: Enable ongoing performance analysis

### Tool Selection

#### Primary: `samply` (macOS-friendly)
- **GitHub**: https://github.com/mstange/samply
- **Advantages**:
  - Native macOS support (uses Instruments)
  - Interactive web UI for flamegraphs
  - Low overhead
  - Works with release builds
- **Use Case**: Development profiling on macOS

#### Secondary: `flamegraph` (CI/production)
- **GitHub**: https://github.com/flamegraph-rs/flamegraph
- **Advantages**:
  - Linux support (perf-based)
  - SVG output for CI artifacts
  - Well-established in Rust ecosystem
- **Use Case**: CI profiling, Linux production analysis

#### Tertiary: `cargo-flamegraph` (Convenience)
- **Cargo Plugin**: Wraps flamegraph-rs
- **Advantages**:
  - Single command profiling
  - Automatic symbol resolution
- **Use Case**: Quick local profiling

---

## Installation

### macOS Setup (samply)

```bash
# Install samply
cargo install samply

# macOS requires SIP adjustment for sampling (one-time setup)
# https://github.com/mstange/samply#macos-permissions

# Verify installation
samply --version
```

### Linux Setup (flamegraph)

```bash
# Install prerequisites (Ubuntu/Debian)
sudo apt-get install linux-tools-common linux-tools-generic

# Install flamegraph
cargo install flamegraph

# Allow perf without sudo (optional)
echo 'kernel.perf_event_paranoid=-1' | sudo tee -a /etc/sysctl.conf
sudo sysctl -p

# Verify installation
flamegraph --version
```

### Cross-Platform (cargo-flamegraph)

```bash
# Install cargo-flamegraph
cargo install cargo-flamegraph

# Verify installation
cargo flamegraph --version
```

---

## Profiling Workflows

### 1. Profile E2E Benchmark (Recommended for Baseline)

Captures the entire workflow execution including orchestration lifecycle:

```bash
# macOS
samply record cargo bench --bench e2e_latency -- --profile-time=60

# Linux
cargo flamegraph --bench e2e_latency -- --profile-time=60

# Output: Interactive flamegraph showing hot paths
```

**What to Look For**:
- Time spent in `lifecycle/` modules (task_initializer, step_enqueuer, result_processor, etc.)
- Database query time vs business logic time
- Serialization/deserialization overhead
- Lock contention (should be minimal with our architecture)

### 2. Profile SQL Benchmarks

Isolates database performance:

```bash
# Profile just SQL function benchmarks
samply record cargo bench --package tasker-shared --features benchmarks sql_functions

# Output: Shows PostgreSQL function overhead
```

**What to Look For**:
- Time in `sqlx` query execution
- Connection pool overhead
- Query planning time (shouldn't be visible if using prepared statements)

### 3. Profile Integration Tests (Realistic Workload)

Profile actual test execution for realistic patterns:

```bash
# Profile a specific integration test
samply record cargo test --test e2e_tests e2e::rust::simple_integration_tests::test_linear_workflow

# Profile all integration tests (longer run)
samply record cargo test --test e2e_tests --all-features
```

**What to Look For**:
- Initialization overhead
- Test setup time vs actual execution time
- Repeated patterns across tests

### 4. Profile Specific Lifecycle Components

Isolate specific modules for deep analysis:

```bash
# Example: Profile only result processing
samply record cargo test --package tasker-orchestration --test lifecycle_integration_tests \
  test_result_processing_updates_task_state --all-features -- --nocapture

# Or profile a unit test for a specific function
samply record cargo test --package tasker-orchestration \
  result_processor::tests::test_process_step_result_success --all-features
```

---

## Baseline Profiling Plan

### Phase 1: Capture Pre-Refactor Baselines (Day 1)

**Goal**: Establish performance baseline of current lifecycle code before Actor/Services refactor

```bash
# 1. Clean build
cargo clean
cargo build --release --all-features

# 2. Profile E2E benchmarks (primary baseline)
samply record --output=baseline-e2e-pre-refactor.json \
  cargo bench --bench e2e_latency

# 3. Profile SQL benchmarks
samply record --output=baseline-sql-pre-refactor.json \
  cargo bench --package tasker-shared --features benchmarks

# 4. Profile specific lifecycle operations
samply record --output=baseline-task-init-pre-refactor.json \
  cargo test --package tasker-orchestration \
  lifecycle::task_initializer::tests --all-features

samply record --output=baseline-step-enqueue-pre-refactor.json \
  cargo test --package tasker-orchestration \
  lifecycle::step_enqueuer::tests --all-features

samply record --output=baseline-result-processor-pre-refactor.json \
  cargo test --package tasker-orchestration \
  lifecycle::result_processor::tests --all-features
```

**Deliverables** (completed, profiles removed in TAS-166 — superseded by cluster benchmarks):
- ~~Baseline profile files in `profiles/pre-refactor/`~~ (removed)
- Performance baselines now in `docs/benchmarks/README.md`

### Phase 2: Identify Optimization Opportunities (Day 1)

**Goal**: Document current performance characteristics to preserve in refactor

**Analysis Checklist**:
1. ✅ Time spent in each lifecycle module (task_initializer, step_enqueuer, etc.)
2. ✅ Database query time breakdown
3. ✅ Serialization overhead (JSON, MessagePack)
4. ✅ Lock contention points (if any)
5. ✅ Unnecessary allocations or clones
6. ✅ Recursive call depth

**Document Findings**:
Performance baselines are now documented in `docs/benchmarks/README.md` (TAS-166).
The original `lifecycle-performance-baseline.md` was removed — its measurements had
data quality issues and the refactor it targeted is complete.

### Phase 3: Post-Refactor Validation (After Refactor)

**Goal**: Validate Actor/Services refactor maintains or improves performance

```bash
# Re-run same profiling commands after refactor
samply record --output=baseline-e2e-post-refactor.json \
  cargo bench --bench e2e_latency

# Compare baselines
# (samply doesn't have built-in diff, use manual comparison)
```

**Success Criteria**:
- E2E latency: Within 10% of baseline (preferably faster)
- SQL latency: Unchanged (no regression from refactor)
- Lifecycle operation time: Within 20% of baseline
- No new hot paths or contention points

**Regression Signals**:
- E2E latency >20% slower
- New allocations/clones in hot paths
- Increased lock contention
- Message passing overhead >5% of total time

---

## Profiling Best Practices

### 1. Use Release Builds

```bash
# Always profile release builds (--release flag)
cargo build --release --all-features
samply record cargo bench --bench e2e_latency
```

**Rationale**: Debug builds have 10-100x overhead that masks real performance issues

### 2. Run Multiple Times

```bash
# Run 3 times, compare consistency
for i in {1..3}; do
  samply record --output=profile-$i.json cargo bench --bench e2e_latency
done
```

**Rationale**: Catch warm-up effects, JIT compilation, cache behavior

### 3. Isolate Interference

```bash
# Close other applications
# Disable background processes (Spotlight, backups)
# Use consistent hardware (don't profile on battery power)
```

### 4. Focus on Hot Paths

**80/20 Rule**: 80% of time is spent in 20% of code

**Priority Order**:
1. Top 5 functions by time (>5% each)
2. Recursive calls (can amplify overhead)
3. Locks and synchronization (contention multiplies)
4. Allocations in loops (O(n) becomes visible)

### 5. Benchmark-Driven Profiling

Always profile **realistic workloads**:
- ✅ E2E benchmarks (represents user experience)
- ✅ Integration tests (real workflow patterns)
- ❌ Unit tests (too isolated, not representative)

---

## Flamegraph Interpretation

### Reading Flamegraphs

```
┌─────────────────────────────────────────────┐ ← Total Program Time (100%)
│                                             │
│  ┌────────────────┐  ┌─────────────────┐  │
│  │ Database Ops   │  │ Serialization   │  │ ← High-level Operations (60%)
│  │ (30%)          │  │ (30%)           │  │
│  │                │  │                 │  │
│  │  ┌──────────┐  │  │  ┌───────────┐ │  │
│  │  │ SQL Exec │  │  │  │ JSON Ser  │ │  │ ← Leaf Operations (25%)
│  │  │ (25%)    │  │  │  │ (20%)     │ │  │
│  └──┴──────────┴──┘  └──┴───────────┴─┘  │
│                                             │
│  ┌──────────────────────────────────────┐  │
│  │ Business Logic (20%)                  │  │ ← Remaining Time
│  └──────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

**Width** = Time spent in function (including children)
**Height** = Call stack depth
**Color** = Function group (can be customized)

### Key Patterns

#### 1. Wide Flat Bars = Hot Path
```
┌───────────────────────────────────────┐
│ step_enqueuer::enqueue_ready_steps()  │  ← 40% of total time
└───────────────────────────────────────┘
```
**Action**: Optimize this function

#### 2. Deep Call Stack = Recursion/Abstractions
```
┌─────────────────────────┐
│ process_dependencies()  │
│  ┌─────────────────────┐│
│  │ resolve_deps()      ││
│  │  ┌─────────────────┐││
│  │  │ check_ready()   │││
│  │  └─────────────────┘││
│  └─────────────────────┘│
└─────────────────────────┘
```
**Action**: Consider flattening or caching

#### 3. Many Narrow Bars = Fragmentation
```
┌─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┐
│A│B│C│D│E│F│G│H│I│J│K│L│M│  ← Many small functions
└─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┘
```
**Action**: Not necessarily bad (may be inlining), but check if overhead-heavy

---

## Integration with CI

### GitHub Actions Workflow (Future Enhancement)

```yaml
# .github/workflows/profile-benchmarks.yml
name: Profile Benchmarks

on:
  pull_request:
    paths:
      - 'tasker-orchestration/src/orchestration/lifecycle/**'
      - 'tasker-shared/src/**'

jobs:
  profile:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install flamegraph
        run: cargo install flamegraph

      - name: Profile benchmarks
        run: |
          cargo flamegraph --bench e2e_latency -- --profile-time=60 -o flamegraph.svg

      - name: Upload flamegraph
        uses: actions/upload-artifact@v3
        with:
          name: flamegraph
          path: flamegraph.svg

      - name: Compare with baseline
        run: |
          # TODO: Implement baseline comparison
          # Download previous flamegraph, compare hot paths
```

---

## Documentation Structure

### Created Documents

1. **This Document**: `docs/observability/benchmark-audit-and-profiling-plan.md`
   - Benchmark inventory
   - Profiling tooling setup
   - Baseline capture plan

2. **Existing**: `docs/observability/benchmarking-guide.md`
   - SQL benchmark documentation
   - Running instructions
   - Performance expectations

3. ~~`docs/observability/lifecycle-performance-baseline.md`~~ (Removed — superseded by `docs/benchmarks/README.md`)

---

## Next Steps

### Before Actor/Services Refactor

1. ✅ **Audit Complete**: Documented benchmark status
2. ⏳ **Install Profiling Tools**:
   ```bash
   cargo install samply  # macOS
   cargo install flamegraph  # Linux
   ```
3. ⏳ **Capture Baselines** (1 day):
   - Run profiling plan Phase 1
   - Generate flamegraphs
   - Document hot paths
4. ✅ **Baseline Document**: Superseded by `docs/benchmarks/README.md` (TAS-166)

### During Actor/Services Refactor

1. **Incremental Profiling**: Profile after each major component conversion
2. **Compare Baselines**: Ensure no performance regressions
3. **Document Changes**: Note architectural changes affecting performance

### After Actor/Services Refactor

1. **Full Re-Profile**: Run profiling plan Phase 3
2. **Comparison Analysis**: Document performance changes
3. **Update Documentation**: Reflect new architecture
4. **Benchmark Updates**: Update benchmarks if Actor/Services changes measurement approach

---

## Summary

**Current State**:
- ✅ SQL benchmarks working
- ✅ E2E benchmarks working
- ✅ Event propagation benchmarks working
- ✅ Task initialization benchmarks working
- ⚠️ Component benchmarks are placeholders (OK for now)

**Decision**:
- Keep placeholder benchmarks for future work
- Move forward with profiling and baseline capture
- Sufficient coverage to validate Actor/Services refactor

**Action Plan**:
1. Install profiling tools (samply/flamegraph)
2. Capture pre-refactor baselines (1 day)
3. Document current hot paths
4. Proceed with Actor/Services refactor
5. Validate post-refactor performance

**Success Criteria**:
- Baseline profiles captured
- Hot paths documented
- Post-refactor validation plan established
- No performance regressions from refactor

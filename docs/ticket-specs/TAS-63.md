# TAS-63: Code Coverage Analysis and Gap Closure

**Status**: In Progress
**Priority**: High
**Last Updated**: 2026-01-28
**Original Baseline**: 2025-11-02 (43.35% line coverage, 917 tests)
**Current State**: 1,185+ tests across workspace

---

## Executive Summary

This document merges the original TAS-63 coverage analysis with updated findings from January 2026. The test suite has grown significantly (917 → 1,185+ tests), but critical coverage gaps remain in core orchestration logic.

### Key Findings (2026-01-28 Update)

| Metric | Nov 2025 Baseline | Jan 2026 Current |
|--------|-------------------|------------------|
| Total Tests | 917 | 1,185+ |
| Line Coverage | 43.35% | ~43% (similar) |
| Source Files with Tests | Not measured | 178/413 (43%) |
| Integration Test Files | Not measured | 115 dedicated files |

**Critical Discovery**: The `result_processing` module (2,505 lines of core task completion logic) has **zero inline tests** and relies entirely on E2E coverage.

---

## Coverage Gaps: Prioritized List

### 🔴 Critical Priority (Production Risk)

#### 1. Result Processing Module (NEW - Not in original ticket)
**Location**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/`

| File | Lines | Inline Tests | Risk |
|------|-------|--------------|------|
| `message_handler.rs` | 799 | 0 | **Critical** |
| `state_transition_handler.rs` | 348 | 0 | **Critical** |
| `task_coordinator.rs` | 322 | 0 | **Critical** |
| `processing_context.rs` | 360 | 0 | **High** |

**Impact**: This is the core task completion logic. Edge cases (partial failures, concurrent state transitions, malformed messages) are not tested at the unit level.

**Recommended Tests**:
- State transition validation logic
- Error recovery paths in message handling
- Concurrent task coordination scenarios
- Processing context state management

#### 2. FFI Dispatch Channel (Severity increased)
**Location**: `tasker-worker/src/handlers/ffi_dispatch_channel.rs`
**Size**: 1,143 lines
**Tests**: 4 inline tests only

**Impact**: Bridges Rust to Ruby/Python/TypeScript workers. Failures affect all cross-language workloads.

**Recommended Tests**:
- Channel backpressure handling
- Timeout scenarios during FFI calls
- Error propagation from foreign handlers
- Semaphore exhaustion behavior

#### 3. Actor System (From original ticket - still 0%)
**Location**: `tasker-orchestration/src/orchestration/actors/`

All 4 core actors have 0% unit test coverage:
- `TaskRequestActor` - Task initialization
- `ResultProcessorActor` - Step result processing
- `StepEnqueuerActor` - Batch step enqueueing
- `TaskFinalizerActor` - Task completion

**Note**: These are tested via integration tests but edge cases aren't covered.

### 🟠 High Priority (Quality Risk)

#### 4. Web Handlers (From original ticket - still 0%)
**Locations**:
- `tasker-orchestration/src/web/` (middleware, routes, extractors)
- `tasker-worker/src/web/` (handlers, middleware)

**Recommended Tests**:
- Authentication middleware edge cases
- Route parameter extraction/validation
- Error response formatting
- Request validation logic

#### 5. CLI Binary (From original ticket - still 0%)
**Location**: `tasker-client/bin/tasker-cli.rs`
**Size**: 1,302 lines
**Coverage**: 0%

**Recommended Tests**:
- Command parsing and validation
- Error handling and user feedback
- Configuration file handling

#### 6. Bootstrap Code (From original ticket - minimal coverage)
| File | Coverage |
|------|----------|
| `tasker-orchestration/bootstrap.rs` | 0% (251 lines) |
| `tasker-worker/bootstrap.rs` | ~20% (216 lines) |

### 🟡 Medium Priority (Technical Debt)

#### 7. Core Orchestration Integration Points
**Files needing tests**:
- `orchestration/core.rs` - Lifecycle integration point
- `orchestration/commands/mod.rs`, `orchestration/commands/types.rs` - Command dispatch
- `orchestration/hydration/mod.rs`, `hydration/step_result_hydrator.rs` - Data hydration

#### 8. Infrastructure Components (From original ticket)
**tasker-shared** untested areas:
- `cache/mod.rs`, `cache/errors.rs`, `cache/traits.rs` - Cache infrastructure
- `config/mod.rs`, `config/circuit_breaker.rs`, `config/web.rs` - Config system
- `database/mod.rs`, `database/migrator.rs`, `database/sql_functions.rs` - Database layer
- `metrics/*.rs` - All metrics collection
- `event_system/mod.rs`, `events/mod.rs` - Event systems
- `messaging/mod.rs` - Messaging routing

#### 9. Error Handling Paths
**Current state**: Tests primarily cover happy paths + single failure scenarios.

**Missing coverage**:
- Network timeouts in async handlers
- Circuit breaker cascade failures
- Database pool exhaustion
- Partial message corruption
- Race conditions in concurrent processing

### 🟢 Lower Priority (Completeness)

#### 10. Property-Based Testing
State machines would benefit from property-based tests:
```rust
proptest! {
    #[test]
    fn task_state_transitions_are_valid(transitions in arb_transitions()) {
        // Assert all transitions follow valid state machine rules
    }
}
```

#### 11. Doctest Migration (From ticket comment)
Migrate from `ignore` doctests to `no_run` wherever possible to maintain code validity checking without requiring runtime dependencies.

---

## Coverage Strengths (What's Working Well)

### High Coverage Areas (>90%)
- **Test Factories**: 96-100% (`foundation.rs`, `complex_workflows.rs`, `states.rs`)
- **Worker Event Types**: 95-100% (worker_queues/events)
- **Task Templates**: 97% (`task_template.rs`)
- **Type Definitions**: 98% (`api/orchestration.rs`)
- **Resilience Config**: 95-100% (circuit_breaker, config)
- **Handler Registry**: 95% (`workers/rust/registry.rs`)
- **Health Checks**: 100% (`worker/health.rs`)

### Strong Test Infrastructure
- **Factory System**: Comprehensive test data builders in `tasker-shared/src/models/factories/`
- **Test Utilities**: `tests/common/` with actor harnesses, lifecycle managers, cluster simulation
- **E2E Patterns**: 115 integration test files covering 8 major workflow patterns
- **Multi-Language**: Ruby (637 specs), Python (416 tests), TypeScript (31 test files)

---

## Action Plan

### Phase 1: Critical Gap Closure (Weeks 1-2)

| Task | Target Coverage | Effort |
|------|-----------------|--------|
| Add unit tests to `result_processing/` | 50% | High |
| Add tests to `ffi_dispatch_channel.rs` | 40% | Medium |
| Add actor unit tests | 30% | Medium |

### Phase 2: High Priority Gaps (Weeks 3-4)

| Task | Target Coverage | Effort |
|------|-----------------|--------|
| Web handler tests | 40% | Medium |
| CLI command tests | 30% | Low |
| Bootstrap tests | 40% | Low |

### Phase 3: CI Integration (Week 5)

```yaml
# .github/workflows/coverage.yml
- name: Generate Coverage
  run: |
    DATABASE_URL=${{ secrets.DATABASE_URL }} \
    TASKER_ENV=test \
    cargo llvm-cov nextest --all-features --workspace \
      --lcov --output-path coverage.lcov

- name: Upload to Codecov
  uses: codecov/codecov-action@v4
  with:
    files: ./coverage.lcov

- name: Check Coverage Thresholds
  run: |
    cargo llvm-cov --fail-under-lines 45 \
      --fail-under-functions 40
```

### Phase 4: Error Path Testing (Weeks 6-7)

Add targeted error scenario tests:
```rust
#[tokio::test]
async fn test_handles_database_timeout() { ... }

#[tokio::test]
async fn test_recovers_from_partial_batch_failure() { ... }

#[tokio::test]
async fn test_circuit_breaker_opens_after_threshold() { ... }
```

### Phase 5: Advanced Testing (Ongoing)

- Property-based tests for state machines
- Chaos engineering tests (network partitions, worker crashes)
- Performance benchmarks for critical paths
- Doctest migration (`ignore` → `no_run`)

---

## Coverage Targets

### Per-Package Goals

| Package | Current | Target | Priority |
|---------|---------|--------|----------|
| `tasker-orchestration/lifecycle/result_processing` | 0% | 50% | Critical |
| `tasker-orchestration/actors` | 0% | 50% | Critical |
| `tasker-worker/handlers` | ~5% | 40% | High |
| `tasker-orchestration/web` | 0% | 40% | High |
| `tasker-client` (CLI) | 0% | 30% | Medium |
| `tasker-shared/state_machine` | 40-70% | 80% | High |
| Overall workspace | ~43% | 55% | Target |

### Coverage Gates for PRs

- **New code**: Minimum 60% coverage
- **Critical paths**: No reduction allowed
- **Overall**: No more than 2% regression

---

## Commands Reference

### Generate Coverage Report
```bash
# CORRECT - Run ALL tests with coverage
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test \
cargo llvm-cov nextest --no-fail-fast --all-features --workspace

# HTML report for browsing
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test \
cargo llvm-cov nextest --all-features --workspace \
  --html --output-dir coverage/html

# LCOV for CI
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test \
cargo llvm-cov nextest --all-features --workspace \
  --lcov --output-path coverage/full-suite.lcov
```

**Critical Note**: Do NOT add `run` after `nextest` - it acts as a test filter!

### Per-Package Analysis
```bash
cargo llvm-cov nextest --all-features \
  --package tasker-orchestration \
  --html --output-dir coverage/orchestration
```

---

## Open Questions

1. **Coverage Tool**: Continue with `cargo-llvm-cov` or also evaluate `tarpaulin`?
2. **FFI Coverage**: How do we capture coverage for Ruby/Python code paths?
3. **Integration vs Unit**: Should we shift strategy toward more unit tests?
4. **CI Frequency**: Coverage on every PR or just main branch?
5. **Blocking PRs**: Should coverage regression block merges?

---

## References

- [cargo-llvm-cov Documentation](https://github.com/taiki-e/cargo-llvm-cov)
- [nextest Documentation](https://nexte.st/)
- [Codecov Rust Integration](https://docs.codecov.com/docs/rust)
- TAS-49: Comprehensive DLQ testing (example of good coverage approach)
- `docs/testing/cluster-testing-guide.md`: Cluster test patterns
- `docs/testing/comprehensive-lifecycle-testing-guide.md`: Lifecycle test patterns

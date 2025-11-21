# TAS-64: Retryability and Resumability E2E Testing Plan

**Created**: 2025-11-20
**Status**: In Progress
**Branch**: `jcoletaylor/tas-64-retryability-and-resumability-e2e-testing`
**Related**: [Batch Processing](../../batch-processing.md), [Retry Semantics](../../retry-semantics.md), [States and Lifecycles](../../states-and-lifecycles.md)

---

## Implementation Progress

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 | ✅ Complete | Fast polling configuration for test environment |
| Phase 2 | ✅ Complete | FailNTimesHandler error injection handler |
| Phase 3 | ✅ Complete | Task templates for retry testing |
| Phase 4 | ✅ Complete | E2E retry mechanics tests (both passing) |
| Phase 5 | ✅ Complete | Batch resumption tests with CheckpointAndFailHandler |
| Phase 6 | 🔲 Pending | Integration test enhancements (future work) |

### Completed Tests (2025-11-21)

**Retry Mechanics:**
- **`test_retry_after_transient_failure`** - Proves step fails 2 times then succeeds on attempt 3
- **`test_retry_exhaustion_leads_to_error`** - Proves max_attempts exhaustion leads to error state

**Batch Resumption:**
- **`test_batch_worker_resumes_from_checkpoint`** - Proves batch workers resume from cursor checkpoint after retry
- **`test_batch_no_duplicate_processing`** - Validates no duplicate item processing on resumption

### Key Implementation Details

1. **Handler Registration Pattern**: Generic handlers like `FailNTimesHandler` must be registered under each step name that uses them (not by handler class name), since the worker looks up handlers by `template_step_name`.

2. **Runtime Config**: Handler config must be read from `step_data.step_definition.handler.initialization` at execution time, not from construction-time config.

3. **Attempt Counting**: The `workflow_step.attempts` field is 1-indexed (first attempt = 1).

4. **Checkpoint Preservation**: The `workflow_step.results` field is preserved by `ResetForRetry`, allowing batch workers to store and retrieve checkpoint progress.

5. **BatchWorkerContext**: Use `BatchWorkerContext::from_step_data()` to extract cursor configuration from `workflow_step.inputs`.

### Files Created/Modified

**New Files:**
- `workers/rust/src/step_handlers/error_injection/mod.rs`
- `workers/rust/src/step_handlers/error_injection/fail_n_times_handler.rs`
- `workers/rust/src/step_handlers/error_injection/checkpoint_and_fail_handler.rs`
- `tests/fixtures/task_templates/rust/retry_mechanics_test.yaml`
- `tests/fixtures/task_templates/rust/retry_exhaustion_test.yaml`
- `tests/fixtures/task_templates/rust/batch_resumption_test.yaml`
- `tests/e2e/rust/retry_mechanics_test.rs`
- `tests/e2e/rust/batch_resumption_test.rs`

**Modified Files:**
- `config/tasker/environments/test/orchestration.toml` - Fast polling config
- `workers/rust/src/step_handlers/mod.rs` - Added error_injection module
- `workers/rust/src/step_handlers/registry.rs` - Added `register_handler_as()` method, registered handlers under step names
- `tests/e2e/rust/mod.rs` - Added retry_mechanics_test and batch_resumption_test modules

---

## Overview

This ticket adds rigorous E2E testing for retry mechanics and batch job cursor-based resumability. We need to prove that:

1. **Steps actually retry after transient failures** (not just SQL function tests)
2. **Batch workers resume from cursor checkpoints on retry**
3. **The full orchestration loop works**: backoff → WaitingForRetry → polling discovery → re-execution

## Problem Statement

Current testing coverage validates retry mechanics at the SQL function level (`retry_boundary_tests.rs`) and state machine level, but does not prove that:

- The complete retry flow works end-to-end through orchestration
- Batch workers correctly preserve and resume from cursor checkpoints
- Exponential backoff timing is respected in practice
- Multiple retry attempts actually execute handlers again

## Key Findings from Research

### Current Gaps

| Gap | Impact |
|-----|--------|
| No Rust error injection handlers | Can't test retry in Rust E2E tests |
| No "fail N times then succeed" patterns | Can't prove retry actually re-executes |
| No cursor checkpoint resumption tests | Infrastructure exists but untested |
| Default 30s polling intervals | Too slow for E2E tests |
| No backoff timing validation | Can't verify exponential delays |

### Available Infrastructure

- `LifecycleTestManager` has `fail_step()` method for integration tests
- `ResetForRetry` preserves `workflow_steps.results` (where cursors live)
- SQL function `evaluate_step_state_readiness()` handles `waiting_for_retry` state
- Configurable polling intervals via TOML overrides
- `BackoffCalculator` implements exponential backoff with jitter

---

## Implementation Plan

### Phase 1: Test Configuration for Fast Retry Cycles

**Goal**: Configure test environment for fast polling and short backoff delays.

**File**: `config/tasker/environments/test/orchestration.toml`

Add/update:
```toml
[orchestration.event_systems.orchestration.timing]
fallback_polling_interval_seconds = 1

[orchestration.event_systems.task_readiness.timing]
fallback_polling_interval_seconds = 1

[orchestration.event_systems.orchestration.processing.backoff]
initial_delay_ms = 50
max_delay_ms = 500
multiplier = 1.5
```

**Rationale**: With 1-second polling and 50ms initial backoff, retry tests complete in seconds rather than minutes.

---

### Phase 2: Rust Error Injection Handlers

**Goal**: Create handlers that fail controllably for retry testing.

**Directory**: `workers/rust/src/step_handlers/error_injection/`

#### 2.1 FailNTimesHandler

Fails the first N attempts, then succeeds.

```rust
pub struct FailNTimesHandler;

impl StepHandler for FailNTimesHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let fail_count = step_data.handler_initialization
            .get("fail_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as i32;

        let current_attempt = step_data.workflow_step.attempts;

        if current_attempt < fail_count {
            // Return retryable error
            Ok(error_result(
                step_uuid,
                format!("Simulated failure on attempt {}", current_attempt + 1),
                elapsed_ms,
                true, // retryable
            ))
        } else {
            // Success on attempt >= fail_count
            Ok(success_result(
                step_uuid,
                json!({
                    "attempts_before_success": current_attempt + 1,
                    "message": "Succeeded after simulated failures"
                }),
                elapsed_ms,
                None,
            ))
        }
    }
}
```

**Use Cases**:
- Test retry mechanics work end-to-end
- Validate attempt counting
- Test retry exhaustion scenarios

#### 2.2 CheckpointAndFailHandler

For batch cursor resumption testing. Processes items with checkpointing, fails at configurable point.

```rust
pub struct CheckpointAndFailHandler;

impl StepHandler for CheckpointAndFailHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let context = BatchWorkerContext::from_step_data(step_data)?;

        // Check for existing checkpoint in results
        let resume_from = step_data.workflow_step.results
            .as_ref()
            .and_then(|r| r.get("batch_cursor"))
            .and_then(|c| c.get("current_position"))
            .and_then(|p| p.as_u64())
            .unwrap_or(context.start_position());

        let fail_after = step_data.handler_initialization
            .get("fail_after_items")
            .and_then(|v| v.as_u64())
            .unwrap_or(50);

        let fail_on_attempt = step_data.handler_initialization
            .get("fail_on_attempt")
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32;

        let checkpoint_interval = step_data.handler_initialization
            .get("checkpoint_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(25);

        let mut processed = 0u64;
        let mut current_position = resume_from;
        let end_position = context.end_position();

        while current_position < end_position {
            // Process item
            processed += 1;
            current_position += 1;

            // Checkpoint at intervals
            if processed % checkpoint_interval == 0 {
                // In real implementation, would update step results
            }

            // Fail at configured point on specific attempt
            if processed >= fail_after && step_data.workflow_step.attempts == fail_on_attempt {
                return Ok(error_result_with_checkpoint(
                    step_uuid,
                    "Simulated failure at checkpoint",
                    current_position,
                    processed,
                    true, // retryable
                ));
            }
        }

        // Success - processed all items
        Ok(success_result(
            step_uuid,
            json!({
                "total_processed": processed,
                "resumed_from": resume_from,
                "final_position": current_position
            }),
            elapsed_ms,
            None,
        ))
    }
}
```

**Use Cases**:
- Test cursor-based resumption
- Validate no duplicate processing
- Test checkpoint preservation across retries

#### 2.3 ConditionalErrorHandler

Fails based on context conditions for complex scenarios.

```rust
pub struct ConditionalErrorHandler;

impl StepHandler for ConditionalErrorHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let should_fail = step_data.task.context
            .get("trigger_failure")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let error_type = step_data.task.context
            .get("error_type")
            .and_then(|v| v.as_str())
            .unwrap_or("retryable");

        if should_fail {
            let retryable = error_type != "permanent";
            return Ok(error_result(
                step_uuid,
                format!("Conditional {} error triggered", error_type),
                elapsed_ms,
                retryable,
            ));
        }

        Ok(success_result(step_uuid, json!({"status": "ok"}), elapsed_ms, None))
    }
}
```

---

### Phase 3: Task Templates for Retry Testing

**Directory**: `tests/fixtures/task_templates/rust/`

#### 3.1 retry_mechanics_test.yaml

```yaml
name: retry_mechanics_test
namespace_name: rust_e2e_retry
version: "1.0.0"
description: "Tests retry mechanics with configurable failure count"

task_handler:
  callable: rust_handler
  initialization: {}

steps:
  - name: fail_twice_then_succeed
    dependencies: []
    handler:
      callable: FailNTimesHandler
      initialization:
        fail_count: 2
    retry:
      retryable: true
      max_attempts: 5
      backoff: exponential
      backoff_base_ms: 50
      max_backoff_ms: 200
    lifecycle:
      max_steps_in_process_minutes: 5
```

#### 3.2 retry_exhaustion_test.yaml

```yaml
name: retry_exhaustion_test
namespace_name: rust_e2e_retry
version: "1.0.0"
description: "Tests retry exhaustion leading to permanent failure"

task_handler:
  callable: rust_handler
  initialization: {}

steps:
  - name: always_fail
    dependencies: []
    handler:
      callable: FailNTimesHandler
      initialization:
        fail_count: 10  # More than max_attempts
    retry:
      retryable: true
      max_attempts: 3
      backoff: exponential
      backoff_base_ms: 50
      max_backoff_ms: 100
    lifecycle:
      max_steps_in_process_minutes: 5
```

#### 3.3 batch_resumption_test.yaml

```yaml
name: batch_resumption_test
namespace_name: rust_e2e_batch_retry
version: "1.0.0"
description: "Tests batch worker cursor-based resumption after failure"

task_handler:
  callable: rust_handler
  initialization: {}

steps:
  - name: analyze_data
    type: batchable
    dependencies: []
    handler:
      callable: SimpleBatchAnalyzer
      initialization:
        total_items: 200
        batch_size: 100
        worker_count: 2

  - name: process_batch
    type: batch_worker
    dependencies:
      - analyze_data
    handler:
      callable: CheckpointAndFailHandler
      initialization:
        checkpoint_interval: 25
        fail_after_items: 50
        fail_on_attempt: 1
    retry:
      retryable: true
      max_attempts: 3
      backoff: exponential
      backoff_base_ms: 50
      max_backoff_ms: 100
    lifecycle:
      max_steps_in_process_minutes: 5

  - name: aggregate_results
    type: deferred_convergence
    dependencies:
      - process_batch
    handler:
      callable: SimpleBatchAggregator
      initialization: {}
```

---

### Phase 4: E2E Retry Mechanics Tests

**File**: `tests/e2e/rust/retry_mechanics_test.rs`

```rust
//! E2E tests proving retry mechanics work through full orchestration loop

#[tokio::test]
async fn test_retry_after_transient_failure() -> Result<()> {
    // Creates task with FailNTimesHandler(fail_count=2)
    // Waits for task completion (should succeed on 3rd attempt)
    // Validates step has attempts = 3 in final state
}

#[tokio::test]
async fn test_retry_exhaustion_leads_to_error() -> Result<()> {
    // Creates task with FailNTimesHandler(fail_count=10) but max_attempts=3
    // Waits for task to enter Error/BlockedByFailures state
    // Validates step exhausted all retries
}

#[tokio::test]
async fn test_waiting_for_retry_state_transition() -> Result<()> {
    // Creates task with retryable handler
    // Monitors state transitions through polling
    // Validates: InProgress → WaitingForRetry → Pending → InProgress
}

#[tokio::test]
async fn test_backoff_timing_respected() -> Result<()> {
    // Creates task with known backoff config
    // Measures time between retry attempts
    // Validates exponential increase within tolerance
}

#[tokio::test]
async fn test_mixed_workflow_with_retries() -> Result<()> {
    // Diamond workflow where one branch fails twice then succeeds
    // Other branch succeeds immediately
    // Validates convergence waits for retry completion
}
```

---

### Phase 5: E2E Batch Resumption Tests

**File**: `tests/e2e/rust/batch_resumption_test.rs`

```rust
//! E2E tests proving batch cursor-based resumption works

#[tokio::test]
async fn test_batch_worker_resumes_from_checkpoint() -> Result<()> {
    // Creates batch task with CheckpointAndFailHandler
    // Worker processes 50 items, checkpoints, fails
    // On retry, worker resumes from checkpoint
    // Validates total items processed = expected (no duplicates)
}

#[tokio::test]
async fn test_checkpoint_preservation_across_retry() -> Result<()> {
    // Validates results.batch_cursor preserved by ResetForRetry
    // Inspects workflow step results before and after retry
}

#[tokio::test]
async fn test_multiple_batch_workers_partial_failure() -> Result<()> {
    // 3 batch workers: one fails mid-processing, others succeed
    // Failed worker retries and completes
    // Convergence aggregates all results correctly
}

#[tokio::test]
async fn test_batch_no_duplicate_processing() -> Result<()> {
    // Specifically validates that resumed worker doesn't reprocess
    // items from before the checkpoint
}
```

---

### Phase 6: Integration Test Enhancements

**Directory**: `tests/integration/retry_mechanics/`

Lower-level tests using `LifecycleTestManager`:

#### 6.1 retry_state_machine_tests.rs

```rust
//! Tests retry state machine transitions directly

#[sqlx::test]
async fn test_waiting_for_retry_to_pending_on_backoff_expiry(pool: PgPool) -> Result<()> {
    // Create step in WaitingForRetry state
    // Set backoff to expire immediately
    // Verify SQL function returns step as ready
}

#[sqlx::test]
async fn test_retry_eligible_false_after_exhaustion(pool: PgPool) -> Result<()> {
    // Create step with attempts = max_attempts
    // Verify retry_eligible = false in readiness status
}
```

#### 6.2 backoff_calculation_tests.rs

```rust
//! Tests backoff timing calculations

#[test]
fn test_exponential_backoff_formula() {
    // Validate: delay = base * multiplier^attempts
}

#[test]
fn test_max_backoff_cap() {
    // Validate backoff never exceeds max_backoff_ms
}

#[test]
fn test_jitter_application() {
    // Validate jitter adds randomness within bounds
}
```

#### 6.3 cursor_preservation_tests.rs

```rust
//! Tests cursor preservation through retry cycle

#[sqlx::test]
async fn test_reset_for_retry_preserves_results(pool: PgPool) -> Result<()> {
    // Create step with results containing batch_cursor
    // Execute ResetForRetry action
    // Verify results.batch_cursor still present
}

#[sqlx::test]
async fn test_cursor_serialization_roundtrip(pool: PgPool) -> Result<()> {
    // Write various cursor types to results
    // Read back and verify data integrity
}
```

---

## File Summary

### New Files

```
docs/ticket-specs/TAS-64/
└── plan.md                                    # This plan

workers/rust/src/step_handlers/error_injection/
├── mod.rs
├── fail_n_times_handler.rs
├── checkpoint_and_fail_handler.rs
└── conditional_error_handler.rs

tests/fixtures/task_templates/rust/
├── retry_mechanics_test.yaml
├── retry_exhaustion_test.yaml
└── batch_resumption_test.yaml

tests/e2e/rust/
├── retry_mechanics_test.rs
└── batch_resumption_test.rs

tests/integration/retry_mechanics/
├── mod.rs
├── retry_state_machine_tests.rs
├── backoff_calculation_tests.rs
└── cursor_preservation_tests.rs
```

### Modified Files

- `config/tasker/environments/test/orchestration.toml` - Fast polling config
- `workers/rust/src/step_handlers/mod.rs` - Add error_injection module
- `tests/e2e/rust/mod.rs` - Add new test modules
- `tests/integration/mod.rs` - Add retry_mechanics module

---

## Success Criteria

| Criterion | Validation |
|-----------|------------|
| E2E test proves step retries after transient failure | `test_retry_after_transient_failure` passes |
| E2E test proves retry exhaustion leads to error | `test_retry_exhaustion_leads_to_error` passes |
| E2E test proves batch worker resumes from checkpoint | `test_batch_worker_resumes_from_checkpoint` passes |
| E2E test proves no duplicate processing | `test_batch_no_duplicate_processing` passes |
| Integration tests validate backoff timing | `backoff_calculation_tests` pass |
| Tests run with fast config (<5s per scenario) | All tests complete in reasonable time |
| Tests work in Docker and native execution | CI pipeline passes |

---

## Execution Order

1. **Phase 1**: Test configuration (enables fast test cycles)
2. **Phase 2**: Error injection handlers (prerequisite for all retry tests)
3. **Phase 3**: Task templates (define test workflows)
4. **Phase 4**: E2E retry tests (proves core retry works)
5. **Phase 5**: E2E batch resumption tests (proves cursor resumption)
6. **Phase 6**: Integration test enhancements (deeper validation)

---

## Dependencies

- TAS-49 (DLQ System) - `ResetForRetry` preserves results ✅
- TAS-59 (Batch Processing) - Cursor infrastructure ✅
- TAS-42 (WaitingForRetry state) - State machine support ✅

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Polling timing in tests is flaky | Use generous timeouts with fast polling |
| Cursor checkpoint format varies | Standardize checkpoint schema |
| Handler registration not automatic | Ensure handlers registered in mod.rs |

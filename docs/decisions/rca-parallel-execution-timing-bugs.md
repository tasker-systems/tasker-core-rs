# RCA: Parallel Execution Exposing Latent Timing Bugs

**Date**: 2025-12-07
**Related Ticket**: TAS-67 (Rust Worker Dual Event System)
**Status**: Resolved
**Impact**: Flaky E2E test `test_mixed_workflow_scenario`

---

## Executive Summary

During TAS-67 implementation (fire-and-forget handler dispatch), a previously hidden bug in the SQL function `get_task_execution_context()` became consistently reproducible. The bug was a **logical precedence error** that had always existed but was masked by sequential execution timing. Introducing true parallelism changed the probability distribution of state combinations, transforming a Heisenbug into a Bohrbug.

This document captures the root cause analysis as a reference for understanding how architectural changes to concurrency can surface latent bugs in distributed systems.

---

## The Bug

### Symptom

Test `test_mixed_workflow_scenario` intermittently failed with timeout waiting for `BlockedByFailures` status, while the API returned `HasReadySteps`.

```
⏳ Waiting for task to fail (max 10s)...
   Task execution status: processing (processing)
   Task execution status: has_ready_steps (has_ready_steps)  ← Wrong!
   Task execution status: has_ready_steps (has_ready_steps)
   ... timeout ...
```

### Root Cause

The SQL function `get_task_execution_context()` checked `ready_steps > 0` BEFORE `permanently_blocked_steps > 0`:

```sql
-- BUGGY: Wrong precedence order
CASE
  WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'           -- ← Checked FIRST
  WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked_by_failures'
  ...
END as execution_status
```

When a task had BOTH permanently blocked steps AND ready steps, the function returned `has_ready_steps` instead of `blocked_by_failures`.

### The Fix

Migration `20251207000000_fix_execution_status_priority.sql` corrects the precedence:

```sql
-- FIXED: blocked_by_failures takes semantic priority
CASE
  WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked_by_failures'  -- ← Now FIRST
  WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
  ...
END as execution_status
```

---

## Why Did This Surface Now?

### The Test Scenario

```yaml
# 3 parallel steps with NO dependencies (can all run concurrently)
steps:
  - name: success_step
    retryable: false

  - name: permanent_error_step
    retryable: false          # Fails permanently

  - name: retryable_error_step
    retryable: true
    max_attempts: 2           # Fails, but becomes "ready" after backoff
```

### Before TAS-67: Blocking Handler Dispatch

The pre-TAS-67 architecture used blocking `.call()` in the event handler:

```rust
// workers/rust/src/event_handler.rs (before)
let result = handler.call(&step).await;  // BLOCKS until handler completes
```

This created **effectively sequential execution** even for independent steps:

```
Timeline (Sequential):
────────────────────────────────────────────────────────────────────

t=0ms     [success_step starts]
t=50ms    [success_step completes]
t=51ms    [permanent_error_step starts]
t=100ms   [permanent_error_step fails → PERMANENTLY BLOCKED]
t=101ms   [retryable_error_step starts]
t=150ms   [retryable_error_step fails → enters 100ms backoff]
t=151ms   ──► STATUS CHECK
              permanently_blocked_steps = 1
              ready_steps = 0 (still in backoff!)
              ──► Returns: blocked_by_failures ✓

The backoff hadn't elapsed yet because steps were processed one at a time.
```

### After TAS-67: Fire-and-Forget Handler Dispatch

TAS-67 introduced non-blocking dispatch via channels:

```rust
// Fire-and-forget pattern
dispatch_sender.send(DispatchHandlerMessage { step, ... }).await;
// Returns immediately - handler executes in separate task
```

This enables **true parallel execution**:

```
Timeline (Parallel):
────────────────────────────────────────────────────────────────────

t=0ms     [success_step starts]──────────────────►[completes t=50ms]
t=0ms     [permanent_error_step starts]──────────►[fails t=50ms → BLOCKED]
t=0ms     [retryable_error_step starts]──────────►[fails t=50ms → backoff]

t=150ms   [retryable_error_step backoff expires → becomes READY]

t=151ms   ──► STATUS CHECK
              permanently_blocked_steps = 1
              ready_steps = 1 (backoff elapsed!)
              ──► Returns: has_ready_steps ✗ (BUG!)
```

---

## Probability Analysis

### The "Both States" Window

The bug manifests when checking status while the task has BOTH:
- At least one permanently blocked step
- At least one ready step (e.g., retryable step after backoff)

```
Sequential Processing:
├────────────────────────────────────────────────────────────────────┤
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
│ Very LOW probability of "both states" window                       │
│ Steps complete serially; backoff rarely overlaps with status check │
└────────────────────────────────────────────────────────────────────┘

Parallel Processing:
├────────────────────────────────────────────────────────────────────┤
│░░░░░░░░░░░░████████████████████████████████████████████░░░░░░░░░░░│
│            ↑                                          ↑            │
│            │ HIGH probability "both states" window    │            │
│            │ All steps complete ~simultaneously       │            │
│            │ Backoff expires while status is polled  │            │
└────────────────────────────────────────────────────────────────────┘
```

### Quantifying the Change

| Metric | Sequential | Parallel |
|--------|------------|----------|
| Step completion spread | ~150ms | ~50ms |
| "Both states" window duration | ~0ms (transient) | ~100ms+ (stable) |
| Probability of hitting bug | <1% | >50% |
| Bug classification | Heisenbug | Bohrbug |

---

## Bug Classification

### Heisenbug → Bohrbug Transformation

| Property | Before (Heisenbug) | After (Bohrbug) |
|----------|-------------------|-----------------|
| **Reproducibility** | Intermittent, timing-dependent | Consistent, deterministic |
| **Root cause** | Logical precedence error | Same |
| **Visibility** | Hidden by sequential timing | Exposed by parallel timing |
| **Debug difficulty** | Extremely hard (may never reproduce) | Straightforward |
| **Detection in CI** | Might pass for months | Fails consistently under load |

### Why This Matters

1. **The bug was always present** - It existed in the SQL function since it was written
2. **Sequential execution hid it** - Incidental timing prevented the problematic state
3. **Parallelization surfaced it** - Not by introducing a bug, but by applying concurrency pressure
4. **This is good** - Better to find in tests than production

---

## Semantic Correctness

### The Correct Mental Model

> "If ANY step is permanently blocked, the task cannot make further progress toward completion, even if other steps are ready to execute."

A task with permanent failures is **blocked by failures** regardless of what else might be runnable. The old code implicitly assumed:

> "If work is available, we're making progress"

This is **incorrect** for workflows where:
- Convergence points require ALL branches to complete
- Final task status depends on ALL steps succeeding
- Partial progress doesn't constitute overall success

### State Precedence (Correct Order)

```sql
-- 1. Permanent failures block overall progress
WHEN permanently_blocked_steps > 0 THEN 'blocked_by_failures'

-- 2. Ready work can continue (but may not lead to completion)
WHEN ready_steps > 0 THEN 'has_ready_steps'

-- 3. Work in flight
WHEN in_progress_steps > 0 THEN 'processing'

-- 4. All done
WHEN completed_steps = total_steps THEN 'all_complete'

-- 5. Waiting for dependencies
ELSE 'waiting_for_dependencies'
```

---

## Patterns to Watch For

### 1. State Combination Explosions

Sequential processing often means only one state at a time. Parallelism creates state combinations that were previously impossible:

```
Sequential: A → B → C (states are mutually exclusive in time)
Parallel:   A + B + C (states can coexist)
```

**Watch for**: CASE statements, if/else chains, and state machines that assume mutual exclusivity.

### 2. Timing-Dependent Invariants

Code may accidentally depend on timing:

```rust
// Assumes step_a completes before step_b starts
if step_a.is_complete() {
    // Safe to check step_b
}
```

**Watch for**: Implicit ordering assumptions in status calculations, rollups, and aggregations.

### 3. Transient vs Stable States

Some states were transient under sequential processing but become stable under parallel:

| State | Sequential | Parallel |
|-------|------------|----------|
| "1 complete, 1 in-progress" | Transient (~ms) | Stable (seconds) |
| "blocked + ready" | Nearly impossible | Common |
| "multiple errors" | Rare | Frequent |

**Watch for**: Error handling, status rollups, and progress calculations that assumed single-state scenarios.

### 4. Test Timing Sensitivity

Tests written for sequential execution may have implicit timing dependencies:

```rust
// This worked when steps were sequential
wait_for_status(BlockedByFailures, timeout: 10s);

// But fails when parallel execution creates a different status first
```

**Watch for**: Tests that pass in isolation but fail under concurrent load.

---

## Verification Strategy

### After Parallelization Changes

1. **Run tests multiple times** - Timing bugs may not manifest on first run
2. **Run tests under load** - Concurrent test execution increases probability
3. **Add explicit state combination tests** - Test scenarios that were previously impossible
4. **Review CASE/if-else precedence** - Check all status calculations for correct ordering

### Example: Testing State Combinations

```rust
#[tokio::test]
async fn test_blocked_with_ready_steps() {
    // Explicitly create the state combination
    let task = create_task_with_parallel_steps();

    // Force one step to permanent failure
    force_step_to_permanent_failure(&task, "step_a").await;

    // Force another step to ready (after backoff)
    force_step_to_ready_after_backoff(&task, "step_b").await;

    // Verify correct precedence
    let status = get_task_execution_status(&task).await;
    assert_eq!(status, ExecutionStatus::BlockedByFailures);
}
```

---

## Conclusion

This bug exemplifies how **architectural improvements to concurrency can surface latent correctness issues**. The TAS-67 parallelization didn't introduce a bug—it revealed one that had been hidden by incidental sequential timing.

This is a **positive outcome**: the bug was found in testing rather than production. The fix ensures correct semantic precedence regardless of execution timing, making the system more robust under parallel load.

### Key Takeaways

1. **Parallelization is a stress test** - It exposes timing-dependent bugs
2. **Sequential execution hides bugs** - Incidental ordering masks logical errors
3. **State precedence matters** - Review all status calculations when adding concurrency
4. **Heisenbugs become Bohrbugs** - Parallel execution makes rare bugs reproducible
5. **This is good engineering** - Finding bugs through architectural improvements validates the testing strategy

---

## References

- **Migration**: `migrations/20251207000000_fix_execution_status_priority.sql`
- **Test**: `tests/e2e/ruby/error_scenarios_test.rs::test_mixed_workflow_scenario`
- **SQL Function**: `get_task_execution_context()` in `migrations/20251001000000_fix_permanently_blocked_detection.sql`
- **TAS-67**: See [TAS-67 ADR](./TAS-67-dual-event-system.md)

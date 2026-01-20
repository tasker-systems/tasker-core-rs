# TAS-73: Atomic Task Finalization Design

**Status:** Design Proposal
**Priority:** P2 (Operational improvement, not correctness issue)
**Related:** completion_handler.rs, task_transition.rs

---

## Problem Statement

When multiple orchestrators detect that all steps of a task are complete, they may simultaneously attempt to finalize the task. The current implementation:

1. **Works correctly** - State machine guards prevent invalid transitions
2. **Is inelegant** - Second orchestrator gets a `StateTransitionFailed` error instead of graceful return

### Current Flow

```rust
// completion_handler.rs:56-93
let current_state = state_machine.current_state().await?;  // ← Query (no lock)

if current_state == TaskState::Complete {
    return Ok(...);  // Already done
}

// ← GAP: Another orchestrator can be here too

state_machine.transition(TaskEvent::AllStepsSuccessful).await?;  // ← Transition
```

### Race Scenario

```
T0: Orchestrator A: current_state() → EvaluatingResults
T0: Orchestrator B: current_state() → EvaluatingResults
T1: A: transition() → Complete (succeeds)
T2: B: transition() → StateTransitionFailed (current state is now Complete)
```

Result: B gets an error logged, even though the task completed successfully.

---

## Previous Attempt: Claim-Based

A previous implementation tried using explicit claim columns:

```sql
-- NOT RECOMMENDED
ALTER TABLE tasker.tasks ADD COLUMN finalization_claimed_at TIMESTAMP;
ALTER TABLE tasker.tasks ADD COLUMN finalization_claimed_by UUID;
```

**Problem:** If orchestrator A claims the task then crashes before completing, the claim is stuck. Orchestrator B cannot finalize because A "owns" it.

**Required mitigations (complex):**
- Claim timeout logic
- Cleanup background job
- Edge cases around timeout expiry during processing

---

## Proposed Solution: Transaction-Based Locking

Use PostgreSQL's row-level locking within a transaction to coordinate finalization atomically.

### Core Concept

```rust
pub async fn complete_task_atomic(
    &self,
    task: Task,
    context: Option<TaskExecutionContext>,
    correlation_id: Uuid,
) -> Result<FinalizationResult, FinalizationError> {
    let pool = self.context.database_pool();
    let mut tx = pool.begin().await?;

    // Step 1: Acquire exclusive lock on task row
    // Other orchestrators will BLOCK here, not error
    sqlx::query!(
        "SELECT 1 FROM tasker.tasks WHERE task_uuid = $1 FOR UPDATE",
        task.task_uuid
    )
    .fetch_one(&mut *tx)
    .await?;

    // Step 2: Check current state (while holding lock)
    let current_state = self.get_current_state_with_tx(&mut tx, task.task_uuid).await?;

    // Step 3: If already complete, release lock and return gracefully
    if current_state == TaskState::Complete {
        tx.rollback().await?;  // Explicit rollback releases lock immediately
        return Ok(FinalizationResult {
            task_uuid: task.task_uuid,
            action: FinalizationAction::Completed,
            // ... other fields
        });
    }

    // Step 4: Perform transition within same transaction
    // Use TaskTransition::create_with_transaction() to stay in our tx
    self.transition_with_tx(&mut tx, task.task_uuid, TaskEvent::AllStepsSuccessful).await?;

    // Step 5: Commit releases the lock
    tx.commit().await?;

    Ok(FinalizationResult {
        task_uuid: task.task_uuid,
        action: FinalizationAction::Completed,
        // ... other fields
    })
}
```

### Concurrent Scenario (Fixed)

```
T0: Orchestrator A: BEGIN; SELECT ... FOR UPDATE (acquires lock)
T0: Orchestrator B: BEGIN; SELECT ... FOR UPDATE (BLOCKS, waiting for A's lock)
T1: A: Check state → EvaluatingResults
T2: A: Transition to Complete
T3: A: COMMIT (releases lock)
T4: B: (unblocks, acquires lock)
T5: B: Check state → Complete
T6: B: ROLLBACK, return Ok(Completed)  ← Graceful, no error!
```

---

## Implementation Details

### Required Changes

1. **Add helper method for state resolution within transaction:**

```rust
// In completion_handler.rs or a new atomic_finalization.rs module
async fn get_current_state_with_tx(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_uuid: Uuid,
) -> Result<TaskState, FinalizationError> {
    let row = sqlx::query!(
        r#"
        SELECT to_state
        FROM tasker.task_transitions
        WHERE task_uuid = $1 AND most_recent = true
        "#,
        task_uuid
    )
    .fetch_optional(&mut **tx)
    .await?;

    match row {
        Some(r) => r.to_state.parse().map_err(|_| FinalizationError::StateMachine {
            error: "Invalid state in database".to_string(),
            task_uuid,
        }),
        None => Ok(TaskState::Pending),
    }
}
```

2. **Add transition method that uses external transaction:**

```rust
async fn transition_with_tx(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_uuid: Uuid,
    event: TaskEvent,
) -> Result<(), FinalizationError> {
    // Determine target state based on current state and event
    let current_state = self.get_current_state_with_tx(tx, task_uuid).await?;
    let target_state = determine_target_state(current_state, &event)?;

    // Create transition using existing create_with_transaction
    let new_transition = NewTaskTransition {
        task_uuid,
        to_state: target_state.to_string(),
        from_state: Some(current_state.to_string()),
        processor_uuid: Some(self.context.processor_uuid()),
        metadata: Some(serde_json::json!({
            "event": format!("{:?}", event),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    };

    TaskTransition::create_with_transaction(tx, new_transition)
        .await
        .map_err(|e| FinalizationError::StateMachine {
            error: format!("Failed to create transition: {e}"),
            task_uuid,
        })?;

    Ok(())
}
```

3. **Update complete_task to use atomic version:**

The existing `complete_task` method can either:
- Be replaced with the atomic version
- Call the atomic version internally
- Remain as fallback with atomic version as opt-in

### Transaction Timeout Considerations

PostgreSQL has `statement_timeout` and `lock_timeout` settings:

```sql
-- Per-transaction timeout (optional safety net)
SET LOCAL lock_timeout = '5s';
```

If lock cannot be acquired within timeout, the query fails with a clear error. This prevents indefinite blocking if something goes wrong.

### No Schema Changes Required

Unlike the claim-based approach, this solution requires no database schema changes. It uses PostgreSQL's built-in transaction and locking semantics.

---

## Comparison: Claim-Based vs Transaction-Based

| Aspect | Claim-Based | Transaction-Based |
|--------|-------------|-------------------|
| Schema changes | Yes (2 columns) | No |
| Crash recovery | Manual (timeout + cleanup) | Automatic (tx rollback) |
| Second orchestrator behavior | Error (claim exists) | Waits, then graceful return |
| Complexity | High (claim lifecycle) | Low (standard tx pattern) |
| Lock duration | Until explicit release | Until COMMIT/ROLLBACK |
| Visibility | Claim visible in DB | Lock not visible (pg_locks) |

---

## Testing Strategy

1. **Unit test:** Verify transition within transaction works correctly
2. **Integration test:** Two concurrent finalization attempts
3. **Multi-instance test:** N orchestrators, one task completion

### Example Test Scenario

```rust
#[tokio::test]
async fn test_concurrent_finalization_is_graceful() {
    // Setup: Create task in EvaluatingResults state

    // Spawn two concurrent finalization attempts
    let (result_a, result_b) = tokio::join!(
        handler.complete_task_atomic(task.clone(), None, Uuid::new_v4()),
        handler.complete_task_atomic(task.clone(), None, Uuid::new_v4()),
    );

    // Both should succeed (one does work, other returns gracefully)
    assert!(result_a.is_ok());
    assert!(result_b.is_ok());

    // Task should be in Complete state exactly once
    let final_state = get_task_state(task.task_uuid).await;
    assert_eq!(final_state, TaskState::Complete);

    // Only one transition to Complete should exist
    let transitions = TaskTransition::list_by_task(&pool, task.task_uuid).await?;
    let complete_transitions: Vec<_> = transitions
        .iter()
        .filter(|t| t.to_state == "complete")
        .collect();
    assert_eq!(complete_transitions.len(), 1);
}
```

---

## Implementation Priority

This is a **P2 improvement** because:

1. Current behavior is **correct** (no data corruption)
2. Issue is **operational** (logged errors, not failures)
3. Impact scales with number of orchestrators

Recommend implementing after:
- Identity hash strategy (P1)
- Multi-instance test infrastructure (P0 for validation)

Once multi-instance tests are running, we can observe the frequency of these race conditions and prioritize accordingly.

---

## References

- `tasker-orchestration/src/orchestration/lifecycle/task_finalization/completion_handler.rs`
- `tasker-shared/src/models/core/task_transition.rs:246-297` (`create_with_transaction`)
- `docs/architecture/idempotency-and-atomicity.md:616-660` (previous design discussion)
- PostgreSQL documentation: [Explicit Locking](https://www.postgresql.org/docs/current/explicit-locking.html)

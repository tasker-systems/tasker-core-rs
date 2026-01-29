# Defense in Depth

This document describes Tasker Core's multi-layered protection model for idempotency and data integrity.

## The Four Protection Layers

Tasker Core uses four independent protection layers. Each layer catches what others might miss, and no single layer bears full responsibility for data integrity.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Layer 4: Application Logic                   │
│                    (State-based deduplication)                  │
├─────────────────────────────────────────────────────────────────┤
│                    Layer 3: Transaction Boundaries              │
│                    (All-or-nothing semantics)                   │
├─────────────────────────────────────────────────────────────────┤
│                    Layer 2: State Machine Guards                │
│                    (Current state validation)                   │
├─────────────────────────────────────────────────────────────────┤
│                    Layer 1: Database Atomicity                  │
│                    (Unique constraints, row locks, CAS)         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Layer 1: Database Atomicity

The foundation layer using PostgreSQL's transactional guarantees.

### Mechanisms

| Mechanism | Purpose | Example |
|-----------|---------|---------|
| Unique constraints | Prevent duplicate records | One active task per (namespace, external_id) |
| Row-level locking | Prevent concurrent modification | `SELECT ... FOR UPDATE` on task claim |
| Compare-and-swap | Atomic state transitions | `UPDATE ... WHERE state = $expected` |
| Advisory locks | Distributed coordination | Template cache invalidation |

### Atomic Claiming Pattern

```sql
-- Only one processor can claim a task
UPDATE tasks
SET state = 'in_progress',
    processor_uuid = $1,
    claimed_at = NOW()
WHERE id = $2
  AND state = 'pending'  -- CAS: only if still pending
RETURNING *;
```

If two processors try to claim the same task:
- First: Succeeds, task transitions to `in_progress`
- Second: Fails (0 rows affected), no state change

### Why This Works

PostgreSQL's MVCC ensures the `WHERE state = 'pending'` check and the `SET state = 'in_progress'` happen atomically. There's no window where both processors see `state = 'pending'`.

---

## Layer 2: State Machine Guards

State machine validation before any transition is attempted.

### Implementation

```rust
impl TaskStateMachine {
    pub fn can_transition(&self, from: TaskState, to: TaskState) -> bool {
        VALID_TRANSITIONS.contains(&(from, to))
    }

    pub fn transition(&mut self, to: TaskState) -> Result<(), StateError> {
        if !self.can_transition(self.current, to) {
            return Err(StateError::InvalidTransition { from: self.current, to });
        }
        // Proceed with transition
    }
}
```

### Valid Transitions Matrix

The state machine explicitly defines which transitions are valid:

```
Pending → Initializing → EnqueuingSteps → StepsInProcess
                                              ↓
Complete ← EvaluatingResults ← (step completions)
                  ↓
               Error (from any state)
```

Invalid transitions are rejected before reaching the database.

### Why This Works

Application-level guards prevent obviously invalid operations from even attempting database changes. This reduces database load and provides better error messages.

---

## Layer 3: Transaction Boundaries

All-or-nothing semantics for multi-step operations.

### Example: Step Enqueueing

```rust
async fn enqueue_steps(task_id: TaskId, steps: Vec<Step>) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Insert all steps
    for step in steps {
        insert_step(&mut tx, task_id, &step).await?;
    }

    // Update task state
    update_task_state(&mut tx, task_id, TaskState::StepsInProcess).await?;

    // Atomic commit - all or nothing
    tx.commit().await?;
    Ok(())
}
```

If step insertion fails:
- Transaction rolls back
- Task state unchanged
- No partial steps created

### Why This Works

PostgreSQL transactions ensure that either all changes commit or none do. There's no intermediate state where some steps exist but task state is wrong.

---

## Layer 4: Application-Level Filtering

State-based deduplication in application logic.

### Example: Result Processing

```rust
async fn process_result(step_id: StepId, result: HandlerResult) -> Result<()> {
    let step = get_step(step_id).await?;

    // Filter: Only process if step is in_progress
    if step.state != StepState::InProgress {
        log::info!("Ignoring result for step {} in state {:?}", step_id, step.state);
        return Ok(()); // Idempotent: already processed
    }

    // Proceed with result processing
    apply_result(step, result).await
}
```

### Why This Works

Even if the same result arrives multiple times (network retries, duplicate messages), only the first processing has effect. Subsequent attempts see the step already transitioned and exit cleanly.

---

## The TAS-54 Discovery: Ownership Was Harmful

### What We Learned

TAS-54 analyzed processor UUID "ownership" enforcement:

```rust
// OLD: Ownership enforcement (REMOVED)
fn can_process(&self, processor_uuid: Uuid) -> bool {
    self.owner_uuid == processor_uuid  // BLOCKED recovery!
}

// NEW: Ownership tracking only (for audit)
fn process(&self, processor_uuid: Uuid) -> Result<()> {
    self.record_processor(processor_uuid);  // Track, don't enforce
    // ... proceed with processing
}
```

### Why Ownership Enforcement Was Removed

| Scenario | With Enforcement | Without Enforcement |
|----------|-----------------|---------------------|
| Normal operation | Works | Works |
| Orchestrator crash & restart | **BLOCKED** - new UUID | Automatic recovery |
| Duplicate message | Rejected | Layer 1 (CAS) rejects |
| Race condition | Rejected | Layer 1 (CAS) rejects |

The four protection layers already prevent corruption. Ownership added:
- **Zero additional safety** (layers 1-4 sufficient)
- **Recovery blocking** (crashed tasks stuck forever)
- **Operational complexity** (manual intervention needed)

### The Verdict

> "Processor UUID ownership was redundant protection with harmful side effects."

When two actors receive identical messages:
- First: Succeeds atomically (Layer 1 CAS)
- Second: Fails cleanly (Layer 1 CAS)
- No partial state, no corruption
- No ownership check needed

---

## Designing New Protections

When adding protection mechanisms, evaluate against this checklist:

### Before Adding Protection

1. **Which layer does this belong to?** (Database, state machine, transaction, application)
2. **What does it protect against?** (Be specific: race condition, duplicate, corruption)
3. **Do existing layers already cover this?** (Usually yes)
4. **What failure modes does it introduce?** (Blocked recovery, increased latency)
5. **Can the system recover if this protection itself fails?**

### The Minimal Set Principle

> Find the minimal set of protections that prevents corruption. Additional layers that prevent recovery are worse than none.

A system that:
- Has fewer protections
- Recovers automatically from crashes
- Handles duplicates idempotently

Is better than a system that:
- Has more protections
- Requires manual intervention after crashes
- Is "theoretically more secure"

---

---

## Relationship to Fail Loudly

Defense in Depth and [Fail Loudly](./fail-loudly.md) are complementary principles:

| Defense in Depth | Fail Loudly |
|------------------|-------------|
| Multiple layers prevent corruption | Errors surface problems immediately |
| Redundancy catches edge cases | Transparency enables diagnosis |
| Protection happens before damage | Visibility happens at detection |

Both reject the same anti-pattern: **silent failures**.

- Defense in Depth rejects: silent corruption (data changed without protection)
- Fail Loudly rejects: silent defaults (missing data hidden with fabricated values)

Together they ensure: if something goes wrong, we know about it—either protection prevents it, or an error surfaces it.

---

## Related Documentation

- [Tasker Core Tenets](./tasker-core-tenets.md) - Tenet #1: Defense in Depth, Tenet #11: Fail Loudly
- [Fail Loudly](./fail-loudly.md) - Errors as first-class citizens
- [Idempotency and Atomicity](../idempotency-and-atomicity.md) - Implementation details
- [States and Lifecycles](../states-and-lifecycles.md) - State machine specifications
- [TAS-54 ADR](../decisions/TAS-54-ownership-removal.md) - Processor UUID ownership removal decision

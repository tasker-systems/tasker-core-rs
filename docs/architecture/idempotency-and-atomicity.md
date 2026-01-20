# Idempotency and Atomicity Guarantees

**Last Updated**: 2025-01-19
**Audience**: Architects, Developers
**Status**: Active
**Related Docs**: [Documentation Hub](README.md) | [States and Lifecycles](states-and-lifecycles.md) | [Events and Commands](events-and-commands.md) | [Task Readiness & Execution](task-and-step-readiness-and-execution.md)

← Back to [Documentation Hub](README.md)

---

## Overview

Tasker Core is designed for distributed orchestration with multiple orchestrator instances processing tasks concurrently. This document explains the **defense-in-depth** approach that ensures safe concurrent operation without race conditions, data corruption, or lost work.

The system provides idempotency and atomicity through **four overlapping protection layers**:

1. **Database Atomicity**: PostgreSQL constraints, row locking, and compare-and-swap operations
2. **State Machine Guards**: Current-state validation before all transitions
3. **Transaction Boundaries**: All-or-nothing semantics for complex operations
4. **Application Logic**: State-based filtering and idempotent patterns

These layers work together to ensure that **operations can be safely retried**, **multiple orchestrators can process work concurrently**, and **crashes don't leave the system in an inconsistent state**.

---

## Core Protection Mechanisms

### Layer 1: Database Atomicity

PostgreSQL provides fundamental atomic guarantees through several mechanisms:

#### Unique Constraints

**Purpose**: Prevent duplicate creation of entities

**Key Constraints**:
- `tasker.tasks.identity_hash` (UNIQUE) - Prevents duplicate task creation from identical requests
- `tasker.task_namespaces.name` (UNIQUE) - Namespace name uniqueness
- `tasker.named_tasks (namespace_id, name, version)` (UNIQUE) - Task template uniqueness
- `tasker.named_steps.system_name` (UNIQUE) - Step handler uniqueness

**Example Protection**:
```rust
// Two orchestrators receive identical TaskRequestMessage
// Orchestrator A creates task first -> commits successfully
// Orchestrator B attempts to create -> unique constraint violation
// Result: Exactly one task created, error cleanly handled
```

See [Task Initialization](#task-initialization-idempotency) for details on how this protects task creation.

#### Row-Level Locking

**Purpose**: Prevent concurrent modifications to the same database row

**Locking Patterns**:

1. **`FOR UPDATE`** - Exclusive lock, blocks concurrent transactions
   ```sql
   -- Used in: transition_task_state_atomic()
   SELECT * FROM tasker.tasks WHERE task_uuid = $1 FOR UPDATE;
   -- Blocks until transaction commits or rolls back
   ```

2. **`FOR UPDATE SKIP LOCKED`** - Lock-free work distribution
   ```sql
   -- Used in: get_next_ready_tasks()
   SELECT * FROM tasker.tasks
   WHERE state = ANY($1)
   FOR UPDATE SKIP LOCKED
   LIMIT $2;
   -- Each orchestrator gets different tasks, no blocking
   ```

**Example Protection**:
```rust
// Scenario: Two orchestrators attempt state transition on same task
// Orchestrator A: BEGIN; SELECT FOR UPDATE; UPDATE state; COMMIT;
// Orchestrator B: BEGIN; SELECT FOR UPDATE (BLOCKS until A commits)
//                 UPDATE fails due to state validation
// Result: Only one transition succeeds, no race condition
```

#### Compare-and-Swap Semantics

**Purpose**: Validate expected state before making changes

**Pattern**: All state transitions validate current state in the same transaction as the update
```sql
-- From transition_task_state_atomic()
UPDATE tasker.tasks
SET state = $new_state, updated_at = NOW()
WHERE task_uuid = $uuid
  AND state = $expected_current_state  -- Critical: CAS validation
RETURNING *;
```

**Example Protection**:
```rust
// Orchestrator A and B both think task is in "Pending" state
// A transitions: WHERE state = 'Pending' -> succeeds, now "Initializing"
// B transitions: WHERE state = 'Pending' -> returns 0 rows (fails gracefully)
// Result: Atomic transition, no invalid state
```

See [SQL Function Architecture](#sql-function-atomicity) for more on database-level guarantees.

### Layer 2: State Machine Guards

**Purpose**: Enforce valid state transitions through application-level validation

Both task and step state machines validate current state before allowing transitions. This provides protection even when database constraints alone wouldn't catch invalid operations.

#### Task State Machine

Defined in `tasker-shared/src/state_machine/task_state_machine.rs`, the TaskStateMachine validates:

1. **Current state retrieval**: Always fetch latest state from database
2. **Event applicability**: Check if event is valid for current state
3. **Terminal state protection**: Cannot transition from Complete/Error/Cancelled
4. **Ownership tracking**: Processor UUID tracked for audit (not enforced after TAS-54)

**Example Protection**:
```rust
// TaskStateMachine prevents invalid transitions
let mut state_machine = TaskStateMachine::new(task, context);

// Attempt to mark complete when still processing
let result = state_machine.transition(TaskEvent::MarkComplete).await;
// Result: Error - cannot mark complete while steps are in progress

// Current state validation prevents:
// - Completing tasks with pending steps
// - Re-initializing completed tasks
// - Transitioning from terminal states
```

See [States and Lifecycles](states-and-lifecycles.md) for complete state machine documentation.

#### Workflow Step State Machine

Defined in `tasker-shared/src/state_machine/step_state_machine.rs`, the StepStateMachine ensures:

1. **Execution claiming**: Only Pending/Enqueued steps can transition to InProgress
2. **Completion validation**: Only InProgress steps can be marked complete
3. **Retry eligibility**: Validates max_attempts and backoff timing

**Example Protection**:
```rust
// Worker attempts to claim already-processing step
let mut step_machine = StepStateMachine::new(step.into(), context);

match step_machine.current_state().await {
    WorkflowStepState::InProgress => {
        // Already being processed by another worker
        return Ok(false); // Cannot claim
    }
    WorkflowStepState::Pending | WorkflowStepState::Enqueued => {
        // Attempt atomic transition
        step_machine.transition(StepEvent::Start).await?;
    }
}
```

This prevents:
- Multiple workers executing the same step concurrently
- Marking steps complete that weren't started
- Retrying steps that exceeded max_attempts

### Layer 3: Transaction Boundaries

**Purpose**: Ensure all-or-nothing semantics for multi-step operations

Critical operations wrap multiple database changes in a single transaction, ensuring atomic completion or full rollback on failure.

#### Task Initialization Transaction

Task creation involves multiple dependent entities that must all succeed or all fail:

```rust
// From TaskInitializer.initialize_task()
let mut tx = pool.begin().await?;

// 1. Create or find namespace (find-or-create is idempotent)
let namespace = NamespaceResolver::resolve_namespace(&mut tx, namespace_name).await?;

// 2. Create or find named task
let named_task = NamespaceResolver::resolve_named_task(&mut tx, namespace, task_name).await?;

// 3. Create task record
let task = create_task(&mut tx, named_task.uuid, context).await?;

// 4. Create all workflow steps and edges
let (step_count, step_mapping) = WorkflowStepBuilder::create_workflow_steps(
    &mut tx, task.uuid, template
).await?;

// 5. Initialize state machine
StateInitializer::initialize_task_state(&mut tx, task.uuid).await?;

// ALL OR NOTHING: Commit entire transaction
tx.commit().await?;
```

**Example Protection**:
```rust
// Scenario: Task creation partially fails
// - Namespace created ✓
// - Named task created ✓
// - Task record created ✓
// - Workflow steps: Cycle detected ✗ (error thrown)
// Result: tx.rollback() -> ALL changes reverted, clean failure
```

#### Cycle Detection Enforcement

Workflow dependencies are validated during task initialization to prevent circular references:

```rust
// From WorkflowStepBuilder::create_step_dependencies()
for dependency in &step_definition.dependencies {
    let from_uuid = step_mapping[dependency];
    let to_uuid = step_mapping[&step_definition.name];

    // Check for self-reference
    if from_uuid == to_uuid {
        return Err(CycleDetected { from, to });
    }

    // Check for path that would create cycle
    if WorkflowStepEdge::would_create_cycle(pool, from_uuid, to_uuid).await? {
        return Err(CycleDetected { from, to });
    }

    // Safe to create edge
    WorkflowStepEdge::create_with_transaction(&mut tx, edge).await?;
}
```

This prevents invalid DAG structures from ever being persisted to the database.

### Layer 4: Application Logic Patterns

**Purpose**: Implement idempotent patterns at the application level

Beyond database and state machine protections, application code uses several patterns to ensure safe retry and concurrent operation.

#### Find-or-Create Pattern

Used for entities that should be unique but may be created by multiple orchestrators:

```rust
// From NamespaceResolver
pub async fn resolve_namespace(
    tx: &mut Transaction<'_, Postgres>,
    name: &str,
) -> Result<TaskNamespace> {
    // Try to find existing
    if let Some(namespace) = TaskNamespace::find_by_name(pool, name).await? {
        return Ok(namespace);
    }

    // Create if not found
    match TaskNamespace::create_with_transaction(tx, NewTaskNamespace { name }).await {
        Ok(namespace) => Ok(namespace),
        Err(sqlx::Error::Database(e)) if is_unique_violation(&e) => {
            // Another orchestrator created it between our find and create
            // Re-query to get the one that won the race
            TaskNamespace::find_by_name(pool, name).await?
                .ok_or(Error::NotFound)
        }
        Err(e) => Err(e),
    }
}
```

**Why This Works**:
- First attempt: Finds existing → idempotent
- Create attempt: Unique constraint prevents duplicates
- Retry after unique violation: Gets the winner → idempotent
- Result: Exactly one namespace, regardless of concurrent attempts

#### State-Based Filtering

Operations filter by state to naturally deduplicate work:

```rust
// From StepEnqueuerService
// Only enqueue steps in specific states
let ready_steps = steps.iter()
    .filter(|step| matches!(
        step.state,
        WorkflowStepState::Pending | WorkflowStepState::WaitingForRetry
    ))
    .collect();

// Skip steps already:
// - Enqueued (another orchestrator handled it)
// - InProgress (worker is executing)
// - Complete (already done)
// - Error (terminal state)
```

**Example Protection**:
```rust
// Scenario: Orchestrator crash mid-batch
// Before crash: Enqueued steps 1-5 of 10
// After restart: Process task again
// State filtering:
//   - Steps 1-5: state = Enqueued → skip
//   - Steps 6-10: state = Pending → enqueue
// Result: Each step enqueued exactly once
```

#### State-Before-Queue Pattern (TAS-29)

Ensures workers only see steps in correct state:

```rust
// 1. Commit state transition to database FIRST
step_state_machine.transition(StepEvent::Enqueue).await?;
// Step now in Enqueued state in database

// 2. THEN send PGMQ notification
pgmq_client.send_with_notify(queue_name, step_message).await?;

// Worker receives notification and:
// - Queries database for step
// - Sees state = Enqueued (committed)
// - Can safely claim and execute
```

**Why Order Matters**:
```rust
// Wrong order (queue-before-state):
// 1. Send PGMQ message
// 2. Worker receives immediately
// 3. Worker queries database → state still Pending
// 4. Worker might skip or fail to claim
// 5. State transition commits

// Correct order (state-before-queue):
// 1. State transition commits
// 2. Send PGMQ message
// 3. Worker receives
// 4. Worker queries → state correctly Enqueued
// 5. Worker can claim
```

See [Events and Commands](events-and-commands.md) for event system details.

---

## Component-by-Component Guarantees

### Task Initialization Idempotency

**Component**: `TaskRequestActor` and `TaskInitializer` service
**Operation**: Creating a new task from a template
**File**: `tasker-orchestration/src/orchestration/lifecycle/task_initialization/`

#### Protection Mechanisms

1. **Identity Hash Unique Constraint**
   ```rust
   // Tasks are identified by hash of (namespace, task_name, context)
   let identity_hash = calculate_identity_hash(namespace, name, context);

   NewTask {
       identity_hash,  // Unique constraint prevents duplicates
       named_task_uuid,
       context,
       // ...
   }
   ```

2. **Transaction Atomicity**
   - All entities created in single transaction
   - Namespace, named task, task, workflow steps, edges
   - Cycle detection validates DAG before committing
   - Any failure rolls back everything

3. **Find-or-Create for Shared Entities**
   - Namespaces can be created by any orchestrator
   - Named tasks shared across workflow instances
   - Named steps reused across tasks

#### Concurrent Scenario

**Two orchestrators receive identical TaskRequestMessage**:

```
T0: Orchestrator A begins transaction
T1: Orchestrator B begins transaction
T2: A creates namespace "payments"
T3: B attempts to create namespace "payments"
T4: A creates task with identity_hash "abc123"
T5: B attempts to create task with identity_hash "abc123"
T6: A commits successfully ✓
T7: B attempts commit → unique constraint violation on identity_hash
T8: B transaction rolled back
```

**Result**:
- Exactly one task created
- No partial state in database
- Orchestrator B receives clear error
- Retry-safe: B can check if task exists and return it

#### Cycle Detection

Prevents invalid workflow definitions:

```rust
// Template defines: A depends on B, B depends on C, C depends on A
// During initialization:
//   - Create steps A, B, C
//   - Create edge A -> B (valid)
//   - Create edge B -> C (valid)
//   - Attempt edge C -> A
//     - would_create_cycle() returns true
//     - Error: CycleDetected
//   - Transaction rolled back
// Result: Invalid workflow rejected, no partial data
```

See `tasker-shared/src/models/core/workflow_step_edge.rs:236-270` for cycle detection implementation.

### Step Enqueueing Idempotency

**Component**: `StepEnqueuerActor` and `StepEnqueuerService`
**Operation**: Enqueueing ready workflow steps to worker queues
**File**: `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer_services/`

#### Multi-Layer Protection

1. **SQL-Level Row Locking**
   ```sql
   -- get_next_ready_tasks() uses SKIP LOCKED
   SELECT task_uuid FROM tasker.tasks
   WHERE state = ANY($states)
   FOR UPDATE SKIP LOCKED  -- Prevents concurrent claiming
   LIMIT $batch_size;
   ```
   Each orchestrator gets different tasks, no overlap

2. **State Machine Compare-and-Swap**
   ```rust
   // Only transition if task in expected state
   state_machine.transition(TaskEvent::EnqueueSteps(uuids)).await?;
   // Fails if another orchestrator already transitioned
   ```

3. **Step State Filtering**
   ```rust
   // Only enqueue steps in specific states
   let enqueueable = steps.filter(|s| matches!(
       s.state,
       WorkflowStepState::Pending | WorkflowStepState::WaitingForRetry
   ));
   ```

4. **State-Before-Queue Ordering**
   ```rust
   // 1. Commit step state to Enqueued
   step.transition(StepEvent::Enqueue).await?;

   // 2. Send PGMQ message
   pgmq.send_with_notify(queue, message).await?;
   ```

#### Concurrent Scenario

**Two orchestrators discover the same ready steps**:

```
T0: Orchestrator A queries get_next_ready_tasks(batch=100)
T1: Orchestrator B queries get_next_ready_tasks(batch=100)
T2: A gets tasks [1,2,3] (locked by A's transaction)
T3: B gets tasks [4,5,6] (different rows, SKIP LOCKED)
T4: A enqueues steps for tasks 1,2,3
T5: B enqueues steps for tasks 4,5,6
T6: Both commit successfully
```

**Result**: No overlap, each task processed once

**Orchestrator Crash Mid-Batch**:

```
T0: Orchestrator A gets task 1 with steps [A, B, C, D]
T1: A enqueues steps A, B to "payments_queue"
T2: A crashes before processing steps C, D
T3: Task 1 state still EnqueuingSteps
T4: Orchestrator B picks up task 1 (A's transaction rolled back)
T5: B queries steps for task 1
T6: Steps A, B have state = Enqueued → skip
T7: Steps C, D have state = Pending → enqueue
```

**Result**: Steps A, B enqueued once, C, D recovered and enqueued

### Result Processing Idempotency

**Component**: `ResultProcessorActor` and `OrchestrationResultProcessor`
**Operation**: Processing step execution results from workers
**File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/`

#### Protection Mechanisms

1. **State Guard Validation**
   ```rust
   // TaskCoordinator validates step state before processing result
   let current_state = step_state_machine.current_state().await?;

   match current_state {
       WorkflowStepState::InProgress => {
           // Valid: step is being processed
           step_state_machine.transition(StepEvent::Complete).await?;
       }
       WorkflowStepState::Complete => {
           // Idempotent: already processed this result
           return Ok(AlreadyComplete);
       }
       _ => {
           // Invalid state for result processing
           return Err(InvalidState);
       }
   }
   ```

2. **Atomic State Transitions**
   - Step result processing uses compare-and-swap
   - Task state transitions validate current state
   - All updates in same transaction as state check

3. **TAS-54: Ownership Removed**
   - Processor UUID tracked for audit only
   - Not enforced for transitions
   - Any orchestrator can process results
   - Enables recovery after crashes

#### Concurrent Scenario

**Worker submits result, orchestrator crashes, retry arrives**:

```
T0: Worker completes step A, submits result to orchestration_step_results queue
T1: Orchestrator A pulls message, begins processing
T2: A transitions step A to Complete
T3: A begins task state evaluation
T4: A crashes before deleting PGMQ message
T5: PGMQ visibility timeout expires → message reappears
T6: Orchestrator B pulls same message
T7: B queries step A state → Complete
T8: B returns early (idempotent, already processed)
T9: B deletes PGMQ message
```

**Result**: Step processed exactly once, retry is harmless

**Before TAS-54 (Ownership Enforced)**:
```
// Orchestrator A owned task in EvaluatingResults state
// A crashes
// B receives retry
// B checks: task.processor_uuid != B.uuid
// Error: Ownership violation → TASK STUCK
```

**After TAS-54 (Ownership Audit-Only)**:
```
// Orchestrator A owned task in EvaluatingResults state
// A crashes
// B receives retry
// B checks: current task state (no ownership check)
// B processes successfully → TASK RECOVERS
```

See [TAS-54 ADR](../decisions/TAS-54-ownership-removal.md) for full analysis.

### Task Finalization Idempotency

**Component**: `TaskFinalizerActor` and `TaskFinalizer` service
**Operation**: Finalizing task to terminal state
**File**: `tasker-orchestration/src/orchestration/lifecycle/task_finalization/`

#### Current Protection (Sufficient for Recovery)

1. **State Guard Protection**
   ```rust
   // TaskFinalizer checks current task state
   let context = ExecutionContextProvider::fetch(task_uuid).await?;

   match context.should_finalize() {
       true => {
           // Transition to Complete
           task_state_machine.transition(TaskEvent::MarkComplete).await?;
       }
       false => {
           // Not ready to finalize (steps still pending)
           return Ok(NotReady);
       }
   }
   ```

2. **Idempotent for Recovery**
   ```rust
   // Scenario: Orchestrator crashes during finalization
   // - Task state already Complete → state guard returns early
   // - Task state still StepsInProcess → retry succeeds
   // Result: Recovery works, final state reached
   ```

#### Concurrent Scenario (Not Graceful)

**Two orchestrators attempt finalization simultaneously**:

```
T0: Orchestrators A and B both receive finalization trigger
T1: A checks: all steps complete → proceed
T2: B checks: all steps complete → proceed
T3: A transitions task to Complete (succeeds)
T4: B attempts transition to Complete
T5: State guard: task already Complete
T6: B receives StateMachineError (invalid transition)
```

**Result**:
- ✓ Task finalized exactly once (correct)
- ✓ No data corruption
- ⚠️ Orchestrator B gets error (not graceful)

#### Future Enhancement (TAS-37)

**Atomic claiming would make concurrent finalization graceful**:

```sql
-- Proposed claim_task_for_finalization() function
UPDATE tasker.tasks
SET finalization_claimed_at = NOW(),
    finalization_claimed_by = $processor_uuid
WHERE task_uuid = $uuid
  AND state = 'StepsInProcess'
  AND finalization_claimed_at IS NULL
RETURNING *;
```

**With TAS-37**:
```
T0: Orchestrators A and B both receive finalization trigger
T1: A calls claim_task_for_finalization() → succeeds
T2: B calls claim_task_for_finalization() → returns 0 rows
T3: A proceeds with finalization
T4: B returns early (silent success, already claimed)
```

See [TAS-37](https://linear.app/tasker-systems/issue/TAS-37) for specification (implementation deferred).

---

## SQL Function Atomicity

**File**: `tasker-shared/src/database/sql/`
**Documented**: [Task Readiness & Execution](task-and-step-readiness-and-execution.md)

### Atomic State Transitions

**Function**: `transition_task_state_atomic()`
**Protection**: Compare-and-swap with row locking

```sql
-- Atomic state transition with validation
UPDATE tasker.tasks
SET state = $new_state,
    updated_at = NOW()
WHERE task_uuid = $uuid
  AND state = $expected_current_state  -- CAS: only if state matches
FOR UPDATE;  -- Lock prevents concurrent modifications
```

**Key Guarantees**:
- Returns 0 rows if state doesn't match → safe retry
- Row lock prevents concurrent transitions
- Processor UUID tracked for audit, not enforced (TAS-54)

### Work Distribution Without Contention

**Function**: `get_next_ready_tasks()`
**Protection**: Lock-free claiming via SKIP LOCKED

```sql
SELECT task_uuid, correlation_id, state
FROM tasker.tasks
WHERE state = ANY($processable_states)
  AND (
    state NOT IN ('WaitingForRetry') OR
    last_retry_at + retry_interval < NOW()
  )
ORDER BY
  CASE state
    WHEN 'Pending' THEN 1
    WHEN 'WaitingForRetry' THEN 2
    ELSE 3
  END,
  created_at ASC
FOR UPDATE SKIP LOCKED  -- Skip locked rows, no blocking
LIMIT $batch_size;
```

**Key Guarantees**:
- Each orchestrator gets different tasks
- No blocking or contention
- Dynamic priority (Pending before WaitingForRetry)
- Prevents task starvation

### Step Readiness with Dependency Validation

**Function**: `get_step_readiness_status()`
**Protection**: Validates dependencies in single query

```sql
WITH step_dependencies AS (
  SELECT COUNT(*) as total_deps,
         SUM(CASE WHEN dep_step.state = 'Complete' THEN 1 ELSE 0 END) as completed_deps
  FROM tasker.workflow_step_edges e
  JOIN tasker.workflow_steps dep_step ON e.from_step_uuid = dep_step.uuid
  WHERE e.to_step_uuid = $step_uuid
)
SELECT
  CASE
    WHEN total_deps = completed_deps THEN 'Ready'
    WHEN step.state = 'Error' AND step.attempts < step.max_attempts THEN 'WaitingForRetry'
    ELSE 'Blocked'
  END as readiness
FROM step_dependencies, tasker.workflow_steps step
WHERE step.uuid = $step_uuid;
```

**Key Guarantees**:
- Atomic dependency check
- Handles retry logic with backoff
- Prevents premature execution

### Cycle Detection

**Function**: `WorkflowStepEdge::would_create_cycle()` (Rust, uses SQL)
**Protection**: Recursive CTE path traversal

```sql
WITH RECURSIVE step_path AS (
  -- Base: Start from proposed destination
  SELECT from_step_uuid, to_step_uuid, 1 as depth
  FROM tasker.workflow_step_edges
  WHERE from_step_uuid = $proposed_to

  UNION ALL

  -- Recursive: Follow edges
  SELECT sp.from_step_uuid, wse.to_step_uuid, sp.depth + 1
  FROM step_path sp
  JOIN tasker.workflow_step_edges wse ON sp.to_step_uuid = wse.from_step_uuid
  WHERE sp.depth < 100  -- Prevent infinite recursion
)
SELECT COUNT(*) as has_path
FROM step_path
WHERE to_step_uuid = $proposed_from;
```

**Returns**: True if adding edge would create cycle

**Enforcement**: Called by `WorkflowStepBuilder` during task initialization
- Self-reference check: `from_uuid == to_uuid`
- Path check: Would adding edge create cycle?
- Error before commit: Transaction rolled back on cycle

See `tasker-orchestration/src/orchestration/lifecycle/task_initialization/workflow_step_builder.rs` for enforcement.

---

## Cross-Cutting Scenarios

### Multiple Orchestrators Processing Same Task

**Scenario**: Load balancer distributes work to multiple orchestrators

**Protection**:

1. **Work Distribution**:
   ```sql
   -- Each orchestrator gets different tasks via SKIP LOCKED
   Orchestrator A: Tasks [1, 2, 3]
   Orchestrator B: Tasks [4, 5, 6]
   ```

2. **State Transitions**:
   ```rust
   // Both attempt to transition same task (shouldn't happen, but...)
   A: transition(Pending -> Initializing) → succeeds
   B: transition(Pending -> Initializing) → fails (state already changed)
   ```

3. **Step Enqueueing**:
   ```rust
   // Task in EnqueuingSteps state
   A: Processes task, enqueues steps A, B
   B: Cannot claim task (state not in processable states)
   // OR if B claims during transition:
   B: Filters steps by state → A, B already Enqueued, skips them
   ```

**Result**: No duplicate work, clean coordination

### Orchestrator Crashes and Recovers

**Scenario**: Orchestrator crashes mid-operation, another takes over

#### During Task Initialization

```
Before TAS-54:
T0: Orchestrator A initializes task 1
T1: Task transitions to Initializing (processor_uuid = A)
T2: A crashes
T3: Task stuck in Initializing forever (ownership blocks recovery)

After TAS-54:
T0: Orchestrator A initializes task 1
T1: Task transitions to Initializing (processor_uuid = A for audit)
T2: A crashes
T3: Orchestrator B picks up task 1
T4: B transitions Initializing -> EnqueuingSteps (succeeds, no ownership check)
T5: Task recovers automatically
```

#### During Step Enqueueing

```
T0: Orchestrator A enqueues steps [A, B] of task 1
T1: A crashes before committing
T2: Transaction rolls back
T3: Steps A, B remain in Pending state
T4: Orchestrator B picks up task 1
T5: B enqueues steps A, B (state still Pending)
T6: No duplicate work
```

#### During Result Processing

```
T0: Worker completes step A
T1: Orchestrator A receives result, transitions step to Complete
T2: A crashes before updating task state
T3: PGMQ message visibility timeout expires
T4: Orchestrator B receives same result message
T5: B queries step A → already Complete
T6: B skips processing (idempotent)
T7: B evaluates task state, continues workflow
```

**Result**: Complete recovery, no manual intervention

### Retry After Transient Failure

**Scenario**: Database connection lost during operation

```rust
// Orchestrator attempts task initialization
let result = task_initializer.initialize(request).await;

match result {
    Err(TaskInitializationError::Database(_)) => {
        // Transient failure (connection lost)
        // Retry same request
        let retry_result = task_initializer.initialize(request).await;

        // Possibilities:
        // 1. Succeeds: Transaction completed before connection lost
        //    → identity_hash unique constraint prevents duplicate
        //    → Get existing task
        // 2. Succeeds: Transaction rolled back
        //    → Create task successfully
        // 3. Fails: Different error
        //    → Handle appropriately
    }
    Ok(task) => { /* Success */ }
}
```

**Key Pattern**: Operations are designed to be retry-safe
- Database constraints prevent duplicates
- State guards prevent invalid transitions
- Find-or-create handles concurrent creation

### PGMQ Message Duplicate Delivery

**Scenario**: PGMQ message processed twice due to visibility timeout

```rust
// Worker completes step, sends result
pgmq.send("orchestration_step_results", result).await?;

// Orchestrator A receives message
let message = pgmq.read("orchestration_step_results").await?;

// A processes result
result_processor.process(message.payload).await?;

// A about to delete message, crashes
// Message visibility timeout expires → message reappears

// Orchestrator B receives same message
let duplicate = pgmq.read("orchestration_step_results").await?;

// B processes result
// State machine checks: step already Complete
// Returns early (idempotent)
result_processor.process(duplicate.payload).await?; // Harmless

// B deletes message
pgmq.delete(duplicate.msg_id).await?;
```

**Protection**:
- State guards: Check current state before processing
- Idempotent handlers: Safe to process same message multiple times
- Message deletion: Only after confirmed processing

See [Events and Commands](events-and-commands.md) for PGMQ architecture.

---

## Multi-Instance Validation (TAS-73)

The defense-in-depth architecture was validated through comprehensive multi-instance cluster testing in TAS-73. This section documents the validation results and confirms the effectiveness of the protection mechanisms.

### Test Configuration

- **Orchestration Instances**: 2 (ports 8080, 8081)
- **Worker Instances**: 2 per type (Rust: 8100-8101, Ruby: 8200-8201, Python: 8300-8301, TypeScript: 8400-8401)
- **Total Services**: 10 concurrent instances
- **Database**: Shared PostgreSQL with PGMQ messaging

### Validation Results

| Metric | Result |
|--------|--------|
| **Tests Passed** | 1,645 |
| **Intermittent Failures** | 3 (resource contention, not race conditions) |
| **Tests Skipped** | 21 (domain event tests, require single-instance) |
| **Race Conditions Detected** | 0 |
| **Data Corruption Detected** | 0 |

### What Was Validated

1. **Concurrent Task Creation**
   - Tasks created through different orchestration instances
   - No duplicate tasks or UUIDs
   - All tasks complete successfully
   - State consistent across all instances

2. **Work Distribution**
   - `SKIP LOCKED` distributes tasks without overlap
   - Multiple workers claim different steps
   - No duplicate step processing

3. **State Machine Guards**
   - Invalid transitions rejected at state machine layer
   - Compare-and-swap prevents concurrent modifications
   - Terminal states protected from re-entry

4. **Transaction Boundaries**
   - All-or-nothing semantics maintained under load
   - No partial task initialization observed
   - Crash recovery works correctly

5. **Cross-Instance Consistency**
   - Task state queries return same result from any instance
   - Step state transitions visible immediately to all instances
   - No stale reads observed

### Protection Layer Effectiveness

| Layer | Validation Method | Result |
|-------|-------------------|--------|
| **Database Atomicity** | Concurrent unique constraint tests | Duplicates correctly rejected |
| **State Machine Guards** | Parallel transition attempts | Invalid transitions blocked |
| **Transaction Boundaries** | Crash injection tests | Clean rollback, no corruption |
| **Application Logic** | State filtering under load | Idempotent processing confirmed |

### Intermittent Failures Analysis

Three tests showed intermittent failures under heavy parallelization:

- **Root Cause**: Database connection pool exhaustion when running 1600+ tests in parallel
- **Evidence**: Failures occurred only at high parallelism (>4 threads), not with serialized execution
- **Classification**: Resource contention, NOT race conditions
- **Mitigation**: Nextest configured with `test-threads = 1` for multi_instance tests

**Key Finding**: No race conditions were detected. All intermittent failures traced to resource limits.

### Domain Event Tests

21 tests were excluded from cluster mode using `#[cfg(not(feature = "test-cluster"))]`:

- **Reason**: Domain event tests verify in-process event delivery (publish/subscribe within single process)
- **Behavior in Cluster**: Events published in one instance aren't delivered to subscribers in another instance
- **Status**: Working as designed - these tests run correctly in single-instance CI

### Stress Test Results

**Rapid Task Burst Test**:
- 25 tasks created in <1 second
- All tasks completed successfully
- No duplicate UUIDs
- Creation rate: ~50 tasks/second sustained

**Round-Robin Distribution Test**:
- Tasks distributed evenly across orchestration instances
- Load balancing working correctly
- No single-instance bottleneck

### Recommendations Validated

The following architectural decisions were validated by cluster testing:

1. **TAS-54 (Ownership Removal)**: Processor UUID as audit-only (not enforced) enables automatic recovery
2. **SKIP LOCKED Pattern**: Effective for contention-free work distribution
3. **State-Before-Queue Pattern**: Prevents workers from seeing uncommitted state
4. **Find-or-Create Pattern**: Handles concurrent entity creation correctly

### Future Enhancements Identified

Testing identified one P2 improvement opportunity:

**TAS-37: Atomic Finalization Claiming** (see [Atomic Finalization Design](../ticket-specs/TAS-73/atomic-finalization-design.md))
- Current: Second orchestrator gets `StateMachineError` during concurrent finalization
- Proposed: Transaction-based locking for graceful handling
- Priority: P2 (operational improvement, correctness already ensured)

### Running Cluster Validation

To reproduce the validation:

```bash
# Setup cluster environment
cargo make setup-env-cluster

# Start full cluster
cargo make cluster-start-all

# Run all tests including cluster tests
cargo make test-rust-all

# Stop cluster
cargo make cluster-stop
```

See [Cluster Testing Guide](../testing/cluster-testing-guide.md) for detailed instructions.

---

## Design Principles

### Defense in Depth

The system **intentionally provides multiple overlapping protection layers** rather than relying on a single mechanism. This ensures:

1. **Resilience**: If one layer fails (e.g., application bug), others prevent corruption
2. **Clear Semantics**: Each layer has a specific purpose and failure mode
3. **Ease of Reasoning**: Developers can understand guarantees at each level
4. **Graceful Degradation**: System remains safe even under partial failures

### Fail-Safe Defaults

When in doubt, the system **errs on the side of caution**:

- State transitions fail if current state doesn't match → prevents invalid states
- Unique constraints fail creation → prevents duplicates
- Row locks block concurrent access → prevents race conditions
- Cycle detection fails initialization → prevents invalid workflows

**Better to fail cleanly than to corrupt data.**

### Retry Safety

All critical operations are designed to be **safely retryable**:

- **Idempotent**: Same operation, repeated → same outcome
- **State-Based**: Check current state before acting
- **Atomic**: All-or-nothing commits
- **No Side Effects**: Operations don't accumulate partial state

This enables:
- Automatic retry after transient failures
- Duplicate message handling
- Recovery after crashes
- Horizontal scaling without coordination overhead

### Audit Trail Without Enforcement

**TAS-54 Decision**: Track ownership for observability, don't enforce for correctness

```rust
// Processor UUID recorded in all transitions
pub struct TaskTransition {
    pub task_uuid: Uuid,
    pub from_state: TaskState,
    pub to_state: TaskState,
    pub processor_uuid: Uuid,  // For audit and debugging
    pub event: String,
    pub timestamp: DateTime<Utc>,
}

// But NOT enforced in transition logic
impl TaskStateMachine {
    pub async fn transition(&mut self, event: TaskEvent) -> Result<TaskState> {
        // ✅ Tracks processor UUID
        // ❌ Does NOT require ownership match
        // Reason: Enables recovery after crashes
    }
}
```

**Why This Works**:
- State guards provide correctness (current state validation)
- Processor UUID provides observability (who did what when)
- No ownership blocking means automatic recovery
- Full audit trail for debugging and monitoring

---

## Implementation Checklist

When implementing new orchestration operations, ensure:

### Database Layer
- [ ] Unique constraints for entities that must be singular
- [ ] `FOR UPDATE` locking for state transitions
- [ ] `FOR UPDATE SKIP LOCKED` for work distribution
- [ ] Compare-and-swap (CAS) in UPDATE WHERE clauses
- [ ] Transaction wrapping for multi-step operations

### State Machine Layer
- [ ] Current state retrieval before transitions
- [ ] Event applicability validation
- [ ] Terminal state protection
- [ ] Error handling for invalid transitions

### Application Layer
- [ ] Find-or-create pattern for shared entities
- [ ] State-based filtering before processing
- [ ] State-before-queue ordering for events
- [ ] Idempotent message handlers

### Testing
- [ ] Concurrent operation tests (multiple orchestrators)
- [ ] Crash recovery tests (mid-operation failures)
- [ ] Retry safety tests (duplicate message handling)
- [ ] Race condition tests (timing-dependent scenarios)

---

## Related Documentation

### Core Architecture
- **[States and Lifecycles](states-and-lifecycles.md)** - Dual state machine architecture
- **[Events and Commands](events-and-commands.md)** - Event-driven coordination patterns
- **[Actor-Based Architecture](actors.md)** - Orchestration actor pattern (TAS-46)
- **[Task Readiness & Execution](../reference/task-and-step-readiness-and-execution.md)** - SQL functions and execution logic

### Implementation Details
- **[TAS-54 ADR](../decisions/TAS-54-ownership-removal.md)** - Processor UUID ownership removal decision
- **[TAS-37](https://linear.app/tasker-systems/issue/TAS-37)** - Atomic claiming (future enhancement)
- **[TAS-41](https://linear.app/tasker-systems/issue/TAS-41)** - Enhanced state machines with ownership

### Multi-Instance Validation (TAS-73)
- **[Cluster Testing Guide](../testing/cluster-testing-guide.md)** - Running multi-instance cluster tests
- **[TAS-73 Research Findings](../ticket-specs/TAS-73/research-findings.md)** - Research and findings summary
- **[TAS-73 Multi-Instance Deployment](../ticket-specs/TAS-73/multi-instance-deployment.md)** - Cluster deployment design
- **[TAS-73 Atomic Finalization Design](../ticket-specs/TAS-73/atomic-finalization-design.md)** - P2 enhancement proposal

### Testing
- **[Comprehensive Lifecycle Testing](../testing/comprehensive-lifecycle-testing-guide.md)** - Testing patterns including concurrent scenarios

---

← Back to [Documentation Hub](README.md)

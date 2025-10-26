# TAS-54 Phase 1: Idempotency Audit Findings

**Date**: 2025-10-26
**Status**: In Progress
**Auditor**: Claude Code

## Executive Summary

This document captures the comprehensive idempotency audit of all orchestration actors and supporting services. The goal is to determine whether processor UUID ownership enforcement is necessary or if pure state machine logic with audit-only processor tracking is sufficient.

## Phase 1.1: TaskRequestActor Idempotency Analysis

### Actor Overview

**Location**: `tasker-orchestration/src/actors/task_request_actor.rs`

**Message**: `ProcessTaskRequestMessage`

**Flow**:
1. TaskRequestActor receives message
2. Serializes to JSON payload
3. Delegates to TaskRequestProcessor.process_task_request()
4. Processor delegates to TaskInitializer.create_task_from_request()

### Delegated Service: TaskInitializer

**Location**: `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs`

**Operation Flow**:
```
1. Begin SQLx transaction
2. NamespaceResolver.resolve_named_task_uuid()
   ├─ find_or_create_namespace() - IDEMPOTENT
   └─ find_or_create_named_task() - NOT IDEMPOTENT (requires pre-registration)
3. Task.create_with_transaction()
4. TemplateLoader.load_task_template() - READ-ONLY
5. WorkflowStepBuilder.create_workflow_steps()
6. StateInitializer.create_initial_state_transitions_in_tx()
7. Commit transaction
8. StateInitializer.initialize_state_machines_post_transaction()
9. Publish task_initialized event
```

### Database-Level Idempotency Protections

#### 1. Task Deduplication via identity_hash

**Constraint**: `CREATE UNIQUE INDEX index_tasks_on_identity_hash ON tasker_tasks (identity_hash)`

**Hash Generation** (`task.rs:875`):
```rust
pub fn generate_identity_hash(
    named_task_uuid: Uuid,
    context: &Option<serde_json::Value>,
) -> String {
    let mut hasher = DefaultHasher::new();
    named_task_uuid.hash(&mut hasher);
    if let Some(ctx) = context {
        ctx.to_string().hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}
```

**Idempotency Guarantee**: Same named_task_uuid + context = same hash. If two orchestrators try to initialize the same task (same template + context), the second INSERT will fail with unique constraint violation.

**Result**: ✅ **Database prevents duplicate task creation**

#### 2. Namespace Deduplication

**Constraint**: `tasker_task_namespaces_name_unique UNIQUE (name)`

**Code** (`namespace_resolver.rs:45`):
```rust
async fn find_or_create_namespace(&self, namespace_name: &str)
    -> Result<TaskNamespace, TaskInitializationError> {
    // Try to find existing namespace first
    if let Some(existing) = TaskNamespace::find_by_name(pool, namespace_name).await? {
        return Ok(existing);
    }
    // Create new namespace if not found
    TaskNamespace::create(pool, new_namespace).await?
}
```

**Idempotency Guarantee**: Classic find-or-create pattern. If two orchestrators try to create "payments" namespace:
- First: finds nothing, creates successfully
- Second: either finds the first's namespace OR gets unique constraint violation on INSERT

**Result**: ✅ **Namespace creation is idempotent**

#### 3. Named Task Resolution (NOT Idempotent by Design)

**Constraint**: `tasker_named_tasks_namespace_name_unique UNIQUE (task_namespace_uuid, name)`

**Code** (`namespace_resolver.rs:76`):
```rust
async fn find_or_create_named_task(...) -> Result<NamedTask, TaskInitializationError> {
    let existing_task = NamedTask::find_by_name_version_namespace(...).await?;

    if let Some(existing) = existing_task {
        return Ok(existing);
    }

    // Template not found - this should never happen in production
    // Templates must be registered by workers before orchestration can use them
    Err(TaskInitializationError::ConfigurationNotFound(...))
}
```

**Idempotency Guarantee**: **NOT IDEMPOTENT** - by design. Named tasks must be pre-registered by workers. This is intentional:
- Workers register their handlers (creates NamedTask)
- Orchestration looks up existing NamedTask
- If not found, fails with clear error

**Result**: ⚠️ **Not idempotent, but correctly fails-fast. Does not allow duplicate creation.**

#### 4. Workflow Step Creation

**Code** (`workflow_step_builder.rs:45`):
```rust
async fn create_steps(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_uuid: Uuid,
    task_template: &TaskTemplate,
) -> Result<HashMap<String, Uuid>, TaskInitializationError>
```

**Idempotency Guarantee**: Workflow steps are created within the same transaction as the task. Since task creation is protected by identity_hash unique constraint, workflow steps can never be duplicated - the transaction will rollback if task insert fails.

**Result**: ✅ **Workflow steps protected by task-level transaction atomicity**

### Transaction Scope Analysis

**Critical Protection**: All database writes happen within a single SQLx transaction:

```rust
// From service.rs:113
let mut tx = self.context.database_pool().begin().await?;

// All writes use the transaction:
let task = self.create_task_record(&mut tx, task_request).await?;
self.workflow_step_builder.create_workflow_steps(&mut tx, ...).await?;
self.state_initializer.create_initial_state_transitions_in_tx(&mut tx, ...).await?;

// Atomic commit
tx.commit().await?;
```

**Idempotency Impact**: If ANY operation fails (including unique constraint violations), the ENTIRE transaction rolls back. This provides all-or-nothing semantics:
- Success: Task + steps + initial transitions created atomically
- Failure: Nothing persisted, safe to retry

**Result**: ✅ **Transaction atomicity prevents partial state**

### State Machine Initialization

**Two-Phase Approach**:

1. **In-Transaction** (`state_initializer.rs:38`):
   - Creates initial TaskTransition (Pending state)
   - Creates initial WorkflowStepTransition for each step
   - Protected by transaction atomicity

2. **Post-Transaction** (`state_initializer.rs:84`):
   - Initializes TaskStateMachine
   - Transitions task from Pending → Initializing
   - Verifies step state machines exist

**Idempotency Guarantee**:
- Database transitions are in-transaction, atomic
- State machine transitions (Pending → Initializing) are guarded by current_state check
- If re-run, transition is skipped if already in non-Pending state

**Code** (`state_initializer.rs:115`):
```rust
if current_state == TaskState::Pending {
    match task_state_machine.transition(TaskEvent::Start).await {
        Ok(success) => { /* transitioned */ }
        Err(e) => { /* failed */ }
    }
} else {
    info!("Task already in non-Pending state, no transition needed");
}
```

**Result**: ✅ **State initialization has built-in current-state guards**

### Processor UUID Usage in TaskInitializer

**Current Usage** (`state_initializer.rs:49`):
```rust
let new_task_transition = NewTaskTransition {
    task_uuid,
    to_state: "pending".to_string(),
    from_state: None,
    processor_uuid: Some(self.context.processor_uuid()),  // ← AUDIT TRAIL ONLY
    metadata: Some(json!({"initial_state": "pending", "from_service": "task_initializer"})),
};
```

**Observation**: Processor UUID is stored in initial transition but NOT used for ownership enforcement during task initialization. The transaction and identity_hash provide the actual protection.

**Result**: ℹ️ **Processor UUID is audit-only during initialization**

### Idempotency Verdict: TaskRequestActor

| Operation | Idempotent | Protection Mechanism |
|-----------|------------|---------------------|
| Namespace creation | ✅ Yes | Find-or-create + unique constraint |
| Named task lookup | ⚠️ Fail-fast | Required pre-registration |
| Task creation | ✅ Yes | identity_hash unique constraint |
| Workflow step creation | ✅ Yes | Transaction atomicity |
| State transitions (DB) | ✅ Yes | Transaction atomicity |
| State machines (in-memory) | ✅ Yes | Current-state guards |
| **Overall** | **✅ Yes** | **Multi-layer protection** |

### Race Condition Analysis

**Scenario**: Two orchestrators receive identical TaskRequestMessage simultaneously

**Timeline**:
```
T0: Orchestrator A begins transaction
T0: Orchestrator B begins transaction
T1: Both compute same identity_hash
T2: A inserts task successfully
T2: B attempts insert
T3: B's insert fails with unique constraint violation on identity_hash
T3: B's transaction rolls back
T4: A commits successfully
```

**Result**: One orchestrator succeeds, one fails cleanly. No partial state, no corruption.

**Current Behavior**: Second orchestrator gets TaskInitializationError and likely retries, then gets "Template not found" or similar error indicating task already exists.

**Improvement Opportunity**: Could add explicit "task already exists" handling to make retry behavior more graceful.

### Critical Finding: Processor Ownership Not Needed for Initialization

**Evidence**:
1. Transaction atomicity prevents partial writes
2. identity_hash prevents duplicate tasks
3. Unique constraints on namespaces prevent duplicates
4. State machine has current-state guards
5. Processor UUID is already audit-only in this phase

**Conclusion**: Task initialization is already fully idempotent without processor ownership enforcement. The processor_uuid in transitions is purely for audit/debugging.

---

## Phase 1.2: ResultProcessorActor Idempotency Analysis

### Actor Overview

**Location**: `tasker-orchestration/src/actors/result_processor_actor.rs`

**Message**: `ProcessStepResultMessage`

**Flow**:
1. ResultProcessorActor receives StepExecutionResult message
2. Delegates to OrchestrationResultProcessor.handle_step_execution_result()
3. MessageHandler orchestrates three focused components:
   - MetadataProcessor: Processes backoff metadata (coordination only, no writes)
   - StateTransitionHandler: Transitions steps from EnqueuedForOrchestration → final state
   - TaskCoordinator: Coordinates task finalization

### Delegated Services Analysis

#### 1. MetadataProcessor (Coordination Only)

**Purpose**: Process backoff metadata for retry timing decisions

**Operations**: READ-ONLY - no database writes

**Idempotency**: ✅ Safe to call multiple times (pure coordination)

#### 2. StateTransitionHandler (`state_transition_handler.rs:32`)

**Purpose**: Transition steps from EnqueuedForOrchestration states to final states

**Flow**:
```rust
1. Load WorkflowStep by UUID
2. Get current state from database
3. Check if step is in EnqueuedForOrchestration or EnqueuedAsErrorForOrchestration
4. If YES:
   - Create StepStateMachine
   - Determine final event (Complete or Error)
   - Transition to final state
5. If NO: Skip (already processed)
```

**Idempotency Guard** (`state_transition_handler.rs:77`):
```rust
if matches!(
    step_state,
    WorkflowStepState::EnqueuedForOrchestration
        | WorkflowStepState::EnqueuedAsErrorForOrchestration
) {
    // Process transition
} else {
    debug!("Step not in notification state - skipping orchestration transition");
}
```

**Idempotency Guarantee**: If called twice:
- First call: Step is in EnqueuedForOrchestration, transitions to Complete/Error
- Second call: Step is already in Complete/Error, guard skips processing

**Result**: ✅ **State guards provide idempotency**

#### 3. TaskCoordinator (`task_coordinator.rs:34`)

**Purpose**: Coordinate task finalization after step completion

**Flow**:
```rust
1. Load WorkflowStep by UUID
2. Create TaskStateMachine (with processor_uuid)
3. Get current task state
4. Check if transition needed:
   - StepsInProcess → EvaluatingResults: Transition required
   - EvaluatingResults: Already evaluating, proceed to finalization
   - Other states: Skip
5. If transition succeeds: Delegate to TaskFinalizer
```

**Processor Ownership Enforcement** (`task_coordinator.rs:77-80`):
```rust
let mut task_state_machine = TaskStateMachine::for_task(
    workflow_step.task_uuid,
    self.context.database_pool().clone(),
    self.context.processor_uuid(),  // ← OWNERSHIP ENFORCEMENT
).await?;
```

**State Transition with Ownership Check** (`task_state_machine.rs:125-131`):
```rust
if target_state.requires_ownership() {  // EvaluatingResults requires ownership
    let current_processor = self.get_current_processor().await?;
    TransitionGuard::check_ownership(target_state, current_processor, self.processor_uuid)
        .map_err(|e| StateMachineError::GuardFailed {
            reason: e.to_string(),
        })?;
}
```

**Ownership States**: Only 3 states require processor ownership:
- `Initializing`
- `EnqueuingSteps`
- `EvaluatingResults` ← **Used by ResultProcessorActor**

**check_ownership Logic** (`guards.rs:97-113`):
```rust
pub fn check_ownership(
    state: TaskState,
    task_processor: Option<Uuid>,
    requesting_processor: Uuid,
) -> GuardResult<()> {
    if !state.requires_ownership() {
        return Ok(());
    }

    match task_processor {
        Some(owner) if owner == requesting_processor => Ok(()),  // Same processor
        Some(owner) => Err(business_rule_violation(format!(
            "Processor {} does not own task in state {:?} (owned by {})",
            requesting_processor, state, owner
        ))),
        None => Ok(()), // No owner yet, can claim
    }
}
```

**Idempotency Impact**:

**Scenario 1: Same Orchestrator Processes Twice**
```
T0: Orchestrator A processes step result
T1: Task in StepsInProcess, owned by A
T2: A transitions to EvaluatingResults (ownership check passes: A == A)
T3: A finalizes task
T4: Duplicate message arrives
T5: Task in terminal state (Complete/Error)
T6: Current state check in TaskCoordinator determines no transition needed
```
**Result**: ✅ **Idempotent - guard skips duplicate processing**

**Scenario 2: Different Orchestrator Tries to Process** ⚠️
```
T0: Orchestrator A processes step result
T1: Task in StepsInProcess, owned by A
T2: A transitions to EvaluatingResults (ownership: A)
T3: A crashes before finalization
T4: Orchestrator B receives retry of same step result
T5: Task in EvaluatingResults, owned by A (STALE)
T6: B attempts transition to EvaluatingResults
T7: Ownership check fails: B != A
T8: ERROR: "Processor B does not own task (owned by A)"
```
**Result**: ❌ **BLOCKED BY STALE OWNERSHIP - This is the TAS-54 problem!**

### Idempotency Verdict: ResultProcessorActor

| Operation | Idempotent | Protection Mechanism | Ownership Impact |
|-----------|------------|---------------------|------------------|
| MetadataProcessor | ✅ Yes | Read-only coordination | N/A |
| StateTransitionHandler | ✅ Yes | Current state guards | No ownership |
| TaskCoordinator (same orchestrator) | ✅ Yes | State machine guards | Ownership passes |
| TaskCoordinator (different orchestrator) | ❌ No | Ownership enforcement | **BLOCKED** |
| **Overall (healthy system)** | **✅ Yes** | **Multi-layer protection** | **No issues** |
| **Overall (after crash)** | **❌ No** | **Ownership blocks recovery** | **CRITICAL** |

### Critical Finding: Processor Ownership Blocks Recovery

**The TAS-54 Problem Manifest**:

When an orchestrator crashes with tasks in `EvaluatingResults` state:
1. Task transitions are stored with dead orchestrator's UUID
2. New orchestrator receives step results for retry
3. New orchestrator attempts to finalize task
4. Ownership check fails because task "owned" by dead orchestrator
5. Task stuck in `EvaluatingResults` state indefinitely

**Evidence** (`task_coordinator.rs:99-101`):
```rust
task_state_machine
    .transition(TaskEvent::StepCompleted(*step_uuid))
    .await?  // ← This fails ownership check if processor changed
```

**Why This Matters**:
- StepsInProcess → EvaluatingResults requires ownership check
- EvaluatingResults is an ACTIVE processing state
- Once claimed by orchestrator A, orchestrator B cannot take over
- Task remains stuck until manual intervention

### Processor UUID Usage in ResultProcessorActor

**Current Usage**:
1. **Audit Trail**: Processor UUID stored in every TaskTransition
2. **Ownership Enforcement**: Guards state transitions requiring ownership
3. **Blocking Behavior**: Prevents orchestrator B from processing work started by orchestrator A

**Observation**: Without ownership enforcement:
- StateTransitionHandler already has current-state guards (idempotent)
- TaskCoordinator already checks current state before transitioning
- State machine transitions are atomic database operations
- Only missing: audit trail (would still be present, just not enforced)

**Result**: 🔴 **Processor ownership enforcement is the root cause of stale task recovery failures**

## Phase 1.3: StepEnqueuerActor (Complete)

**See**: `phase1.3-step-enqueuer-findings.md` for comprehensive analysis

### Executive Summary

✅ **StepEnqueuerActor operations are fully idempotent without processor ownership enforcement**

The actor and its delegated services (BatchProcessor, StateHandlers, StepEnqueuer) provide robust idempotency through multiple protection layers:

1. **SQL FOR UPDATE SKIP LOCKED**: Prevents concurrent task claiming in `get_next_ready_tasks()`
2. **Atomic State Transitions**: Database-level compare-and-swap in state machines
3. **Step State Filtering**: Only enqueues steps in `Pending` or `WaitingForRetry` states
4. **State-Before-Queue Pattern**: Commits state transitions before PGMQ notifications (TAS-29 Phase 5.4)
5. **Transaction Atomicity**: All database writes are atomic operations

### Key Finding

The processor UUID is tracked for audit purposes only. The `requires_ownership()` method returns `false` for all states (TAS-54 implementation already complete).

### Race Condition Analysis

**Scenario: Two Orchestrators Process Same Task**
- SQL row locking prevents overlap
- Even if partial overlap occurs, step state filtering prevents duplicate enqueueing
- State machine CAS prevents duplicate transitions

**Scenario: Orchestrator Crashes During Enqueueing**
- Recovery orchestrator discovers same task
- Step filtering skips already-enqueued steps
- Only pending steps are enqueued

**Verdict**: ✅ All race conditions handled safely

### Critical Components

1. **BatchProcessor**: Uses SQL locking for task discovery
2. **StateHandlers**: Uses state machine guards for transitions
3. **StepEnqueuer**: Uses step state filtering + state-before-queue pattern
4. **State Machines**: Atomic transitions with no ownership enforcement

### Comparison with Other Actors

| Actor | Primary Protection | Ownership Required |
|-------|-------------------|-------------------|
| TaskRequestActor | identity_hash + transaction | ❌ No |
| ResultProcessorActor | State guards | ❌ No (was Yes, removed) |
| StepEnqueuerActor | SQL locking + state guards | ❌ No |

All three actors are now fully idempotent without processor ownership enforcement.

## Phase 1.4: TaskFinalizerActor (Complete)

**See**:
- `phase1.4-task-finalizer-findings.md` for comprehensive analysis
- `phase1.4-summary.md` for executive summary

### Executive Summary

⚠️ **TaskFinalizerActor is sufficient for TAS-54 recovery, but TAS-37 atomic claiming not implemented**

**What Works for TAS-54**:
- ✅ Current-state guards prevent state corruption
- ✅ No processor ownership blocking recovery
- ✅ Retry after crash succeeds
- ✅ Same orchestrator calling twice is fully idempotent

**Critical Discovery**: The TAS-37 specification describes elaborate atomic claiming (`claim_task_for_finalization()` SQL function, `FinalizationClaimer` component), but **NONE OF THIS EXISTS**. TaskFinalizer relies entirely on state machine guards.

**Impact**: Works for TAS-54 (automatic recovery), but concurrent finalization from different orchestrators gets errors instead of graceful handling.

## Phase 1.5: SQL Function Atomicity Analysis (Complete)

**See**: `phase1.5-sql-atomicity-findings.md` for comprehensive analysis

### Executive Summary

✅ **SQL functions provide strong atomic guarantees through PostgreSQL primitives**

**Key Findings**:
- Database atomicity via FOR UPDATE locking, FOR UPDATE SKIP LOCKED, and compare-and-swap
- Processor UUID enforcement not required - state validation alone provides sufficient protection
- 18 functions analyzed across 5 categories (state transitions, discovery, claiming, PGMQ, DAG)
- 4 critical functions missing (referenced in specs but not implemented)

**Missing Functions**:
1. ❌ `transition_step_state_atomic()` - Step-level atomic transitions
2. ❌ `claim_task_for_finalization()` - TAS-37 claiming
3. ❌ `finalize_task_completion()` - Finalization orchestration
4. ❌ `detect_cycle()` - DAG cycle prevention

**Conclusion**: SQL layer provides strong atomicity. Missing functions should be implemented for production completeness, but current state is sufficient for TAS-54 recovery goals.

---

## Progress Tracker

- [x] Phase 1.1: TaskRequestActor - COMPLETE (fully idempotent)
- [x] Phase 1.2: ResultProcessorActor - COMPLETE (ownership was blocking, now removed)
- [x] Phase 1.3: StepEnqueuerActor - COMPLETE (fully idempotent with multi-layer protection)
- [x] Phase 1.4: TaskFinalizerActor - COMPLETE (sufficient for TAS-54, TAS-37 not implemented)
- [x] Phase 1.5: SQL Function Atomicity - COMPLETE (strong guarantees, 4 functions missing)
- [x] Final synthesis and recommendations - **See COMPREHENSIVE-FINDINGS-SUMMARY.md**

---

## Comprehensive Summary

**All phases complete!** See `COMPREHENSIVE-FINDINGS-SUMMARY.md` for:
- Cross-cutting analysis across all actors
- Idempotency guarantee layers (database, state machine, application, PGMQ)
- Complete findings summary
- Recommendations for future work
- TAS-54 resolution confirmation

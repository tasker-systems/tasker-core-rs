# States and Lifecycles

**Last Updated**: 2025-10-10
**Audience**: All
**Status**: Active
**Related Docs**: [Documentation Hub](README.md) | [Events and Commands](events-and-commands.md) | [Task Readiness & Execution](task-and-step-readiness-and-execution.md)

← Back to [Documentation Hub](README.md)

---

This document provides comprehensive documentation of the state machine architecture in tasker-core, covering both task and workflow step lifecycles, their state transitions, and the underlying persistence mechanisms.

## Overview

The tasker-core system implements a sophisticated dual-state-machine architecture:

1. **Task State Machine**: Manages overall workflow orchestration with 12 comprehensive states (TAS-41 enhancement)
2. **Workflow Step State Machine**: Manages individual step execution with 8 states including orchestration queuing

Both state machines work in coordination to provide atomic, auditable, and resilient workflow execution with proper event-driven communication between orchestration and worker systems.

## Task State Machine Architecture

### Task State Definitions

The task state machine implements 12 comprehensive states as defined in `tasker-shared/src/state_machine/states.rs`:

#### Initial States
- **`Pending`**: Created but not started (default initial state)
- **`Initializing`**: Discovering initial ready steps and setting up task context

#### Active Processing States  
- **`EnqueuingSteps`**: Actively enqueuing ready steps to worker queues
- **`StepsInProcess`**: Steps are being processed by workers (orchestration monitoring)
- **`EvaluatingResults`**: Processing results from completed steps and determining next actions

#### Waiting States
- **`WaitingForDependencies`**: No ready steps, waiting for dependencies to be satisfied
- **`WaitingForRetry`**: Waiting for retry timeout before attempting failed steps again
- **`BlockedByFailures`**: Has failures that prevent progress (manual intervention may be needed)

#### Terminal States
- **`Complete`**: All steps completed successfully (terminal)
- **`Error`**: Task failed permanently (terminal)
- **`Cancelled`**: Task was cancelled (terminal)
- **`ResolvedManually`**: Manually resolved by operator (terminal)

### Task State Properties

Each state has key properties that drive system behavior:

```rust
impl TaskState {
    pub fn is_terminal(&self) -> bool         // Cannot transition further
    pub fn requires_ownership(&self) -> bool  // Processor ownership required
    pub fn is_active(&self) -> bool          // Currently being processed  
    pub fn is_waiting(&self) -> bool         // Waiting for external conditions
    pub fn can_be_processed(&self) -> bool   // Available for orchestration pickup
}
```

**Ownership-Required States**: `Initializing`, `EnqueuingSteps`, `StepsInProcess`, `EvaluatingResults`
**Processable States**: `Pending`, `WaitingForDependencies`, `WaitingForRetry`

### Task Lifecycle Flow

```mermaid
stateDiagram-v2
    [*] --> Pending
    
    %% Initial Flow
    Pending --> Initializing : Start
    
    %% From Initializing
    Initializing --> EnqueuingSteps : ReadyStepsFound(count)
    Initializing --> Complete : NoStepsFound
    Initializing --> WaitingForDependencies : NoDependenciesReady
    
    %% Processing Flow
    EnqueuingSteps --> StepsInProcess : StepsEnqueued(uuids)
    EnqueuingSteps --> Error : EnqueueFailed(error)
    
    StepsInProcess --> EvaluatingResults : AllStepsCompleted
    StepsInProcess --> EvaluatingResults : StepCompleted(uuid)
    StepsInProcess --> WaitingForRetry : StepFailed(uuid)
    
    %% Result Evaluation
    EvaluatingResults --> Complete : AllStepsSuccessful
    EvaluatingResults --> EnqueuingSteps : ReadyStepsFound(count)
    EvaluatingResults --> WaitingForDependencies : NoDependenciesReady
    EvaluatingResults --> BlockedByFailures : PermanentFailure(error)
    
    %% Waiting States
    WaitingForDependencies --> EvaluatingResults : DependenciesReady
    WaitingForRetry --> EnqueuingSteps : RetryReady
    
    %% Problem Resolution
    BlockedByFailures --> Error : GiveUp
    BlockedByFailures --> ResolvedManually : ManualResolution
    
    %% Cancellation (from any non-terminal state)
    Pending --> Cancelled : Cancel
    Initializing --> Cancelled : Cancel
    EnqueuingSteps --> Cancelled : Cancel
    StepsInProcess --> Cancelled : Cancel
    EvaluatingResults --> Cancelled : Cancel
    WaitingForDependencies --> Cancelled : Cancel
    WaitingForRetry --> Cancelled : Cancel
    BlockedByFailures --> Cancelled : Cancel
    
    %% Legacy Support
    Error --> Pending : Reset
    
    %% Terminal States
    Complete --> [*]
    Error --> [*]
    Cancelled --> [*]
    ResolvedManually --> [*]
```

### Task Event System

Task state transitions are driven by events defined in `tasker-shared/src/state_machine/events.rs`:

#### Lifecycle Events
- `Start`: Begin task processing
- `Cancel`: Cancel task execution
- `GiveUp`: Abandon task (BlockedByFailures -> Error)
- `ManualResolution`: Manually resolve task

#### Discovery Events  
- `ReadyStepsFound(count)`: Ready steps discovered during initialization/evaluation
- `NoStepsFound`: No steps defined - task can complete immediately
- `NoDependenciesReady`: Dependencies not satisfied - wait required
- `DependenciesReady`: Dependencies now ready - can proceed

#### Processing Events
- `StepsEnqueued(vec<Uuid>)`: Steps successfully queued for workers  
- `EnqueueFailed(error)`: Failed to enqueue steps
- `StepCompleted(uuid)`: Individual step completed
- `StepFailed(uuid)`: Individual step failed
- `AllStepsCompleted`: All current batch steps finished
- `AllStepsSuccessful`: All steps completed successfully

#### System Events
- `PermanentFailure(error)`: Unrecoverable failure
- `RetryReady`: Retry timeout expired
- `Timeout`: Operation timeout occurred
- `ProcessorCrashed`: Processor became unavailable

### Processor Ownership (TAS-41)

The task state machine implements processor ownership for active states to prevent race conditions:

```rust
// Ownership validation in task_state_machine.rs
if target_state.requires_ownership() {
    let current_processor = self.get_current_processor().await?;
    TransitionGuard::check_ownership(target_state, current_processor, self.processor_uuid)?;
}
```

**Ownership Rules**:
- States requiring ownership: `Initializing`, `EnqueuingSteps`, `StepsInProcess`, `EvaluatingResults`
- Processor UUID stored in `tasker.task_transitions.processor_uuid` column
- Atomic ownership claiming prevents concurrent processing
- Ownership validated on each transition attempt

## Workflow Step State Machine Architecture

### Step State Definitions

The workflow step state machine implements 9 states for individual step execution:

#### Processing Pipeline States
- **`Pending`**: Initial state when step is created
- **`Enqueued`**: Queued for processing but not yet claimed by worker
- **`InProgress`**: Currently being executed by a worker
- **`EnqueuedForOrchestration`**: Worker completed, queued for orchestration processing
- **`EnqueuedAsErrorForOrchestration`**: Worker failed, queued for orchestration error processing

#### Waiting States
- **`WaitingForRetry`**: Step failed with retryable error, waiting for backoff period before retry

#### Terminal States
- **`Complete`**: Step completed successfully (after orchestration processing)
- **`Error`**: Step failed permanently (non-retryable or max retries exceeded)
- **`Cancelled`**: Step was cancelled
- **`ResolvedManually`**: Step was manually resolved by operator

#### State Machine Evolution (TAS-42)

Prior to TAS-42, the `Error` state was used for both retryable and permanent failures. The introduction of `WaitingForRetry` created a semantic change:

- **Before TAS-42**: `Error` = any failure (retryable or permanent)
- **After TAS-42**: `Error` = permanent failure only, `WaitingForRetry` = retryable failure awaiting backoff

This change required updates to:
1. `get_step_readiness_status()` to recognize `WaitingForRetry` as a ready-eligible state
2. `get_task_execution_context()` to properly detect blocked vs recovering tasks
3. Error classification logic to distinguish permanent from retryable errors

### Step State Properties

```rust
impl WorkflowStepState {
    pub fn is_terminal(&self) -> bool                    // No further transitions
    pub fn is_error(&self) -> bool                       // In error state (may allow retry)
    pub fn is_active(&self) -> bool                      // Being processed by worker
    pub fn is_in_processing_pipeline(&self) -> bool     // In execution pipeline
    pub fn is_ready_for_claiming(&self) -> bool         // Available for worker claim
    pub fn satisfies_dependencies(&self) -> bool        // Can satisfy other step dependencies
}
```

### Step Lifecycle Flow

```mermaid
stateDiagram-v2
    [*] --> Pending

    %% Main Execution Path
    Pending --> Enqueued : Enqueue
    Enqueued --> InProgress : Start (worker claims)
    InProgress --> EnqueuedForOrchestration : EnqueueForOrchestration(success)
    EnqueuedForOrchestration --> Complete : Complete(results) [orchestration]

    %% Error Handling Path (TAS-42)
    InProgress --> EnqueuedAsErrorForOrchestration : EnqueueForOrchestration(error)
    EnqueuedAsErrorForOrchestration --> WaitingForRetry : WaitForRetry(error) [retryable]
    EnqueuedAsErrorForOrchestration --> Error : Fail(error) [permanent/max retries]

    %% Retry Path
    WaitingForRetry --> Pending : Retry (after backoff)

    %% Legacy Direct Path (deprecated)
    InProgress --> Complete : Complete(results) [direct - legacy]
    InProgress --> Error : Fail(error) [direct - legacy]

    %% Legacy Backward Compatibility
    Pending --> InProgress : Start [legacy]

    %% Direct Failure Paths (error before worker processing)
    Pending --> Error : Fail(error)
    Enqueued --> Error : Fail(error)

    %% Cancellation Paths
    Pending --> Cancelled : Cancel
    Enqueued --> Cancelled : Cancel
    InProgress --> Cancelled : Cancel
    EnqueuedForOrchestration --> Cancelled : Cancel
    EnqueuedAsErrorForOrchestration --> Cancelled : Cancel
    WaitingForRetry --> Cancelled : Cancel
    Error --> Cancelled : Cancel

    %% Manual Resolution (from any state)
    Pending --> ResolvedManually : ResolveManually
    Enqueued --> ResolvedManually : ResolveManually
    InProgress --> ResolvedManually : ResolveManually
    EnqueuedForOrchestration --> ResolvedManually : ResolveManually
    EnqueuedAsErrorForOrchestration --> ResolvedManually : ResolveManually
    WaitingForRetry --> ResolvedManually : ResolveManually
    Error --> ResolvedManually : ResolveManually

    %% Terminal States
    Complete --> [*]
    Error --> [*]
    Cancelled --> [*]
    ResolvedManually --> [*]
```

### Step Event System

Step transitions are driven by `StepEvent` types:

#### Processing Events
- `Enqueue`: Queue step for worker processing
- `Start`: Begin step execution (worker claims step)
- `EnqueueForOrchestration(results)`: Worker completes, queues for orchestration
- `Complete(results)`: Mark step complete (from orchestration or legacy direct)
- `Fail(error)`: Mark step as permanently failed
- `WaitForRetry(error)`: Mark step for retry after backoff (TAS-42)

#### Control Events
- `Cancel`: Cancel step execution
- `ResolveManually`: Manual operator resolution
- `Retry`: Retry step from WaitingForRetry or Error state

### Step Execution Flow Integration

The step state machine integrates tightly with the task state machine:

1. **Task Discovers Ready Steps**: `TaskEvent::ReadyStepsFound(count)` -> Task moves to `EnqueuingSteps`
2. **Steps Get Enqueued**: `StepEvent::Enqueue` -> Steps move to `Enqueued` state
3. **Workers Claim Steps**: `StepEvent::Start` -> Steps move to `InProgress`
4. **Workers Complete Steps**: `StepEvent::EnqueueForOrchestration(results)` -> Steps move to `EnqueuedForOrchestration`
5. **Orchestration Processes Results**: `StepEvent::Complete(results)` -> Steps move to `Complete`
6. **Task Evaluates Progress**: `TaskEvent::StepCompleted(uuid)` -> Task moves to `EvaluatingResults`
7. **Task Completes or Continues**: Based on remaining steps -> Task moves to `Complete` or back to `EnqueuingSteps`

## Guard Conditions and Validation

Both state machines implement comprehensive guard conditions in `tasker-shared/src/state_machine/guards.rs`:

### Task Guards

#### TransitionGuard
- Validates all task state transitions
- Prevents invalid state combinations
- Enforces terminal state immutability
- Supports legacy transition compatibility

#### Ownership Validation
- Checks processor ownership for ownership-required states
- Prevents concurrent task processing
- Allows ownership claiming for unowned tasks

### Step Guards

#### StepDependenciesMetGuard
- Validates all step dependencies are satisfied
- Delegates to `WorkflowStep::dependencies_met()`
- Prevents premature step execution

#### StepNotInProgressGuard  
- Ensures step is not already being processed
- Prevents duplicate worker claims
- Validates step availability

#### Retry Guards
- `StepCanBeRetriedGuard`: Validates step is in Error state
- Checks retry limits and conditions
- Prevents infinite retry loops

#### Orchestration Guards
- `StepCanBeEnqueuedForOrchestrationGuard`: Step must be InProgress
- `StepCanBeCompletedFromOrchestrationGuard`: Step must be EnqueuedForOrchestration
- `StepCanBeFailedFromOrchestrationGuard`: Step must be EnqueuedForOrchestration

## Persistence Layer Architecture

### Delegation Pattern

The persistence layer in `tasker-shared/src/state_machine/persistence.rs` implements a delegation pattern to the model layer:

```rust
// TaskTransitionPersistence -> TaskTransition::create() & TaskTransition::get_current()
// StepTransitionPersistence -> WorkflowStepTransition::create() & WorkflowStepTransition::get_current()
```

**Benefits**:
- No SQL duplication between state machine and models
- Atomic transaction handling in models
- Single source of truth for database operations
- Independent testability of model methods

### Transition Storage

#### Task Transitions (`tasker.task_transitions`)
```sql
CREATE TABLE tasker.task_transitions (
  task_transition_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
  task_uuid UUID NOT NULL,
  to_state VARCHAR NOT NULL,
  from_state VARCHAR,
  processor_uuid UUID,           -- TAS-41: Ownership tracking
  metadata JSONB,
  sort_key INTEGER NOT NULL,
  most_recent BOOLEAN DEFAULT false,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

#### Step Transitions (`tasker.workflow_step_transitions`)
```sql
CREATE TABLE tasker.workflow_step_transitions (
  workflow_step_transition_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
  workflow_step_uuid UUID NOT NULL,
  to_state VARCHAR NOT NULL,
  from_state VARCHAR,
  metadata JSONB,
  sort_key INTEGER NOT NULL,
  most_recent BOOLEAN DEFAULT false,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### Current State Resolution

Both transition models implement efficient current state resolution:

```rust
// O(1) current state lookup using most_recent flag
TaskTransition::get_current(pool, task_uuid) -> Option<TaskTransition>
WorkflowStepTransition::get_current(pool, step_uuid) -> Option<WorkflowStepTransition>
```

**Performance Optimization**:
- `most_recent = true` flag on latest transition only
- Indexed queries: `(task_uuid, most_recent) WHERE most_recent = true`
- Atomic flag updates during transition creation

### Atomic Transitions with Ownership

TAS-41 introduced atomic transitions with processor ownership:

```rust
impl TaskTransitionPersistence {
    pub async fn transition_with_ownership(
        &self,
        task_uuid: Uuid,
        from_state: TaskState,
        to_state: TaskState, 
        processor_uuid: Uuid,
        metadata: Option<Value>,
        pool: &PgPool,
    ) -> PersistenceResult<bool>
}
```

**Atomicity Guarantees**:
- Single database transaction for state change
- Processor UUID stored in dedicated column
- `most_recent` flag updated atomically
- Race condition prevention through database constraints

## Action System

Both state machines execute actions after successful transitions:

### Task Actions
1. **PublishTransitionEventAction**: Publishes task state change events
2. **UpdateTaskCompletionAction**: Updates task completion status
3. **ErrorStateCleanupAction**: Performs error state cleanup

### Step Actions  
1. **PublishTransitionEventAction**: Publishes step state change events
2. **UpdateStepResultsAction**: Updates step results and execution data
3. **TriggerStepDiscoveryAction**: Triggers task-level step discovery
4. **ErrorStateCleanupAction**: Performs step error cleanup

Actions execute sequentially after transition persistence, ensuring consistency.

## State Machine Integration Points

### Task <-> Step Coordination

1. **Step Discovery**: Task initialization discovers ready steps
2. **Step Enqueueing**: Task enqueues discovered steps to worker queues  
3. **Progress Monitoring**: Task monitors step completion via events
4. **Result Processing**: Task processes step results and discovers next steps
5. **Completion Detection**: Task completes when all steps are complete

### Event-Driven Communication

- **pg_notify**: PostgreSQL notifications for real-time coordination
- **Event Publishers**: Publish state transition events to event system
- **Event Subscribers**: React to state changes across system boundaries
- **Queue Integration**: Provider-agnostic message queues (PGMQ or RabbitMQ) for worker communication

### Worker Integration

- **Step Claiming**: Workers claim `Enqueued` steps from queues
- **Progress Updates**: Workers transition steps to `InProgress` 
- **Result Submission**: Workers submit results via `EnqueueForOrchestration`
- **Orchestration Processing**: Orchestration processes results and completes steps

This sophisticated state machine architecture provides the foundation for reliable, auditable, and scalable workflow orchestration in the tasker-core system.

## Step Result Audit System (TAS-62)

The step result audit system provides SOC2-compliant audit trails for workflow step execution results, enabling complete attribution tracking for compliance and debugging.

### Audit Table Design

The `tasker.workflow_step_result_audit` table stores lightweight references with attribution data:

```sql
CREATE TABLE tasker.workflow_step_result_audit (
    workflow_step_result_audit_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    workflow_step_uuid UUID NOT NULL REFERENCES tasker.workflow_steps,
    workflow_step_transition_uuid UUID NOT NULL REFERENCES tasker.workflow_step_transitions,
    task_uuid UUID NOT NULL REFERENCES tasker.tasks,
    recorded_at TIMESTAMP NOT NULL DEFAULT NOW(),

    -- Attribution (NEW data not in transitions)
    worker_uuid UUID,
    correlation_id UUID,

    -- Extracted scalars for indexing/filtering
    success BOOLEAN NOT NULL,
    execution_time_ms BIGINT,

    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (workflow_step_uuid, workflow_step_transition_uuid)
);
```

### Design Principles

1. **No Data Duplication**: Full execution results already exist in `tasker.workflow_step_transitions.metadata`. The audit table stores references only.

2. **Attribution Capture**: The audit system captures NEW attribution data:
   - `worker_uuid`: Which worker instance processed the step
   - `correlation_id`: Distributed tracing identifier for request correlation

3. **Indexed Scalars**: Success and execution time are extracted for efficient filtering without JSON parsing.

4. **SQL Trigger**: A database trigger (`trg_step_result_audit`) guarantees audit record creation when workers persist results, ensuring SOC2 compliance.

### Attribution Flow

Attribution data flows through the system via `TransitionContext`:

```rust
// Worker creates attribution context
let context = TransitionContext::with_worker(
    worker_uuid,
    Some(correlation_id),
);

// Context is merged into transition metadata
state_machine.transition_with_context(event, Some(context)).await?;

// SQL trigger extracts attribution from metadata
-- In trigger:
-- v_worker_uuid := (NEW.metadata->>'worker_uuid')::UUID;
-- v_correlation_id := (NEW.metadata->>'correlation_id')::UUID;
```

### Trigger Behavior

The `create_step_result_audit` trigger fires on transitions to:
- `enqueued_for_orchestration`: Successful step completion
- `enqueued_as_error_for_orchestration`: Failed step completion

These states represent when workers persist execution results, creating the audit trail.

### Querying Audit History

#### Via API

```
GET /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}/audit
```

Returns audit records with full transition details via JOIN, ordered by `recorded_at` DESC.

#### Via Client

```rust
let audit_history = client.get_step_audit_history(task_uuid, step_uuid).await?;
for record in audit_history {
    println!("Worker: {:?}, Success: {}, Time: {:?}ms",
        record.worker_uuid,
        record.success,
        record.execution_time_ms
    );
}
```

#### Via Model

```rust
// Get audit history for a step with full transition details
let history = WorkflowStepResultAudit::get_audit_history(&pool, step_uuid).await?;

// Get all audit records for a task
let task_history = WorkflowStepResultAudit::get_task_audit_history(&pool, task_uuid).await?;

// Query by worker for attribution investigation
let worker_records = WorkflowStepResultAudit::get_by_worker(&pool, worker_uuid, Some(100)).await?;

// Query by correlation ID for distributed tracing
let correlated = WorkflowStepResultAudit::get_by_correlation_id(&pool, correlation_id).await?;
```

### Indexes for Common Query Patterns

The audit table includes optimized indexes:

- `idx_audit_step_uuid`: Primary query - get audit history for a step
- `idx_audit_task_uuid`: Get all audit records for a task
- `idx_audit_recorded_at`: Time-range queries for SOC2 audit reports
- `idx_audit_worker_uuid`: Attribution investigation (partial index)
- `idx_audit_correlation_id`: Distributed tracing queries (partial index)
- `idx_audit_success`: Success/failure filtering

### Historical Data

The migration includes a backfill for existing transitions. Historical records will have NULL attribution (worker_uuid, correlation_id) since that data wasn't captured before TAS-62.
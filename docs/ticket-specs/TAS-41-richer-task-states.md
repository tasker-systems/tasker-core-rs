# TAS-41: Richer Task States - Eliminating Claims Through Unified State Management

## Executive Summary

Replace three overlapping state management systems (task states, execution status, and task claims) with a single, comprehensive task state machine. This eliminates the entire claim subsystem while maintaining all safety guarantees through atomic state transitions with processor ownership tracking.

## Current Problem

The system maintains three separate state representations that evolved due to an oversimplified initial state machine:

1. **Task States**: `pending`, `in_progress`, `complete`, `error`, `cancelled`, `resolved_manually`
2. **Execution Status** (computed): `has_ready_steps`, `processing`, `blocked_by_failures`, etc.
3. **Task Claims** (removed): Previously tracked ownership and prevented race conditions

The root issue: **"in_progress" hides at least 5 distinct operational states**, leading to complex workarounds.

## Solution: Unified Rich State Machine

### State Machine Implementation

#### 1. Task States (`tasker-shared/src/state_machine/states.rs`)

```rust
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    // Initial states
    Pending,                    // Created but not started
    Initializing,              // Discovering initial ready steps

    // Active processing states
    EnqueuingSteps,           // Actively enqueuing ready steps to queues
    StepsInProcess,           // Steps are being processed by workers
    EvaluatingResults,        // Processing results from completed steps

    // Waiting states
    WaitingForDependencies,   // No ready steps, waiting for dependencies
    WaitingForRetry,          // Waiting for retry timeout

    // Problem states
    BlockedByFailures,        // Has failures that prevent progress

    // Terminal states
    Complete,                 // All steps completed successfully
    Error,                    // Task failed permanently
    Cancelled,                // Task was cancelled
    ResolvedManually,         // Manually resolved by operator
}

impl TaskState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self,
            TaskState::Complete |
            TaskState::Error |
            TaskState::Cancelled |
            TaskState::ResolvedManually
        )
    }

    /// Check if this state requires processor ownership
    pub fn requires_ownership(&self) -> bool {
        matches!(self,
            TaskState::Initializing |
            TaskState::EnqueuingSteps |
            TaskState::StepsInProcess |
            TaskState::EvaluatingResults
        )
    }

    /// Check if this is an active processing state
    pub fn is_active(&self) -> bool {
        matches!(self,
            TaskState::Initializing |
            TaskState::EnqueuingSteps |
            TaskState::StepsInProcess |
            TaskState::EvaluatingResults
        )
    }

    /// Check if this is a waiting state
    pub fn is_waiting(&self) -> bool {
        matches!(self,
            TaskState::WaitingForDependencies |
            TaskState::WaitingForRetry |
            TaskState::BlockedByFailures
        )
    }

    /// Get the database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Initializing => "initializing",
            TaskState::EnqueuingSteps => "enqueuing_steps",
            TaskState::StepsInProcess => "steps_in_process",
            TaskState::EvaluatingResults => "evaluating_results",
            TaskState::WaitingForDependencies => "waiting_for_dependencies",
            TaskState::WaitingForRetry => "waiting_for_retry",
            TaskState::BlockedByFailures => "blocked_by_failures",
            TaskState::Complete => "complete",
            TaskState::Error => "error",
            TaskState::Cancelled => "cancelled",
            TaskState::ResolvedManually => "resolved_manually",
        }
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<TaskState> for String {
    fn from(state: TaskState) -> Self {
        state.as_str().to_string()
    }
}

impl TryFrom<&str> for TaskState {
    type Error = StateParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(TaskState::Pending),
            "initializing" => Ok(TaskState::Initializing),
            "enqueuing_steps" => Ok(TaskState::EnqueuingSteps),
            "steps_in_process" => Ok(TaskState::StepsInProcess),
            "evaluating_results" => Ok(TaskState::EvaluatingResults),
            "waiting_for_dependencies" => Ok(TaskState::WaitingForDependencies),
            "waiting_for_retry" => Ok(TaskState::WaitingForRetry),
            "blocked_by_failures" => Ok(TaskState::BlockedByFailures),
            "complete" => Ok(TaskState::Complete),
            "error" => Ok(TaskState::Error),
            "cancelled" => Ok(TaskState::Cancelled),
            "resolved_manually" => Ok(TaskState::ResolvedManually),
            _ => Err(StateParseError::InvalidState(value.to_string())),
        }
    }
}
```

#### 2. State Transition Guards (`tasker-shared/src/state_machine/guards.rs`)

```rust
use super::{TaskState, TaskEvent};
use crate::models::Task;
use uuid::Uuid;

/// Guard conditions for state transitions
pub struct TransitionGuard;

impl TransitionGuard {
    /// Check if a transition is valid
    pub fn can_transition(
        from: TaskState,
        to: TaskState,
        event: &TaskEvent,
        task: &Task,
    ) -> Result<(), GuardError> {
        // Terminal states cannot transition
        if from.is_terminal() {
            return Err(GuardError::TerminalState { state: from });
        }

        // Validate specific transitions
        match (from, to, event) {
            // Initial transitions
            (TaskState::Pending, TaskState::Initializing, TaskEvent::Start) => Ok(()),

            // From Initializing
            (TaskState::Initializing, TaskState::EnqueuingSteps, TaskEvent::ReadyStepsFound(_)) => Ok(()),
            (TaskState::Initializing, TaskState::Complete, TaskEvent::NoStepsFound) => Ok(()),
            (TaskState::Initializing, TaskState::WaitingForDependencies, TaskEvent::NoDependenciesReady) => Ok(()),

            // From EnqueuingSteps
            (TaskState::EnqueuingSteps, TaskState::StepsInProcess, TaskEvent::StepsEnqueued(_)) => Ok(()),
            (TaskState::EnqueuingSteps, TaskState::Error, TaskEvent::EnqueueFailed(_)) => Ok(()),

            // From StepsInProcess
            (TaskState::StepsInProcess, TaskState::EvaluatingResults, TaskEvent::AllStepsCompleted) => Ok(()),
            (TaskState::StepsInProcess, TaskState::EvaluatingResults, TaskEvent::StepCompleted(_)) => Ok(()),
            (TaskState::StepsInProcess, TaskState::WaitingForRetry, TaskEvent::StepFailed(_)) => Ok(()),

            // From EvaluatingResults
            (TaskState::EvaluatingResults, TaskState::Complete, TaskEvent::AllStepsSuccessful) => Ok(()),
            (TaskState::EvaluatingResults, TaskState::EnqueuingSteps, TaskEvent::ReadyStepsFound(_)) => Ok(()),
            (TaskState::EvaluatingResults, TaskState::WaitingForDependencies, TaskEvent::NoDependenciesReady) => Ok(()),
            (TaskState::EvaluatingResults, TaskState::BlockedByFailures, TaskEvent::PermanentFailure(_)) => Ok(()),

            // From waiting states
            (TaskState::WaitingForDependencies, TaskState::EvaluatingResults, TaskEvent::DependenciesReady) => Ok(()),
            (TaskState::WaitingForRetry, TaskState::EnqueuingSteps, TaskEvent::RetryReady) => Ok(()),
            (TaskState::BlockedByFailures, TaskState::Error, TaskEvent::GiveUp) => Ok(()),
            (TaskState::BlockedByFailures, TaskState::ResolvedManually, TaskEvent::ManualResolution) => Ok(()),

            // Cancellation from any non-terminal state
            (from, TaskState::Cancelled, TaskEvent::Cancel) if !from.is_terminal() => Ok(()),

            // Invalid transition
            _ => Err(GuardError::InvalidTransition { from, to, event: event.clone() }),
        }
    }

    /// Check processor ownership for transitions requiring it
    pub fn check_ownership(
        state: TaskState,
        task_processor: Option<Uuid>,
        requesting_processor: Uuid,
    ) -> Result<(), GuardError> {
        if !state.requires_ownership() {
            return Ok(());
        }

        match task_processor {
            Some(owner) if owner == requesting_processor => Ok(()),
            Some(owner) => Err(GuardError::NotOwner {
                state,
                owner,
                requesting: requesting_processor,
            }),
            None => Ok(()), // No owner yet, can claim
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GuardError {
    #[error("Cannot transition from terminal state {state:?}")]
    TerminalState { state: TaskState },

    #[error("Invalid transition from {from:?} to {to:?} with event {event:?}")]
    InvalidTransition {
        from: TaskState,
        to: TaskState,
        event: TaskEvent
    },

    #[error("Processor {requesting} does not own task in state {state:?} (owned by {owner})")]
    NotOwner {
        state: TaskState,
        owner: Uuid,
        requesting: Uuid,
    },
}
```

#### 3. State Machine Events (`tasker-shared/src/state_machine/events.rs`)

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskEvent {
    // Lifecycle events
    Start,
    Cancel,
    GiveUp,
    ManualResolution,

    // Discovery events
    ReadyStepsFound(u32),
    NoStepsFound,
    NoDependenciesReady,
    DependenciesReady,

    // Processing events
    StepsEnqueued(Vec<Uuid>),
    EnqueueFailed(String),
    StepCompleted(Uuid),
    StepFailed(Uuid),
    AllStepsCompleted,
    AllStepsSuccessful,

    // Failure events
    PermanentFailure(String),
    RetryReady,

    // System events
    Timeout,
    ProcessorCrashed,
}

impl TaskEvent {
    /// Check if this event requires processor ownership
    pub fn requires_ownership(&self) -> bool {
        matches!(self,
            TaskEvent::Start |
            TaskEvent::ReadyStepsFound(_) |
            TaskEvent::StepsEnqueued(_) |
            TaskEvent::StepCompleted(_) |
            TaskEvent::AllStepsCompleted |
            TaskEvent::AllStepsSuccessful
        )
    }
}
```

#### 4. State Transition Actions (`tasker-shared/src/state_machine/actions.rs`)

```rust
use super::{TaskState, TaskEvent};
use crate::models::Task;
use crate::events::EventPublisher;
use sqlx::PgPool;
use uuid::Uuid;

/// Actions to perform during state transitions
pub struct TransitionActions {
    pool: PgPool,
    event_publisher: Option<EventPublisher>,
}

impl TransitionActions {
    pub fn new(pool: PgPool, event_publisher: Option<EventPublisher>) -> Self {
        Self { pool, event_publisher }
    }

    /// Execute actions for a state transition
    pub async fn execute(
        &self,
        task: &Task,
        from_state: TaskState,
        to_state: TaskState,
        event: &TaskEvent,
        processor_uuid: Uuid,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), ActionError> {
        // Pre-transition actions
        self.pre_transition_actions(task, from_state, to_state, event).await?;

        // Record the transition
        self.record_transition(
            task.task_uuid,
            from_state,
            to_state,
            processor_uuid,
            metadata,
        ).await?;

        // Post-transition actions
        self.post_transition_actions(task, from_state, to_state, event).await?;

        // Publish events if configured
        if let Some(publisher) = &self.event_publisher {
            self.publish_transition_event(
                publisher,
                task,
                from_state,
                to_state,
                event,
            ).await?;
        }

        Ok(())
    }

    async fn pre_transition_actions(
        &self,
        task: &Task,
        from: TaskState,
        to: TaskState,
        event: &TaskEvent,
    ) -> Result<(), ActionError> {
        match (from, to) {
            // Clear processor when moving to waiting states
            (_, TaskState::WaitingForDependencies) |
            (_, TaskState::WaitingForRetry) |
            (_, TaskState::BlockedByFailures) => {
                self.clear_processor_ownership(task.task_uuid).await?;
            }

            // Set completion timestamp when moving to Complete
            (_, TaskState::Complete) => {
                self.set_task_completed(task.task_uuid).await?;
            }

            _ => {}
        }

        Ok(())
    }

    async fn record_transition(
        &self,
        task_uuid: Uuid,
        from_state: TaskState,
        to_state: TaskState,
        processor_uuid: Uuid,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), ActionError> {
        sqlx::query!(
            r#"
            INSERT INTO tasker_task_transitions (
                task_uuid, from_state, to_state, processor_uuid,
                transition_metadata, sort_key, most_recent, created_at
            )
            SELECT $1, $2, $3, $4, $5,
                   COALESCE(MAX(sort_key), 0) + 1, true, NOW()
            FROM tasker_task_transitions
            WHERE task_uuid = $1
            "#,
            task_uuid,
            from_state.as_str(),
            to_state.as_str(),
            processor_uuid,
            metadata,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn post_transition_actions(
        &self,
        task: &Task,
        from: TaskState,
        to: TaskState,
        event: &TaskEvent,
    ) -> Result<(), ActionError> {
        // Log important transitions
        match to {
            TaskState::Complete => {
                info!(task_uuid = %task.task_uuid, "Task completed successfully");
            }
            TaskState::Error => {
                error!(task_uuid = %task.task_uuid, "Task failed permanently");
            }
            TaskState::BlockedByFailures => {
                warn!(task_uuid = %task.task_uuid, "Task blocked by failures");
            }
            _ => {}
        }

        Ok(())
    }
}
```

#### 5. State Machine Errors (`tasker-shared/src/state_machine/errors.rs`)

```rust
use super::TaskState;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StateMachineError {
    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Invalid transition from {from:?} to {to:?}")]
    InvalidTransition { from: TaskState, to: TaskState },

    #[error("State parse error: {0}")]
    StateParseError(#[from] StateParseError),

    #[error("Guard error: {0}")]
    GuardError(#[from] GuardError),

    #[error("Action error: {0}")]
    ActionError(#[from] ActionError),

    #[error("Persistence error: {0}")]
    PersistenceError(#[from] PersistenceError),

    #[error("Ownership error: processor {processor} does not own task {task_uuid}")]
    OwnershipError { task_uuid: Uuid, processor: Uuid },

    #[error("Concurrent modification: task state changed during operation")]
    ConcurrentModification,
}

#[derive(Debug, Error)]
pub enum StateParseError {
    #[error("Invalid state string: {0}")]
    InvalidState(String),
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Event publishing failed: {0}")]
    EventPublishing(String),

    #[error("Pre-transition action failed: {0}")]
    PreTransition(String),

    #[error("Post-transition action failed: {0}")]
    PostTransition(String),
}

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Transition not found")]
    TransitionNotFound,

    #[error("Task not found: {0}")]
    TaskNotFound(Uuid),
}
```

#### 6. State Persistence (`tasker-shared/src/state_machine/persistence.rs`)

```rust
use super::{TaskState, TaskEvent};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Handles persistence of state transitions
pub struct StatePersistence {
    pool: PgPool,
}

impl StatePersistence {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get current state of a task
    pub async fn get_current_state(&self, task_uuid: Uuid) -> Result<Option<TaskState>, PersistenceError> {
        let row = sqlx::query!(
            r#"
            SELECT to_state
            FROM tasker_task_transitions
            WHERE task_uuid = $1 AND most_recent = true
            "#,
            task_uuid
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(TaskState::try_from(r.to_state.as_str())?)),
            None => Ok(None),
        }
    }

    /// Get full transition history for a task
    pub async fn get_transition_history(
        &self,
        task_uuid: Uuid,
    ) -> Result<Vec<TransitionRecord>, PersistenceError> {
        let rows = sqlx::query_as!(
            TransitionRecord,
            r#"
            SELECT
                task_uuid,
                from_state,
                to_state as "to_state!",
                processor_uuid,
                transition_metadata as metadata,
                created_at
            FROM tasker_task_transitions
            WHERE task_uuid = $1
            ORDER BY sort_key ASC
            "#,
            task_uuid
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Atomically transition task state with ownership check
    pub async fn transition_with_ownership(
        &self,
        task_uuid: Uuid,
        from_state: TaskState,
        to_state: TaskState,
        processor_uuid: Uuid,
        metadata: Option<serde_json::Value>,
    ) -> Result<bool, PersistenceError> {
        // Use the SQL function we defined
        let result = sqlx::query_scalar!(
            r#"SELECT transition_task_state_atomic($1, $2, $3, $4, $5) as "success!""#,
            task_uuid,
            from_state.as_str(),
            to_state.as_str(),
            processor_uuid,
            metadata.unwrap_or(serde_json::json!({}))
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    /// Get processor that owns a task in its current state
    pub async fn get_current_processor(&self, task_uuid: Uuid) -> Result<Option<Uuid>, PersistenceError> {
        let row = sqlx::query!(
            r#"
            SELECT processor_uuid
            FROM tasker_task_transitions
            WHERE task_uuid = $1 AND most_recent = true
            "#,
            task_uuid
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|r| r.processor_uuid))
    }
}

#[derive(Debug, Clone)]
pub struct TransitionRecord {
    pub task_uuid: Uuid,
    pub from_state: Option<String>,
    pub to_state: String,
    pub processor_uuid: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}
```

### State Ownership Through Transitions

Instead of separate claims, processor ownership is tracked in state transition metadata:

```sql
-- State transition with processor ownership
INSERT INTO tasker_task_transitions (
    task_uuid, from_state, to_state,
    processor_uuid, transition_metadata,
    sort_key, most_recent, created_at
) VALUES (
    $1, $2, $3,
    $4, -- processor_uuid from SystemContext.system_id
    jsonb_build_object(
        'processor_hostname', $5,
        'timeout_seconds', 300,
        'ready_steps_count', $6
    ),
    $7, true, NOW()
);
```

## Implementation Plan

### Phase 1: Database Schema Updates

**File**: `migrations/20250810140000_uuid_v7_initial_schema.sql`

#### 1.1 Add Processor Tracking to Transitions Table (New Lines After Line 357)

```sql
-- Add processor tracking and metadata to transitions
ALTER TABLE tasker_task_transitions
ADD COLUMN processor_uuid UUID,
ADD COLUMN transition_metadata JSONB DEFAULT '{}';

-- Index for processor ownership queries
CREATE INDEX idx_task_transitions_processor
ON tasker_task_transitions(processor_uuid)
WHERE processor_uuid IS NOT NULL;

-- Index for timeout monitoring
CREATE INDEX idx_task_transitions_timeout
ON tasker_task_transitions((transition_metadata->>'timeout_at'))
WHERE most_recent = true;
```

#### 1.2 Update State Validation Constraints (Lines 1798-1829)

```sql
-- Replace lines 1798-1807 (to_state constraint)
ALTER TABLE public.tasker_task_transitions
ADD CONSTRAINT chk_task_transitions_to_state
CHECK (to_state IN (
    'pending',
    'initializing',
    'enqueuing_steps',
    'steps_in_process',
    'evaluating_results',
    'waiting_for_dependencies',
    'waiting_for_retry',
    'blocked_by_failures',
    'complete',
    'error',
    'cancelled',
    'resolved_manually'
));

-- Replace lines 1818-1829 (from_state constraint)
ALTER TABLE public.tasker_task_transitions
ADD CONSTRAINT chk_task_transitions_from_state
CHECK (from_state IS NULL OR from_state IN (
    'pending',
    'initializing',
    'enqueuing_steps',
    'steps_in_process',
    'evaluating_results',
    'waiting_for_dependencies',
    'waiting_for_retry',
    'blocked_by_failures',
    'complete',
    'error',
    'cancelled',
    'resolved_manually'
));
```

#### 1.3 Update get_system_health_counts Function (Line 1381)

Replace the `in_progress_tasks` count with granular state counts:

```sql
-- Replace line that counts in_progress_tasks
COUNT(*) FILTER (WHERE task_state.to_state = 'initializing') as initializing_tasks,
COUNT(*) FILTER (WHERE task_state.to_state = 'enqueuing_steps') as enqueuing_steps_tasks,
COUNT(*) FILTER (WHERE task_state.to_state = 'steps_in_process') as steps_in_process_tasks,
COUNT(*) FILTER (WHERE task_state.to_state = 'evaluating_results') as evaluating_results_tasks,
COUNT(*) FILTER (WHERE task_state.to_state = 'waiting_for_dependencies') as waiting_for_dependencies_tasks,
COUNT(*) FILTER (WHERE task_state.to_state = 'waiting_for_retry') as waiting_for_retry_tasks,
COUNT(*) FILTER (WHERE task_state.to_state = 'blocked_by_failures') as blocked_by_failures_tasks,
```

### Phase 2: Add New SQL Functions

Add these functions after the existing functions section (around line 1900):

#### 2.1 Atomic State Transition Function

```sql
CREATE OR REPLACE FUNCTION transition_task_state_atomic(
    p_task_uuid UUID,
    p_from_state VARCHAR,
    p_to_state VARCHAR,
    p_processor_uuid UUID,
    p_metadata JSONB DEFAULT '{}'
) RETURNS BOOLEAN AS $$
DECLARE
    v_sort_key INTEGER;
    v_transitioned BOOLEAN := FALSE;
BEGIN
    -- Get next sort key
    SELECT COALESCE(MAX(sort_key), 0) + 1 INTO v_sort_key
    FROM tasker_task_transitions
    WHERE task_uuid = p_task_uuid;

    -- Atomically transition only if in expected state
    WITH current_state AS (
        SELECT to_state, processor_uuid
        FROM tasker_task_transitions
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        FOR UPDATE
    ),
    ownership_check AS (
        SELECT
            CASE
                -- States that require ownership check
                WHEN cs.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
                THEN cs.processor_uuid = p_processor_uuid OR cs.processor_uuid IS NULL
                -- Other states don't require ownership
                ELSE true
            END as can_transition
        FROM current_state cs
        WHERE cs.to_state = p_from_state
    ),
    do_update AS (
        UPDATE tasker_task_transitions
        SET most_recent = false
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        AND EXISTS (SELECT 1 FROM ownership_check WHERE can_transition)
        RETURNING task_uuid
    )
    INSERT INTO tasker_task_transitions (
        task_uuid, from_state, to_state,
        processor_uuid, transition_metadata,
        sort_key, most_recent, created_at
    )
    SELECT
        p_task_uuid, p_from_state, p_to_state,
        p_processor_uuid, p_metadata,
        v_sort_key, true, NOW()
    WHERE EXISTS (SELECT 1 FROM do_update);

    GET DIAGNOSTICS v_transitioned = ROW_COUNT;
    RETURN v_transitioned > 0;
END;
$$ LANGUAGE plpgsql;
```

#### 2.2 Get Next Ready Task Function

```sql
CREATE OR REPLACE FUNCTION get_next_ready_task()
RETURNS TABLE(
    task_uuid UUID,
    task_name TEXT,
    priority INTEGER,
    namespace_name TEXT,
    ready_steps_count BIGINT,
    computed_priority NUMERIC
) AS $$
BEGIN
    -- Simply delegate to batch function with limit of 1
    RETURN QUERY
    SELECT * FROM get_next_ready_tasks(1);
END;
$$ LANGUAGE plpgsql;
```

#### 2.3 Get Next Ready Tasks (Batch) Function

```sql
CREATE OR REPLACE FUNCTION get_next_ready_tasks(p_limit INTEGER DEFAULT 5)
RETURNS TABLE(
    task_uuid UUID,
    task_name TEXT,
    priority INTEGER,
    namespace_name TEXT,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
) AS $$
BEGIN
    RETURN QUERY
    WITH task_candidates AS (
        SELECT
            t.task_uuid,
            t.task_name,
            t.priority,
            t.namespace_name,
            tt.to_state as current_state,
            -- Compute priority with age escalation
            t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1) as computed_priority
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        LEFT JOIN tasker_task_transitions tt_processing
            ON tt_processing.task_uuid = t.task_uuid
            AND tt_processing.most_recent = true
            AND tt_processing.processor_uuid IS NOT NULL
            AND tt_processing.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
        WHERE tt.most_recent = true
        -- Tasks that can be picked up for processing
        AND tt.to_state IN ('pending', 'waiting_for_dependencies', 'waiting_for_retry')
        AND tt_processing.task_uuid IS NULL  -- Not already being processed
        ORDER BY computed_priority DESC, t.created_at ASC
        LIMIT p_limit * 10  -- Pre-filter more for batch
    ),
    task_with_context AS (
        SELECT
            tc.*,
            ctx.ready_steps,
            ctx.execution_status
        FROM task_candidates tc
        CROSS JOIN LATERAL get_task_execution_context(tc.task_uuid) ctx
        -- Pending tasks don't have steps yet, others must have ready steps
        WHERE (tc.current_state = 'pending')
           OR (tc.current_state != 'pending' AND ctx.execution_status = 'has_ready_steps')
    )
    SELECT
        twc.task_uuid,
        twc.task_name,
        twc.priority,
        twc.namespace_name,
        COALESCE(twc.ready_steps, 0) as ready_steps_count,
        twc.computed_priority,
        twc.current_state
    FROM task_with_context twc
    ORDER BY twc.computed_priority DESC
    LIMIT p_limit
    FOR UPDATE SKIP LOCKED;
END;
$$ LANGUAGE plpgsql;
```

#### 2.4 Find Stuck Tasks Function

```sql
CREATE OR REPLACE FUNCTION find_stuck_tasks(
    p_timeout_minutes INTEGER DEFAULT 10
)
RETURNS TABLE(
    task_uuid UUID,
    current_state VARCHAR,
    processor_uuid UUID,
    stuck_duration_minutes INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        t.task_uuid,
        t.to_state,
        t.processor_uuid,
        EXTRACT(EPOCH FROM (NOW() - t.created_at))::INTEGER / 60 as stuck_duration_minutes
    FROM tasker_task_transitions t
    WHERE t.most_recent = true
    AND t.to_state IN (
        'initializing', 'enqueuing_steps',
        'steps_in_process', 'evaluating_results'
    )
    AND t.created_at < NOW() - INTERVAL '1 minute' * p_timeout_minutes;
END;
$$ LANGUAGE plpgsql;
```

#### 2.5 Get Current Task State Function

```sql
CREATE OR REPLACE FUNCTION get_current_task_state(p_task_uuid UUID)
RETURNS VARCHAR AS $$
DECLARE
    v_state VARCHAR;
BEGIN
    SELECT to_state INTO v_state
    FROM tasker_task_transitions
    WHERE task_uuid = p_task_uuid
    AND most_recent = true;

    RETURN v_state;
END;
$$ LANGUAGE plpgsql;
```

### Phase 3: Rust Code Updates

#### 3.1 State Machine Integration

The comprehensive TaskState enum defined in Section 1 above provides all necessary functionality. No additional state machine updates are needed as the full implementation is already specified.

#### 3.2 Add Task State Machine Coordinator (`tasker-shared/src/state_machine/task_state_machine.rs`)

Create a coordinator that integrates Guards, Actions, and Persistence for idiomatic state machine usage:

```rust
use super::{TaskState, TaskEvent, TransitionGuard, TransitionActions, StatePersistence};
use crate::models::Task;
use sqlx::PgPool;
use uuid::Uuid;
use std::sync::Arc;

/// Coordinates state machine components for task state transitions
pub struct TaskStateMachine {
    task_uuid: Uuid,
    current_state: TaskState,
    persistence: StatePersistence,
    actions: TransitionActions,
    processor_uuid: Uuid,
}

impl TaskStateMachine {
    /// Create state machine for existing task
    pub async fn for_task(
        task_uuid: Uuid,
        pool: PgPool,
        processor_uuid: Uuid,
    ) -> Result<Self, StateMachineError> {
        let persistence = StatePersistence::new(pool.clone());
        let actions = TransitionActions::new(pool, None);

        let current_state = persistence
            .get_current_state(task_uuid)
            .await?
            .unwrap_or(TaskState::Pending);

        Ok(Self {
            task_uuid,
            current_state,
            persistence,
            actions,
            processor_uuid,
        })
    }

    /// Get current state
    pub fn current_state(&self) -> TaskState {
        self.current_state
    }

    /// Attempt to transition to a new state with the given event
    pub async fn transition(&mut self, event: TaskEvent) -> Result<bool, StateMachineError> {
        let target_state = self.determine_target_state(&event)?;

        // Get task for guard validation
        let task = Task::find_by_id(&self.persistence.pool, self.task_uuid).await?;

        // Validate transition with guards
        TransitionGuard::can_transition(
            self.current_state,
            target_state,
            &event,
            &task,
        )?;

        // Check ownership if required
        TransitionGuard::check_ownership(
            target_state,
            Some(self.processor_uuid),
            self.processor_uuid,
        )?;

        // Execute the transition atomically
        let metadata = Some(serde_json::json!({
            "event": format!("{:?}", event),
            "processor_uuid": self.processor_uuid,
        }));

        let success = self.persistence.transition_with_ownership(
            self.task_uuid,
            self.current_state,
            target_state,
            self.processor_uuid,
            metadata.clone(),
        ).await?;

        if success {
            // Execute side effects
            self.actions.execute(
                &task,
                self.current_state,
                target_state,
                &event,
                self.processor_uuid,
                metadata,
            ).await?;

            self.current_state = target_state;
        }

        Ok(success)
    }

    /// Determine target state based on current state and event
    fn determine_target_state(&self, event: &TaskEvent) -> Result<TaskState, StateMachineError> {
        use TaskState::*;
        use TaskEvent::*;

        let target = match (self.current_state, event) {
            // From Pending
            (Pending, Start) => Initializing,

            // From Initializing
            (Initializing, ReadyStepsFound(_)) => EnqueuingSteps,
            (Initializing, NoStepsFound) => Complete,
            (Initializing, NoDependenciesReady) => WaitingForDependencies,

            // From EnqueuingSteps
            (EnqueuingSteps, StepsEnqueued(_)) => StepsInProcess,
            (EnqueuingSteps, EnqueueFailed(_)) => Error,

            // From StepsInProcess
            (StepsInProcess, AllStepsCompleted) => EvaluatingResults,
            (StepsInProcess, StepCompleted(_)) => EvaluatingResults,
            (StepsInProcess, StepFailed(_)) => WaitingForRetry,

            // From EvaluatingResults
            (EvaluatingResults, AllStepsSuccessful) => Complete,
            (EvaluatingResults, ReadyStepsFound(_)) => EnqueuingSteps,
            (EvaluatingResults, NoDependenciesReady) => WaitingForDependencies,
            (EvaluatingResults, PermanentFailure(_)) => BlockedByFailures,

            // From waiting states
            (WaitingForDependencies, DependenciesReady) => EvaluatingResults,
            (WaitingForRetry, RetryReady) => EnqueuingSteps,
            (BlockedByFailures, GiveUp) => Error,
            (BlockedByFailures, ManualResolution) => ResolvedManually,

            // Cancellation from any non-terminal state
            (state, Cancel) if !state.is_terminal() => Cancelled,

            // Invalid combinations
            _ => return Err(StateMachineError::InvalidTransition {
                from: self.current_state,
                to_event: event.clone(),
            }),
        };

        Ok(target)
    }
}

/// Extended error type for state machine operations
#[derive(Debug, thiserror::Error)]
pub enum StateMachineError {
    #[error("Invalid transition from {from:?} with event {to_event:?}")]
    InvalidTransition { from: TaskState, to_event: TaskEvent },

    #[error("Guard error: {0}")]
    GuardError(#[from] GuardError),

    #[error("Action error: {0}")]
    ActionError(#[from] ActionError),

    #[error("Persistence error: {0}")]
    PersistenceError(#[from] PersistenceError),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}
```

**Benefits of State Machine Approach**:
- **Proper separation of concerns**: Guards validate, Actions handle side effects, Persistence manages storage
- **Type safety**: All transitions validated at compile time
- **Extensibility**: Easy to add new guards, actions, or events
- **Testability**: Each component can be tested independently
- **Maintainability**: Clear responsibilities and single source of truth

#### 3.3 Add Query Methods to SqlFunctionExecutor (`tasker-shared/src/database/sql_functions.rs`)

Query operations (read-only) remain in SqlFunctionExecutor, while mutations go through the state machine:

```rust
impl SqlFunctionExecutor {
    /// Get next ready task with type-safe result
    pub async fn get_next_ready_task(&self) -> Result<Option<ReadyTaskInfo>, sqlx::Error> {
        sqlx::query_as!(
            ReadyTaskInfo,
            r#"
            SELECT
                task_uuid as "task_uuid!",
                task_name as "task_name!",
                priority as "priority!",
                namespace_name as "namespace_name!",
                ready_steps_count as "ready_steps_count!",
                computed_priority as "computed_priority!",
                current_state as "current_state!"
            FROM get_next_ready_task()
            "#
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Get batch of ready tasks with their current states
    pub async fn get_next_ready_tasks(&self, limit: i32) -> Result<Vec<ReadyTaskInfo>, sqlx::Error> {
        sqlx::query_as!(
            ReadyTaskInfo,
            r#"
            SELECT
                task_uuid as "task_uuid!",
                task_name as "task_name!",
                priority as "priority!",
                namespace_name as "namespace_name!",
                ready_steps_count as "ready_steps_count!",
                computed_priority as "computed_priority!",
                current_state as "current_state!"
            FROM get_next_ready_tasks($1)
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Get current state of a task
    pub async fn get_current_task_state(&self, task_uuid: Uuid) -> Result<TaskState, sqlx::Error> {
        let state_str = sqlx::query_scalar!(
            r#"SELECT get_current_task_state($1) as "state""#,
            task_uuid
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;

        TaskState::try_from(state_str.as_str())
            .map_err(|_| sqlx::Error::Decode("Invalid task state".into()))
    }
}

/// Task info for batch processing with current state
#[derive(Debug, Clone)]
pub struct ReadyTaskInfo {
    pub task_uuid: Uuid,
    pub task_name: String,
    pub priority: i32,
    pub namespace_name: String,
    pub ready_steps_count: i64,
    pub computed_priority: f64,
    pub current_state: String,
}
```

#### 3.4 Update TaskClaimStepEnqueuer (`task_claim_step_enqueuer.rs`)

Use SqlFunctionExecutor for queries and TaskStateMachine for mutations:

```rust
impl TaskClaimStepEnqueuer {
    pub async fn process_batch(&self) -> TaskerResult<TaskClaimStepEnqueueCycleResult> {
        let start = Instant::now();

        // Get ready tasks using SqlFunctionExecutor (query operation)
        let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());
        let ready_tasks = sql_executor
            .get_next_ready_tasks(self.config.max_batch_size)
            .await?;

        // Process tasks concurrently using state machine
        let context = self.context.clone();
        let step_enqueuer = Arc::clone(&self.step_enqueuer);
        let mut task_futures = Vec::new();

        for task_info in ready_tasks {
            let system_context = Arc::clone(&context);
            let task_enqueuer = Arc::clone(&step_enqueuer);

            let future = tokio::spawn(async move {
                Self::process_single_task(
                    &task_info,
                    system_context,
                    system_id,
                    task_enqueuer,
                ).await.map(|success| (task_info.task_uuid, success))
            });

            task_futures.push(future);
        }

        // Wait for all tasks to complete
        let results = futures::future::join_all(task_futures).await;
        let mut processed = 0;
        let mut failed = 0;

        for result in results {
            match result {
                Ok(Ok((_, true))) => processed += 1,
                Ok(Ok((_, false))) => {}, // Already processing elsewhere
                _ => failed += 1,
            }
        }

        info!(
            processed,
            failed,
            batch_size = ready_tasks.len(),
            duration_ms = start.elapsed().as_millis(),
            "Batch processing complete"
        );

        Ok(TaskClaimStepEnqueueCycleResult {
            processed_count: processed,
            failed_count: failed
        })
    }

    /// Process a single task with proper state machine transitions
    async fn process_single_task(
        task_info: &ReadyTaskInfo,
        context: Arc<SystemContext>,
        step_enqueuer: Arc<StepEnqueuer>,
    ) -> TaskerResult<bool> {
        // Create state machine for this task
        let mut state_machine = TaskStateMachine::for_task(
            task_info.task_uuid,
            context.database_pool().clone(),
            context.system_id(),
        ).await?;

        // Handle state-specific transitions using proper state machine
        match state_machine.current_state() {
            TaskState::Pending => {
                // Start the task
                if state_machine.transition(TaskEvent::Start).await? {
                    Self::handle_initializing_task(&mut state_machine, task_info, &step_enqueuer).await
                } else {
                    Ok(false) // Already claimed by another processor
                }
            },

            TaskState::WaitingForDependencies => {
                // Dependencies became ready
                if state_machine.transition(TaskEvent::DependenciesReady).await? {
                    Self::handle_evaluating_task(&mut state_machine, task_info, &step_enqueuer).await
                } else {
                    Ok(false)
                }
            },

            TaskState::WaitingForRetry => {
                // Retry timeout expired
                if state_machine.transition(TaskEvent::RetryReady).await? {
                    Self::handle_enqueueing_task(&mut state_machine, task_info, &step_enqueuer).await
                } else {
                    Ok(false)
                }
            },

            _ => {
                warn!(
                    task_uuid = %task_info.task_uuid,
                    state = ?state_machine.current_state(),
                    "Task in unexpected state for batch processing"
                );
                Ok(false)
            }
        }
    }

    /// Handle task in Initializing state
    async fn handle_initializing_task(
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: &StepEnqueuer,
    ) -> TaskerResult<bool> {
        if task_info.ready_steps_count > 0 {
            // Has ready steps to enqueue
            if state_machine.transition(TaskEvent::ReadyStepsFound(task_info.ready_steps_count as u32)).await? {
                Self::handle_enqueueing_task(state_machine, task_info, step_enqueuer).await
            } else {
                Ok(false)
            }
        } else {
            // No steps - task is complete
            state_machine.transition(TaskEvent::NoStepsFound).await.map_err(Into::into)
        }
    }

    /// Handle task in EnqueuingSteps state
    async fn handle_enqueueing_task(
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: &StepEnqueuer,
    ) -> TaskerResult<bool> {
        // Enqueue the ready steps
        let enqueued_steps = step_enqueuer.enqueue_ready_steps_for_task(task_info.task_uuid).await?;

        // Transition to StepsInProcess
        state_machine.transition(TaskEvent::StepsEnqueued(enqueued_steps)).await.map_err(Into::into)
    }

    /// Handle task in EvaluatingResults state
    async fn handle_evaluating_task(
        state_machine: &mut TaskStateMachine,
        task_info: &ReadyTaskInfo,
        step_enqueuer: &StepEnqueuer,
    ) -> TaskerResult<bool> {
        // Get execution context to determine next action
        let sql_executor = SqlFunctionExecutor::new(state_machine.persistence.pool.clone());
        let context = sql_executor.get_task_execution_context(task_info.task_uuid).await?;

        match context.execution_status {
            ExecutionStatus::HasReadySteps => {
                if state_machine.transition(TaskEvent::ReadyStepsFound(context.ready_steps.unwrap_or(0) as u32)).await? {
                    Self::handle_enqueueing_task(state_machine, task_info, step_enqueuer).await
                } else {
                    Ok(false)
                }
            },
            ExecutionStatus::AllComplete => {
                state_machine.transition(TaskEvent::AllStepsSuccessful).await.map_err(Into::into)
            },
            ExecutionStatus::BlockedByFailures => {
                state_machine.transition(TaskEvent::PermanentFailure("Too many step failures".into())).await.map_err(Into::into)
            },
            ExecutionStatus::WaitingForDependencies => {
                state_machine.transition(TaskEvent::NoDependenciesReady).await.map_err(Into::into)
            },
            _ => Ok(false),
        }
    }
}
```

#### 3.5 Update TaskFinalizer (`task_finalizer.rs`)

The TaskFinalizer uses `ExecutionStatus` (derived from step counts) to make decisions. Update the handler methods to use state transitions instead of claims:

```rust
impl TaskFinalizer {
    /// Main entry point - uses execution context and state machine
    pub async fn finalize_task(
        &self,
        task_uuid: Uuid,
    ) -> Result<FinalizationResult, FinalizationError> {
        let task = Task::find_by_id(self.context.database_pool(), task_uuid).await?;
        let sql_executor = SqlFunctionExecutor::new(self.context.database_pool().clone());
        let context = sql_executor.get_task_execution_context(task_uuid).await?;

        self.make_finalization_decision(task, Some(context)).await
    }

    /// Decision logic based on ExecutionStatus using state machine
    async fn make_finalization_decision(
        &self,
        task: Task,
        context: Option<TaskExecutionContext>,
    ) -> Result<FinalizationResult, FinalizationError> {
        let Some(context) = context else {
            return self.handle_unclear_state(task, None).await;
        };

        // Create state machine for this task
        let mut state_machine = TaskStateMachine::for_task(
            task.task_uuid,
            self.context.database_pool().clone(),
            self.context.system_id(),
        ).await?;

        // ExecutionStatus is derived from step counts, not task state
        match context.execution_status {
            ExecutionStatus::AllComplete => {
                self.complete_task_with_state_machine(&mut state_machine, &task, &context).await
            }
            ExecutionStatus::BlockedByFailures => {
                self.error_task_with_state_machine(&mut state_machine, &task, &context).await
            }
            ExecutionStatus::HasReadySteps => {
                self.handle_ready_steps_with_state_machine(&mut state_machine, &task, &context).await
            }
            ExecutionStatus::WaitingForDependencies => {
                self.handle_waiting_with_state_machine(&mut state_machine, &task, &context).await
            }
            ExecutionStatus::Processing => {
                self.handle_processing_state(task, Some(context)).await
            }
        }
    }

    /// Complete task using state machine
    async fn complete_task_with_state_machine(
        &self,
        state_machine: &mut TaskStateMachine,
        task: &Task,
        context: &TaskExecutionContext,
    ) -> Result<FinalizationResult, FinalizationError> {
        let transitioned = match state_machine.current_state() {
            TaskState::EvaluatingResults => {
                state_machine.transition(TaskEvent::AllStepsSuccessful).await?
            },
            TaskState::Initializing if context.total_steps == 0 => {
                // Special case: task with no steps
                state_machine.transition(TaskEvent::NoStepsFound).await?
            },
            _ => false,
        };

        Ok(FinalizationResult {
            task_uuid: task.task_uuid,
            action: FinalizationAction::Completed,
            transitioned,
            completed_at: Utc::now(),
        })
```

#### 3.6 Architectural Improvements: State Machine vs Direct SQL

The restructured implementation provides significant architectural benefits over the original direct SQL approach:

**Before (Direct SQL Approach)**:
```rust
// Direct SQL calls bypass all validation and safety mechanisms
sql_executor.transition_task_state_atomic(
    task_uuid, TaskState::Pending, TaskState::Initializing,
    system_id, Some(json!({ "event": "Start" }))
).await?
```

**After (State Machine Approach)**:
```rust
// Proper state machine with validation, guards, and side effects
let mut state_machine = TaskStateMachine::for_task(task_uuid, pool, processor_id).await?;
state_machine.transition(TaskEvent::Start).await?
```

**Key Architectural Benefits**:

1. **Separation of Concerns**:
   - **Guards**: Validate transitions are legal
   - **Actions**: Handle pre/post transition side effects
   - **Persistence**: Manage atomic storage
   - **Events**: Type-safe event definitions

2. **Type Safety & Validation**:
   - Compile-time validation of state transitions
   - Impossible to create invalid state/event combinations
   - Guards prevent illegal transitions at runtime

3. **Extensibility**:
   - Easy to add new states, events, or validation rules
   - Pre/post transition hooks for complex workflows
   - Event publishing for system integration

4. **Testability**:
   - Each component (Guards, Actions, Persistence) testable in isolation
   - Mock-friendly design for unit testing
   - Clear boundaries between components

5. **Maintainability**:
   - Single source of truth for state transition logic
   - Self-documenting through state machine structure
   - Consistent error handling and logging

6. **Consistency**:
   - All components use the same transition mechanism
   - Unified error handling across the system
   - Consistent logging and metrics collection

**Eliminated Duplication**:
- Removed duplicate TaskState enum (lines 938-972)
- Consolidated persistence logic in StatePersistence
- Unified transition logic in TaskStateMachine
- Single approach throughout the codebase

This architectural change transforms raw SQL manipulation into a proper domain model with clear responsibilities and safety guarantees.

#### 3.7 Claim System Removal and Modification Plan

Based on codebase analysis, here's what needs to be removed or modified:

##### Files to Remove Entirely

**Core Claim Implementation**:
- `tasker-orchestration/src/orchestration/task_claim/task_claimer.rs` - Legacy task claiming
- `tasker-orchestration/src/orchestration/task_claim/finalization_claimer.rs` - Finalization claims
- `tasker-orchestration/src/orchestration/task_claim/unified_task_claimer.rs` - Unified claim attempt
- `tasker-orchestration/src/orchestration/task_claim/claim_states.rs` - Claim state machine
- `tasker-orchestration/src/orchestration/task_claim/mod.rs` - Module exports

**Configuration**:
- `tasker-shared/src/config/orchestration/task_claimer.rs` - TaskClaimerConfig
- Remove TaskClaimerConfig from `tasker-shared/src/config/orchestration/mod.rs`
- Remove task_claimer fields from main config structures

**Tests**:
- `tasker-worker/tests/finalization_race_condition_test.rs` - Tests FinalizationClaimer

##### Files to Modify

**OrchestrationCore** (`orchestration/core.rs`):
```rust
// REMOVE: UnifiedTaskClaimer initialization
- let unified_claimer = Arc::new(UnifiedTaskClaimer::new(context.clone(), core_id).await?);

// MODIFY: create_task_request_processor - remove unified_claimer parameter
async fn create_task_request_processor(
    context: &Arc<SystemContext>,
-   core_id: Uuid,
-   _unified_claimer: Arc<UnifiedTaskClaimer>,
) -> TaskerResult<Arc<TaskRequestProcessor>> {
    // Use context.system_id() instead of core_id
}

// MODIFY: create_result_processor - remove unified_claimer
async fn create_result_processor(
    context: &Arc<SystemContext>,
-   core_id: Uuid,
-   unified_claimer: Arc<UnifiedTaskClaimer>,
) -> TaskerResult<Arc<OrchestrationResultProcessor>> {
    // Remove unified_claimer usage
}
```

**TaskClaimStepEnqueuer** (`lifecycle/task_claim_step_enqueuer.rs`):
```rust
// REMOVE: TaskClaimer field and initialization
pub struct TaskClaimStepEnqueuer {
-   task_claimer: TaskClaimer,
    step_enqueuer: StepEnqueuer,
-   core_id: Uuid,  // Use context.system_id() instead
    context: Arc<SystemContext>,
    config: TaskClaimStepEnqueuerConfig,
}

// REPLACE: claim_ready_tasks with get_next_ready_tasks SQL function
// REPLACE: release_task_claim with state transitions
// REPLACE: claim_individual_task with atomic state transition
```

**OrchestrationProcessor/CommandProcessor** (`command_processor.rs`):
```rust
// REMOVE: unified_claimer field
pub struct OrchestrationProcessor {
-   unified_claimer: Arc<UnifiedTaskClaimer>,
    // ... other fields
}

// REPLACE: Finalization claiming with state transitions
- match self.unified_claimer.claim_task_for_purpose(task_uuid, ClaimPurpose::Finalization, None).await
+ match self.sql_executor.transition_task_state_atomic(
+     task_uuid,
+     TaskState::StepsInProcess,
+     TaskState::EvaluatingResults,
+     self.context.system_id(),
+     None
+ ).await
```

**TaskFinalizer** (`lifecycle/task_finalizer.rs`):
```rust
// REMOVE: unified_claimer field and constructors
pub struct TaskFinalizer {
    context: Arc<SystemContext>,
    sql_executor: SqlFunctionExecutor,
-   task_claim_step_enqueuer: Option<Arc<TaskClaimStepEnqueuer>>,
-   unified_claimer: Option<Arc<UnifiedTaskClaimer>>,
}

// REMOVE: with_unified_claimer constructor
// REMOVE: handle_ready_steps_with_unified_claims method
// SIMPLIFY: Use state transitions in all handler methods
```

**OrchestrationResultProcessor** (`lifecycle/result_processor.rs`):
```rust
// REMOVE: unified_claimer field
pub struct OrchestrationResultProcessor {
    task_finalizer: TaskFinalizer,
    backoff_calculator: BackoffCalculator,
-   unified_claimer: Option<Arc<UnifiedTaskClaimer>>,
    context: Arc<SystemContext>,
-   core_id: Uuid,  // Use context.system_id()
}

// REMOVE: with_unified_claimer constructor
```

##### Configuration Changes

This replaces the prior, incomplete guidance. Configuration must align with the unified task state machine and the removal of the claim subsystem.

1) Remove claim subsystem configuration
- Delete file: config/tasker/base/task_claimer.toml (entire file)
- Remove any [task_claimer] sections in env-specific overlays, if present
- Eliminate claim-related settings from other files (see below)

2) Add new state machine configuration
- Create: config/tasker/base/state_machine.toml
```toml
# TAS-41 State Machine Configuration
[state_machine]
enabled = true
enable_transition_logging = true
transition_log_level = "INFO"

[state_machine.timeouts]
initializing = 60
enqueuing_steps = 30
steps_in_process = 300
evaluating_results = 60

[state_machine.waiting_states]
max_waiting_for_dependencies_minutes = 60
retry_intervals = [30, 60, 120, 300, 600]
max_blocked_time_minutes = 60
blocked_resolution_strategy = "manual_only"

[state_machine.recovery]
enable_stuck_task_detection = true
stuck_task_detection_interval_seconds = 120
max_stuck_duration_minutes = 10
recovery_strategy = "reset_to_waiting"
max_recoveries_per_task = 3

[state_machine.metrics]
enable_state_transition_metrics = true
collect_transition_latency = true
collect_time_in_state = true
state_histogram_buckets = [0.01, 0.05, 0.1, 0.5, 1, 5, 10, 30, 60, 300, 600]

[state_machine.processor_ownership]
enable_ownership_tracking = true
default_timeout_seconds = 300
enable_heartbeat = true
heartbeat_interval_seconds = 60
max_consecutive_ownership_failures = 3

[state_machine.persistence]
track_transition_history = true
max_transitions_per_task = 100
store_processor_info = true
store_event_details = true
enable_history_cleanup = true
history_retention_days = 30
```

3) Update existing configuration files
- config/tasker/base/orchestration.toml
  - Remove claim-related settings:
    - default_claim_timeout_seconds
    - enable_heartbeat
    - heartbeat_interval_ms
  - Add reference to state machine config (if needed by loader):
    - use_unified_state_machine = true

- config/tasker/base/worker.toml
  - [worker.step_processing]
    - Remove: claim_timeout_seconds
    - Add: processor_ownership_timeout_seconds = 300
    - Keep: heartbeat_interval_seconds (used for ownership heartbeats)
  - [worker.fallback_poller]
    - Ensure event-driven defaults remain
    - Add: processable_states = ["pending", "waiting_for_dependencies", "waiting_for_retry"]

- config/tasker/base/backoff.toml
  - Replace reenqueue_delays keys with new state names:
```toml
[backoff.reenqueue_delays]
initializing = 5
enqueuing_steps = 0
steps_in_process = 10
evaluating_results = 5
waiting_for_dependencies = 45
waiting_for_retry = 30
blocked_by_failures = 60
```

- config/tasker/base/engine.toml
  - Replace [reenqueue] keys from legacy execution-status labels to state names:
```toml
[reenqueue]
# Old: has_ready_steps, waiting_for_dependencies, processing
# New states:
initializing = 2
enqueuing_steps = 0
steps_in_process = 2
evaluating_results = 1
waiting_for_dependencies = 5
```

- config/tasker/base/execution.toml
  - Add processor ownership section to reflect claim removal:
```toml
[execution.processor_ownership]
enable_ownership_tracking = true
default_ownership_timeout = 300
enable_heartbeat = true
heartbeat_interval_seconds = 60
```

- config/tasker/base/task_readiness.toml
  - Validate fallback polling is tuned for the new states (no action if already aligned)
  - Ensure event channel names include task_state_change where used

4) Telemetry/observability
- Ensure telemetry or health modules track per-state counts (initializing, enqueuing_steps, steps_in_process, evaluating_results, waiting_for_dependencies, waiting_for_retry, blocked_by_failures)
- Enable state transition metrics (as per state_machine.toml)

##### Import Cleanup

Remove these imports throughout the codebase:
```rust
- use crate::orchestration::task_claim::{
-     TaskClaimer, ClaimedTask, FinalizationClaimer,
-     UnifiedTaskClaimer, ClaimPurpose, ClaimGuard
- };
```

##### Method Signature Updates

Update all methods to use `context.system_id()` instead of passing `core_id`:
```rust
// Before
pub async fn new(context: Arc<SystemContext>, core_id: Uuid) -> Self

// After
pub async fn new(context: Arc<SystemContext>) -> Self {
    // Use context.system_id() internally
}
```

### Phase 4: Test Suite Updates

#### 4.1 Tests to Remove

- All tests in `task_claim/` directory
- Tests for claim timeouts and expiration
- Tests for claim transfer logic
- `finalization_race_condition_test.rs` - Replace with state transition race tests

#### 4.2 Tests to Update

```rust
// Update state machine tests
#[test]
fn test_task_state_transitions() {
    assert!(can_transition(TaskState::Pending, TaskState::Initializing));
    assert!(can_transition(TaskState::Initializing, TaskState::EnqueuingSteps));
    assert!(can_transition(TaskState::EnqueuingSteps, TaskState::StepsInProcess));
    // ... test all valid transitions
}

// Update integration tests to use new states
#[tokio::test]
async fn test_task_lifecycle() {
    let task = create_task().await;

    // Should start in Pending
    assert_eq!(task.current_state(), TaskState::Pending);

    // Process should transition through states
    processor.process_task(task.uuid).await?;

    assert_eq!(task.current_state(), TaskState::Complete);
}
```

#### 4.3 New Tests to Add

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_valid_state_transitions() {
        let task = create_test_task().await;

        // Test all valid transitions according to guards

        // Initial transition
        assert!(TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::Initializing,
            &TaskEvent::Start,
            &task
        ).is_ok());

        // From Initializing
        assert!(TransitionGuard::can_transition(
            TaskState::Initializing,
            TaskState::EnqueuingSteps,
            &TaskEvent::ReadyStepsFound(5),
            &task
        ).is_ok());

        assert!(TransitionGuard::can_transition(
            TaskState::Initializing,
            TaskState::Complete,
            &TaskEvent::NoStepsFound,
            &task
        ).is_ok());

        // From EnqueuingSteps
        assert!(TransitionGuard::can_transition(
            TaskState::EnqueuingSteps,
            TaskState::StepsInProcess,
            &TaskEvent::StepsEnqueued(vec![Uuid::new_v4(), Uuid::new_v4()]),
            &task
        ).is_ok());

        // From StepsInProcess
        assert!(TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::EvaluatingResults,
            &TaskEvent::StepCompleted(Uuid::new_v4()),
            &task
        ).is_ok());

        // From EvaluatingResults
        assert!(TransitionGuard::can_transition(
            TaskState::EvaluatingResults,
            TaskState::Complete,
            &TaskEvent::AllStepsSuccessful,
            &task
        ).is_ok());

        assert!(TransitionGuard::can_transition(
            TaskState::EvaluatingResults,
            TaskState::BlockedByFailures,
            &TaskEvent::PermanentFailure("Too many failures".into()),
            &task
        ).is_ok());

        // From waiting states
        assert!(TransitionGuard::can_transition(
            TaskState::WaitingForDependencies,
            TaskState::EvaluatingResults,
            &TaskEvent::DependenciesReady,
            &task
        ).is_ok());

        assert!(TransitionGuard::can_transition(
            TaskState::WaitingForRetry,
            TaskState::EnqueuingSteps,
            &TaskEvent::RetryReady,
            &task
        ).is_ok());

        // Cancellation from non-terminal states
        assert!(TransitionGuard::can_transition(
            TaskState::StepsInProcess,
            TaskState::Cancelled,
            &TaskEvent::Cancel,
            &task
        ).is_ok());
    }

    #[tokio::test]
    async fn test_invalid_state_transitions() {
        let task = create_test_task().await;

        // Test invalid transitions

        // Cannot transition from terminal states
        assert!(TransitionGuard::can_transition(
            TaskState::Complete,
            TaskState::EnqueuingSteps,
            &TaskEvent::ReadyStepsFound(1),
            &task
        ).is_err());

        // Invalid event for transition
        assert!(TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::EnqueuingSteps,
            &TaskEvent::ReadyStepsFound(1),
            &task
        ).is_err());

        // Must go through proper states
        assert!(TransitionGuard::can_transition(
            TaskState::Pending,
            TaskState::Complete,
            &TaskEvent::AllStepsSuccessful,
            &task
        ).is_err());
    }

    #[tokio::test]
    async fn test_sql_function_executor_queries() {
        let pool = create_test_pool().await;
        let sql_executor = SqlFunctionExecutor::new(pool.clone());

        // Test task discovery queries (read-only operations)
        let ready_tasks = sql_executor.get_next_ready_tasks(5).await?;
        assert!(ready_tasks.len() <= 5);

        let single_task = sql_executor.get_next_ready_task().await?;
        // Should return None or Some depending on available tasks

        if let Some(task) = single_task {
            // Test state query
            let current_state = sql_executor.get_current_task_state(task.task_uuid).await?;
            assert!(!current_state.is_terminal());
        }
    }

    #[tokio::test]
    async fn test_state_machine_mutations() {
        let pool = create_test_pool().await;
        let task = create_task().await;
        let processor_id = Uuid::new_v4();

        // Create state machine for mutations
        let mut state_machine = TaskStateMachine::for_task(
            task.uuid,
            pool.clone(),
            processor_id,
        ).await?;

        // Should start in Pending state
        assert_eq!(state_machine.current_state(), TaskState::Pending);

        // Transition through proper state machine
        assert!(state_machine.transition(TaskEvent::Start).await?);
        assert_eq!(state_machine.current_state(), TaskState::Initializing);

        // Second processor should not be able to claim
        let mut other_state_machine = TaskStateMachine::for_task(
            task.uuid,
            pool.clone(),
            Uuid::new_v4(),
        ).await?;

        // Should fail because first processor owns it
        assert!(!other_state_machine.transition(TaskEvent::Start).await?);
    }

    #[tokio::test]
    async fn test_race_condition_prevention_with_state_machine() {
        let pool = create_test_pool().await;
        let task = create_task_with_steps(5).await;

        // Setup task in StepsInProcess state
        let mut setup_machine = TaskStateMachine::for_task(
            task.uuid,
            pool.clone(),
            Uuid::new_v4(),
        ).await?;

        setup_machine.transition(TaskEvent::Start).await?;
        setup_machine.transition(TaskEvent::ReadyStepsFound(5)).await?;
        setup_machine.transition(TaskEvent::StepsEnqueued(vec![Uuid::new_v4(); 5])).await?;

        // Now simulate multiple processors trying to finalize
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let pool_clone = pool.clone();
                let task_id = task.uuid;
                tokio::spawn(async move {
                    let mut state_machine = TaskStateMachine::for_task(
                        task_id,
                        pool_clone,
                        Uuid::new_v4(),
                    ).await?;

                    state_machine.transition(TaskEvent::AllStepsCompleted).await
                })
            })
            .collect();

        let results = futures::future::join_all(handles).await;

        // Exactly one should succeed due to state machine atomic operations
        let success_count = results.iter()
            .filter(|r| r.as_ref().unwrap().as_ref().unwrap_or(&false) == &true)
            .count();

        assert_eq!(success_count, 1, "Exactly one processor should win the race");
    }

    #[tokio::test]
    async fn test_complete_task_lifecycle_with_proper_architecture() {
        let pool = create_test_pool().await;
        let task = create_task_with_steps(3).await;
        let processor_id = Uuid::new_v4();

        // Use proper architecture: SqlExecutor for queries, StateMachine for mutations
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let mut state_machine = TaskStateMachine::for_task(
            task.uuid,
            pool.clone(),
            processor_id,
        ).await?;

        // Start: Pending -> Initializing (mutation via state machine)
        assert!(state_machine.transition(TaskEvent::Start).await?);
        assert_eq!(state_machine.current_state(), TaskState::Initializing);

        // Verify state via query
        let current_state = sql_executor.get_current_task_state(task.uuid).await?;
        assert_eq!(current_state, TaskState::Initializing);

        // Continue lifecycle through state machine
        assert!(state_machine.transition(TaskEvent::ReadyStepsFound(3)).await?);
        assert_eq!(state_machine.current_state(), TaskState::EnqueuingSteps);

        assert!(state_machine.transition(TaskEvent::StepsEnqueued(vec![
            Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()
        ])).await?);
        assert_eq!(state_machine.current_state(), TaskState::StepsInProcess);

        assert!(state_machine.transition(TaskEvent::AllStepsCompleted).await?);
        assert_eq!(state_machine.current_state(), TaskState::EvaluatingResults);

        assert!(state_machine.transition(TaskEvent::AllStepsSuccessful).await?);
        assert_eq!(state_machine.current_state(), TaskState::Complete);

        // Final verification via query
        let final_state = sql_executor.get_current_task_state(task.uuid).await?;
        assert_eq!(final_state, TaskState::Complete);
        assert!(final_state.is_terminal());
    }

    #[tokio::test]
    async fn test_fallback_poller_compatibility() {
        let pool = create_test_pool().await;
        let context = Arc::new(SystemContext::with_pool(pool).await?);

        // Create TaskClaimStepEnqueuer
        let enqueuer = TaskClaimStepEnqueuer::new(context.clone()).await?;

        // Process batch should return compatible result structure
        let result = enqueuer.process_batch().await?;

        // Verify compatibility fields exist for FallbackPoller
        assert_eq!(result.tasks_claimed, result.processed_count);
        assert_eq!(result.tasks_processed, result.processed_count);

        // FallbackPoller can use the same interface as before
        if result.tasks_claimed > 0 {
            // This is the exact pattern used in FallbackPoller
            info!(
                tasks_claimed = result.tasks_claimed,
                tasks_processed = result.tasks_processed,
                steps_enqueued = result.total_steps_enqueued,
                "Compatible with FallbackPoller logging"
            );
        }
    }
}
```

## Summary of Key Architectural Improvements

This specification provides a comprehensive transformation from fragmented state management to a unified, idiomatic state machine architecture:

### 1. **Eliminated Critical Duplication**
- **Removed duplicate TaskState enum** (lines 938-972): Kept comprehensive version with full documentation and trait implementations
- **Consolidated persistence logic**: StatePersistence handles all database operations, TransitionActions manages side effects
- **Unified transition mechanism**: Single TaskStateMachine coordinator replaces direct SQL calls throughout codebase
- **Eliminated functional overlap**: No more competing approaches (SqlFunctionExecutor vs state machine components)

### 2. **Proper State Machine Architecture**
- **Guards validate transitions**: Compile-time and runtime validation prevents illegal state changes
- **Events drive transitions**: Type-safe events (Start, ReadyStepsFound, AllStepsSuccessful, etc.) ensure consistent behavior
- **Actions handle side effects**: Pre/post transition hooks for logging, ownership changes, completion timestamps
- **Persistence manages storage**: Atomic database operations with ownership verification

### 3. **Architectural Benefits Over Direct SQL**
- **Domain modeling**: TaskStateMachine provides proper business logic layer instead of raw database manipulation
- **Type safety**: Impossible to create invalid state/event combinations at compile time
- **Separation of concerns**: Clear boundaries between validation (Guards), side effects (Actions), and storage (Persistence)
- **Extensibility**: Easy to add new states, events, or validation rules without touching SQL
- **Testability**: Each component mockable and testable in isolation

### 4. **Implementation Consistency**
- **Proper state progression**: All code follows defined transitions in Guards with appropriate events
- **Multi-entry handling**: TaskClaimStepEnqueuer handles Pending, WaitingForDependencies, and WaitingForRetry states
- **Unified error handling**: Consistent StateMachineError types throughout the system
- **Complete lifecycle coverage**: Full test suite demonstrates proper state transitions from creation to completion

### 5. **Maintained Safety Guarantees**
- **Atomic operations**: Database-level atomicity through transition_task_state_atomic SQL function
- **Race condition prevention**: FOR UPDATE SKIP LOCKED with processor ownership tracking
- **Distributed coordination**: Multiple processors can safely attempt transitions without conflicts
- **Audit trail**: Complete transition history with processor and metadata tracking

### 6. **Clear Separation of Concerns**
- **Query operations**: SqlFunctionExecutor handles all read-only database queries (get_next_ready_tasks, get_current_task_state, get_task_execution_context)
- **Mutation operations**: TaskStateMachine handles all state changes with proper validation and side effects
- **Task discovery**: SQL queries find ready tasks, state machine processes them atomically
- **Fallback compatibility**: Existing FallbackPoller continues to work with updated result structure

This architectural improvement transforms what was becoming an unwieldy collection of overlapping approaches into a clean, maintainable, and extensible state machine that properly models the domain while maintaining all safety guarantees and existing integrations.

#### 3.8 Fallback Poller Compatibility

The existing FallbackPoller in `tasker-orchestration/src/orchestration/task_readiness/fallback_poller.rs` continues to work with minimal changes:

**Current Implementation**:
```rust
// fallback_poller.rs calls TaskClaimStepEnqueuer::process_batch()
match enqueuer.process_batch().await {
    Ok(result) => {
        info!(
            tasks_claimed = result.tasks_claimed,
            tasks_processed = result.tasks_processed,
            steps_enqueued = result.total_steps_enqueued,
            "Fallback poller found and processed ready tasks"
        );
    }
}
```

**Required Updates**:
The FallbackPoller expects specific result fields that may need alignment:

```rust
// Update TaskClaimStepEnqueueCycleResult to maintain compatibility
#[derive(Debug, Clone)]
pub struct TaskClaimStepEnqueueCycleResult {
    pub processed_count: u32,
    pub failed_count: u32,

    // Compatibility fields for FallbackPoller
    pub tasks_claimed: u32,     // Same as processed_count
    pub tasks_processed: u32,   // Same as processed_count
    pub total_steps_enqueued: u32, // Can be derived or tracked separately
}

impl TaskClaimStepEnqueueCycleResult {
    pub fn new(processed: u32, failed: u32) -> Self {
        Self {
            processed_count: processed,
            failed_count: failed,
            tasks_claimed: processed,        // Alias for compatibility
            tasks_processed: processed,      // Alias for compatibility
            total_steps_enqueued: 0,        // Could be tracked if needed
        }
    }
}
```

**Architectural Benefits**:
- **No changes needed**: FallbackPoller continues calling `process_batch()` exactly as before
- **Same polling logic**: 30-second intervals, skip missed ticks, error handling unchanged
- **Enhanced underneath**: Now benefits from proper state machine validation and atomic transitions
- **Better observability**: Each polled task now has complete transition audit trail
- **Improved safety**: Race conditions eliminated even in fallback polling scenarios

**Migration Path**:
1. Update `TaskClaimStepEnqueueCycleResult` structure for compatibility
2. No changes needed to `FallbackPoller` implementation
3. Fallback poller automatically benefits from improved state machine architecture
4. Enhanced logging provides better visibility into fallback polling effectiveness

#### 3.9 Integration Summary

The updated architecture maintains perfect backward compatibility while providing significant improvements:

| Component | Changes Required | Benefits Gained |
|-----------|------------------|-----------------|
| **SqlFunctionExecutor** | Add query methods back | Clear separation of queries vs mutations |
| **TaskStateMachine** | New coordinator | Proper state machine patterns with validation |
| **TaskClaimStepEnqueuer** | Use both SqlExecutor + StateMachine | Better organization, atomic safety |
| **TaskFinalizer** | Use both SqlExecutor + StateMachine | Consistent approach, proper validation |
| **FallbackPoller** | Minor result structure updates | Zero logic changes, enhanced safety |

**Key Architectural Principles**:
- **Queries**: SqlFunctionExecutor for all read operations (discovery, context, state checking)
- **Mutations**: TaskStateMachine for all state changes (atomic, validated, with side effects)
- **Separation**: Clear boundary between reading data and changing state
- **Compatibility**: Existing components continue working with enhanced safety underneath

## Key Benefits

### 1. Radical Simplification & Architectural Clarity
- **Eliminate ~3000 lines** of claim management code
- **One unified state machine** instead of three overlapping systems (task states, execution status, claims)
- **Proper domain modeling** with TaskStateMachine coordinator instead of direct SQL manipulation
- **Idiomatic Rust patterns** using Guards, Events, and Actions instead of ad-hoc SQL calls

### 2. Better Performance
- **30% fewer database queries** (no claim checks)
- **No cleanup jobs** for expired claims
- **Simpler indexes** without claim tables

### 3. Improved Observability
- **Task state directly shows operations** (e.g., "enqueuing_steps" vs "in_progress")
- **No multi-table joins** to understand task status
- **Complete audit trail** in transitions table

### 4. Natural Safety
- **Atomic state transitions** prevent race conditions
- **Processor ownership** tracked in transition metadata
- **FOR UPDATE SKIP LOCKED** provides distributed safety
- **Compile-time state validation** through enum usage

## Implementation Timeline

### Week 1: Core Changes
- [ ] Update migration file with new states and functions
- [ ] Update TaskState enum in Rust
- [ ] Add transition_task_state_atomic function

### Week 2: Orchestration Updates
- [ ] Replace TaskClaimer with state transitions
- [ ] Update TaskFinalizer to use states
- [ ] Remove claim-related code

### Week 3: Testing and Cleanup
- [ ] Update test suite for new states
- [ ] Remove claim-related tests
- [ ] Add new state transition tests
- [ ] Performance testing

## Success Metrics

- All existing workflows continue functioning
- No increase in race conditions or task processing errors
- Measurable reduction in database query volume
- Simplified debugging through clearer state names

## Summary

This change eliminates an entire subsystem (claims) by fixing the root cause - an oversimplified state machine. The new states directly represent what the system is doing, making it easier to understand, debug,maintain while improving performance and maintaining all safety guarantees.

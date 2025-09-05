# TAS-41: Enqueued For Orchestration State Implementation

## Problem Statement

### Architectural Issue: Event Flow Timing Race Condition

The current orchestration system has a critical timing race condition in hybrid/polling deployment modes that prevents proper orchestration metadata processing, particularly backoff handling and other post-processing logic.

#### Current Problematic Flow

After analyzing the codebase, here's the actual race condition flow with supporting code evidence:

1. **Worker Completes Step**: `tasker-worker/src/worker/command_processor.rs:636-691` executes step
2. **Worker Transitions Directly**: Worker persists results AND transitions step state directly to `Complete`/`Error`

```rust path=/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-worker/src/worker/command_processor.rs start=662
// Current problematic code - transitions directly to terminal states
let step_event = if step_result.success {
    StepEvent::Complete(Some(serde_json::to_value(&step_result)?))
} else {
    let error_message = step_result
        .error
        .as_ref()
        .map(|e| e.message.clone())
        .unwrap_or_else(|| "Unknown error".to_string());
    StepEvent::Fail(error_message)
};
```

3. **Task Appears Ready**: `tasker_ready_tasks` view immediately shows task as having ready steps because terminal states (`Complete`/`Error`) satisfy dependencies

```sql path=/Users/petetaylor/projects/tasker-systems/tasker-core/migrations/20250810140000_uuid_v7_initial_schema.sql start=1230
-- From get_step_readiness_status_batch function - shows how Complete/Error states are treated
COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
```

4. **Readiness Poller Wins Race**: `task_readiness/fallback_poller.rs` finds ready task and enqueues next step
5. **Orchestration Processing Bypassed**: `orchestration_queues/fallback_poller.rs` never processes step results from queue
6. **Metadata Lost**: `step_result_processor.rs` orchestration metadata processing (backoff calculations, retry logic) never executes
7. **Task Finalization Logic Skipped**: `task_finalizer.rs` coordination logic is bypassed

#### Core Issue

Workers are short-circuiting the orchestration evaluation by transitioning steps directly to terminal states (`Complete`/`Error`), which immediately makes tasks appear "ready" for next step processing, bypassing the orchestration system's ability to:

- Process step result metadata for backoff decisions
- Apply retry coordination logic
- Execute task finalization coordination
- Handle cross-cutting orchestration concerns

This is the same issue we solved previously in the opposite direction where orchestration was enqueueing steps that went straight to `InProgress` without the `Enqueued` state.

## Solution: EnqueuedForOrchestration State

### New State Flow Design

1. **Worker Executes Step**: Worker processes step and persists results
2. **Worker Transitions to Intermediate State**: Worker transitions step to `EnqueuedForOrchestration` (not terminal)
3. **Worker Sends Message**: Worker sends `StepResultMessage` to `orchestration_step_results` queue
4. **Orchestration Processing**: `StepResultProcessor` processes orchestration metadata, backoff logic, etc.
5. **Orchestration Finalizes**: Orchestration system transitions from `EnqueuedForOrchestration` to `Complete`/`Error`
6. **Task Becomes Ready**: Only after orchestration processing does task appear in `tasker_ready_tasks` view
7. **Next Step Enqueuing**: `TaskReadinessFallbackPoller` enqueues next step after all orchestration logic has run

### State Machine Changes Required

#### 1. Add New WorkflowStepState

**File**: `tasker-shared/src/state_machine/states.rs:74-89`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepState {
    /// Initial state when step is created
    Pending,
    /// Step has been enqueued for processing but not yet claimed by a worker
    Enqueued,
    /// Step is currently being executed by a worker
    InProgress,
    /// Step completed by worker, enqueued for orchestration processing
    EnqueuedForOrchestration,  // NEW STATE
    /// Step completed successfully (after orchestration processing)
    Complete,
    /// Step failed with an error (after orchestration processing)
    Error,
    /// Step was cancelled
    Cancelled,
    /// Step was manually resolved by operator
    ResolvedManually,
}
```
**State Properties**:
- `EnqueuedForOrchestration` is NOT a terminal state (critical for dependency logic)
- Steps in this state should NOT satisfy dependencies for other steps
- Steps in this state should NOT contribute to ready step counts in `tasker_ready_tasks` view
- Only orchestration system can transition FROM this state to `Complete`/`Error`
- Workers can only transition TO this state (never FROM it)
- Must update `satisfies_dependencies()` method in `WorkflowStepState`

```rust path=/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-shared/src/state_machine/states.rs start=120
// Current logic that will need updating
/// Check if this step satisfies dependencies for other steps
pub fn satisfies_dependencies(&self) -> bool {
    matches!(self, Self::Complete | Self::ResolvedManually)
    // EnqueuedForOrchestration should NOT be included here
}
```
- Workers can only transition TO this state

#### 2. Add New StepEvent

**File**: `tasker-shared/src/state_machine/events.rs:52-67`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StepEvent {
    /// Enqueue the step for processing (pending → enqueued)
    Enqueue,
    /// Start processing the step (enqueued → in_progress)
    Start,
    /// Enqueue step results for orchestration processing (in_progress → enqueued_for_orchestration)
    EnqueueForOrchestration(Option<Value>),  // NEW EVENT
    /// Mark step as complete with optional results (enqueued_for_orchestration → complete)
    Complete(Option<Value>),
    /// Mark step as failed with error message (enqueued_for_orchestration → error)
    Fail(String),
    /// Cancel the step
    Cancel,
    /// Manually resolve the step
    ResolveManually,
    /// Retry the step (from error state)
    Retry,
}
```

#### 3. Update State Transition Guards

**File**: `tasker-shared/src/state_machine/guards.rs`

New transition guards needed:
- `InProgress → EnqueuedForOrchestration`: Allowed for workers after step execution
- `EnqueuedForOrchestration → Complete`: Allowed for orchestration after processing
- `EnqueuedForOrchestration → Error`: Allowed for orchestration after processing
- `EnqueuedForOrchestration → Retry`: Allowed for orchestration for retry scenarios

#### 4. Update State Machine Actions

**File**: `tasker-shared/src/state_machine/actions.rs`

New actions needed:
- `EnqueueForOrchestrationAction`: Persist step results and update state
- Update existing `CompleteStepAction` and `FailStepAction` to work from `EnqueuedForOrchestration`

### Code Changes Required

#### 1. Worker Command Processor Update

**File**: `tasker-worker/src/worker/command_processor.rs:662-671`

```rust
// 3. Transition using EnqueueForOrchestration instead of Complete/Fail
let step_event = if step_result.success {
    StepEvent::EnqueueForOrchestration(Some(serde_json::to_value(&step_result)?))
} else {
    // For failures, we still use EnqueueForOrchestration to allow orchestration to process
    // The orchestration system will determine if it should Complete or Fail based on metadata
    StepEvent::EnqueueForOrchestration(Some(serde_json::to_value(&step_result)?))
};
```

#### 2. Step Result Processor Enhancement

**File**: `tasker-orchestration/src/orchestration/lifecycle/step_result_processor.rs`

The processor already has the right architecture but needs to handle the new state:

```rust path=/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-orchestration/src/orchestration/lifecycle/step_result_processor.rs start=221
// Current processor architecture - needs to handle new state
self.orchestration_result_processor
    .handle_step_result_with_metadata(step_result)
    .await
    .map_err(|e| {
        TaskerError::OrchestrationError(format!("Failed to handle step result: {e}"))
    })?;
```

The processor should:
1. Receive `StepResultMessage` from `orchestration_step_results` queue  
2. Load step in `EnqueuedForOrchestration` state using existing database operations
3. Process orchestration metadata (backoff, retry logic, etc.) using existing `OrchestrationResultProcessor`
4. Transition to final state (`Complete` or `Error`) based on processing results
5. Apply backoff calculations and retry coordination logic that's currently being bypassed

#### 3. SQL Function Updates

**File**: `migrations/20250810140000_uuid_v7_initial_schema.sql`

Multiple functions need updates to handle the new state:

##### tasker_ready_tasks View (lines 980-1053)
The view uses `get_task_execution_context()` function which needs updates:

```sql path=/Users/petetaylor/projects/tasker-systems/tasker-core/migrations/20250810140000_uuid_v7_initial_schema.sql start=1030
-- Current view relies on get_task_execution_context for readiness logic
JOIN LATERAL (
    SELECT * FROM get_task_execution_context(t.task_uuid)
) tec ON true
WHERE
    -- Only include tasks with ready steps (from existing SQL function logic)
    tec.ready_steps > 0
    -- Only include tasks that have ready steps to execute (not already processing)
    AND tec.execution_status = 'has_ready_steps'
```

##### get_task_execution_context Function (lines 903-949)
Critical updates needed to exclude `EnqueuedForOrchestration` from completed counts:

```sql path=/Users/petetaylor/projects/tasker-systems/tasker-core/migrations/20250810140000_uuid_v7_initial_schema.sql start=908
-- Current logic that needs updating
COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
-- Should NOT include 'enqueued_for_orchestration' in completed count
-- EnqueuedForOrchestration steps should be counted separately as "processing"
```

##### is_step_ready_for_execution Function (lines 1225-1266)
- Ensure dependencies check terminal states correctly
- `EnqueuedForOrchestration` should be treated as non-terminal

##### Other Functions Requiring Updates:
- `get_task_ready_steps` (lines 1498-1503)
- `get_task_context_with_ready_steps` (lines 1952-2001) 
- `calculate_task_priority_with_escalation` (lines 2067-2107)

### Database Schema Impact

#### State Enum Update

The PostgreSQL enum for step states needs updating:

```sql
ALTER TYPE workflow_step_state ADD VALUE 'enqueued_for_orchestration';
```

#### Index Considerations

New indexes may be beneficial:
- Index on `(state, updated_at)` for efficient orchestration queue processing
- Partial indexes excluding `enqueued_for_orchestration` for ready task queries

### Migration Strategy

#### Phase 1: State Machine Foundation
1. Add `EnqueuedForOrchestration` state to enum
2. Add `EnqueueForOrchestration` event
3. Implement transition guards and actions
4. Update SQL functions and views

#### Phase 2: Worker Integration
1. Update worker command processor to use new event
2. Ensure backward compatibility during transition
3. Test with single worker before rolling out

#### Phase 3: Orchestration Integration
1. Update `StepResultProcessor` to handle new state
2. Implement orchestration metadata processing logic
3. Add proper error handling and logging

#### Phase 4: SQL Optimization
1. Update database schema with new enum value
2. Add appropriate indexes
3. Update any remaining SQL functions

### Testing Strategy

#### Unit Tests
- State machine transition tests for new state
- Worker command processor tests with new event
- Step result processor tests for orchestration logic

#### Integration Tests
- End-to-end workflow tests ensuring proper orchestration processing
- Race condition tests to verify timing issues are resolved
- Fallback poller coordination tests

#### Performance Tests
- Ensure new state doesn't negatively impact ready task queries
- Verify orchestration processing doesn't become a bottleneck

### Success Criteria

1. **Race Condition Eliminated**: Workers no longer bypass orchestration processing
2. **Orchestration Metadata Applied**: Backoff calculations and retry logic execute correctly
3. **No Performance Regression**: Task processing performance maintained or improved
4. **Backward Compatibility**: Existing workflows continue to function during migration
5. **Proper Event Flow**: All components process events in the correct sequence

### Risk Assessment and Code Analysis Findings

#### **CRITICAL FINDING**: Current State Machine Logic

Analysis of the codebase reveals that the current `WorkflowStepState` enum already has proper helper methods that will need updates:

```rust path=/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-shared/src/state_machine/states.rs start=120
/// Check if this step satisfies dependencies for other steps  
pub fn satisfies_dependencies(&self) -> bool {
    matches!(self, Self::Complete | Self::ResolvedManually)
    // CRITICAL: EnqueuedForOrchestration must NOT be included here
}

/// Check if this is a terminal state (no further transitions allowed)
pub fn is_terminal(&self) -> bool {
    matches!(
        self,
        Self::Complete | Self::Cancelled | Self::ResolvedManually
    )
    // CRITICAL: EnqueuedForOrchestration must NOT be included here
}
```

#### Low Risk
- State machine changes follow existing patterns in `states.rs` and `events.rs`
- Worker command processor changes are isolated to transition logic
- Orchestration processor architecture already supports the required flow

#### Medium Risk  
- SQL function updates in `get_task_execution_context()` and related functions
- Timing coordination between worker transitions and orchestration processing
- Database migration requires careful enum value addition

#### **HIGH RISK**: SQL Dependency Logic

The SQL functions that determine task readiness are complex and interconnected:

```sql path=/Users/petetaylor/projects/tasker-systems/tasker-core/migrations/20250810140000_uuid_v7_initial_schema.sql start=1097
-- Dependency satisfaction logic that will be affected
COUNT(CASE WHEN parent_state.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
-- EnqueuedForOrchestration must NOT be included in completed_deps count
```

#### Mitigation Strategies
- **Comprehensive SQL Testing**: All functions that check step states need thorough testing
- **Gradual Rollout**: Deploy state machine changes before worker behavior changes
- **Monitoring**: Track steps in `EnqueuedForOrchestration` state for timing analysis
- **Rollback Plan**: Ability to revert worker behavior to direct transitions if issues arise

### Implementation Files Summary

#### Core State Machine Changes
- `tasker-shared/src/state_machine/states.rs` - Add new state enum value
- `tasker-shared/src/state_machine/events.rs` - Add new event type
- `tasker-shared/src/state_machine/guards.rs` - Add transition guards
- `tasker-shared/src/state_machine/actions.rs` - Add transition actions

#### Worker Changes
- `tasker-worker/src/worker/command_processor.rs` - Update step completion logic

#### Orchestration Changes
- `tasker-orchestration/src/orchestration/lifecycle/step_result_processor.rs` - Enhanced processing
- `tasker-orchestration/src/orchestration/orchestration_queues/fallback_poller.rs` - Process new states

#### Database Changes
- `migrations/20250810140000_uuid_v7_initial_schema.sql` - Update multiple SQL functions
- New migration for enum value addition

## Additional Implementation Considerations

### Performance Impact Analysis

**Positive Impacts:**
- Eliminates race conditions that can cause missed orchestration processing
- Ensures proper backoff calculation and retry logic execution
- Maintains event-driven architecture benefits

**Potential Concerns:**
- Additional state transition adds one more step to the processing pipeline
- Steps will remain in intermediate state longer (until orchestration processes them)
- Slight increase in database state transition records

### Integration with Existing Fallback Pollers

The existing fallback poller architecture already handles orchestration queue processing:

```rust path=/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-orchestration/src/orchestration/orchestration_queues/fallback_poller.rs start=330
// Current fallback poller handles step results  
tasker_shared::config::ConfigDrivenMessageEvent::StepResults(_event) => {
    stats.step_results_processed.fetch_add(1, Ordering::Relaxed);
    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
    command_sender
        .send(OrchestrationCommand::ProcessStepResultFromMessage {
            queue_name: queue_name.to_string(),
            message: message.clone(),
            resp: resp_tx,
        })
        .await
}
```

This means the orchestration processing pipeline will correctly handle steps in the new `EnqueuedForOrchestration` state.

### Database Schema Validation

The PostgreSQL enum modification is straightforward:

```sql path=null start=null
-- Add the new state value to existing enum
ALTER TYPE workflow_step_state ADD VALUE 'enqueued_for_orchestration';

-- Verify all functions handle the new state correctly
-- Particularly important: dependency satisfaction queries
```

## Conclusion

This implementation will resolve the architectural timing race condition and ensure proper orchestration metadata processing while maintaining system reliability and performance. The solution leverages existing architecture patterns and requires minimal changes to core processing logic, with the main complexity being in SQL function updates to handle the new intermediate state correctly.

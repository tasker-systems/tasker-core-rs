# TAS-41: Immediate Step Enqueuing After Task Creation

## Status
**Specification Phase** - Ready for Implementation

## Summary
Enhance the task creation process to immediately enqueue the first batch of ready steps, reducing latency between task creation and step execution. This ensures that tasks become actionable immediately upon creation while maintaining race-condition safety through the existing claim mechanism.

## Problem Statement

### Current Behavior
1. Task creation via `create_task_from_request` only initializes the task and workflow steps
2. Steps are not enqueued until the orchestration loop discovers the task through SQL polling
3. This introduces unnecessary latency between task creation and step execution
4. The system relies on periodic polling to discover newly created tasks

### Desired Behavior
1. Task creation should immediately enqueue ready steps
2. Maintain race-condition safety using the existing claim mechanism
3. Reduce latency to near-zero for task execution startup
4. Preserve all existing safety guarantees and transactional integrity

## Solution Design

### Core Approach
Create a new method `create_and_enqueue_task_from_request` that:
1. Creates the task using existing `create_task_from_request`
2. Claims the individual task using a new SQL function
3. Processes the claimed task to enqueue ready steps
4. Releases the claim immediately after enqueuing

### Key Components

#### 1. New SQL Function: `claim_individual_task`
```sql
CREATE OR REPLACE FUNCTION claim_individual_task(
    p_task_uuid uuid,
    p_orchestrator_id character varying,
    p_claim_timeout_seconds integer DEFAULT 300)
RETURNS TABLE(
    task_uuid uuid,
    namespace_name character varying,
    priority integer,
    computed_priority float8,
    age_hours float8,
    ready_steps_count bigint,
    claim_timeout_seconds integer)
LANGUAGE plpgsql
AS $$
BEGIN
    -- Atomically claim a specific task if it's ready
    RETURN QUERY
    UPDATE tasker_tasks t
    SET claimed_at = NOW(),
        claimed_by = p_orchestrator_id,
        claim_timeout_seconds = COALESCE(p_claim_timeout_seconds, t.claim_timeout_seconds),
        updated_at = NOW()
    FROM (
        SELECT
            t.task_uuid,
            -- Compute priority using same logic as claim_ready_tasks
            (CASE
                WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)
                WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)
                WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)
                WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)
                ELSE                             0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)
            END)::float8 as computed_priority_calc,
            ROUND(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600.0, 2)::float8 as age_hours_calc
        FROM tasker_tasks t
        JOIN LATERAL (SELECT * FROM get_task_execution_context(t.task_uuid)) tec ON true
        WHERE t.task_uuid = p_task_uuid
            AND t.complete = false
            AND tec.ready_steps > 0
            AND (t.claimed_at IS NULL OR t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval))
    ) eligible_task
    WHERE t.task_uuid = eligible_task.task_uuid
    RETURNING
        t.task_uuid,
        (SELECT name FROM tasker_task_namespaces WHERE task_namespace_uuid = 
            (SELECT task_namespace_uuid FROM tasker_named_tasks WHERE named_task_uuid = t.named_task_uuid)
        ) as namespace_name,
        t.priority,
        eligible_task.computed_priority_calc::float8 as computed_priority,
        eligible_task.age_hours_calc::float8 as age_hours,
        (SELECT ready_steps FROM get_task_execution_context(t.task_uuid)) as ready_steps_count,
        t.claim_timeout_seconds;
END;
$$;
```

#### 2. TaskClaimer Enhancement
Add new method to `task_claimer.rs`:
```rust
/// Claim a specific task by UUID
pub async fn claim_individual_task(
    &self,
    task_uuid: Uuid,
) -> TaskerResult<Option<ClaimedTask>> {
    debug!(
        task_uuid = task_uuid.to_string(),
        orchestrator_id = %self.orchestrator_id,
        "Attempting to claim individual task"
    );

    let query = r#"
        SELECT task_uuid, namespace_name, priority, computed_priority, age_hours,
               ready_steps_count, claim_timeout_seconds
        FROM claim_individual_task($1::UUID, $2::VARCHAR, $3::INTEGER)
    "#;

    let rows = sqlx::query_as::<_, ClaimedTaskRow>(query)
        .bind(task_uuid)
        .bind(&self.orchestrator_id)
        .bind(self.config.default_claim_timeout)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to claim individual task {}: {}", task_uuid, e);
            TaskerError::DatabaseError(format!("Individual task claiming failed: {e}"))
        })?;

    // Should return at most one row
    let claimed_task = rows.into_iter().next().map(|row| ClaimedTask {
        task_uuid: row.task_uuid,
        namespace_name: row.namespace_name,
        priority: row.priority,
        computed_priority: row.computed_priority,
        age_hours: row.age_hours,
        ready_steps_count: row.ready_steps_count,
        claim_timeout_seconds: row.claim_timeout_seconds,
        claimed_at: Utc::now(),
    });

    if let Some(ref task) = claimed_task {
        info!(
            task_uuid = task.task_uuid.to_string(),
            namespace = %task.namespace_name,
            ready_steps = task.ready_steps_count,
            "Successfully claimed individual task"
        );
    } else {
        debug!(
            task_uuid = task_uuid.to_string(),
            "Task not eligible for claiming (may be complete, have no ready steps, or already claimed)"
        );
    }

    Ok(claimed_task)
}
```

#### 3. TaskClaimStepEnqueuer Enhancement
Add new method to `task_claim_step_enqueuer.rs`:
```rust
/// Process a single task: claim it, enqueue steps, and release
pub async fn process_single_task(
    &self,
    task_uuid: Uuid,
) -> TaskerResult<Option<StepEnqueueResult>> {
    debug!(
        task_uuid = task_uuid.to_string(),
        "Processing single task for immediate step enqueuing"
    );

    // Claim the individual task
    let claimed_task = match self.task_claimer.claim_individual_task(task_uuid).await? {
        Some(task) => task,
        None => {
            debug!(
                task_uuid = task_uuid.to_string(),
                "Task not ready for processing (no ready steps or already claimed)"
            );
            return Ok(None);
        }
    };

    // Process the claimed task to enqueue steps
    let enqueue_result = match self.process_claimed_task(&claimed_task).await {
        Ok(result) => result,
        Err(e) => {
            error!(
                task_uuid = task_uuid.to_string(),
                error = %e,
                "Failed to process claimed task"
            );
            
            // Always try to release the claim on error
            let _ = self.task_claimer.release_task_claim(task_uuid).await;
            return Err(e);
        }
    };

    // Release the claim immediately after processing
    match self.task_claimer.release_task_claim(task_uuid).await {
        Ok(released) => {
            if !released {
                warn!(
                    task_uuid = task_uuid.to_string(),
                    "Task claim was not released (may have been released already)"
                );
            }
        }
        Err(e) => {
            warn!(
                task_uuid = task_uuid.to_string(),
                error = %e,
                "Failed to release task claim after processing"
            );
        }
    }

    info!(
        task_uuid = task_uuid.to_string(),
        steps_enqueued = enqueue_result.steps_enqueued,
        steps_failed = enqueue_result.steps_failed,
        "Successfully processed single task with immediate step enqueuing"
    );

    Ok(Some(enqueue_result))
}
```

#### 4. TaskInitializer Enhancement
Modify the TaskInitializer struct to include an optional TaskClaimStepEnqueuer:

```rust
// Update struct definition
pub struct TaskInitializer {
    context: Arc<SystemContext>,
    state_manager: Option<StateManager>,
    task_claim_step_enqueuer: Option<Arc<TaskClaimStepEnqueuer>>,
}

// Update constructor methods
impl TaskInitializer {
    /// Create a new TaskInitializer without step enqueuer (backward compatible)
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self {
            context,
            state_manager: None,
            task_claim_step_enqueuer: None,
        }
    }

    /// Create a TaskInitializer with TaskClaimStepEnqueuer for immediate step enqueuing
    pub fn with_step_enqueuer(
        context: Arc<SystemContext>,
        task_claim_step_enqueuer: Arc<TaskClaimStepEnqueuer>,
    ) -> Self {
        Self {
            context,
            state_manager: None,
            task_claim_step_enqueuer: Some(task_claim_step_enqueuer),
        }
    }

    /// Create a TaskInitializer with both StateManager and TaskClaimStepEnqueuer
    pub fn with_state_manager_and_step_enqueuer(
        context: Arc<SystemContext>,
        task_claim_step_enqueuer: Arc<TaskClaimStepEnqueuer>,
    ) -> Self {
        let pool = context.database_pool().clone();
        let sql_executor = SqlFunctionExecutor::new(pool.clone());
        let state_manager =
            StateManager::new(sql_executor, context.event_publisher.clone(), pool.clone());

        Self {
            context,
            state_manager: Some(state_manager),
            task_claim_step_enqueuer: Some(task_claim_step_enqueuer),
        }
    }

    /// Create a task and immediately enqueue its ready steps
    pub async fn create_and_enqueue_task_from_request(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, TaskInitializationError> {
        // First, create the task using existing method
        let initialization_result = self.create_task_from_request(task_request).await?;
        
        info!(
            task_uuid = initialization_result.task_uuid.to_string(),
            step_count = initialization_result.step_count,
            "Task created, attempting immediate step enqueuing"
        );

        // Only attempt immediate enqueuing if we have a task_claim_step_enqueuer
        if let Some(ref task_claim_step_enqueuer) = self.task_claim_step_enqueuer {
            // Process the single task to enqueue its steps
            match task_claim_step_enqueuer
                .process_single_task(initialization_result.task_uuid)
                .await
            {
                Ok(Some(enqueue_result)) => {
                    info!(
                        task_uuid = initialization_result.task_uuid.to_string(),
                        steps_enqueued = enqueue_result.steps_enqueued,
                        "Successfully enqueued steps immediately after task creation"
                    );
                }
                Ok(None) => {
                    // This is unusual but not an error - task may have no ready steps initially
                    debug!(
                        task_uuid = initialization_result.task_uuid.to_string(),
                        "No steps were ready for immediate enqueuing"
                    );
                }
                Err(e) => {
                    // Log the error but don't fail the task creation
                    // The orchestration loop will pick up the task eventually
                    warn!(
                        task_uuid = initialization_result.task_uuid.to_string(),
                        error = %e,
                        "Failed to immediately enqueue steps, task will be processed in next orchestration cycle"
                    );
                }
            }
        } else {
            debug!(
                task_uuid = initialization_result.task_uuid.to_string(),
                "TaskClaimStepEnqueuer not configured, skipping immediate step enqueuing"
            );
        }
        
        Ok(initialization_result)
    }
}
```

#### 5. Web API and Orchestration Core Updates
Update TaskInitializer creation in `web/state.rs`:
```rust
// In AppState::new(), update TaskInitializer creation:
let task_claim_step_enqueuer = Arc::new(TaskClaimStepEnqueuer::new(
    task_claimer.clone(),
    step_enqueuer.clone(),
    TaskClaimStepEnqueueConfig::default(),
));

let task_initializer = Arc::new(TaskInitializer::with_step_enqueuer(
    orchestration_core.context.clone(),
    task_claim_step_enqueuer.clone(),
));

// Update create_task handler to use:
state.task_initializer.create_and_enqueue_task_from_request(task_request).await
```

Update TaskInitializer creation in `orchestration/core.rs`:
```rust
// In create_task_request_processor(), update TaskInitializer creation:
let task_claim_step_enqueuer = Arc::new(TaskClaimStepEnqueuer::new(
    task_claimer.clone(),
    step_enqueuer.clone(),
    TaskClaimStepEnqueueConfig::default(),
));

let task_initializer = Arc::new(TaskInitializer::with_step_enqueuer(
    context.clone(),
    task_claim_step_enqueuer,
));
```

#### 6. Task Request Processor Update
Update both `handle_valid_task_request` and `process_task_request` in `task_request_processor.rs`:
```rust
// Replace create_task_from_request calls with:
self.task_initializer.create_and_enqueue_task_from_request(request.task_request.clone()).await
```

## Implementation Plan

### Phase 1: Database Migration
1. Create migration file with `claim_individual_task` SQL function
2. Test the function manually to ensure it works correctly
3. Verify it handles edge cases (already claimed, no ready steps, etc.)

### Phase 2: Core Components
1. Add `claim_individual_task` method to TaskClaimer
2. Add `process_single_task` method to TaskClaimStepEnqueuer
3. Add unit tests for both new methods

### Phase 3: TaskInitializer Integration
1. Modify TaskInitializer struct to include `Option<Arc<TaskClaimStepEnqueuer>>`
2. Add new constructor methods `with_step_enqueuer` and `with_state_manager_and_step_enqueuer`
3. Implement `create_and_enqueue_task_from_request` method
4. Update TaskInitializer creation in web/state.rs and orchestration/core.rs

### Phase 4: API Updates
1. Update web API handler to use new method
2. Update task request processor to use new method
3. Add integration tests

### Phase 5: Testing & Validation
1. Test race condition safety with concurrent task creation
2. Verify immediate step enqueuing reduces latency
3. Ensure fallback to orchestration loop works if immediate enqueuing fails
4. Load test with high task creation rates

## Benefits

### Performance
- **Reduced Latency**: Near-zero delay between task creation and step execution
- **Better Resource Utilization**: Steps start processing immediately
- **Improved Throughput**: No waiting for next orchestration cycle

### Reliability
- **Race-Condition Safe**: Uses existing claim mechanism
- **Graceful Fallback**: If immediate enqueuing fails, orchestration loop picks up task
- **Transactional Safety**: Task creation remains atomic

### User Experience
- **Immediate Feedback**: Tasks become active immediately
- **Predictable Behavior**: No variable polling delays
- **Better SLAs**: Consistent task startup times

## Risks & Mitigations

### Risk 1: Increased Database Load
**Mitigation**: The claim is released immediately after enqueuing, minimizing lock duration

### Risk 2: Dependency Injection Complexity
**Mitigation**: Clean struct-based dependency injection with Option<Arc<T>> pattern for optional dependencies

### Risk 3: Error Handling Complexity
**Mitigation**: Ensure immediate enqueuing failures don't fail task creation

## Success Metrics

1. **Latency Reduction**: Measure time from task creation to first step execution
2. **Success Rate**: Track percentage of tasks with successful immediate enqueuing
3. **Database Impact**: Monitor claim/release operation frequency
4. **Error Rate**: Track failures in immediate enqueuing

## Testing Strategy

### Unit Tests
- Test `claim_individual_task` with various task states
- Test `process_single_task` with claimed and unclaimed tasks
- Test `create_and_enqueue_task_from_request` success and failure paths

### Integration Tests
- Test end-to-end task creation with immediate enqueuing
- Test concurrent task creation for race conditions
- Test fallback when immediate enqueuing fails

### Performance Tests
- Measure latency improvement with immediate enqueuing
- Load test with high task creation rates
- Verify no negative impact on orchestration loop

## Future Enhancements

1. **Batch Immediate Enqueuing**: Support creating multiple tasks with immediate enqueuing
2. **Priority-Based Immediate Enqueuing**: Skip immediate enqueuing for low-priority tasks
3. **Configurable Behavior**: Allow disabling immediate enqueuing via configuration
4. **Metrics Collection**: Add detailed metrics for immediate enqueuing success/failure

## Dependencies

- Existing task claim mechanism (TAS-40)
- Task initialization system (TAS-31)
- Step enqueuing infrastructure
- SQLx transaction support

## References

- Task Claimer: `tasker-orchestration/src/orchestration/task_claim/task_claimer.rs`
- Task Claim Step Enqueuer: `tasker-orchestration/src/orchestration/lifecycle/task_claim_step_enqueuer.rs`
- Task Initializer: `tasker-orchestration/src/orchestration/lifecycle/task_initializer.rs`
- Web API: `tasker-orchestration/src/web/handlers/tasks.rs`
- Task Request Processor: `tasker-orchestration/src/orchestration/lifecycle/task_request_processor.rs`
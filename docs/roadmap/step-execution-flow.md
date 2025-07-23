# Step Execution Flow and Result Persistence

## Overview

This document traces the complete flow of step execution and result persistence in the Tasker orchestration system, from the initial step execution through to database persistence.

## Step Result Persistence Flow

The system correctly persists step results through the following flow:

### 1. Step Execution (`src/orchestration/step_executor.rs`)

```rust
// Line 803: Execute step through framework integration
let step_result = framework
    .execute_step_with_handler(&execution_context, handler_class, &step_config)
    .await?;
```

The `StepResult` contains:
- `status`: StepStatus (Completed, Failed, Skipped, etc.)
- `output`: serde_json::Value (the actual step results)
- `error_message`: Option<String>
- `retry_after`: Option<u32>
- `metadata`: HashMap<String, Value>

### 2. Step Finalization (`src/orchestration/step_executor.rs`)

```rust
// Line 563: For completed steps, preserve results
if let Err(e) = self.state_manager.complete_step_with_results(
    step_id, 
    Some(step_result.output.clone())
).await {
    warn!("Failed to complete step with results");
}
```

### 3. State Manager (`src/orchestration/state_manager.rs`)

```rust
// Line 619-636: Complete step with results
pub async fn complete_step_with_results(
    &self,
    step_id: i64,
    step_results: Option<serde_json::Value>,
) -> OrchestrationResult<()> {
    let mut step_state_machine = self.get_or_create_step_state_machine(step_id).await?;
    
    // Create completion event with results
    let event = StepEvent::Complete(step_results);
    
    step_state_machine.transition(event).await?;
    Ok(())
}
```

### 4. State Machine Actions (`src/state_machine/actions.rs`)

```rust
// Line 171-173: Extract results from event
if to_state == WorkflowStepState::Complete.to_string() {
    let results = extract_results_from_event(event)?;
    
    // Line 179: Persist results to database
    step_clone.mark_processed(pool, results.clone()).await?;
}

// Line 371-384: Result extraction from StepEvent::Complete
fn extract_results_from_event(event: &str) -> ActionResult<Option<Value>> {
    if let Ok(event_data) = serde_json::from_str::<Value>(event) {
        // StepEvent::Complete serializes as: {"type": "Complete", "data": { results }}
        if event_type == "Complete" {
            if let Some(results) = event_data.get("data") {
                return Ok(Some(results.clone()));
            }
        }
    }
    Ok(None)
}
```

### 5. Database Persistence (`src/models/core/workflow_step.rs`)

```rust
// Line 316-338: Mark step as processed with results
pub async fn mark_processed(
    &mut self,
    pool: &PgPool,
    results: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE tasker_workflow_steps 
        SET processed = true, 
            in_process = false,
            processed_at = $2,
            results = $3,        // ✅ Results persisted here
            updated_at = NOW()
        WHERE workflow_step_id = $1
        "#,
        self.workflow_step_id,
        now,
        results
    )
    .execute(pool)
    .await?;
}
```

## Complete Flow Summary

1. **StepExecutor** executes step via framework integration → receives `StepResult`
2. **StepExecutor** calls `StateManager.complete_step_with_results()` with the output
3. **StateManager** creates `StepEvent::Complete(results)` and transitions state machine
4. **StepStateMachine** processes the event through actions
5. **Actions** extract results from event and call `WorkflowStep.mark_processed()`
6. **WorkflowStep** updates database with results in the `results` column

## Key Points

- Step results ARE properly persisted to the database when steps complete successfully
- The `results` column in `tasker_workflow_steps` table stores the step output as JSON
- Failed steps use a different flow through `fail_step_with_error()` and don't persist results
- The persistence happens as part of the state transition to "complete"

## Common Issues

If step results aren't appearing in the database:

1. **Step Failure**: Steps that fail don't persist results (they use the error flow instead)
2. **Empty Results**: Step handlers might be returning empty/null results
3. **Serialization Issues**: Results might fail JSON serialization
4. **State Transition Failure**: If the state machine transition fails, results won't be persisted

## Debugging Step Result Issues

To debug missing step results:

1. Check if the step actually completed successfully (vs failed)
2. Verify the step handler is returning non-empty results
3. Check the logs for "Step marked as complete with results" messages
4. Query the database directly: `SELECT workflow_step_id, results FROM tasker_workflow_steps WHERE task_id = ?`
5. Verify the state machine transition succeeded

## Next Investigation

The error logs show that subsequent steps (reserve_inventory, process_payment, ship_order) are looking for results from the validate_order step but can't find them. This suggests either:

1. The validate_order step is not completing successfully
2. The results are not being loaded correctly in the step sequence
3. The dependency resolution is not working properly

The specific error "validate_order step has no results" indicates the Ruby side is trying to access results from a previous step but the data isn't available in the step sequence passed to the handler.
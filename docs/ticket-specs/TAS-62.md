# TAS-62: Workflow Step Result Audit Log

## Overview

Add a lightweight audit trail for workflow step execution results. The audit table stores **references and attribution only**—full result data is retrieved via JOIN to the existing `tasker_workflow_step_transitions` table which already captures the complete `StepExecutionResult` in its metadata.

## Design Principles

1. **No data duplication**: Results already exist in transition metadata
2. **Attribution capture**: NEW data (worker_uuid, correlation_id) not currently stored
3. **Indexed scalars**: Extract `success` and `execution_time_ms` for efficient filtering
4. **SQL trigger**: Guaranteed audit record creation (SOC2 compliance)
5. **Lightweight records**: ~100 bytes per audit entry vs ~1KB+ for duplicated results

## Implementation Plan

### Phase 1: Database Migration

**File**: `migrations/YYYYMMDDHHMMSS_add_step_result_audit.sql`

```sql
-- 1. Create audit table
CREATE TABLE tasker_workflow_step_result_audit (
    workflow_step_result_audit_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    workflow_step_uuid UUID NOT NULL REFERENCES tasker_workflow_steps(workflow_step_uuid),
    workflow_step_transition_uuid UUID NOT NULL REFERENCES tasker_workflow_step_transitions(workflow_step_transition_uuid),
    task_uuid UUID NOT NULL REFERENCES tasker_tasks(task_uuid),
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

-- 2. Indexes
CREATE INDEX idx_audit_step_uuid ON tasker_workflow_step_result_audit(workflow_step_uuid);
CREATE INDEX idx_audit_task_uuid ON tasker_workflow_step_result_audit(task_uuid);
CREATE INDEX idx_audit_recorded_at ON tasker_workflow_step_result_audit(recorded_at);
CREATE INDEX idx_audit_worker_uuid ON tasker_workflow_step_result_audit(worker_uuid) WHERE worker_uuid IS NOT NULL;

-- 3. Trigger function
CREATE OR REPLACE FUNCTION create_step_result_audit()
RETURNS TRIGGER AS $$
DECLARE
    v_task_uuid UUID;
    v_worker_uuid UUID;
    v_correlation_id UUID;
    v_success BOOLEAN;
    v_execution_time_ms BIGINT;
    v_event_json JSONB;
BEGIN
    -- Only audit worker result transitions
    IF NEW.to_state NOT IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration') THEN
        RETURN NEW;
    END IF;

    -- Get task_uuid from workflow step
    SELECT task_uuid INTO v_task_uuid
    FROM tasker_workflow_steps
    WHERE workflow_step_uuid = NEW.workflow_step_uuid;

    -- Extract attribution from metadata (added by application code)
    v_worker_uuid := (NEW.metadata->>'worker_uuid')::UUID;
    v_correlation_id := (NEW.metadata->>'correlation_id')::UUID;

    -- Parse the event JSON to extract success and execution_time_ms
    v_event_json := (NEW.metadata->>'event')::JSONB;
    v_success := (v_event_json->>'success')::BOOLEAN;
    v_execution_time_ms := (v_event_json->'metadata'->>'execution_time_ms')::BIGINT;

    -- Fallback: determine success from state if not in event
    IF v_success IS NULL THEN
        v_success := (NEW.to_state = 'enqueued_for_orchestration');
    END IF;

    -- Insert audit record
    INSERT INTO tasker_workflow_step_result_audit (
        workflow_step_uuid,
        workflow_step_transition_uuid,
        task_uuid,
        worker_uuid,
        correlation_id,
        success,
        execution_time_ms
    ) VALUES (
        NEW.workflow_step_uuid,
        NEW.workflow_step_transition_uuid,
        v_task_uuid,
        v_worker_uuid,
        v_correlation_id,
        COALESCE(v_success, NEW.to_state = 'enqueued_for_orchestration'),
        v_execution_time_ms
    );

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 4. Create trigger
CREATE TRIGGER trg_step_result_audit
    AFTER INSERT ON tasker_workflow_step_transitions
    FOR EACH ROW
    EXECUTE FUNCTION create_step_result_audit();

-- 5. Backfill existing transitions (NULL attribution for historical)
INSERT INTO tasker_workflow_step_result_audit (
    workflow_step_uuid,
    workflow_step_transition_uuid,
    task_uuid,
    worker_uuid,
    correlation_id,
    success,
    execution_time_ms
)
SELECT
    t.workflow_step_uuid,
    t.workflow_step_transition_uuid,
    ws.task_uuid,
    NULL,  -- No historical attribution
    NULL,
    t.to_state = 'enqueued_for_orchestration',
    ((t.metadata->>'event')::JSONB->'metadata'->>'execution_time_ms')::BIGINT
FROM tasker_workflow_step_transitions t
JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = t.workflow_step_uuid
WHERE t.to_state IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration')
ON CONFLICT (workflow_step_uuid, workflow_step_transition_uuid) DO NOTHING;
```

### Phase 2: Attribution Enrichment in Application Code

**2.1 New TransitionContext type**

**File**: `tasker-shared/src/state_machine/context.rs` (new)

```rust
/// Context data passed to state machine transitions for audit enrichment
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransitionContext {
    pub worker_uuid: Option<Uuid>,
    pub correlation_id: Option<Uuid>,
}
```

**2.2 Modify StepStateMachine**

**File**: `tasker-shared/src/state_machine/step_state_machine.rs`

- Add `transition_with_context()` method that merges context into transition metadata
- Keep existing `transition()` as-is for backward compatibility (calls new method with None context)

**2.3 Modify Worker Command Processor**

**File**: `tasker-worker/src/worker/command_processor.rs` (lines ~1140-1195)

In `handle_send_step_result()`:
1. Capture `self.worker_id` (already available)
2. Get `correlation_id` from task (already fetched)
3. Create `TransitionContext` with both values
4. Call `transition_with_context()` instead of `transition()`

### Phase 3: Audit Model

**File**: `tasker-shared/src/models/core/workflow_step_result_audit.rs` (new)

```rust
pub struct WorkflowStepResultAudit {
    pub workflow_step_result_audit_uuid: Uuid,
    pub workflow_step_uuid: Uuid,
    pub workflow_step_transition_uuid: Uuid,
    pub task_uuid: Uuid,
    pub recorded_at: NaiveDateTime,
    pub worker_uuid: Option<Uuid>,
    pub correlation_id: Option<Uuid>,
    pub success: bool,
    pub execution_time_ms: Option<i64>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl WorkflowStepResultAudit {
    /// Get audit history for a step with full results via JOIN
    pub async fn get_audit_history(
        pool: &PgPool,
        step_uuid: Uuid,
    ) -> Result<Vec<StepAuditWithResults>, sqlx::Error>;

    /// Get audit history for all steps in a task
    pub async fn get_task_audit_history(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepAuditWithResults>, sqlx::Error>;
}
```

### Phase 4: API Types

**File**: `tasker-shared/src/types/api/orchestration.rs`

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct StepAuditResponse {
    pub audit_uuid: String,
    pub workflow_step_uuid: String,
    pub transition_uuid: String,
    pub task_uuid: String,
    pub recorded_at: String,

    // Attribution
    pub worker_uuid: Option<String>,
    pub correlation_id: Option<String>,

    // Result summary
    pub success: bool,
    pub execution_time_ms: Option<i64>,

    // Full result (from transition metadata via JOIN)
    pub result: Option<serde_json::Value>,

    // Context
    pub step_name: String,
    pub from_state: Option<String>,
    pub to_state: String,
}
```

### Phase 5: API Handler

**File**: `tasker-orchestration/src/web/handlers/steps.rs`

```rust
/// GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}/audit
pub async fn get_step_audit(
    State(state): State<AppState>,
    Path((task_uuid, step_uuid)): Path<(String, String)>,
) -> ApiResult<Json<Vec<StepAuditResponse>>> {
    // 1. Validate UUIDs
    // 2. Verify step belongs to task
    // 3. Query audit history via model (includes JOIN for full results)
    // 4. Transform to API response
}
```

**File**: `tasker-orchestration/src/web/routes.rs`

```rust
.route(
    "/tasks/:uuid/workflow_steps/:step_uuid/audit",
    get(handlers::steps::get_step_audit),
)
```

### Phase 6: Client Method

**File**: `tasker-client/src/api_clients/orchestration_client.rs`

```rust
pub async fn get_step_audit_history(
    &self,
    task_uuid: Uuid,
    step_uuid: Uuid,
) -> TaskerResult<Vec<StepAuditResponse>> {
    let url = self.base_url.join(&format!(
        "/v1/tasks/{}/workflow_steps/{}/audit",
        task_uuid, step_uuid
    ))?;

    let response = self.client.get(url).send().await?;
    self.handle_response(response, "get step audit history").await
}
```

### Phase 7: Documentation

**File**: `docs/states-and-lifecycles.md`

Add section documenting:
- Audit table purpose and trigger behavior
- What triggers audit record creation (worker result persistence)
- Attribution data captured
- How to query audit history via API

## Files to Modify

| File | Change |
|------|--------|
| `migrations/YYYYMMDDHHMMSS_add_step_result_audit.sql` | New migration |
| `tasker-shared/src/state_machine/mod.rs` | Export new context module |
| `tasker-shared/src/state_machine/context.rs` | New TransitionContext type |
| `tasker-shared/src/state_machine/step_state_machine.rs` | Add transition_with_context() |
| `tasker-shared/src/state_machine/persistence.rs` | Accept metadata with attribution |
| `tasker-worker/src/worker/command_processor.rs` | Pass TransitionContext |
| `tasker-shared/src/models/core/mod.rs` | Export new audit model |
| `tasker-shared/src/models/core/workflow_step_result_audit.rs` | New model |
| `tasker-shared/src/types/api/orchestration.rs` | StepAuditResponse type |
| `tasker-orchestration/src/web/handlers/steps.rs` | get_step_audit handler |
| `tasker-orchestration/src/web/routes.rs` | Add audit route |
| `tasker-client/src/api_clients/orchestration_client.rs` | get_step_audit_history() |
| `docs/states-and-lifecycles.md` | Document audit system |

## Testing Strategy

1. **Migration tests**: Verify trigger fires correctly, backfill works
2. **Integration tests**: End-to-end worker execution creates audit records
3. **API tests**: Endpoint returns correct data with JOIN to transitions
4. **Attribution tests**: Verify worker_uuid and correlation_id captured

## Open Questions

None - design is complete.

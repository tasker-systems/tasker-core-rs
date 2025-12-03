-- TAS-62: Workflow Step Result Audit Log
--
-- Creates a lightweight audit trail for workflow step execution results.
-- The audit table stores references and attribution only - full result data
-- is retrieved via JOIN to tasker_workflow_step_transitions which already
-- captures the complete StepExecutionResult in its metadata.
--
-- Design Principles:
-- 1. No data duplication: Results already exist in transition metadata
-- 2. Attribution capture: NEW data (worker_uuid, correlation_id) not currently stored
-- 3. Indexed scalars: Extract success and execution_time_ms for efficient filtering
-- 4. SQL trigger: Guaranteed audit record creation (SOC2 compliance)
-- 5. Lightweight records: ~100 bytes per audit entry vs ~1KB+ for duplicated results

-- ============================================================================
-- 1. Create audit table
-- ============================================================================

CREATE TABLE IF NOT EXISTS tasker_workflow_step_result_audit (
    -- Primary key using UUIDv7 for time-ordered, distributed-friendly IDs
    workflow_step_result_audit_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),

    -- Foreign keys with full referential integrity
    workflow_step_uuid UUID NOT NULL REFERENCES tasker_workflow_steps(workflow_step_uuid),
    workflow_step_transition_uuid UUID NOT NULL REFERENCES tasker_workflow_step_transitions(workflow_step_transition_uuid),
    task_uuid UUID NOT NULL REFERENCES tasker_tasks(task_uuid),

    -- Timestamp when audit record was created
    recorded_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),

    -- Attribution context (NEW data not in transitions)
    -- These values are extracted from transition metadata where application code enriches them
    worker_uuid UUID,
    correlation_id UUID,

    -- Extracted scalars for indexing/filtering without JSON parsing
    success BOOLEAN NOT NULL,
    execution_time_ms BIGINT,

    -- Standard audit timestamps
    created_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),

    -- Unique constraint: one audit record per (step, transition) pair
    -- This ensures we don't create duplicate audit records for the same transition
    CONSTRAINT uq_audit_step_transition UNIQUE (workflow_step_uuid, workflow_step_transition_uuid)
);

-- Add table comment for documentation
COMMENT ON TABLE tasker_workflow_step_result_audit IS
'Lightweight audit trail for workflow step execution results. Stores references and attribution only - full results accessed via JOIN to transitions table. Created by TAS-62.';

COMMENT ON COLUMN tasker_workflow_step_result_audit.worker_uuid IS
'UUID of the worker that processed this step (attribution for SOC2 compliance)';

COMMENT ON COLUMN tasker_workflow_step_result_audit.correlation_id IS
'Correlation ID for distributed tracing across the orchestration system';

COMMENT ON COLUMN tasker_workflow_step_result_audit.success IS
'Whether the step execution succeeded (extracted from transition for indexing)';

COMMENT ON COLUMN tasker_workflow_step_result_audit.execution_time_ms IS
'Step execution time in milliseconds (extracted from transition for filtering)';

-- ============================================================================
-- 2. Create indexes for common query patterns
-- ============================================================================

-- Primary query pattern: get audit history for a specific step
CREATE INDEX IF NOT EXISTS idx_audit_step_uuid
    ON tasker_workflow_step_result_audit(workflow_step_uuid);

-- Query all audit records for a task
CREATE INDEX IF NOT EXISTS idx_audit_task_uuid
    ON tasker_workflow_step_result_audit(task_uuid);

-- Time-range queries for SOC2 audit reports
CREATE INDEX IF NOT EXISTS idx_audit_recorded_at
    ON tasker_workflow_step_result_audit(recorded_at);

-- Attribution investigation queries (partial index for efficiency)
CREATE INDEX IF NOT EXISTS idx_audit_worker_uuid
    ON tasker_workflow_step_result_audit(worker_uuid)
    WHERE worker_uuid IS NOT NULL;

-- Distributed tracing queries (partial index for efficiency)
CREATE INDEX IF NOT EXISTS idx_audit_correlation_id
    ON tasker_workflow_step_result_audit(correlation_id)
    WHERE correlation_id IS NOT NULL;

-- Success/failure filtering
CREATE INDEX IF NOT EXISTS idx_audit_success
    ON tasker_workflow_step_result_audit(success);

-- ============================================================================
-- 3. Create trigger function
-- ============================================================================

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
    -- These are the two states where workers persist execution results
    IF NEW.to_state NOT IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration') THEN
        RETURN NEW;
    END IF;

    -- Get task_uuid from workflow step (denormalized for query efficiency)
    SELECT ws.task_uuid INTO v_task_uuid
    FROM tasker_workflow_steps ws
    WHERE ws.workflow_step_uuid = NEW.workflow_step_uuid;

    -- Extract attribution from transition metadata
    -- These are added by application code in TransitionContext
    BEGIN
        v_worker_uuid := (NEW.metadata->>'worker_uuid')::UUID;
    EXCEPTION WHEN OTHERS THEN
        v_worker_uuid := NULL;
    END;

    BEGIN
        v_correlation_id := (NEW.metadata->>'correlation_id')::UUID;
    EXCEPTION WHEN OTHERS THEN
        v_correlation_id := NULL;
    END;

    -- Parse the event JSON to extract success and execution_time_ms
    -- The event field contains the serialized StepEvent which wraps StepExecutionResult
    BEGIN
        v_event_json := (NEW.metadata->>'event')::JSONB;
        v_success := (v_event_json->>'success')::BOOLEAN;
        v_execution_time_ms := (v_event_json->'metadata'->>'execution_time_ms')::BIGINT;
    EXCEPTION WHEN OTHERS THEN
        v_event_json := NULL;
        v_success := NULL;
        v_execution_time_ms := NULL;
    END;

    -- Fallback: determine success from state if not extractable from event
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
        v_success,
        v_execution_time_ms
    );

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION create_step_result_audit() IS
'Trigger function that creates audit records when workers persist step results. Fires on transitions to enqueued_for_orchestration or enqueued_as_error_for_orchestration states.';

-- ============================================================================
-- 4. Create trigger
-- ============================================================================

-- Drop trigger if it exists (for idempotent migrations)
DROP TRIGGER IF EXISTS trg_step_result_audit ON tasker_workflow_step_transitions;

-- Create the trigger
CREATE TRIGGER trg_step_result_audit
    AFTER INSERT ON tasker_workflow_step_transitions
    FOR EACH ROW
    EXECUTE FUNCTION create_step_result_audit();

COMMENT ON TRIGGER trg_step_result_audit ON tasker_workflow_step_transitions IS
'Creates audit records when workers persist step execution results (TAS-62)';

-- ============================================================================
-- 5. Backfill existing transitions
-- ============================================================================

-- Insert audit records for historical transitions
-- These will have NULL attribution since that data wasn't captured before TAS-62
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
    -- No historical attribution available
    (t.metadata->>'worker_uuid')::UUID,
    (t.metadata->>'correlation_id')::UUID,
    -- Determine success from state
    CASE
        WHEN t.to_state = 'enqueued_for_orchestration' THEN TRUE
        ELSE FALSE
    END,
    -- Try to extract execution_time_ms from event JSON
    (
        CASE
            WHEN t.metadata->>'event' IS NOT NULL THEN
                ((t.metadata->>'event')::JSONB->'metadata'->>'execution_time_ms')::BIGINT
            ELSE NULL
        END
    )
FROM tasker_workflow_step_transitions t
JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = t.workflow_step_uuid
WHERE t.to_state IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration')
ON CONFLICT (workflow_step_uuid, workflow_step_transition_uuid) DO NOTHING;

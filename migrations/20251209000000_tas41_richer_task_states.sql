-- TAS-41: Richer Task States Migration
-- This migration implements the comprehensive task state machine to replace the claim subsystem

-- ============================================================================
-- Phase 1.1: Add processor tracking to transitions table
-- ============================================================================

ALTER TABLE public.tasker_task_transitions
ADD COLUMN IF NOT EXISTS processor_uuid UUID,
ADD COLUMN IF NOT EXISTS transition_metadata JSONB DEFAULT '{}';

-- Index for processor ownership queries
CREATE INDEX IF NOT EXISTS idx_task_transitions_processor
ON public.tasker_task_transitions(processor_uuid)
WHERE processor_uuid IS NOT NULL;

-- Index for timeout monitoring
CREATE INDEX IF NOT EXISTS idx_task_transitions_timeout
ON public.tasker_task_transitions((transition_metadata->>'timeout_at'))
WHERE most_recent = true;

-- ============================================================================
-- Phase 1.2: Update state validation constraints for 12 states
-- ============================================================================

-- Drop existing constraints first
ALTER TABLE public.tasker_task_transitions
DROP CONSTRAINT IF EXISTS chk_task_transitions_to_state;

ALTER TABLE public.tasker_task_transitions
DROP CONSTRAINT IF EXISTS chk_task_transitions_from_state;

-- Add new constraints with 12 states
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

-- ============================================================================
-- Phase 1.3: Update get_system_health_counts function for granular states
-- ============================================================================

DROP FUNCTION IF EXISTS get_system_health_counts();

CREATE OR REPLACE FUNCTION get_system_health_counts()
RETURNS TABLE(
    pending_tasks BIGINT,
    initializing_tasks BIGINT,
    enqueuing_steps_tasks BIGINT,
    steps_in_process_tasks BIGINT,
    evaluating_results_tasks BIGINT,
    waiting_for_dependencies_tasks BIGINT,
    waiting_for_retry_tasks BIGINT,
    blocked_by_failures_tasks BIGINT,
    complete_tasks BIGINT,
    error_tasks BIGINT,
    cancelled_tasks BIGINT,
    resolved_manually_tasks BIGINT,
    total_tasks BIGINT,
    pending_steps BIGINT,
    enqueued_steps BIGINT,
    running_steps BIGINT,
    complete_steps BIGINT,
    cancelled_steps BIGINT,
    failed_steps BIGINT,
    resolved_manually_steps BIGINT,
    total_steps BIGINT
) AS $$
BEGIN
    RETURN QUERY
    WITH task_counts AS (
        SELECT
            COUNT(*) FILTER (WHERE task_state.to_state = 'pending') as pending_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'initializing') as initializing_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'enqueuing_steps') as enqueuing_steps_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'steps_in_process') as steps_in_process_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'evaluating_results') as evaluating_results_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'waiting_for_dependencies') as waiting_for_dependencies_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'waiting_for_retry') as waiting_for_retry_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'blocked_by_failures') as blocked_by_failures_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'complete') as complete_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'error') as error_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'cancelled') as cancelled_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'resolved_manually') as resolved_manually_tasks,
            COUNT(*) as total_tasks
        FROM tasker_tasks t
        LEFT JOIN tasker_task_transitions task_state ON task_state.task_uuid = t.task_uuid AND task_state.most_recent = true
    ),
    step_counts AS (
        SELECT
            COUNT(*) FILTER (WHERE step_state.to_state = 'pending') as pending_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'enqueued') as enqueued_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'running') as running_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'complete') as complete_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'cancelled') as cancelled_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'failed') as failed_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'resolved_manually') as resolved_manually_steps,
            COUNT(*) as total_steps
        FROM tasker_workflow_steps s
        LEFT JOIN tasker_workflow_step_transitions step_state ON step_state.workflow_step_uuid = s.workflow_step_uuid AND step_state.most_recent = true
    )
    SELECT
        task_counts.*,
        step_counts.*
    FROM task_counts, step_counts;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Phase 1.4: Add transition_task_state_atomic SQL function
-- ============================================================================

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
        sort_key, most_recent, created_at, updated_at
    )
    SELECT
        p_task_uuid, p_from_state, p_to_state,
        p_processor_uuid, p_metadata,
        v_sort_key, true, NOW(), NOW()
    WHERE EXISTS (SELECT 1 FROM do_update);

    GET DIAGNOSTICS v_transitioned = ROW_COUNT;
    RETURN v_transitioned > 0;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Phase 1.5: Add get_next_ready_tasks SQL functions
-- ============================================================================

-- Single task version (delegates to batch version)
CREATE OR REPLACE FUNCTION get_next_ready_task()
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
) AS $$
BEGIN
    -- Simply delegate to batch function with limit of 1
    RETURN QUERY
    SELECT * FROM get_next_ready_tasks(1);
END;
$$ LANGUAGE plpgsql;

-- Batch version
CREATE OR REPLACE FUNCTION get_next_ready_tasks(p_limit INTEGER DEFAULT 5)
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
) AS $$
BEGIN
    RETURN QUERY
    WITH task_candidates AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            t.priority,
            tns.name as namespace_name,
            tt.to_state as current_state,
            -- Compute priority with age escalation
            t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1) as computed_priority
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt on nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns on tns.task_namespace_uuid = nt.task_namespace_uuid
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

-- ============================================================================
-- get task ready info for single task
-- ================================

CREATE OR REPLACE FUNCTION get_task_ready_info(p_task_uuid UUID)
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
) AS $$
BEGIN
    RETURN QUERY
    WITH task_candidate AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            t.priority,
            tns.name as namespace_name,
            tt.to_state as current_state,
            -- Compute priority with age escalation
            t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1) as computed_priority
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt on nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns on tns.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN tasker_task_transitions tt_processing
            ON tt_processing.task_uuid = t.task_uuid
            AND tt_processing.most_recent = true
            AND tt_processing.processor_uuid IS NOT NULL
            AND tt_processing.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
        WHERE tt.most_recent = true
        AND t.task_uuid = p_task_uuid
    ),
    task_with_context AS (
        SELECT
            tc.*,
            ctx.ready_steps,
            ctx.execution_status
        FROM task_candidate tc
        CROSS JOIN LATERAL get_task_execution_context(p_task_uuid) ctx
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
    ORDER BY twc.computed_priority DESC;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Phase 1.6: Add find_stuck_tasks and get_current_task_state functions
-- ============================================================================

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

-- ============================================================================
-- Rollback Section
-- ============================================================================

-- To rollback this migration, run:
-- DROP FUNCTION IF EXISTS get_current_task_state(UUID);
-- DROP FUNCTION IF EXISTS find_stuck_tasks(INTEGER);
-- DROP FUNCTION IF EXISTS get_next_ready_tasks(INTEGER);
-- DROP FUNCTION IF EXISTS get_next_ready_task();
-- DROP FUNCTION IF EXISTS transition_task_state_atomic(UUID, VARCHAR, VARCHAR, UUID, JSONB);
-- DROP INDEX IF EXISTS idx_task_transitions_timeout;
-- DROP INDEX IF EXISTS idx_task_transitions_processor;
-- ALTER TABLE public.tasker_task_transitions DROP COLUMN IF EXISTS transition_metadata;
-- ALTER TABLE public.tasker_task_transitions DROP COLUMN IF EXISTS processor_uuid;

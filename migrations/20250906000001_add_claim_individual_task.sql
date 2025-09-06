-- Migration: Add claim_individual_task function for TAS-41
-- Purpose: Enable claiming of specific individual tasks for immediate step enqueuing after creation
-- This reduces latency between task creation and step execution

-- Function: claim_individual_task
-- Claims a specific task by UUID for processing if it's ready
-- Similar to claim_ready_tasks but for a single, specific task
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
    -- Uses UPDATE...RETURNING pattern for atomic claim
    RETURN QUERY
    UPDATE tasker_tasks t
    SET claimed_at = NOW(),
        claimed_by = p_orchestrator_id,
        claim_timeout_seconds = COALESCE(p_claim_timeout_seconds, t.claim_timeout_seconds),
        updated_at = NOW()
    FROM (
        SELECT
            t.task_uuid,
            -- Compute priority using same logic as claim_ready_tasks for consistency
            -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
            -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
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
            AND (t.claimed_at IS NULL
                 OR t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval)
                 OR t.claimed_by = p_orchestrator_id)  -- Allow re-claiming by same processor for claim transfer
        FOR UPDATE OF t SKIP LOCKED
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

-- Add comment for documentation
COMMENT ON FUNCTION claim_individual_task IS 'TAS-41: Claims a specific task by UUID for immediate processing. Used to reduce latency after task creation by immediately enqueuing ready steps.';

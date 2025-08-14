-- TAS-32: Update SQL Functions for Enqueued State
--
-- This migration updates SQL functions to properly handle the new 'enqueued' state
-- introduced in TAS-32 for queue state management improvements.
--
-- KEY CHANGES:
-- 1. Exclude 'enqueued' steps from ready_for_execution (they're already in pipeline)
-- 2. Add enqueued_steps count to system health metrics
-- 3. Update step counting logic to include enqueued state
-- 4. Improved in_backoff_steps calculation using explicit backoff_request_seconds

-- =============================================================================
-- UPDATE get_step_readiness_status FUNCTION
-- =============================================================================

-- No need to drop this function as we're not changing its return type
CREATE OR REPLACE FUNCTION public.get_step_readiness_status(
    input_task_uuid uuid,
    step_uuids uuid[] DEFAULT NULL::uuid[]
)
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
    name text,
    current_state text,
    dependencies_satisfied boolean,
    retry_eligible boolean,
    ready_for_execution boolean,
    last_failure_at timestamp without time zone,
    next_retry_at timestamp without time zone,
    total_parents integer,
    completed_parents integer,
    attempts integer,
    retry_limit integer,
    backoff_request_seconds integer,
    last_attempted_at timestamp without time zone
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM tasker_workflow_step_transitions wst
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),
  dependency_counts AS (
    SELECT
      wse.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_state.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM tasker_workflow_step_edges wse
    LEFT JOIN step_states parent_state ON parent_state.workflow_step_uuid = wse.from_step_uuid
    WHERE wse.to_step_uuid IN (
      SELECT ws.workflow_step_uuid
      FROM tasker_workflow_steps ws
      WHERE ws.task_uuid = input_task_uuid
    )
    GROUP BY wse.to_step_uuid
  ),
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM tasker_workflow_step_transitions wst
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  )
  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied (keeping original CASE logic)
    CASE
      WHEN dc.total_deps IS NULL THEN true  -- No dependencies
      WHEN dc.completed_deps = dc.total_deps THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Retry eligibility (keeping original CASE logic)
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Ready for execution
    -- NOTE: 'enqueued' is NOT included here because enqueued steps are already in the queue pipeline
    -- Only 'pending' and 'error' states are eligible for execution
    CASE
      WHEN COALESCE(ss.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)
      AND (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps)
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))
      AND (COALESCE(ws.retryable, true) = true)
      AND (ws.in_process = false OR ws.in_process IS NULL)
      AND (
        -- Check explicit backoff timing (most restrictive)
        -- If backoff is set, the backoff period must have expired
        CASE
          WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
            ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') <= NOW()
          ELSE true  -- No explicit backoff set
        END
        AND
        -- Then check failure-based backoff
        (lf.failure_time IS NULL OR
         lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    lf.failure_time as last_failure_at,

    -- Next retry calculation
    CASE
      WHEN ws.last_attempted_at IS NOT NULL AND ws.backoff_request_seconds IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN lf.failure_time IS NOT NULL THEN
        lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = input_task_uuid
    AND (step_uuids IS NULL OR ws.workflow_step_uuid = ANY(step_uuids))
  ORDER BY ws.workflow_step_uuid;
END;
$$;

-- =============================================================================
-- UPDATE get_task_execution_context FUNCTION
-- =============================================================================

-- Drop the function first because we're adding a new column to the return type
DROP FUNCTION IF EXISTS public.get_task_execution_context(uuid) CASCADE;

CREATE FUNCTION public.get_task_execution_context(input_task_uuid uuid)
RETURNS TABLE(
    task_uuid uuid,
    named_task_uuid uuid,
    status text,
    total_steps bigint,
    pending_steps bigint,
    in_progress_steps bigint,
    completed_steps bigint,
    failed_steps bigint,
    ready_steps bigint,
    execution_status text,
    recommended_action text,
    completion_percentage numeric,  -- MAINTAINED for backward compatibility
    health_status text,            -- MAINTAINED for backward compatibility
    enqueued_steps bigint          -- NEW: Added at END for backward compatibility
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    SELECT * FROM get_step_readiness_status(input_task_uuid, NULL)
  ),
  task_info AS (
    SELECT
      t.task_uuid,
      t.named_task_uuid,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM tasker_tasks t
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
    WHERE t.task_uuid = input_task_uuid
  ),
  aggregated_stats AS (
    SELECT
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,  -- NEW
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      -- Count PERMANENTLY blocked failures (exhausted retries OR explicitly marked as not retryable)
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.retry_limit) THEN 1 END) as permanently_blocked_steps
    FROM step_data sd
  )
  SELECT
    ti.task_uuid,
    ti.named_task_uuid,
    ti.current_status as status,

    -- Step Statistics
    COALESCE(ast.total_steps, 0) as total_steps,
    COALESCE(ast.pending_steps, 0) as pending_steps,
    COALESCE(ast.in_progress_steps, 0) as in_progress_steps,
    COALESCE(ast.completed_steps, 0) as completed_steps,
    COALESCE(ast.failed_steps, 0) as failed_steps,
    COALESCE(ast.ready_steps, 0) as ready_steps,

    -- Execution State Logic (updated to account for enqueued steps)
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 OR COALESCE(ast.enqueued_steps, 0) > 0 THEN 'processing'  -- UPDATED
      -- Only blocked if failed steps are NOT retry-eligible
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- Recommended Action Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 OR COALESCE(ast.enqueued_steps, 0) > 0 THEN 'wait_for_completion'  -- UPDATED
      -- Only handle failures if they're truly blocked
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'handle_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Progress Metrics
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    -- Health Status Logic
    CASE
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      -- Only blocked if failures are truly not retry-eligible
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      -- Waiting state for retry-eligible failures with backoff
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    -- NEW: enqueued_steps added at end for backward compatibility
    COALESCE(ast.enqueued_steps, 0) as enqueued_steps

  FROM task_info ti
  CROSS JOIN aggregated_stats ast;
END;
$$;

-- =============================================================================
-- UPDATE get_task_execution_contexts_batch FUNCTION
-- =============================================================================

-- Drop the function first because we're adding a new column to the return type
DROP FUNCTION IF EXISTS public.get_task_execution_contexts_batch(uuid[]) CASCADE;

CREATE FUNCTION public.get_task_execution_contexts_batch(input_task_uuids uuid[])
RETURNS TABLE(
    task_uuid uuid,
    named_task_uuid uuid,
    status text,
    total_steps bigint,
    pending_steps bigint,
    in_progress_steps bigint,
    completed_steps bigint,
    failed_steps bigint,
    ready_steps bigint,
    execution_status text,
    recommended_action text,
    completion_percentage numeric,  -- MAINTAINED for backward compatibility
    health_status text,            -- MAINTAINED for backward compatibility
    enqueued_steps bigint          -- NEW: Added at END for backward compatibility
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    SELECT * FROM get_step_readiness_status_batch(input_task_uuids)
  ),
  task_info AS (
    SELECT
      t.task_uuid,
      t.named_task_uuid,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM unnest(input_task_uuids) AS task_list(task_uuid)
    JOIN tasker_tasks t ON t.task_uuid = task_list.task_uuid
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
  ),
  aggregated_stats AS (
    SELECT
      sd.task_uuid,
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,  -- NEW
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      COUNT(CASE WHEN sd.current_state = 'error' AND (sd.attempts >= sd.retry_limit) THEN 1 END) as permanently_blocked_steps
    FROM step_data sd
    GROUP BY sd.task_uuid
  )
  SELECT
    ti.task_uuid,
    ti.named_task_uuid,
    ti.current_status as status,
    COALESCE(ast.total_steps, 0) as total_steps,
    COALESCE(ast.pending_steps, 0) as pending_steps,
    COALESCE(ast.in_progress_steps, 0) as in_progress_steps,
    COALESCE(ast.completed_steps, 0) as completed_steps,
    COALESCE(ast.failed_steps, 0) as failed_steps,
    COALESCE(ast.ready_steps, 0) as ready_steps,

    -- Execution status (updated to account for enqueued steps)
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 OR COALESCE(ast.enqueued_steps, 0) > 0 THEN 'processing'  -- UPDATED
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- Recommended action (keeping consistent with original batch function)
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'process_ready_steps'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'review_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'mark_complete'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 OR COALESCE(ast.enqueued_steps, 0) > 0 THEN 'wait_for_completion'  -- UPDATED
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Completion percentage (keeping consistent with original batch function)
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 100.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::numeric / ast.total_steps::numeric) * 100, 2)
    END as completion_percentage,

    -- Health Status Logic
    CASE
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    -- NEW: enqueued_steps added at end for backward compatibility
    COALESCE(ast.enqueued_steps, 0) as enqueued_steps

  FROM task_info ti
  LEFT JOIN aggregated_stats ast ON ast.task_uuid = ti.task_uuid;
END;
$$;

-- =============================================================================
-- UPDATE get_step_readiness_status_batch FUNCTION
-- =============================================================================

-- No need to drop this function as we're not changing its return type
CREATE OR REPLACE FUNCTION public.get_step_readiness_status_batch(input_task_uuids uuid[])
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
    name text,
    current_state text,
    dependencies_satisfied boolean,
    retry_eligible boolean,
    ready_for_execution boolean,
    last_failure_at timestamp without time zone,
    next_retry_at timestamp without time zone,
    total_parents integer,
    completed_parents integer,
    attempts integer,
    retry_limit integer,
    backoff_request_seconds integer,
    last_attempted_at timestamp without time zone
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM tasker_workflow_step_transitions wst
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),
  dependency_counts AS (
    SELECT
      wse.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_state.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM tasker_workflow_step_edges wse
    LEFT JOIN step_states parent_state ON parent_state.workflow_step_uuid = wse.from_step_uuid
    WHERE wse.to_step_uuid IN (
      SELECT ws.workflow_step_uuid
      FROM tasker_workflow_steps ws
      WHERE ws.task_uuid = ANY(input_task_uuids)
    )
    GROUP BY wse.to_step_uuid
  ),
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM tasker_workflow_step_transitions wst
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  )
  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied (keeping original CASE logic)
    CASE
      WHEN dc.total_deps IS NULL THEN true
      WHEN dc.completed_deps = dc.total_deps THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Retry eligibility (keeping original CASE logic)
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Ready for execution (matching original batch function logic)
    -- NOTE: 'enqueued' is NOT included here because enqueued steps are already in the queue pipeline
    CASE
      WHEN COALESCE(ss.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)
      AND (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps)
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))
      AND (COALESCE(ws.retryable, true) = true)
      AND (ws.in_process = false OR ws.in_process IS NULL)
      AND (
        ws.last_attempted_at IS NULL OR
        ws.last_attempted_at + (COALESCE(ws.backoff_request_seconds, 0) * interval '1 second') <= NOW() OR
        (lf.failure_time IS NULL OR
         lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    lf.failure_time as last_failure_at,

    -- Next retry calculation
    CASE
      WHEN ws.last_attempted_at IS NOT NULL AND ws.backoff_request_seconds IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN lf.failure_time IS NOT NULL THEN
        lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = ANY(input_task_uuids)
  ORDER BY ws.workflow_step_uuid;
END;
$$;

-- =============================================================================
-- UPDATE get_system_health_counts FUNCTION
-- =============================================================================

-- Drop the function first because we're adding a new column to the return type
DROP FUNCTION IF EXISTS public.get_system_health_counts() CASCADE;

CREATE FUNCTION public.get_system_health_counts()
RETURNS TABLE(
    total_tasks bigint,
    pending_tasks bigint,
    in_progress_tasks bigint,
    complete_tasks bigint,
    error_tasks bigint,
    cancelled_tasks bigint,
    total_steps bigint,
    pending_steps bigint,
    in_progress_steps bigint,
    complete_steps bigint,
    error_steps bigint,
    retryable_error_steps bigint,
    exhausted_retry_steps bigint,
    in_backoff_steps bigint,
    active_connections bigint,
    max_connections bigint,
    enqueued_steps bigint  -- NEW: Added at END for backward compatibility
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
    RETURN QUERY
    WITH task_counts AS (
        SELECT
            COUNT(*) as total_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'pending') as pending_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'in_progress') as in_progress_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'complete') as complete_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'error') as error_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'cancelled') as cancelled_tasks
        FROM tasker_tasks t
        LEFT JOIN tasker_task_transitions task_state ON task_state.task_uuid = t.task_uuid
            AND task_state.most_recent = true
    ),
    step_counts AS (
        SELECT
            COUNT(*) as total_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'pending') as pending_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'enqueued') as enqueued_steps,  -- NEW
            COUNT(*) FILTER (WHERE step_state.to_state = 'in_progress') as in_progress_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'complete') as complete_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'error') as error_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts < ws.retry_limit
                AND COALESCE(ws.retryable, true) = true
            ) as retryable_error_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts >= ws.retry_limit
            ) as exhausted_retry_steps,
            -- IMPROVED: More accurate in_backoff_steps calculation using explicit backoff_request_seconds
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.backoff_request_seconds IS NOT NULL
                AND ws.last_attempted_at IS NOT NULL
                AND ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') > NOW()
            ) as in_backoff_steps
        FROM tasker_workflow_steps ws
        LEFT JOIN tasker_workflow_step_transitions step_state ON step_state.workflow_step_uuid = ws.workflow_step_uuid
            AND step_state.most_recent = true
    ),
    connection_info AS (
        SELECT
            (SELECT count(*) FROM pg_stat_activity WHERE state != 'idle') as active_connections,
            (SELECT setting::bigint FROM pg_settings WHERE name = 'max_connections') as max_connections
    )
    SELECT
        tc.total_tasks,
        tc.pending_tasks,
        tc.in_progress_tasks,
        tc.complete_tasks,
        tc.error_tasks,
        tc.cancelled_tasks,
        sc.total_steps,
        sc.pending_steps,
        sc.in_progress_steps,
        sc.complete_steps,
        sc.error_steps,
        sc.retryable_error_steps,
        sc.exhausted_retry_steps,
        sc.in_backoff_steps,
        ci.active_connections,
        ci.max_connections,
        sc.enqueued_steps  -- NEW: Added at end for backward compatibility
    FROM task_counts tc
    CROSS JOIN step_counts sc
    CROSS JOIN connection_info ci;
END;
$$;

-- =============================================================================
-- RECREATE TASKER_READY_TASKS VIEW (Required due to CASCADE)
-- =============================================================================
--
-- IMPORTANT: The tasker_ready_tasks view was dropped during the CASCADE operation
-- when we dropped the get_task_execution_context function above. We need to recreate
-- it with the updated schema that includes the new enqueued_steps column.
--
-- This view is critical for:
-- - Task claiming and orchestration
-- - Ready task discovery
-- - Priority-based task processing
-- - System monitoring and health checks

CREATE OR REPLACE VIEW public.tasker_ready_tasks AS
SELECT
    t.task_uuid,
    tn.name as namespace_name,
    t.priority,
    t.created_at,
    t.updated_at,
    t.claimed_at,
    t.claimed_by,
    -- Use existing get_task_execution_context() function for proven readiness logic
    tec.ready_steps as ready_steps_count,
    tec.execution_status,
    tec.total_steps,
    tec.completed_steps,
    tec.pending_steps,
    tec.failed_steps,
    tec.in_progress_steps,
    tec.enqueued_steps as enqueued_steps_count,
    -- Calculate task age in hours for monitoring and debugging
    ROUND(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600.0, 2)::float8 as age_hours,
    -- Computed priority with time-weighted escalation to prevent starvation
    -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
    -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
    (CASE
        WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)      -- Urgent (4): +1 per 5min, max +2 (final: 6)
        WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)      -- High (3): +1 per 3min, max +3 (final: 6)
        WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)      -- Normal (2): +1 per 2min, max +4 (final: 6)
        WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)       -- Low (1): +1 per 1min, max +5 (final: 6)
        ELSE                            0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)                      -- Zero/Invalid (0): +1 per 30sec, max +6 (final: 6)
    END)::float8 as computed_priority,
    -- Calculate claim status for distributed coordination using configurable timeout
    CASE
        WHEN t.claimed_at IS NULL THEN 'available'
        WHEN t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval) THEN 'stale_claim'
        ELSE 'claimed'
    END as claim_status,
    -- Calculate how long task has been claimed (for monitoring)
    CASE
        WHEN t.claimed_at IS NOT NULL
        THEN EXTRACT(EPOCH FROM (NOW() - t.claimed_at))::integer
        ELSE NULL
    END as claimed_duration_seconds
FROM public.tasker_tasks t
-- Join with named_tasks to get task metadata
INNER JOIN public.tasker_named_tasks nt ON t.named_task_uuid = nt.named_task_uuid
-- Join with task_namespaces to get namespace name
INNER JOIN public.tasker_task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
-- Use LATERAL JOIN with get_task_execution_context() for efficiency
JOIN LATERAL (
    SELECT * FROM get_task_execution_context(t.task_uuid)
) tec ON true
WHERE
    -- Only include incomplete tasks
    t.complete = false
    -- Only include tasks with ready steps (from existing SQL function logic)
    AND tec.ready_steps > 0
    AND tec.execution_status IN ('processing', 'pending', 'has_ready_steps')
    -- Include unclaimed tasks OR stale claims (configurable timeout per task)
    AND (t.claimed_at IS NULL OR t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval))
ORDER BY
    -- Use computed_priority to ensure fairness with time-weighted escalation
    -- This prevents starvation while respecting base priority levels
    -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
    -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
    (CASE
        WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)      -- Urgent (4): +1 per 5min, max +2 (final: 6)
        WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)      -- High (3): +1 per 3min, max +3 (final: 6)
        WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)      -- Normal (2): +1 per 2min, max +4 (final: 6)
        WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)       -- Low (1): +1 per 1min, max +5 (final: 6)
        ELSE                            0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)                      -- Zero/Invalid (0): +1 per 30sec, max +6 (final: 6)
    END)::float8 DESC,
    -- Break ties with creation order (oldest first)
    t.created_at ASC;

-- =============================================================================
-- SECTION 8: ORCHESTRATION FUNCTIONS (Critical for distributed processing)
-- =============================================================================

--
-- Name: claim_ready_tasks; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.claim_ready_tasks(
    p_orchestrator_id character varying,
    p_limit integer DEFAULT 1,
    p_namespace_filter character varying DEFAULT NULL
)
RETURNS TABLE(
    task_uuid uuid,
    namespace_name character varying,
    priority integer,
    computed_priority float8,
    age_hours float8,
    ready_steps_count bigint,
    claim_timeout_seconds integer
)
LANGUAGE plpgsql
AS $$
BEGIN
    -- Atomically claim tasks using proper locking on base table with computed priority ordering
    RETURN QUERY
    UPDATE tasker_tasks t
    SET claimed_at = NOW(),
        claimed_by = p_orchestrator_id,
        updated_at = NOW()
    FROM (
        SELECT
            t.task_uuid,
            -- Include computed priority and age for debugging and monitoring
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
        JOIN tasker_named_tasks nt ON t.named_task_uuid = nt.named_task_uuid
        JOIN tasker_task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
        JOIN LATERAL (SELECT * FROM get_task_execution_context(t.task_uuid)) tec ON true
        WHERE t.complete = false
            AND tec.ready_steps > 0
            AND (t.claimed_at IS NULL OR t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval))
            AND (p_namespace_filter IS NULL OR tn.name = p_namespace_filter)
        ORDER BY
            -- Use computed priority for fair ordering
            -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
            -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
            (CASE
                WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)
                WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)
                WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)
                WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)
                ELSE                             0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)
            END)::float8 DESC,
            t.created_at ASC
        LIMIT p_limit
        FOR UPDATE OF t SKIP LOCKED
    ) ready_tasks
    WHERE t.task_uuid = ready_tasks.task_uuid
    RETURNING
        t.task_uuid,
        (SELECT name FROM tasker_task_namespaces WHERE task_namespace_uuid =
            (SELECT task_namespace_uuid FROM tasker_named_tasks WHERE named_task_uuid = t.named_task_uuid)
        ) as namespace_name,
        t.priority,
        ready_tasks.computed_priority_calc::float8 as computed_priority,
        ready_tasks.age_hours_calc::float8 as age_hours,
        (SELECT ready_steps FROM get_task_execution_context(t.task_uuid)) as ready_steps_count,
        t.claim_timeout_seconds;
END;
$$;

--
-- Name: release_task_claim; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.release_task_claim(
    p_task_uuid uuid,
    p_orchestrator_id character varying
)
RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows_updated integer;
BEGIN
    UPDATE tasker_tasks
    SET claimed_at = NULL,
        claimed_by = NULL,
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND claimed_by = p_orchestrator_id;  -- Only release if we own the claim

    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;

    -- Return true if claim was released, false if not found or not owned
    RETURN v_rows_updated > 0;
END;
$$;

--
-- Name: extend_task_claim; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.extend_task_claim(
    p_task_uuid uuid,
    p_orchestrator_id character varying
)
RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows_updated integer;
BEGIN
    UPDATE tasker_tasks
    SET claimed_at = NOW(),  -- Reset claim time
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND claimed_by = p_orchestrator_id  -- Only extend if we own the claim
        AND claimed_at IS NOT NULL;         -- And claim exists

    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;

    -- Return true if claim was extended, false if not found or not owned
    RETURN v_rows_updated > 0;
END;
$$;

--
-- Name: get_step_transitive_dependencies; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_step_transitive_dependencies(target_step_uuid uuid)
RETURNS TABLE (
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
    step_name character varying(128),
    results jsonb,
    processed boolean,
    distance integer
)
LANGUAGE plpgsql STABLE
AS $$
BEGIN
    RETURN QUERY
    WITH RECURSIVE transitive_deps AS (
        -- Base case: direct parents of the target step
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ws.named_step_uuid,
            ns.name as step_name,
            ws.results,
            ws.processed,
            1 as distance
        FROM tasker_workflow_step_edges wse
        JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        WHERE wse.to_step_uuid = target_step_uuid

        UNION ALL

        -- Recursive case: parents of parents
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ws.named_step_uuid,
            ns.name as step_name,
            ws.results,
            ws.processed,
            td.distance + 1
        FROM transitive_deps td
        JOIN tasker_workflow_step_edges wse ON wse.to_step_uuid = td.workflow_step_uuid
        JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        WHERE td.distance < 50  -- Prevent infinite recursion
    )
    SELECT
        td.workflow_step_uuid,
        td.task_uuid,
        td.named_step_uuid,
        td.step_name,
        td.results,
        td.processed,
        td.distance
    FROM transitive_deps td
    ORDER BY td.distance ASC, td.workflow_step_uuid ASC;
END;
$$;


-- =============================================================================
-- MIGRATION COMPLETION CONFIRMATION
-- =============================================================================

DO $$
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '=============================================================================';
    RAISE NOTICE 'TAS-32 SQL FUNCTIONS UPDATE COMPLETED';
    RAISE NOTICE '=============================================================================';
    RAISE NOTICE '';
    RAISE NOTICE 'FUNCTIONS UPDATED:';
    RAISE NOTICE '  ✅ get_step_readiness_status() - Excludes enqueued steps from ready_for_execution';
    RAISE NOTICE '  ✅ get_step_readiness_status_batch() - Excludes enqueued steps from ready_for_execution';
    RAISE NOTICE '  ✅ get_task_execution_context() - Adds enqueued_steps count with backward compatibility';
    RAISE NOTICE '  ✅ get_task_execution_contexts_batch() - Adds enqueued_steps count with backward compatibility';
    RAISE NOTICE '  ✅ get_system_health_counts() - Adds enqueued_steps metric with backward compatibility';
    RAISE NOTICE '';
    RAISE NOTICE 'VIEW RECREATED:';
    RAISE NOTICE '  ✅ tasker_ready_tasks - Recreated after CASCADE drop with enqueued_steps support';
    RAISE NOTICE '';
    RAISE NOTICE 'CLAIMING FUNCTIONS RECREATED:';
    RAISE NOTICE '  ✅ claim_ready_tasks() - Recreated after CASCADE drop (critical for orchestration)';
    RAISE NOTICE '  ✅ release_task_claim() - Recreated after CASCADE drop (critical for orchestration)';
    RAISE NOTICE '';
    RAISE NOTICE 'KEY ARCHITECTURAL CHANGES:';
    RAISE NOTICE '  • Enqueued steps are NOT considered ready for execution (already in pipeline)';
    RAISE NOTICE '  • Processing status includes both in_progress AND enqueued steps';
    RAISE NOTICE '  • System health metrics now track enqueued steps separately';
    RAISE NOTICE '  • Database maintains single source of truth for step processing state';
    RAISE NOTICE '  • New columns added at END of tables for backward compatibility';
    RAISE NOTICE '  • Improved in_backoff_steps calculation using explicit backoff_request_seconds';
    RAISE NOTICE '  • tasker_ready_tasks view recreated due to CASCADE dependency on updated functions';
    RAISE NOTICE '';
    RAISE NOTICE 'BACKWARD COMPATIBILITY:';
    RAISE NOTICE '  • All original columns maintained in same positions';
    RAISE NOTICE '  • New enqueued_steps column added at end of return tables';
    RAISE NOTICE '  • Existing client code will continue to work unchanged';
    RAISE NOTICE '  • View recreation transparent to existing clients';
    RAISE NOTICE '';
    RAISE NOTICE '=============================================================================';
END
$$;

-- Fix step readiness logic for NULL attempts
-- When attempts is NULL, it should be treated as 0 for comparison purposes

-- Update get_step_readiness_status function
CREATE OR REPLACE FUNCTION public.get_step_readiness_status(input_task_id bigint, step_ids bigint[] DEFAULT NULL::bigint[]) RETURNS TABLE(workflow_step_id bigint, task_id bigint, named_step_id integer, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, retry_limit integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  SELECT
    ws.workflow_step_id,
    ws.task_id,
    ws.named_step_id,
    ns.name::TEXT,

    -- Current State Information (optimized using most_recent flag)
    COALESCE(current_state.to_state, 'pending')::TEXT as current_state,

    -- Dependency Satisfaction Analysis
    CASE
      WHEN dep_edges.to_step_id IS NULL OR
           COUNT(dep_edges.from_step_id) = 0 THEN true
      ELSE
        COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id)
    END as dependencies_satisfied,

    -- Retry Eligibility
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Overall Ready for Execution (complex business logic)
    CASE
      WHEN COALESCE(current_state.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)  -- CRITICAL: Only unprocessed steps can be ready
      AND (dep_edges.to_step_id IS NULL OR
           COUNT(dep_edges.from_step_id) = 0 OR
           COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id))
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))  -- FIXED: Handle NULL attempts
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
        (last_failure.created_at IS NULL OR
         last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    -- Timing Information
    last_failure.created_at as last_failure_at,
    CASE
      WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN last_failure.created_at IS NOT NULL THEN
        last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    -- Dependency Context (calculated from joins)
    COALESCE(COUNT(dep_edges.from_step_id), 0)::INTEGER as total_parents,
    COALESCE(COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END), 0)::INTEGER as completed_parents,

    -- Retry Context
    COALESCE(ws.attempts, 0)::INTEGER as attempts,  -- FIXED: Return 0 instead of NULL
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id

  -- OPTIMIZED: Current State using most_recent flag instead of DISTINCT ON
  LEFT JOIN tasker_workflow_step_transitions current_state
    ON current_state.workflow_step_id = ws.workflow_step_id
    AND current_state.most_recent = true

  -- OPTIMIZED: Dependency check using direct joins (no subquery)
  LEFT JOIN tasker_workflow_step_edges dep_edges
    ON dep_edges.to_step_id = ws.workflow_step_id
  LEFT JOIN tasker_workflow_step_transitions parent_states
    ON parent_states.workflow_step_id = dep_edges.from_step_id
    AND parent_states.most_recent = true

  -- OPTIMIZED: Last failure using index-optimized approach
  LEFT JOIN tasker_workflow_step_transitions last_failure
    ON last_failure.workflow_step_id = ws.workflow_step_id
    AND last_failure.to_state = 'error'
    AND last_failure.most_recent = true

  -- KEY PERFORMANCE IMPROVEMENT: Filter by task first, then optionally by step IDs
  -- CRITICAL FIX: Include ALL steps for task execution context calculation
  -- Only filter by processed status when specifically querying for ready steps
  WHERE ws.task_id = input_task_id
    AND (step_ids IS NULL OR ws.workflow_step_id = ANY(step_ids))

  GROUP BY
    ws.workflow_step_id, ws.task_id, ws.named_step_id, ns.name,
    current_state.to_state, last_failure.created_at,
    ws.attempts, ws.retry_limit, ws.backoff_request_seconds, ws.last_attempted_at,
    ws.in_process, ws.processed, ws.retryable, dep_edges.to_step_id,
    current_state.workflow_step_id, last_failure.workflow_step_id;
END;
$$;

-- Also update the batch version
CREATE OR REPLACE FUNCTION public.get_step_readiness_status_batch(input_task_ids bigint[]) RETURNS TABLE(workflow_step_id bigint, task_id bigint, named_step_id integer, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, retry_limit integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  SELECT
    ws.workflow_step_id,
    ws.task_id,
    ws.named_step_id,
    ns.name::TEXT,

    -- Current State Information (optimized using most_recent flag)
    COALESCE(current_state.to_state, 'pending')::TEXT as current_state,

    -- Dependency Satisfaction Analysis
    CASE
      WHEN dep_edges.to_step_id IS NULL OR
           COUNT(dep_edges.from_step_id) = 0 THEN true
      ELSE
        COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id)
    END as dependencies_satisfied,

    -- Retry Eligibility
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Overall Ready for Execution (complex business logic)
    CASE
      WHEN COALESCE(current_state.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)
      AND (dep_edges.to_step_id IS NULL OR
           COUNT(dep_edges.from_step_id) = 0 OR
           COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id))
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))  -- FIXED: Handle NULL attempts
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
        (last_failure.created_at IS NULL OR
         last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    -- Timing Information
    last_failure.created_at as last_failure_at,
    CASE
      WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN last_failure.created_at IS NOT NULL THEN
        last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    -- Dependency Context (calculated from joins)
    COALESCE(COUNT(dep_edges.from_step_id), 0)::INTEGER as total_parents,
    COALESCE(COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END), 0)::INTEGER as completed_parents,

    -- Retry Context
    COALESCE(ws.attempts, 0)::INTEGER as attempts,  -- FIXED: Return 0 instead of NULL
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id

  -- OPTIMIZED: Current State using most_recent flag instead of DISTINCT ON
  LEFT JOIN tasker_workflow_step_transitions current_state
    ON current_state.workflow_step_id = ws.workflow_step_id
    AND current_state.most_recent = true

  -- OPTIMIZED: Dependency check using direct joins (no subquery)
  LEFT JOIN tasker_workflow_step_edges dep_edges
    ON dep_edges.to_step_id = ws.workflow_step_id
  LEFT JOIN tasker_workflow_step_transitions parent_states
    ON parent_states.workflow_step_id = dep_edges.from_step_id
    AND parent_states.most_recent = true

  -- OPTIMIZED: Last failure using index-optimized approach
  LEFT JOIN tasker_workflow_step_transitions last_failure
    ON last_failure.workflow_step_id = ws.workflow_step_id
    AND last_failure.to_state = 'error'
    AND last_failure.most_recent = true

  -- KEY PERFORMANCE IMPROVEMENT: Filter by task first, then optionally by step IDs
  WHERE ws.task_id = ANY(input_task_ids)

  GROUP BY
    ws.workflow_step_id, ws.task_id, ws.named_step_id, ns.name,
    current_state.to_state, last_failure.created_at,
    ws.attempts, ws.retry_limit, ws.backoff_request_seconds, ws.last_attempted_at,
    ws.in_process, ws.processed, ws.retryable, dep_edges.to_step_id,
    current_state.workflow_step_id, last_failure.workflow_step_id;
END;
$$;
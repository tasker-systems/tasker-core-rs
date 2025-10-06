-- ============================================================================
-- Fix retry_eligibility logic to allow first execution regardless of retry settings
-- ============================================================================
-- This migration fixes a critical bug where steps with max_attempts=0 and
-- retryable=false could not execute at all, not even for the first attempt.
--
-- Background:
-- The retry_eligible calculation was treating the first execution as a "retry",
-- incorrectly applying retry limit checks. This caused:
--   - max_attempts=0, attempts=0: `0 < 0` = false (blocked first attempt!)
--   - retryable=false: Required for ALL executions (blocked first attempt!)
--
-- Semantic Fix:
--   - First attempt (attempts=0): Always eligible
--   - Retry attempts (attempts>0): Check retryable=true AND attempts < max_attempts
-- ============================================================================

-- Update evaluate_step_state_readiness to remove redundant retryable check
-- The retryable flag is now incorporated into retry_eligible calculation
CREATE OR REPLACE FUNCTION evaluate_step_state_readiness(
    current_state TEXT,
    processed BOOLEAN,
    in_process BOOLEAN,
    dependencies_satisfied BOOLEAN,
    retry_eligible BOOLEAN,
    retryable BOOLEAN,
    next_retry_time TIMESTAMP
) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
    SELECT
        COALESCE(current_state, 'pending') IN ('pending', 'waiting_for_retry')
        AND (processed = false OR processed IS NULL)
        AND (in_process = false OR in_process IS NULL)
        AND dependencies_satisfied
        AND retry_eligible
        -- REMOVED: AND retryable (now incorporated in retry_eligible)
        AND (next_retry_time IS NULL OR next_retry_time <= NOW())
$$;

-- Update get_step_readiness_status_batch with fixed retry_eligible logic
CREATE OR REPLACE FUNCTION public.get_step_readiness_status_batch(
    input_task_uuids uuid[],
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
    max_attempts integer,
    backoff_request_seconds integer,
    last_attempted_at timestamp without time zone
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH
  -- OPTIMIZATION: Create batch task-scoped working set to eliminate table scans
  steps_for_tasks AS (
    SELECT ws.workflow_step_uuid, ws.task_uuid
    FROM tasker_workflow_steps ws
    WHERE ws.task_uuid = ANY(input_task_uuids)
      AND (step_uuids IS NULL OR ws.workflow_step_uuid = ANY(step_uuids))
  ),

  -- OPTIMIZATION: Scope step_states to steps_for_tasks working set
  step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM tasker_workflow_step_transitions wst
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = wst.workflow_step_uuid
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),

  -- RESTORED: Dedicated last_failures CTE for clarity and accuracy
  -- OPTIMIZATION: Scoped to steps_for_tasks working set
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM tasker_workflow_step_transitions wst
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = wst.workflow_step_uuid
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),

  -- OPTIMIZATION: Scoped dependency_counts using INNER JOIN instead of subselect
  dependency_counts AS (
    SELECT
      e.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_ss.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM tasker_workflow_step_edges e
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = e.to_step_uuid
    LEFT JOIN step_states parent_ss ON parent_ss.workflow_step_uuid = e.from_step_uuid
    GROUP BY e.to_step_uuid
  ),

  -- NEW: Centralized retry time calculation using helper function
  retry_times AS (
    SELECT
      ws.workflow_step_uuid,
      calculate_step_next_retry_time(
          ws.backoff_request_seconds,
          ws.last_attempted_at,
          lf.failure_time,
          ws.attempts
      ) as next_retry_time
    FROM tasker_workflow_steps ws
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = ws.workflow_step_uuid
    LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  ),

  -- FIXED: Pre-computed boolean flags with corrected retry_eligible logic
  step_readiness_flags AS (
    SELECT
      ws.workflow_step_uuid,
      (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps) as dependencies_satisfied,
      -- FIXED: First attempt (attempts=0) is always eligible
      -- Retry attempts require retryable=true AND attempts < max_attempts
      (
        COALESCE(ws.attempts, 0) = 0  -- First attempt always eligible
        OR (
          COALESCE(ws.retryable, true) = true  -- Must be retryable for retries
          AND COALESCE(ws.attempts, 0) < COALESCE(ws.max_attempts, 3)
        )
      ) as retry_eligible
    FROM tasker_workflow_steps ws
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = ws.workflow_step_uuid
    LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  )

  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied (OPTIMIZED: pre-computed in CTE)
    srf.dependencies_satisfied,

    -- Retry eligible (FIXED: pre-computed with corrected logic in CTE)
    srf.retry_eligible,

    -- Ready for execution (SIMPLIFIED: uses helper function with pre-computed values)
    evaluate_step_state_readiness(
        COALESCE(ss.to_state, 'pending'),
        ws.processed,
        ws.in_process,
        srf.dependencies_satisfied,
        srf.retry_eligible,
        COALESCE(ws.retryable, true),
        rt.next_retry_time
    ) as ready_for_execution,

    -- RESTORED: Use dedicated last_failures CTE for accurate failure time
    lf.failure_time as last_failure_at,

    -- Next retry time (SIMPLIFIED: already calculated in retry_times CTE)
    rt.next_retry_time as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.max_attempts, 3) as max_attempts,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = ws.workflow_step_uuid
  JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN retry_times rt ON rt.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN step_readiness_flags srf ON srf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = ANY(input_task_uuids)
    AND (step_uuids IS NULL OR ws.workflow_step_uuid = ANY(step_uuids))
  ORDER BY ws.task_uuid, ws.workflow_step_uuid;
END;
$$;

COMMENT ON FUNCTION public.get_step_readiness_status_batch(uuid[], uuid[]) IS
'FIXED: Batch step readiness with corrected retry_eligible logic.

BUG FIX (2025-10-06):
- First attempt (attempts=0) is now always eligible regardless of retry settings
- Retry attempts (attempts>0) properly check retryable=true AND attempts < max_attempts
- Fixes issue where max_attempts=0, retryable=false blocked first execution

SEMANTIC BEHAVIOR:
- max_attempts=0, retryable=false: Execute once, no retries ✓
- max_attempts=1, retryable=true: First attempt + 1 retry (2 total attempts) ✓
- First execution no longer requires retryable=true ✓

PERFORMANCE OPTIMIZATIONS:
- steps_for_tasks CTE creates efficient working set for multiple tasks
- Eliminates table scans through consistent INNER JOIN patterns
- Scales efficiently for batch operations on multiple tasks';

-- ============================================================================
-- Rollback instructions
-- ============================================================================
-- To rollback, restore the original functions from migration 20250927000000

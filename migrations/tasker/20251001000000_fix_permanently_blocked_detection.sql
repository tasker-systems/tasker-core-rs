-- ============================================================================
-- Fix permanently_blocked_steps detection to properly identify non-retryable errors
-- ============================================================================
-- This migration fixes the get_task_execution_context function to correctly
-- identify steps that are in permanent error state (either exhausted retries
-- OR marked as not retryable by the worker).
--
-- Background:
-- When a worker encounters a permanent error (e.g., missing handler, configuration
-- error), it marks the step as non-retryable in the metadata. However, the
-- get_task_execution_context function was only checking if attempts >= max_attempts,
-- which didn't catch steps where max_attempts > 0 but the error is non-retryable.
--
-- This caused tasks to be marked as "waiting_for_dependencies" instead of
-- "blocked_by_failures", leading to confusing error messages.
-- ============================================================================

-- Update get_task_execution_context to properly detect permanently blocked steps
CREATE OR REPLACE FUNCTION public.get_task_execution_context(input_task_uuid uuid)
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
    completion_percentage numeric,
    health_status text,
    enqueued_steps bigint
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
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued_for_orchestration' THEN 1 END) as enqueued_for_orchestration_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      -- FIXED: Count permanently blocked steps - either exhausted retries OR marked as not retry_eligible
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.max_attempts OR sd.retry_eligible = false) THEN 1 END) as permanently_blocked_steps
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

    -- FIXED: Execution State Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0
           OR COALESCE(ast.enqueued_steps, 0) > 0
           OR COALESCE(ast.enqueued_for_orchestration_steps, 0) > 0 THEN 'processing'
      -- FIXED: Check permanently_blocked_steps which now includes retry_eligible = false
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- FIXED: Recommended Action Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      -- FIXED: Check permanently_blocked_steps which now includes retry_eligible = false
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'handle_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Progress Metrics
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    -- FIXED: Health Status Logic
    CASE
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      -- FIXED: Check permanently_blocked_steps which now includes retry_eligible = false
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      -- Waiting state for retry-eligible failures with backoff
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    -- Enqueued steps (must be last to match RETURNS TABLE declaration)
    COALESCE(ast.enqueued_steps, 0) + COALESCE(ast.enqueued_for_orchestration_steps, 0) as enqueued_steps

  FROM task_info ti
  CROSS JOIN aggregated_stats ast;
END;
$$;

-- ============================================================================
-- Rollback instructions
-- ============================================================================
-- To rollback, restore the original function from migration 20250810140000

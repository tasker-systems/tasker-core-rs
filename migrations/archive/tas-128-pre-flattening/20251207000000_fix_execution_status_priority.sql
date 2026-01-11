-- ============================================================================
-- Fix execution_status priority and synchronize batch function
-- ============================================================================
--
-- PRIMARY FIX: execution_status priority order
-- ────────────────────────────────────────────
-- Bug: When a task has BOTH permanently blocked steps AND ready steps, the SQL
-- function would return 'has_ready_steps' instead of 'blocked_by_failures'.
--
-- This caused flaky tests in scenarios where:
-- 1. A task has multiple parallel steps with no dependencies
-- 2. One step fails permanently (retryable=false)
-- 3. Another step fails but is retryable and becomes ready after backoff
-- 4. Task state machine correctly transitions to 'blocked_by_failures'
-- 5. BUT get_task_execution_context returns 'has_ready_steps' because it checks
--    ready_steps > 0 BEFORE checking permanently_blocked_steps > 0
--
-- Fix: Check permanently_blocked_steps BEFORE ready_steps. If ANY step is
-- permanently blocked, the task should be considered blocked regardless of
-- whether other steps are ready for execution.
--
-- This matches the semantic: "blocked by failures" means progress is blocked
-- even if some work is still possible on other branches.
--
-- SECONDARY FIXES: Batch function synchronization
-- ────────────────────────────────────────────────
-- The batch function (get_task_execution_contexts_batch) was NOT updated by
-- migration 20251001000000_fix_permanently_blocked_detection.sql and had
-- accumulated several bugs:
--
-- 1. WRONG recommended_action VALUES (Critical)
--    Old batch function had: 'process_ready_steps', 'review_failures', 'mark_complete'
--    Rust enum expects:      'execute_ready_steps', 'handle_failures', 'finalize_task'
--    These old values don't match RecommendedAction enum and caused silent fallbacks!
--
-- 2. MISSING retry_eligible CHECK
--    The 20251001 migration added `OR sd.retry_eligible = false` to the
--    permanently_blocked_steps calculation in the non-batch function, but
--    forgot to update the batch function.
--
-- 3. STRUCTURAL CHANGE (Performance neutral)
--    Changed from get_step_readiness_status_batch() to CROSS JOIN LATERAL
--    with get_step_readiness_status(). EXPLAIN ANALYZE shows ~equal performance.
--
-- See: docs/architecture-decisions/rca-parallel-execution-timing-bugs.md
-- ============================================================================

-- Update get_task_execution_context to check blocked_by_failures BEFORE ready_steps
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
      COUNT(CASE WHEN sd.current_state IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration') THEN 1 END) as enqueued_for_orchestration_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      -- Count permanently blocked steps - either exhausted retries OR marked as not retry_eligible
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

    -- FIXED: Execution State Logic - blocked_by_failures now takes priority over has_ready_steps
    -- If ANY step is permanently blocked, the task is blocked regardless of other ready steps
    CASE
      -- Check for permanent blocks FIRST - this takes priority
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked_by_failures'
      -- Then check for ready steps
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      -- Then check for processing
      WHEN COALESCE(ast.in_progress_steps, 0) > 0
           OR COALESCE(ast.enqueued_steps, 0) > 0
           OR COALESCE(ast.enqueued_for_orchestration_steps, 0) > 0 THEN 'processing'
      -- Check for completion
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      -- Default to waiting
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- FIXED: Recommended Action Logic - consistent with execution_status priority
    CASE
      -- If blocked, recommend handling failures
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'handle_failures'
      -- If ready steps, recommend executing them
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      -- If processing, wait
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      -- If complete, finalize
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      -- Default to waiting
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Progress Metrics
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    -- Health Status Logic - consistent with execution_status priority
    CASE
      -- If permanently blocked, health is blocked
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked'
      -- If no failures, healthy
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      -- If failures but also ready steps, recovering
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      -- If retry-eligible failures with backoff, recovering
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
-- BATCH VERSION: Synchronize with non-batch function
-- ============================================================================
-- This batch function had diverged from the non-batch version:
-- - Missing retry_eligible check in permanently_blocked_steps (from 20251001)
-- - Wrong recommended_action values (process_ready_steps → execute_ready_steps, etc.)
-- - Old priority order for execution_status
--
-- This update brings it in sync with the non-batch function.
-- ============================================================================
CREATE OR REPLACE FUNCTION public.get_task_execution_contexts_batch(input_task_uuids uuid[])
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
    SELECT srs.*
    FROM unnest(input_task_uuids) AS task_uuid_list(task_uuid)
    CROSS JOIN LATERAL get_step_readiness_status(task_uuid_list.task_uuid, NULL) srs
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
    WHERE t.task_uuid = ANY(input_task_uuids)
  ),
  aggregated_stats AS (
    SELECT
      sd.task_uuid,
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration') THEN 1 END) as enqueued_for_orchestration_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.max_attempts OR sd.retry_eligible = false) THEN 1 END) as permanently_blocked_steps
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

    -- FIXED: blocked_by_failures takes priority over has_ready_steps
    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0
           OR COALESCE(ast.enqueued_steps, 0) > 0
           OR COALESCE(ast.enqueued_for_orchestration_steps, 0) > 0 THEN 'processing'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'handle_failures'
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked'
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    COALESCE(ast.enqueued_steps, 0) + COALESCE(ast.enqueued_for_orchestration_steps, 0) as enqueued_steps

  FROM task_info ti
  LEFT JOIN aggregated_stats ast ON ast.task_uuid = ti.task_uuid;
END;
$$;

-- ============================================================================
-- Rollback instructions
-- ============================================================================
-- To rollback the non-batch function:
--   Restore from migration 20251001000000_fix_permanently_blocked_detection.sql
--
-- To rollback the batch function:
--   Restore from migration 20250810140000_uuid_v7_initial_schema.sql
--   WARNING: The original batch function had incorrect recommended_action values
--   that don't match the Rust RecommendedAction enum. Rolling back is NOT recommended.
--
-- Rollback is NOT recommended because:
-- 1. The batch function's old recommended_action values were incorrect
-- 2. The batch function was missing retry_eligible check
-- 3. The priority fix is semantically correct

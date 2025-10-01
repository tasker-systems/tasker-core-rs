-- Add WaitingForRetry state to workflow step state validation
-- This migration updates CHECK constraints to include the new 'waiting_for_retry' state
-- for proper error handling and retry logic

-- =============================================================================
-- UPDATE WORKFLOW STEP STATE VALIDATION
-- =============================================================================

-- Update CHECK constraint for to_state column to include waiting_for_retry
DO $$
BEGIN
    -- Drop existing constraint if it exists
    IF EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'chk_workflow_step_transitions_to_state'
        AND table_name = 'tasker_workflow_step_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_workflow_step_transitions
        DROP CONSTRAINT chk_workflow_step_transitions_to_state;

        RAISE NOTICE 'Dropped existing to_state constraint';
    END IF;

    -- Add updated constraint with waiting_for_retry state
    ALTER TABLE public.tasker_workflow_step_transitions
    ADD CONSTRAINT chk_workflow_step_transitions_to_state
    CHECK (to_state IN (
        'pending',
        'enqueued',
        'in_progress',
        'enqueued_for_orchestration',
        'enqueued_as_error_for_orchestration',
        'waiting_for_retry',
        'complete',
        'error',
        'cancelled',
        'resolved_manually'
    ));

    RAISE NOTICE 'Added updated to_state validation constraint with waiting_for_retry state';
END
$$;

-- Update CHECK constraint for from_state column to include waiting_for_retry
DO $$
BEGIN
    -- Drop existing constraint if it exists
    IF EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'chk_workflow_step_transitions_from_state'
        AND table_name = 'tasker_workflow_step_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_workflow_step_transitions
        DROP CONSTRAINT chk_workflow_step_transitions_from_state;

        RAISE NOTICE 'Dropped existing from_state constraint';
    END IF;

    -- Add updated constraint with waiting_for_retry state
    ALTER TABLE public.tasker_workflow_step_transitions
    ADD CONSTRAINT chk_workflow_step_transitions_from_state
    CHECK (from_state IS NULL OR from_state IN (
        'pending',
        'enqueued',
        'in_progress',
        'enqueued_for_orchestration',
        'enqueued_as_error_for_orchestration',
        'waiting_for_retry',
        'complete',
        'error',
        'cancelled',
        'resolved_manually'
    ));

    RAISE NOTICE 'Added updated from_state validation constraint with waiting_for_retry state';
END
$$;

-- =============================================================================
-- HELPER FUNCTIONS FOR STEP READINESS LOGIC
-- =============================================================================

-- Calculate the next retry time for a step based on backoff configuration
-- This function encapsulates all backoff calculation logic in one place
CREATE OR REPLACE FUNCTION calculate_step_next_retry_time(
    backoff_request_seconds INTEGER,
    last_attempted_at TIMESTAMP,
    failure_time TIMESTAMP,
    attempts INTEGER
) RETURNS TIMESTAMP
LANGUAGE SQL STABLE AS $$
    SELECT CASE
        WHEN backoff_request_seconds IS NOT NULL AND last_attempted_at IS NOT NULL THEN
            last_attempted_at + (backoff_request_seconds * interval '1 second')
        WHEN failure_time IS NOT NULL THEN
            failure_time + (LEAST(power(2, COALESCE(attempts, 1)) * interval '1 second', interval '30 seconds'))
        ELSE NULL
    END
$$;

-- Evaluate if a step is ready for execution based on its state and conditions
-- This function provides consistent readiness logic for all ready-eligible states
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
        AND retryable
        AND (next_retry_time IS NULL OR next_retry_time <= NOW())
$$;

-- Optimize step readiness SQL functions with task-scoped CTEs and performance improvements
-- This migration rebuilds the readiness functions to eliminate table scans while preserving
-- all business logic including waiting_for_retry support and error state exclusion.

-- =============================================================================
-- DELEGATED get_step_readiness_status FUNCTION (calls batch function)
-- =============================================================================

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
  -- DELEGATION: Use batch function with single-element array for DRY principle
  RETURN QUERY
  SELECT * FROM get_step_readiness_status_batch(ARRAY[input_task_uuid], step_uuids);
END;
$$;

COMMENT ON FUNCTION public.get_step_readiness_status(uuid, uuid[]) IS
'Single-task step readiness analysis function (delegates to batch function).
This function provides a convenient interface for single-task analysis while
leveraging the optimized batch implementation to eliminate code duplication.
All complex business logic resides in get_step_readiness_status_batch.';

-- =============================================================================
-- OPTIMIZED get_step_readiness_status_batch FUNCTION
-- =============================================================================

-- Drop old version with different signature
DROP FUNCTION IF EXISTS public.get_step_readiness_status_batch(uuid[]);

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
    retry_limit integer,
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

  -- NEW: Pre-computed boolean flags to eliminate duplication
  step_readiness_flags AS (
    SELECT
      ws.workflow_step_uuid,
      (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps) as dependencies_satisfied,
      (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3)) as retry_eligible
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

    -- Retry eligible (OPTIMIZED: pre-computed in CTE)
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
    COALESCE(ws.retry_limit, 3) as retry_limit,
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

-- =============================================================================
-- FUNCTION DOCUMENTATION AND PERFORMANCE NOTES
-- =============================================================================

COMMENT ON FUNCTION public.get_step_readiness_status(uuid, uuid[]) IS
'Optimized step readiness analysis function with task-scoped CTEs for performance.

PERFORMANCE OPTIMIZATIONS:
- task_steps CTE eliminates table scans by creating focused working set
- All subsequent CTEs use INNER JOINs to task_steps for efficient filtering
- Restored last_failures CTE for accurate failure time tracking
- dependency_counts uses INNER JOIN instead of inefficient subselect

BUSINESS LOGIC:
- Excludes error state steps from being ready (prevents invalid re-enqueuing)
- Supports waiting_for_retry state with proper backoff expiration logic
- Maintains accurate failure time tracking and retry calculations
- Preserves all dependency satisfaction and retry eligibility logic';

COMMENT ON FUNCTION public.get_step_readiness_status_batch(uuid[], uuid[]) IS
'Optimized batch step readiness analysis function with multi-task scoped CTEs.

PERFORMANCE OPTIMIZATIONS:
- steps_for_tasks CTE creates efficient working set for multiple tasks
- Eliminates table scans through consistent INNER JOIN patterns
- Scales efficiently for batch operations on multiple tasks
- Maintains same optimization patterns as single-task version

BUSINESS LOGIC:
- Identical logic to single-task version for consistency
- Supports all error handling and retry states
- Efficient for orchestration batch operations';

-- =============================================================================
-- PERFORMANCE VALIDATION QUERIES (for testing/monitoring)
-- =============================================================================

-- Query to verify that task-scoped CTEs are being used efficiently
-- (Should show index usage on task_uuid and minimal rows processed)

-- Example validation query (commented out for production):
/*
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM get_step_readiness_status('01234567-89ab-cdef-0123-456789abcdef'::uuid);
*/

-- Query to compare performance between optimized and legacy versions
-- (Use for benchmarking after migration)

-- Example benchmark query (commented out for production):
/*
SELECT
  'optimized' as version,
  COUNT(*) as steps_analyzed,
  pg_stat_statements.mean_time as avg_execution_time_ms
FROM get_step_readiness_status('01234567-89ab-cdef-0123-456789abcdef'::uuid)
CROSS JOIN pg_stat_statements
WHERE pg_stat_statements.query LIKE '%get_step_readiness_status%';
*/

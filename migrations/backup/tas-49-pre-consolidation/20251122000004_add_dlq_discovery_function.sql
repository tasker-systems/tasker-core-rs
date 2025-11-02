-- ============================================================================
-- TAS-49 Phase 2: DLQ Discovery Function
-- ============================================================================
-- Migration: 20251122000004_add_dlq_discovery_function.sql
-- Description: Discovery function for stale task detection with O(1) optimization
-- Dependencies: 20251122000002_add_dlq_helper_functions.sql,
--               20251122000003_add_dlq_views.sql (for v_task_state_analysis)
--
-- Function:
-- get_stale_tasks_for_dlq() - Discovers stale tasks using base view query
--
-- Performance Optimization:
-- This function queries v_task_state_analysis ONCE, performing all expensive
-- joins outside any loop. Main detection function calls this ONCE and then
-- iterates over results, achieving O(1) expensive operations instead of O(n).
-- ============================================================================

-- ============================================================================
-- Function: Get Stale Tasks for DLQ
-- ============================================================================
-- Discovery function that identifies tasks exceeding staleness thresholds.
-- Uses v_task_state_analysis base view for efficient multi-table joins.
--
-- Key Performance Feature:
-- - Queries base view ONCE (all expensive joins materialized in single query)
-- - Main function calls this ONCE before loop, not inside loop initialization
-- - Achieves O(1) expensive operations instead of O(n) per-iteration queries
--
-- Anti-Join Optimization:
-- Uses LEFT JOIN + IS NULL pattern instead of NOT EXISTS subquery for better
-- query planner optimization. Excludes tasks already in DLQ with pending status.
--
-- Parameters:
--   p_default_waiting_deps    - Default threshold for waiting_for_dependencies (minutes)
--   p_default_waiting_retry   - Default threshold for waiting_for_retry (minutes)
--   p_default_steps_process   - Default threshold for steps_in_process (minutes)
--   p_max_lifetime_hours      - Maximum task lifetime regardless of state (hours)
--   p_batch_size              - Maximum number of stale tasks to return
--
-- Returns: TABLE with columns:
--   - task_uuid: UUID of stale task
--   - namespace_name: Namespace for context
--   - task_name: Template name for context
--   - current_state: State when detected as stale
--   - time_in_state_minutes: How long in current state
--   - threshold_minutes: Threshold that was exceeded
--   - task_age_minutes: Total task age (for lifetime check)

CREATE OR REPLACE FUNCTION get_stale_tasks_for_dlq(
    p_default_waiting_deps INTEGER DEFAULT 60,
    p_default_waiting_retry INTEGER DEFAULT 30,
    p_default_steps_process INTEGER DEFAULT 30,
    p_max_lifetime_hours INTEGER DEFAULT 72,
    p_batch_size INTEGER DEFAULT 100
) RETURNS TABLE (
    task_uuid UUID,
    namespace_name VARCHAR,
    task_name VARCHAR,
    current_state VARCHAR,
    time_in_state_minutes NUMERIC,
    threshold_minutes INTEGER,
    task_age_minutes NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        tsa.task_uuid,
        tsa.namespace_name,
        tsa.task_name,
        tsa.current_state,
        tsa.minutes_in_state as time_in_state_minutes,
        -- Recalculate threshold with provided defaults (may differ from view defaults)
        calculate_staleness_threshold(
            tsa.current_state,
            tsa.template_config,
            p_default_waiting_deps,
            p_default_waiting_retry,
            p_default_steps_process
        ) as threshold_minutes,
        tsa.task_age_minutes
    FROM v_task_state_analysis tsa
    -- Anti-join optimization: exclude tasks already in DLQ with pending status
    -- Uses LEFT JOIN + IS NULL instead of NOT EXISTS for better query planner optimization
    LEFT JOIN tasker_tasks_dlq dlq
        ON dlq.task_uuid = tsa.task_uuid
        AND dlq.resolution_status = 'pending'
    WHERE
        -- Task not already in DLQ (anti-join condition)
        dlq.dlq_entry_uuid IS NULL
        -- State-specific staleness threshold exceeded
        AND (
            tsa.minutes_in_state > calculate_staleness_threshold(
                tsa.current_state,
                tsa.template_config,
                p_default_waiting_deps,
                p_default_waiting_retry,
                p_default_steps_process
            )
            -- OR maximum lifetime exceeded regardless of state
            OR tsa.task_age_minutes > (p_max_lifetime_hours * 60)
        )
    -- Prioritize by time in state (oldest first)
    ORDER BY tsa.minutes_in_state DESC
    -- Limit to batch size for controlled processing
    LIMIT p_batch_size;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION get_stale_tasks_for_dlq IS
'TAS-49 Phase 3: Discovery function for stale task detection.

Performance Optimization (O(1) vs O(n)):
- Queries v_task_state_analysis base view ONCE
- All expensive joins (tasks, transitions, named_tasks, namespaces) happen in single query
- Main detection function calls this ONCE before loop, not inside loop initialization
- Result: O(1) expensive operations + O(n) cheap processing instead of O(n) expensive operations

Anti-Join Optimization:
- Uses LEFT JOIN + IS NULL instead of NOT EXISTS subquery
- Better query planner optimization for large tables
- Leverages idx_dlq_unique_pending_task partial index

Staleness Detection Logic:
1. Check state-specific threshold from template configuration
2. Fall back to provided default thresholds
3. Check maximum lifetime threshold (any state)
4. Exclude tasks already in DLQ with pending status

Usage:
-- Called by detect_and_transition_stale_tasks() BEFORE loop:
FOR stale_task IN (SELECT * FROM get_stale_tasks_for_dlq(60, 30, 30, 72, 100))
LOOP
    -- Process stale_task (cheap operations only)
END LOOP;

Parameters:
- p_default_waiting_deps: Default for waiting_for_dependencies state (minutes)
- p_default_waiting_retry: Default for waiting_for_retry state (minutes)
- p_default_steps_process: Default for steps_in_process state (minutes)
- p_max_lifetime_hours: Maximum task age regardless of state (hours)
- p_batch_size: Limit results for controlled processing

Returns: TABLE of stale tasks with:
- task_uuid: Task identifier
- namespace_name: Namespace for context
- task_name: Template name for context
- current_state: State when detected as stale
- time_in_state_minutes: How long in current state
- threshold_minutes: Threshold that was exceeded
- task_age_minutes: Total task age for lifetime check

Example:
SELECT * FROM get_stale_tasks_for_dlq(
    60,   -- waiting_for_dependencies default
    30,   -- waiting_for_retry default
    30,   -- steps_in_process default
    72,   -- max lifetime hours
    100   -- batch size
);';

-- ============================================================================
-- Migration Validation
-- ============================================================================

DO $$
BEGIN
    -- Verify discovery function created
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc
        WHERE proname = 'get_stale_tasks_for_dlq'
    ) THEN
        RAISE EXCEPTION 'get_stale_tasks_for_dlq function not created';
    END IF;

    -- Verify function returns expected columns
    PERFORM *
    FROM get_stale_tasks_for_dlq(60, 30, 30, 72, 1)
    LIMIT 0;

    RAISE NOTICE 'TAS-49 Phase 3: DLQ discovery function created successfully';
END $$;

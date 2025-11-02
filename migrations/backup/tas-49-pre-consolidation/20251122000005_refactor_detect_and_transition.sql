-- ============================================================================
-- TAS-49 Phase 2: Refactor Main Detection Function
-- ============================================================================
-- Migration: 20251122000005_refactor_detect_and_transition.sql
-- Description: Refactor detect_and_transition_stale_tasks() to use helper functions
-- Dependencies: 20251122000002_add_dlq_helper_functions.sql,
--               20251122000004_add_dlq_discovery_function.sql
--
-- Changes:
-- 1. Replace inline CTE (lines 42-113) with get_stale_tasks_for_dlq() call
-- 2. Replace DLQ INSERT (lines 134-152) with create_dlq_entry() call
-- 3. Replace state transition (lines 162-199) with transition_stale_task_to_error() call
--
-- Result: 220 lines → ~90 lines of clean orchestration logic
-- ============================================================================

-- ============================================================================
-- Function: Detect and Transition Stale Tasks (Refactored)
-- ============================================================================

CREATE OR REPLACE FUNCTION detect_and_transition_stale_tasks(
    p_dry_run BOOLEAN DEFAULT true,
    p_batch_size INTEGER DEFAULT 100,
    p_default_waiting_deps_threshold INTEGER DEFAULT 60,
    p_default_waiting_retry_threshold INTEGER DEFAULT 30,
    p_default_steps_in_process_threshold INTEGER DEFAULT 30,
    p_default_task_max_lifetime_hours INTEGER DEFAULT 24
)
RETURNS TABLE(
    task_uuid UUID,
    namespace_name VARCHAR,
    task_name VARCHAR,
    current_state VARCHAR,
    time_in_state_minutes INTEGER,
    staleness_threshold_minutes INTEGER,
    action_taken VARCHAR,
    moved_to_dlq BOOLEAN,
    transition_success BOOLEAN
) AS $$
DECLARE
    v_stale_task RECORD;
    v_dlq_entry_uuid UUID;
    v_transition_success BOOLEAN;
    v_action_taken VARCHAR;
BEGIN
    -- Discovery: Query stale tasks ONCE using optimized function
    -- O(1) expensive joins instead of O(n) inline CTE per iteration
    FOR v_stale_task IN (
        SELECT *
        FROM get_stale_tasks_for_dlq(
            p_default_waiting_deps_threshold,
            p_default_waiting_retry_threshold,
            p_default_steps_in_process_threshold,
            p_default_task_max_lifetime_hours,
            p_batch_size
        )
    )
    LOOP
        -- Initialize result tracking
        v_dlq_entry_uuid := NULL;
        v_transition_success := false;

        IF p_dry_run THEN
            -- Dry run: just report what would happen
            v_action_taken := 'would_transition_to_dlq_and_error';

        ELSE
            -- Real execution: create DLQ entry and transition state
            BEGIN
                -- Step 1: Create DLQ entry using helper function
                v_dlq_entry_uuid := create_dlq_entry(
                    v_stale_task.task_uuid,
                    v_stale_task.namespace_name,
                    v_stale_task.task_name,
                    v_stale_task.current_state,
                    v_stale_task.time_in_state_minutes::INTEGER,
                    v_stale_task.threshold_minutes,
                    'staleness_timeout'
                );

            EXCEPTION
                WHEN unique_violation THEN
                    -- Already has pending DLQ entry (race condition), skip this task
                    CONTINUE;
            END;

            -- Step 2: Transition task to error state using helper function
            v_transition_success := transition_stale_task_to_error(
                v_stale_task.task_uuid,
                v_stale_task.current_state,
                v_stale_task.namespace_name,
                v_stale_task.task_name
            );

            -- Determine action taken based on results
            v_action_taken := CASE
                WHEN v_dlq_entry_uuid IS NOT NULL AND v_transition_success THEN
                    'transitioned_to_dlq_and_error'
                WHEN v_dlq_entry_uuid IS NOT NULL THEN
                    'moved_to_dlq_only'
                WHEN v_transition_success THEN
                    'transitioned_to_error_only'
                ELSE
                    'transition_failed'
            END;
        END IF;

        -- Return result for this task
        RETURN QUERY SELECT
            v_stale_task.task_uuid,
            v_stale_task.namespace_name,
            v_stale_task.task_name,
            v_stale_task.current_state,
            v_stale_task.time_in_state_minutes::INTEGER,
            v_stale_task.threshold_minutes,
            v_action_taken,
            (v_dlq_entry_uuid IS NOT NULL)::BOOLEAN as moved_to_dlq,
            v_transition_success;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION detect_and_transition_stale_tasks IS
'TAS-49 Phase 2 (Refactored): Detect stale tasks and create DLQ investigation entries.

Refactoring Summary:
- 220 lines → ~90 lines (58% reduction)
- Inline CTE replaced with get_stale_tasks_for_dlq() function call
- Inline DLQ INSERT replaced with create_dlq_entry() function call
- Inline state transition replaced with transition_stale_task_to_error() function call

Performance Optimization:
- O(1) expensive operations: get_stale_tasks_for_dlq() queries base view ONCE
- O(n) cheap operations: Loop processes results using simple helper function calls
- Previous: O(n) expensive multi-table joins inside loop initialization

Function Decomposition Benefits:
- Single responsibility: Main function orchestrates, helpers implement details
- Testability: Each helper function independently testable
- Maintainability: Changes to DLQ entry format only affect create_dlq_entry()
- Reusability: Helper functions usable by other operations

This function does NOT resolve tasks - it only:
1. Identifies tasks stuck beyond thresholds (via get_stale_tasks_for_dlq)
2. Creates DLQ entries for operator investigation (via create_dlq_entry)
3. Transitions them to Error state (via transition_stale_task_to_error)

Operators investigate DLQ entries and manually resolve/retry/cancel as appropriate.

Parameters:
- p_dry_run: If true, only reports what would happen (default: true)
- p_batch_size: Maximum tasks to process per run (default: 100)
- p_default_waiting_deps_threshold: Default minutes for waiting_for_dependencies (default: 60)
- p_default_waiting_retry_threshold: Default minutes for waiting_for_retry (default: 30)
- p_default_steps_in_process_threshold: Default minutes for steps_in_process (default: 30)
- p_default_task_max_lifetime_hours: Maximum task lifetime regardless of state (default: 24)

Returns: TABLE with columns:
- task_uuid: Task that was detected as stale
- namespace_name: Namespace for context
- task_name: Template name for context
- current_state: State when detected
- time_in_state_minutes: How long in state
- staleness_threshold_minutes: Threshold that was exceeded
- action_taken: What happened (enum: would_transition_to_dlq_and_error, transitioned_to_dlq_and_error,
                moved_to_dlq_only, transitioned_to_error_only, transition_failed)
- moved_to_dlq: Whether DLQ entry was created
- transition_success: Whether state transition succeeded

Example:
-- Dry run (safe, no changes)
SELECT * FROM detect_and_transition_stale_tasks(true, 10);

-- Real execution (creates DLQ entries and transitions)
SELECT * FROM detect_and_transition_stale_tasks(false, 100, 60, 30, 30, 72);';

-- ============================================================================
-- Migration Validation
-- ============================================================================

DO $$
BEGIN
    -- Verify function was replaced
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc
        WHERE proname = 'detect_and_transition_stale_tasks'
    ) THEN
        RAISE EXCEPTION 'detect_and_transition_stale_tasks function not found';
    END IF;

    -- Verify function is callable (dry run mode, safe)
    PERFORM * FROM detect_and_transition_stale_tasks(true, 1);

    RAISE NOTICE 'TAS-49 Phase 4: Main detection function refactored successfully';
END $$;

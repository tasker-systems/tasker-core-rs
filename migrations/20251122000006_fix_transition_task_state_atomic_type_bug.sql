-- Fix type mismatch bug in transition_task_state_atomic (TAS-49 Phase 5)
--
-- The original TAS-41 function had v_transitioned declared as BOOLEAN
-- but assigned ROW_COUNT (INTEGER) to it, causing "operator does not exist: boolean > integer"
--
-- This fix will be incorporated into the consolidated DLQ migration.

CREATE OR REPLACE FUNCTION transition_task_state_atomic(
    p_task_uuid UUID,
    p_from_state VARCHAR,
    p_to_state VARCHAR,
    p_processor_uuid UUID,
    p_metadata JSONB DEFAULT '{}'
) RETURNS BOOLEAN AS $$
DECLARE
    v_sort_key INTEGER;
    v_row_count INTEGER;  -- FIX: Changed from BOOLEAN to INTEGER
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

    GET DIAGNOSTICS v_row_count = ROW_COUNT;
    RETURN v_row_count > 0;  -- FIX: Now comparing INTEGER > INTEGER
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION transition_task_state_atomic IS
'TAS-41/TAS-49: Atomic state transition with type bug fix.

Fixed in TAS-49 Phase 5: v_transitioned changed from BOOLEAN to INTEGER
to correctly capture ROW_COUNT before comparison.

This function will be incorporated into consolidated DLQ migration.';

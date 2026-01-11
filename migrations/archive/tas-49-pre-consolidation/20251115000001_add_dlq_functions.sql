-- ============================================================================
-- TAS-49 Phase 1.3: DLQ SQL Functions
-- ============================================================================
-- Migration: 20251115000001_add_dlq_functions.sql
-- Description: Core DLQ detection functions
-- Dependencies: 20251115000000_add_dlq_tables.sql
--
-- Functions:
-- 1. detect_and_transition_stale_tasks() - Automatic staleness detection
-- ============================================================================

-- ============================================================================
-- Function: Detect and Transition Stale Tasks to DLQ
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
    v_task_record RECORD;
    v_dlq_inserted BOOLEAN;
    v_transition_completed BOOLEAN;
    v_task_snapshot JSONB;
BEGIN
    -- Find stale tasks that exceed thresholds
    FOR v_task_record IN
        SELECT
            t.task_uuid,
            tns.name as namespace_name,
            nt.name as task_name,
            tt.to_state as current_state,
            EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as time_in_state_minutes,
            EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 as task_age_minutes,
            nt.configuration as template_config,
            -- Determine threshold based on state and template config
            -- Per-template lifecycle config takes precedence over defaults
            CASE tt.to_state
                WHEN 'waiting_for_dependencies' THEN
                    COALESCE(
                        (nt.configuration->'lifecycle'->>'max_waiting_for_dependencies_minutes')::INTEGER,
                        p_default_waiting_deps_threshold
                    )
                WHEN 'waiting_for_retry' THEN
                    COALESCE(
                        (nt.configuration->'lifecycle'->>'max_waiting_for_retry_minutes')::INTEGER,
                        p_default_waiting_retry_threshold
                    )
                WHEN 'steps_in_process' THEN
                    COALESCE(
                        (nt.configuration->'lifecycle'->>'max_steps_in_process_minutes')::INTEGER,
                        p_default_steps_in_process_threshold
                    )
                ELSE 1440  -- 24 hours default for other states
            END as threshold_minutes,
            -- Overall task max lifetime check
            COALESCE(
                (nt.configuration->'lifecycle'->>'max_duration_minutes')::INTEGER,
                p_default_task_max_lifetime_hours * 60
            ) as task_lifetime_threshold_minutes
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
        WHERE tt.most_recent = true
          -- Only non-terminal states
          AND tt.to_state NOT IN ('complete', 'error', 'cancelled', 'resolved_manually')
          -- Task is stale in current state OR exceeded max lifetime
          AND (
              EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 > CASE tt.to_state
                  WHEN 'waiting_for_dependencies' THEN COALESCE(
                      (nt.configuration->'lifecycle'->>'max_waiting_for_dependencies_minutes')::INTEGER,
                      p_default_waiting_deps_threshold
                  )
                  WHEN 'waiting_for_retry' THEN COALESCE(
                      (nt.configuration->'lifecycle'->>'max_waiting_for_retry_minutes')::INTEGER,
                      p_default_waiting_retry_threshold
                  )
                  WHEN 'steps_in_process' THEN COALESCE(
                      (nt.configuration->'lifecycle'->>'max_steps_in_process_minutes')::INTEGER,
                      p_default_steps_in_process_threshold
                  )
                  ELSE 1440
              END
              OR
              EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 > COALESCE(
                  (nt.configuration->'lifecycle'->>'max_duration_minutes')::INTEGER,
                  p_default_task_max_lifetime_hours * 60
              )
          )
          -- Skip tasks already in DLQ with pending investigation
          AND NOT EXISTS (
              SELECT 1 FROM tasker_tasks_dlq dlq
              WHERE dlq.task_uuid = t.task_uuid
                AND dlq.resolution_status = 'pending'
          )
        ORDER BY tt.created_at ASC  -- Oldest stale tasks first
        LIMIT p_batch_size
    LOOP
        v_dlq_inserted := false;
        v_transition_completed := false;

        -- Build task snapshot for investigation
        SELECT jsonb_build_object(
            'task_uuid', v_task_record.task_uuid,
            'namespace', v_task_record.namespace_name,
            'task_name', v_task_record.task_name,
            'current_state', v_task_record.current_state,
            'time_in_state_minutes', v_task_record.time_in_state_minutes,
            'threshold_minutes', v_task_record.threshold_minutes,
            'task_age_minutes', v_task_record.task_age_minutes,
            'template_config', v_task_record.template_config,
            'detection_time', NOW()
        ) INTO v_task_snapshot;

        IF NOT p_dry_run THEN
            -- Insert DLQ entry
            BEGIN
                INSERT INTO tasker_tasks_dlq (
                    task_uuid,
                    original_state,
                    dlq_reason,
                    dlq_timestamp,
                    task_snapshot,
                    metadata
                ) VALUES (
                    v_task_record.task_uuid,
                    v_task_record.current_state,
                    'staleness_timeout',
                    NOW(),
                    v_task_snapshot,
                    jsonb_build_object(
                        'detection_method', 'automatic_staleness_detection',
                        'time_in_state_minutes', v_task_record.time_in_state_minutes,
                        'threshold_minutes', v_task_record.threshold_minutes
                    )
                );

                v_dlq_inserted := true;
            EXCEPTION
                WHEN unique_violation THEN
                    -- Already has pending DLQ entry (race condition), skip
                    CONTINUE;
            END;

            -- Transition task to error state using TAS-41 atomic transition
            BEGIN
                -- Mark old transition as not most_recent
                UPDATE tasker_task_transitions
                SET most_recent = false
                WHERE task_uuid = v_task_record.task_uuid
                  AND most_recent = true;

                -- Insert new error transition
                INSERT INTO tasker_task_transitions (
                    task_uuid,
                    from_state,
                    to_state,
                    most_recent,
                    created_at,
                    reason,
                    transition_metadata
                ) VALUES (
                    v_task_record.task_uuid,
                    v_task_record.current_state,
                    'error',
                    true,
                    NOW(),
                    'staleness_timeout',
                    jsonb_build_object(
                        'time_in_state_minutes', v_task_record.time_in_state_minutes,
                        'threshold_minutes', v_task_record.threshold_minutes,
                        'automatic_transition', true,
                        'dlq_entry_created', v_dlq_inserted
                    )
                );

                v_transition_completed := true;
            EXCEPTION
                WHEN OTHERS THEN
                    -- Log error but continue processing other tasks
                    RAISE WARNING 'Failed to transition task % to error state: %',
                        v_task_record.task_uuid, SQLERRM;
            END;
        END IF;

        -- Return result for this task
        RETURN QUERY SELECT
            v_task_record.task_uuid,
            v_task_record.namespace_name,
            v_task_record.task_name,
            v_task_record.current_state,
            v_task_record.time_in_state_minutes::INTEGER,
            v_task_record.threshold_minutes::INTEGER,
            (CASE
                WHEN p_dry_run THEN 'would_transition_to_dlq_and_error'
                WHEN v_dlq_inserted AND v_transition_completed THEN 'transitioned_to_dlq_and_error'
                WHEN v_dlq_inserted THEN 'moved_to_dlq_only'
                WHEN v_transition_completed THEN 'transitioned_to_error_only'
                ELSE 'transition_failed'
            END)::VARCHAR,
            v_dlq_inserted,
            v_transition_completed;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION detect_and_transition_stale_tasks IS
'TAS-49: Detect stale tasks and create DLQ investigation entries.

This function does NOT resolve tasks - it only:
1. Identifies tasks stuck beyond thresholds
2. Transitions them to Error state (TAS-41 state machine)
3. Creates DLQ entries for operator investigation

Per-Template Configuration:
- Reads lifecycle config from nt.configuration->>''lifecycle''
- Falls back to default thresholds if template doesn''t specify
- Precedence: template config > default parameters

Resolution workflow:
1. Operator reviews DLQ entry via API (GET /api/v1/dlq/{task_uuid})
2. Operator uses existing step APIs to fix problem steps (PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid})
3. Task state machine automatically progresses when steps fixed
4. Operator updates DLQ entry to track resolution (PATCH /api/v1/dlq/{entry_uuid})

Parameters:
- p_dry_run: If true, only returns what would be done without making changes
- p_batch_size: Maximum number of stale tasks to process in single call
- p_default_*_threshold: Default thresholds when template doesn''t specify

Returns:
- task_uuid: Task that was (or would be) transitioned
- namespace_name, task_name: For identification
- current_state: State before transition
- time_in_state_minutes: How long task was stuck
- staleness_threshold_minutes: Threshold that was exceeded
- action_taken: What was done (or would be done)
- moved_to_dlq, transition_success: Operation results';

-- ============================================================================
-- Migration Validation
-- ============================================================================

DO $$
BEGIN
    -- Verify functions created
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc WHERE proname = 'detect_and_transition_stale_tasks'
    ) THEN
        RAISE EXCEPTION 'detect_and_transition_stale_tasks function not created';
    END IF;

    RAISE NOTICE 'TAS-49 Phase 1.3: DLQ SQL functions created successfully';
END $$;

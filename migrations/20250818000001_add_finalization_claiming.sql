-- Migration: 20250818000001_add_finalization_claiming.sql
-- TAS-37: Add finalization claiming functions to prevent race conditions

-- Create the main finalization claiming function
CREATE OR REPLACE FUNCTION public.claim_task_for_finalization(
    p_task_uuid uuid,
    p_processor_id character varying,
    p_timeout_seconds integer DEFAULT 30
)
RETURNS TABLE(
    claimed boolean,
    already_claimed_by character varying,
    task_state character varying,
    message character varying,
    finalization_attempt_uuid uuid
)
LANGUAGE plpgsql
AS $$
DECLARE
    v_current_claimed_by character varying;
    v_current_claimed_at timestamp;
    v_task_complete boolean;
    v_task_exists boolean;
    v_has_completed_steps boolean;
    v_execution_status character varying;
    v_attempt_uuid uuid;
BEGIN
    -- First check if task exists and get its current state
    SELECT
        (t.task_uuid IS NOT NULL),
        t.complete,
        t.claimed_by,
        t.claimed_at,
        tec.execution_status,
        EXISTS(
            SELECT 1 FROM tasker_workflow_steps ws
            WHERE ws.task_uuid = t.task_uuid
            AND ws.processed = true
        )
    INTO
        v_task_exists,
        v_task_complete,
        v_current_claimed_by,
        v_current_claimed_at,
        v_execution_status,
        v_has_completed_steps
    FROM tasker_tasks t
    LEFT JOIN LATERAL (
        SELECT execution_status
        FROM get_task_execution_context(t.task_uuid)
    ) tec ON true
    WHERE t.task_uuid = p_task_uuid;

    -- Task doesn't exist
    IF NOT v_task_exists THEN
        -- Record the attempt
        INSERT INTO tasker_finalization_attempts (
            task_uuid, processor_id, claimed, already_claimed_by, task_state, message
        ) VALUES (
            p_task_uuid, p_processor_id, false, NULL, 'not_found', 'Task not found'
        ) RETURNING tasker_finalization_attempts.finalization_attempt_uuid INTO v_attempt_uuid;
        
        RETURN QUERY SELECT
            false,
            NULL::character varying,
            'not_found'::character varying,
            'Task not found'::character varying,
            v_attempt_uuid;
        RETURN;
    END IF;

    -- Task already complete
    IF v_task_complete THEN
        -- Record the attempt
        INSERT INTO tasker_finalization_attempts (
            task_uuid, processor_id, claimed, already_claimed_by, task_state, message
        ) VALUES (
            p_task_uuid, p_processor_id, false, NULL, 'complete', 'Task already complete'
        ) RETURNING tasker_finalization_attempts.finalization_attempt_uuid INTO v_attempt_uuid;
        
        RETURN QUERY SELECT
            false,
            NULL::character varying,
            'complete'::character varying,
            'Task already complete'::character varying,
            v_attempt_uuid;
        RETURN;
    END IF;

    -- Task doesn't need finalization yet (no completed/failed steps)
    IF NOT v_has_completed_steps THEN
        -- Record the attempt
        INSERT INTO tasker_finalization_attempts (
            task_uuid, processor_id, claimed, already_claimed_by, task_state, message
        ) VALUES (
            p_task_uuid, p_processor_id, false, NULL, v_execution_status, 'Task has no completed or failed steps'
        ) RETURNING tasker_finalization_attempts.finalization_attempt_uuid INTO v_attempt_uuid;
        
        RETURN QUERY SELECT
            false,
            NULL::character varying,
            v_execution_status,
            'Task has no completed or failed steps'::character varying,
            v_attempt_uuid;
        RETURN;
    END IF;

    -- Check if already claimed and claim still valid
    IF v_current_claimed_by IS NOT NULL AND
       v_current_claimed_at > (NOW() - (p_timeout_seconds || ' seconds')::interval) THEN
        -- Already claimed by someone else
        IF v_current_claimed_by != p_processor_id THEN
            -- Record the attempt
            INSERT INTO tasker_finalization_attempts (
                task_uuid, processor_id, claimed, already_claimed_by, task_state, message
            ) VALUES (
                p_task_uuid, p_processor_id, false, v_current_claimed_by, v_execution_status, 'Task already claimed for finalization'
            ) RETURNING tasker_finalization_attempts.finalization_attempt_uuid INTO v_attempt_uuid;
            
            RETURN QUERY SELECT
                false,
                v_current_claimed_by,
                v_execution_status,
                'Task already claimed for finalization'::character varying,
                v_attempt_uuid;
            RETURN;
        ELSE
            -- We already have the claim - extend it
            UPDATE tasker_tasks
            SET claimed_at = NOW(),
                updated_at = NOW()
            WHERE task_uuid = p_task_uuid
            AND claimed_by = p_processor_id;

            -- Record the attempt
            INSERT INTO tasker_finalization_attempts (
                task_uuid, processor_id, claimed, already_claimed_by, task_state, message
            ) VALUES (
                p_task_uuid, p_processor_id, true, p_processor_id, v_execution_status, 'Claim extended'
            ) RETURNING tasker_finalization_attempts.finalization_attempt_uuid INTO v_attempt_uuid;

            RETURN QUERY SELECT
                true,
                p_processor_id,
                v_execution_status,
                'Claim extended'::character varying,
                v_attempt_uuid;
            RETURN;
        END IF;
    END IF;

    -- Try to claim the task atomically
    UPDATE tasker_tasks
    SET claimed_by = p_processor_id,
        claimed_at = NOW(),
        claim_timeout_seconds = p_timeout_seconds,
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND (claimed_by IS NULL
             OR claimed_at < (NOW() - (claim_timeout_seconds || ' seconds')::interval)
             OR claimed_by = p_processor_id)
        AND complete = false;

    -- Check if the update succeeded
    IF FOUND THEN
        -- Successfully claimed - record the attempt
        INSERT INTO tasker_finalization_attempts (
            task_uuid, processor_id, claimed, already_claimed_by, task_state, message
        ) VALUES (
            p_task_uuid, p_processor_id, true, p_processor_id, v_execution_status, 'Successfully claimed for finalization'
        ) RETURNING tasker_finalization_attempts.finalization_attempt_uuid INTO v_attempt_uuid;
        
        RETURN QUERY SELECT
            true,
            p_processor_id,
            v_execution_status,
            'Successfully claimed for finalization'::character varying,
            v_attempt_uuid;
        RETURN;
    ELSE
        -- Failed to claim - record the attempt
        INSERT INTO tasker_finalization_attempts (
            task_uuid, processor_id, claimed, already_claimed_by, task_state, message
        ) VALUES (
            p_task_uuid, p_processor_id, false, v_current_claimed_by, v_execution_status, 'Failed to claim - race condition'
        ) RETURNING tasker_finalization_attempts.finalization_attempt_uuid INTO v_attempt_uuid;
        
        RETURN QUERY SELECT
            false,
            v_current_claimed_by,
            v_execution_status,
            'Failed to claim - race condition'::character varying,
            v_attempt_uuid;
        RETURN;
    END IF;
END;
$$;

-- Create companion function to release finalization claim
CREATE OR REPLACE FUNCTION public.release_finalization_claim(
    p_task_uuid uuid,
    p_processor_id character varying
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
        AND claimed_by = p_processor_id;

    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;
    RETURN v_rows_updated > 0;
END;
$$;

-- Add index to support finalization claiming queries
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_finalization_claim
ON tasker_tasks(task_uuid, complete, claimed_by, claimed_at)
WHERE complete = false;

-- Create tracking table for finalization attempts (optional but recommended for debugging)
CREATE TABLE IF NOT EXISTS tasker_finalization_attempts (
    finalization_attempt_uuid uuid PRIMARY KEY DEFAULT uuid_generate_v7(),
    task_uuid uuid NOT NULL,
    processor_id character varying NOT NULL,
    claimed boolean NOT NULL,
    already_claimed_by character varying,
    task_state character varying,
    message character varying,
    attempted_at timestamp DEFAULT NOW() NOT NULL
);

-- Add indexes for the tracking table
CREATE INDEX IF NOT EXISTS idx_finalization_attempts_task_uuid
ON tasker_finalization_attempts(task_uuid, attempted_at DESC);

CREATE INDEX IF NOT EXISTS idx_finalization_attempts_processor
ON tasker_finalization_attempts(processor_id, attempted_at DESC);

-- Add comments for documentation
COMMENT ON FUNCTION claim_task_for_finalization IS
'Atomically claim a task for finalization processing. Prevents multiple processors from finalizing the same task simultaneously.';

COMMENT ON FUNCTION release_finalization_claim IS
'Release a finalization claim when done processing or on error. Only releases if caller owns the claim.';

COMMENT ON TABLE tasker_finalization_attempts IS
'Track finalization attempts for debugging and metrics. Records all claim attempts with their outcomes.';
-- Migration: Add claim transfer functions for TAS-41 claim transfer architecture
-- Purpose: Enable safe claim transfers between different purposes when held by same processor
-- This allows complex multi-phase operations without race conditions

-- Function: transfer_task_claim
-- Transfers a claim from one purpose to another for the same processor
-- This is the key function that enables the TAS-41 claim transfer architecture
CREATE OR REPLACE FUNCTION transfer_task_claim(
    p_task_uuid uuid,
    p_processor_id character varying,
    p_from_purpose character varying,
    p_to_purpose character varying,
    p_new_timeout_seconds integer DEFAULT NULL
) RETURNS TABLE(
    transferred boolean,
    task_uuid uuid,
    processor_id character varying,
    new_purpose character varying,
    claimed_at timestamp with time zone,
    claim_timeout_seconds integer,
    message character varying
) LANGUAGE plpgsql
AS $$
DECLARE
    v_current_holder character varying;
    v_current_timeout integer;
    v_claimed_at timestamp with time zone;
    v_task_complete boolean;
    v_final_timeout integer;
BEGIN
    -- Get current claim information
    SELECT t.claimed_by, t.claim_timeout_seconds, t.claimed_at, t.complete
    INTO v_current_holder, v_current_timeout, v_claimed_at, v_task_complete
    FROM tasker_tasks t
    WHERE t.task_uuid = p_task_uuid;

    -- Check if task exists
    IF NOT FOUND THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_to_purpose,
            NULL::timestamp with time zone, 0,
            'Task not found';
        RETURN;
    END IF;

    -- Check if task is complete
    IF v_task_complete THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_to_purpose,
            NULL::timestamp with time zone, 0,
            'Task is complete and cannot be claimed';
        RETURN;
    END IF;

    -- Check if task is currently claimed
    IF v_current_holder IS NULL OR v_claimed_at IS NULL THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_to_purpose,
            NULL::timestamp with time zone, 0,
            'Task is not currently claimed';
        RETURN;
    END IF;

    -- Check if the claim has expired
    IF v_claimed_at < (NOW() - (v_current_timeout || ' seconds')::interval) THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_to_purpose,
            v_claimed_at, v_current_timeout,
            'Current claim has expired';
        RETURN;
    END IF;

    -- Check if the same processor holds the claim (this is the key safety check)
    IF v_current_holder != p_processor_id THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_to_purpose,
            v_claimed_at, v_current_timeout,
            format('Task is claimed by different processor: %s', v_current_holder);
        RETURN;
    END IF;

    -- Determine final timeout (use new timeout if provided, otherwise keep existing)
    v_final_timeout := COALESCE(p_new_timeout_seconds, v_current_timeout);

    -- Perform the claim transfer (update claim timestamp and timeout)
    -- The purpose information is tracked at the application level through the TaskClaimState enum
    UPDATE tasker_tasks 
    SET claimed_at = NOW(),
        claim_timeout_seconds = v_final_timeout,
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND claimed_by = p_processor_id;  -- Double-check ownership

    -- Verify the update succeeded
    IF NOT FOUND THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_to_purpose,
            v_claimed_at, v_current_timeout,
            'Failed to update claim - race condition detected';
        RETURN;
    END IF;

    -- Return success
    RETURN QUERY SELECT 
        true, p_task_uuid, p_processor_id, p_to_purpose,
        NOW(), v_final_timeout,
        format('Successfully transferred claim from %s to %s', p_from_purpose, p_to_purpose);
END;
$$;

-- Function: get_task_claim_info
-- Helper function to get current claim information for TaskClaimState conversion
CREATE OR REPLACE FUNCTION get_task_claim_info(
    p_task_uuid uuid
) RETURNS TABLE(
    task_uuid uuid,
    complete boolean,
    claimed_by character varying,
    claimed_at timestamp with time zone,
    claim_timeout_seconds integer,
    is_expired boolean,
    updated_at timestamp with time zone,
    created_at timestamp with time zone
) LANGUAGE plpgsql
AS $$
BEGIN
    RETURN QUERY
    SELECT 
        t.task_uuid,
        t.complete,
        t.claimed_by,
        t.claimed_at,
        t.claim_timeout_seconds,
        CASE 
            WHEN t.claimed_at IS NULL THEN false
            WHEN t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval) THEN true
            ELSE false
        END as is_expired,
        t.updated_at,
        t.created_at
    FROM tasker_tasks t
    WHERE t.task_uuid = p_task_uuid;
END;
$$;

-- Function: claim_task_with_purpose
-- Enhanced claim function that includes purpose information for better tracking
-- This extends the existing claim_ready_tasks pattern but for single tasks with purpose
CREATE OR REPLACE FUNCTION claim_task_with_purpose(
    p_task_uuid uuid,
    p_processor_id character varying,
    p_purpose character varying,
    p_claim_timeout_seconds integer DEFAULT 300
) RETURNS TABLE(
    claimed boolean,
    task_uuid uuid,
    processor_id character varying,
    purpose character varying,
    claimed_at timestamp with time zone,
    claim_timeout_seconds integer,
    message character varying
) LANGUAGE plpgsql
AS $$
DECLARE
    v_task_complete boolean;
    v_current_holder character varying;
    v_current_claimed_at timestamp with time zone;
    v_current_timeout integer;
BEGIN
    -- Get current task state
    SELECT t.complete, t.claimed_by, t.claimed_at, t.claim_timeout_seconds
    INTO v_task_complete, v_current_holder, v_current_claimed_at, v_current_timeout
    FROM tasker_tasks t
    WHERE t.task_uuid = p_task_uuid;

    -- Check if task exists
    IF NOT FOUND THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_purpose,
            NULL::timestamp with time zone, 0,
            'Task not found';
        RETURN;
    END IF;

    -- Check if task is complete
    IF v_task_complete THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_purpose,
            NULL::timestamp with time zone, 0,
            'Task is complete and cannot be claimed';
        RETURN;
    END IF;

    -- Check if task is already claimed by someone else
    IF v_current_holder IS NOT NULL AND 
       v_current_claimed_at IS NOT NULL AND
       v_current_claimed_at >= (NOW() - (v_current_timeout || ' seconds')::interval) AND
       v_current_holder != p_processor_id THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_purpose,
            v_current_claimed_at, v_current_timeout,
            format('Task is claimed by different processor: %s', v_current_holder);
        RETURN;
    END IF;

    -- Attempt to claim the task
    UPDATE tasker_tasks
    SET claimed_at = NOW(),
        claimed_by = p_processor_id,
        claim_timeout_seconds = COALESCE(p_claim_timeout_seconds, claim_timeout_seconds),
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND complete = false
        AND (claimed_at IS NULL 
             OR claimed_at < (NOW() - (claim_timeout_seconds || ' seconds')::interval)
             OR claimed_by = p_processor_id);

    -- Check if the claim succeeded
    IF NOT FOUND THEN
        RETURN QUERY SELECT 
            false, p_task_uuid, p_processor_id, p_purpose,
            NULL::timestamp with time zone, 0,
            'Failed to claim task - race condition or invalid state';
        RETURN;
    END IF;

    -- Return success
    RETURN QUERY SELECT 
        true, p_task_uuid, p_processor_id, p_purpose,
        NOW(), COALESCE(p_claim_timeout_seconds, v_current_timeout, 300),
        format('Successfully claimed task for purpose: %s', p_purpose);
END;
$$;

-- Add comments for documentation
COMMENT ON FUNCTION transfer_task_claim IS 'TAS-41: Transfers a task claim between purposes for the same processor. Enables complex multi-phase operations without race conditions.';

COMMENT ON FUNCTION get_task_claim_info IS 'TAS-41: Returns detailed claim information for TaskClaimState enum conversion and state synchronization.';

COMMENT ON FUNCTION claim_task_with_purpose IS 'TAS-41: Enhanced task claiming with purpose tracking. Extends the existing claiming pattern with application-level purpose information.';

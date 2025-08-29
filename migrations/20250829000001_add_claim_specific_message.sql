-- Migration: Add claim specific message function with VT override capability
-- This function allows claiming a specific message by ID even if it has an active visibility timeout
-- Critical for fallback poller scenarios where we need to override existing VT

-- Function to claim a specific message by ID with optional VT override
CREATE OR REPLACE FUNCTION pgmq_claim_specific_message(
    queue_name text,
    target_msg_id bigint,
    new_vt_seconds integer DEFAULT 30,
    override_existing_vt boolean DEFAULT false
) RETURNS TABLE (
    msg_id bigint,
    read_ct integer,
    enqueued_at timestamp with time zone,
    vt timestamp with time zone,
    message jsonb
) AS $$
DECLARE
    queue_table_name text;
    archive_table_name text;
    sql_query text;
    result_record record;
BEGIN
    -- Validate queue name (security measure)
    IF queue_name ~ '[^a-zA-Z0-9_]' THEN
        RAISE EXCEPTION 'Invalid queue name: %', queue_name;
    END IF;
    
    -- Construct table names
    queue_table_name := 'pgmq.' || quote_ident(queue_name);
    archive_table_name := 'pgmq.' || quote_ident(queue_name || '_archive');
    
    -- Check if the queue table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_schema = 'pgmq' 
        AND table_name = queue_name
    ) THEN
        RAISE EXCEPTION 'Queue does not exist: %', queue_name;
    END IF;
    
    -- Build the dynamic SQL query to claim the specific message
    -- The key difference: we can override existing VT if override_existing_vt is true
    sql_query := format('
        UPDATE %I 
        SET 
            vt = (now() + interval ''%s seconds''),
            read_ct = read_ct + 1
        WHERE msg_id = %L
        AND (%s OR vt <= now())
        RETURNING msg_id, read_ct, enqueued_at, vt, message
    ', queue_table_name, new_vt_seconds, target_msg_id, 
       CASE WHEN override_existing_vt THEN 'true' ELSE 'false' END);
    
    -- Execute the query and return the result
    FOR result_record IN EXECUTE sql_query LOOP
        RETURN QUERY SELECT 
            result_record.msg_id,
            result_record.read_ct,
            result_record.enqueued_at,
            result_record.vt,
            result_record.message;
        RETURN; -- Exit after first match (should only be one)
    END LOOP;
    
    -- If we get here, no message was found/claimed
    -- This could be because:
    -- 1. Message doesn't exist
    -- 2. Message has active VT and override_existing_vt = false
    -- 3. Message was already processed/archived
    RETURN;
END;
$$ LANGUAGE plpgsql;

-- Grant necessary permissions
GRANT EXECUTE ON FUNCTION pgmq_claim_specific_message TO PUBLIC;

-- Add helpful comment for documentation
COMMENT ON FUNCTION pgmq_claim_specific_message IS 
'Claims a specific message by ID with optional visibility timeout override. 
Used by fallback pollers to claim messages that may have active VT from previous reads.
Parameters:
- queue_name: Name of the PGMQ queue
- target_msg_id: Specific message ID to claim
- new_vt_seconds: New visibility timeout to set (default 30)
- override_existing_vt: Whether to claim even if message has active VT (default false)
Returns the claimed message or NULL if not found/claimable.';
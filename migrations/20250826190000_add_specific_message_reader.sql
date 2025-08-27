-- Migration: Add specific message reader function with visibility timeout
-- This function safely reads a specific message ID from a PGMQ queue
-- and sets a visibility timeout to prevent race conditions

-- Function to read a specific message by ID with visibility timeout
CREATE OR REPLACE FUNCTION pgmq_read_specific_message(
    queue_name text,
    target_msg_id bigint,
    vt_seconds integer DEFAULT 30
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
    
    -- Build the dynamic SQL query to read the specific message
    sql_query := format('
        UPDATE %I 
        SET 
            vt = (now() + interval ''%s seconds''),
            read_ct = read_ct + 1
        WHERE msg_id = %L
        AND vt <= now()
        RETURNING msg_id, read_ct, enqueued_at, vt, message
    ', queue_table_name, vt_seconds, target_msg_id);
    
    -- Execute the query and return the result
    FOR result_record IN EXECUTE sql_query LOOP
        RETURN QUERY SELECT 
            result_record.msg_id,
            result_record.read_ct,
            result_record.enqueued_at,
            result_record.vt,
            result_record.message;
    END LOOP;
    
    -- If no record was returned, the message either:
    -- 1. Doesn't exist
    -- 2. Is already claimed (vt > now())
    -- 3. Has been archived
    
    RETURN;
END;
$$ LANGUAGE plpgsql;

-- Function to delete a specific message by ID (after successful processing)
CREATE OR REPLACE FUNCTION pgmq_delete_specific_message(
    queue_name text,
    target_msg_id bigint
) RETURNS boolean AS $$
DECLARE
    queue_table_name text;
    deleted_count integer;
BEGIN
    -- Validate queue name (security measure)
    IF queue_name ~ '[^a-zA-Z0-9_]' THEN
        RAISE EXCEPTION 'Invalid queue name: %', queue_name;
    END IF;
    
    -- Construct table name
    queue_table_name := 'pgmq.' || quote_ident(queue_name);
    
    -- Check if the queue table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_schema = 'pgmq' 
        AND table_name = queue_name
    ) THEN
        RAISE EXCEPTION 'Queue does not exist: %', queue_name;
    END IF;
    
    -- Delete the specific message
    EXECUTE format('DELETE FROM %I WHERE msg_id = %L', queue_table_name, target_msg_id);
    
    -- Get the number of rows affected
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    
    RETURN deleted_count > 0;
END;
$$ LANGUAGE plpgsql;

-- Function to extend visibility timeout for a specific message (if processing takes longer)
CREATE OR REPLACE FUNCTION pgmq_extend_vt_specific_message(
    queue_name text,
    target_msg_id bigint,
    additional_vt_seconds integer DEFAULT 30
) RETURNS boolean AS $$
DECLARE
    queue_table_name text;
    updated_count integer;
BEGIN
    -- Validate queue name (security measure)
    IF queue_name ~ '[^a-zA-Z0-9_]' THEN
        RAISE EXCEPTION 'Invalid queue name: %', queue_name;
    END IF;
    
    -- Construct table name
    queue_table_name := 'pgmq.' || quote_ident(queue_name);
    
    -- Check if the queue table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_schema = 'pgmq' 
        AND table_name = queue_name
    ) THEN
        RAISE EXCEPTION 'Queue does not exist: %', queue_name;
    END IF;
    
    -- Extend the visibility timeout for the specific message
    EXECUTE format('
        UPDATE %I 
        SET vt = vt + interval ''%s seconds''
        WHERE msg_id = %L
    ', queue_table_name, additional_vt_seconds, target_msg_id);
    
    -- Get the number of rows affected
    GET DIAGNOSTICS updated_count = ROW_COUNT;
    
    RETURN updated_count > 0;
END;
$$ LANGUAGE plpgsql;
-- Migration: Add PGMQ Notification Wrapper Functions
-- Generated at: 2025-08-26 18:09:21 UTC
-- Updated at: 2026-01-14 (TAS-133: Full payload notifications)
--
-- TAS-41 Update: Replaced trigger-based approach with atomic wrapper functions
-- that combine pgmq.send with pg_notify in a single transaction.
--
-- TAS-133 Update: Added full message payload inclusion in notifications when
-- message size < 7KB (pg_notify limit is ~8KB). This enables RabbitMQ-style
-- direct message processing without a separate fetch operation.
--
-- Benefits:
-- - No dependency on PGMQ internal tables
-- - No trigger installation timing issues
-- - Atomic operations (message + notification)
-- - Simple, testable, maintainable
-- - Reliable namespace extraction
-- - Full payload delivery for small messages (TAS-133)

-- UP Migration

-- Robust namespace extraction helper function
CREATE OR REPLACE FUNCTION extract_queue_namespace(queue_name TEXT)
RETURNS TEXT AS $$
BEGIN
    -- Handle orchestration queues
    IF queue_name ~ '^orchestration' THEN
        RETURN 'orchestration';
    END IF;

    -- Handle worker queues: worker_namespace_queue -> namespace
    IF queue_name ~ '^worker_.*_queue$' THEN
        RETURN COALESCE(
            (regexp_match(queue_name, '^worker_(.+?)_queue$'))[1],
            'worker'
        );
    END IF;

    -- Handle standard namespace_queue pattern
    IF queue_name ~ '^[a-zA-Z][a-zA-Z0-9_]*_queue$' THEN
        RETURN COALESCE(
            (regexp_match(queue_name, '^([a-zA-Z][a-zA-Z0-9_]*)_queue$'))[1],
            'default'
        );
    END IF;

    -- Fallback for any other pattern
    RETURN 'default';
END;
$$ LANGUAGE plpgsql;

-- Function to notify when queues are created (updated to use robust extraction)
CREATE OR REPLACE FUNCTION pgmq_notify_queue_created()
RETURNS trigger AS $$
DECLARE
    event_payload TEXT;
    channel_name TEXT := 'pgmq_queue_created';
    namespace_name TEXT;
BEGIN
    -- Extract namespace using robust helper function
    namespace_name := extract_queue_namespace(NEW.queue_name);

    -- Build event payload
    event_payload := json_build_object(
        'event_type', 'queue_created',
        'queue_name', NEW.queue_name,
        'namespace', namespace_name,
        'created_at', NOW()::timestamptz
    )::text;

    -- Truncate if payload exceeds limit
    IF length(event_payload) > 7800 THEN
        event_payload := substring(event_payload, 1, 7790) || '...}';
    END IF;

    -- Send notification
    PERFORM pg_notify(channel_name, event_payload);

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- TAS-133: Payload size threshold for full message inclusion
-- pg_notify has ~8KB limit; we use 7000 bytes to leave room for metadata JSON wrapper
-- Messages larger than this threshold fall back to signal-only notifications
CREATE OR REPLACE FUNCTION pgmq_payload_size_threshold()
RETURNS INTEGER AS $$
BEGIN
    RETURN 7000;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Wrapper function that sends message AND notification atomically
-- TAS-133: Now includes full message payload when size < 7KB threshold
CREATE OR REPLACE FUNCTION pgmq_send_with_notify(
    queue_name TEXT,
    message JSONB,
    delay_seconds INTEGER DEFAULT 0
) RETURNS BIGINT AS $$
DECLARE
    msg_id BIGINT;
    namespace_name TEXT;
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
    message_str TEXT;
    include_full_payload BOOLEAN;
    size_threshold INTEGER;
BEGIN
    -- Send message using PGMQ's native function
    SELECT pgmq.send(queue_name, message, delay_seconds) INTO msg_id;

    -- Extract namespace from queue name using robust helper
    namespace_name := extract_queue_namespace(queue_name);

    -- Build namespace-specific channel name
    namespace_channel := 'pgmq_message_ready.' || namespace_name;

    -- TAS-133: Check if we can include full message payload
    -- This enables RabbitMQ-style direct processing without separate fetch
    message_str := message::text;
    size_threshold := pgmq_payload_size_threshold();
    include_full_payload := length(message_str) < size_threshold;

    IF include_full_payload THEN
        -- TAS-133: Full payload notification (RabbitMQ-style)
        -- Consumer can process directly and ack without fetching
        event_payload := json_build_object(
            'event_type', 'message_with_payload',
            'msg_id', msg_id,
            'queue_name', queue_name,
            'namespace', namespace_name,
            'message', message,
            'ready_at', NOW()::timestamptz,
            'delay_seconds', delay_seconds
        )::text;
    ELSE
        -- Signal-only notification (fallback for large messages)
        -- Consumer must fetch message separately via pgmq_read_specific_message
        event_payload := json_build_object(
            'event_type', 'message_ready',
            'msg_id', msg_id,
            'queue_name', queue_name,
            'namespace', namespace_name,
            'ready_at', NOW()::timestamptz,
            'delay_seconds', delay_seconds,
            'payload_omitted', true
        )::text;
    END IF;

    -- Safety truncation (should never trigger with proper threshold)
    IF length(event_payload) > 7800 THEN
        -- If we somehow exceed limit, fall back to signal-only
        event_payload := json_build_object(
            'event_type', 'message_ready',
            'msg_id', msg_id,
            'queue_name', queue_name,
            'namespace', namespace_name,
            'ready_at', NOW()::timestamptz,
            'payload_omitted', true,
            'truncated', true
        )::text;
    END IF;

    -- Send notifications in same transaction
    PERFORM pg_notify(namespace_channel, event_payload);

    -- Also send to global channel if different
    IF namespace_channel != global_channel THEN
        PERFORM pg_notify(global_channel, event_payload);
    END IF;

    RETURN msg_id;
END;
$$ LANGUAGE plpgsql;

-- Batch version for efficiency
CREATE OR REPLACE FUNCTION pgmq_send_batch_with_notify(
    queue_name TEXT,
    messages JSONB[],
    delay_seconds INTEGER DEFAULT 0
) RETURNS SETOF BIGINT AS $$
DECLARE
    msg_id BIGINT;
    msg_ids BIGINT[];
    namespace_name TEXT;
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
BEGIN
    -- Send batch using PGMQ's native function and collect results
    SELECT ARRAY_AGG(t.msg_id) INTO msg_ids
    FROM pgmq.send_batch(queue_name, messages, delay_seconds) AS t(msg_id);

    -- Extract namespace and build channels
    namespace_name := extract_queue_namespace(queue_name);
    namespace_channel := 'pgmq_message_ready.' || namespace_name;

    -- Build event payload for batch
    event_payload := json_build_object(
        'event_type', 'batch_ready',
        'msg_ids', msg_ids,
        'queue_name', queue_name,
        'namespace', namespace_name,
        'message_count', array_length(msg_ids, 1),
        'ready_at', NOW()::timestamptz,
        'delay_seconds', delay_seconds
    )::text;

    -- Truncate if payload exceeds limit
    IF length(event_payload) > 7800 THEN
        event_payload := substring(event_payload, 1, 7790) || '...}';
    END IF;

    -- Send notifications in same transaction
    PERFORM pg_notify(namespace_channel, event_payload);

    -- Also send to global channel if different
    IF namespace_channel != global_channel THEN
        PERFORM pg_notify(global_channel, event_payload);
    END IF;

    -- Return the message IDs
    FOREACH msg_id IN ARRAY msg_ids LOOP
        RETURN NEXT msg_id;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Install trigger on pgmq.meta table for queue creation notifications
-- (Only if the table exists)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'pgmq' AND table_name = 'meta') THEN

        -- Drop existing trigger if it exists (migrations might run multiple times)
        DROP TRIGGER IF EXISTS pgmq_queue_created_trigger ON pgmq.meta;

        CREATE TRIGGER pgmq_queue_created_trigger
            AFTER INSERT ON pgmq.meta
            FOR EACH ROW
            EXECUTE FUNCTION pgmq_notify_queue_created();

        RAISE NOTICE 'Installed queue creation trigger on pgmq.meta';
    ELSE
        RAISE NOTICE 'pgmq.meta table not found - skipping queue creation trigger';
    END IF;
END;
$$;

-- TAS-41: Message ready triggers replaced with wrapper functions
-- No longer need to install triggers on individual queue tables
-- Applications should use pgmq_send_with_notify() and pgmq_send_batch_with_notify()
-- instead of direct pgmq.send() calls

DO $$
BEGIN
    RAISE NOTICE 'PGMQ notification wrapper functions created successfully';
    RAISE NOTICE 'Use pgmq_send_with_notify() instead of pgmq.send() for push notifications';
    RAISE NOTICE 'Use pgmq_send_batch_with_notify() instead of pgmq.send_batch() for batch push notifications';
END;
$$;

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

    -- Construct table names with PGMQ's naming convention (q_ prefix for queues, a_ prefix for archives)
    queue_table_name := 'pgmq.q_' || queue_name;
    archive_table_name := 'pgmq.a_' || queue_name;

    -- Check if the queue table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'pgmq'
        AND table_name = 'q_' || queue_name
    ) THEN
        RAISE EXCEPTION 'Queue does not exist: %', queue_name;
    END IF;

    -- Build the dynamic SQL query to read the specific message
    sql_query := format('
        UPDATE %s
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

    -- Construct table name with PGMQ's naming convention (q_ prefix)
    queue_table_name := 'pgmq.q_' || queue_name;

    -- Check if the queue table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'pgmq'
        AND table_name = queue_name
    ) THEN
        RAISE EXCEPTION 'Queue does not exist: %', queue_name;
    END IF;

    -- Delete the specific message
    EXECUTE format('DELETE FROM %s WHERE msg_id = %L', queue_table_name, target_msg_id);

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
        UPDATE %s
        SET vt = vt + interval ''%s seconds''
        WHERE msg_id = %L
    ', queue_table_name, additional_vt_seconds, target_msg_id);

    -- Get the number of rows affected
    GET DIAGNOSTICS updated_count = ROW_COUNT;

    RETURN updated_count > 0;
END;
$$ LANGUAGE plpgsql;

-- DOWN Migration (Rollback)
-- Uncomment to rollback wrapper functions:

-- DROP FUNCTION IF EXISTS pgmq_send_batch_with_notify(TEXT, JSONB[], INTEGER) CASCADE;
-- DROP FUNCTION IF EXISTS pgmq_send_with_notify(TEXT, JSONB, INTEGER) CASCADE;
-- DROP FUNCTION IF EXISTS pgmq_notify_queue_created() CASCADE;
-- DROP FUNCTION IF EXISTS extract_queue_namespace(TEXT) CASCADE;
-- DROP TRIGGER IF EXISTS pgmq_queue_created_trigger ON pgmq.meta;

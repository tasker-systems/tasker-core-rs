-- Migration: Add PGMQ Notification Triggers
-- Generated at: 2025-08-26 18:09:21 UTC
-- Updated at: 2025-09-03 23:17:00 UTC
-- Configuration:
--   Queue pattern: Enhanced to support orchestration and worker queue naming
--     - orchestration_*_queue -> orchestration namespace
--     - worker_*_queue -> extract middle part as namespace
--     - simple_queue -> extract prefix as namespace
--   Channel prefix: None
--
-- Migration Safety: Uses DROP TRIGGER IF EXISTS before CREATE TRIGGER
-- to handle multiple migration runs safely
--   Max payload size: 7800 bytes
--
-- EventDrivenOnly Support: This migration enables pgmq-notify for
-- orchestration queues to support EventDrivenOnly deployment mode

-- UP Migration

-- Function to notify when queues are created
CREATE OR REPLACE FUNCTION pgmq_notify_queue_created()
RETURNS trigger AS $$
DECLARE
    event_payload TEXT;
    channel_name TEXT := 'pgmq_queue_created';
    namespace_name TEXT;
BEGIN
    -- Extract namespace from queue name using configured pattern
    namespace_name := (regexp_match(NEW.queue_name, '(\w+)_queue'))[1];
    IF namespace_name IS NULL THEN
        namespace_name := 'default';
    END IF;

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

-- Function to notify when messages are ready
CREATE OR REPLACE FUNCTION pgmq_notify_message_ready()
RETURNS trigger AS $$
DECLARE
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
    namespace_name TEXT;
    queue_name_val TEXT;
BEGIN
    -- Get queue name from table name (remove 'q_' prefix)
    queue_name_val := substring(TG_TABLE_NAME, 3);

    -- Extract namespace from queue name using patterns for orchestration and worker queues
    IF queue_name_val LIKE 'orchestration%_queue' THEN
        -- For orchestration queues like 'orchestration_task_requests_queue'
        namespace_name := 'orchestration';
    ELSIF queue_name_val ~ '^worker_.*_queue$' THEN
        -- For worker queues like 'worker_linear_workflow_queue', extract the middle part
        namespace_name := (regexp_match(queue_name_val, '^worker_(.*)_queue$'))[1];
        IF namespace_name IS NULL THEN
            namespace_name := 'worker';
        END IF;
    ELSE
        -- Fallback to original pattern for simple cases like 'orders_queue'
        namespace_name := (regexp_match(queue_name_val, '^(\w+)_queue$'))[1];
        IF namespace_name IS NULL THEN
            namespace_name := 'default';
        END IF;
    END IF;

    -- Build namespace-specific channel name
    namespace_channel := 'pgmq_message_ready.' || namespace_name;

    -- Build event payload
    event_payload := json_build_object(
        'event_type', 'message_ready',
        'msg_id', NEW.msg_id,
        'queue_name', queue_name_val,
        'namespace', namespace_name,
        'ready_at', NOW()::timestamptz,
        'visibility_timeout_seconds', EXTRACT(EPOCH FROM NEW.vt - NOW())::integer
    )::text;

    -- Truncate if payload exceeds limit
    IF length(event_payload) > 7800 THEN
        event_payload := substring(event_payload, 1, 7790) || '...}';
    END IF;

    -- Send to namespace-specific channel
    PERFORM pg_notify(namespace_channel, event_payload);

    -- Also send to global channel if different
    IF namespace_channel != global_channel THEN
        PERFORM pg_notify(global_channel, event_payload);
    END IF;

    RETURN NEW;
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

-- Install message ready triggers on all existing PGMQ queue tables
DO $$
DECLARE
    queue_record RECORD;
    trigger_name TEXT;
    truncated_trigger_name TEXT;
BEGIN
    FOR queue_record IN
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'pgmq'
          AND table_name LIKE 'q_%'
    LOOP
        -- Build trigger name and handle PostgreSQL identifier length limits (63 chars)
        trigger_name := 'pgmq_message_ready_trigger_' || queue_record.table_name;
        
        -- Truncate trigger name if it's too long for PostgreSQL (63 char limit)
        IF length(trigger_name) > 63 THEN
            truncated_trigger_name := left(trigger_name, 63);
        ELSE
            truncated_trigger_name := trigger_name;
        END IF;

        -- Drop existing trigger if it exists (migrations might run multiple times)
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON pgmq.%I',
            truncated_trigger_name, queue_record.table_name);

        EXECUTE format('CREATE TRIGGER %I
            AFTER INSERT ON pgmq.%I
            FOR EACH ROW
            EXECUTE FUNCTION pgmq_notify_message_ready()',
            truncated_trigger_name, queue_record.table_name);

        RAISE NOTICE 'Installed message ready trigger % on pgmq.%', 
            truncated_trigger_name, queue_record.table_name;
    END LOOP;
END;
$$;

-- Specifically ensure orchestration queue triggers are installed
-- (In case orchestration queues are created after this migration runs)
DO $$
DECLARE
    orch_queue_names TEXT[] := ARRAY[
        'q_orchestration_task_requests_queue',
        'q_orchestration_step_results_queue', 
        'q_orchestration_task_finalizations_queue'
    ];
    queue_name TEXT;
    trigger_name TEXT;
BEGIN
    FOREACH queue_name IN ARRAY orch_queue_names
    LOOP
        -- Only install if the table exists
        IF EXISTS (SELECT 1 FROM information_schema.tables 
                   WHERE table_schema = 'pgmq' AND table_name = queue_name) THEN
            
            trigger_name := 'pgmq_msg_ready_' || right(queue_name, 20); -- Abbreviated trigger name
            
            EXECUTE format('DROP TRIGGER IF EXISTS %I ON pgmq.%I',
                trigger_name, queue_name);
                
            EXECUTE format('CREATE TRIGGER %I
                AFTER INSERT ON pgmq.%I
                FOR EACH ROW
                EXECUTE FUNCTION pgmq_notify_message_ready()',
                trigger_name, queue_name);
                
            RAISE NOTICE 'Ensured orchestration trigger % on pgmq.%', 
                trigger_name, queue_name;
        ELSE
            RAISE NOTICE 'Orchestration queue pgmq.% does not exist yet - skipping', queue_name;
        END IF;
    END LOOP;
END;
$$;

-- DOWN Migration (Rollback)

-- Drop triggers on all PGMQ queue tables
-- DO $$
-- DECLARE
--     trigger_record RECORD;
-- BEGIN
--     FOR trigger_record IN
--         SELECT trigger_name, event_object_table
--         FROM information_schema.triggers
--         WHERE trigger_schema = 'pgmq'
--           AND trigger_name LIKE 'pgmq_%_trigger%'
--     LOOP
--         EXECUTE format('DROP TRIGGER IF EXISTS %I ON pgmq.%I',
--             trigger_record.trigger_name, trigger_record.event_object_table);

--         RAISE NOTICE 'Dropped trigger % on pgmq.%',
--             trigger_record.trigger_name, trigger_record.event_object_table;
--     END LOOP;
-- END;
-- $$;

-- -- Drop trigger functions
-- DROP FUNCTION IF EXISTS pgmq_notify_message_ready() CASCADE;
-- DROP FUNCTION IF EXISTS pgmq_notify_queue_created() CASCADE;

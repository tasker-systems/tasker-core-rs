-- Add headers column to pgmq queue tables for compatibility with pgmq extension v1.5.1+
--
-- Issue: pgmq-rs crate v0.30.1 creates queue tables without the headers column,
-- but pgmq extension v1.5.1+ expects all queue tables to have a headers JSONB column.
--
-- This migration ensures compatibility by:
-- 1. Adding headers column to existing queue tables that lack it
-- 2. Providing a function to automatically add headers to new queue tables
--
-- Background:
-- - pgmq extension v1.5.1 introduced headers support for message metadata
-- - pgmq.read() function expects the headers column to exist
-- - Rust pgmq-rs crate hasn't been updated to include headers in table creation
--
-- This migration bridges the compatibility gap until pgmq-rs is updated.

-- Function to add headers column to a pgmq queue table if it doesn't exist
CREATE OR REPLACE FUNCTION pgmq_ensure_headers_column(queue_name TEXT)
RETURNS VOID AS $$
DECLARE
    full_table_name TEXT;
    column_exists BOOLEAN;
BEGIN
    -- Construct full table name (pgmq queues are prefixed with 'q_')
    full_table_name := 'q_' || queue_name;

    -- Check if headers column exists
    SELECT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'pgmq'
        AND table_name = full_table_name
        AND column_name = 'headers'
    ) INTO column_exists;

    -- Add headers column if it doesn't exist
    IF NOT column_exists THEN
        EXECUTE format('ALTER TABLE pgmq.%I ADD COLUMN headers JSONB', full_table_name);
        RAISE NOTICE 'Added headers column to pgmq.%', full_table_name;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Add headers column to existing queue tables that might be missing it
-- This handles any queues created by the Rust pgmq-rs crate

DO $$
DECLARE
    queue_record RECORD;
    table_name TEXT;
BEGIN
    -- Find all pgmq queue tables and ensure they have headers column
    FOR queue_record IN
        SELECT schemaname, tablename
        FROM pg_tables
        WHERE schemaname = 'pgmq'
        AND tablename LIKE 'q_%'
    LOOP
        -- Extract queue name by removing 'q_' prefix
        SELECT substring(queue_record.tablename from 3) INTO table_name;

        -- Use our function to ensure headers column exists
        PERFORM pgmq_ensure_headers_column(table_name);
    END LOOP;
END;
$$;

-- Create a trigger function to automatically add headers column to new queue tables
-- This ensures future queue tables created by pgmq-rs will be compatible
CREATE OR REPLACE FUNCTION pgmq_auto_add_headers_trigger()
RETURNS event_trigger AS $$
DECLARE
    obj RECORD;
    queue_name TEXT;
BEGIN
    -- Only process CREATE TABLE events in pgmq schema
    FOR obj IN SELECT * FROM pg_event_trigger_ddl_commands()
    WHERE command_tag = 'CREATE TABLE' AND schema_name = 'pgmq'
    LOOP
        -- Extract queue name from table name (remove 'q_' prefix)
        IF obj.object_identity LIKE 'pgmq.q_%' THEN
            queue_name := substring(obj.object_identity from 8);  -- Remove 'pgmq.q_'

            -- Ensure the new table has headers column
            PERFORM pgmq_ensure_headers_column(queue_name);
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Create the event trigger to automatically add headers to new pgmq tables
DROP EVENT TRIGGER IF EXISTS pgmq_headers_compatibility_trigger;
CREATE EVENT TRIGGER pgmq_headers_compatibility_trigger
    ON ddl_command_end
    WHEN TAG IN ('CREATE TABLE')
    EXECUTE FUNCTION pgmq_auto_add_headers_trigger();

COMMENT ON FUNCTION pgmq_ensure_headers_column(TEXT) IS
'Ensures a pgmq queue table has the headers JSONB column required by pgmq extension v1.5.1+';

COMMENT ON FUNCTION pgmq_auto_add_headers_trigger() IS
'Event trigger function to automatically add headers column to new pgmq queue tables';

COMMENT ON EVENT TRIGGER pgmq_headers_compatibility_trigger IS
'Automatically adds headers column to new pgmq queue tables for pgmq-rs compatibility';

-- TAS-78: PGMQ Extensions and Headers Support
--
-- This migration sets up PGMQ for the separate PGMQ database.
-- Extracted from the original uuid_v7_initial_schema.sql to support
-- split database deployments where PGMQ runs on a dedicated database.
--
-- CONTENTS:
-- - pgmq extension (CASCADE includes pgcrypto dependency)
-- - pg_uuidv7 extension (for UUID v7 support in queue messages)
-- - PGMQ headers compatibility functions and triggers
--
-- NOTE: When running in single-database mode, these extensions may already
-- exist from the Tasker migrations. The CREATE EXTENSION IF NOT EXISTS
-- statements handle this gracefully.

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;

-- Create tasker schema (needed for functions below)
CREATE SCHEMA IF NOT EXISTS tasker;

-- UUID v7 Compatibility Layer: Supports both PostgreSQL 17 (pg_uuidv7 extension)
-- and PostgreSQL 18+ (native uuidv7() function)
DO $uuid_compat$
BEGIN
  -- Check if pg_uuidv7 extension is available (PostgreSQL 17)
  IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_uuidv7') THEN
    CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;
    RAISE NOTICE 'Using pg_uuidv7 extension for UUID v7 support';
  ELSE
    -- PostgreSQL 18+: Native uuidv7() exists, create alias function for compatibility
    -- The alias uuid_generate_v7() may already exist from docker-entrypoint-initdb.d
    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'uuid_generate_v7' AND pronamespace = 'public'::regnamespace) THEN
      CREATE OR REPLACE FUNCTION public.uuid_generate_v7() RETURNS uuid
      AS $fn$ SELECT uuidv7(); $fn$ LANGUAGE SQL IMMUTABLE PARALLEL SAFE;
      RAISE NOTICE 'Created uuid_generate_v7() wrapper for native uuidv7()';
    ELSE
      RAISE NOTICE 'uuid_generate_v7() already exists, using existing function';
    END IF;
  END IF;
END $uuid_compat$;

--
-- PGMQ Headers
--

-- Function to add headers column to a pgmq queue table if it doesn't exist
CREATE OR REPLACE FUNCTION tasker.pgmq_ensure_headers_column(queue_name TEXT)
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
        PERFORM tasker.pgmq_ensure_headers_column(table_name);
    END LOOP;
END;
$$;

-- Create a trigger function to automatically add headers column to new queue tables
-- This ensures future queue tables created by pgmq-rs will be compatible
CREATE OR REPLACE FUNCTION tasker.pgmq_auto_add_headers_trigger()
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
            PERFORM tasker.pgmq_ensure_headers_column(queue_name);
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Create the event trigger to automatically add headers to new pgmq tables
DROP EVENT TRIGGER IF EXISTS pgmq_headers_compatibility_trigger;
CREATE EVENT TRIGGER pgmq_headers_compatibility_trigger
    ON ddl_command_end
    WHEN TAG IN ('CREATE TABLE')
    EXECUTE FUNCTION tasker.pgmq_auto_add_headers_trigger();

COMMENT ON FUNCTION tasker.pgmq_ensure_headers_column(TEXT) IS
'Ensures a pgmq queue table has the headers JSONB column required by pgmq extension v1.5.1+';

COMMENT ON FUNCTION tasker.pgmq_auto_add_headers_trigger() IS
'Event trigger function to automatically add headers column to new pgmq queue tables';

COMMENT ON EVENT TRIGGER pgmq_headers_compatibility_trigger IS
'Automatically adds headers column to new pgmq queue tables for pgmq-rs compatibility';

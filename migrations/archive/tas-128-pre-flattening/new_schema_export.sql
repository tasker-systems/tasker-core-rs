--
-- PostgreSQL database dump
--

\restrict qQIa4hJsRfF0VR1S1fMyzCW2PWTkeXglkALREIKnEtuWug2WZah9orqhVnHwvTp

-- Dumped from database version 18.1 (Debian 18.1-1.pgdg12+2)
-- Dumped by pg_dump version 18.1 (Debian 18.1-1.pgdg12+2)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: pgmq; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA pgmq;


--
-- Name: tasker; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA tasker;


--
-- Name: pgmq; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgmq WITH SCHEMA pgmq;


--
-- Name: EXTENSION pgmq; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pgmq IS 'A lightweight message queue. Like AWS SQS and RSMQ but on Postgres.';


--
-- Name: dlq_reason; Type: TYPE; Schema: tasker; Owner: -
--

CREATE TYPE tasker.dlq_reason AS ENUM (
    'staleness_timeout',
    'max_retries_exceeded',
    'dependency_cycle_detected',
    'worker_unavailable',
    'manual_dlq'
);


--
-- Name: dlq_resolution_status; Type: TYPE; Schema: tasker; Owner: -
--

CREATE TYPE tasker.dlq_resolution_status AS ENUM (
    'pending',
    'manually_resolved',
    'permanently_failed',
    'cancelled'
);


--
-- Name: extract_queue_namespace(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.extract_queue_namespace(queue_name text) RETURNS text
    LANGUAGE plpgsql
    AS $_$
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
$_$;


--
-- Name: pgmq_auto_add_headers_trigger(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_auto_add_headers_trigger() RETURNS event_trigger
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: FUNCTION pgmq_auto_add_headers_trigger(); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.pgmq_auto_add_headers_trigger() IS 'Event trigger function to automatically add headers column to new pgmq queue tables';


--
-- Name: pgmq_delete_specific_message(text, bigint); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_delete_specific_message(queue_name text, target_msg_id bigint) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_ensure_headers_column(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_ensure_headers_column(queue_name text) RETURNS void
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: FUNCTION pgmq_ensure_headers_column(queue_name text); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.pgmq_ensure_headers_column(queue_name text) IS 'Ensures a pgmq queue table has the headers JSONB column required by pgmq extension v1.5.1+';


--
-- Name: pgmq_extend_vt_specific_message(text, bigint, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_extend_vt_specific_message(queue_name text, target_msg_id bigint, additional_vt_seconds integer DEFAULT 30) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_notify_queue_created(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_notify_queue_created() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_read_specific_message(text, bigint, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_read_specific_message(queue_name text, target_msg_id bigint, vt_seconds integer DEFAULT 30) RETURNS TABLE(msg_id bigint, read_ct integer, enqueued_at timestamp with time zone, vt timestamp with time zone, message jsonb)
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_send_batch_with_notify(text, jsonb[], integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_send_batch_with_notify(queue_name text, messages jsonb[], delay_seconds integer DEFAULT 0) RETURNS SETOF bigint
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_send_with_notify(text, jsonb, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgmq_send_with_notify(queue_name text, message jsonb, delay_seconds integer DEFAULT 0) RETURNS bigint
    LANGUAGE plpgsql
    AS $$
DECLARE
    msg_id BIGINT;
    namespace_name TEXT;
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
BEGIN
    -- Send message using PGMQ's native function
    SELECT pgmq.send(queue_name, message, delay_seconds) INTO msg_id;

    -- Extract namespace from queue name using robust helper
    namespace_name := extract_queue_namespace(queue_name);

    -- Build namespace-specific channel name
    namespace_channel := 'pgmq_message_ready.' || namespace_name;

    -- Build event payload
    event_payload := json_build_object(
        'event_type', 'message_ready',
        'msg_id', msg_id,
        'queue_name', queue_name,
        'namespace', namespace_name,
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

    RETURN msg_id;
END;
$$;


--
-- Name: uuid_generate_v7(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.uuid_generate_v7() RETURNS uuid
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$ SELECT uuidv7(); $$;


--
-- Name: calculate_dependency_levels(uuid); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.calculate_dependency_levels(input_task_uuid uuid) RETURNS TABLE(workflow_step_uuid uuid, dependency_level integer)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH RECURSIVE dependency_levels AS (
    -- Base case: Find root nodes (steps with no dependencies)
    SELECT
      ws.workflow_step_uuid,
      0 as level
    FROM workflow_steps ws
    WHERE ws.task_uuid = input_task_uuid
      AND NOT EXISTS (
        SELECT 1
        FROM workflow_step_edges wse
        WHERE wse.to_step_uuid = ws.workflow_step_uuid
      )

    UNION ALL

    -- Recursive case: Find children of current level nodes
    SELECT
      wse.to_step_uuid as workflow_step_uuid,
      dl.level + 1 as level
    FROM dependency_levels dl
    JOIN workflow_step_edges wse ON wse.from_step_uuid = dl.workflow_step_uuid
    JOIN workflow_steps ws ON ws.workflow_step_uuid = wse.to_step_uuid
    WHERE ws.task_uuid = input_task_uuid
  )
  SELECT
    dl.workflow_step_uuid,
    MAX(dl.level) as dependency_level  -- Use MAX to handle multiple paths to same node
  FROM dependency_levels dl
  GROUP BY dl.workflow_step_uuid
  ORDER BY dependency_level, workflow_step_uuid;
END;
$$;


--
-- Name: calculate_staleness_threshold(character varying, jsonb, integer, integer, integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.calculate_staleness_threshold(p_task_state character varying, p_template_config jsonb, p_default_waiting_deps integer DEFAULT 60, p_default_waiting_retry integer DEFAULT 30, p_default_steps_process integer DEFAULT 30) RETURNS integer
    LANGUAGE plpgsql IMMUTABLE
    AS $$
BEGIN
    RETURN CASE p_task_state
        WHEN 'waiting_for_dependencies' THEN
            COALESCE(
                (p_template_config->'lifecycle'->>'max_waiting_for_dependencies_minutes')::INTEGER,
                p_default_waiting_deps
            )
        WHEN 'waiting_for_retry' THEN
            COALESCE(
                (p_template_config->'lifecycle'->>'max_waiting_for_retry_minutes')::INTEGER,
                p_default_waiting_retry
            )
        WHEN 'steps_in_process' THEN
            COALESCE(
                (p_template_config->'lifecycle'->>'max_steps_in_process_minutes')::INTEGER,
                p_default_steps_process
            )
        ELSE 1440  -- 24 hours for other states
    END;
END;
$$;


--
-- Name: FUNCTION calculate_staleness_threshold(p_task_state character varying, p_template_config jsonb, p_default_waiting_deps integer, p_default_waiting_retry integer, p_default_steps_process integer); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.calculate_staleness_threshold(p_task_state character varying, p_template_config jsonb, p_default_waiting_deps integer, p_default_waiting_retry integer, p_default_steps_process integer) IS 'TAS-49 Phase 2: Calculate staleness threshold for a task state.

Checks template configuration for state-specific thresholds, falling back
to provided defaults or hardcoded defaults if not found.

Template Configuration Path:
  configuration->''lifecycle''->''max_waiting_for_dependencies_minutes''
  configuration->''lifecycle''->''max_waiting_for_retry_minutes''
  configuration->''lifecycle''->''max_steps_in_process_minutes''

Usage:
- v_task_state_analysis view (replaces inline CASE statement)
- detect_and_transition_stale_tasks() function
- Any monitoring or alerting that needs threshold calculation

Parameters:
- p_task_state: Current task state
- p_template_config: JSONB template configuration
- p_default_waiting_deps: Default threshold for waiting_for_dependencies (60 min)
- p_default_waiting_retry: Default threshold for waiting_for_retry (30 min)
- p_default_steps_process: Default threshold for steps_in_process (30 min)

Returns: Threshold in minutes (INTEGER)';


--
-- Name: calculate_step_next_retry_time(integer, timestamp without time zone, timestamp without time zone, integer, integer, numeric); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.calculate_step_next_retry_time(backoff_request_seconds integer, last_attempted_at timestamp without time zone, failure_time timestamp without time zone, attempts integer, p_max_backoff_seconds integer DEFAULT 60, p_backoff_multiplier numeric DEFAULT 2.0) RETURNS timestamp without time zone
    LANGUAGE sql STABLE
    AS $$
    SELECT CASE
        -- Primary path: Use Rust-calculated backoff (set via BackoffCalculator)
        WHEN backoff_request_seconds IS NOT NULL AND last_attempted_at IS NOT NULL THEN
            last_attempted_at + (backoff_request_seconds * interval '1 second')

        -- Fallback path: Calculate exponentially with configurable params
        -- Used when Rust hasn't set backoff_request_seconds yet (race condition safety)
        WHEN failure_time IS NOT NULL THEN
            failure_time + (
                LEAST(
                    power(p_backoff_multiplier, COALESCE(attempts, 1)) * interval '1 second',
                    p_max_backoff_seconds * interval '1 second'
                )
            )

        ELSE NULL
    END
$$;


--
-- Name: create_dlq_entry(uuid, character varying, character varying, character varying, integer, integer, character varying); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.create_dlq_entry(p_task_uuid uuid, p_namespace_name character varying, p_task_name character varying, p_current_state character varying, p_time_in_state_minutes integer, p_threshold_minutes integer, p_dlq_reason character varying DEFAULT 'staleness_timeout'::character varying) RETURNS uuid
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_dlq_entry_uuid UUID;
    v_task_snapshot JSONB;
BEGIN
    -- Build task snapshot (simplified version - full implementation would include steps)
    SELECT jsonb_build_object(
        'task_uuid', t.task_uuid,
        'namespace', p_namespace_name,
        'task_name', p_task_name,
        'current_state', p_current_state,
        'time_in_state_minutes', p_time_in_state_minutes,
        'threshold_minutes', p_threshold_minutes,
        'snapshot_timestamp', NOW(),
        'task_details', jsonb_build_object(
            'correlation_id', t.correlation_id,
            'priority', t.priority,
            'requested_at', t.requested_at,
            'created_at', t.created_at
        )
    ) INTO v_task_snapshot
    FROM tasks t
    WHERE t.task_uuid = p_task_uuid;

    -- Create DLQ entry
    INSERT INTO tasks_dlq (
        task_uuid,
        original_state,
        dlq_reason,
        task_snapshot,
        metadata
    )
    VALUES (
        p_task_uuid,
        p_current_state,
        p_dlq_reason::dlq_reason,
        v_task_snapshot,
        jsonb_build_object(
            'detection_method', 'automatic_staleness',
            'time_in_state_minutes', p_time_in_state_minutes,
            'threshold_minutes', p_threshold_minutes
        )
    )
    ON CONFLICT (task_uuid) WHERE resolution_status = 'pending'
    DO NOTHING
    RETURNING dlq_entry_uuid INTO v_dlq_entry_uuid;

    RETURN v_dlq_entry_uuid;

EXCEPTION
    WHEN OTHERS THEN
        -- Log error but don't fail the transaction
        RAISE WARNING 'Failed to create DLQ entry for task %: %', p_task_uuid, SQLERRM;
        RETURN NULL;
END;
$$;


--
-- Name: FUNCTION create_dlq_entry(p_task_uuid uuid, p_namespace_name character varying, p_task_name character varying, p_current_state character varying, p_time_in_state_minutes integer, p_threshold_minutes integer, p_dlq_reason character varying); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.create_dlq_entry(p_task_uuid uuid, p_namespace_name character varying, p_task_name character varying, p_current_state character varying, p_time_in_state_minutes integer, p_threshold_minutes integer, p_dlq_reason character varying) IS 'TAS-49 Phase 2: Create DLQ entry for a stale task with comprehensive snapshot.

Atomically creates DLQ investigation entry with task state snapshot.
Uses ON CONFLICT to prevent duplicate pending entries for same task.

Error Handling:
- Catches all exceptions to prevent transaction rollback
- Logs warnings for debugging
- Returns NULL to indicate failure

Called by:
- detect_and_transition_stale_tasks() for each stale task
- Can be called manually for operator-initiated DLQ entries';


--
-- Name: create_step_result_audit(); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.create_step_result_audit() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_task_uuid UUID;
    v_worker_uuid UUID;
    v_correlation_id UUID;
    v_success BOOLEAN;
    v_execution_time_ms BIGINT;
    v_event_json JSONB;
BEGIN
    -- Only audit worker result transitions
    -- These are the two states where workers persist execution results
    IF NEW.to_state NOT IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration') THEN
        RETURN NEW;
    END IF;

    -- Get task_uuid from workflow step (denormalized for query efficiency)
    SELECT ws.task_uuid INTO v_task_uuid
    FROM workflow_steps ws
    WHERE ws.workflow_step_uuid = NEW.workflow_step_uuid;

    -- Extract attribution from transition metadata
    -- These are added by application code in TransitionContext
    BEGIN
        v_worker_uuid := (NEW.metadata->>'worker_uuid')::UUID;
    EXCEPTION WHEN OTHERS THEN
        v_worker_uuid := NULL;
    END;

    BEGIN
        v_correlation_id := (NEW.metadata->>'correlation_id')::UUID;
    EXCEPTION WHEN OTHERS THEN
        v_correlation_id := NULL;
    END;

    -- Parse the event JSON to extract success and execution_time_ms
    -- The event field contains the serialized StepEvent which wraps StepExecutionResult
    BEGIN
        v_event_json := (NEW.metadata->>'event')::JSONB;
        v_success := (v_event_json->>'success')::BOOLEAN;
        v_execution_time_ms := (v_event_json->'metadata'->>'execution_time_ms')::BIGINT;
    EXCEPTION WHEN OTHERS THEN
        v_event_json := NULL;
        v_success := NULL;
        v_execution_time_ms := NULL;
    END;

    -- Fallback: determine success from state if not extractable from event
    IF v_success IS NULL THEN
        v_success := (NEW.to_state = 'enqueued_for_orchestration');
    END IF;

    -- Insert audit record
    INSERT INTO workflow_step_result_audit (
        workflow_step_uuid,
        workflow_step_transition_uuid,
        task_uuid,
        worker_uuid,
        correlation_id,
        success,
        execution_time_ms
    ) VALUES (
        NEW.workflow_step_uuid,
        NEW.workflow_step_transition_uuid,
        v_task_uuid,
        v_worker_uuid,
        v_correlation_id,
        v_success,
        v_execution_time_ms
    );

    RETURN NEW;
END;
$$;


--
-- Name: FUNCTION create_step_result_audit(); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.create_step_result_audit() IS 'Trigger function that creates audit records when workers persist step results. Fires on transitions to enqueued_for_orchestration or enqueued_as_error_for_orchestration states.';


--
-- Name: detect_and_transition_stale_tasks(boolean, integer, integer, integer, integer, integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.detect_and_transition_stale_tasks(p_dry_run boolean DEFAULT true, p_batch_size integer DEFAULT 100, p_default_waiting_deps_threshold integer DEFAULT 60, p_default_waiting_retry_threshold integer DEFAULT 30, p_default_steps_in_process_threshold integer DEFAULT 30, p_default_task_max_lifetime_hours integer DEFAULT 24) RETURNS TABLE(task_uuid uuid, namespace_name character varying, task_name character varying, current_state character varying, time_in_state_minutes integer, staleness_threshold_minutes integer, action_taken character varying, moved_to_dlq boolean, transition_success boolean)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_stale_task RECORD;
    v_dlq_entry_uuid UUID;
    v_transition_success BOOLEAN;
    v_action VARCHAR;
BEGIN
    -- Query stale tasks ONCE using discovery function (O(1) expensive operations)
    FOR v_stale_task IN
        SELECT *
        FROM get_stale_tasks_for_dlq(
            p_default_waiting_deps_threshold,
            p_default_waiting_retry_threshold,
            p_default_steps_in_process_threshold,
            p_default_task_max_lifetime_hours,
            p_batch_size
        )
    LOOP
        IF p_dry_run THEN
            -- Dry run: Report what would happen without making changes
            v_action := 'would_transition_to_dlq_and_error';
            v_dlq_entry_uuid := NULL;
            v_transition_success := FALSE;
        ELSE
            -- Real execution: Create DLQ entry and transition to error
            v_dlq_entry_uuid := create_dlq_entry(
                v_stale_task.task_uuid,
                v_stale_task.namespace_name,
                v_stale_task.task_name,
                v_stale_task.current_state,
                v_stale_task.time_in_state_minutes::INTEGER,
                v_stale_task.threshold_minutes,
                'staleness_timeout'
            );

            v_transition_success := transition_stale_task_to_error(
                v_stale_task.task_uuid,
                v_stale_task.current_state,
                v_stale_task.namespace_name,
                v_stale_task.task_name
            );

            -- Determine action taken based on results
            IF v_dlq_entry_uuid IS NOT NULL AND v_transition_success THEN
                v_action := 'transitioned_to_dlq_and_error';
            ELSIF v_dlq_entry_uuid IS NOT NULL THEN
                v_action := 'moved_to_dlq_only';
            ELSIF v_transition_success THEN
                v_action := 'transitioned_to_error_only';
            ELSE
                v_action := 'transition_failed';
            END IF;
        END IF;

        -- Return row for each processed task
        RETURN QUERY SELECT
            v_stale_task.task_uuid,
            v_stale_task.namespace_name::VARCHAR,
            v_stale_task.task_name::VARCHAR,
            v_stale_task.current_state::VARCHAR,
            v_stale_task.time_in_state_minutes::INTEGER,
            v_stale_task.threshold_minutes,
            v_action,
            (v_dlq_entry_uuid IS NOT NULL),
            COALESCE(v_transition_success, FALSE);
    END LOOP;

    RETURN;
END;
$$;


--
-- Name: FUNCTION detect_and_transition_stale_tasks(p_dry_run boolean, p_batch_size integer, p_default_waiting_deps_threshold integer, p_default_waiting_retry_threshold integer, p_default_steps_in_process_threshold integer, p_default_task_max_lifetime_hours integer); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.detect_and_transition_stale_tasks(p_dry_run boolean, p_batch_size integer, p_default_waiting_deps_threshold integer, p_default_waiting_retry_threshold integer, p_default_steps_in_process_threshold integer, p_default_task_max_lifetime_hours integer) IS 'TAS-49: Main orchestration function for automatic staleness detection and DLQ processing.

Orchestrates the complete staleness detection workflow:
1. Discovery: Call get_stale_tasks_for_dlq() ONCE (O(1) expensive operations)
2. For each stale task:
   a. Create DLQ entry with task snapshot
   b. Transition task to Error state
   c. Track results and errors

Dry Run Mode:
- p_dry_run = true: Report what would happen without making changes
- p_dry_run = false: Actually create DLQ entries and transition tasks';


--
-- Name: evaluate_step_state_readiness(text, boolean, boolean, boolean, boolean, boolean, timestamp without time zone); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.evaluate_step_state_readiness(current_state text, processed boolean, in_process boolean, dependencies_satisfied boolean, retry_eligible boolean, retryable boolean, next_retry_time timestamp without time zone) RETURNS boolean
    LANGUAGE sql STABLE
    AS $$
    SELECT
        COALESCE(current_state, 'pending') IN ('pending', 'waiting_for_retry')
        AND (processed = false OR processed IS NULL)
        AND (in_process = false OR in_process IS NULL)
        AND dependencies_satisfied
        AND retry_eligible
        -- REMOVED: AND retryable (now incorporated in retry_eligible)
        AND (next_retry_time IS NULL OR next_retry_time <= NOW())
$$;


--
-- Name: extract_queue_namespace(text); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.extract_queue_namespace(queue_name text) RETURNS text
    LANGUAGE plpgsql
    AS $_$
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
$_$;


--
-- Name: find_stuck_tasks(integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.find_stuck_tasks(p_timeout_minutes integer DEFAULT 10) RETURNS TABLE(task_uuid uuid, current_state character varying, processor_uuid uuid, stuck_duration_minutes integer)
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN QUERY
    SELECT
        t.task_uuid,
        t.to_state,
        t.processor_uuid,
        EXTRACT(EPOCH FROM (NOW() - t.created_at))::INTEGER / 60 as stuck_duration_minutes
    FROM task_transitions t
    WHERE t.most_recent = true
    AND t.to_state IN (
        'initializing', 'enqueuing_steps',
        'steps_in_process', 'evaluating_results'
    )
    AND t.created_at < NOW() - INTERVAL '1 minute' * p_timeout_minutes;
END;
$$;


--
-- Name: get_analytics_metrics(timestamp with time zone); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_analytics_metrics(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone) RETURNS TABLE(active_tasks_count bigint, total_namespaces_count bigint, unique_task_types_count bigint, system_health_score numeric, task_throughput bigint, completion_count bigint, error_count bigint, completion_rate numeric, error_rate numeric, avg_task_duration numeric, avg_step_duration numeric, step_throughput bigint, analysis_period_start timestamp with time zone, calculated_at timestamp with time zone)
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    -- Set analysis start time (default to 1 hour ago if not provided)
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '1 hour');

    RETURN QUERY
    WITH active_tasks AS (
        SELECT COUNT(DISTINCT t.task_uuid) as active_count
        FROM tasks t
        INNER JOIN workflow_steps ws ON ws.task_uuid = t.task_uuid
        INNER JOIN workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE wst.most_recent = true
          AND wst.to_state NOT IN ('complete', 'error', 'skipped', 'resolved_manually')
    ),
    namespace_summary AS (
        SELECT COUNT(DISTINCT tn.name) as namespace_count
        FROM task_namespaces tn
        INNER JOIN named_tasks nt ON nt.task_namespace_uuid = tn.task_namespace_uuid
        INNER JOIN tasks t ON t.named_task_uuid = nt.named_task_uuid
    ),
    task_type_summary AS (
        SELECT COUNT(DISTINCT nt.name) as task_type_count
        FROM named_tasks nt
        INNER JOIN tasks t ON t.named_task_uuid = nt.named_task_uuid
    ),
    recent_task_health AS (
        SELECT
            COUNT(DISTINCT t.task_uuid) as total_recent_tasks,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE wst.to_state = 'complete' AND wst.most_recent = true
            ) as completed_tasks,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE wst.to_state = 'error' AND wst.most_recent = true
            ) as error_tasks
        FROM tasks t
        INNER JOIN workflow_steps ws ON ws.task_uuid = t.task_uuid
        INNER JOIN workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE t.created_at > NOW() - INTERVAL '1 hour'
    ),
    period_metrics AS (
        SELECT
            COUNT(DISTINCT t.task_uuid) as throughput,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
            ) as completions,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE error_wst.to_state = 'error' AND error_wst.most_recent = true
            ) as errors,
            COUNT(DISTINCT ws.workflow_step_uuid) as step_count,
            AVG(
                CASE
                    WHEN completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
                    THEN EXTRACT(EPOCH FROM (completed_wst.created_at - t.created_at))
                    ELSE NULL
                END
            ) as avg_task_seconds,
            AVG(
                CASE
                    WHEN step_completed.to_state = 'complete' AND step_completed.most_recent = true
                    THEN EXTRACT(EPOCH FROM (step_completed.created_at - ws.created_at))
                    ELSE NULL
                END
            ) as avg_step_seconds
        FROM tasks t
        LEFT JOIN workflow_steps ws ON ws.task_uuid = t.task_uuid
        LEFT JOIN workflow_step_transitions completed_wst ON completed_wst.workflow_step_uuid = ws.workflow_step_uuid
            AND completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
        LEFT JOIN workflow_step_transitions error_wst ON error_wst.workflow_step_uuid = ws.workflow_step_uuid
            AND error_wst.to_state = 'error' AND error_wst.most_recent = true
        LEFT JOIN workflow_step_transitions step_completed ON step_completed.workflow_step_uuid = ws.workflow_step_uuid
            AND step_completed.to_state = 'complete' AND step_completed.most_recent = true
        WHERE t.created_at > analysis_start
    )
    SELECT
        at.active_count,
        ns.namespace_count,
        tts.task_type_count,
        CASE
            WHEN (rth.completed_tasks + rth.error_tasks) > 0
            THEN ROUND((rth.completed_tasks::NUMERIC / (rth.completed_tasks + rth.error_tasks)), 3)
            ELSE 1.0
        END as health_score,
        pm.throughput,
        pm.completions,
        pm.errors,
        CASE
            WHEN pm.throughput > 0
            THEN ROUND((pm.completions::NUMERIC / pm.throughput * 100), 2)
            ELSE 0.0
        END as completion_rate_pct,
        CASE
            WHEN pm.throughput > 0
            THEN ROUND((pm.errors::NUMERIC / pm.throughput * 100), 2)
            ELSE 0.0
        END as error_rate_pct,
        ROUND(COALESCE(pm.avg_task_seconds, 0), 3),
        ROUND(COALESCE(pm.avg_step_seconds, 0), 3),
        pm.step_count,
        analysis_start,
        NOW()
    FROM active_tasks at
    CROSS JOIN namespace_summary ns
    CROSS JOIN task_type_summary tts
    CROSS JOIN recent_task_health rth
    CROSS JOIN period_metrics pm;
END;
$$;


--
-- Name: get_current_task_state(uuid); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_current_task_state(p_task_uuid uuid) RETURNS character varying
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_state VARCHAR;
BEGIN
    SELECT to_state INTO v_state
    FROM task_transitions
    WHERE task_uuid = p_task_uuid
    AND most_recent = true;

    RETURN v_state;
END;
$$;


--
-- Name: get_next_ready_task(); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_next_ready_task() RETURNS TABLE(task_uuid uuid, task_name character varying, priority integer, namespace_name character varying, ready_steps_count bigint, computed_priority numeric, current_state character varying)
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Simply delegate to batch function with limit of 1
    RETURN QUERY
    SELECT * FROM get_next_ready_tasks(1);
END;
$$;


--
-- Name: get_next_ready_tasks(integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_next_ready_tasks(p_limit integer DEFAULT 5) RETURNS TABLE(task_uuid uuid, task_name character varying, priority integer, namespace_name character varying, ready_steps_count bigint, computed_priority numeric, current_state character varying)
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_max_waiting_for_deps_minutes INTEGER;
    v_max_waiting_for_retry_minutes INTEGER;
    v_decay_start_hours NUMERIC;
    v_decay_half_life_hours NUMERIC;
    v_stale_threshold_hours NUMERIC;
    v_minimum_priority NUMERIC;
BEGIN
    -- Configuration parameters (defaults match config/tasker/base/state_machine.toml)
    v_max_waiting_for_deps_minutes := 60;
    v_max_waiting_for_retry_minutes := 30;
    v_decay_start_hours := 1.0;
    v_decay_half_life_hours := 12.0;
    v_stale_threshold_hours := 24.0;
    v_minimum_priority := 0.1;

    RETURN QUERY
    WITH task_candidates AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            t.priority,
            tns.name as namespace_name,
            tt.to_state as current_state,
            tt.created_at as state_entered_at,
            -- TAS-48 Solution 1B: Priority with exponential decay
            CASE
                -- Fresh tasks (<decay_start_hours): Normal age escalation
                WHEN EXTRACT(EPOCH FROM (NOW() - tt.created_at)) < (v_decay_start_hours * 3600) THEN
                    t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1)

                -- Aging tasks (decay_start to stale_threshold): Exponential decay
                WHEN EXTRACT(EPOCH FROM (NOW() - tt.created_at)) < (v_stale_threshold_hours * 3600) THEN
                    t.priority * EXP(-1 * EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / (v_decay_half_life_hours * 3600))

                -- Stale tasks (>stale_threshold): Minimum priority (DLQ candidates)
                ELSE
                    v_minimum_priority
            END as computed_priority
        FROM tasks t
        JOIN task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN named_tasks nt on nt.named_task_uuid = t.named_task_uuid
        JOIN task_namespaces tns on tns.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN task_transitions tt_processing
            ON tt_processing.task_uuid = t.task_uuid
            AND tt_processing.most_recent = true
            AND tt_processing.processor_uuid IS NOT NULL
            AND tt_processing.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
        WHERE tt.most_recent = true
        AND tt.to_state IN ('pending', 'waiting_for_dependencies', 'waiting_for_retry')
        AND tt_processing.task_uuid IS NULL  -- Not already being processed
        -- TAS-48 Solution 1A: Staleness-aware filtering
        AND (
            tt.to_state = 'pending'  -- New tasks always eligible
            OR (
                tt.to_state = 'waiting_for_dependencies'
                AND EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 <= v_max_waiting_for_deps_minutes
            )
            OR (
                tt.to_state = 'waiting_for_retry'
                AND EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 <= v_max_waiting_for_retry_minutes
            )
        )
        ORDER BY computed_priority DESC, t.created_at ASC
        LIMIT p_limit * 10  -- Pre-filter more for batch
    ),
    task_with_context AS (
        SELECT
            tc.task_uuid,
            tc.task_name,
            tc.priority,
            tc.namespace_name,
            ctx.ready_steps,
            tc.computed_priority,
            tc.current_state,
            ctx.execution_status
        FROM task_candidates tc
        CROSS JOIN LATERAL get_task_execution_context(tc.task_uuid) ctx
        -- Pending tasks don't have steps yet, others must have ready steps
        WHERE (tc.current_state = 'pending')
           OR (tc.current_state != 'pending' AND ctx.execution_status = 'has_ready_steps')
    )
    SELECT
        twc.task_uuid,
        twc.task_name,
        twc.priority,
        twc.namespace_name,
        COALESCE(twc.ready_steps, 0) as ready_steps_count,
        twc.computed_priority,
        twc.current_state
    FROM task_with_context twc
    ORDER BY twc.computed_priority DESC
    LIMIT p_limit
    FOR UPDATE SKIP LOCKED;
END;
$$;


--
-- Name: FUNCTION get_next_ready_tasks(p_limit integer); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.get_next_ready_tasks(p_limit integer) IS 'TAS-48: Returns ready tasks with staleness filtering (1A) and exponential priority decay (1B).
Staleness exclusion: Tasks stuck >60min (dependencies) or >30min (retry) excluded from discovery.
Priority decay: Fresh tasks get age escalation, aging tasks decay exponentially (12hr half-life),
stale tasks (>24hr) get minimum priority (0.1) and become DLQ candidates.
Ensures fresh tasks always discoverable regardless of stale task count.';


--
-- Name: get_slowest_steps(timestamp with time zone, integer, text, text, text); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_slowest_steps(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone, limit_count integer DEFAULT 10, namespace_filter text DEFAULT NULL::text, task_name_filter text DEFAULT NULL::text, version_filter text DEFAULT NULL::text) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, step_name character varying, task_name character varying, namespace_name character varying, version character varying, duration_seconds numeric, attempts integer, created_at timestamp without time zone, completed_at timestamp without time zone, retryable boolean, step_status character varying)
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH step_durations AS (
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ns.name as step_name,
            nt.name as task_name,
            tn.name as namespace_name,
            nt.version,
            ws.created_at,
            ws.attempts,
            ws.retryable,
            wst.created_at as completion_time,
            wst.to_state as final_state,
            EXTRACT(EPOCH FROM (wst.created_at - ws.created_at)) as duration_seconds
        FROM workflow_steps ws
        INNER JOIN named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        INNER JOIN tasks t ON t.task_uuid = ws.task_uuid
        INNER JOIN named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        INNER JOIN task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        INNER JOIN workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE ws.created_at > analysis_start
          AND wst.most_recent = true
          AND wst.to_state = 'complete'
          AND (namespace_filter IS NULL OR tn.name = namespace_filter)
          AND (task_name_filter IS NULL OR nt.name = task_name_filter)
          AND (version_filter IS NULL OR nt.version = version_filter)
    )
    SELECT
        sd.workflow_step_uuid,
        sd.task_uuid,
        sd.step_name,
        sd.task_name,
        sd.namespace_name,
        sd.version,
        ROUND(sd.duration_seconds, 3),
        sd.attempts,
        sd.created_at,
        sd.completion_time,
        sd.retryable,
        sd.final_state
    FROM step_durations sd
    WHERE sd.duration_seconds IS NOT NULL
      AND sd.duration_seconds > 0
    ORDER BY sd.duration_seconds DESC
    LIMIT limit_count;
END;
$$;


--
-- Name: get_slowest_tasks(timestamp with time zone, integer, text, text, text); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_slowest_tasks(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone, limit_count integer DEFAULT 10, namespace_filter text DEFAULT NULL::text, task_name_filter text DEFAULT NULL::text, version_filter text DEFAULT NULL::text) RETURNS TABLE(task_uuid uuid, task_name character varying, namespace_name character varying, version character varying, duration_seconds numeric, step_count bigint, completed_steps bigint, error_steps bigint, created_at timestamp without time zone, completed_at timestamp without time zone, initiator character varying, source_system character varying)
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH task_durations AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            tn.name as namespace_name,
            nt.version,
            t.created_at,
            t.initiator,
            t.source_system,
            MAX(wst.created_at) FILTER (
                WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
            ) as latest_completion,
            COUNT(DISTINCT ws.workflow_step_uuid) as step_count,
            COUNT(DISTINCT ws.workflow_step_uuid) FILTER (
                WHERE wst.to_state = 'complete' AND wst.most_recent = true
            ) as completed_steps,
            COUNT(DISTINCT ws.workflow_step_uuid) FILTER (
                WHERE wst.to_state = 'error' AND wst.most_recent = true
            ) as error_steps
        FROM tasks t
        INNER JOIN named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        INNER JOIN task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN workflow_steps ws ON ws.task_uuid = t.task_uuid
        LEFT JOIN workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE t.created_at > analysis_start
          AND (namespace_filter IS NULL OR tn.name = namespace_filter)
          AND (task_name_filter IS NULL OR nt.name = task_name_filter)
          AND (version_filter IS NULL OR nt.version = version_filter)
        GROUP BY t.task_uuid, nt.name, tn.name, nt.version, t.created_at, t.initiator, t.source_system
        HAVING MAX(wst.created_at) FILTER (
            WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
        ) IS NOT NULL
    )
    SELECT
        td.task_uuid,
        td.task_name,
        td.namespace_name,
        td.version,
        ROUND(EXTRACT(EPOCH FROM (td.latest_completion - td.created_at)), 3) as duration_seconds,
        td.step_count,
        td.completed_steps,
        td.error_steps,
        td.created_at,
        td.latest_completion,
        td.initiator,
        td.source_system
    FROM task_durations td
    WHERE EXTRACT(EPOCH FROM (td.latest_completion - td.created_at)) > 0
    ORDER BY EXTRACT(EPOCH FROM (td.latest_completion - td.created_at)) DESC
    LIMIT limit_count;
END;
$$;


--
-- Name: get_stale_tasks_for_dlq(integer, integer, integer, integer, integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_stale_tasks_for_dlq(p_default_waiting_deps integer DEFAULT 60, p_default_waiting_retry integer DEFAULT 30, p_default_steps_process integer DEFAULT 30, p_max_lifetime_hours integer DEFAULT 24, p_batch_size integer DEFAULT 100) RETURNS TABLE(task_uuid uuid, namespace_name character varying, task_name character varying, current_state character varying, time_in_state_minutes numeric, threshold_minutes integer, task_age_minutes numeric)
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN QUERY
    SELECT
        tsa.task_uuid,
        tsa.namespace_name,
        tsa.task_name,
        tsa.current_state,
        tsa.time_in_state_minutes,
        calculate_staleness_threshold(
            tsa.current_state,
            tsa.template_config,
            p_default_waiting_deps,
            p_default_waiting_retry,
            p_default_steps_process
        ) as threshold_minutes,
        tsa.task_age_minutes
    FROM v_task_state_analysis tsa
    LEFT JOIN tasks_dlq dlq
        ON dlq.task_uuid = tsa.task_uuid
        AND dlq.resolution_status = 'pending'
    WHERE
        -- Only consider states that can become stale
        tsa.current_state IN ('waiting_for_dependencies', 'waiting_for_retry', 'steps_in_process')
        -- Not already in DLQ (LEFT JOIN + IS NULL pattern = anti-join optimization)
        AND dlq.task_uuid IS NULL
        -- Either: Exceeded state-specific threshold OR exceeded max task lifetime
        AND (
            tsa.time_in_state_minutes >= calculate_staleness_threshold(
                tsa.current_state,
                tsa.template_config,
                p_default_waiting_deps,
                p_default_waiting_retry,
                p_default_steps_process
            )
            OR tsa.task_age_minutes >= (p_max_lifetime_hours * 60)
        )
    ORDER BY tsa.time_in_state_minutes DESC
    LIMIT p_batch_size;
END;
$$;


--
-- Name: FUNCTION get_stale_tasks_for_dlq(p_default_waiting_deps integer, p_default_waiting_retry integer, p_default_steps_process integer, p_max_lifetime_hours integer, p_batch_size integer); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.get_stale_tasks_for_dlq(p_default_waiting_deps integer, p_default_waiting_retry integer, p_default_steps_process integer, p_max_lifetime_hours integer, p_batch_size integer) IS 'TAS-49 Phase 2: Discover stale tasks exceeding thresholds with O(1) optimization.

Key Performance Feature:
- Queries v_task_state_analysis base view ONCE (all expensive joins)
- Main detection function calls this ONCE before loop, not inside loop
- Achieves O(1) expensive operations instead of O(n) per-iteration queries';


--
-- Name: get_step_readiness_status(uuid, uuid[]); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_step_readiness_status(input_task_uuid uuid, step_uuids uuid[] DEFAULT NULL::uuid[]) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, named_step_uuid uuid, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, max_attempts integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  -- DELEGATION: Use batch function with single-element array for DRY principle
  RETURN QUERY
  SELECT * FROM get_step_readiness_status_batch(ARRAY[input_task_uuid], step_uuids);
END;
$$;


--
-- Name: FUNCTION get_step_readiness_status(input_task_uuid uuid, step_uuids uuid[]); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.get_step_readiness_status(input_task_uuid uuid, step_uuids uuid[]) IS 'Optimized step readiness analysis function with task-scoped CTEs for performance.

PERFORMANCE OPTIMIZATIONS:
- task_steps CTE eliminates table scans by creating focused working set
- All subsequent CTEs use INNER JOINs to task_steps for efficient filtering
- Restored last_failures CTE for accurate failure time tracking
- dependency_counts uses INNER JOIN instead of inefficient subselect';


--
-- Name: get_step_readiness_status_batch(uuid[], uuid[]); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_step_readiness_status_batch(input_task_uuids uuid[], step_uuids uuid[] DEFAULT NULL::uuid[]) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, named_step_uuid uuid, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, max_attempts integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH
  -- OPTIMIZATION: Create batch task-scoped working set to eliminate table scans
  steps_for_tasks AS (
    SELECT ws.workflow_step_uuid, ws.task_uuid
    FROM workflow_steps ws
    WHERE ws.task_uuid = ANY(input_task_uuids)
      AND (step_uuids IS NULL OR ws.workflow_step_uuid = ANY(step_uuids))
  ),

  -- OPTIMIZATION: Scope step_states to steps_for_tasks working set
  step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM workflow_step_transitions wst
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = wst.workflow_step_uuid
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),

  -- RESTORED: Dedicated last_failures CTE for clarity and accuracy
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM workflow_step_transitions wst
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = wst.workflow_step_uuid
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),

  -- OPTIMIZATION: Scoped dependency_counts using INNER JOIN instead of subselect
  dependency_counts AS (
    SELECT
      e.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_ss.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM workflow_step_edges e
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = e.to_step_uuid
    LEFT JOIN step_states parent_ss ON parent_ss.workflow_step_uuid = e.from_step_uuid
    GROUP BY e.to_step_uuid
  ),

  -- Centralized retry time calculation using helper function
  retry_times AS (
    SELECT
      ws.workflow_step_uuid,
      calculate_step_next_retry_time(
          ws.backoff_request_seconds,
          ws.last_attempted_at,
          lf.failure_time,
          ws.attempts
      ) as next_retry_time
    FROM workflow_steps ws
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = ws.workflow_step_uuid
    LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  ),

  -- Pre-computed boolean flags with corrected retry_eligible logic
  step_readiness_flags AS (
    SELECT
      ws.workflow_step_uuid,
      (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps) as dependencies_satisfied,
      -- First attempt (attempts=0) is always eligible
      -- Retry attempts require retryable=true AND attempts < max_attempts
      (
        COALESCE(ws.attempts, 0) = 0  -- First attempt always eligible
        OR (
          COALESCE(ws.retryable, true) = true  -- Must be retryable for retries
          AND COALESCE(ws.attempts, 0) < COALESCE(ws.max_attempts, 3)
        )
      ) as retry_eligible
    FROM workflow_steps ws
    INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = ws.workflow_step_uuid
    LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  )

  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied (pre-computed in CTE)
    srf.dependencies_satisfied,

    -- Retry eligible (pre-computed with corrected logic in CTE)
    srf.retry_eligible,

    -- Ready for execution (uses helper function with pre-computed values)
    evaluate_step_state_readiness(
        COALESCE(ss.to_state, 'pending'),
        ws.processed,
        ws.in_process,
        srf.dependencies_satisfied,
        srf.retry_eligible,
        COALESCE(ws.retryable, true),
        rt.next_retry_time
    ) as ready_for_execution,

    -- Use dedicated last_failures CTE for accurate failure time
    lf.failure_time as last_failure_at,

    -- Next retry time (already calculated in retry_times CTE)
    rt.next_retry_time as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.max_attempts, 3) as max_attempts,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM workflow_steps ws
  INNER JOIN steps_for_tasks sft ON sft.workflow_step_uuid = ws.workflow_step_uuid
  JOIN named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN retry_times rt ON rt.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN step_readiness_flags srf ON srf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = ANY(input_task_uuids)
    AND (step_uuids IS NULL OR ws.workflow_step_uuid = ANY(step_uuids))
  ORDER BY ws.task_uuid, ws.workflow_step_uuid;
END;
$$;


--
-- Name: FUNCTION get_step_readiness_status_batch(input_task_uuids uuid[], step_uuids uuid[]); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.get_step_readiness_status_batch(input_task_uuids uuid[], step_uuids uuid[]) IS 'Batch step readiness with corrected retry_eligible logic.

BUG FIX (2025-10-06):
- First attempt (attempts=0) is now always eligible regardless of retry settings
- Retry attempts (attempts>0) properly check retryable=true AND attempts < max_attempts
- Fixes issue where max_attempts=0, retryable=false blocked first execution';


--
-- Name: get_step_transitive_dependencies(uuid); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_step_transitive_dependencies(target_step_uuid uuid) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, named_step_uuid uuid, step_name character varying, results jsonb, processed boolean, distance integer)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
    RETURN QUERY
    WITH RECURSIVE transitive_deps AS (
        -- Base case: direct parents of the target step
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ws.named_step_uuid,
            ns.name as step_name,
            ws.results,
            ws.processed,
            1 as distance
        FROM workflow_step_edges wse
        JOIN workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        WHERE wse.to_step_uuid = target_step_uuid

        UNION ALL

        -- Recursive case: parents of parents
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ws.named_step_uuid,
            ns.name as step_name,
            ws.results,
            ws.processed,
            td.distance + 1
        FROM transitive_deps td
        JOIN workflow_step_edges wse ON wse.to_step_uuid = td.workflow_step_uuid
        JOIN workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        WHERE td.distance < 50  -- Prevent infinite recursion
    )
    SELECT
        td.workflow_step_uuid,
        td.task_uuid,
        td.named_step_uuid,
        td.step_name,
        td.results,
        td.processed,
        td.distance
    FROM transitive_deps td
    ORDER BY td.distance ASC, td.workflow_step_uuid ASC;
END;
$$;


--
-- Name: get_system_health_counts(); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_system_health_counts() RETURNS TABLE(pending_tasks bigint, initializing_tasks bigint, enqueuing_steps_tasks bigint, steps_in_process_tasks bigint, evaluating_results_tasks bigint, waiting_for_dependencies_tasks bigint, waiting_for_retry_tasks bigint, blocked_by_failures_tasks bigint, complete_tasks bigint, error_tasks bigint, cancelled_tasks bigint, resolved_manually_tasks bigint, total_tasks bigint, pending_steps bigint, enqueued_steps bigint, in_progress_steps bigint, enqueued_for_orchestration_steps bigint, enqueued_as_error_for_orchestration_steps bigint, waiting_for_retry_steps bigint, complete_steps bigint, error_steps bigint, cancelled_steps bigint, resolved_manually_steps bigint, total_steps bigint)
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN QUERY
    WITH task_counts AS (
        SELECT
            COUNT(*) FILTER (WHERE task_state.to_state = 'pending') as pending_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'initializing') as initializing_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'enqueuing_steps') as enqueuing_steps_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'steps_in_process') as steps_in_process_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'evaluating_results') as evaluating_results_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'waiting_for_dependencies') as waiting_for_dependencies_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'waiting_for_retry') as waiting_for_retry_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'blocked_by_failures') as blocked_by_failures_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'complete') as complete_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'error') as error_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'cancelled') as cancelled_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'resolved_manually') as resolved_manually_tasks,
            COUNT(*) as total_tasks
        FROM tasks t
        LEFT JOIN task_transitions task_state ON task_state.task_uuid = t.task_uuid AND task_state.most_recent = true
    ),
    step_counts AS (
        SELECT
            COUNT(*) FILTER (WHERE step_state.to_state = 'pending') as pending_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'enqueued') as enqueued_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'in_progress') as in_progress_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'enqueued_for_orchestration') as enqueued_for_orchestration_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'enqueued_as_error_for_orchestration') as enqueued_as_error_for_orchestration_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'waiting_for_retry') as waiting_for_retry_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'complete') as complete_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'error') as error_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'cancelled') as cancelled_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'resolved_manually') as resolved_manually_steps,
            COUNT(*) as total_steps
        FROM workflow_steps s
        LEFT JOIN workflow_step_transitions step_state ON step_state.workflow_step_uuid = s.workflow_step_uuid AND step_state.most_recent = true
    )
    SELECT
        task_counts.*,
        step_counts.*
    FROM task_counts, step_counts;
END;
$$;


--
-- Name: get_task_execution_context(uuid); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_task_execution_context(input_task_uuid uuid) RETURNS TABLE(task_uuid uuid, named_task_uuid uuid, status text, total_steps bigint, pending_steps bigint, in_progress_steps bigint, completed_steps bigint, failed_steps bigint, ready_steps bigint, execution_status text, recommended_action text, completion_percentage numeric, health_status text, enqueued_steps bigint)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    SELECT * FROM get_step_readiness_status(input_task_uuid, NULL)
  ),
  task_info AS (
    SELECT
      t.task_uuid,
      t.named_task_uuid,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM tasks t
    LEFT JOIN task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
    WHERE t.task_uuid = input_task_uuid
  ),
  aggregated_stats AS (
    SELECT
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration') THEN 1 END) as enqueued_for_orchestration_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      -- Count permanently blocked steps
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.max_attempts OR sd.retry_eligible = false) THEN 1 END) as permanently_blocked_steps
    FROM step_data sd
  )
  SELECT
    ti.task_uuid,
    ti.named_task_uuid,
    ti.current_status as status,

    -- Step Statistics
    COALESCE(ast.total_steps, 0) as total_steps,
    COALESCE(ast.pending_steps, 0) as pending_steps,
    COALESCE(ast.in_progress_steps, 0) as in_progress_steps,
    COALESCE(ast.completed_steps, 0) as completed_steps,
    COALESCE(ast.failed_steps, 0) as failed_steps,
    COALESCE(ast.ready_steps, 0) as ready_steps,

    -- Execution State Logic - blocked_by_failures takes priority over has_ready_steps
    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0
           OR COALESCE(ast.enqueued_steps, 0) > 0
           OR COALESCE(ast.enqueued_for_orchestration_steps, 0) > 0 THEN 'processing'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- Recommended Action Logic
    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'handle_failures'
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Progress Metrics
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    -- Health Status Logic
    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked'
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    -- Enqueued steps
    COALESCE(ast.enqueued_steps, 0) + COALESCE(ast.enqueued_for_orchestration_steps, 0) as enqueued_steps

  FROM task_info ti
  CROSS JOIN aggregated_stats ast;
END;
$$;


--
-- Name: get_task_execution_contexts_batch(uuid[]); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_task_execution_contexts_batch(input_task_uuids uuid[]) RETURNS TABLE(task_uuid uuid, named_task_uuid uuid, status text, total_steps bigint, pending_steps bigint, in_progress_steps bigint, completed_steps bigint, failed_steps bigint, ready_steps bigint, execution_status text, recommended_action text, completion_percentage numeric, health_status text, enqueued_steps bigint)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    SELECT srs.*
    FROM unnest(input_task_uuids) AS task_uuid_list(task_uuid)
    CROSS JOIN LATERAL get_step_readiness_status(task_uuid_list.task_uuid, NULL) srs
  ),
  task_info AS (
    SELECT
      t.task_uuid,
      t.named_task_uuid,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM tasks t
    LEFT JOIN task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
    WHERE t.task_uuid = ANY(input_task_uuids)
  ),
  aggregated_stats AS (
    SELECT
      sd.task_uuid,
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('enqueued_for_orchestration', 'enqueued_as_error_for_orchestration') THEN 1 END) as enqueued_for_orchestration_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.max_attempts OR sd.retry_eligible = false) THEN 1 END) as permanently_blocked_steps
    FROM step_data sd
    GROUP BY sd.task_uuid
  )
  SELECT
    ti.task_uuid,
    ti.named_task_uuid,
    ti.current_status as status,

    COALESCE(ast.total_steps, 0) as total_steps,
    COALESCE(ast.pending_steps, 0) as pending_steps,
    COALESCE(ast.in_progress_steps, 0) as in_progress_steps,
    COALESCE(ast.completed_steps, 0) as completed_steps,
    COALESCE(ast.failed_steps, 0) as failed_steps,
    COALESCE(ast.ready_steps, 0) as ready_steps,

    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0
           OR COALESCE(ast.enqueued_steps, 0) > 0
           OR COALESCE(ast.enqueued_for_orchestration_steps, 0) > 0 THEN 'processing'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'handle_failures'
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    CASE
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'blocked'
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    COALESCE(ast.enqueued_steps, 0) + COALESCE(ast.enqueued_for_orchestration_steps, 0) as enqueued_steps

  FROM task_info ti
  LEFT JOIN aggregated_stats ast ON ast.task_uuid = ti.task_uuid;
END;
$$;


--
-- Name: get_task_ready_info(uuid); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.get_task_ready_info(p_task_uuid uuid) RETURNS TABLE(task_uuid uuid, task_name character varying, priority integer, namespace_name character varying, ready_steps_count bigint, computed_priority numeric, current_state character varying)
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN QUERY
    WITH task_candidate AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            t.priority,
            tns.name as namespace_name,
            tt.to_state as current_state,
            -- Compute priority with age escalation
            t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1) as computed_priority
        FROM tasks t
        JOIN task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN named_tasks nt on nt.named_task_uuid = t.named_task_uuid
        JOIN task_namespaces tns on tns.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN task_transitions tt_processing
            ON tt_processing.task_uuid = t.task_uuid
            AND tt_processing.most_recent = true
            AND tt_processing.processor_uuid IS NOT NULL
            AND tt_processing.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
        WHERE tt.most_recent = true
        AND t.task_uuid = p_task_uuid
    ),
    task_with_context AS (
        SELECT
            tc.*,
            ctx.ready_steps,
            ctx.execution_status
        FROM task_candidate tc
        CROSS JOIN LATERAL get_task_execution_context(p_task_uuid) ctx
    )
    SELECT
        twc.task_uuid,
        twc.task_name,
        twc.priority,
        twc.namespace_name,
        COALESCE(twc.ready_steps, 0) as ready_steps_count,
        twc.computed_priority,
        twc.current_state
    FROM task_with_context twc
    ORDER BY twc.computed_priority DESC;
END;
$$;


--
-- Name: pgmq_auto_add_headers_trigger(); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_auto_add_headers_trigger() RETURNS event_trigger
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: FUNCTION pgmq_auto_add_headers_trigger(); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.pgmq_auto_add_headers_trigger() IS 'Event trigger function to automatically add headers column to new pgmq queue tables';


--
-- Name: pgmq_delete_specific_message(text, bigint); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_delete_specific_message(queue_name text, target_msg_id bigint) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_ensure_headers_column(text); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_ensure_headers_column(queue_name text) RETURNS void
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: FUNCTION pgmq_ensure_headers_column(queue_name text); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.pgmq_ensure_headers_column(queue_name text) IS 'Ensures a pgmq queue table has the headers JSONB column required by pgmq extension v1.5.1+';


--
-- Name: pgmq_extend_vt_specific_message(text, bigint, integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_extend_vt_specific_message(queue_name text, target_msg_id bigint, additional_vt_seconds integer DEFAULT 30) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_notify_queue_created(); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_notify_queue_created() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_read_specific_message(text, bigint, integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_read_specific_message(queue_name text, target_msg_id bigint, vt_seconds integer DEFAULT 30) RETURNS TABLE(msg_id bigint, read_ct integer, enqueued_at timestamp with time zone, vt timestamp with time zone, message jsonb)
    LANGUAGE plpgsql
    AS $$
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

    -- Construct table names with PGMQ's naming convention
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

    RETURN;
END;
$$;


--
-- Name: pgmq_send_batch_with_notify(text, jsonb[], integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_send_batch_with_notify(queue_name text, messages jsonb[], delay_seconds integer DEFAULT 0) RETURNS SETOF bigint
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: pgmq_send_with_notify(text, jsonb, integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.pgmq_send_with_notify(queue_name text, message jsonb, delay_seconds integer DEFAULT 0) RETURNS bigint
    LANGUAGE plpgsql
    AS $$
DECLARE
    msg_id BIGINT;
    namespace_name TEXT;
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
BEGIN
    -- Send message using PGMQ's native function
    SELECT pgmq.send(queue_name, message, delay_seconds) INTO msg_id;

    -- Extract namespace from queue name using robust helper
    namespace_name := extract_queue_namespace(queue_name);

    -- Build namespace-specific channel name
    namespace_channel := 'pgmq_message_ready.' || namespace_name;

    -- Build event payload
    event_payload := json_build_object(
        'event_type', 'message_ready',
        'msg_id', msg_id,
        'queue_name', queue_name,
        'namespace', namespace_name,
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

    RETURN msg_id;
END;
$$;


--
-- Name: set_step_backoff_atomic(uuid, integer); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.set_step_backoff_atomic(p_step_uuid uuid, p_backoff_seconds integer) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Update with implicit row-level locking via UPDATE statement
    UPDATE workflow_steps
    SET
        backoff_request_seconds = p_backoff_seconds,
        last_attempted_at = NOW(),
        updated_at = NOW()
    WHERE workflow_step_uuid = p_step_uuid;

    -- FOUND is a special PL/pgSQL variable that's true if UPDATE affected any rows
    RETURN FOUND;
END;
$$;


--
-- Name: transition_stale_task_to_error(uuid, character varying, character varying, character varying); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.transition_stale_task_to_error(p_task_uuid uuid, p_current_state character varying, p_namespace_name character varying, p_task_name character varying) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_success BOOLEAN;
BEGIN
    -- Attempt state transition using atomic function
    v_success := transition_task_state_atomic(
        p_task_uuid,
        p_current_state,  -- from_state (for validation)
        'error',          -- to_state
        NULL,             -- processor_uuid (audit only, not enforced per TAS-54)
        jsonb_build_object(
            'reason', 'staleness_timeout',
            'namespace', p_namespace_name,
            'task_name', p_task_name,
            'detected_at', NOW()
        )
    );

    RETURN v_success;

EXCEPTION
    WHEN OTHERS THEN
        -- Log error but don't fail the transaction
        RAISE WARNING 'Failed to transition task % to error: %', p_task_uuid, SQLERRM;
        RETURN FALSE;
END;
$$;


--
-- Name: FUNCTION transition_stale_task_to_error(p_task_uuid uuid, p_current_state character varying, p_namespace_name character varying, p_task_name character varying); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.transition_stale_task_to_error(p_task_uuid uuid, p_current_state character varying, p_namespace_name character varying, p_task_name character varying) IS 'TAS-49 Phase 2: Transition stale task to Error state with proper error handling.

Wraps transition_task_state_atomic() with staleness-specific context and
error handling. Prevents transaction rollback on transition failures.';


--
-- Name: transition_task_state_atomic(uuid, character varying, character varying, uuid, jsonb); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.transition_task_state_atomic(p_task_uuid uuid, p_from_state character varying, p_to_state character varying, p_processor_uuid uuid, p_metadata jsonb DEFAULT '{}'::jsonb) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_sort_key INTEGER;
    v_row_count INTEGER;
BEGIN
    -- Get next sort key
    SELECT COALESCE(MAX(sort_key), 0) + 1 INTO v_sort_key
    FROM task_transitions
    WHERE task_uuid = p_task_uuid;

    -- Atomically transition only if in expected state
    -- TAS-54: Ownership checking removed - processor_uuid tracked for audit only
    WITH current_state AS (
        SELECT to_state
        FROM task_transitions
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        FOR UPDATE
    ),
    do_update AS (
        UPDATE task_transitions
        SET most_recent = false
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        AND EXISTS (
            SELECT 1 FROM current_state
            WHERE to_state = p_from_state
        )
        RETURNING task_uuid
    )
    INSERT INTO task_transitions (
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
    RETURN v_row_count > 0;
END;
$$;


--
-- Name: FUNCTION transition_task_state_atomic(p_task_uuid uuid, p_from_state character varying, p_to_state character varying, p_processor_uuid uuid, p_metadata jsonb); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.transition_task_state_atomic(p_task_uuid uuid, p_from_state character varying, p_to_state character varying, p_processor_uuid uuid, p_metadata jsonb) IS 'TAS-41/TAS-49/TAS-54: Atomic state transition without ownership enforcement.

Performs compare-and-swap atomic state transition based solely on current state.
Processor UUID is stored in transitions table for audit trail and debugging.

Returns:
- TRUE if transition succeeded (current state matched p_from_state)
- FALSE if state mismatch (task not in expected state)';


--
-- Name: update_updated_at_column(); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.update_updated_at_column() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;


--
-- Name: FUNCTION update_updated_at_column(); Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON FUNCTION tasker.update_updated_at_column() IS 'TAS-49: Trigger function to automatically update updated_at timestamp on row modification';


--
-- Name: uuid_generate_v7(); Type: FUNCTION; Schema: tasker; Owner: -
--

CREATE FUNCTION tasker.uuid_generate_v7() RETURNS uuid
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$ SELECT uuidv7(); $$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: annotation_types; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.annotation_types (
    annotation_type_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: annotation_types_annotation_type_id_seq; Type: SEQUENCE; Schema: tasker; Owner: -
--

CREATE SEQUENCE tasker.annotation_types_annotation_type_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: annotation_types_annotation_type_id_seq; Type: SEQUENCE OWNED BY; Schema: tasker; Owner: -
--

ALTER SEQUENCE tasker.annotation_types_annotation_type_id_seq OWNED BY tasker.annotation_types.annotation_type_id;


--
-- Name: dependent_system_object_maps; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.dependent_system_object_maps (
    dependent_system_object_map_id bigint CONSTRAINT dependent_system_object_map_dependent_system_object_ma_not_null NOT NULL,
    dependent_system_one_uuid uuid NOT NULL,
    dependent_system_two_uuid uuid NOT NULL,
    remote_id_one character varying(128) NOT NULL,
    remote_id_two character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: dependent_system_objec_dependent_system_object_map_i_seq; Type: SEQUENCE; Schema: tasker; Owner: -
--

CREATE SEQUENCE tasker.dependent_system_objec_dependent_system_object_map_i_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: dependent_system_objec_dependent_system_object_map_i_seq; Type: SEQUENCE OWNED BY; Schema: tasker; Owner: -
--

ALTER SEQUENCE tasker.dependent_system_objec_dependent_system_object_map_i_seq OWNED BY tasker.dependent_system_object_maps.dependent_system_object_map_id;


--
-- Name: dependent_systems; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.dependent_systems (
    dependent_system_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: named_steps; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.named_steps (
    named_step_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    dependent_system_uuid uuid NOT NULL,
    name character varying(128) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: named_tasks; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.named_tasks (
    named_task_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    task_namespace_uuid uuid NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    version character varying(16) DEFAULT '0.1.0'::character varying NOT NULL,
    configuration jsonb DEFAULT '{}'::jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: named_tasks_named_steps; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.named_tasks_named_steps (
    ntns_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    named_task_uuid uuid NOT NULL,
    named_step_uuid uuid NOT NULL,
    skippable boolean DEFAULT false NOT NULL,
    default_retryable boolean DEFAULT true NOT NULL,
    default_max_attempts integer DEFAULT 3 NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: workflow_step_edges; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_step_edges (
    workflow_step_edge_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    from_step_uuid uuid NOT NULL,
    to_step_uuid uuid NOT NULL,
    name character varying NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: workflow_steps; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_steps (
    workflow_step_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    named_step_uuid uuid NOT NULL,
    retryable boolean DEFAULT true NOT NULL,
    max_attempts integer DEFAULT 3,
    in_process boolean DEFAULT false NOT NULL,
    processed boolean DEFAULT false NOT NULL,
    processed_at timestamp without time zone,
    attempts integer,
    last_attempted_at timestamp without time zone,
    backoff_request_seconds integer,
    inputs jsonb,
    results jsonb,
    skippable boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    checkpoint jsonb
);


--
-- Name: COLUMN workflow_steps.checkpoint; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_steps.checkpoint IS 'Handler-driven checkpoint data for batch processing resumability (TAS-125)';


--
-- Name: step_dag_relationships; Type: VIEW; Schema: tasker; Owner: -
--

CREATE VIEW tasker.step_dag_relationships AS
 SELECT ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    COALESCE(parent_data.parent_uuids, '[]'::jsonb) AS parent_step_uuids,
    COALESCE(child_data.child_uuids, '[]'::jsonb) AS child_step_uuids,
    COALESCE(parent_data.parent_count, (0)::bigint) AS parent_count,
    COALESCE(child_data.child_count, (0)::bigint) AS child_count,
        CASE
            WHEN (COALESCE(parent_data.parent_count, (0)::bigint) = 0) THEN true
            ELSE false
        END AS is_root_step,
        CASE
            WHEN (COALESCE(child_data.child_count, (0)::bigint) = 0) THEN true
            ELSE false
        END AS is_leaf_step,
    depth_info.min_depth_from_root
   FROM (((tasker.workflow_steps ws
     LEFT JOIN ( SELECT workflow_step_edges.to_step_uuid,
            jsonb_agg(workflow_step_edges.from_step_uuid ORDER BY workflow_step_edges.from_step_uuid) AS parent_uuids,
            count(*) AS parent_count
           FROM tasker.workflow_step_edges
          GROUP BY workflow_step_edges.to_step_uuid) parent_data ON ((parent_data.to_step_uuid = ws.workflow_step_uuid)))
     LEFT JOIN ( SELECT workflow_step_edges.from_step_uuid,
            jsonb_agg(workflow_step_edges.to_step_uuid ORDER BY workflow_step_edges.to_step_uuid) AS child_uuids,
            count(*) AS child_count
           FROM tasker.workflow_step_edges
          GROUP BY workflow_step_edges.from_step_uuid) child_data ON ((child_data.from_step_uuid = ws.workflow_step_uuid)))
     LEFT JOIN ( WITH RECURSIVE step_depths AS (
                 SELECT ws_inner.workflow_step_uuid,
                    0 AS depth_from_root,
                    ws_inner.task_uuid
                   FROM tasker.workflow_steps ws_inner
                  WHERE (NOT (EXISTS ( SELECT 1
                           FROM tasker.workflow_step_edges e
                          WHERE (e.to_step_uuid = ws_inner.workflow_step_uuid))))
                UNION ALL
                 SELECT e.to_step_uuid,
                    (sd.depth_from_root + 1),
                    sd.task_uuid
                   FROM (step_depths sd
                     JOIN tasker.workflow_step_edges e ON ((e.from_step_uuid = sd.workflow_step_uuid)))
                  WHERE (sd.depth_from_root < 50)
                )
         SELECT step_depths.workflow_step_uuid,
            min(step_depths.depth_from_root) AS min_depth_from_root
           FROM step_depths
          GROUP BY step_depths.workflow_step_uuid) depth_info ON ((depth_info.workflow_step_uuid = ws.workflow_step_uuid)));


--
-- Name: task_annotations; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.task_annotations (
    task_annotation_id bigint NOT NULL,
    task_uuid uuid NOT NULL,
    annotation_type_id integer NOT NULL,
    annotation jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: task_annotations_task_annotation_id_seq; Type: SEQUENCE; Schema: tasker; Owner: -
--

CREATE SEQUENCE tasker.task_annotations_task_annotation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: task_annotations_task_annotation_id_seq; Type: SEQUENCE OWNED BY; Schema: tasker; Owner: -
--

ALTER SEQUENCE tasker.task_annotations_task_annotation_id_seq OWNED BY tasker.task_annotations.task_annotation_id;


--
-- Name: task_namespaces; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.task_namespaces (
    task_namespace_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: task_transitions; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.task_transitions (
    task_transition_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    processor_uuid uuid,
    transition_metadata jsonb DEFAULT '{}'::jsonb,
    CONSTRAINT chk_task_transitions_from_state CHECK (((from_state IS NULL) OR ((from_state)::text = ANY (ARRAY[('pending'::character varying)::text, ('initializing'::character varying)::text, ('enqueuing_steps'::character varying)::text, ('steps_in_process'::character varying)::text, ('evaluating_results'::character varying)::text, ('waiting_for_dependencies'::character varying)::text, ('waiting_for_retry'::character varying)::text, ('blocked_by_failures'::character varying)::text, ('complete'::character varying)::text, ('error'::character varying)::text, ('cancelled'::character varying)::text, ('resolved_manually'::character varying)::text])))),
    CONSTRAINT chk_task_transitions_to_state CHECK (((to_state)::text = ANY (ARRAY[('pending'::character varying)::text, ('initializing'::character varying)::text, ('enqueuing_steps'::character varying)::text, ('steps_in_process'::character varying)::text, ('evaluating_results'::character varying)::text, ('waiting_for_dependencies'::character varying)::text, ('waiting_for_retry'::character varying)::text, ('blocked_by_failures'::character varying)::text, ('complete'::character varying)::text, ('error'::character varying)::text, ('cancelled'::character varying)::text, ('resolved_manually'::character varying)::text])))
);


--
-- Name: tasks; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.tasks (
    task_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    named_task_uuid uuid NOT NULL,
    complete boolean DEFAULT false NOT NULL,
    requested_at timestamp without time zone NOT NULL,
    completed_at timestamp without time zone,
    initiator character varying(128),
    source_system character varying(128),
    reason character varying(128),
    bypass_steps json,
    tags jsonb,
    context jsonb,
    identity_hash character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    correlation_id uuid NOT NULL,
    parent_correlation_id uuid
);


--
-- Name: COLUMN tasks.correlation_id; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks.correlation_id IS 'UUID v7 correlation ID for distributed tracing (NOT NULL). Auto-generated if not provided at task creation. Enables end-to-end request tracking across orchestration and workers.';


--
-- Name: COLUMN tasks.parent_correlation_id; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks.parent_correlation_id IS 'Optional correlation ID of parent workflow. Enables tracking of nested/chained workflow relationships.';


--
-- Name: tasks_dlq; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.tasks_dlq (
    dlq_entry_uuid uuid DEFAULT tasker.uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    original_state character varying(50) NOT NULL,
    dlq_reason tasker.dlq_reason NOT NULL,
    dlq_timestamp timestamp without time zone DEFAULT now() NOT NULL,
    resolution_status tasker.dlq_resolution_status DEFAULT 'pending'::tasker.dlq_resolution_status NOT NULL,
    resolution_timestamp timestamp without time zone,
    resolution_notes text,
    resolved_by character varying(255),
    task_snapshot jsonb NOT NULL,
    metadata jsonb,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE tasks_dlq; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON TABLE tasker.tasks_dlq IS 'TAS-49: Dead Letter Queue investigation tracking and audit trail.

Architecture: DLQ is an INVESTIGATION TRACKER, not a task manipulation layer
- Tasks remain in tasks table (typically in Error state)
- DLQ entries track "why stuck" and "what operator did"
- Multiple DLQ entries per task allowed (historical trail across multiple instances)
- Only one "pending" investigation per task at a time

Workflow Example:
1. Task stuck 60min → staleness detector → task Error state + DLQ entry (pending)
2. Operator investigates via DLQ API → reviews task_snapshot JSONB
3. Operator fixes problem steps using existing step APIs (PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid})
4. Task state machine automatically progresses when steps fixed
5. Operator updates DLQ entry to track resolution (PATCH /api/v1/dlq/{dlq_entry_uuid})

Query patterns:
- Active investigations: WHERE resolution_status = ''pending'' ORDER BY dlq_timestamp
- DLQ history for task: WHERE task_uuid = $1 ORDER BY created_at
- Pattern analysis: GROUP BY dlq_reason to identify systemic issues';


--
-- Name: COLUMN tasks_dlq.task_snapshot; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks_dlq.task_snapshot IS 'Complete snapshot of task state when moved to DLQ.
Includes task details, steps, transitions, and execution context.
Enables investigation without modifying main task tables.';


--
-- Name: COLUMN tasks_dlq.metadata; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks_dlq.metadata IS 'Additional context about DLQ entry.
Examples: error stack trace, detection method, related tasks, investigation notes.';


--
-- Name: v_dlq_dashboard; Type: VIEW; Schema: tasker; Owner: -
--

CREATE VIEW tasker.v_dlq_dashboard AS
 SELECT dlq_reason,
    count(*) AS total_entries,
    count(*) FILTER (WHERE (resolution_status = 'pending'::tasker.dlq_resolution_status)) AS pending,
    count(*) FILTER (WHERE (resolution_status = 'manually_resolved'::tasker.dlq_resolution_status)) AS manually_resolved,
    count(*) FILTER (WHERE (resolution_status = 'permanently_failed'::tasker.dlq_resolution_status)) AS permanent_failures,
    count(*) FILTER (WHERE (resolution_status = 'cancelled'::tasker.dlq_resolution_status)) AS cancelled,
    min(dlq_timestamp) AS oldest_entry,
    max(dlq_timestamp) AS newest_entry,
    avg((EXTRACT(epoch FROM (COALESCE((resolution_timestamp)::timestamp with time zone, now()) - (dlq_timestamp)::timestamp with time zone)) / (60)::numeric)) AS avg_resolution_time_minutes
   FROM tasker.tasks_dlq
  GROUP BY dlq_reason;


--
-- Name: VIEW v_dlq_dashboard; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON VIEW tasker.v_dlq_dashboard IS 'TAS-49: High-level DLQ metrics for monitoring and alerting.

Aggregates DLQ statistics by reason:
- Total entries and breakdown by resolution status
- Time range (oldest/newest entries)
- Average resolution time';


--
-- Name: v_dlq_investigation_queue; Type: VIEW; Schema: tasker; Owner: -
--

CREATE VIEW tasker.v_dlq_investigation_queue AS
 SELECT dlq_entry_uuid,
    task_uuid,
    original_state,
    dlq_reason,
    dlq_timestamp,
    (EXTRACT(epoch FROM (now() - (dlq_timestamp)::timestamp with time zone)) / (60)::numeric) AS minutes_in_dlq,
    (task_snapshot ->> 'namespace'::text) AS namespace_name,
    (task_snapshot ->> 'task_name'::text) AS task_name,
    (task_snapshot ->> 'current_state'::text) AS current_state,
    ((metadata ->> 'time_in_state_minutes'::text))::integer AS time_in_state_minutes,
    ((
        CASE dlq_reason
            WHEN 'dependency_cycle_detected'::tasker.dlq_reason THEN 1000
            WHEN 'max_retries_exceeded'::tasker.dlq_reason THEN 500
            WHEN 'staleness_timeout'::tasker.dlq_reason THEN 100
            WHEN 'worker_unavailable'::tasker.dlq_reason THEN 50
            ELSE 10
        END)::numeric + (EXTRACT(epoch FROM (now() - (dlq_timestamp)::timestamp with time zone)) / (60)::numeric)) AS priority_score
   FROM tasker.tasks_dlq dlq
  WHERE (resolution_status = 'pending'::tasker.dlq_resolution_status)
  ORDER BY ((
        CASE dlq_reason
            WHEN 'dependency_cycle_detected'::tasker.dlq_reason THEN 1000
            WHEN 'max_retries_exceeded'::tasker.dlq_reason THEN 500
            WHEN 'staleness_timeout'::tasker.dlq_reason THEN 100
            WHEN 'worker_unavailable'::tasker.dlq_reason THEN 50
            ELSE 10
        END)::numeric + (EXTRACT(epoch FROM (now() - (dlq_timestamp)::timestamp with time zone)) / (60)::numeric)) DESC;


--
-- Name: VIEW v_dlq_investigation_queue; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON VIEW tasker.v_dlq_investigation_queue IS 'TAS-49: Prioritized queue of pending DLQ investigations.

Provides operator dashboard with intelligent prioritization:
- Dependency cycles: Highest priority (block entire workflows)
- Max retries exceeded: High priority (permanent failures)
- Staleness timeouts: Medium priority (may self-resolve)
- Age factor: Older entries get higher scores';


--
-- Name: v_task_state_analysis; Type: VIEW; Schema: tasker; Owner: -
--

CREATE VIEW tasker.v_task_state_analysis AS
 SELECT t.task_uuid,
    t.correlation_id,
    tns.name AS namespace_name,
    tns.task_namespace_uuid AS namespace_uuid,
    nt.name AS task_name,
    nt.named_task_uuid,
    nt.configuration AS template_config,
    tt.to_state AS current_state,
    tt.created_at AS state_entered_at,
    (EXTRACT(epoch FROM (now() - (tt.created_at)::timestamp with time zone)) / (60)::numeric) AS time_in_state_minutes,
    (EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (60)::numeric) AS task_age_minutes,
    t.priority,
    t.complete
   FROM (((tasker.tasks t
     JOIN tasker.named_tasks nt ON ((nt.named_task_uuid = t.named_task_uuid)))
     JOIN tasker.task_namespaces tns ON ((tns.task_namespace_uuid = nt.task_namespace_uuid)))
     JOIN tasker.task_transitions tt ON (((tt.task_uuid = t.task_uuid) AND (tt.most_recent = true))))
  WHERE (t.complete = false);


--
-- Name: VIEW v_task_state_analysis; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON VIEW tasker.v_task_state_analysis IS 'TAS-49: Base view for task state analysis with namespace and template information.

Provides foundation for staleness monitoring and DLQ detection.
Joins across tasks, named_tasks, task_namespaces, and task_transitions.

Key Columns:
- time_in_state_minutes: How long task has been in current state
- task_age_minutes: Total task lifetime
- template_config: JSONB configuration with threshold overrides';


--
-- Name: v_task_staleness_monitoring; Type: VIEW; Schema: tasker; Owner: -
--

CREATE VIEW tasker.v_task_staleness_monitoring AS
 SELECT task_uuid,
    namespace_name,
    task_name,
    current_state,
    time_in_state_minutes,
    task_age_minutes,
    tasker.calculate_staleness_threshold(current_state, template_config, 60, 30, 30) AS staleness_threshold_minutes,
        CASE
            WHEN (time_in_state_minutes >= (tasker.calculate_staleness_threshold(current_state, template_config, 60, 30, 30))::numeric) THEN 'stale'::text
            WHEN (time_in_state_minutes >= ((tasker.calculate_staleness_threshold(current_state, template_config, 60, 30, 30))::numeric * 0.8)) THEN 'warning'::text
            ELSE 'healthy'::text
        END AS health_status,
    priority
   FROM tasker.v_task_state_analysis tsa
  WHERE ((current_state)::text = ANY (ARRAY[('waiting_for_dependencies'::character varying)::text, ('waiting_for_retry'::character varying)::text, ('steps_in_process'::character varying)::text]));


--
-- Name: VIEW v_task_staleness_monitoring; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON VIEW tasker.v_task_staleness_monitoring IS 'TAS-49: Real-time staleness monitoring for active tasks.

Provides operational visibility into task health:
- "stale": Exceeded threshold, candidate for DLQ
- "warning": At 80% of threshold, needs attention
- "healthy": Within normal operating parameters';


--
-- Name: workflow_step_result_audit; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_step_result_audit (
    workflow_step_result_audit_uuid uuid DEFAULT tasker.uuid_generate_v7() CONSTRAINT workflow_step_result_audit_workflow_step_result_audit__not_null NOT NULL,
    workflow_step_uuid uuid NOT NULL,
    workflow_step_transition_uuid uuid CONSTRAINT workflow_step_result_audit_workflow_step_transition_uu_not_null NOT NULL,
    task_uuid uuid NOT NULL,
    recorded_at timestamp(6) without time zone DEFAULT now() NOT NULL,
    worker_uuid uuid,
    correlation_id uuid,
    success boolean NOT NULL,
    execution_time_ms bigint,
    created_at timestamp(6) without time zone DEFAULT now() NOT NULL,
    updated_at timestamp(6) without time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE workflow_step_result_audit; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON TABLE tasker.workflow_step_result_audit IS 'Lightweight audit trail for workflow step execution results. Stores references and attribution only - full results accessed via JOIN to transitions table. Created by TAS-62.';


--
-- Name: COLUMN workflow_step_result_audit.worker_uuid; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.worker_uuid IS 'UUID of the worker that processed this step (attribution for SOC2 compliance)';


--
-- Name: COLUMN workflow_step_result_audit.correlation_id; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.correlation_id IS 'Correlation ID for distributed tracing across the orchestration system';


--
-- Name: COLUMN workflow_step_result_audit.success; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.success IS 'Whether the step execution succeeded (extracted from transition for indexing)';


--
-- Name: COLUMN workflow_step_result_audit.execution_time_ms; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.execution_time_ms IS 'Step execution time in milliseconds (extracted from transition for filtering)';


--
-- Name: workflow_step_transitions; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_step_transitions (
    workflow_step_transition_uuid uuid DEFAULT tasker.uuid_generate_v7() CONSTRAINT workflow_step_transitions_workflow_step_transition_uui_not_null NOT NULL,
    workflow_step_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    CONSTRAINT chk_workflow_step_transitions_from_state CHECK (((from_state IS NULL) OR ((from_state)::text = ANY (ARRAY[('pending'::character varying)::text, ('enqueued'::character varying)::text, ('in_progress'::character varying)::text, ('enqueued_for_orchestration'::character varying)::text, ('enqueued_as_error_for_orchestration'::character varying)::text, ('waiting_for_retry'::character varying)::text, ('complete'::character varying)::text, ('error'::character varying)::text, ('cancelled'::character varying)::text, ('resolved_manually'::character varying)::text])))),
    CONSTRAINT chk_workflow_step_transitions_to_state CHECK (((to_state)::text = ANY (ARRAY[('pending'::character varying)::text, ('enqueued'::character varying)::text, ('in_progress'::character varying)::text, ('enqueued_for_orchestration'::character varying)::text, ('enqueued_as_error_for_orchestration'::character varying)::text, ('waiting_for_retry'::character varying)::text, ('complete'::character varying)::text, ('error'::character varying)::text, ('cancelled'::character varying)::text, ('resolved_manually'::character varying)::text])))
);


--
-- Name: annotation_types annotation_types_name_key; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.annotation_types
    ADD CONSTRAINT annotation_types_name_key UNIQUE (name);


--
-- Name: annotation_types annotation_types_name_unique; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.annotation_types
    ADD CONSTRAINT annotation_types_name_unique UNIQUE (name);


--
-- Name: annotation_types annotation_types_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.annotation_types
    ADD CONSTRAINT annotation_types_pkey PRIMARY KEY (annotation_type_id);


--
-- Name: dependent_system_object_maps dependent_system_object_maps_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.dependent_system_object_maps
    ADD CONSTRAINT dependent_system_object_maps_pkey PRIMARY KEY (dependent_system_object_map_id);


--
-- Name: dependent_systems dependent_systems_name_key; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.dependent_systems
    ADD CONSTRAINT dependent_systems_name_key UNIQUE (name);


--
-- Name: dependent_systems dependent_systems_name_unique; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.dependent_systems
    ADD CONSTRAINT dependent_systems_name_unique UNIQUE (name);


--
-- Name: dependent_systems dependent_systems_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.dependent_systems
    ADD CONSTRAINT dependent_systems_pkey PRIMARY KEY (dependent_system_uuid);


--
-- Name: named_steps named_steps_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_steps
    ADD CONSTRAINT named_steps_pkey PRIMARY KEY (named_step_uuid);


--
-- Name: named_steps named_steps_system_name_unique; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_steps
    ADD CONSTRAINT named_steps_system_name_unique UNIQUE (dependent_system_uuid, name);


--
-- Name: named_tasks_named_steps named_tasks_named_steps_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_pkey PRIMARY KEY (ntns_uuid);


--
-- Name: named_tasks_named_steps named_tasks_named_steps_unique; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_unique UNIQUE (named_task_uuid, named_step_uuid);


--
-- Name: named_tasks named_tasks_namespace_name_version_unique; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_tasks
    ADD CONSTRAINT named_tasks_namespace_name_version_unique UNIQUE (task_namespace_uuid, name, version);


--
-- Name: named_tasks named_tasks_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_tasks
    ADD CONSTRAINT named_tasks_pkey PRIMARY KEY (named_task_uuid);


--
-- Name: task_annotations task_annotations_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_annotations
    ADD CONSTRAINT task_annotations_pkey PRIMARY KEY (task_annotation_id);


--
-- Name: task_namespaces task_namespaces_name_key; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_namespaces
    ADD CONSTRAINT task_namespaces_name_key UNIQUE (name);


--
-- Name: task_namespaces task_namespaces_name_unique; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_namespaces
    ADD CONSTRAINT task_namespaces_name_unique UNIQUE (name);


--
-- Name: task_namespaces task_namespaces_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_namespaces
    ADD CONSTRAINT task_namespaces_pkey PRIMARY KEY (task_namespace_uuid);


--
-- Name: task_transitions task_transitions_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_transitions
    ADD CONSTRAINT task_transitions_pkey PRIMARY KEY (task_transition_uuid);


--
-- Name: tasks_dlq tasks_dlq_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.tasks_dlq
    ADD CONSTRAINT tasks_dlq_pkey PRIMARY KEY (dlq_entry_uuid);


--
-- Name: tasks tasks_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.tasks
    ADD CONSTRAINT tasks_pkey PRIMARY KEY (task_uuid);


--
-- Name: workflow_step_edges unique_edge_per_step_pair; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT unique_edge_per_step_pair UNIQUE (from_step_uuid, to_step_uuid, name);


--
-- Name: CONSTRAINT unique_edge_per_step_pair ON workflow_step_edges; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON CONSTRAINT unique_edge_per_step_pair ON tasker.workflow_step_edges IS 'Prevents duplicate edges between the same two steps with the same edge name. Critical for batch processing worker creation idempotency.';


--
-- Name: workflow_step_result_audit uq_audit_step_transition; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT uq_audit_step_transition UNIQUE (workflow_step_uuid, workflow_step_transition_uuid);


--
-- Name: workflow_step_edges workflow_step_edges_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT workflow_step_edges_pkey PRIMARY KEY (workflow_step_edge_uuid);


--
-- Name: workflow_step_result_audit workflow_step_result_audit_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_pkey PRIMARY KEY (workflow_step_result_audit_uuid);


--
-- Name: workflow_step_transitions workflow_step_transitions_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_transitions
    ADD CONSTRAINT workflow_step_transitions_pkey PRIMARY KEY (workflow_step_transition_uuid);


--
-- Name: workflow_steps workflow_steps_pkey; Type: CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_steps
    ADD CONSTRAINT workflow_steps_pkey PRIMARY KEY (workflow_step_uuid);


--
-- Name: idx_audit_correlation_id; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_audit_correlation_id ON tasker.workflow_step_result_audit USING btree (correlation_id) WHERE (correlation_id IS NOT NULL);


--
-- Name: idx_audit_recorded_at; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_audit_recorded_at ON tasker.workflow_step_result_audit USING btree (recorded_at);


--
-- Name: idx_audit_step_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_audit_step_uuid ON tasker.workflow_step_result_audit USING btree (workflow_step_uuid);


--
-- Name: idx_audit_success; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_audit_success ON tasker.workflow_step_result_audit USING btree (success);


--
-- Name: idx_audit_task_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_audit_task_uuid ON tasker.workflow_step_result_audit USING btree (task_uuid);


--
-- Name: idx_audit_worker_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_audit_worker_uuid ON tasker.workflow_step_result_audit USING btree (worker_uuid) WHERE (worker_uuid IS NOT NULL);


--
-- Name: idx_dependent_systems_name; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_dependent_systems_name ON tasker.dependent_systems USING btree (name);


--
-- Name: idx_dlq_reason; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_dlq_reason ON tasker.tasks_dlq USING btree (dlq_reason);


--
-- Name: idx_dlq_resolution_status; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_dlq_resolution_status ON tasker.tasks_dlq USING btree (resolution_status, dlq_timestamp);


--
-- Name: idx_dlq_task_lookup; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_dlq_task_lookup ON tasker.tasks_dlq USING btree (task_uuid);


--
-- Name: idx_dlq_timestamp; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_dlq_timestamp ON tasker.tasks_dlq USING btree (dlq_timestamp DESC);


--
-- Name: idx_dlq_unique_pending_task; Type: INDEX; Schema: tasker; Owner: -
--

CREATE UNIQUE INDEX idx_dlq_unique_pending_task ON tasker.tasks_dlq USING btree (task_uuid) WHERE (resolution_status = 'pending'::tasker.dlq_resolution_status);


--
-- Name: INDEX idx_dlq_unique_pending_task; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON INDEX tasker.idx_dlq_unique_pending_task IS 'TAS-49: Ensures only one pending DLQ investigation per task.
Historical DLQ entries (manually_resolved, permanently_failed, cancelled) are preserved as audit trail.
Allows multiple DLQ investigations per task over time, but only one active investigation at a time.';


--
-- Name: idx_named_steps_system_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_named_steps_system_uuid ON tasker.named_steps USING btree (dependent_system_uuid);


--
-- Name: idx_named_tasks_lifecycle_config; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_named_tasks_lifecycle_config ON tasker.named_tasks USING gin (((configuration -> 'lifecycle'::text))) WHERE ((configuration -> 'lifecycle'::text) IS NOT NULL);


--
-- Name: INDEX idx_named_tasks_lifecycle_config; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON INDEX tasker.idx_named_tasks_lifecycle_config IS 'TAS-49 Phase 2: GIN index for lifecycle configuration JSONB path queries.
Optimizes threshold lookups in calculate_staleness_threshold() function.';


--
-- Name: idx_named_tasks_namespace_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_named_tasks_namespace_uuid ON tasker.named_tasks USING btree (task_namespace_uuid);


--
-- Name: idx_task_annotations_task_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_annotations_task_uuid ON tasker.task_annotations USING btree (task_uuid);


--
-- Name: idx_task_annotations_type_id; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_annotations_type_id ON tasker.task_annotations USING btree (annotation_type_id);


--
-- Name: idx_task_namespaces_name; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_namespaces_name ON tasker.task_namespaces USING btree (name);


--
-- Name: idx_task_transitions_most_recent; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_transitions_most_recent ON tasker.task_transitions USING btree (most_recent) WHERE (most_recent = true);


--
-- Name: idx_task_transitions_processor; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_transitions_processor ON tasker.task_transitions USING btree (processor_uuid) WHERE (processor_uuid IS NOT NULL);


--
-- Name: idx_task_transitions_state_lookup; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_transitions_state_lookup ON tasker.task_transitions USING btree (task_uuid, to_state, most_recent) WHERE (most_recent = true);


--
-- Name: idx_task_transitions_task_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_transitions_task_uuid ON tasker.task_transitions USING btree (task_uuid);


--
-- Name: idx_task_transitions_timeout; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_transitions_timeout ON tasker.task_transitions USING btree (((transition_metadata ->> 'timeout_at'::text))) WHERE (most_recent = true);


--
-- Name: idx_task_transitions_uuid_most_recent; Type: INDEX; Schema: tasker; Owner: -
--

CREATE UNIQUE INDEX idx_task_transitions_uuid_most_recent ON tasker.task_transitions USING btree (task_uuid, most_recent) WHERE (most_recent = true);


--
-- Name: idx_task_transitions_uuid_sort_key; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_transitions_uuid_sort_key ON tasker.task_transitions USING btree (task_uuid, sort_key);


--
-- Name: idx_task_transitions_uuid_temporal; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_task_transitions_uuid_temporal ON tasker.task_transitions USING btree (task_transition_uuid, created_at);


--
-- Name: idx_tasks_active_with_priority_covering; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_active_with_priority_covering ON tasker.tasks USING btree (complete, priority, task_uuid) INCLUDE (named_task_uuid, requested_at) WHERE (complete = false);


--
-- Name: idx_tasks_complete; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_complete ON tasker.tasks USING btree (complete);


--
-- Name: idx_tasks_completed_at; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_completed_at ON tasker.tasks USING btree (completed_at);


--
-- Name: idx_tasks_correlation_hierarchy; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_correlation_hierarchy ON tasker.tasks USING btree (parent_correlation_id, correlation_id) WHERE (parent_correlation_id IS NOT NULL);


--
-- Name: idx_tasks_correlation_id; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_correlation_id ON tasker.tasks USING btree (correlation_id);


--
-- Name: idx_tasks_identity_hash; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_identity_hash ON tasker.tasks USING btree (identity_hash);


--
-- Name: idx_tasks_named_task_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_named_task_uuid ON tasker.tasks USING btree (named_task_uuid);


--
-- Name: idx_tasks_parent_correlation_id; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_parent_correlation_id ON tasker.tasks USING btree (parent_correlation_id) WHERE (parent_correlation_id IS NOT NULL);


--
-- Name: idx_tasks_priority; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_priority ON tasker.tasks USING btree (priority);


--
-- Name: idx_tasks_requested_at; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_requested_at ON tasker.tasks USING btree (requested_at);


--
-- Name: idx_tasks_source_system; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_source_system ON tasker.tasks USING btree (source_system);


--
-- Name: idx_tasks_tags_gin; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_tags_gin ON tasker.tasks USING gin (tags);


--
-- Name: idx_tasks_tags_gin_path; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_tags_gin_path ON tasker.tasks USING gin (tags jsonb_path_ops);


--
-- Name: idx_tasks_uuid_temporal; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_tasks_uuid_temporal ON tasker.tasks USING btree (task_uuid, created_at);


--
-- Name: idx_workflow_step_edges_from_step; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_edges_from_step ON tasker.workflow_step_edges USING btree (from_step_uuid);


--
-- Name: idx_workflow_step_edges_from_step_name; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_edges_from_step_name ON tasker.workflow_step_edges USING btree (from_step_uuid, name);


--
-- Name: idx_workflow_step_edges_to_step; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_edges_to_step ON tasker.workflow_step_edges USING btree (to_step_uuid);


--
-- Name: idx_workflow_step_transitions_most_recent; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_transitions_most_recent ON tasker.workflow_step_transitions USING btree (most_recent) WHERE (most_recent = true);


--
-- Name: idx_workflow_step_transitions_state_lookup; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_transitions_state_lookup ON tasker.workflow_step_transitions USING btree (workflow_step_uuid, to_state, most_recent) WHERE (most_recent = true);


--
-- Name: idx_workflow_step_transitions_step_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_transitions_step_uuid ON tasker.workflow_step_transitions USING btree (workflow_step_uuid);


--
-- Name: idx_workflow_step_transitions_uuid_most_recent; Type: INDEX; Schema: tasker; Owner: -
--

CREATE UNIQUE INDEX idx_workflow_step_transitions_uuid_most_recent ON tasker.workflow_step_transitions USING btree (workflow_step_uuid, most_recent) WHERE (most_recent = true);


--
-- Name: idx_workflow_step_transitions_uuid_sort_key; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_transitions_uuid_sort_key ON tasker.workflow_step_transitions USING btree (workflow_step_uuid, sort_key);


--
-- Name: idx_workflow_step_transitions_uuid_temporal; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_step_transitions_uuid_temporal ON tasker.workflow_step_transitions USING btree (workflow_step_transition_uuid, created_at);


--
-- Name: idx_workflow_steps_active_operations; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_active_operations ON tasker.workflow_steps USING btree (workflow_step_uuid, task_uuid) WHERE ((processed = false) OR (processed IS NULL));


--
-- Name: idx_workflow_steps_checkpoint_cursor; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_checkpoint_cursor ON tasker.workflow_steps USING btree (((checkpoint ->> 'cursor'::text))) WHERE (checkpoint IS NOT NULL);


--
-- Name: idx_workflow_steps_checkpoint_exists; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_checkpoint_exists ON tasker.workflow_steps USING btree (workflow_step_uuid) WHERE (checkpoint IS NOT NULL);


--
-- Name: idx_workflow_steps_named_step_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_named_step_uuid ON tasker.workflow_steps USING btree (named_step_uuid);


--
-- Name: idx_workflow_steps_processed_at; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_processed_at ON tasker.workflow_steps USING btree (processed_at);


--
-- Name: idx_workflow_steps_processing_status; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_processing_status ON tasker.workflow_steps USING btree (task_uuid, processed, in_process);


--
-- Name: idx_workflow_steps_ready_covering; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_ready_covering ON tasker.workflow_steps USING btree (task_uuid, processed, in_process) INCLUDE (workflow_step_uuid, attempts, max_attempts, retryable) WHERE (processed = false);


--
-- Name: idx_workflow_steps_retry_logic; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_retry_logic ON tasker.workflow_steps USING btree (attempts, max_attempts, retryable);


--
-- Name: idx_workflow_steps_retry_status; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_retry_status ON tasker.workflow_steps USING btree (attempts, max_attempts);


--
-- Name: idx_workflow_steps_task_covering; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_task_covering ON tasker.workflow_steps USING btree (task_uuid) INCLUDE (workflow_step_uuid, processed, in_process, attempts, max_attempts);


--
-- Name: idx_workflow_steps_task_grouping_active; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_task_grouping_active ON tasker.workflow_steps USING btree (task_uuid, workflow_step_uuid) WHERE ((processed = false) OR (processed IS NULL));


--
-- Name: idx_workflow_steps_task_readiness; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_task_readiness ON tasker.workflow_steps USING btree (task_uuid, processed, workflow_step_uuid) WHERE (processed = false);


--
-- Name: idx_workflow_steps_task_uuid; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_task_uuid ON tasker.workflow_steps USING btree (task_uuid);


--
-- Name: idx_workflow_steps_transitive_deps; Type: INDEX; Schema: tasker; Owner: -
--

CREATE INDEX idx_workflow_steps_transitive_deps ON tasker.workflow_steps USING btree (workflow_step_uuid, named_step_uuid) INCLUDE (task_uuid, processed);


--
-- Name: workflow_step_transitions trg_step_result_audit; Type: TRIGGER; Schema: tasker; Owner: -
--

CREATE TRIGGER trg_step_result_audit AFTER INSERT ON tasker.workflow_step_transitions FOR EACH ROW EXECUTE FUNCTION tasker.create_step_result_audit();


--
-- Name: TRIGGER trg_step_result_audit ON workflow_step_transitions; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON TRIGGER trg_step_result_audit ON tasker.workflow_step_transitions IS 'Creates audit records when workers persist step execution results (TAS-62)';


--
-- Name: tasks_dlq update_dlq_updated_at; Type: TRIGGER; Schema: tasker; Owner: -
--

CREATE TRIGGER update_dlq_updated_at BEFORE UPDATE ON tasker.tasks_dlq FOR EACH ROW EXECUTE FUNCTION tasker.update_updated_at_column();


--
-- Name: dependent_system_object_maps dependent_system_object_maps_dependent_system_one_uuid_f; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.dependent_system_object_maps
    ADD CONSTRAINT dependent_system_object_maps_dependent_system_one_uuid_f FOREIGN KEY (dependent_system_one_uuid) REFERENCES tasker.dependent_systems(dependent_system_uuid);


--
-- Name: dependent_system_object_maps dependent_system_object_maps_dependent_system_two_uuid_f; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.dependent_system_object_maps
    ADD CONSTRAINT dependent_system_object_maps_dependent_system_two_uuid_f FOREIGN KEY (dependent_system_two_uuid) REFERENCES tasker.dependent_systems(dependent_system_uuid);


--
-- Name: named_steps named_steps_dependent_system_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_steps
    ADD CONSTRAINT named_steps_dependent_system_uuid_fkey FOREIGN KEY (dependent_system_uuid) REFERENCES tasker.dependent_systems(dependent_system_uuid);


--
-- Name: named_tasks_named_steps named_tasks_named_steps_named_step_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_named_step_uuid_fkey FOREIGN KEY (named_step_uuid) REFERENCES tasker.named_steps(named_step_uuid);


--
-- Name: named_tasks_named_steps named_tasks_named_steps_named_task_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_named_task_uuid_fkey FOREIGN KEY (named_task_uuid) REFERENCES tasker.named_tasks(named_task_uuid);


--
-- Name: named_tasks named_tasks_task_namespace_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.named_tasks
    ADD CONSTRAINT named_tasks_task_namespace_uuid_fkey FOREIGN KEY (task_namespace_uuid) REFERENCES tasker.task_namespaces(task_namespace_uuid);


--
-- Name: task_annotations task_annotations_annotation_type_id_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_annotations
    ADD CONSTRAINT task_annotations_annotation_type_id_fkey FOREIGN KEY (annotation_type_id) REFERENCES tasker.annotation_types(annotation_type_id);


--
-- Name: task_annotations task_annotations_task_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_annotations
    ADD CONSTRAINT task_annotations_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);


--
-- Name: task_transitions task_transitions_task_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.task_transitions
    ADD CONSTRAINT task_transitions_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);


--
-- Name: tasks_dlq tasks_dlq_task_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.tasks_dlq
    ADD CONSTRAINT tasks_dlq_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);


--
-- Name: tasks tasks_named_task_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.tasks
    ADD CONSTRAINT tasks_named_task_uuid_fkey FOREIGN KEY (named_task_uuid) REFERENCES tasker.named_tasks(named_task_uuid);


--
-- Name: workflow_step_edges workflow_step_edges_from_step_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT workflow_step_edges_from_step_uuid_fkey FOREIGN KEY (from_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);


--
-- Name: workflow_step_edges workflow_step_edges_to_step_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT workflow_step_edges_to_step_uuid_fkey FOREIGN KEY (to_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);


--
-- Name: workflow_step_result_audit workflow_step_result_audit_step_transition_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_step_transition_uuid_fkey FOREIGN KEY (workflow_step_transition_uuid) REFERENCES tasker.workflow_step_transitions(workflow_step_transition_uuid);


--
-- Name: workflow_step_result_audit workflow_step_result_audit_task_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);


--
-- Name: workflow_step_result_audit workflow_step_result_audit_workflow_step_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_workflow_step_uuid_fkey FOREIGN KEY (workflow_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);


--
-- Name: workflow_step_transitions workflow_step_transitions_workflow_step_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_step_transitions
    ADD CONSTRAINT workflow_step_transitions_workflow_step_uuid_fkey FOREIGN KEY (workflow_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);


--
-- Name: workflow_steps workflow_steps_named_step_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_steps
    ADD CONSTRAINT workflow_steps_named_step_uuid_fkey FOREIGN KEY (named_step_uuid) REFERENCES tasker.named_steps(named_step_uuid);


--
-- Name: workflow_steps workflow_steps_task_uuid_fkey; Type: FK CONSTRAINT; Schema: tasker; Owner: -
--

ALTER TABLE ONLY tasker.workflow_steps
    ADD CONSTRAINT workflow_steps_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);


--
-- Name: pgmq_headers_compatibility_trigger; Type: EVENT TRIGGER; Schema: -; Owner: -
--

CREATE EVENT TRIGGER pgmq_headers_compatibility_trigger ON ddl_command_end
         WHEN TAG IN ('CREATE TABLE')
   EXECUTE FUNCTION public.pgmq_auto_add_headers_trigger();


--
-- Name: EVENT TRIGGER pgmq_headers_compatibility_trigger; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EVENT TRIGGER pgmq_headers_compatibility_trigger IS 'Automatically adds headers column to new pgmq queue tables for pgmq-rs compatibility';


--
-- PostgreSQL database dump complete
--

\unrestrict qQIa4hJsRfF0VR1S1fMyzCW2PWTkeXglkALREIKnEtuWug2WZah9orqhVnHwvTp


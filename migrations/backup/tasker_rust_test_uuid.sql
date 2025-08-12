--
-- PostgreSQL database dump
--

-- Dumped from database version 17.4 (Debian 17.4-1.pgdg120+2)
-- Dumped by pg_dump version 17.5 (Homebrew)

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
-- Name: pgmq; Type: SCHEMA; Schema: -; Owner: tasker
--

CREATE SCHEMA pgmq;


ALTER SCHEMA pgmq OWNER TO tasker;

--
-- Name: pg_uuidv7; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_uuidv7 WITH SCHEMA public;


--
-- Name: EXTENSION pg_uuidv7; Type: COMMENT; Schema: -; Owner:
--

COMMENT ON EXTENSION pg_uuidv7 IS 'pg_uuidv7: create UUIDv7 values in postgres';


--
-- Name: pgmq; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgmq WITH SCHEMA pgmq;


--
-- Name: EXTENSION pgmq; Type: COMMENT; Schema: -; Owner:
--

COMMENT ON EXTENSION pgmq IS 'A lightweight message queue. Like AWS SQS and RSMQ but on Postgres.';


--
-- Name: calculate_dependency_levels(uuid); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.calculate_dependency_levels(input_task_uuid uuid) RETURNS TABLE(workflow_step_uuid uuid, dependency_level integer)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH RECURSIVE dependency_levels AS (
    -- Base case: Find root nodes (steps with no dependencies)
    SELECT
      ws.workflow_step_uuid,
      0 as level
    FROM tasker_workflow_steps ws
    WHERE ws.task_uuid = input_task_uuid
      AND NOT EXISTS (
        SELECT 1
        FROM tasker_workflow_step_edges wse
        WHERE wse.to_step_uuid = ws.workflow_step_uuid
      )

    UNION ALL

    -- Recursive case: Find children of current level nodes
    SELECT
      wse.to_step_uuid as workflow_step_uuid,
      dl.level + 1 as level
    FROM dependency_levels dl
    JOIN tasker_workflow_step_edges wse ON wse.from_step_uuid = dl.workflow_step_uuid
    JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.to_step_uuid
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


ALTER FUNCTION public.calculate_dependency_levels(input_task_uuid uuid) OWNER TO tasker;

--
-- Name: claim_ready_tasks(character varying, integer, character varying); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.claim_ready_tasks(p_orchestrator_id character varying, p_limit integer DEFAULT 1, p_namespace_filter character varying DEFAULT NULL::character varying) RETURNS TABLE(task_uuid uuid, namespace_name character varying, priority integer, computed_priority double precision, age_hours double precision, ready_steps_count bigint, claim_timeout_seconds integer)
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Atomically claim tasks using proper locking on base table with computed priority ordering
    RETURN QUERY
    UPDATE tasker_tasks t
    SET claimed_at = NOW(),
        claimed_by = p_orchestrator_id,
        updated_at = NOW()
    FROM (
        SELECT
            t.task_uuid,
            -- Include computed priority and age for debugging and monitoring
            -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
            -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
            (CASE
                WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)
                WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)
                WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)
                WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)
                ELSE                             0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)
            END)::float8 as computed_priority_calc,
            ROUND(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600.0, 2)::float8 as age_hours_calc
        FROM tasker_tasks t
        JOIN tasker_named_tasks nt ON t.named_task_uuid = nt.named_task_uuid
        JOIN tasker_task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
        JOIN LATERAL (SELECT * FROM get_task_execution_context(t.task_uuid)) tec ON true
        WHERE t.complete = false
            AND tec.ready_steps > 0
            AND (t.claimed_at IS NULL OR t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval))
            AND (p_namespace_filter IS NULL OR tn.name = p_namespace_filter)
        ORDER BY
            -- Use computed priority for fair ordering
            -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
            -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
            (CASE
                WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)
                WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)
                WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)
                WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)
                ELSE                             0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)
            END)::float8 DESC,
            t.created_at ASC
        LIMIT p_limit
        FOR UPDATE OF t SKIP LOCKED
    ) ready_tasks
    WHERE t.task_uuid = ready_tasks.task_uuid
    RETURNING
        t.task_uuid,
        (SELECT name FROM tasker_task_namespaces WHERE task_namespace_uuid =
            (SELECT task_namespace_uuid FROM tasker_named_tasks WHERE named_task_uuid = t.named_task_uuid)
        ) as namespace_name,
        t.priority,
        ready_tasks.computed_priority_calc::float8 as computed_priority,
        ready_tasks.age_hours_calc::float8 as age_hours,
        (SELECT ready_steps FROM get_task_execution_context(t.task_uuid)) as ready_steps_count,
        t.claim_timeout_seconds;
END;
$$;


ALTER FUNCTION public.claim_ready_tasks(p_orchestrator_id character varying, p_limit integer, p_namespace_filter character varying) OWNER TO tasker;

--
-- Name: extend_task_claim(uuid, character varying); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.extend_task_claim(p_task_uuid uuid, p_orchestrator_id character varying) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_rows_updated integer;
BEGIN
    UPDATE tasker_tasks
    SET claimed_at = NOW(),  -- Reset claim time
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND claimed_by = p_orchestrator_id  -- Only extend if we own the claim
        AND claimed_at IS NOT NULL;         -- And claim exists

    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;

    -- Return true if claim was extended, false if not found or not owned
    RETURN v_rows_updated > 0;
END;
$$;


ALTER FUNCTION public.extend_task_claim(p_task_uuid uuid, p_orchestrator_id character varying) OWNER TO tasker;

--
-- Name: get_analytics_metrics(timestamp with time zone); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_analytics_metrics(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone) RETURNS TABLE(active_tasks_count bigint, total_namespaces_count bigint, unique_task_types_count bigint, system_health_score numeric, task_throughput bigint, completion_count bigint, error_count bigint, completion_rate numeric, error_rate numeric, avg_task_duration numeric, avg_step_duration numeric, step_throughput bigint, analysis_period_start timestamp with time zone, calculated_at timestamp with time zone)
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
        FROM tasker_tasks t
        INNER JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE wst.most_recent = true
          AND wst.to_state NOT IN ('complete', 'error', 'skipped', 'resolved_manually')
    ),
    namespace_summary AS (
        SELECT COUNT(DISTINCT tn.name) as namespace_count
        FROM tasker_task_namespaces tn
        INNER JOIN tasker_named_tasks nt ON nt.task_namespace_uuid = tn.task_namespace_uuid
        INNER JOIN tasker_tasks t ON t.named_task_uuid = nt.named_task_uuid
    ),
    task_type_summary AS (
        SELECT COUNT(DISTINCT nt.name) as task_type_count
        FROM tasker_named_tasks nt
        INNER JOIN tasker_tasks t ON t.named_task_uuid = nt.named_task_uuid
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
        FROM tasker_tasks t
        INNER JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
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
        FROM tasker_tasks t
        LEFT JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        LEFT JOIN tasker_workflow_step_transitions completed_wst ON completed_wst.workflow_step_uuid = ws.workflow_step_uuid
            AND completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
        LEFT JOIN tasker_workflow_step_transitions error_wst ON error_wst.workflow_step_uuid = ws.workflow_step_uuid
            AND error_wst.to_state = 'error' AND error_wst.most_recent = true
        LEFT JOIN tasker_workflow_step_transitions step_completed ON step_completed.workflow_step_uuid = ws.workflow_step_uuid
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


ALTER FUNCTION public.get_analytics_metrics(since_timestamp timestamp with time zone) OWNER TO tasker;

--
-- Name: get_slowest_steps(timestamp with time zone, integer, text, text, text); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_slowest_steps(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone, limit_count integer DEFAULT 10, namespace_filter text DEFAULT NULL::text, task_name_filter text DEFAULT NULL::text, version_filter text DEFAULT NULL::text) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, step_name character varying, task_name character varying, namespace_name character varying, version character varying, duration_seconds numeric, attempts integer, created_at timestamp without time zone, completed_at timestamp without time zone, retryable boolean, step_status character varying)
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
        FROM tasker_workflow_steps ws
        INNER JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        INNER JOIN tasker_tasks t ON t.task_uuid = ws.task_uuid
        INNER JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
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


ALTER FUNCTION public.get_slowest_steps(since_timestamp timestamp with time zone, limit_count integer, namespace_filter text, task_name_filter text, version_filter text) OWNER TO tasker;

--
-- Name: get_slowest_tasks(timestamp with time zone, integer, text, text, text); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_slowest_tasks(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone, limit_count integer DEFAULT 10, namespace_filter text DEFAULT NULL::text, task_name_filter text DEFAULT NULL::text, version_filter text DEFAULT NULL::text) RETURNS TABLE(task_uuid uuid, task_name character varying, namespace_name character varying, version character varying, duration_seconds numeric, step_count bigint, completed_steps bigint, error_steps bigint, created_at timestamp without time zone, completed_at timestamp without time zone, initiator character varying, source_system character varying)
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
        FROM tasker_tasks t
        INNER JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        LEFT JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
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


ALTER FUNCTION public.get_slowest_tasks(since_timestamp timestamp with time zone, limit_count integer, namespace_filter text, task_name_filter text, version_filter text) OWNER TO tasker;

--
-- Name: get_step_readiness_status(uuid, uuid[]); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_step_readiness_status(input_task_uuid uuid, step_uuids uuid[] DEFAULT NULL::uuid[]) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, named_step_uuid uuid, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, retry_limit integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM tasker_workflow_step_transitions wst
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),
  dependency_counts AS (
    SELECT
      wse.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_state.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM tasker_workflow_step_edges wse
    LEFT JOIN step_states parent_state ON parent_state.workflow_step_uuid = wse.from_step_uuid
    WHERE wse.to_step_uuid IN (
      SELECT ws.workflow_step_uuid
      FROM tasker_workflow_steps ws
      WHERE ws.task_uuid = input_task_uuid
    )
    GROUP BY wse.to_step_uuid
  ),
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM tasker_workflow_step_transitions wst
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  )
  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied
    CASE
      WHEN dc.total_deps IS NULL THEN true  -- No dependencies
      WHEN dc.completed_deps = dc.total_deps THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Retry eligibility
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Ready for execution
    CASE
      WHEN COALESCE(ss.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)
      AND (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps)
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))
      AND (COALESCE(ws.retryable, true) = true)
      AND (ws.in_process = false OR ws.in_process IS NULL)
      AND (
        -- Check explicit backoff timing (most restrictive)
        -- If backoff is set, the backoff period must have expired
        CASE
          WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
            ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') <= NOW()
          ELSE true  -- No explicit backoff set
        END
        AND
        -- Then check failure-based backoff
        (lf.failure_time IS NULL OR
         lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    lf.failure_time as last_failure_at,

    -- Next retry calculation
    CASE
      WHEN ws.last_attempted_at IS NOT NULL AND ws.backoff_request_seconds IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN lf.failure_time IS NOT NULL THEN
        lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = input_task_uuid
    AND (step_uuids IS NULL OR ws.workflow_step_uuid = ANY(step_uuids))
  ORDER BY ws.workflow_step_uuid;
END;
$$;


ALTER FUNCTION public.get_step_readiness_status(input_task_uuid uuid, step_uuids uuid[]) OWNER TO tasker;

--
-- Name: get_step_readiness_status_batch(uuid[]); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_step_readiness_status_batch(input_task_uuids uuid[]) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, named_step_uuid uuid, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, retry_limit integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM tasker_workflow_step_transitions wst
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),
  dependency_counts AS (
    SELECT
      wse.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_state.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM tasker_workflow_step_edges wse
    LEFT JOIN step_states parent_state ON parent_state.workflow_step_uuid = wse.from_step_uuid
    WHERE wse.to_step_uuid IN (
      SELECT ws.workflow_step_uuid
      FROM tasker_workflow_steps ws
      WHERE ws.task_uuid = ANY(input_task_uuids)
    )
    GROUP BY wse.to_step_uuid
  ),
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM tasker_workflow_step_transitions wst
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  )
  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied
    CASE
      WHEN dc.total_deps IS NULL THEN true
      WHEN dc.completed_deps = dc.total_deps THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Retry eligibility
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Ready for execution
    CASE
      WHEN COALESCE(ss.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)
      AND (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps)
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))
      AND (COALESCE(ws.retryable, true) = true)
      AND (ws.in_process = false OR ws.in_process IS NULL)
      AND (
        ws.last_attempted_at IS NULL OR
        ws.last_attempted_at + (COALESCE(ws.backoff_request_seconds, 0) * interval '1 second') <= NOW() OR
        (lf.failure_time IS NULL OR
         lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    lf.failure_time as last_failure_at,

    -- Next retry calculation
    CASE
      WHEN ws.last_attempted_at IS NOT NULL AND ws.backoff_request_seconds IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN lf.failure_time IS NOT NULL THEN
        lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = ANY(input_task_uuids)
  ORDER BY ws.workflow_step_uuid;
END;
$$;


ALTER FUNCTION public.get_step_readiness_status_batch(input_task_uuids uuid[]) OWNER TO tasker;

--
-- Name: get_step_transitive_dependencies(uuid); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_step_transitive_dependencies(target_step_uuid uuid) RETURNS TABLE(workflow_step_uuid uuid, task_uuid uuid, named_step_uuid uuid, step_name character varying, results jsonb, processed boolean, distance integer)
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
        FROM tasker_workflow_step_edges wse
        JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
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
        JOIN tasker_workflow_step_edges wse ON wse.to_step_uuid = td.workflow_step_uuid
        JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
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


ALTER FUNCTION public.get_step_transitive_dependencies(target_step_uuid uuid) OWNER TO tasker;

--
-- Name: get_system_health_counts(); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_system_health_counts() RETURNS TABLE(total_tasks bigint, pending_tasks bigint, in_progress_tasks bigint, complete_tasks bigint, error_tasks bigint, cancelled_tasks bigint, total_steps bigint, pending_steps bigint, in_progress_steps bigint, complete_steps bigint, error_steps bigint, retryable_error_steps bigint, exhausted_retry_steps bigint, in_backoff_steps bigint, active_connections bigint, max_connections bigint)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
    RETURN QUERY
    WITH task_counts AS (
        SELECT
            COUNT(*) as total_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'pending') as pending_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'in_progress') as in_progress_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'complete') as complete_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'error') as error_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'cancelled') as cancelled_tasks
        FROM tasker_tasks t
        LEFT JOIN tasker_task_transitions task_state ON task_state.task_uuid = t.task_uuid
            AND task_state.most_recent = true
    ),
    step_counts AS (
        SELECT
            COUNT(*) as total_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'pending') as pending_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'in_progress') as in_progress_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'complete') as complete_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'error') as error_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts < ws.retry_limit
                AND COALESCE(ws.retryable, true) = true
            ) as retryable_error_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts >= ws.retry_limit
            ) as exhausted_retry_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts < ws.retry_limit
                AND COALESCE(ws.retryable, true) = true
                AND ws.last_attempted_at IS NOT NULL
            ) as in_backoff_steps
        FROM tasker_workflow_steps ws
        LEFT JOIN tasker_workflow_step_transitions step_state ON step_state.workflow_step_uuid = ws.workflow_step_uuid
            AND step_state.most_recent = true
    ),
    connection_info AS (
        SELECT
            (SELECT count(*) FROM pg_stat_activity WHERE state != 'idle') as active_connections,
            (SELECT setting::int FROM pg_settings WHERE name = 'max_connections') as max_connections
    )
    SELECT
        tc.total_tasks,
        tc.pending_tasks,
        tc.in_progress_tasks,
        tc.complete_tasks,
        tc.error_tasks,
        tc.cancelled_tasks,
        sc.total_steps,
        sc.pending_steps,
        sc.in_progress_steps,
        sc.complete_steps,
        sc.error_steps,
        sc.retryable_error_steps,
        sc.exhausted_retry_steps,
        sc.in_backoff_steps,
        ci.active_connections,
        ci.max_connections
    FROM task_counts tc
    CROSS JOIN step_counts sc
    CROSS JOIN connection_info ci;
END;
$$;


ALTER FUNCTION public.get_system_health_counts() OWNER TO tasker;

--
-- Name: get_task_execution_context(uuid); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_task_execution_context(input_task_uuid uuid) RETURNS TABLE(task_uuid uuid, named_task_uuid uuid, status text, total_steps bigint, pending_steps bigint, in_progress_steps bigint, completed_steps bigint, failed_steps bigint, ready_steps bigint, execution_status text, recommended_action text, completion_percentage numeric, health_status text)
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
    FROM tasker_tasks t
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
    WHERE t.task_uuid = input_task_uuid
  ),
  aggregated_stats AS (
    SELECT
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      -- Count PERMANENTLY blocked failures (exhausted retries OR explicitly marked as not retryable)
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.retry_limit) THEN 1 END) as permanently_blocked_steps
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

    -- FIXED: Execution State Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'processing'
      -- OLD BUG: WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      -- NEW FIX: Only blocked if failed steps are NOT retry-eligible
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- FIXED: Recommended Action Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      -- OLD BUG: WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'handle_failures'
      -- NEW FIX: Only handle failures if they're truly blocked
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'handle_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Progress Metrics
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    -- FIXED: Health Status Logic
    CASE
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      -- NEW FIX: Only blocked if failures are truly not retry-eligible
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      -- NEW: Waiting state for retry-eligible failures with backoff
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status

  FROM task_info ti
  CROSS JOIN aggregated_stats ast;
END;
$$;


ALTER FUNCTION public.get_task_execution_context(input_task_uuid uuid) OWNER TO tasker;

--
-- Name: get_task_execution_contexts_batch(uuid[]); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.get_task_execution_contexts_batch(input_task_uuids uuid[]) RETURNS TABLE(task_uuid uuid, named_task_uuid uuid, status text, total_steps bigint, pending_steps bigint, in_progress_steps bigint, completed_steps bigint, failed_steps bigint, ready_steps bigint, execution_status text, recommended_action text, completion_percentage numeric, health_status text)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    SELECT * FROM get_step_readiness_status_batch(input_task_uuids)
  ),
  task_info AS (
    SELECT
      t.task_uuid,
      t.named_task_uuid,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM unnest(input_task_uuids) AS task_list(task_uuid)
    JOIN tasker_tasks t ON t.task_uuid = task_list.task_uuid
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
  ),
  aggregated_stats AS (
    SELECT
      sd.task_uuid,
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      COUNT(CASE WHEN sd.current_state = 'error' AND (sd.attempts >= sd.retry_limit) THEN 1 END) as permanently_blocked_steps
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

    -- Execution status
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'processing'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- Recommended action
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'process_ready_steps'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'review_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'mark_complete'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Completion percentage
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 100.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::numeric / ast.total_steps::numeric) * 100, 2)
    END as completion_percentage,

    -- Health Status Logic
    CASE
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status

  FROM task_info ti
  LEFT JOIN aggregated_stats ast ON ast.task_uuid = ti.task_uuid;
END;
$$;


ALTER FUNCTION public.get_task_execution_contexts_batch(input_task_uuids uuid[]) OWNER TO tasker;

--
-- Name: pgmq_auto_add_headers_trigger(); Type: FUNCTION; Schema: public; Owner: tasker
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


ALTER FUNCTION public.pgmq_auto_add_headers_trigger() OWNER TO tasker;

--
-- Name: FUNCTION pgmq_auto_add_headers_trigger(); Type: COMMENT; Schema: public; Owner: tasker
--

COMMENT ON FUNCTION public.pgmq_auto_add_headers_trigger() IS 'Event trigger function to automatically add headers column to new pgmq queue tables';


--
-- Name: pgmq_ensure_headers_column(text); Type: FUNCTION; Schema: public; Owner: tasker
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


ALTER FUNCTION public.pgmq_ensure_headers_column(queue_name text) OWNER TO tasker;

--
-- Name: FUNCTION pgmq_ensure_headers_column(queue_name text); Type: COMMENT; Schema: public; Owner: tasker
--

COMMENT ON FUNCTION public.pgmq_ensure_headers_column(queue_name text) IS 'Ensures a pgmq queue table has the headers JSONB column required by pgmq extension v1.5.1+';


--
-- Name: release_task_claim(uuid, character varying); Type: FUNCTION; Schema: public; Owner: tasker
--

CREATE FUNCTION public.release_task_claim(p_task_uuid uuid, p_orchestrator_id character varying) RETURNS boolean
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
        AND claimed_by = p_orchestrator_id;  -- Only release if we own the claim

    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;

    -- Return true if claim was released, false if not found or not owned
    RETURN v_rows_updated > 0;
END;
$$;


ALTER FUNCTION public.release_task_claim(p_task_uuid uuid, p_orchestrator_id character varying) OWNER TO tasker;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: _sqlx_migrations; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public._sqlx_migrations (
    version bigint NOT NULL,
    description text NOT NULL,
    installed_on timestamp with time zone DEFAULT now() NOT NULL,
    success boolean NOT NULL,
    checksum bytea NOT NULL,
    execution_time bigint NOT NULL
);


ALTER TABLE public._sqlx_migrations OWNER TO tasker;

--
-- Name: tasker_annotation_types; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_annotation_types (
    annotation_type_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_annotation_types OWNER TO tasker;

--
-- Name: tasker_annotation_types_annotation_type_id_seq; Type: SEQUENCE; Schema: public; Owner: tasker
--

CREATE SEQUENCE public.tasker_annotation_types_annotation_type_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.tasker_annotation_types_annotation_type_id_seq OWNER TO tasker;

--
-- Name: tasker_annotation_types_annotation_type_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: tasker
--

ALTER SEQUENCE public.tasker_annotation_types_annotation_type_id_seq OWNED BY public.tasker_annotation_types.annotation_type_id;


--
-- Name: tasker_dependent_system_object_maps; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_dependent_system_object_maps (
    dependent_system_object_map_id bigint NOT NULL,
    dependent_system_one_uuid uuid NOT NULL,
    dependent_system_two_uuid uuid NOT NULL,
    remote_id_one character varying(128) NOT NULL,
    remote_id_two character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_dependent_system_object_maps OWNER TO tasker;

--
-- Name: tasker_dependent_system_objec_dependent_system_object_map_i_seq; Type: SEQUENCE; Schema: public; Owner: tasker
--

CREATE SEQUENCE public.tasker_dependent_system_objec_dependent_system_object_map_i_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.tasker_dependent_system_objec_dependent_system_object_map_i_seq OWNER TO tasker;

--
-- Name: tasker_dependent_system_objec_dependent_system_object_map_i_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: tasker
--

ALTER SEQUENCE public.tasker_dependent_system_objec_dependent_system_object_map_i_seq OWNED BY public.tasker_dependent_system_object_maps.dependent_system_object_map_id;


--
-- Name: tasker_dependent_systems; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_dependent_systems (
    dependent_system_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_dependent_systems OWNER TO tasker;

--
-- Name: tasker_named_steps; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_named_steps (
    named_step_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    dependent_system_uuid uuid NOT NULL,
    name character varying(128) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_named_steps OWNER TO tasker;

--
-- Name: tasker_named_tasks; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_named_tasks (
    named_task_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    task_namespace_uuid uuid NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    version character varying(16) DEFAULT '0.1.0'::character varying NOT NULL,
    configuration jsonb DEFAULT '{}'::jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_named_tasks OWNER TO tasker;

--
-- Name: tasker_named_tasks_named_steps; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_named_tasks_named_steps (
    ntns_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    named_task_uuid uuid NOT NULL,
    named_step_uuid uuid NOT NULL,
    skippable boolean DEFAULT false NOT NULL,
    default_retryable boolean DEFAULT true NOT NULL,
    default_retry_limit integer DEFAULT 3 NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_named_tasks_named_steps OWNER TO tasker;

--
-- Name: tasker_task_namespaces; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_task_namespaces (
    task_namespace_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_task_namespaces OWNER TO tasker;

--
-- Name: tasker_tasks; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_tasks (
    task_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
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
    claim_id character varying,
    reference_id character varying(255),
    priority integer DEFAULT 0 NOT NULL,
    claim_timeout_seconds integer DEFAULT 60 NOT NULL,
    claimed_at timestamp(6) without time zone,
    claimed_by character varying
);


ALTER TABLE public.tasker_tasks OWNER TO tasker;

--
-- Name: tasker_ready_tasks; Type: VIEW; Schema: public; Owner: tasker
--

CREATE VIEW public.tasker_ready_tasks AS
 SELECT t.task_uuid,
    tn.name AS namespace_name,
    t.priority,
    t.created_at,
    t.updated_at,
    t.claimed_at,
    t.claimed_by,
    tec.ready_steps AS ready_steps_count,
    tec.execution_status,
    tec.total_steps,
    tec.completed_steps,
    tec.pending_steps,
    tec.failed_steps,
    tec.in_progress_steps,
    (round((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / 3600.0), 2))::double precision AS age_hours,
    (
        CASE
            WHEN (t.priority >= 4) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (300)::numeric), (2)::numeric))
            WHEN (t.priority = 3) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (180)::numeric), (3)::numeric))
            WHEN (t.priority = 2) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (120)::numeric), (4)::numeric))
            WHEN (t.priority = 1) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (60)::numeric), (5)::numeric))
            ELSE ((0)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (30)::numeric), (6)::numeric))
        END)::double precision AS computed_priority,
        CASE
            WHEN (t.claimed_at IS NULL) THEN 'available'::text
            WHEN (t.claimed_at < (now() - ((t.claim_timeout_seconds || ' seconds'::text))::interval)) THEN 'stale_claim'::text
            ELSE 'claimed'::text
        END AS claim_status,
        CASE
            WHEN (t.claimed_at IS NOT NULL) THEN (EXTRACT(epoch FROM (now() - (t.claimed_at)::timestamp with time zone)))::integer
            ELSE NULL::integer
        END AS claimed_duration_seconds
   FROM (((public.tasker_tasks t
     JOIN public.tasker_named_tasks nt ON ((t.named_task_uuid = nt.named_task_uuid)))
     JOIN public.tasker_task_namespaces tn ON ((nt.task_namespace_uuid = tn.task_namespace_uuid)))
     JOIN LATERAL ( SELECT get_task_execution_context.task_uuid,
            get_task_execution_context.named_task_uuid,
            get_task_execution_context.status,
            get_task_execution_context.total_steps,
            get_task_execution_context.pending_steps,
            get_task_execution_context.in_progress_steps,
            get_task_execution_context.completed_steps,
            get_task_execution_context.failed_steps,
            get_task_execution_context.ready_steps,
            get_task_execution_context.execution_status,
            get_task_execution_context.recommended_action,
            get_task_execution_context.completion_percentage,
            get_task_execution_context.health_status
           FROM public.get_task_execution_context(t.task_uuid) get_task_execution_context(task_uuid, named_task_uuid, status, total_steps, pending_steps, in_progress_steps, completed_steps, failed_steps, ready_steps, execution_status, recommended_action, completion_percentage, health_status)) tec ON (true))
  WHERE ((t.complete = false) AND (tec.ready_steps > 0) AND (tec.execution_status = ANY (ARRAY['processing'::text, 'pending'::text, 'has_ready_steps'::text])) AND ((t.claimed_at IS NULL) OR (t.claimed_at < (now() - ((t.claim_timeout_seconds || ' seconds'::text))::interval))))
  ORDER BY ((
        CASE
            WHEN (t.priority >= 4) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (300)::numeric), (2)::numeric))
            WHEN (t.priority = 3) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (180)::numeric), (3)::numeric))
            WHEN (t.priority = 2) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (120)::numeric), (4)::numeric))
            WHEN (t.priority = 1) THEN ((t.priority)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (60)::numeric), (5)::numeric))
            ELSE ((0)::numeric + LEAST((EXTRACT(epoch FROM (now() - (t.created_at)::timestamp with time zone)) / (30)::numeric), (6)::numeric))
        END)::double precision) DESC, t.created_at;


ALTER VIEW public.tasker_ready_tasks OWNER TO tasker;

--
-- Name: tasker_workflow_step_edges; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_workflow_step_edges (
    workflow_step_edge_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    from_step_uuid uuid NOT NULL,
    to_step_uuid uuid NOT NULL,
    name character varying NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_workflow_step_edges OWNER TO tasker;

--
-- Name: tasker_workflow_steps; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_workflow_steps (
    workflow_step_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    named_step_uuid uuid NOT NULL,
    retryable boolean DEFAULT true NOT NULL,
    retry_limit integer DEFAULT 3,
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
    claim_id character varying,
    reference_id character varying(255),
    priority integer DEFAULT 0 NOT NULL,
    claim_timeout_seconds integer DEFAULT 60 NOT NULL
);


ALTER TABLE public.tasker_workflow_steps OWNER TO tasker;

--
-- Name: tasker_step_dag_relationships; Type: VIEW; Schema: public; Owner: tasker
--

CREATE VIEW public.tasker_step_dag_relationships AS
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
   FROM (((public.tasker_workflow_steps ws
     LEFT JOIN ( SELECT tasker_workflow_step_edges.to_step_uuid,
            jsonb_agg(tasker_workflow_step_edges.from_step_uuid ORDER BY tasker_workflow_step_edges.from_step_uuid) AS parent_uuids,
            count(*) AS parent_count
           FROM public.tasker_workflow_step_edges
          GROUP BY tasker_workflow_step_edges.to_step_uuid) parent_data ON ((parent_data.to_step_uuid = ws.workflow_step_uuid)))
     LEFT JOIN ( SELECT tasker_workflow_step_edges.from_step_uuid,
            jsonb_agg(tasker_workflow_step_edges.to_step_uuid ORDER BY tasker_workflow_step_edges.to_step_uuid) AS child_uuids,
            count(*) AS child_count
           FROM public.tasker_workflow_step_edges
          GROUP BY tasker_workflow_step_edges.from_step_uuid) child_data ON ((child_data.from_step_uuid = ws.workflow_step_uuid)))
     LEFT JOIN ( WITH RECURSIVE step_depths AS (
                 SELECT ws_inner.workflow_step_uuid,
                    0 AS depth_from_root,
                    ws_inner.task_uuid
                   FROM public.tasker_workflow_steps ws_inner
                  WHERE (NOT (EXISTS ( SELECT 1
                           FROM public.tasker_workflow_step_edges e
                          WHERE (e.to_step_uuid = ws_inner.workflow_step_uuid))))
                UNION ALL
                 SELECT e.to_step_uuid,
                    (sd.depth_from_root + 1),
                    sd.task_uuid
                   FROM (step_depths sd
                     JOIN public.tasker_workflow_step_edges e ON ((e.from_step_uuid = sd.workflow_step_uuid)))
                  WHERE (sd.depth_from_root < 50)
                )
         SELECT step_depths.workflow_step_uuid,
            min(step_depths.depth_from_root) AS min_depth_from_root
           FROM step_depths
          GROUP BY step_depths.workflow_step_uuid) depth_info ON ((depth_info.workflow_step_uuid = ws.workflow_step_uuid)));


ALTER VIEW public.tasker_step_dag_relationships OWNER TO tasker;

--
-- Name: tasker_task_annotations; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_task_annotations (
    task_annotation_id bigint NOT NULL,
    task_uuid uuid NOT NULL,
    annotation_type_id integer NOT NULL,
    annotation jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_task_annotations OWNER TO tasker;

--
-- Name: tasker_task_annotations_task_annotation_id_seq; Type: SEQUENCE; Schema: public; Owner: tasker
--

CREATE SEQUENCE public.tasker_task_annotations_task_annotation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.tasker_task_annotations_task_annotation_id_seq OWNER TO tasker;

--
-- Name: tasker_task_annotations_task_annotation_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: tasker
--

ALTER SEQUENCE public.tasker_task_annotations_task_annotation_id_seq OWNED BY public.tasker_task_annotations.task_annotation_id;


--
-- Name: tasker_task_transitions; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_task_transitions (
    task_transition_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_task_transitions OWNER TO tasker;

--
-- Name: tasker_workflow_step_transitions; Type: TABLE; Schema: public; Owner: tasker
--

CREATE TABLE public.tasker_workflow_step_transitions (
    workflow_step_transition_uuid uuid DEFAULT public.uuid_generate_v7() NOT NULL,
    workflow_step_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


ALTER TABLE public.tasker_workflow_step_transitions OWNER TO tasker;

--
-- Name: tasker_annotation_types annotation_type_id; Type: DEFAULT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_annotation_types ALTER COLUMN annotation_type_id SET DEFAULT nextval('public.tasker_annotation_types_annotation_type_id_seq'::regclass);


--
-- Name: tasker_dependent_system_object_maps dependent_system_object_map_id; Type: DEFAULT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps ALTER COLUMN dependent_system_object_map_id SET DEFAULT nextval('public.tasker_dependent_system_objec_dependent_system_object_map_i_seq'::regclass);


--
-- Name: tasker_task_annotations task_annotation_id; Type: DEFAULT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_annotations ALTER COLUMN task_annotation_id SET DEFAULT nextval('public.tasker_task_annotations_task_annotation_id_seq'::regclass);


--
-- Data for Name: _sqlx_migrations; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public._sqlx_migrations (version, description, installed_on, success, checksum, execution_time) FROM stdin;
-- 20250810140000	uuid v7 initial schema	2025-08-11 13:16:46.368056+00	t	\\x312cf240255ef5a1f1b1da1b3786140d29588506eb13713cd82c694e74dd4be41c84935259caf85fb4416582ed3d2b6a	37098334
\.


--
-- Data for Name: tasker_annotation_types; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_annotation_types (annotation_type_id, name, description, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_dependent_system_object_maps; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_dependent_system_object_maps (dependent_system_object_map_id, dependent_system_one_uuid, dependent_system_two_uuid, remote_id_one, remote_id_two, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_dependent_systems; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_dependent_systems (dependent_system_uuid, name, description, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_named_steps; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_named_steps (named_step_uuid, dependent_system_uuid, name, description, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_named_tasks; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_named_tasks (named_task_uuid, task_namespace_uuid, name, description, version, configuration, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_named_tasks_named_steps; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_named_tasks_named_steps (ntns_uuid, named_task_uuid, named_step_uuid, skippable, default_retryable, default_retry_limit, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_task_annotations; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_task_annotations (task_annotation_id, task_uuid, annotation_type_id, annotation, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_task_namespaces; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_task_namespaces (task_namespace_uuid, name, description, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_task_transitions; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_task_transitions (task_transition_uuid, task_uuid, to_state, from_state, metadata, sort_key, most_recent, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_tasks; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_tasks (task_uuid, named_task_uuid, complete, requested_at, completed_at, initiator, source_system, reason, bypass_steps, tags, context, identity_hash, created_at, updated_at, claim_id, reference_id, priority, claim_timeout_seconds, claimed_at, claimed_by) FROM stdin;
\.


--
-- Data for Name: tasker_workflow_step_edges; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_workflow_step_edges (workflow_step_edge_uuid, from_step_uuid, to_step_uuid, name, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_workflow_step_transitions; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_workflow_step_transitions (workflow_step_transition_uuid, workflow_step_uuid, to_state, from_state, metadata, sort_key, most_recent, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: tasker_workflow_steps; Type: TABLE DATA; Schema: public; Owner: tasker
--

COPY public.tasker_workflow_steps (workflow_step_uuid, task_uuid, named_step_uuid, retryable, retry_limit, in_process, processed, processed_at, attempts, last_attempted_at, backoff_request_seconds, inputs, results, skippable, created_at, updated_at, claim_id, reference_id, priority, claim_timeout_seconds) FROM stdin;
\.


--
-- Name: tasker_annotation_types_annotation_type_id_seq; Type: SEQUENCE SET; Schema: public; Owner: tasker
--

SELECT pg_catalog.setval('public.tasker_annotation_types_annotation_type_id_seq', 1, false);


--
-- Name: tasker_dependent_system_objec_dependent_system_object_map_i_seq; Type: SEQUENCE SET; Schema: public; Owner: tasker
--

SELECT pg_catalog.setval('public.tasker_dependent_system_objec_dependent_system_object_map_i_seq', 1, false);


--
-- Name: tasker_task_annotations_task_annotation_id_seq; Type: SEQUENCE SET; Schema: public; Owner: tasker
--

SELECT pg_catalog.setval('public.tasker_task_annotations_task_annotation_id_seq', 1, false);


--
-- Name: _sqlx_migrations _sqlx_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public._sqlx_migrations
    ADD CONSTRAINT _sqlx_migrations_pkey PRIMARY KEY (version);


--
-- Name: tasker_annotation_types tasker_annotation_types_name_key; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_annotation_types
    ADD CONSTRAINT tasker_annotation_types_name_key UNIQUE (name);


--
-- Name: tasker_annotation_types tasker_annotation_types_name_unique; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_annotation_types
    ADD CONSTRAINT tasker_annotation_types_name_unique UNIQUE (name);


--
-- Name: tasker_annotation_types tasker_annotation_types_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_annotation_types
    ADD CONSTRAINT tasker_annotation_types_pkey PRIMARY KEY (annotation_type_id);


--
-- Name: tasker_dependent_system_object_maps tasker_dependent_system_object_maps_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT tasker_dependent_system_object_maps_pkey PRIMARY KEY (dependent_system_object_map_id);


--
-- Name: tasker_dependent_systems tasker_dependent_systems_name_key; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_dependent_systems
    ADD CONSTRAINT tasker_dependent_systems_name_key UNIQUE (name);


--
-- Name: tasker_dependent_systems tasker_dependent_systems_name_unique; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_dependent_systems
    ADD CONSTRAINT tasker_dependent_systems_name_unique UNIQUE (name);


--
-- Name: tasker_dependent_systems tasker_dependent_systems_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_dependent_systems
    ADD CONSTRAINT tasker_dependent_systems_pkey PRIMARY KEY (dependent_system_uuid);


--
-- Name: tasker_named_steps tasker_named_steps_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT tasker_named_steps_pkey PRIMARY KEY (named_step_uuid);


--
-- Name: tasker_named_steps tasker_named_steps_system_name_unique; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT tasker_named_steps_system_name_unique UNIQUE (dependent_system_uuid, name);


--
-- Name: tasker_named_tasks_named_steps tasker_named_tasks_named_steps_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_pkey PRIMARY KEY (ntns_uuid);


--
-- Name: tasker_named_tasks_named_steps tasker_named_tasks_named_steps_unique; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_unique UNIQUE (named_task_uuid, named_step_uuid);


--
-- Name: tasker_named_tasks tasker_named_tasks_namespace_name_unique; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_namespace_name_unique UNIQUE (task_namespace_uuid, name);


--
-- Name: tasker_named_tasks tasker_named_tasks_namespace_name_version_unique; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_namespace_name_version_unique UNIQUE (task_namespace_uuid, name, version);


--
-- Name: tasker_named_tasks tasker_named_tasks_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_pkey PRIMARY KEY (named_task_uuid);


--
-- Name: tasker_task_annotations tasker_task_annotations_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT tasker_task_annotations_pkey PRIMARY KEY (task_annotation_id);


--
-- Name: tasker_task_namespaces tasker_task_namespaces_name_key; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_namespaces
    ADD CONSTRAINT tasker_task_namespaces_name_key UNIQUE (name);


--
-- Name: tasker_task_namespaces tasker_task_namespaces_name_unique; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_namespaces
    ADD CONSTRAINT tasker_task_namespaces_name_unique UNIQUE (name);


--
-- Name: tasker_task_namespaces tasker_task_namespaces_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_namespaces
    ADD CONSTRAINT tasker_task_namespaces_pkey PRIMARY KEY (task_namespace_uuid);


--
-- Name: tasker_task_transitions tasker_task_transitions_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_transitions
    ADD CONSTRAINT tasker_task_transitions_pkey PRIMARY KEY (task_transition_uuid);


--
-- Name: tasker_tasks tasker_tasks_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_tasks
    ADD CONSTRAINT tasker_tasks_pkey PRIMARY KEY (task_uuid);


--
-- Name: tasker_workflow_step_edges tasker_workflow_step_edges_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT tasker_workflow_step_edges_pkey PRIMARY KEY (workflow_step_edge_uuid);


--
-- Name: tasker_workflow_step_transitions tasker_workflow_step_transitions_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_step_transitions
    ADD CONSTRAINT tasker_workflow_step_transitions_pkey PRIMARY KEY (workflow_step_transition_uuid);


--
-- Name: tasker_workflow_steps tasker_workflow_steps_pkey; Type: CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT tasker_workflow_steps_pkey PRIMARY KEY (workflow_step_uuid);


--
-- Name: idx_task_transitions_state_lookup; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_task_transitions_state_lookup ON public.tasker_task_transitions USING btree (task_uuid, to_state, most_recent) WHERE (most_recent = true);


--
-- Name: idx_task_transitions_uuid_most_recent; Type: INDEX; Schema: public; Owner: tasker
--

CREATE UNIQUE INDEX idx_task_transitions_uuid_most_recent ON public.tasker_task_transitions USING btree (task_uuid, most_recent) WHERE (most_recent = true);


--
-- Name: idx_task_transitions_uuid_sort_key; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_task_transitions_uuid_sort_key ON public.tasker_task_transitions USING btree (task_uuid, sort_key);


--
-- Name: idx_task_transitions_uuid_temporal; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_task_transitions_uuid_temporal ON public.tasker_task_transitions USING btree (task_transition_uuid, created_at);


--
-- Name: idx_tasker_dependent_systems_name; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_dependent_systems_name ON public.tasker_dependent_systems USING btree (name);


--
-- Name: idx_tasker_named_steps_system_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_named_steps_system_uuid ON public.tasker_named_steps USING btree (dependent_system_uuid);


--
-- Name: idx_tasker_named_tasks_namespace_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_named_tasks_namespace_uuid ON public.tasker_named_tasks USING btree (task_namespace_uuid);


--
-- Name: idx_tasker_task_annotations_task_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_task_annotations_task_uuid ON public.tasker_task_annotations USING btree (task_uuid);


--
-- Name: idx_tasker_task_annotations_type_id; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_task_annotations_type_id ON public.tasker_task_annotations USING btree (annotation_type_id);


--
-- Name: idx_tasker_task_namespaces_name; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_task_namespaces_name ON public.tasker_task_namespaces USING btree (name);


--
-- Name: idx_tasker_task_transitions_most_recent; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_task_transitions_most_recent ON public.tasker_task_transitions USING btree (most_recent) WHERE (most_recent = true);


--
-- Name: idx_tasker_task_transitions_task_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_task_transitions_task_uuid ON public.tasker_task_transitions USING btree (task_uuid);


--
-- Name: idx_tasker_tasks_claiming; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_claiming ON public.tasker_tasks USING btree (claimed_at, claimed_by) WHERE (claimed_at IS NOT NULL);


--
-- Name: idx_tasker_tasks_complete; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_complete ON public.tasker_tasks USING btree (complete);


--
-- Name: idx_tasker_tasks_completed_at; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_completed_at ON public.tasker_tasks USING btree (completed_at);


--
-- Name: idx_tasker_tasks_identity_hash; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_identity_hash ON public.tasker_tasks USING btree (identity_hash);


--
-- Name: idx_tasker_tasks_named_task_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_named_task_uuid ON public.tasker_tasks USING btree (named_task_uuid);


--
-- Name: idx_tasker_tasks_priority; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_priority ON public.tasker_tasks USING btree (priority);


--
-- Name: idx_tasker_tasks_requested_at; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_requested_at ON public.tasker_tasks USING btree (requested_at);


--
-- Name: idx_tasker_tasks_source_system; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_source_system ON public.tasker_tasks USING btree (source_system);


--
-- Name: idx_tasker_tasks_tags_gin; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_tags_gin ON public.tasker_tasks USING gin (tags);


--
-- Name: idx_tasker_tasks_tags_gin_path; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_tags_gin_path ON public.tasker_tasks USING gin (tags jsonb_path_ops);


--
-- Name: idx_tasker_tasks_unclaimed; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_tasks_unclaimed ON public.tasker_tasks USING btree (complete, priority, created_at) WHERE ((complete = false) AND (claimed_at IS NULL));


--
-- Name: idx_tasker_workflow_step_edges_from_step; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_workflow_step_edges_from_step ON public.tasker_workflow_step_edges USING btree (from_step_uuid);


--
-- Name: idx_tasker_workflow_step_edges_to_step; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_workflow_step_edges_to_step ON public.tasker_workflow_step_edges USING btree (to_step_uuid);


--
-- Name: idx_tasker_workflow_step_transitions_most_recent; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_workflow_step_transitions_most_recent ON public.tasker_workflow_step_transitions USING btree (most_recent) WHERE (most_recent = true);


--
-- Name: idx_tasker_workflow_step_transitions_step_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_workflow_step_transitions_step_uuid ON public.tasker_workflow_step_transitions USING btree (workflow_step_uuid);


--
-- Name: idx_tasker_workflow_steps_named_step_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_workflow_steps_named_step_uuid ON public.tasker_workflow_steps USING btree (named_step_uuid);


--
-- Name: idx_tasker_workflow_steps_task_uuid; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasker_workflow_steps_task_uuid ON public.tasker_workflow_steps USING btree (task_uuid);


--
-- Name: idx_tasks_active_with_priority_covering; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasks_active_with_priority_covering ON public.tasker_tasks USING btree (complete, priority, task_uuid) INCLUDE (named_task_uuid, requested_at) WHERE (complete = false);


--
-- Name: idx_tasks_uuid_temporal; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_tasks_uuid_temporal ON public.tasker_tasks USING btree (task_uuid, created_at);


--
-- Name: idx_workflow_step_transitions_state_lookup; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_step_transitions_state_lookup ON public.tasker_workflow_step_transitions USING btree (workflow_step_uuid, to_state, most_recent) WHERE (most_recent = true);


--
-- Name: idx_workflow_step_transitions_uuid_most_recent; Type: INDEX; Schema: public; Owner: tasker
--

CREATE UNIQUE INDEX idx_workflow_step_transitions_uuid_most_recent ON public.tasker_workflow_step_transitions USING btree (workflow_step_uuid, most_recent) WHERE (most_recent = true);


--
-- Name: idx_workflow_step_transitions_uuid_sort_key; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_step_transitions_uuid_sort_key ON public.tasker_workflow_step_transitions USING btree (workflow_step_uuid, sort_key);


--
-- Name: idx_workflow_step_transitions_uuid_temporal; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_step_transitions_uuid_temporal ON public.tasker_workflow_step_transitions USING btree (workflow_step_transition_uuid, created_at);


--
-- Name: idx_workflow_steps_active_operations; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_active_operations ON public.tasker_workflow_steps USING btree (workflow_step_uuid, task_uuid) WHERE ((processed = false) OR (processed IS NULL));


--
-- Name: idx_workflow_steps_processed_at; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_processed_at ON public.tasker_workflow_steps USING btree (processed_at);


--
-- Name: idx_workflow_steps_processing_status; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_processing_status ON public.tasker_workflow_steps USING btree (task_uuid, processed, in_process);


--
-- Name: idx_workflow_steps_ready_covering; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_ready_covering ON public.tasker_workflow_steps USING btree (task_uuid, processed, in_process) INCLUDE (workflow_step_uuid, attempts, retry_limit, retryable) WHERE (processed = false);


--
-- Name: idx_workflow_steps_retry_logic; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_retry_logic ON public.tasker_workflow_steps USING btree (attempts, retry_limit, retryable);


--
-- Name: idx_workflow_steps_retry_status; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_retry_status ON public.tasker_workflow_steps USING btree (attempts, retry_limit);


--
-- Name: idx_workflow_steps_task_covering; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_task_covering ON public.tasker_workflow_steps USING btree (task_uuid) INCLUDE (workflow_step_uuid, processed, in_process, attempts, retry_limit);


--
-- Name: idx_workflow_steps_task_grouping_active; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_task_grouping_active ON public.tasker_workflow_steps USING btree (task_uuid, workflow_step_uuid) WHERE ((processed = false) OR (processed IS NULL));


--
-- Name: idx_workflow_steps_task_readiness; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_task_readiness ON public.tasker_workflow_steps USING btree (task_uuid, processed, workflow_step_uuid) WHERE (processed = false);


--
-- Name: idx_workflow_steps_transitive_deps; Type: INDEX; Schema: public; Owner: tasker
--

CREATE INDEX idx_workflow_steps_transitive_deps ON public.tasker_workflow_steps USING btree (workflow_step_uuid, named_step_uuid) INCLUDE (task_uuid, results, processed);


--
-- Name: tasker_dependent_system_object_maps tasker_dependent_system_object_maps_dependent_system_one_uuid_f; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT tasker_dependent_system_object_maps_dependent_system_one_uuid_f FOREIGN KEY (dependent_system_one_uuid) REFERENCES public.tasker_dependent_systems(dependent_system_uuid);


--
-- Name: tasker_dependent_system_object_maps tasker_dependent_system_object_maps_dependent_system_two_uuid_f; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT tasker_dependent_system_object_maps_dependent_system_two_uuid_f FOREIGN KEY (dependent_system_two_uuid) REFERENCES public.tasker_dependent_systems(dependent_system_uuid);


--
-- Name: tasker_named_steps tasker_named_steps_dependent_system_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT tasker_named_steps_dependent_system_uuid_fkey FOREIGN KEY (dependent_system_uuid) REFERENCES public.tasker_dependent_systems(dependent_system_uuid);


--
-- Name: tasker_named_tasks_named_steps tasker_named_tasks_named_steps_named_step_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_named_step_uuid_fkey FOREIGN KEY (named_step_uuid) REFERENCES public.tasker_named_steps(named_step_uuid);


--
-- Name: tasker_named_tasks_named_steps tasker_named_tasks_named_steps_named_task_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_named_task_uuid_fkey FOREIGN KEY (named_task_uuid) REFERENCES public.tasker_named_tasks(named_task_uuid);


--
-- Name: tasker_named_tasks tasker_named_tasks_task_namespace_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_task_namespace_uuid_fkey FOREIGN KEY (task_namespace_uuid) REFERENCES public.tasker_task_namespaces(task_namespace_uuid);


--
-- Name: tasker_task_annotations tasker_task_annotations_annotation_type_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT tasker_task_annotations_annotation_type_id_fkey FOREIGN KEY (annotation_type_id) REFERENCES public.tasker_annotation_types(annotation_type_id);


--
-- Name: tasker_task_annotations tasker_task_annotations_task_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT tasker_task_annotations_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES public.tasker_tasks(task_uuid);


--
-- Name: tasker_task_transitions tasker_task_transitions_task_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_task_transitions
    ADD CONSTRAINT tasker_task_transitions_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES public.tasker_tasks(task_uuid);


--
-- Name: tasker_tasks tasker_tasks_named_task_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_tasks
    ADD CONSTRAINT tasker_tasks_named_task_uuid_fkey FOREIGN KEY (named_task_uuid) REFERENCES public.tasker_named_tasks(named_task_uuid);


--
-- Name: tasker_workflow_step_edges tasker_workflow_step_edges_from_step_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT tasker_workflow_step_edges_from_step_uuid_fkey FOREIGN KEY (from_step_uuid) REFERENCES public.tasker_workflow_steps(workflow_step_uuid);


--
-- Name: tasker_workflow_step_edges tasker_workflow_step_edges_to_step_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT tasker_workflow_step_edges_to_step_uuid_fkey FOREIGN KEY (to_step_uuid) REFERENCES public.tasker_workflow_steps(workflow_step_uuid);


--
-- Name: tasker_workflow_step_transitions tasker_workflow_step_transitions_workflow_step_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_step_transitions
    ADD CONSTRAINT tasker_workflow_step_transitions_workflow_step_uuid_fkey FOREIGN KEY (workflow_step_uuid) REFERENCES public.tasker_workflow_steps(workflow_step_uuid);


--
-- Name: tasker_workflow_steps tasker_workflow_steps_named_step_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT tasker_workflow_steps_named_step_uuid_fkey FOREIGN KEY (named_step_uuid) REFERENCES public.tasker_named_steps(named_step_uuid);


--
-- Name: tasker_workflow_steps tasker_workflow_steps_task_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: tasker
--

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT tasker_workflow_steps_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES public.tasker_tasks(task_uuid);


--
-- Name: DEFAULT PRIVILEGES FOR SEQUENCES; Type: DEFAULT ACL; Schema: pgmq; Owner: tasker
--

ALTER DEFAULT PRIVILEGES FOR ROLE tasker IN SCHEMA pgmq GRANT SELECT ON SEQUENCES TO pg_monitor;


--
-- Name: DEFAULT PRIVILEGES FOR TABLES; Type: DEFAULT ACL; Schema: pgmq; Owner: tasker
--

ALTER DEFAULT PRIVILEGES FOR ROLE tasker IN SCHEMA pgmq GRANT SELECT ON TABLES TO pg_monitor;


--
-- Name: pgmq_headers_compatibility_trigger; Type: EVENT TRIGGER; Schema: -; Owner: tasker
--

CREATE EVENT TRIGGER pgmq_headers_compatibility_trigger ON ddl_command_end
         WHEN TAG IN ('CREATE TABLE')
   EXECUTE FUNCTION public.pgmq_auto_add_headers_trigger();


ALTER EVENT TRIGGER pgmq_headers_compatibility_trigger OWNER TO tasker;

--
-- Name: EVENT TRIGGER pgmq_headers_compatibility_trigger; Type: COMMENT; Schema: -; Owner: tasker
--

COMMENT ON EVENT TRIGGER pgmq_headers_compatibility_trigger IS 'Automatically adds headers column to new pgmq queue tables for pgmq-rs compatibility';


--
-- PostgreSQL database dump complete
--

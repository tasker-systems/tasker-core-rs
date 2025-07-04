-- Initial schema migration for Tasker Core Rust
-- Copied directly from production-ready Rails Tasker engine structure
-- Source: /Users/petetaylor/projects/tasker-core-rs/db/structure.sql
-- This ensures 100% accuracy with the Rails engine database schema

-- Name: calculate_dependency_levels(bigint); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.calculate_dependency_levels(input_task_id bigint) RETURNS TABLE(workflow_step_id bigint, dependency_level integer)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH RECURSIVE dependency_levels AS (
    -- Base case: Find root nodes (steps with no dependencies)
    SELECT
      ws.workflow_step_id,
      0 as level
    FROM tasker_workflow_steps ws
    WHERE ws.task_id = input_task_id
      AND NOT EXISTS (
        SELECT 1
        FROM tasker_workflow_step_edges wse
        WHERE wse.to_step_id = ws.workflow_step_id
      )

    UNION ALL

    -- Recursive case: Find children of current level nodes
    SELECT
      wse.to_step_id as workflow_step_id,
      dl.level + 1 as level
    FROM dependency_levels dl
    JOIN tasker_workflow_step_edges wse ON wse.from_step_id = dl.workflow_step_id
    JOIN tasker_workflow_steps ws ON ws.workflow_step_id = wse.to_step_id
    WHERE ws.task_id = input_task_id
  )
  SELECT
    dl.workflow_step_id,
    MAX(dl.level) as dependency_level  -- Use MAX to handle multiple paths to same node
  FROM dependency_levels dl
  GROUP BY dl.workflow_step_id
  ORDER BY dependency_level, workflow_step_id;
END;
$$;


--
-- Name: get_analytics_metrics_v01(timestamp with time zone); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_analytics_metrics_v01(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone) RETURNS TABLE(active_tasks_count bigint, total_namespaces_count bigint, unique_task_types_count bigint, system_health_score numeric, task_throughput bigint, completion_count bigint, error_count bigint, completion_rate numeric, error_rate numeric, avg_task_duration numeric, avg_step_duration numeric, step_throughput bigint, analysis_period_start timestamp with time zone, calculated_at timestamp with time zone)
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    -- Set analysis start time (default to 1 hour ago if not provided)
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '1 hour');

    RETURN QUERY
    WITH active_tasks AS (
        SELECT COUNT(DISTINCT t.task_id) as active_count
        FROM tasker_tasks t
        INNER JOIN tasker_workflow_steps ws ON ws.task_id = t.task_id
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_id = ws.workflow_step_id
        WHERE wst.most_recent = true
          AND wst.to_state NOT IN ('complete', 'error', 'skipped', 'resolved_manually')
    ),
    namespace_summary AS (
        SELECT COUNT(DISTINCT tn.name) as namespace_count
        FROM tasker_task_namespaces tn
        INNER JOIN tasker_named_tasks nt ON nt.task_namespace_id = tn.task_namespace_id
        INNER JOIN tasker_tasks t ON t.named_task_id = nt.named_task_id
    ),
    task_type_summary AS (
        SELECT COUNT(DISTINCT nt.name) as task_type_count
        FROM tasker_named_tasks nt
        INNER JOIN tasker_tasks t ON t.named_task_id = nt.named_task_id
    ),
    recent_task_health AS (
        SELECT
            COUNT(DISTINCT t.task_id) as total_recent_tasks,
            COUNT(DISTINCT t.task_id) FILTER (
                WHERE wst.to_state = 'complete' AND wst.most_recent = true
            ) as completed_tasks,
            COUNT(DISTINCT t.task_id) FILTER (
                WHERE wst.to_state = 'error' AND wst.most_recent = true
            ) as error_tasks
        FROM tasker_tasks t
        INNER JOIN tasker_workflow_steps ws ON ws.task_id = t.task_id
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_id = ws.workflow_step_id
        WHERE t.created_at > NOW() - INTERVAL '1 hour'
    ),
    period_metrics AS (
        SELECT
            COUNT(DISTINCT t.task_id) as throughput,
            COUNT(DISTINCT t.task_id) FILTER (
                WHERE completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
            ) as completions,
            COUNT(DISTINCT t.task_id) FILTER (
                WHERE error_wst.to_state = 'error' AND error_wst.most_recent = true
            ) as errors,
            COUNT(DISTINCT ws.workflow_step_id) as step_count,
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
        LEFT JOIN tasker_workflow_steps ws ON ws.task_id = t.task_id
        LEFT JOIN tasker_workflow_step_transitions completed_wst ON completed_wst.workflow_step_id = ws.workflow_step_id
            AND completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
        LEFT JOIN tasker_workflow_step_transitions error_wst ON error_wst.workflow_step_id = ws.workflow_step_id
            AND error_wst.to_state = 'error' AND error_wst.most_recent = true
        LEFT JOIN tasker_workflow_step_transitions step_completed ON step_completed.workflow_step_id = ws.workflow_step_id
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
-- Name: get_slowest_steps_v01(timestamp with time zone, integer, text, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_slowest_steps_v01(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone, limit_count integer DEFAULT 10, namespace_filter text DEFAULT NULL::text, task_name_filter text DEFAULT NULL::text, version_filter text DEFAULT NULL::text) RETURNS TABLE(workflow_step_id bigint, task_id bigint, step_name character varying, task_name character varying, namespace_name character varying, version character varying, duration_seconds numeric, attempts integer, created_at timestamp with time zone, completed_at timestamp with time zone, retryable boolean, step_status character varying)
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    -- Set analysis start time (default to 24 hours ago if not provided)
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH step_durations AS (
        SELECT
            ws.workflow_step_id,
            ws.task_id,
            ns.name as step_name,
            nt.name as task_name,
            tn.name as namespace_name,
            nt.version,
            ws.created_at,
            ws.attempts,
            ws.retryable,
            -- Find the completion time
            wst.created_at as completion_time,
            wst.to_state as final_state,
            -- Calculate duration from step creation to completion
            EXTRACT(EPOCH FROM (wst.created_at - ws.created_at)) as duration_seconds
        FROM tasker_workflow_steps ws
        INNER JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
        INNER JOIN tasker_tasks t ON t.task_id = ws.task_id
        INNER JOIN tasker_named_tasks nt ON nt.named_task_id = t.named_task_id
        INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_id = nt.task_namespace_id
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_id = ws.workflow_step_id
        WHERE ws.created_at > analysis_start
          AND wst.most_recent = true
          AND wst.to_state = 'complete'  -- Only include completed steps for accurate duration
          AND (namespace_filter IS NULL OR tn.name = namespace_filter)
          AND (task_name_filter IS NULL OR nt.name = task_name_filter)
          AND (version_filter IS NULL OR nt.version = version_filter)
    )
    SELECT
        sd.workflow_step_id,
        sd.task_id,
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
      AND sd.duration_seconds > 0  -- Filter out negative durations (data integrity check)
    ORDER BY sd.duration_seconds DESC
    LIMIT limit_count;
END;
$$;


--
-- Name: get_slowest_tasks_v01(timestamp with time zone, integer, text, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_slowest_tasks_v01(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone, limit_count integer DEFAULT 10, namespace_filter text DEFAULT NULL::text, task_name_filter text DEFAULT NULL::text, version_filter text DEFAULT NULL::text) RETURNS TABLE(task_id bigint, task_name character varying, namespace_name character varying, version character varying, duration_seconds numeric, step_count bigint, completed_steps bigint, error_steps bigint, created_at timestamp with time zone, completed_at timestamp with time zone, initiator character varying, source_system character varying)
    LANGUAGE plpgsql STABLE
    AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    -- Set analysis start time (default to 24 hours ago if not provided)
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH task_durations AS (
        SELECT
            t.task_id,
            nt.name as task_name,
            tn.name as namespace_name,
            nt.version,
            t.created_at,
            t.initiator,
            t.source_system,
            -- Find the latest completion time across all steps
            MAX(wst.created_at) FILTER (
                WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
            ) as latest_completion,
            -- Calculate duration from task creation to latest step completion
            EXTRACT(EPOCH FROM (
                MAX(wst.created_at) FILTER (
                    WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
                ) - t.created_at
            )) as duration_seconds,
            COUNT(ws.workflow_step_id) as total_steps,
            COUNT(ws.workflow_step_id) FILTER (
                WHERE complete_wst.to_state = 'complete' AND complete_wst.most_recent = true
            ) as completed_step_count,
            COUNT(ws.workflow_step_id) FILTER (
                WHERE error_wst.to_state = 'error' AND error_wst.most_recent = true
            ) as error_step_count
        FROM tasker_tasks t
        INNER JOIN tasker_named_tasks nt ON nt.named_task_id = t.named_task_id
        INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_id = nt.task_namespace_id
        INNER JOIN tasker_workflow_steps ws ON ws.task_id = t.task_id
        LEFT JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_id = ws.workflow_step_id
        LEFT JOIN tasker_workflow_step_transitions complete_wst ON complete_wst.workflow_step_id = ws.workflow_step_id
            AND complete_wst.to_state = 'complete' AND complete_wst.most_recent = true
        LEFT JOIN tasker_workflow_step_transitions error_wst ON error_wst.workflow_step_id = ws.workflow_step_id
            AND error_wst.to_state = 'error' AND error_wst.most_recent = true
        WHERE t.created_at > analysis_start
          AND (namespace_filter IS NULL OR tn.name = namespace_filter)
          AND (task_name_filter IS NULL OR nt.name = task_name_filter)
          AND (version_filter IS NULL OR nt.version = version_filter)
        GROUP BY t.task_id, nt.name, tn.name, nt.version, t.created_at, t.initiator, t.source_system
        HAVING MAX(wst.created_at) FILTER (
            WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
        ) IS NOT NULL  -- Only include tasks that have at least one completed/failed step
    )
    SELECT
        td.task_id,
        td.task_name,
        td.namespace_name,
        td.version,
        ROUND(td.duration_seconds, 3),
        td.total_steps,
        td.completed_step_count,
        td.error_step_count,
        td.created_at,
        td.latest_completion,
        td.initiator,
        td.source_system
    FROM task_durations td
    WHERE td.duration_seconds IS NOT NULL
    ORDER BY td.duration_seconds DESC
    LIMIT limit_count;
END;
$$;


--
-- Name: get_step_readiness_status(bigint, bigint[]); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_step_readiness_status(input_task_id bigint, step_ids bigint[] DEFAULT NULL::bigint[]) RETURNS TABLE(workflow_step_id bigint, task_id bigint, named_step_id integer, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, retry_limit integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  SELECT
    ws.workflow_step_id,
    ws.task_id,
    ws.named_step_id,
    ns.name::TEXT,

    -- Current State Information (optimized using most_recent flag)
    COALESCE(current_state.to_state, 'pending')::TEXT as current_state,

    -- Dependency Analysis (calculated from direct joins)
    CASE
      WHEN dep_edges.to_step_id IS NULL THEN true  -- Root steps (no parents)
      WHEN COUNT(dep_edges.from_step_id) = 0 THEN true  -- Steps with zero dependencies
      WHEN COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id) THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Simplified Retry & Backoff Analysis
    CASE
      WHEN ws.attempts >= COALESCE(ws.retry_limit, 3) THEN false
      WHEN ws.attempts > 0 AND COALESCE(ws.retryable, true) = false THEN false
      WHEN last_failure.created_at IS NULL THEN true
      WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') <= NOW()
      WHEN last_failure.created_at IS NOT NULL THEN
        last_failure.created_at + (
          LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')
        ) <= NOW()
      ELSE true
    END as retry_eligible,

    -- Simplified Final Readiness Calculation
    CASE
      WHEN COALESCE(current_state.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)  -- CRITICAL: Only unprocessed steps can be ready
      AND (dep_edges.to_step_id IS NULL OR
           COUNT(dep_edges.from_step_id) = 0 OR
           COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id))
      AND (ws.attempts < COALESCE(ws.retry_limit, 3))
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
        (last_failure.created_at IS NULL OR
         last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    -- Timing Information
    last_failure.created_at as last_failure_at,
    CASE
      WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN last_failure.created_at IS NOT NULL THEN
        last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    -- Dependency Context (calculated from joins)
    COALESCE(COUNT(dep_edges.from_step_id), 0)::INTEGER as total_parents,
    COALESCE(COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END), 0)::INTEGER as completed_parents,

    -- Retry Context
    ws.attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id

  -- OPTIMIZED: Current State using most_recent flag instead of DISTINCT ON
  LEFT JOIN tasker_workflow_step_transitions current_state
    ON current_state.workflow_step_id = ws.workflow_step_id
    AND current_state.most_recent = true

  -- OPTIMIZED: Dependency check using direct joins (no subquery)
  LEFT JOIN tasker_workflow_step_edges dep_edges
    ON dep_edges.to_step_id = ws.workflow_step_id
  LEFT JOIN tasker_workflow_step_transitions parent_states
    ON parent_states.workflow_step_id = dep_edges.from_step_id
    AND parent_states.most_recent = true

  -- OPTIMIZED: Last failure using index-optimized approach
  LEFT JOIN tasker_workflow_step_transitions last_failure
    ON last_failure.workflow_step_id = ws.workflow_step_id
    AND last_failure.to_state = 'error'
    AND last_failure.most_recent = true

  -- KEY PERFORMANCE IMPROVEMENT: Filter by task first, then optionally by step IDs
  -- CRITICAL FIX: Include ALL steps for task execution context calculation
  -- Only filter by processed status when specifically querying for ready steps
  WHERE ws.task_id = input_task_id
    AND (step_ids IS NULL OR ws.workflow_step_id = ANY(step_ids))

  GROUP BY
    ws.workflow_step_id, ws.task_id, ws.named_step_id, ns.name,
    current_state.to_state, last_failure.created_at,
    ws.attempts, ws.retry_limit, ws.backoff_request_seconds, ws.last_attempted_at,
    ws.in_process, ws.processed, ws.retryable, dep_edges.to_step_id,
    current_state.workflow_step_id, last_failure.workflow_step_id;
END;
$$;


--
-- Name: get_step_readiness_status_batch(bigint[]); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_step_readiness_status_batch(input_task_ids bigint[]) RETURNS TABLE(workflow_step_id bigint, task_id bigint, named_step_id integer, name text, current_state text, dependencies_satisfied boolean, retry_eligible boolean, ready_for_execution boolean, last_failure_at timestamp without time zone, next_retry_at timestamp without time zone, total_parents integer, completed_parents integer, attempts integer, retry_limit integer, backoff_request_seconds integer, last_attempted_at timestamp without time zone)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  SELECT
    ws.workflow_step_id,
    ws.task_id,
    ws.named_step_id,
    ns.name::TEXT,

    -- Current State Information (optimized using most_recent flag)
    COALESCE(current_state.to_state, 'pending')::TEXT as current_state,

    -- Dependency Analysis (calculated from direct joins)
    CASE
      WHEN dep_edges.to_step_id IS NULL THEN true  -- Root steps (no parents)
      WHEN COUNT(dep_edges.from_step_id) = 0 THEN true  -- Steps with zero dependencies
      WHEN COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id) THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Simplified Retry & Backoff Analysis
    CASE
      WHEN ws.attempts >= COALESCE(ws.retry_limit, 3) THEN false
      WHEN ws.attempts > 0 AND COALESCE(ws.retryable, true) = false THEN false
      WHEN last_failure.created_at IS NULL THEN true
      WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') <= NOW()
      WHEN last_failure.created_at IS NOT NULL THEN
        last_failure.created_at + (
          LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')
        ) <= NOW()
      ELSE true
    END as retry_eligible,

    -- Simplified Final Readiness Calculation
    CASE
      WHEN COALESCE(current_state.to_state, 'pending') IN ('pending', 'error')
      AND (ws.processed = false OR ws.processed IS NULL)  -- CRITICAL: Only unprocessed steps can be ready
      AND (dep_edges.to_step_id IS NULL OR
           COUNT(dep_edges.from_step_id) = 0 OR
           COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END) = COUNT(dep_edges.from_step_id))
      AND (ws.attempts < COALESCE(ws.retry_limit, 3))
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
        (last_failure.created_at IS NULL OR
         last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    -- Timing Information
    last_failure.created_at as last_failure_at,
    CASE
      WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN last_failure.created_at IS NOT NULL THEN
        last_failure.created_at + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    -- Dependency Context (calculated from joins)
    COALESCE(COUNT(dep_edges.from_step_id), 0)::INTEGER as total_parents,
    COALESCE(COUNT(CASE WHEN parent_states.to_state IN ('complete', 'resolved_manually') THEN 1 END), 0)::INTEGER as completed_parents,

    -- Retry Context
    ws.attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id

  -- OPTIMIZED: Current State using most_recent flag instead of DISTINCT ON
  LEFT JOIN tasker_workflow_step_transitions current_state
    ON current_state.workflow_step_id = ws.workflow_step_id
    AND current_state.most_recent = true

  -- OPTIMIZED: Dependency check using direct joins (no subquery)
  LEFT JOIN tasker_workflow_step_edges dep_edges
    ON dep_edges.to_step_id = ws.workflow_step_id
  LEFT JOIN tasker_workflow_step_transitions parent_states
    ON parent_states.workflow_step_id = dep_edges.from_step_id
    AND parent_states.most_recent = true

  -- OPTIMIZED: Last failure using index-optimized approach
  LEFT JOIN tasker_workflow_step_transitions last_failure
    ON last_failure.workflow_step_id = ws.workflow_step_id
    AND last_failure.to_state = 'error'
    AND last_failure.most_recent = true

  -- KEY PERFORMANCE IMPROVEMENT: Filter by multiple tasks at once
  -- CRITICAL FIX: Include ALL steps for task execution context calculation
  -- Only filter by processed status when specifically querying for ready steps
  WHERE ws.task_id = ANY(input_task_ids)

  GROUP BY
    ws.workflow_step_id, ws.task_id, ws.named_step_id, ns.name,
    current_state.to_state, last_failure.created_at,
    ws.attempts, ws.retry_limit, ws.backoff_request_seconds, ws.last_attempted_at,
    ws.in_process, ws.processed, ws.retryable, dep_edges.to_step_id

  -- IMPORTANT: Order by task_id, then workflow_step_id for consistent grouping
  ORDER BY ws.task_id, ws.workflow_step_id;
END;
$$;


--
-- Name: get_system_health_counts_v01(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_system_health_counts_v01() RETURNS TABLE(total_tasks bigint, pending_tasks bigint, in_progress_tasks bigint, complete_tasks bigint, error_tasks bigint, cancelled_tasks bigint, total_steps bigint, pending_steps bigint, in_progress_steps bigint, complete_steps bigint, error_steps bigint, retryable_error_steps bigint, exhausted_retry_steps bigint, in_backoff_steps bigint, active_connections bigint, max_connections bigint)
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
        LEFT JOIN tasker_task_transitions task_state ON task_state.task_id = t.task_id
            AND task_state.most_recent = true
    ),
    step_counts AS (
        SELECT
            COUNT(*) as total_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'pending') as pending_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'in_progress') as in_progress_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'complete') as complete_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'error') as error_steps,

            -- Retry-specific logic - retryable errors
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts < ws.retry_limit
                AND COALESCE(ws.retryable, true) = true
            ) as retryable_error_steps,

            -- Exhausted retries
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts >= ws.retry_limit
            ) as exhausted_retry_steps,

            -- In backoff (error state but not exhausted retries and has last_attempted_at)
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts < ws.retry_limit
                AND COALESCE(ws.retryable, true) = true
                AND ws.last_attempted_at IS NOT NULL
            ) as in_backoff_steps

        FROM tasker_workflow_steps ws
        LEFT JOIN tasker_workflow_step_transitions step_state ON step_state.workflow_step_id = ws.workflow_step_id
            AND step_state.most_recent = true
    ),
    connection_info AS (
        SELECT
            COUNT(*) as active_connections,
            COALESCE((SELECT setting::BIGINT FROM pg_settings WHERE name = 'max_connections'), 0) as max_connections
        FROM pg_stat_activity
        WHERE datname = current_database()
            AND state = 'active'
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


--
-- Name: get_task_execution_context(bigint); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_task_execution_context(input_task_id bigint) RETURNS TABLE(task_id bigint, named_task_id integer, status text, total_steps bigint, pending_steps bigint, in_progress_steps bigint, completed_steps bigint, failed_steps bigint, ready_steps bigint, execution_status text, recommended_action text, completion_percentage numeric, health_status text)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  -- Use the step readiness function to get step data, then aggregate
  -- The step readiness function handles the case where no steps exist
  RETURN QUERY
  WITH step_data AS (
    SELECT * FROM get_step_readiness_status(input_task_id, NULL)
  ),
  task_info AS (
    SELECT
      t.task_id,
      t.named_task_id,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM tasker_tasks t
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_id = t.task_id
      AND task_state.most_recent = true
    WHERE t.task_id = input_task_id
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
    ti.task_id,
    ti.named_task_id,
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


--
-- Name: get_task_execution_contexts_batch(bigint[]); Type: FUNCTION; Schema: public; Owner: -
--

CREATE OR REPLACE FUNCTION public.get_task_execution_contexts_batch(input_task_ids bigint[]) RETURNS TABLE(task_id bigint, named_task_id integer, status text, total_steps bigint, pending_steps bigint, in_progress_steps bigint, completed_steps bigint, failed_steps bigint, ready_steps bigint, execution_status text, recommended_action text, completion_percentage numeric, health_status text)
    LANGUAGE plpgsql STABLE
    AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    -- Get step readiness data for all tasks at once using batch function
    SELECT * FROM get_step_readiness_status_batch(input_task_ids)
  ),
  task_info AS (
    SELECT
      t.task_id,
      t.named_task_id,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM unnest(input_task_ids) AS task_list(task_id)
    JOIN tasker_tasks t ON t.task_id = task_list.task_id
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_id = t.task_id
      AND task_state.most_recent = true
  ),
  aggregated_stats AS (
    SELECT
      sd.task_id,
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      -- Count PERMANENTLY blocked failures (exhausted retries)
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.retry_limit) THEN 1 END) as permanently_blocked_steps
    FROM step_data sd
    GROUP BY sd.task_id
  )
  SELECT
    ti.task_id,
    ti.named_task_id,
    ti.current_status as status,

    -- Step Statistics
    COALESCE(ast.total_steps, 0) as total_steps,
    COALESCE(ast.pending_steps, 0) as pending_steps,
    COALESCE(ast.in_progress_steps, 0) as in_progress_steps,
    COALESCE(ast.completed_steps, 0) as completed_steps,
    COALESCE(ast.failed_steps, 0) as failed_steps,
    COALESCE(ast.ready_steps, 0) as ready_steps,

    -- Execution State Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'processing'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- Recommended Action Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'handle_failures'
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
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status

  FROM task_info ti
  LEFT JOIN aggregated_stats ast ON ast.task_id = ti.task_id
  ORDER BY ti.task_id;
END;
$$;

--
-- Name: tasker_annotation_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_annotation_types (
    annotation_type_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_annotation_types_annotation_type_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_annotation_types_annotation_type_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_annotation_types_annotation_type_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_annotation_types_annotation_type_id_seq OWNED BY public.tasker_annotation_types.annotation_type_id;


--
-- Name: tasker_dependent_system_object_maps; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_dependent_system_object_maps (
    dependent_system_object_map_id bigint NOT NULL,
    dependent_system_one_id integer NOT NULL,
    dependent_system_two_id integer NOT NULL,
    remote_id_one character varying(128) NOT NULL,
    remote_id_two character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_dependent_system_objec_dependent_system_object_map_i_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_dependent_system_objec_dependent_system_object_map_i_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_dependent_system_objec_dependent_system_object_map_i_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_dependent_system_objec_dependent_system_object_map_i_seq OWNED BY public.tasker_dependent_system_object_maps.dependent_system_object_map_id;


--
-- Name: tasker_dependent_systems; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_dependent_systems (
    dependent_system_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_dependent_systems_dependent_system_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_dependent_systems_dependent_system_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_dependent_systems_dependent_system_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_dependent_systems_dependent_system_id_seq OWNED BY public.tasker_dependent_systems.dependent_system_id;


--
-- Name: tasker_named_steps; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_named_steps (
    named_step_id integer NOT NULL,
    dependent_system_id integer NOT NULL,
    name character varying(128) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_named_steps_named_step_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_named_steps_named_step_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_named_steps_named_step_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_named_steps_named_step_id_seq OWNED BY public.tasker_named_steps.named_step_id;


--
-- Name: tasker_named_tasks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_named_tasks (
    named_task_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    task_namespace_id bigint DEFAULT 1 NOT NULL,
    version character varying(16) DEFAULT '0.1.0'::character varying NOT NULL,
    configuration jsonb DEFAULT '"{}"'::jsonb
);


--
-- Name: tasker_named_tasks_named_steps; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_named_tasks_named_steps (
    id integer NOT NULL,
    named_task_id integer NOT NULL,
    named_step_id integer NOT NULL,
    skippable boolean DEFAULT false NOT NULL,
    default_retryable boolean DEFAULT true NOT NULL,
    default_retry_limit integer DEFAULT 3 NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_named_tasks_named_steps_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_named_tasks_named_steps_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_named_tasks_named_steps_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_named_tasks_named_steps_id_seq OWNED BY public.tasker_named_tasks_named_steps.id;


--
-- Name: tasker_named_tasks_named_task_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_named_tasks_named_task_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_named_tasks_named_task_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_named_tasks_named_task_id_seq OWNED BY public.tasker_named_tasks.named_task_id;


--
-- Name: tasker_workflow_step_edges; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_workflow_step_edges (
    id bigint NOT NULL,
    from_step_id bigint NOT NULL,
    to_step_id bigint NOT NULL,
    name character varying NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_workflow_steps; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_workflow_steps (
    workflow_step_id bigint NOT NULL,
    task_id bigint NOT NULL,
    named_step_id integer NOT NULL,
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
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    skippable boolean DEFAULT false NOT NULL
);


--
-- Name: tasker_step_dag_relationships; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.tasker_step_dag_relationships AS
 SELECT ws.workflow_step_id,
    ws.task_id,
    ws.named_step_id,
    COALESCE(parent_data.parent_ids, '[]'::jsonb) AS parent_step_ids,
    COALESCE(child_data.child_ids, '[]'::jsonb) AS child_step_ids,
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
     LEFT JOIN ( SELECT tasker_workflow_step_edges.to_step_id,
            jsonb_agg(tasker_workflow_step_edges.from_step_id ORDER BY tasker_workflow_step_edges.from_step_id) AS parent_ids,
            count(*) AS parent_count
           FROM public.tasker_workflow_step_edges
          GROUP BY tasker_workflow_step_edges.to_step_id) parent_data ON ((parent_data.to_step_id = ws.workflow_step_id)))
     LEFT JOIN ( SELECT tasker_workflow_step_edges.from_step_id,
            jsonb_agg(tasker_workflow_step_edges.to_step_id ORDER BY tasker_workflow_step_edges.to_step_id) AS child_ids,
            count(*) AS child_count
           FROM public.tasker_workflow_step_edges
          GROUP BY tasker_workflow_step_edges.from_step_id) child_data ON ((child_data.from_step_id = ws.workflow_step_id)))
     LEFT JOIN ( WITH RECURSIVE step_depths AS (
                 SELECT ws_inner.workflow_step_id,
                    0 AS depth_from_root,
                    ws_inner.task_id
                   FROM public.tasker_workflow_steps ws_inner
                  WHERE (NOT (EXISTS ( SELECT 1
                           FROM public.tasker_workflow_step_edges e
                          WHERE (e.to_step_id = ws_inner.workflow_step_id))))
                UNION ALL
                 SELECT e.to_step_id,
                    (sd.depth_from_root + 1),
                    sd.task_id
                   FROM (step_depths sd
                     JOIN public.tasker_workflow_step_edges e ON ((e.from_step_id = sd.workflow_step_id)))
                  WHERE (sd.depth_from_root < 50)
                )
         SELECT step_depths.workflow_step_id,
            min(step_depths.depth_from_root) AS min_depth_from_root
           FROM step_depths
          GROUP BY step_depths.workflow_step_id) depth_info ON ((depth_info.workflow_step_id = ws.workflow_step_id)));


--
-- Name: tasker_task_annotations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_task_annotations (
    task_annotation_id bigint NOT NULL,
    task_id bigint NOT NULL,
    annotation_type_id integer NOT NULL,
    annotation jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_task_annotations_task_annotation_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_task_annotations_task_annotation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_task_annotations_task_annotation_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_task_annotations_task_annotation_id_seq OWNED BY public.tasker_task_annotations.task_annotation_id;


--
-- Name: tasker_task_namespaces; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_task_namespaces (
    task_namespace_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_task_namespaces_task_namespace_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_task_namespaces_task_namespace_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_task_namespaces_task_namespace_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_task_namespaces_task_namespace_id_seq OWNED BY public.tasker_task_namespaces.task_namespace_id;


--
-- Name: tasker_task_transitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_task_transitions (
    id bigint NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT true NOT NULL,
    task_id bigint NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_task_transitions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_task_transitions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_task_transitions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_task_transitions_id_seq OWNED BY public.tasker_task_transitions.id;


--
-- Name: tasker_tasks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_tasks (
    task_id bigint NOT NULL,
    named_task_id integer NOT NULL,
    complete boolean DEFAULT false NOT NULL,
    requested_at timestamp without time zone NOT NULL,
    initiator character varying(128),
    source_system character varying(128),
    reason character varying(128),
    bypass_steps json,
    tags jsonb,
    context jsonb,
    identity_hash character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_tasks_task_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_tasks_task_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_tasks_task_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_tasks_task_id_seq OWNED BY public.tasker_tasks.task_id;


--
-- Name: tasker_workflow_step_edges_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_workflow_step_edges_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_workflow_step_edges_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_workflow_step_edges_id_seq OWNED BY public.tasker_workflow_step_edges.id;


--
-- Name: tasker_workflow_step_transitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE IF NOT EXISTS public.tasker_workflow_step_transitions (
    id bigint NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT true NOT NULL,
    workflow_step_id bigint NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: tasker_workflow_step_transitions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_workflow_step_transitions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_workflow_step_transitions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_workflow_step_transitions_id_seq OWNED BY public.tasker_workflow_step_transitions.id;


--
-- Name: tasker_workflow_steps_workflow_step_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE IF NOT EXISTS public.tasker_workflow_steps_workflow_step_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasker_workflow_steps_workflow_step_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasker_workflow_steps_workflow_step_id_seq OWNED BY public.tasker_workflow_steps.workflow_step_id;


--
-- Name: tasker_annotation_types annotation_type_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_annotation_types ALTER COLUMN annotation_type_id SET DEFAULT nextval('public.tasker_annotation_types_annotation_type_id_seq'::regclass);


--
-- Name: tasker_dependent_system_object_maps dependent_system_object_map_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps ALTER COLUMN dependent_system_object_map_id SET DEFAULT nextval('public.tasker_dependent_system_objec_dependent_system_object_map_i_seq'::regclass);


--
-- Name: tasker_dependent_systems dependent_system_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_dependent_systems ALTER COLUMN dependent_system_id SET DEFAULT nextval('public.tasker_dependent_systems_dependent_system_id_seq'::regclass);


--
-- Name: tasker_named_steps named_step_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_steps ALTER COLUMN named_step_id SET DEFAULT nextval('public.tasker_named_steps_named_step_id_seq'::regclass);


--
-- Name: tasker_named_tasks named_task_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_tasks ALTER COLUMN named_task_id SET DEFAULT nextval('public.tasker_named_tasks_named_task_id_seq'::regclass);


--
-- Name: tasker_named_tasks_named_steps id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps ALTER COLUMN id SET DEFAULT nextval('public.tasker_named_tasks_named_steps_id_seq'::regclass);


--
-- Name: tasker_task_annotations task_annotation_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_annotations ALTER COLUMN task_annotation_id SET DEFAULT nextval('public.tasker_task_annotations_task_annotation_id_seq'::regclass);


--
-- Name: tasker_task_namespaces task_namespace_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_namespaces ALTER COLUMN task_namespace_id SET DEFAULT nextval('public.tasker_task_namespaces_task_namespace_id_seq'::regclass);


--
-- Name: tasker_task_transitions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_transitions ALTER COLUMN id SET DEFAULT nextval('public.tasker_task_transitions_id_seq'::regclass);


--
-- Name: tasker_tasks task_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_tasks ALTER COLUMN task_id SET DEFAULT nextval('public.tasker_tasks_task_id_seq'::regclass);


--
-- Name: tasker_workflow_step_edges id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_step_edges ALTER COLUMN id SET DEFAULT nextval('public.tasker_workflow_step_edges_id_seq'::regclass);


--
-- Name: tasker_workflow_step_transitions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_step_transitions ALTER COLUMN id SET DEFAULT nextval('public.tasker_workflow_step_transitions_id_seq'::regclass);


--
-- Name: tasker_workflow_steps workflow_step_id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_steps ALTER COLUMN workflow_step_id SET DEFAULT nextval('public.tasker_workflow_steps_workflow_step_id_seq'::regclass);

--
-- Name: tasker_annotation_types tasker_annotation_types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_annotation_types
    ADD CONSTRAINT tasker_annotation_types_pkey PRIMARY KEY (annotation_type_id);


--
-- Name: tasker_dependent_system_object_maps tasker_dependent_system_object_maps_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT tasker_dependent_system_object_maps_pkey PRIMARY KEY (dependent_system_object_map_id);


--
-- Name: tasker_dependent_systems tasker_dependent_systems_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_dependent_systems
    ADD CONSTRAINT tasker_dependent_systems_pkey PRIMARY KEY (dependent_system_id);


--
-- Name: tasker_named_steps tasker_named_steps_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT tasker_named_steps_pkey PRIMARY KEY (named_step_id);


--
-- Name: tasker_named_tasks_named_steps tasker_named_tasks_named_steps_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_pkey PRIMARY KEY (id);


--
-- Name: tasker_named_tasks tasker_named_tasks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_pkey PRIMARY KEY (named_task_id);


--
-- Name: tasker_task_annotations tasker_task_annotations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT tasker_task_annotations_pkey PRIMARY KEY (task_annotation_id);


--
-- Name: tasker_task_namespaces tasker_task_namespaces_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_namespaces
    ADD CONSTRAINT tasker_task_namespaces_pkey PRIMARY KEY (task_namespace_id);


--
-- Name: tasker_task_transitions tasker_task_transitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_transitions
    ADD CONSTRAINT tasker_task_transitions_pkey PRIMARY KEY (id);


--
-- Name: tasker_tasks tasker_tasks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_tasks
    ADD CONSTRAINT tasker_tasks_pkey PRIMARY KEY (task_id);


--
-- Name: tasker_workflow_step_edges tasker_workflow_step_edges_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT tasker_workflow_step_edges_pkey PRIMARY KEY (id);


--
-- Name: tasker_workflow_step_transitions tasker_workflow_step_transitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_step_transitions
    ADD CONSTRAINT tasker_workflow_step_transitions_pkey PRIMARY KEY (id);


--
-- Name: tasker_workflow_steps tasker_workflow_steps_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT tasker_workflow_steps_pkey PRIMARY KEY (workflow_step_id);


--
-- Name: annotation_types_name_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX annotation_types_name_index ON public.tasker_annotation_types USING btree (name);


--
-- Name: annotation_types_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX annotation_types_name_unique ON public.tasker_annotation_types USING btree (name);


--
-- Name: dependent_system_object_maps_dependent_system_one_id_dependent_; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX dependent_system_object_maps_dependent_system_one_id_dependent_ ON public.tasker_dependent_system_object_maps USING btree (dependent_system_one_id, dependent_system_two_id, remote_id_one, remote_id_two);


--
-- Name: dependent_system_object_maps_dependent_system_one_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX dependent_system_object_maps_dependent_system_one_id_index ON public.tasker_dependent_system_object_maps USING btree (dependent_system_one_id);


--
-- Name: dependent_system_object_maps_dependent_system_two_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX dependent_system_object_maps_dependent_system_two_id_index ON public.tasker_dependent_system_object_maps USING btree (dependent_system_two_id);


--
-- Name: dependent_system_object_maps_remote_id_one_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX dependent_system_object_maps_remote_id_one_index ON public.tasker_dependent_system_object_maps USING btree (remote_id_one);


--
-- Name: dependent_system_object_maps_remote_id_two_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX dependent_system_object_maps_remote_id_two_index ON public.tasker_dependent_system_object_maps USING btree (remote_id_two);


--
-- Name: dependent_systems_name_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX dependent_systems_name_index ON public.tasker_dependent_systems USING btree (name);


--
-- Name: dependent_systems_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX dependent_systems_name_unique ON public.tasker_dependent_systems USING btree (name);


--
-- Name: idx_on_workflow_step_id_most_recent_97d5374ad6; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_workflow_step_id_most_recent_97d5374ad6 ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, most_recent) WHERE (most_recent = true);


--
-- Name: idx_on_workflow_step_id_sort_key_4d476d7adb; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_workflow_step_id_sort_key_4d476d7adb ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, sort_key);


--
-- Name: idx_step_edges_from_to; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_step_edges_from_to ON public.tasker_workflow_step_edges USING btree (from_step_id, to_step_id);


--
-- Name: idx_step_edges_to_from; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_step_edges_to_from ON public.tasker_workflow_step_edges USING btree (to_step_id, from_step_id);


--
-- Name: idx_step_transitions_current_state; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_step_transitions_current_state ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, most_recent, to_state) WHERE (most_recent = true);


--
-- Name: idx_step_transitions_most_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_step_transitions_most_recent ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, most_recent) WHERE (most_recent = true);


--
-- Name: idx_steps_active_operations; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_steps_active_operations ON public.tasker_workflow_steps USING btree (workflow_step_id, task_id) WHERE ((processed = false) OR (processed IS NULL));


--
-- Name: idx_task_transitions_most_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_task_transitions_most_recent ON public.tasker_task_transitions USING btree (task_id, most_recent) WHERE (most_recent = true);


--
-- Name: idx_tasks_active_operations; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_tasks_active_operations ON public.tasker_tasks USING btree (task_id) WHERE ((complete = false) OR (complete IS NULL));


--
-- Name: idx_tasks_active_workflow_summary; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_tasks_active_workflow_summary ON public.tasker_tasks USING btree (task_id, complete) WHERE ((complete = false) OR (complete IS NULL));


--
-- Name: idx_tasks_completion_status_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_tasks_completion_status_created ON public.tasker_tasks USING btree (complete, created_at, task_id);


--
-- Name: idx_workflow_step_edges_dependency_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_workflow_step_edges_dependency_lookup ON public.tasker_workflow_step_edges USING btree (to_step_id, from_step_id);


--
-- Name: idx_workflow_steps_task_grouping_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_workflow_steps_task_grouping_active ON public.tasker_workflow_steps USING btree (task_id, workflow_step_id) WHERE ((processed = false) OR (processed IS NULL));


--
-- Name: idx_workflow_steps_task_readiness; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_workflow_steps_task_readiness ON public.tasker_workflow_steps USING btree (task_id, processed, workflow_step_id) WHERE (processed = false);


--
-- Name: index_step_edges_child_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_edges_child_lookup ON public.tasker_workflow_step_edges USING btree (to_step_id);


--
-- Name: index_step_edges_dependency_pair; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_edges_dependency_pair ON public.tasker_workflow_step_edges USING btree (to_step_id, from_step_id);


--
-- Name: index_step_edges_from_step_for_children; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_edges_from_step_for_children ON public.tasker_workflow_step_edges USING btree (from_step_id);


--
-- Name: index_step_edges_from_to_composite; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_edges_from_to_composite ON public.tasker_workflow_step_edges USING btree (from_step_id, to_step_id);


--
-- Name: index_step_edges_parent_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_edges_parent_lookup ON public.tasker_workflow_step_edges USING btree (from_step_id);


--
-- Name: index_step_edges_to_from_composite; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_edges_to_from_composite ON public.tasker_workflow_step_edges USING btree (to_step_id, from_step_id);


--
-- Name: index_step_edges_to_step_for_parents; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_edges_to_step_for_parents ON public.tasker_workflow_step_edges USING btree (to_step_id);


--
-- Name: index_step_transitions_completed; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_transitions_completed ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, to_state) WHERE ((to_state)::text = 'complete'::text);


--
-- Name: index_step_transitions_completed_parents; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_transitions_completed_parents ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, most_recent) WHERE (((to_state)::text = ANY ((ARRAY['complete'::character varying, 'resolved_manually'::character varying])::text[])) AND (most_recent = true));


--
-- Name: index_step_transitions_current_errors; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_transitions_current_errors ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, most_recent, created_at) WHERE (((to_state)::text = 'error'::text) AND (most_recent = true));


--
-- Name: index_step_transitions_current_state_optimized; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_transitions_current_state_optimized ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, most_recent) WHERE (most_recent = true);


--
-- Name: index_step_transitions_failures_with_timing; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_transitions_failures_with_timing ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, to_state, created_at) WHERE ((to_state)::text = 'failed'::text);


--
-- Name: index_step_transitions_for_current_state; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_step_transitions_for_current_state ON public.tasker_workflow_step_transitions USING btree (workflow_step_id, created_at);


--
-- Name: index_task_transitions_current_state; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_task_transitions_current_state ON public.tasker_task_transitions USING btree (task_id, created_at);


--
-- Name: index_task_transitions_current_state_optimized; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_task_transitions_current_state_optimized ON public.tasker_task_transitions USING btree (task_id, most_recent) WHERE (most_recent = true);


--
-- Name: index_tasker_task_transitions_on_sort_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_tasker_task_transitions_on_sort_key ON public.tasker_task_transitions USING btree (sort_key);


--
-- Name: index_tasker_task_transitions_on_task_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_tasker_task_transitions_on_task_id ON public.tasker_task_transitions USING btree (task_id);


--
-- Name: index_tasker_task_transitions_on_task_id_and_most_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_tasker_task_transitions_on_task_id_and_most_recent ON public.tasker_task_transitions USING btree (task_id, most_recent) WHERE (most_recent = true);


--
-- Name: index_tasker_task_transitions_on_task_id_and_sort_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_tasker_task_transitions_on_task_id_and_sort_key ON public.tasker_task_transitions USING btree (task_id, sort_key);


--
-- Name: index_tasker_workflow_step_edges_on_from_step_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_tasker_workflow_step_edges_on_from_step_id ON public.tasker_workflow_step_edges USING btree (from_step_id);


--
-- Name: index_tasker_workflow_step_edges_on_to_step_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_tasker_workflow_step_edges_on_to_step_id ON public.tasker_workflow_step_edges USING btree (to_step_id);


--
-- Name: index_tasker_workflow_step_transitions_on_sort_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_tasker_workflow_step_transitions_on_sort_key ON public.tasker_workflow_step_transitions USING btree (sort_key);


--
-- Name: index_tasker_workflow_step_transitions_on_workflow_step_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_tasker_workflow_step_transitions_on_workflow_step_id ON public.tasker_workflow_step_transitions USING btree (workflow_step_id);


--
-- Name: index_tasks_on_identity_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_tasks_on_identity_hash ON public.tasker_tasks USING btree (identity_hash);


--
-- Name: index_workflow_step_edges_dependency_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_step_edges_dependency_lookup ON public.tasker_workflow_step_edges USING btree (to_step_id, from_step_id);


--
-- Name: index_workflow_steps_backoff_timing; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_backoff_timing ON public.tasker_workflow_steps USING btree (last_attempted_at, backoff_request_seconds) WHERE (backoff_request_seconds IS NOT NULL);


--
-- Name: index_workflow_steps_by_task; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_by_task ON public.tasker_workflow_steps USING btree (task_id);


--
-- Name: index_workflow_steps_processing_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_processing_status ON public.tasker_workflow_steps USING btree (task_id, processed, in_process);


--
-- Name: index_workflow_steps_retry_logic; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_retry_logic ON public.tasker_workflow_steps USING btree (attempts, retry_limit, retryable);


--
-- Name: index_workflow_steps_retry_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_retry_status ON public.tasker_workflow_steps USING btree (attempts, retry_limit);


--
-- Name: index_workflow_steps_task_and_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_task_and_id ON public.tasker_workflow_steps USING btree (task_id, workflow_step_id);


--
-- Name: index_workflow_steps_task_and_step_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_task_and_step_id ON public.tasker_workflow_steps USING btree (task_id, workflow_step_id);


--
-- Name: index_workflow_steps_task_and_step_optimized; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_task_and_step_optimized ON public.tasker_workflow_steps USING btree (task_id, workflow_step_id);


--
-- Name: index_workflow_steps_task_covering; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflow_steps_task_covering ON public.tasker_workflow_steps USING btree (task_id) INCLUDE (workflow_step_id, processed, in_process, attempts, retry_limit);


--
-- Name: named_step_by_system_uniq; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX named_step_by_system_uniq ON public.tasker_named_steps USING btree (dependent_system_id, name);


--
-- Name: named_steps_dependent_system_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_steps_dependent_system_id_index ON public.tasker_named_steps USING btree (dependent_system_id);


--
-- Name: named_steps_name_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_steps_name_index ON public.tasker_named_steps USING btree (name);


--
-- Name: named_tasks_configuration_gin_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_tasks_configuration_gin_index ON public.tasker_named_tasks USING gin (configuration);


--
-- Name: named_tasks_name_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_tasks_name_index ON public.tasker_named_tasks USING btree (name);


--
-- Name: named_tasks_named_steps_named_step_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_tasks_named_steps_named_step_id_index ON public.tasker_named_tasks_named_steps USING btree (named_step_id);


--
-- Name: named_tasks_named_steps_named_task_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_tasks_named_steps_named_task_id_index ON public.tasker_named_tasks_named_steps USING btree (named_task_id);


--
-- Name: named_tasks_namespace_name_version_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX named_tasks_namespace_name_version_unique ON public.tasker_named_tasks USING btree (task_namespace_id, name, version);


--
-- Name: named_tasks_steps_ids_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX named_tasks_steps_ids_unique ON public.tasker_named_tasks_named_steps USING btree (named_task_id, named_step_id);


--
-- Name: named_tasks_task_namespace_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_tasks_task_namespace_id_index ON public.tasker_named_tasks USING btree (task_namespace_id);


--
-- Name: named_tasks_version_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX named_tasks_version_index ON public.tasker_named_tasks USING btree (version);


--
-- Name: task_annotations_annotation_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX task_annotations_annotation_idx ON public.tasker_task_annotations USING gin (annotation);


--
-- Name: task_annotations_annotation_idx1; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX task_annotations_annotation_idx1 ON public.tasker_task_annotations USING gin (annotation jsonb_path_ops);


--
-- Name: task_annotations_annotation_type_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX task_annotations_annotation_type_id_index ON public.tasker_task_annotations USING btree (annotation_type_id);


--
-- Name: task_annotations_task_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX task_annotations_task_id_index ON public.tasker_task_annotations USING btree (task_id);


--
-- Name: task_namespaces_name_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX task_namespaces_name_index ON public.tasker_task_namespaces USING btree (name);


--
-- Name: task_namespaces_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX task_namespaces_name_unique ON public.tasker_task_namespaces USING btree (name);


--
-- Name: tasks_context_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_context_idx ON public.tasker_tasks USING gin (context);


--
-- Name: tasks_context_idx1; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_context_idx1 ON public.tasker_tasks USING gin (context jsonb_path_ops);


--
-- Name: tasks_identity_hash_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_identity_hash_index ON public.tasker_tasks USING btree (identity_hash);


--
-- Name: tasks_named_task_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_named_task_id_index ON public.tasker_tasks USING btree (named_task_id);


--
-- Name: tasks_requested_at_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_requested_at_index ON public.tasker_tasks USING btree (requested_at);


--
-- Name: tasks_source_system_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_source_system_index ON public.tasker_tasks USING btree (source_system);


--
-- Name: tasks_tags_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_tags_idx ON public.tasker_tasks USING gin (tags);


--
-- Name: tasks_tags_idx1; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX tasks_tags_idx1 ON public.tasker_tasks USING gin (tags jsonb_path_ops);


--
-- Name: workflow_steps_last_attempted_at_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX workflow_steps_last_attempted_at_index ON public.tasker_workflow_steps USING btree (last_attempted_at);


--
-- Name: workflow_steps_named_step_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX workflow_steps_named_step_id_index ON public.tasker_workflow_steps USING btree (named_step_id);


--
-- Name: workflow_steps_processed_at_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX workflow_steps_processed_at_index ON public.tasker_workflow_steps USING btree (processed_at);


--
-- Name: workflow_steps_task_id_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX workflow_steps_task_id_index ON public.tasker_workflow_steps USING btree (task_id);


--
-- Name: tasker_dependent_system_object_maps dependent_system_object_maps_dependent_system_one_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT dependent_system_object_maps_dependent_system_one_id_foreign FOREIGN KEY (dependent_system_one_id) REFERENCES public.tasker_dependent_systems(dependent_system_id);


--
-- Name: tasker_dependent_system_object_maps dependent_system_object_maps_dependent_system_two_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT dependent_system_object_maps_dependent_system_two_id_foreign FOREIGN KEY (dependent_system_two_id) REFERENCES public.tasker_dependent_systems(dependent_system_id);


--
-- Name: tasker_named_tasks fk_rails_16a0297759; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT fk_rails_16a0297759 FOREIGN KEY (task_namespace_id) REFERENCES public.tasker_task_namespaces(task_namespace_id);


--
-- Name: tasker_workflow_step_transitions fk_rails_6e0f6eb833; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_step_transitions
    ADD CONSTRAINT fk_rails_6e0f6eb833 FOREIGN KEY (workflow_step_id) REFERENCES public.tasker_workflow_steps(workflow_step_id);


--
-- Name: tasker_workflow_step_edges fk_rails_7b4652ccf2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT fk_rails_7b4652ccf2 FOREIGN KEY (from_step_id) REFERENCES public.tasker_workflow_steps(workflow_step_id);


--
-- Name: tasker_workflow_step_edges fk_rails_93ec0bf6eb; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT fk_rails_93ec0bf6eb FOREIGN KEY (to_step_id) REFERENCES public.tasker_workflow_steps(workflow_step_id);


--
-- Name: tasker_task_transitions fk_rails_e8caec803c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_transitions
    ADD CONSTRAINT fk_rails_e8caec803c FOREIGN KEY (task_id) REFERENCES public.tasker_tasks(task_id);


--
-- Name: tasker_named_steps named_steps_dependent_system_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT named_steps_dependent_system_id_foreign FOREIGN KEY (dependent_system_id) REFERENCES public.tasker_dependent_systems(dependent_system_id);


--
-- Name: tasker_named_tasks_named_steps named_tasks_named_steps_named_step_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_named_step_id_foreign FOREIGN KEY (named_step_id) REFERENCES public.tasker_named_steps(named_step_id);


--
-- Name: tasker_named_tasks_named_steps named_tasks_named_steps_named_task_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_named_task_id_foreign FOREIGN KEY (named_task_id) REFERENCES public.tasker_named_tasks(named_task_id);


--
-- Name: tasker_task_annotations task_annotations_annotation_type_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT task_annotations_annotation_type_id_foreign FOREIGN KEY (annotation_type_id) REFERENCES public.tasker_annotation_types(annotation_type_id);


--
-- Name: tasker_task_annotations task_annotations_task_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT task_annotations_task_id_foreign FOREIGN KEY (task_id) REFERENCES public.tasker_tasks(task_id);


--
-- Name: tasker_tasks tasks_named_task_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_tasks
    ADD CONSTRAINT tasks_named_task_id_foreign FOREIGN KEY (named_task_id) REFERENCES public.tasker_named_tasks(named_task_id);


--
-- Name: tasker_workflow_steps workflow_steps_named_step_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT workflow_steps_named_step_id_foreign FOREIGN KEY (named_step_id) REFERENCES public.tasker_named_steps(named_step_id);


--
-- Name: tasker_workflow_steps workflow_steps_task_id_foreign; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT workflow_steps_task_id_foreign FOREIGN KEY (task_id) REFERENCES public.tasker_tasks(task_id);

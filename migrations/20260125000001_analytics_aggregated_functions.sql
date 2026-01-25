-- TAS-168: Aggregated analytics functions
--
-- These functions perform aggregation at the database level, returning
-- pre-computed statistics grouped by the correct identity keys:
-- - Steps: (namespace_name, task_name, version, step_name)
-- - Tasks: (namespace_name, task_name, version)
--
-- This eliminates the need for Rust-side HashMap aggregation and ensures
-- step_name disambiguation within task template context.

-- ----------------------------------------------------------------------------
-- get_slowest_steps_aggregated: Aggregated step performance by template+step
-- ----------------------------------------------------------------------------
-- Groups by (namespace, task_name, version, step_name) since step names are
-- only unique within a task template, not globally.
CREATE FUNCTION tasker.get_slowest_steps_aggregated(
    since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone,
    limit_count integer DEFAULT 10,
    min_executions integer DEFAULT 5,
    namespace_filter text DEFAULT NULL::text,
    task_name_filter text DEFAULT NULL::text,
    version_filter text DEFAULT NULL::text
) RETURNS TABLE(
    namespace_name character varying,
    task_name character varying,
    version character varying,
    step_name character varying,
    average_duration_seconds numeric,
    max_duration_seconds numeric,
    execution_count bigint,
    error_count bigint,
    error_rate numeric,
    last_executed_at timestamp without time zone
)
LANGUAGE plpgsql STABLE
AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH step_executions AS (
        SELECT
            tn.name as ns_name,
            nt.name as t_name,
            nt.version as t_version,
            ns.name as s_name,
            EXTRACT(EPOCH FROM (wst.created_at - ws.created_at)) as duration_seconds,
            wst.to_state as final_state,
            wst.created_at as completed_at
        FROM workflow_steps ws
        INNER JOIN named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        INNER JOIN tasks t ON t.task_uuid = ws.task_uuid
        INNER JOIN named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        INNER JOIN task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        INNER JOIN workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE ws.created_at > analysis_start
          AND wst.most_recent = true
          AND wst.to_state IN ('complete', 'error')
          AND (namespace_filter IS NULL OR tn.name = namespace_filter)
          AND (task_name_filter IS NULL OR nt.name = task_name_filter)
          AND (version_filter IS NULL OR nt.version = version_filter)
    )
    SELECT
        se.ns_name::varchar AS namespace_name,
        se.t_name::varchar AS task_name,
        se.t_version::varchar AS version,
        se.s_name::varchar AS step_name,
        ROUND(AVG(se.duration_seconds), 3) AS average_duration_seconds,
        ROUND(MAX(se.duration_seconds), 3) AS max_duration_seconds,
        COUNT(*) AS execution_count,
        COUNT(*) FILTER (WHERE se.final_state = 'error') AS error_count,
        ROUND(
            COUNT(*) FILTER (WHERE se.final_state = 'error')::numeric /
            NULLIF(COUNT(*), 0)::numeric,
            4
        ) AS error_rate,
        MAX(se.completed_at) AS last_executed_at
    FROM step_executions se
    WHERE se.duration_seconds IS NOT NULL
      AND se.duration_seconds > 0
    GROUP BY se.ns_name, se.t_name, se.t_version, se.s_name
    HAVING COUNT(*) >= min_executions
    ORDER BY AVG(se.duration_seconds) DESC
    LIMIT limit_count;
END;
$$;

COMMENT ON FUNCTION tasker.get_slowest_steps_aggregated IS
'TAS-168: Returns step performance aggregated by (namespace, task_name, version, step_name).
Step names are only unique within a task template context.';

-- ----------------------------------------------------------------------------
-- get_slowest_tasks_aggregated: Aggregated task performance by template
-- ----------------------------------------------------------------------------
-- Groups by (namespace, task_name, version) - the task template identity.
CREATE FUNCTION tasker.get_slowest_tasks_aggregated(
    since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone,
    limit_count integer DEFAULT 10,
    min_executions integer DEFAULT 5,
    namespace_filter text DEFAULT NULL::text,
    task_name_filter text DEFAULT NULL::text,
    version_filter text DEFAULT NULL::text
) RETURNS TABLE(
    namespace_name character varying,
    task_name character varying,
    version character varying,
    average_duration_seconds numeric,
    max_duration_seconds numeric,
    execution_count bigint,
    average_step_count numeric,
    total_error_steps bigint,
    error_rate numeric,
    last_executed_at timestamp without time zone
)
LANGUAGE plpgsql STABLE
AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH task_executions AS (
        SELECT
            tn.name as ns_name,
            nt.name as t_name,
            nt.version as t_version,
            t.task_uuid,
            t.created_at,
            MAX(wst.created_at) FILTER (
                WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
            ) as latest_completion,
            COUNT(DISTINCT ws.workflow_step_uuid) as step_count,
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
        GROUP BY t.task_uuid, tn.name, nt.name, nt.version, t.created_at
        HAVING MAX(wst.created_at) FILTER (
            WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
        ) IS NOT NULL
    ),
    task_with_durations AS (
        SELECT
            te.ns_name,
            te.t_name,
            te.t_version,
            EXTRACT(EPOCH FROM (te.latest_completion - te.created_at)) as duration_seconds,
            te.step_count,
            te.error_steps,
            te.latest_completion
        FROM task_executions te
        WHERE EXTRACT(EPOCH FROM (te.latest_completion - te.created_at)) > 0
    )
    SELECT
        td.ns_name::varchar AS namespace_name,
        td.t_name::varchar AS task_name,
        td.t_version::varchar AS version,
        ROUND(AVG(td.duration_seconds), 3) AS average_duration_seconds,
        ROUND(MAX(td.duration_seconds), 3) AS max_duration_seconds,
        COUNT(*) AS execution_count,
        ROUND(AVG(td.step_count), 2) AS average_step_count,
        SUM(td.error_steps)::bigint AS total_error_steps,
        ROUND(
            SUM(td.error_steps)::numeric /
            NULLIF(COUNT(*), 0)::numeric,
            4
        ) AS error_rate,
        MAX(td.latest_completion) AS last_executed_at
    FROM task_with_durations td
    GROUP BY td.ns_name, td.t_name, td.t_version
    HAVING COUNT(*) >= min_executions
    ORDER BY AVG(td.duration_seconds) DESC
    LIMIT limit_count;
END;
$$;

COMMENT ON FUNCTION tasker.get_slowest_tasks_aggregated IS
'TAS-168: Returns task performance aggregated by (namespace, task_name, version) - the template identity.';

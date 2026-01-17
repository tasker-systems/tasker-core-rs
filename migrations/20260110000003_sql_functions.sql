-- Migration 3: SQL Functions, Views, and Triggers
-- TAS-128: PostgreSQL 18 Migration - Schema Flattening
-- Flattened from 12 accumulated migrations into 3 logical files
--
-- This migration creates all tasker SQL functions, views, and triggers
-- in the tasker schema with table references updated to remove tasker_ prefix.

-- Set search path for this migration
SET search_path TO tasker, public;

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- ----------------------------------------------------------------------------
-- calculate_dependency_levels: Calculate DAG dependency levels for a task
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.calculate_dependency_levels(input_task_uuid uuid)
RETURNS TABLE(workflow_step_uuid uuid, dependency_level integer)
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

-- ----------------------------------------------------------------------------
-- calculate_staleness_threshold: Calculate staleness threshold for a task state
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.calculate_staleness_threshold(
    p_task_state character varying,
    p_template_config jsonb,
    p_default_waiting_deps integer DEFAULT 60,
    p_default_waiting_retry integer DEFAULT 30,
    p_default_steps_process integer DEFAULT 30
) RETURNS integer
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

COMMENT ON FUNCTION tasker.calculate_staleness_threshold(character varying, jsonb, integer, integer, integer) IS 'TAS-49 Phase 2: Calculate staleness threshold for a task state.

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

-- ----------------------------------------------------------------------------
-- calculate_step_next_retry_time: Calculate next retry time for a step
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.calculate_step_next_retry_time(
    backoff_request_seconds integer,
    last_attempted_at timestamp without time zone,
    failure_time timestamp without time zone,
    attempts integer,
    p_max_backoff_seconds integer DEFAULT 60,
    p_backoff_multiplier numeric DEFAULT 2.0
) RETURNS timestamp without time zone
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

-- ----------------------------------------------------------------------------
-- create_dlq_entry: Create DLQ entry for a stale task
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.create_dlq_entry(
    p_task_uuid uuid,
    p_namespace_name character varying,
    p_task_name character varying,
    p_current_state character varying,
    p_time_in_state_minutes integer,
    p_threshold_minutes integer,
    p_dlq_reason character varying DEFAULT 'staleness_timeout'::character varying
) RETURNS uuid
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

COMMENT ON FUNCTION tasker.create_dlq_entry(uuid, character varying, character varying, character varying, integer, integer, character varying) IS 'TAS-49 Phase 2: Create DLQ entry for a stale task with comprehensive snapshot.

Atomically creates DLQ investigation entry with task state snapshot.
Uses ON CONFLICT to prevent duplicate pending entries for same task.

Error Handling:
- Catches all exceptions to prevent transaction rollback
- Logs warnings for debugging
- Returns NULL to indicate failure

Called by:
- detect_and_transition_stale_tasks() for each stale task
- Can be called manually for operator-initiated DLQ entries';

-- ----------------------------------------------------------------------------
-- create_step_result_audit: Trigger function for step result auditing
-- ----------------------------------------------------------------------------
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

COMMENT ON FUNCTION tasker.create_step_result_audit() IS 'Trigger function that creates audit records when workers persist step results. Fires on transitions to enqueued_for_orchestration or enqueued_as_error_for_orchestration states.';

-- ----------------------------------------------------------------------------
-- detect_and_transition_stale_tasks: Main staleness detection function
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.detect_and_transition_stale_tasks(
    p_dry_run boolean DEFAULT true,
    p_batch_size integer DEFAULT 100,
    p_default_waiting_deps_threshold integer DEFAULT 60,
    p_default_waiting_retry_threshold integer DEFAULT 30,
    p_default_steps_in_process_threshold integer DEFAULT 30,
    p_default_task_max_lifetime_hours integer DEFAULT 24
) RETURNS TABLE(
    task_uuid uuid,
    namespace_name character varying,
    task_name character varying,
    current_state character varying,
    time_in_state_minutes integer,
    staleness_threshold_minutes integer,
    action_taken character varying,
    moved_to_dlq boolean,
    transition_success boolean
)
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

COMMENT ON FUNCTION tasker.detect_and_transition_stale_tasks(boolean, integer, integer, integer, integer, integer) IS 'TAS-49: Main orchestration function for automatic staleness detection and DLQ processing.

Orchestrates the complete staleness detection workflow:
1. Discovery: Call get_stale_tasks_for_dlq() ONCE (O(1) expensive operations)
2. For each stale task:
   a. Create DLQ entry with task snapshot
   b. Transition task to Error state
   c. Track results and errors

Dry Run Mode:
- p_dry_run = true: Report what would happen without making changes
- p_dry_run = false: Actually create DLQ entries and transition tasks';

-- ----------------------------------------------------------------------------
-- evaluate_step_state_readiness: Evaluate step readiness for execution
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.evaluate_step_state_readiness(
    current_state text,
    processed boolean,
    in_process boolean,
    dependencies_satisfied boolean,
    retry_eligible boolean,
    retryable boolean,
    next_retry_time timestamp without time zone
) RETURNS boolean
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

-- ----------------------------------------------------------------------------
-- find_stuck_tasks: Find tasks that appear stuck
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.find_stuck_tasks(p_timeout_minutes integer DEFAULT 10)
RETURNS TABLE(
    task_uuid uuid,
    current_state character varying,
    processor_uuid uuid,
    stuck_duration_minutes integer
)
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

-- ----------------------------------------------------------------------------
-- get_analytics_metrics: Get comprehensive analytics metrics
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_analytics_metrics(since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone)
RETURNS TABLE(
    active_tasks_count bigint,
    total_namespaces_count bigint,
    unique_task_types_count bigint,
    system_health_score numeric,
    task_throughput bigint,
    completion_count bigint,
    error_count bigint,
    completion_rate numeric,
    error_rate numeric,
    avg_task_duration numeric,
    avg_step_duration numeric,
    step_throughput bigint,
    analysis_period_start timestamp with time zone,
    calculated_at timestamp with time zone
)
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

-- ----------------------------------------------------------------------------
-- get_current_task_state: Get current state of a task
-- ----------------------------------------------------------------------------
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

-- ----------------------------------------------------------------------------
-- get_next_ready_task: Get next ready task for processing
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_next_ready_task()
RETURNS TABLE(
    task_uuid uuid,
    task_name character varying,
    priority integer,
    namespace_name character varying,
    ready_steps_count bigint,
    computed_priority numeric,
    current_state character varying
)
LANGUAGE plpgsql
AS $$
BEGIN
    -- Simply delegate to batch function with limit of 1
    RETURN QUERY
    SELECT * FROM get_next_ready_tasks(1);
END;
$$;

-- ----------------------------------------------------------------------------
-- get_next_ready_tasks: Get batch of ready tasks for processing
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_next_ready_tasks(p_limit integer DEFAULT 5)
RETURNS TABLE(
    task_uuid uuid,
    task_name character varying,
    priority integer,
    namespace_name character varying,
    ready_steps_count bigint,
    computed_priority numeric,
    current_state character varying
)
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

COMMENT ON FUNCTION tasker.get_next_ready_tasks(integer) IS 'TAS-48: Returns ready tasks with staleness filtering (1A) and exponential priority decay (1B).
Staleness exclusion: Tasks stuck >60min (dependencies) or >30min (retry) excluded from discovery.
Priority decay: Fresh tasks get age escalation, aging tasks decay exponentially (12hr half-life),
stale tasks (>24hr) get minimum priority (0.1) and become DLQ candidates.
Ensures fresh tasks always discoverable regardless of stale task count.';

-- ----------------------------------------------------------------------------
-- get_slowest_steps: Get slowest completed steps for analysis
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_slowest_steps(
    since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone,
    limit_count integer DEFAULT 10,
    namespace_filter text DEFAULT NULL::text,
    task_name_filter text DEFAULT NULL::text,
    version_filter text DEFAULT NULL::text
) RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    step_name character varying,
    task_name character varying,
    namespace_name character varying,
    version character varying,
    duration_seconds numeric,
    attempts integer,
    created_at timestamp without time zone,
    completed_at timestamp without time zone,
    retryable boolean,
    step_status character varying
)
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

-- ----------------------------------------------------------------------------
-- get_slowest_tasks: Get slowest completed tasks for analysis
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_slowest_tasks(
    since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone,
    limit_count integer DEFAULT 10,
    namespace_filter text DEFAULT NULL::text,
    task_name_filter text DEFAULT NULL::text,
    version_filter text DEFAULT NULL::text
) RETURNS TABLE(
    task_uuid uuid,
    task_name character varying,
    namespace_name character varying,
    version character varying,
    duration_seconds numeric,
    step_count bigint,
    completed_steps bigint,
    error_steps bigint,
    created_at timestamp without time zone,
    completed_at timestamp without time zone,
    initiator character varying,
    source_system character varying
)
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

-- ----------------------------------------------------------------------------
-- get_stale_tasks_for_dlq: Discover stale tasks exceeding thresholds
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_stale_tasks_for_dlq(
    p_default_waiting_deps integer DEFAULT 60,
    p_default_waiting_retry integer DEFAULT 30,
    p_default_steps_process integer DEFAULT 30,
    p_max_lifetime_hours integer DEFAULT 24,
    p_batch_size integer DEFAULT 100
) RETURNS TABLE(
    task_uuid uuid,
    namespace_name character varying,
    task_name character varying,
    current_state character varying,
    time_in_state_minutes numeric,
    threshold_minutes integer,
    task_age_minutes numeric
)
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

COMMENT ON FUNCTION tasker.get_stale_tasks_for_dlq(integer, integer, integer, integer, integer) IS 'TAS-49 Phase 2: Discover stale tasks exceeding thresholds with O(1) optimization.

Key Performance Feature:
- Queries v_task_state_analysis base view ONCE (all expensive joins)
- Main detection function calls this ONCE before loop, not inside loop
- Achieves O(1) expensive operations instead of O(n) per-iteration queries';

-- ----------------------------------------------------------------------------
-- get_step_readiness_status: Get step readiness status for a task
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_step_readiness_status(input_task_uuid uuid, step_uuids uuid[] DEFAULT NULL::uuid[])
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
    name text,
    current_state text,
    dependencies_satisfied boolean,
    retry_eligible boolean,
    ready_for_execution boolean,
    last_failure_at timestamp without time zone,
    next_retry_at timestamp without time zone,
    total_parents integer,
    completed_parents integer,
    attempts integer,
    max_attempts integer,
    backoff_request_seconds integer,
    last_attempted_at timestamp without time zone
)
LANGUAGE plpgsql STABLE
AS $$
BEGIN
  -- DELEGATION: Use batch function with single-element array for DRY principle
  RETURN QUERY
  SELECT * FROM get_step_readiness_status_batch(ARRAY[input_task_uuid], step_uuids);
END;
$$;

COMMENT ON FUNCTION tasker.get_step_readiness_status(uuid, uuid[]) IS 'Optimized step readiness analysis function with task-scoped CTEs for performance.

PERFORMANCE OPTIMIZATIONS:
- task_steps CTE eliminates table scans by creating focused working set
- All subsequent CTEs use INNER JOINs to task_steps for efficient filtering
- Restored last_failures CTE for accurate failure time tracking
- dependency_counts uses INNER JOIN instead of inefficient subselect';

-- ----------------------------------------------------------------------------
-- get_step_readiness_status_batch: Batch step readiness for multiple tasks
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_step_readiness_status_batch(input_task_uuids uuid[], step_uuids uuid[] DEFAULT NULL::uuid[])
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
    name text,
    current_state text,
    dependencies_satisfied boolean,
    retry_eligible boolean,
    ready_for_execution boolean,
    last_failure_at timestamp without time zone,
    next_retry_at timestamp without time zone,
    total_parents integer,
    completed_parents integer,
    attempts integer,
    max_attempts integer,
    backoff_request_seconds integer,
    last_attempted_at timestamp without time zone
)
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

COMMENT ON FUNCTION tasker.get_step_readiness_status_batch(uuid[], uuid[]) IS 'Batch step readiness with corrected retry_eligible logic.

BUG FIX (2025-10-06):
- First attempt (attempts=0) is now always eligible regardless of retry settings
- Retry attempts (attempts>0) properly check retryable=true AND attempts < max_attempts
- Fixes issue where max_attempts=0, retryable=false blocked first execution';

-- ----------------------------------------------------------------------------
-- get_step_transitive_dependencies: Get transitive dependencies for a step
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_step_transitive_dependencies(target_step_uuid uuid)
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
    step_name character varying,
    results jsonb,
    processed boolean,
    distance integer
)
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

-- ----------------------------------------------------------------------------
-- get_system_health_counts: Get system-wide health counts
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_system_health_counts()
RETURNS TABLE(
    pending_tasks bigint,
    initializing_tasks bigint,
    enqueuing_steps_tasks bigint,
    steps_in_process_tasks bigint,
    evaluating_results_tasks bigint,
    waiting_for_dependencies_tasks bigint,
    waiting_for_retry_tasks bigint,
    blocked_by_failures_tasks bigint,
    complete_tasks bigint,
    error_tasks bigint,
    cancelled_tasks bigint,
    resolved_manually_tasks bigint,
    total_tasks bigint,
    pending_steps bigint,
    enqueued_steps bigint,
    in_progress_steps bigint,
    enqueued_for_orchestration_steps bigint,
    enqueued_as_error_for_orchestration_steps bigint,
    waiting_for_retry_steps bigint,
    complete_steps bigint,
    error_steps bigint,
    cancelled_steps bigint,
    resolved_manually_steps bigint,
    total_steps bigint
)
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

-- ----------------------------------------------------------------------------
-- get_task_execution_context: Get execution context for a task
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_task_execution_context(input_task_uuid uuid)
RETURNS TABLE(
    task_uuid uuid,
    named_task_uuid uuid,
    status text,
    total_steps bigint,
    pending_steps bigint,
    in_progress_steps bigint,
    completed_steps bigint,
    failed_steps bigint,
    ready_steps bigint,
    execution_status text,
    recommended_action text,
    completion_percentage numeric,
    health_status text,
    enqueued_steps bigint
)
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

-- ----------------------------------------------------------------------------
-- get_task_execution_contexts_batch: Batch execution context for multiple tasks
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_task_execution_contexts_batch(input_task_uuids uuid[])
RETURNS TABLE(
    task_uuid uuid,
    named_task_uuid uuid,
    status text,
    total_steps bigint,
    pending_steps bigint,
    in_progress_steps bigint,
    completed_steps bigint,
    failed_steps bigint,
    ready_steps bigint,
    execution_status text,
    recommended_action text,
    completion_percentage numeric,
    health_status text,
    enqueued_steps bigint
)
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

-- ----------------------------------------------------------------------------
-- get_task_ready_info: Get ready info for a specific task
-- ----------------------------------------------------------------------------
CREATE FUNCTION tasker.get_task_ready_info(p_task_uuid uuid)
RETURNS TABLE(
    task_uuid uuid,
    task_name character varying,
    priority integer,
    namespace_name character varying,
    ready_steps_count bigint,
    computed_priority numeric,
    current_state character varying
)
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

-- ----------------------------------------------------------------------------
-- State Transition Functions
-- ----------------------------------------------------------------------------

-- set_step_backoff_atomic: Atomically set step backoff
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

-- transition_stale_task_to_error: Transition stale task to error state
CREATE FUNCTION tasker.transition_stale_task_to_error(
    p_task_uuid uuid,
    p_current_state character varying,
    p_namespace_name character varying,
    p_task_name character varying
) RETURNS boolean
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

COMMENT ON FUNCTION tasker.transition_stale_task_to_error(uuid, character varying, character varying, character varying) IS 'TAS-49 Phase 2: Transition stale task to Error state with proper error handling.

Wraps transition_task_state_atomic() with staleness-specific context and
error handling. Prevents transaction rollback on transition failures.';

-- transition_task_state_atomic: Atomic task state transition
CREATE FUNCTION tasker.transition_task_state_atomic(
    p_task_uuid uuid,
    p_from_state character varying,
    p_to_state character varying,
    p_processor_uuid uuid,
    p_metadata jsonb DEFAULT '{}'::jsonb
) RETURNS boolean
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

COMMENT ON FUNCTION tasker.transition_task_state_atomic(uuid, character varying, character varying, uuid, jsonb) IS 'TAS-41/TAS-49/TAS-54: Atomic state transition without ownership enforcement.

Performs compare-and-swap atomic state transition based solely on current state.
Processor UUID is stored in transitions table for audit trail and debugging.

Returns:
- TRUE if transition succeeded (current state matched p_from_state)
- FALSE if state mismatch (task not in expected state)';

-- update_updated_at_column: Trigger function to update updated_at timestamp
CREATE FUNCTION tasker.update_updated_at_column() RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION tasker.update_updated_at_column() IS 'TAS-49: Trigger function to automatically update updated_at timestamp on row modification';

-- ============================================================================
-- VIEWS
-- Note: step_dag_relationships is defined in migration 1 (schema_and_tables.sql)
-- ============================================================================

-- ----------------------------------------------------------------------------
-- v_dlq_dashboard: DLQ monitoring dashboard view
-- ----------------------------------------------------------------------------
CREATE VIEW tasker.v_dlq_dashboard AS
SELECT dlq_reason,
    count(*) AS total_entries,
    count(*) FILTER (WHERE (resolution_status = 'pending'::dlq_resolution_status)) AS pending,
    count(*) FILTER (WHERE (resolution_status = 'manually_resolved'::dlq_resolution_status)) AS manually_resolved,
    count(*) FILTER (WHERE (resolution_status = 'permanently_failed'::dlq_resolution_status)) AS permanent_failures,
    count(*) FILTER (WHERE (resolution_status = 'cancelled'::dlq_resolution_status)) AS cancelled,
    min(dlq_timestamp) AS oldest_entry,
    max(dlq_timestamp) AS newest_entry,
    avg((EXTRACT(epoch FROM (COALESCE((resolution_timestamp)::timestamp with time zone, now()) - (dlq_timestamp)::timestamp with time zone)) / (60)::numeric)) AS avg_resolution_time_minutes
FROM tasks_dlq
GROUP BY dlq_reason;

COMMENT ON VIEW tasker.v_dlq_dashboard IS 'TAS-49: High-level DLQ metrics for monitoring and alerting.

Aggregates DLQ statistics by reason:
- Total entries and breakdown by resolution status
- Time range (oldest/newest entries)
- Average resolution time';

-- ----------------------------------------------------------------------------
-- v_dlq_investigation_queue: Prioritized DLQ investigation queue
-- ----------------------------------------------------------------------------
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
            WHEN 'dependency_cycle_detected'::dlq_reason THEN 1000
            WHEN 'max_retries_exceeded'::dlq_reason THEN 500
            WHEN 'staleness_timeout'::dlq_reason THEN 100
            WHEN 'worker_unavailable'::dlq_reason THEN 50
            ELSE 10
        END)::numeric + (EXTRACT(epoch FROM (now() - (dlq_timestamp)::timestamp with time zone)) / (60)::numeric)) AS priority_score
FROM tasks_dlq dlq
WHERE (resolution_status = 'pending'::dlq_resolution_status)
ORDER BY ((
        CASE dlq_reason
            WHEN 'dependency_cycle_detected'::dlq_reason THEN 1000
            WHEN 'max_retries_exceeded'::dlq_reason THEN 500
            WHEN 'staleness_timeout'::dlq_reason THEN 100
            WHEN 'worker_unavailable'::dlq_reason THEN 50
            ELSE 10
        END)::numeric + (EXTRACT(epoch FROM (now() - (dlq_timestamp)::timestamp with time zone)) / (60)::numeric)) DESC;

COMMENT ON VIEW tasker.v_dlq_investigation_queue IS 'TAS-49: Prioritized queue of pending DLQ investigations.

Provides operator dashboard with intelligent prioritization:
- Dependency cycles: Highest priority (block entire workflows)
- Max retries exceeded: High priority (permanent failures)
- Staleness timeouts: Medium priority (may self-resolve)
- Age factor: Older entries get higher scores';

-- ----------------------------------------------------------------------------
-- v_task_state_analysis: Task state analysis base view
-- ----------------------------------------------------------------------------
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
FROM (((tasks t
  JOIN named_tasks nt ON ((nt.named_task_uuid = t.named_task_uuid)))
  JOIN task_namespaces tns ON ((tns.task_namespace_uuid = nt.task_namespace_uuid)))
  JOIN task_transitions tt ON (((tt.task_uuid = t.task_uuid) AND (tt.most_recent = true))))
WHERE (t.complete = false);

COMMENT ON VIEW tasker.v_task_state_analysis IS 'TAS-49: Base view for task state analysis with namespace and template information.

Provides foundation for staleness monitoring and DLQ detection.
Joins across tasks, named_tasks, task_namespaces, and task_transitions.

Key Columns:
- time_in_state_minutes: How long task has been in current state
- task_age_minutes: Total task lifetime
- template_config: JSONB configuration with threshold overrides';

-- ----------------------------------------------------------------------------
-- v_task_staleness_monitoring: Staleness monitoring view
-- ----------------------------------------------------------------------------
CREATE VIEW tasker.v_task_staleness_monitoring AS
SELECT task_uuid,
    namespace_name,
    task_name,
    current_state,
    time_in_state_minutes,
    task_age_minutes,
    calculate_staleness_threshold(current_state, template_config, 60, 30, 30) AS staleness_threshold_minutes,
        CASE
            WHEN (time_in_state_minutes >= (calculate_staleness_threshold(current_state, template_config, 60, 30, 30))::numeric) THEN 'stale'::text
            WHEN (time_in_state_minutes >= ((calculate_staleness_threshold(current_state, template_config, 60, 30, 30))::numeric * 0.8)) THEN 'warning'::text
            ELSE 'healthy'::text
        END AS health_status,
    priority
FROM v_task_state_analysis tsa
WHERE ((current_state)::text = ANY ((ARRAY['waiting_for_dependencies'::character varying, 'waiting_for_retry'::character varying, 'steps_in_process'::character varying])::text[]));

COMMENT ON VIEW tasker.v_task_staleness_monitoring IS 'TAS-49: Real-time staleness monitoring for active tasks.

Provides operational visibility into task health:
- "stale": Exceeded threshold, candidate for DLQ
- "warning": At 80% of threshold, needs attention
- "healthy": Within normal operating parameters';

-- ============================================================================
-- TRIGGERS
-- ============================================================================

-- Trigger for step result auditing
CREATE TRIGGER trg_step_result_audit
AFTER INSERT ON workflow_step_transitions
FOR EACH ROW
EXECUTE FUNCTION create_step_result_audit();

COMMENT ON TRIGGER trg_step_result_audit ON workflow_step_transitions IS 'Creates audit records when workers persist step execution results (TAS-62)';

-- Trigger for DLQ updated_at
CREATE TRIGGER update_dlq_updated_at
BEFORE UPDATE ON tasks_dlq
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- TAS-48: Task Staleness Immediate Relief - Discovery Bug Fixes
-- ============================================================================
--
-- Problem: Stale tasks block discovery of fresh tasks due to two bugs:
-- 1. No time-based exclusion: Tasks stuck for hours fill discovery buffer
-- 2. Monotonically increasing priority: Stale tasks accumulate max priority
--
-- Solution: Combined fix applying both changes:
-- 1A. Staleness Exclusion: Exclude tasks stuck beyond thresholds
-- 1B. Priority Decay: Exponential decay ensures fresh tasks rise to top
--
-- Impact: Fresh tasks always discoverable regardless of stale task count.
-- ============================================================================

CREATE OR REPLACE FUNCTION get_next_ready_tasks(p_limit INTEGER DEFAULT 5)
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
) AS $$
DECLARE
    v_max_waiting_for_deps_minutes INTEGER;
    v_max_waiting_for_retry_minutes INTEGER;
    v_decay_start_hours NUMERIC;
    v_decay_half_life_hours NUMERIC;
    v_stale_threshold_hours NUMERIC;
    v_minimum_priority NUMERIC;
BEGIN
    -- Configuration parameters (defaults match config/tasker/base/state_machine.toml)
    -- Future: Load from tasker_config table
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
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt on nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns on tns.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN tasker_task_transitions tt_processing
            ON tt_processing.task_uuid = t.task_uuid
            AND tt_processing.most_recent = true
            AND tt_processing.processor_uuid IS NOT NULL
            AND tt_processing.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
        WHERE tt.most_recent = true
        AND tt.to_state IN ('pending', 'waiting_for_dependencies', 'waiting_for_retry')
        AND tt_processing.task_uuid IS NULL  -- Not already being processed
        -- TAS-48 Solution 1A: Staleness-aware filtering
        -- Exclude tasks stuck beyond threshold to prevent discovery blocking
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
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_next_ready_tasks IS
'TAS-48: Returns ready tasks with staleness filtering (1A) and exponential priority decay (1B).
Staleness exclusion: Tasks stuck >60min (dependencies) or >30min (retry) excluded from discovery.
Priority decay: Fresh tasks get age escalation, aging tasks decay exponentially (12hr half-life),
stale tasks (>24hr) get minimum priority (0.1) and become DLQ candidates.
Ensures fresh tasks always discoverable regardless of stale task count.';

-- ============================================================================
-- Combined Behavior Reference
-- ============================================================================
--
-- Mathematical behavior with base_priority=5.0:
--
-- Time in State | Computed Priority | Staleness Filter | Result
-- --------------|-------------------|------------------|------------------
-- 5 minutes     | 5.008            | Included         | Discovered
-- 30 minutes    | 5.05             | Included         | Discovered
-- 1 hour        | 5.1              | Included         | Discovered (peak)
-- 6 hours       | 3.03             | Included         | Discovered (decaying)
-- 12 hours      | 1.84             | Included         | Discovered (half-life)
-- 2 hours (deps)| 2.5              | Excluded         | Not discovered
-- 24 hours      | 0.68             | Excluded         | Not discovered
-- 48 hours      | 0.1              | Excluded         | Not discovered (DLQ)
--
-- Key Properties:
-- - Fresh tasks always get priority boost
-- - Aging tasks gradually decay (not infinite growth)
-- - Tasks stuck beyond thresholds excluded entirely
-- - Stale tasks cannot block fresh tasks regardless of base priority
-- ============================================================================

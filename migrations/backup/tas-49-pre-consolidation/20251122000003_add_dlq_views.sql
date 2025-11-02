-- ============================================================================
-- TAS-49 Phase 1: DLQ Database Views
-- ============================================================================
-- Migration: 20251122000003_add_dlq_views.sql
-- Description: Monitoring and dashboard views for DLQ operations
-- Dependencies: 20251115000000_add_dlq_tables.sql, 20251115000001_add_dlq_functions.sql,
--               20251122000002_add_dlq_helper_functions.sql (for calculate_staleness_threshold)
--
-- Views:
-- 1. v_dlq_dashboard - DLQ statistics by reason
-- 2. v_task_state_analysis - Base view for task state analysis (TAS-49 refactor)
-- 3. v_task_staleness_monitoring - Real-time staleness tracking
-- 4. v_dlq_investigation_queue - Pending investigations prioritized
-- ============================================================================

-- ============================================================================
-- View: DLQ Dashboard Statistics
-- ============================================================================

CREATE OR REPLACE VIEW v_dlq_dashboard AS
SELECT
    dlq_reason,
    COUNT(*) as total_entries,
    COUNT(*) FILTER (WHERE resolution_status = 'pending') as pending,
    COUNT(*) FILTER (WHERE resolution_status = 'manually_resolved') as manually_resolved,
    COUNT(*) FILTER (WHERE resolution_status = 'permanently_failed') as permanent_failures,
    COUNT(*) FILTER (WHERE resolution_status = 'cancelled') as cancelled,
    MIN(dlq_timestamp) as oldest_entry,
    MAX(dlq_timestamp) as newest_entry,
    AVG(
        EXTRACT(EPOCH FROM (
            COALESCE(resolution_timestamp, NOW()) - dlq_timestamp
        )) / 60
    )::INTEGER as avg_resolution_time_minutes
FROM tasker_tasks_dlq
GROUP BY dlq_reason
ORDER BY total_entries DESC;

COMMENT ON VIEW v_dlq_dashboard IS
'TAS-49: DLQ investigation statistics grouped by reason.

Usage:
- Operations dashboard showing DLQ health
- Identify systemic issues (high counts for specific reasons)
- Track investigation velocity (avg_resolution_time_minutes)

Columns:
- dlq_reason: Why tasks entered DLQ
- total_entries: All-time DLQ entries for this reason
- pending: Currently under investigation
- manually_resolved: Successfully fixed by operators
- permanent_failures: Unfixable issues
- cancelled: False positives or duplicates
- avg_resolution_time_minutes: How long investigations typically take';

-- ============================================================================
-- View: Task State Analysis (Base View)
-- ============================================================================
-- TAS-49 Refactor: Extract expensive joins and threshold calculations into
-- reusable base view. This view provides individual task records with
-- calculated thresholds, used by multiple monitoring views and functions.

CREATE OR REPLACE VIEW v_task_state_analysis AS
SELECT
    t.task_uuid,
    nt.named_task_uuid,
    nt.name as task_name,
    tns.name as namespace_name,
    tt.to_state as current_state,
    EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as minutes_in_state,
    EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 as task_age_minutes,
    t.priority,
    t.created_at as task_created_at,
    tt.created_at as state_created_at,
    nt.configuration as template_config,
    -- Calculate threshold using helper function (TAS-49 Phase 2)
    -- Defaults from TAS-48: waiting_for_dependencies=60, waiting_for_retry=30, steps_in_process=30
    calculate_staleness_threshold(
        tt.to_state,
        nt.configuration,
        60,  -- default_waiting_deps
        30,  -- default_waiting_retry
        30   -- default_steps_process
    ) as threshold_minutes
FROM tasker_tasks t
JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid AND tt.most_recent = true
JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
WHERE tt.to_state NOT IN ('complete', 'error', 'cancelled', 'resolved_manually');

COMMENT ON VIEW v_task_state_analysis IS
'TAS-49: Base view for task state analysis with threshold calculations.

Provides individual task records with:
- Current state and time in state
- Calculated staleness thresholds from template configuration
- Template configuration JSONB
- Task metadata (priority, timestamps)

Used by:
- v_task_staleness_monitoring (aggregated metrics)
- get_stale_tasks_for_dlq() (DLQ detection function, Part 2 Phase 3)
- Future monitoring and alerting systems

Performance: Expensive joins materialized once, reusable across queries.
Threshold calculation uses calculate_staleness_threshold() helper function
for reusability and consistency (TAS-49 Phase 2).';

-- ============================================================================
-- View: Task Staleness Monitoring (Refactored)
-- ============================================================================
-- TAS-49 Refactor: Now built on v_task_state_analysis base view instead of
-- inline CTE. Same output, but with reusable foundation.

CREATE OR REPLACE VIEW v_task_staleness_monitoring AS
SELECT
    current_state,
    COUNT(*) as task_count,
    AVG(minutes_in_state)::INTEGER as avg_minutes_in_state,
    MAX(minutes_in_state)::INTEGER as max_minutes_in_state,
    MIN(threshold_minutes)::INTEGER as min_threshold,
    MAX(threshold_minutes)::INTEGER as max_threshold,
    COUNT(*) FILTER (WHERE minutes_in_state > threshold_minutes * 0.75) as approaching_threshold,
    COUNT(*) FILTER (WHERE minutes_in_state > threshold_minutes) as exceeds_threshold,
    array_agg(
        task_uuid ORDER BY minutes_in_state DESC
    ) FILTER (WHERE minutes_in_state > threshold_minutes) as stale_task_uuids
FROM v_task_state_analysis
GROUP BY current_state
ORDER BY exceeds_threshold DESC, approaching_threshold DESC;

COMMENT ON VIEW v_task_staleness_monitoring IS
'TAS-49: Real-time monitoring of task staleness across all waiting states.

Usage:
- Pre-DLQ alerting (approaching_threshold > 0)
- Identify states with systemic staleness issues
- Validate staleness thresholds are appropriate

Columns:
- current_state: Task state being monitored
- task_count: Total tasks in this state
- avg_minutes_in_state: Average time tasks spend in this state
- max_minutes_in_state: Longest task in this state
- min_threshold, max_threshold: Range of thresholds (varies by template)
- approaching_threshold: Tasks at 75% of threshold (warning zone)
- exceeds_threshold: Tasks that should be in DLQ
- stale_task_uuids: Array of tasks exceeding threshold for investigation';

-- ============================================================================
-- View: DLQ Investigation Queue
-- ============================================================================

CREATE OR REPLACE VIEW v_dlq_investigation_queue AS
SELECT
    dlq.dlq_entry_uuid,
    dlq.task_uuid,
    dlq.original_state,
    dlq.dlq_reason,
    dlq.dlq_timestamp,
    EXTRACT(EPOCH FROM (NOW() - dlq.dlq_timestamp)) / 3600 as hours_in_dlq,
    dlq.task_snapshot->>'namespace' as namespace_name,
    dlq.task_snapshot->>'task_name' as task_name,
    (dlq.task_snapshot->>'time_in_state_minutes')::INTEGER as time_in_state_minutes,
    (dlq.task_snapshot->>'threshold_minutes')::INTEGER as threshold_minutes,
    -- Priority: older investigations + critical reasons get higher priority
    CASE dlq.dlq_reason
        WHEN 'dependency_cycle_detected' THEN 1
        WHEN 'max_retries_exceeded' THEN 2
        WHEN 'worker_unavailable' THEN 3
        WHEN 'staleness_timeout' THEN 4
        WHEN 'manual_dlq' THEN 5
        ELSE 6
    END as investigation_priority
FROM tasker_tasks_dlq dlq
WHERE dlq.resolution_status = 'pending'
ORDER BY investigation_priority ASC, dlq.dlq_timestamp ASC;

COMMENT ON VIEW v_dlq_investigation_queue IS
'TAS-49: Pending DLQ investigations ordered by priority.

Usage:
- Operations team daily work queue
- SLA tracking for investigation response time
- Prioritization: dependency cycles > retries > workers > staleness

Columns:
- dlq_entry_uuid: DLQ entry identifier
- task_uuid: Task under investigation
- original_state: Task state when sent to DLQ
- dlq_reason: Why task is in DLQ
- hours_in_dlq: How long investigation has been pending
- investigation_priority: 1 (highest) to 6 (lowest)';

-- ============================================================================
-- Migration Validation
-- ============================================================================

DO $$
BEGIN
    -- Verify views created
    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_dlq_dashboard') THEN
        RAISE EXCEPTION 'v_dlq_dashboard view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_task_state_analysis') THEN
        RAISE EXCEPTION 'v_task_state_analysis view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_task_staleness_monitoring') THEN
        RAISE EXCEPTION 'v_task_staleness_monitoring view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_dlq_investigation_queue') THEN
        RAISE EXCEPTION 'v_dlq_investigation_queue view not created';
    END IF;

    RAISE NOTICE 'TAS-49: DLQ views created successfully (including v_task_state_analysis base view)';
END $$;

-- ============================================================================
-- TAS-49 Phase 1: DLQ Database Views
-- ============================================================================
-- Migration: 20251122000002_add_dlq_views.sql
-- Description: Monitoring and dashboard views for DLQ operations
-- Dependencies: 20251115000000_add_dlq_tables.sql, 20251115000001_add_dlq_functions.sql
--
-- Views:
-- 1. v_dlq_dashboard - DLQ statistics by reason
-- 2. v_task_staleness_monitoring - Real-time staleness tracking
-- 3. v_archive_statistics - Archive growth metrics
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
-- View: Task Staleness Monitoring
-- ============================================================================

CREATE OR REPLACE VIEW v_task_staleness_monitoring AS
WITH current_states AS (
    SELECT
        t.task_uuid,
        nt.name as task_name,
        tns.name as namespace_name,
        tt.to_state as current_state,
        EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as minutes_in_state,
        EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 as task_age_minutes,
        t.priority,
        t.created_at as task_created_at,
        -- Get applicable threshold
        CASE tt.to_state
            WHEN 'waiting_for_dependencies' THEN
                COALESCE(
                    (nt.configuration->'lifecycle'->>'max_waiting_for_dependencies_minutes')::INTEGER,
                    60  -- Default from TAS-48
                )
            WHEN 'waiting_for_retry' THEN
                COALESCE(
                    (nt.configuration->'lifecycle'->>'max_waiting_for_retry_minutes')::INTEGER,
                    30  -- Default from TAS-48
                )
            WHEN 'steps_in_process' THEN
                COALESCE(
                    (nt.configuration->'lifecycle'->>'max_steps_in_process_minutes')::INTEGER,
                    30
                )
            ELSE 1440  -- 24 hours for other states
        END as threshold_minutes
    FROM tasker_tasks t
    JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid AND tt.most_recent = true
    JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
    JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
    WHERE tt.to_state NOT IN ('complete', 'error', 'cancelled', 'resolved_manually')
)
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
FROM current_states
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
-- View: Archive Statistics
-- ============================================================================

CREATE OR REPLACE VIEW v_archive_statistics AS
SELECT
    DATE_TRUNC('month', archived_at) as archive_month,
    COUNT(*) as tasks_archived,
    COUNT(DISTINCT named_task_uuid) as distinct_task_templates,
    MIN(created_at) as oldest_task_archived,
    MAX(created_at) as newest_task_archived,
    AVG(EXTRACT(EPOCH FROM (archived_at - created_at)) / 86400)::INTEGER as avg_task_lifetime_days
FROM tasker_tasks_archive
GROUP BY DATE_TRUNC('month', archived_at)
ORDER BY archive_month DESC;

COMMENT ON VIEW v_archive_statistics IS
'TAS-49: Archive growth metrics and retention analysis.

Usage:
- Monitor archival process health
- Capacity planning for archive storage
- Validate retention policies

Columns:
- archive_month: Month when tasks were archived
- tasks_archived: Number of tasks archived in this month
- distinct_task_templates: Template diversity in archive
- oldest_task_archived: Earliest task creation date archived
- newest_task_archived: Most recent task creation date archived
- avg_task_lifetime_days: Average time from creation to archival';

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

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_task_staleness_monitoring') THEN
        RAISE EXCEPTION 'v_task_staleness_monitoring view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_archive_statistics') THEN
        RAISE EXCEPTION 'v_archive_statistics view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_dlq_investigation_queue') THEN
        RAISE EXCEPTION 'v_dlq_investigation_queue view not created';
    END IF;

    RAISE NOTICE 'TAS-49: DLQ views created successfully';
END $$;

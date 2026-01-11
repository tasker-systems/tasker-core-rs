-- ============================================================================
-- TAS-49: Comprehensive Dead Letter Queue (DLQ) System
-- ============================================================================
-- Migration: 20251115000000_comprehensive_dlq_system.sql
-- Description: Complete DLQ investigation tracking system with staleness detection
-- Dependencies: 20250912000000_tas41_richer_task_states.sql (12-state machine)
--
-- Consolidated from 7 separate migrations into single comprehensive migration:
-- - Phase 1.1: Tables, enums, indexes, constraints, triggers
-- - Phase 1.2: Monitoring and dashboard views
-- - Phase 1.3: Helper functions (threshold calc, DLQ entry, state transition)
-- - Phase 1.4: Discovery function with O(1) optimization
-- - Phase 1.5: Main detection and transition function
-- - Phase 2.0: JSONB indexing for lifecycle configurations
-- - Refactor: transition_task_state_atomic (type fix + TAS-54 ownership removal)
--
-- Architecture:
-- - DLQ = Investigation tracker (NOT task manipulation layer)
-- - Tasks remain in tasker_tasks (typically Error state)
-- - DLQ entries track "why stuck" and "what operator did"
-- - Automatic staleness detection with configurable thresholds
-- - Template-level threshold overrides via JSONB configuration
-- ============================================================================

-- ============================================================================
-- PART 1: UTILITY FUNCTIONS
-- ============================================================================

-- Function: Automatically update updated_at timestamp
--
-- Standard PostgreSQL function for maintaining updated_at columns.
-- Used by triggers to automatically set updated_at to current timestamp.
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION update_updated_at_column() IS
'TAS-49: Trigger function to automatically update updated_at timestamp on row modification';

-- ============================================================================
-- PART 2: TYPE DEFINITIONS (ENUMS)
-- ============================================================================

-- DLQ Resolution Status Enum
-- Tracks the lifecycle of a DLQ investigation (NOT task state)
CREATE TYPE dlq_resolution_status AS ENUM (
    'pending',              -- Investigation in progress
    'manually_resolved',   -- Operator fixed problem steps, task progressed
    'permanently_failed',  -- Unfixable issue (e.g., bad template, data corruption)
    'cancelled'            -- Investigation cancelled (duplicate, false positive, etc.)
);

COMMENT ON TYPE dlq_resolution_status IS
'TAS-49: DLQ investigation status (separate from task state).

State Machine:
  pending → manually_resolved (operator fixed problem via step APIs, task progressed)
  pending → permanently_failed (unfixable issue, task stays in Error state)
  pending → cancelled (investigation no longer needed)

Key Principle: This tracks the INVESTIGATION workflow, not task state.
- Task state is managed by TAS-41 state machine
- Resolution happens at step level via existing step APIs
- DLQ entry tracks "what operator did to investigate"

Example: Task stuck on payment step → DLQ entry created (pending) → operator
resets payment step retry count via step API → task progresses → DLQ entry
updated (manually_resolved).

A task can have multiple DLQ entries over time (investigation history), but
only one pending entry at a time.';

-- DLQ Reason Enum
-- Why was the task sent to DLQ?
CREATE TYPE dlq_reason AS ENUM (
    'staleness_timeout',        -- Exceeded state timeout threshold
    'max_retries_exceeded',     -- TAS-42 retry limit hit
    'dependency_cycle_detected', -- Circular dependency discovered
    'worker_unavailable',       -- No worker available for extended period
    'manual_dlq'               -- Operator manually sent to DLQ
);

COMMENT ON TYPE dlq_reason IS
'TAS-49: Reasons a task is sent to DLQ. Determines investigation priority and remediation approach.';

-- ============================================================================
-- PART 3: TABLE DEFINITIONS
-- ============================================================================

-- Dead Letter Queue Table
-- Purpose: Investigation tracking and audit trail for stuck tasks
-- Note: Tasks remain in tasker_tasks (in Error state), DLQ tracks investigation workflow
CREATE TABLE tasker_tasks_dlq (
    dlq_entry_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    task_uuid UUID NOT NULL REFERENCES tasker_tasks(task_uuid),

    -- DLQ metadata
    original_state VARCHAR(50) NOT NULL,  -- Task state when sent to DLQ
    dlq_reason dlq_reason NOT NULL,
    dlq_timestamp TIMESTAMP NOT NULL DEFAULT NOW(),

    -- Investigation tracking (NOT task retry tracking - that's at step level)
    resolution_status dlq_resolution_status NOT NULL DEFAULT 'pending',
    resolution_timestamp TIMESTAMP,
    resolution_notes TEXT,
    resolved_by VARCHAR(255),

    -- Task snapshot for investigation (full task + steps state at DLQ time)
    task_snapshot JSONB NOT NULL,
    metadata JSONB,  -- For extensibility (e.g., investigation tags, related tickets)

    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tasker_tasks_dlq IS
'TAS-49: Dead Letter Queue investigation tracking and audit trail.

Architecture: DLQ is an INVESTIGATION TRACKER, not a task manipulation layer
- Tasks remain in tasker_tasks table (typically in Error state)
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

COMMENT ON COLUMN tasker_tasks_dlq.task_snapshot IS
'Complete snapshot of task state when moved to DLQ.
Includes task details, steps, transitions, and execution context.
Enables investigation without modifying main task tables.';

COMMENT ON COLUMN tasker_tasks_dlq.metadata IS
'Additional context about DLQ entry.
Examples: error stack trace, detection method, related tasks, investigation notes.';

-- ============================================================================
-- PART 4: INDEXES
-- ============================================================================

-- Fast task lookup for investigation
CREATE INDEX idx_dlq_task_lookup ON tasker_tasks_dlq(task_uuid);

-- Active investigations dashboard
CREATE INDEX idx_dlq_resolution_status ON tasker_tasks_dlq(resolution_status, dlq_timestamp);

-- Pattern analysis and alerting
CREATE INDEX idx_dlq_reason ON tasker_tasks_dlq(dlq_reason);

-- Investigation timestamp range queries
CREATE INDEX idx_dlq_timestamp ON tasker_tasks_dlq(dlq_timestamp DESC);

-- JSONB indexing for lifecycle configuration queries (TAS-49 Phase 2)
-- Enables efficient queries on template threshold configurations
CREATE INDEX idx_named_tasks_lifecycle_config
    ON tasker_named_tasks
    USING GIN ((configuration->'lifecycle'))
    WHERE configuration->'lifecycle' IS NOT NULL;

COMMENT ON INDEX idx_named_tasks_lifecycle_config IS
'TAS-49 Phase 2: GIN index for lifecycle configuration JSONB path queries.
Optimizes threshold lookups in calculate_staleness_threshold() function.
Example query: configuration->''lifecycle''->''max_waiting_for_dependencies_minutes''';

-- ============================================================================
-- PART 5: CONSTRAINTS
-- ============================================================================

-- Constraint: One pending DLQ entry per task
-- Historical entries (manually_resolved, permanently_failed, cancelled) preserved as audit trail
CREATE UNIQUE INDEX idx_dlq_unique_pending_task
    ON tasker_tasks_dlq(task_uuid)
    WHERE resolution_status = 'pending';

COMMENT ON INDEX idx_dlq_unique_pending_task IS
'TAS-49: Ensures only one pending DLQ investigation per task.
Historical DLQ entries (manually_resolved, permanently_failed, cancelled) are preserved as audit trail.
Allows multiple DLQ investigations per task over time, but only one active investigation at a time.';

-- ============================================================================
-- PART 6: TRIGGERS
-- ============================================================================

-- Trigger: Update updated_at on row modification
CREATE TRIGGER update_dlq_updated_at
    BEFORE UPDATE ON tasker_tasks_dlq
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- PART 7: REFACTORED transition_task_state_atomic
-- ============================================================================
-- TAS-49 Phase 5: Fix type mismatch and remove deprecated ownership logic
--
-- Changes:
-- 1. Fix BOOLEAN vs INTEGER type mismatch (v_transitioned → v_row_count)
-- 2. Remove TAS-54 deprecated ownership checking (processor_uuid audit-only)
--
-- Original issues:
-- - v_transitioned declared as BOOLEAN but assigned ROW_COUNT (INTEGER)
-- - Ownership checking logic deprecated in TAS-54 but not removed from SQL
--
-- Result: Simplified atomic state transition based on current state only

CREATE OR REPLACE FUNCTION transition_task_state_atomic(
    p_task_uuid UUID,
    p_from_state VARCHAR,
    p_to_state VARCHAR,
    p_processor_uuid UUID,
    p_metadata JSONB DEFAULT '{}'
) RETURNS BOOLEAN AS $$
DECLARE
    v_sort_key INTEGER;
    v_row_count INTEGER;  -- FIX: Changed from BOOLEAN to INTEGER
BEGIN
    -- Get next sort key
    SELECT COALESCE(MAX(sort_key), 0) + 1 INTO v_sort_key
    FROM tasker_task_transitions
    WHERE task_uuid = p_task_uuid;

    -- Atomically transition only if in expected state
    -- TAS-54: Ownership checking removed - processor_uuid tracked for audit only
    WITH current_state AS (
        SELECT to_state
        FROM tasker_task_transitions
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        FOR UPDATE
    ),
    do_update AS (
        UPDATE tasker_task_transitions
        SET most_recent = false
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        AND EXISTS (
            SELECT 1 FROM current_state
            WHERE to_state = p_from_state
        )
        RETURNING task_uuid
    )
    INSERT INTO tasker_task_transitions (
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
    RETURN v_row_count > 0;  -- FIX: Now comparing INTEGER > INTEGER
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION transition_task_state_atomic IS
'TAS-41/TAS-49/TAS-54: Atomic state transition without ownership enforcement.

Changes:
- TAS-49 Phase 5: Fixed type bug (v_transitioned BOOLEAN → v_row_count INTEGER)
- TAS-54: Removed ownership checking (processor_uuid tracked for audit only)

Performs compare-and-swap atomic state transition based solely on current state.
Processor UUID is stored in transitions table for audit trail and debugging.

Returns:
- TRUE if transition succeeded (current state matched p_from_state)
- FALSE if state mismatch (task not in expected state)

Used by:
- transition_stale_task_to_error() for DLQ staleness detection
- Legacy code (consider migrating to TaskStateMachine in Rust)

Example:
SELECT transition_task_state_atomic(
    ''550e8400-e29b-41d4-a716-446655440000''::UUID,
    ''waiting_for_dependencies'',
    ''error'',
    NULL,  -- processor_uuid optional (audit only)
    ''{"reason": "staleness_timeout"}''::JSONB
);';

-- ============================================================================
-- PART 8: HELPER FUNCTIONS
-- ============================================================================

-- Function: Calculate Staleness Threshold
-- Extracts threshold calculation from inline CASE statement to reusable function.
-- This function determines the appropriate staleness threshold for a given task
-- state by checking template configuration first, then falling back to defaults.

CREATE OR REPLACE FUNCTION calculate_staleness_threshold(
    p_task_state VARCHAR,
    p_template_config JSONB,
    p_default_waiting_deps INTEGER DEFAULT 60,
    p_default_waiting_retry INTEGER DEFAULT 30,
    p_default_steps_process INTEGER DEFAULT 30
) RETURNS INTEGER AS $$
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
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION calculate_staleness_threshold IS
'TAS-49 Phase 2: Calculate staleness threshold for a task state.

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

Returns: Threshold in minutes (INTEGER)

Example:
SELECT calculate_staleness_threshold(
    ''waiting_for_dependencies'',
    ''{"lifecycle": {"max_waiting_for_dependencies_minutes": 45}}''::JSONB,
    60, 30, 30
) as threshold;  -- Returns 45 (from template, not default 60)';

-- Function: Create DLQ Entry
-- Encapsulates DLQ entry creation logic with error handling

CREATE OR REPLACE FUNCTION create_dlq_entry(
    p_task_uuid UUID,
    p_namespace_name VARCHAR,
    p_task_name VARCHAR,
    p_current_state VARCHAR,
    p_time_in_state_minutes INTEGER,
    p_threshold_minutes INTEGER,
    p_dlq_reason VARCHAR DEFAULT 'staleness_timeout'
) RETURNS UUID AS $$
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
    FROM tasker_tasks t
    WHERE t.task_uuid = p_task_uuid;

    -- Create DLQ entry
    INSERT INTO tasker_tasks_dlq (
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
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION create_dlq_entry IS
'TAS-49 Phase 2: Create DLQ entry for a stale task with comprehensive snapshot.

Atomically creates DLQ investigation entry with task state snapshot.
Uses ON CONFLICT to prevent duplicate pending entries for same task.

Error Handling:
- Catches all exceptions to prevent transaction rollback
- Logs warnings for debugging
- Returns NULL to indicate failure

Called by:
- detect_and_transition_stale_tasks() for each stale task
- Can be called manually for operator-initiated DLQ entries

Parameters:
- p_task_uuid: Task being moved to DLQ
- p_namespace_name: Namespace for context
- p_task_name: Template name for context
- p_current_state: State when detected as stale
- p_time_in_state_min: How long task has been in state
- p_threshold_min: Threshold that was exceeded
- p_dlq_reason: Why entering DLQ (default: staleness_timeout)

Returns:
- UUID of created DLQ entry on success
- NULL on failure (with warning logged)

Example:
SELECT create_dlq_entry(
    ''550e8400-e29b-41d4-a716-446655440000''::UUID,
    ''order_processing'',
    ''fulfill_order'',
    ''waiting_for_dependencies'',
    120,
    60,
    ''staleness_timeout''
) as dlq_entry_uuid;';

-- Function: Transition Stale Task to Error
-- Handles state transition for stale tasks with proper error handling

CREATE OR REPLACE FUNCTION transition_stale_task_to_error(
    p_task_uuid UUID,
    p_current_state VARCHAR,
    p_namespace_name VARCHAR,
    p_task_name VARCHAR
) RETURNS BOOLEAN AS $$
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
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION transition_stale_task_to_error IS
'TAS-49 Phase 2: Transition stale task to Error state with proper error handling.

Wraps transition_task_state_atomic() with staleness-specific context and
error handling. Prevents transaction rollback on transition failures.

Called by:
- detect_and_transition_stale_tasks() after DLQ entry creation

Parameters:
- p_task_uuid: Task to transition
- p_current_state: Current state for validation
- p_namespace_name: Namespace for audit trail
- p_task_name: Template name for audit trail

Returns:
- TRUE if transition succeeded
- FALSE if transition failed (with warning logged)

Example:
SELECT transition_stale_task_to_error(
    ''550e8400-e29b-41d4-a716-446655440000''::UUID,
    ''waiting_for_dependencies'',
    ''order_processing'',
    ''fulfill_order''
) as success;';

-- ============================================================================
-- PART 9: DATABASE VIEWS
-- ============================================================================

-- View: Task State Analysis (Base view for staleness detection)
-- Consolidated view of task state with threshold calculations

CREATE OR REPLACE VIEW v_task_state_analysis AS
SELECT
    t.task_uuid,
    t.correlation_id,
    tns.name as namespace_name,
    tns.task_namespace_uuid as namespace_uuid,
    nt.name as task_name,
    nt.named_task_uuid,
    nt.configuration as template_config,
    tt.to_state as current_state,
    tt.created_at as state_entered_at,
    EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as time_in_state_minutes,
    EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 as task_age_minutes,
    t.priority,
    t.complete
FROM tasker_tasks t
JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid AND tt.most_recent = true
WHERE t.complete = false;

COMMENT ON VIEW v_task_state_analysis IS
'TAS-49: Base view for task state analysis with namespace and template information.

Provides foundation for staleness monitoring and DLQ detection.
Joins across tasker_tasks, tasker_named_tasks, tasker_task_namespaces, and tasker_task_transitions.

Key Columns:
- time_in_state_minutes: How long task has been in current state
- task_age_minutes: Total task lifetime
- template_config: JSONB configuration with threshold overrides

Used By:
- v_task_staleness_monitoring view
- get_stale_tasks_for_dlq() discovery function
- Any monitoring dashboards needing task state context

Performance:
- Filtered to incomplete tasks only (complete = false)
- Most recent state via most_recent = true partial index
- Template config available for threshold calculation';

-- View: Task Staleness Monitoring
-- Real-time view of tasks approaching or exceeding staleness thresholds

CREATE OR REPLACE VIEW v_task_staleness_monitoring AS
SELECT
    tsa.task_uuid,
    tsa.namespace_name,
    tsa.task_name,
    tsa.current_state,
    tsa.time_in_state_minutes,
    tsa.task_age_minutes,
    calculate_staleness_threshold(
        tsa.current_state,
        tsa.template_config,
        60, 30, 30  -- default thresholds
    ) as staleness_threshold_minutes,
    CASE
        WHEN tsa.time_in_state_minutes >= calculate_staleness_threshold(tsa.current_state, tsa.template_config, 60, 30, 30)
        THEN 'stale'
        WHEN tsa.time_in_state_minutes >= (calculate_staleness_threshold(tsa.current_state, tsa.template_config, 60, 30, 30) * 0.8)
        THEN 'warning'
        ELSE 'healthy'
    END as health_status,
    tsa.priority
FROM v_task_state_analysis tsa
WHERE tsa.current_state IN ('waiting_for_dependencies', 'waiting_for_retry', 'steps_in_process');

COMMENT ON VIEW v_task_staleness_monitoring IS
'TAS-49: Real-time staleness monitoring for active tasks.

Provides operational visibility into task health:
- "stale": Exceeded threshold, candidate for DLQ
- "warning": At 80% of threshold, needs attention
- "healthy": Within normal operating parameters

Filtered to states that can become stale:
- waiting_for_dependencies
- waiting_for_retry
- steps_in_process

Uses calculate_staleness_threshold() for template-aware thresholds.

Query Examples:
```sql
-- All stale tasks needing investigation
SELECT * FROM v_task_staleness_monitoring WHERE health_status = ''stale'';

-- Tasks approaching staleness by namespace
SELECT namespace_name, COUNT(*)
FROM v_task_staleness_monitoring
WHERE health_status IN (''warning'', ''stale'')
GROUP BY namespace_name;
```';

-- View: DLQ Investigation Queue
-- Prioritized list of pending DLQ investigations

CREATE OR REPLACE VIEW v_dlq_investigation_queue AS
SELECT
    dlq.dlq_entry_uuid,
    dlq.task_uuid,
    dlq.original_state,
    dlq.dlq_reason,
    dlq.dlq_timestamp,
    EXTRACT(EPOCH FROM (NOW() - dlq.dlq_timestamp)) / 60 as minutes_in_dlq,
    dlq.task_snapshot->>'namespace' as namespace_name,
    dlq.task_snapshot->>'task_name' as task_name,
    dlq.task_snapshot->>'current_state' as current_state,
    (dlq.metadata->>'time_in_state_minutes')::INTEGER as time_in_state_minutes,
    -- Priority score: older entries + certain reasons get higher priority
    CASE dlq.dlq_reason
        WHEN 'dependency_cycle_detected' THEN 1000
        WHEN 'max_retries_exceeded' THEN 500
        WHEN 'staleness_timeout' THEN 100
        WHEN 'worker_unavailable' THEN 50
        ELSE 10
    END + EXTRACT(EPOCH FROM (NOW() - dlq.dlq_timestamp)) / 60 as priority_score
FROM tasker_tasks_dlq dlq
WHERE dlq.resolution_status = 'pending'
ORDER BY priority_score DESC;

COMMENT ON VIEW v_dlq_investigation_queue IS
'TAS-49: Prioritized queue of pending DLQ investigations.

Provides operator dashboard with intelligent prioritization:
- Dependency cycles: Highest priority (block entire workflows)
- Max retries exceeded: High priority (permanent failures)
- Staleness timeouts: Medium priority (may self-resolve)
- Age factor: Older entries get higher scores

Columns:
- minutes_in_dlq: How long in investigation queue
- priority_score: Composite score for sorting
- Extracted snapshot fields for quick triage

Usage:
```sql
-- Top 10 investigations needing attention
SELECT * FROM v_dlq_investigation_queue LIMIT 10;

-- Investigations by reason
SELECT dlq_reason, COUNT(*), AVG(minutes_in_dlq)
FROM v_dlq_investigation_queue
GROUP BY dlq_reason;
```';

-- View: DLQ Dashboard Statistics
-- High-level metrics for monitoring and alerting

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
    AVG(EXTRACT(EPOCH FROM (
        COALESCE(resolution_timestamp, NOW()) - dlq_timestamp
    )) / 60) as avg_resolution_time_minutes
FROM tasker_tasks_dlq
GROUP BY dlq_reason;

COMMENT ON VIEW v_dlq_dashboard IS
'TAS-49: High-level DLQ metrics for monitoring and alerting.

Aggregates DLQ statistics by reason:
- Total entries and breakdown by resolution status
- Time range (oldest/newest entries)
- Average resolution time

Usage:
```sql
-- System-wide DLQ health
SELECT * FROM v_dlq_dashboard;

-- Alert: Too many pending investigations
SELECT dlq_reason, pending
FROM v_dlq_dashboard
WHERE pending > 10;
```';

-- ============================================================================
-- PART 10: DISCOVERY FUNCTION
-- ============================================================================

-- Function: Get Stale Tasks for DLQ
-- Discovery function that identifies tasks exceeding staleness thresholds

CREATE OR REPLACE FUNCTION get_stale_tasks_for_dlq(
    p_default_waiting_deps INTEGER DEFAULT 60,
    p_default_waiting_retry INTEGER DEFAULT 30,
    p_default_steps_process INTEGER DEFAULT 30,
    p_max_lifetime_hours INTEGER DEFAULT 24,
    p_batch_size INTEGER DEFAULT 100
)
RETURNS TABLE(
    task_uuid UUID,
    namespace_name VARCHAR,
    task_name VARCHAR,
    current_state VARCHAR,
    time_in_state_minutes NUMERIC,
    threshold_minutes INTEGER,
    task_age_minutes NUMERIC
) AS $$
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
    LEFT JOIN tasker_tasks_dlq dlq
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
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_stale_tasks_for_dlq IS
'TAS-49 Phase 2: Discover stale tasks exceeding thresholds with O(1) optimization.

Key Performance Feature:
- Queries v_task_state_analysis base view ONCE (all expensive joins)
- Main detection function calls this ONCE before loop, not inside loop
- Achieves O(1) expensive operations instead of O(n) per-iteration queries

Anti-Join Optimization:
- LEFT JOIN + IS NULL pattern instead of NOT EXISTS subquery
- Better query planner optimization for excluding DLQ tasks

Template Threshold Support:
- Uses calculate_staleness_threshold() for JSONB config lookups
- Template thresholds override defaults via COALESCE

Parameters:
- p_default_waiting_deps: Default threshold for waiting_for_dependencies (60 min)
- p_default_waiting_retry: Default threshold for waiting_for_retry (30 min)
- p_default_steps_process: Default threshold for steps_in_process (30 min)
- p_max_lifetime_hours: Maximum total task lifetime (24 hours)
- p_batch_size: Limit results to prevent overwhelming system (100)

Returns: Set of stale task records ordered by staleness (worst first)

Example:
```sql
SELECT * FROM get_stale_tasks_for_dlq(60, 30, 30, 24, 50);
```';

-- ============================================================================
-- PART 11: MAIN DETECTION AND TRANSITION FUNCTION
-- ============================================================================

-- Function: Detect and Transition Stale Tasks
-- Main orchestration function for automatic staleness detection

CREATE OR REPLACE FUNCTION detect_and_transition_stale_tasks(
    p_dry_run BOOLEAN DEFAULT true,
    p_batch_size INTEGER DEFAULT 100,
    p_default_waiting_deps_threshold INTEGER DEFAULT 60,
    p_default_waiting_retry_threshold INTEGER DEFAULT 30,
    p_default_steps_in_process_threshold INTEGER DEFAULT 30,
    p_default_task_max_lifetime_hours INTEGER DEFAULT 24
)
RETURNS TABLE(
    task_uuid UUID,
    namespace_name VARCHAR,
    task_name VARCHAR,
    current_state VARCHAR,
    time_in_state_minutes INTEGER,
    staleness_threshold_minutes INTEGER,
    action_taken VARCHAR,
    moved_to_dlq BOOLEAN,
    transition_success BOOLEAN
) AS $$
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
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION detect_and_transition_stale_tasks IS
'TAS-49: Main orchestration function for automatic staleness detection and DLQ processing.

Orchestrates the complete staleness detection workflow:
1. Discovery: Call get_stale_tasks_for_dlq() ONCE (O(1) expensive operations)
2. For each stale task:
   a. Create DLQ entry with task snapshot
   b. Transition task to Error state
   c. Track results and errors

Dry Run Mode:
- p_dry_run = true: Report what would happen without making changes
- p_dry_run = false: Actually create DLQ entries and transition tasks

Action Types Returned:
- would_transition_to_dlq_and_error: Dry run simulation
- transitioned_to_dlq_and_error: Both operations succeeded
- moved_to_dlq_only: DLQ created but state transition failed
- transitioned_to_error_only: State transitioned but DLQ creation failed
- transition_failed: Both operations failed

Parameters:
- p_dry_run: Simulation mode (default true)
- p_batch_size: Max tasks to process per run (default 100)
- p_default_waiting_deps_threshold: Threshold in minutes (default 60)
- p_default_waiting_retry_threshold: Threshold in minutes (default 30)
- p_default_steps_in_process_threshold: Threshold in minutes (default 30)
- p_default_task_max_lifetime_hours: Max task lifetime (default 24)

Returns: Set of processed task records with action taken

Usage:
```sql
-- Dry run to see what would happen
SELECT * FROM detect_and_transition_stale_tasks(true, 10);

-- Real execution with custom thresholds
SELECT * FROM detect_and_transition_stale_tasks(false, 100, 45, 20, 20, 12);
```

Designed for:
- Scheduled background jobs (run every N minutes)
- On-demand operator execution via API
- Monitoring and alerting integration';

-- ============================================================================
-- MIGRATION VALIDATION
-- ============================================================================

-- Verify all components created successfully
DO $$
BEGIN
    -- Check enums
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'dlq_resolution_status') THEN
        RAISE EXCEPTION 'dlq_resolution_status enum not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'dlq_reason') THEN
        RAISE EXCEPTION 'dlq_reason enum not created';
    END IF;

    -- Check table
    IF NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tasker_tasks_dlq') THEN
        RAISE EXCEPTION 'tasker_tasks_dlq table not created';
    END IF;

    -- Check functions
    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'calculate_staleness_threshold') THEN
        RAISE EXCEPTION 'calculate_staleness_threshold function not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'create_dlq_entry') THEN
        RAISE EXCEPTION 'create_dlq_entry function not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'transition_stale_task_to_error') THEN
        RAISE EXCEPTION 'transition_stale_task_to_error function not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'get_stale_tasks_for_dlq') THEN
        RAISE EXCEPTION 'get_stale_tasks_for_dlq function not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'detect_and_transition_stale_tasks') THEN
        RAISE EXCEPTION 'detect_and_transition_stale_tasks function not created';
    END IF;

    -- Check views
    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_task_state_analysis') THEN
        RAISE EXCEPTION 'v_task_state_analysis view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_task_staleness_monitoring') THEN
        RAISE EXCEPTION 'v_task_staleness_monitoring view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_dlq_investigation_queue') THEN
        RAISE EXCEPTION 'v_dlq_investigation_queue view not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_views WHERE viewname = 'v_dlq_dashboard') THEN
        RAISE EXCEPTION 'v_dlq_dashboard view not created';
    END IF;

    RAISE NOTICE 'TAS-49: Comprehensive DLQ system created successfully';
    RAISE NOTICE '  - Tables: 1 (tasker_tasks_dlq)';
    RAISE NOTICE '  - Enums: 2 (dlq_resolution_status, dlq_reason)';
    RAISE NOTICE '  - Functions: 6 (helper + discovery + main + refactored atomic)';
    RAISE NOTICE '  - Views: 4 (dashboard + monitoring + analysis + queue)';
    RAISE NOTICE '  - Indexes: 5 (including JSONB GIN index)';
    RAISE NOTICE '  - Refactored: transition_task_state_atomic (type fix + ownership removal)';
END $$;

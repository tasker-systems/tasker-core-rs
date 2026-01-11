-- ============================================================================
-- TAS-49 Phase 2: DLQ Helper Functions
-- ============================================================================
-- Migration: 20251122000003_add_dlq_helper_functions.sql
-- Description: Helper functions for DLQ operations - threshold calculation,
--              DLQ entry creation, and state transitions
-- Dependencies: 20251115000000_add_dlq_tables.sql, 20251122000002_add_dlq_views.sql
--
-- Functions:
-- 1. calculate_staleness_threshold() - Extract threshold calculation logic
-- 2. create_dlq_entry() - Encapsulate DLQ entry creation
-- 3. transition_stale_task_to_error() - Handle state transition
-- ============================================================================

-- ============================================================================
-- Function: Calculate Staleness Threshold
-- ============================================================================
-- Extracts threshold calculation from inline CASE statement to reusable function.
-- This function determines the appropriate staleness threshold for a given task
-- state by checking template configuration first, then falling back to defaults.
--
-- Parameters:
--   p_task_state       - Current task state (e.g., 'waiting_for_dependencies')
--   p_template_config  - JSONB template configuration
--   p_default_waiting_deps    - Default threshold for waiting_for_dependencies (minutes)
--   p_default_waiting_retry   - Default threshold for waiting_for_retry (minutes)
--   p_default_steps_process   - Default threshold for steps_in_process (minutes)
--
-- Returns: Threshold in minutes (INTEGER)

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

Usage:
- v_task_state_analysis view (replaces inline CASE statement)
- detect_and_transition_stale_tasks() function
- Any monitoring or alerting that needs threshold calculation

Parameters:
- p_task_state: Task state (waiting_for_dependencies, waiting_for_retry, steps_in_process, etc.)
- p_template_config: JSONB configuration from tasker_named_tasks.configuration
- p_default_*: Default thresholds passed from configuration

Returns:
- Threshold in minutes as INTEGER

Examples:
-- Get threshold for waiting_for_dependencies with custom defaults
SELECT calculate_staleness_threshold(
    ''waiting_for_dependencies'',
    ''{"lifecycle": {"max_waiting_for_dependencies_minutes": 120}}''::JSONB,
    60, 30, 30
) as threshold_minutes;  -- Returns: 120

-- Get threshold for state without template override
SELECT calculate_staleness_threshold(
    ''waiting_for_retry'',
    ''{}''::JSONB,
    60, 30, 30
) as threshold_minutes;  -- Returns: 30 (default)

-- Get threshold for unknown state
SELECT calculate_staleness_threshold(
    ''unknown_state'',
    ''{}''::JSONB,
    60, 30, 30
) as threshold_minutes;  -- Returns: 1440 (24 hours)';

-- ============================================================================
-- Function: Create DLQ Entry
-- ============================================================================
-- Encapsulates DLQ entry creation logic with comprehensive snapshot capture.
--
-- Parameters:
--   p_task_uuid          - Task UUID to move to DLQ
--   p_namespace_name     - Namespace name
--   p_task_name          - Task template name
--   p_current_state      - Current task state
--   p_time_in_state_min  - Minutes in current state
--   p_threshold_min      - Staleness threshold that was exceeded
--   p_dlq_reason         - Reason for DLQ entry (default: 'staleness_timeout')
--
-- Returns: UUID of created DLQ entry (or NULL on failure)

CREATE OR REPLACE FUNCTION create_dlq_entry(
    p_task_uuid UUID,
    p_namespace_name VARCHAR,
    p_task_name VARCHAR,
    p_current_state VARCHAR,
    p_time_in_state_min INTEGER,
    p_threshold_min INTEGER,
    p_dlq_reason VARCHAR DEFAULT 'staleness_timeout'
) RETURNS UUID AS $$
DECLARE
    v_dlq_entry_uuid UUID;
BEGIN
    INSERT INTO tasker_tasks_dlq (
        task_uuid,
        original_state,
        dlq_reason,
        task_snapshot,
        metadata,
        resolution_status
    ) VALUES (
        p_task_uuid,
        p_current_state,
        p_dlq_reason::dlq_reason,
        jsonb_build_object(
            'namespace', p_namespace_name,
            'task_name', p_task_name,
            'time_in_state_minutes', p_time_in_state_min,
            'threshold_minutes', p_threshold_min
        ),
        jsonb_build_object(
            'detection_timestamp', NOW(),
            'exceeded_by_minutes', p_time_in_state_min - p_threshold_min
        ),
        'pending'
    )
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
'TAS-49 Phase 2: Create DLQ entry for stale task with comprehensive snapshot.

Encapsulates all DLQ entry creation logic with error handling. Creates entry
with task snapshot, investigation notes, and pending resolution status.

Usage:
- Called by detect_and_transition_stale_tasks() for each stale task
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

Error Handling:
- Catches all exceptions to prevent transaction rollback
- Logs warnings for debugging
- Returns NULL to indicate failure

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

-- ============================================================================
-- Function: Transition Stale Task to Error
-- ============================================================================
-- Handles state transition for stale tasks with proper error handling.
--
-- Parameters:
--   p_task_uuid          - Task UUID to transition
--   p_current_state      - Current state (for validation)
--   p_namespace_name     - Namespace name (for context)
--   p_task_name          - Task name (for context)
--
-- Returns: BOOLEAN indicating success

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

Usage:
- Called by detect_and_transition_stale_tasks() after DLQ entry creation
- Provides staleness-specific metadata in transition record

Parameters:
- p_task_uuid: Task to transition
- p_current_state: Current state for validation
- p_namespace_name: Namespace for audit trail
- p_task_name: Template name for audit trail

Returns:
- TRUE on successful transition
- FALSE on failure (with warning logged)

Error Handling:
- Catches all exceptions to prevent transaction rollback
- Logs warnings for debugging
- Returns FALSE to indicate failure

Atomic Behavior:
- Uses transition_task_state_atomic() for state transition
- Validates current state matches expected state
- Creates transition record with full audit trail
- Processor UUID tracked for audit (not enforced per TAS-54)

Example:
SELECT transition_stale_task_to_error(
    ''550e8400-e29b-41d4-a716-446655440000''::UUID,
    ''waiting_for_dependencies'',
    ''order_processing'',
    ''fulfill_order''
) as transition_success;';

-- ============================================================================
-- Migration Validation
-- ============================================================================

DO $$
BEGIN
    -- Verify helper functions created
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc
        WHERE proname = 'calculate_staleness_threshold'
    ) THEN
        RAISE EXCEPTION 'calculate_staleness_threshold function not created';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_proc
        WHERE proname = 'create_dlq_entry'
    ) THEN
        RAISE EXCEPTION 'create_dlq_entry function not created';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_proc
        WHERE proname = 'transition_stale_task_to_error'
    ) THEN
        RAISE EXCEPTION 'transition_stale_task_to_error function not created';
    END IF;

    RAISE NOTICE 'TAS-49 Phase 2: DLQ helper functions created successfully';
END $$;

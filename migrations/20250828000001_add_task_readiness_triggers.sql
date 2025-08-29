-- TAS-43 Task Readiness Triggers Migration
-- Implements PostgreSQL triggers for event-driven task readiness detection

-- Trigger function for task readiness based on step state changes
-- Leverages existing get_task_execution_context() function for consistency
CREATE OR REPLACE FUNCTION notify_task_ready_on_step_transition() RETURNS TRIGGER AS $$
DECLARE
    task_info RECORD;
    namespace_name TEXT;
    payload JSONB;
    execution_context RECORD;
BEGIN
    -- Only process transitions that could affect task readiness
    -- When steps transition TO 'enqueued', 'complete', 'error', or 'resolved_manually'
    -- other steps might become ready due to dependency satisfaction
    IF NEW.to_state IN ('enqueued', 'complete', 'error', 'resolved_manually') OR
       OLD.to_state IN ('in_progress', 'enqueued') THEN

        -- Get task and namespace information
        SELECT
            t.task_uuid,
            t.priority,
            t.created_at,
            t.complete,
            tn.name as namespace
        INTO task_info
        FROM tasker_workflow_steps ws
        JOIN tasker_tasks t ON t.task_uuid = ws.task_uuid
        JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        WHERE ws.workflow_step_uuid = NEW.workflow_step_uuid;

        -- Skip if task is already complete
        IF task_info.complete THEN
            RETURN NEW;
        END IF;

        -- Use existing get_task_execution_context() for proven readiness logic
        SELECT ready_steps, execution_status
        INTO execution_context
        FROM get_task_execution_context(task_info.task_uuid);

        -- Only notify if task has ready steps (aligns with orchestration loop logic)
        IF execution_context.ready_steps > 0 AND execution_context.execution_status = 'has_ready_steps' THEN

            -- Build minimal payload for pg_notify 8KB limit
            payload := jsonb_build_object(
                'task_uuid', task_info.task_uuid,
                'namespace', task_info.namespace,
                'priority', task_info.priority,
                'ready_steps', execution_context.ready_steps,
                'triggered_by', 'step_transition',
                'step_uuid', NEW.workflow_step_uuid,
                'step_state', NEW.to_state
            );

            -- Send namespace-specific notification
            PERFORM pg_notify('task_ready.' || task_info.namespace, payload::text);

            -- Send global notification for coordinators listening to all namespaces
            PERFORM pg_notify('task_ready', payload::text);

        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply trigger to workflow_step_transitions table
CREATE TRIGGER task_ready_on_step_transition
    AFTER INSERT OR UPDATE ON tasker_workflow_step_transitions
    FOR EACH ROW
    EXECUTE FUNCTION notify_task_ready_on_step_transition();

-- Trigger for task-level state changes
CREATE OR REPLACE FUNCTION notify_task_state_change() RETURNS TRIGGER AS $$
DECLARE
    task_info RECORD;
    namespace_name TEXT;
    payload JSONB;
    execution_context RECORD;
BEGIN
    -- Only process meaningful state transitions
    IF NEW.to_state != OLD.to_state OR OLD.to_state IS NULL THEN

        -- Get task and namespace information
        SELECT
            t.task_uuid,
            t.priority,
            t.created_at,
            t.complete,
            tn.name as namespace
        INTO task_info
        FROM tasker_tasks t
        JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        WHERE t.task_uuid = NEW.task_uuid;

        -- For task completion or error states, notify for cleanup/finalization
        IF NEW.to_state IN ('complete', 'error', 'cancelled') THEN
            payload := jsonb_build_object(
                'task_uuid', task_info.task_uuid,
                'namespace', task_info.namespace,
                'task_state', NEW.to_state,
                'triggered_by', 'task_transition',
                'action_needed', CASE
                    WHEN NEW.to_state = 'complete' THEN 'finalization'
                    WHEN NEW.to_state = 'error' THEN 'error_handling'
                    WHEN NEW.to_state = 'cancelled' THEN 'cleanup'
                END
            );

            PERFORM pg_notify('task_state_change.' || task_info.namespace, payload::text);
            PERFORM pg_notify('task_state_change', payload::text);

        -- For new tasks (transitions to 'in_progress'), check for readiness
        ELSIF NEW.to_state = 'in_progress' AND (OLD.to_state IS NULL OR OLD.to_state = 'pending') THEN
            -- Use get_task_execution_context for consistency
            SELECT ready_steps, execution_status
            INTO execution_context
            FROM get_task_execution_context(task_info.task_uuid);

            IF execution_context.ready_steps > 0 THEN
                payload := jsonb_build_object(
                    'task_uuid', task_info.task_uuid,
                    'namespace', task_info.namespace,
                    'priority', task_info.priority,
                    'ready_steps', execution_context.ready_steps,
                    'triggered_by', 'task_start',
                    'task_state', NEW.to_state
                );

                PERFORM pg_notify('task_ready.' || task_info.namespace, payload::text);
                PERFORM pg_notify('task_ready', payload::text);
            END IF;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply trigger to task_transitions table
CREATE TRIGGER task_state_change_notification
    AFTER INSERT OR UPDATE ON tasker_task_transitions
    FOR EACH ROW
    EXECUTE FUNCTION notify_task_state_change();

-- Namespace creation notification for dynamic coordinator spawning
CREATE OR REPLACE FUNCTION notify_namespace_created() RETURNS TRIGGER AS $$
DECLARE
    payload JSONB;
BEGIN
    payload := jsonb_build_object(
        'namespace_uuid', NEW.task_namespace_uuid,
        'namespace_name', NEW.name,
        'description', NEW.description,
        'created_at', NEW.created_at,
        'triggered_by', 'namespace_creation'
    );

    PERFORM pg_notify('namespace_created', payload::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER namespace_creation_notification
    AFTER INSERT ON tasker_task_namespaces
    FOR EACH ROW
    EXECUTE FUNCTION notify_namespace_created();

-- Index optimizations for trigger performance
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_workflow_step_transitions_task_ready
ON tasker_workflow_step_transitions (workflow_step_uuid, to_state, created_at)
WHERE to_state IN ('enqueued', 'complete', 'error', 'resolved_manually');

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_task_transitions_state_changes
ON tasker_task_transitions (task_uuid, to_state, created_at)
WHERE to_state IN ('in_progress', 'complete', 'error', 'cancelled');

-- Add comments for documentation
COMMENT ON FUNCTION notify_task_ready_on_step_transition() IS 
'TAS-43: Triggers pg_notify when step transitions affect task readiness. Uses existing get_task_execution_context() for consistency.';

COMMENT ON FUNCTION notify_task_state_change() IS 
'TAS-43: Triggers pg_notify on task state changes for completion, error, and new task processing.';

COMMENT ON FUNCTION notify_namespace_created() IS 
'TAS-43: Triggers pg_notify when new namespaces are created for dynamic coordinator spawning.';

COMMENT ON TRIGGER task_ready_on_step_transition ON tasker_workflow_step_transitions IS 
'TAS-43: Event-driven task readiness detection via PostgreSQL LISTEN/NOTIFY';

COMMENT ON TRIGGER task_state_change_notification ON tasker_task_transitions IS 
'TAS-43: Task state change notifications for completion and error handling';

COMMENT ON TRIGGER namespace_creation_notification ON tasker_task_namespaces IS 
'TAS-43: Namespace creation notifications for dynamic coordinator management';
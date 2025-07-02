-- Fix schema alignment migration for Tasker Core Rust
-- Adds missing views and SQL functions from the Rails schema

-- Add missing tables not created in previous migration

-- Step Readiness Status table
CREATE TABLE IF NOT EXISTS tasker_step_readiness_statuses (
    step_readiness_status_id BIGSERIAL PRIMARY KEY,
    workflow_step_id BIGINT NOT NULL REFERENCES tasker_workflow_steps(workflow_step_id),
    dependencies_satisfied BOOLEAN NOT NULL DEFAULT false,
    retry_eligible BOOLEAN NOT NULL DEFAULT false,
    ready_for_execution BOOLEAN NOT NULL DEFAULT false,
    last_readiness_check_at TIMESTAMP WITHOUT TIME ZONE,
    next_readiness_check_at TIMESTAMP WITHOUT TIME ZONE,
    total_parents INTEGER NOT NULL DEFAULT 0,
    completed_parents INTEGER NOT NULL DEFAULT 0,
    backoff_request_seconds INTEGER,
    last_attempted_at TIMESTAMP WITHOUT TIME ZONE,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(workflow_step_id)
);

-- Task Execution Context table
CREATE TABLE IF NOT EXISTS tasker_task_execution_contexts (
    task_execution_context_id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL REFERENCES tasker_tasks(task_id),
    execution_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    recommended_action VARCHAR(64),
    completion_percentage DECIMAL(5,2) NOT NULL DEFAULT 0.00,
    health_status VARCHAR(32) NOT NULL DEFAULT 'unknown',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(task_id)
);

-- Task Diagrams table
CREATE TABLE IF NOT EXISTS tasker_task_diagrams (
    task_diagram_id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL REFERENCES tasker_tasks(task_id),
    diagram_type VARCHAR(32) NOT NULL DEFAULT 'mermaid',
    diagram_data TEXT NOT NULL,
    layout_metadata JSONB NOT NULL DEFAULT '{}',
    generated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    created_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP(6) WITHOUT TIME ZONE NOT NULL DEFAULT NOW()
);

-- Update task_annotations to match expected schema (content instead of annotation)
DO $$ 
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns 
               WHERE table_name = 'tasker_task_annotations' 
               AND column_name = 'annotation') THEN
        ALTER TABLE tasker_task_annotations RENAME COLUMN annotation TO content;
        ALTER TABLE tasker_task_annotations ALTER COLUMN content TYPE TEXT;
    END IF;
END $$;

-- Add missing columns to dependent_system_object_maps if needed
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name = 'tasker_dependent_system_object_maps' 
                   AND column_name = 'object_one_id') THEN
        ALTER TABLE tasker_dependent_system_object_maps 
        ADD COLUMN object_one_id BIGINT NOT NULL DEFAULT 0,
        ADD COLUMN object_two_id BIGINT NOT NULL DEFAULT 0,
        ADD COLUMN mapping_type VARCHAR(64) NOT NULL DEFAULT 'bidirectional',
        ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}';
    END IF;
END $$;

-- Create the step_dag_relationships view
DROP VIEW IF EXISTS tasker_step_dag_relationships;
CREATE VIEW tasker_step_dag_relationships AS
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
   FROM (((tasker_workflow_steps ws
     LEFT JOIN ( SELECT tasker_workflow_step_edges.to_step_id,
            jsonb_agg(tasker_workflow_step_edges.from_step_id ORDER BY tasker_workflow_step_edges.from_step_id) AS parent_ids,
            count(*) AS parent_count
           FROM tasker_workflow_step_edges
          GROUP BY tasker_workflow_step_edges.to_step_id) parent_data ON ((parent_data.to_step_id = ws.workflow_step_id)))
     LEFT JOIN ( SELECT tasker_workflow_step_edges.from_step_id,
            jsonb_agg(tasker_workflow_step_edges.to_step_id ORDER BY tasker_workflow_step_edges.to_step_id) AS child_ids,
            count(*) AS child_count
           FROM tasker_workflow_step_edges
          GROUP BY tasker_workflow_step_edges.from_step_id) child_data ON ((child_data.from_step_id = ws.workflow_step_id)))
     LEFT JOIN ( WITH RECURSIVE step_depths AS (
                 SELECT ws_inner.workflow_step_id,
                    0 AS depth_from_root,
                    ws_inner.task_id
                   FROM tasker_workflow_steps ws_inner
                  WHERE (NOT (EXISTS ( SELECT 1
                           FROM tasker_workflow_step_edges e
                          WHERE (e.to_step_id = ws_inner.workflow_step_id))))
                UNION ALL
                 SELECT e.to_step_id,
                    (sd.depth_from_root + 1),
                    sd.task_id
                   FROM (step_depths sd
                     JOIN tasker_workflow_step_edges e ON ((e.from_step_id = sd.workflow_step_id)))
                  WHERE (sd.depth_from_root < 50)
                )
         SELECT step_depths.workflow_step_id,
            min(step_depths.depth_from_root) AS min_depth_from_root
           FROM step_depths
          GROUP BY step_depths.workflow_step_id) depth_info ON ((depth_info.workflow_step_id = ws.workflow_step_id)));

-- Essential SQL functions for dependency resolution and orchestration

-- Function: calculate_dependency_levels - Critical for step orchestration
CREATE OR REPLACE FUNCTION calculate_dependency_levels(input_task_id bigint) 
RETURNS TABLE(workflow_step_id bigint, dependency_level integer)
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

-- Function: get_step_readiness_status - Critical for step readiness calculation
CREATE OR REPLACE FUNCTION get_step_readiness_status(
    input_task_id bigint, 
    step_ids bigint[] DEFAULT NULL::bigint[]
) 
RETURNS TABLE(
    workflow_step_id bigint, 
    task_id bigint, 
    named_step_id integer, 
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
    retry_limit integer, 
    backoff_request_seconds integer, 
    last_attempted_at timestamp without time zone
)
LANGUAGE plpgsql STABLE
AS $$
BEGIN
    RETURN QUERY
    WITH step_parent_analysis AS (
        SELECT 
            ws.workflow_step_id,
            ws.task_id,
            ws.named_step_id,
            CASE 
                WHEN ws.processed = true THEN 'complete'
                WHEN ws.in_process = true THEN 'in_progress'
                ELSE 'pending'
            END as current_state,
            COALESCE(ws.attempts, 0) as attempts,
            COALESCE(ws.retry_limit, 3) as retry_limit,
            NULL::timestamp as next_retry_at, -- Not available in current schema
            ns.name,
            COALESCE(parent_counts.total_parents, 0) as total_parents,
            COALESCE(completed_parent_counts.completed_parents, 0) as completed_parents,
            CASE 
                WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) 
                     AND ws.retryable = true 
                     AND ws.processed = false
                THEN true 
                ELSE false 
            END as retry_eligible,
            CASE 
                WHEN COALESCE(parent_counts.total_parents, 0) = COALESCE(completed_parent_counts.completed_parents, 0)
                THEN true 
                ELSE false 
            END as dependencies_satisfied,
            NULL::timestamp as last_failure_at, -- Not tracked in current schema
            srs.backoff_request_seconds,
            srs.last_attempted_at
        FROM tasker_workflow_steps ws
        JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
        LEFT JOIN (
            SELECT 
                wse.to_step_id,
                COUNT(*) as total_parents
            FROM tasker_workflow_step_edges wse
            GROUP BY wse.to_step_id
        ) parent_counts ON parent_counts.to_step_id = ws.workflow_step_id
        LEFT JOIN (
            SELECT 
                wse.to_step_id,
                COUNT(*) as completed_parents
            FROM tasker_workflow_step_edges wse
            JOIN tasker_workflow_steps parent_ws ON parent_ws.workflow_step_id = wse.from_step_id
            WHERE parent_ws.processed = true
            GROUP BY wse.to_step_id
        ) completed_parent_counts ON completed_parent_counts.to_step_id = ws.workflow_step_id
        LEFT JOIN tasker_step_readiness_statuses srs ON srs.workflow_step_id = ws.workflow_step_id
        WHERE ws.task_id = input_task_id
          AND (step_ids IS NULL OR ws.workflow_step_id = ANY(step_ids))
    )
    SELECT 
        spa.workflow_step_id,
        spa.task_id,
        spa.named_step_id,
        spa.name,
        spa.current_state,
        spa.dependencies_satisfied,
        spa.retry_eligible,
        CASE 
            WHEN spa.current_state = 'pending' AND spa.dependencies_satisfied
            THEN true
            WHEN spa.retry_eligible AND spa.dependencies_satisfied
            THEN true
            ELSE false
        END as ready_for_execution,
        spa.last_failure_at,
        spa.next_retry_at,
        spa.total_parents::integer,
        spa.completed_parents::integer,
        spa.attempts::integer,
        spa.retry_limit::integer,
        spa.backoff_request_seconds::integer,
        spa.last_attempted_at
    FROM step_parent_analysis spa
    ORDER BY spa.workflow_step_id;
END;
$$;

-- Function: get_task_execution_context - Critical for task orchestration
CREATE OR REPLACE FUNCTION get_task_execution_context(input_task_id bigint) 
RETURNS TABLE(
    task_id bigint, 
    named_task_id integer, 
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
    health_status text
)
LANGUAGE plpgsql STABLE
AS $$
BEGIN
    RETURN QUERY
    WITH task_step_summary AS (
        SELECT 
            t.task_id,
            t.named_task_id,
            CASE WHEN t.complete = true THEN 'complete' ELSE 'in_progress' END as task_status,
            COUNT(ws.workflow_step_id) as total_steps,
            COUNT(ws.workflow_step_id) FILTER (WHERE ws.in_process = false AND ws.processed = false) as pending_steps,
            COUNT(ws.workflow_step_id) FILTER (WHERE ws.in_process = true) as in_progress_steps,
            COUNT(ws.workflow_step_id) FILTER (WHERE ws.processed = true) as completed_steps,
            0::bigint as failed_steps -- Not tracked in current schema
        FROM tasker_tasks t
        LEFT JOIN tasker_workflow_steps ws ON ws.task_id = t.task_id
        WHERE t.task_id = input_task_id
        GROUP BY t.task_id, t.named_task_id, t.complete
    ),
    ready_step_count AS (
        SELECT COUNT(*) as ready_count
        FROM get_step_readiness_status(input_task_id)
        WHERE ready_for_execution = true
    )
    SELECT 
        tss.task_id,
        tss.named_task_id,
        tss.task_status,
        tss.total_steps,
        tss.pending_steps,
        tss.in_progress_steps,
        tss.completed_steps,
        tss.failed_steps,
        rsc.ready_count as ready_steps,
        CASE 
            WHEN tss.total_steps = 0 THEN 'empty'
            WHEN tss.completed_steps = tss.total_steps THEN 'complete'
            WHEN tss.failed_steps > 0 AND (tss.in_progress_steps + rsc.ready_count) = 0 THEN 'stalled'
            WHEN rsc.ready_count > 0 THEN 'ready_to_proceed'
            WHEN tss.in_progress_steps > 0 THEN 'in_progress'
            ELSE 'waiting'
        END as execution_status,
        CASE 
            WHEN tss.total_steps = 0 THEN 'no_steps_defined'
            WHEN tss.completed_steps = tss.total_steps THEN 'task_complete'
            WHEN rsc.ready_count > 0 THEN 'execute_ready_steps'
            WHEN tss.failed_steps > 0 AND (tss.in_progress_steps + rsc.ready_count) = 0 THEN 'review_failures'
            ELSE 'wait_for_completion'
        END as recommended_action,
        CASE 
            WHEN tss.total_steps = 0 THEN 0
            ELSE ROUND((tss.completed_steps::numeric / tss.total_steps * 100), 2)
        END as completion_percentage,
        CASE 
            WHEN tss.failed_steps = 0 THEN 'healthy'
            WHEN (tss.failed_steps::numeric / tss.total_steps) < 0.1 THEN 'minor_issues'
            WHEN (tss.failed_steps::numeric / tss.total_steps) < 0.3 THEN 'degraded'
            ELSE 'unhealthy'
        END as health_status
    FROM task_step_summary tss
    CROSS JOIN ready_step_count rsc;
END;
$$;

-- Add indexes for performance on new tables
CREATE INDEX IF NOT EXISTS idx_tasker_step_readiness_statuses_step_id ON tasker_step_readiness_statuses(workflow_step_id);
CREATE INDEX IF NOT EXISTS idx_tasker_step_readiness_statuses_ready ON tasker_step_readiness_statuses(ready_for_execution) WHERE ready_for_execution = true;
CREATE INDEX IF NOT EXISTS idx_tasker_task_execution_contexts_task_id ON tasker_task_execution_contexts(task_id);
CREATE INDEX IF NOT EXISTS idx_tasker_task_diagrams_task_id ON tasker_task_diagrams(task_id);

-- Add triggers for updated_at columns on new tables
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger 
                   WHERE tgname = 'update_tasker_step_readiness_statuses_updated_at') THEN
        CREATE TRIGGER update_tasker_step_readiness_statuses_updated_at 
            BEFORE UPDATE ON tasker_step_readiness_statuses 
            FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_trigger 
                   WHERE tgname = 'update_tasker_task_execution_contexts_updated_at') THEN
        CREATE TRIGGER update_tasker_task_execution_contexts_updated_at 
            BEFORE UPDATE ON tasker_task_execution_contexts 
            FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_trigger 
                   WHERE tgname = 'update_tasker_task_diagrams_updated_at') THEN
        CREATE TRIGGER update_tasker_task_diagrams_updated_at 
            BEFORE UPDATE ON tasker_task_diagrams 
            FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
    END IF;
END $$;
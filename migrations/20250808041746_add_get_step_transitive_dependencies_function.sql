-- Add SQL function to get transitive dependencies (all ancestor steps) for a given step
-- This enables proper DAG traversal for step result loading without N+1 queries

CREATE OR REPLACE FUNCTION get_step_transitive_dependencies(target_step_id BIGINT)
RETURNS TABLE (
    workflow_step_id BIGINT,
    task_id BIGINT,
    named_step_id INTEGER,
    step_name VARCHAR(128),
    results JSONB,
    processed BOOLEAN,
    distance INTEGER
) AS $$
BEGIN
    RETURN QUERY
    WITH RECURSIVE transitive_ancestors AS (
        -- Base case: direct parent steps
        SELECT 
            ws.workflow_step_id,
            ws.task_id,
            ws.named_step_id,
            ns.name as step_name,
            ws.results,
            ws.processed,
            1 as distance
        FROM tasker_workflow_step_edges e
        JOIN tasker_workflow_steps ws ON ws.workflow_step_id = e.from_step_id
        JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
        WHERE e.to_step_id = target_step_id
        
        UNION ALL
        
        -- Recursive case: find ancestors of ancestors
        SELECT 
            ws.workflow_step_id,
            ws.task_id,
            ws.named_step_id,
            ns.name as step_name,
            ws.results,
            ws.processed,
            ta.distance + 1
        FROM transitive_ancestors ta
        JOIN tasker_workflow_step_edges e ON e.to_step_id = ta.workflow_step_id
        JOIN tasker_workflow_steps ws ON ws.workflow_step_id = e.from_step_id
        JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
        WHERE ta.distance < 50  -- Prevent infinite recursion
    )
    SELECT 
        ta.workflow_step_id,
        ta.task_id,
        ta.named_step_id,
        ta.step_name,
        ta.results,
        ta.processed,
        ta.distance
    FROM transitive_ancestors ta
    ORDER BY ta.distance ASC, ta.workflow_step_id ASC;
END;
$$ LANGUAGE plpgsql STABLE;

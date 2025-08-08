-- Add specialized indexes to optimize the get_step_transitive_dependencies function
-- These indexes are designed to speed up the recursive CTE traversal and result retrieval

-- Composite index for workflow_steps to optimize the JOIN and result extraction in one pass
-- This covers: workflow_step_id lookup, named_step_id JOIN, and includes processed/results for filtering
CREATE INDEX IF NOT EXISTS idx_workflow_steps_transitive_deps 
ON tasker_workflow_steps (workflow_step_id, named_step_id) 
INCLUDE (task_id, results, processed);

-- Composite index for named_steps to optimize the step name lookup
-- This covers the named_step_id JOIN and includes the name for result mapping
CREATE INDEX IF NOT EXISTS idx_named_steps_transitive_deps 
ON tasker_named_steps (named_step_id) 
INCLUDE (name);

-- Note: tasker_workflow_step_edges is already well-indexed for the recursive traversal
-- The existing indexes like idx_step_edges_to_from and index_step_edges_to_step_for_parents
-- provide excellent coverage for the e.to_step_id lookups used in the recursive CTE
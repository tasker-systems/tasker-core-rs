-- TAS-125: Add checkpoint column for handler-driven checkpoint persistence
--
-- This column stores checkpoint data for batch processing workers that yield
-- intermediate progress. The checkpoint structure includes:
-- - cursor: The position where processing should resume
-- - items_processed: Total items processed so far
-- - timestamp: When the checkpoint was created
-- - accumulated_results: Partial results to carry forward
-- - history: Array of previous checkpoint positions for debugging
--
-- Handlers call checkpoint_yield() to persist progress and get re-dispatched,
-- enabling resumability without re-processing already-completed items.

-- Add checkpoint column to workflow_steps table
ALTER TABLE tasker_workflow_steps ADD COLUMN IF NOT EXISTS checkpoint JSONB;

-- Add comment for documentation
COMMENT ON COLUMN tasker_workflow_steps.checkpoint IS 'Handler-driven checkpoint data for batch processing resumability (TAS-125)';

-- Index for efficient checkpoint queries during step resumption
-- Only index non-null checkpoints to avoid bloat
CREATE INDEX IF NOT EXISTS idx_workflow_steps_checkpoint_exists
ON tasker_workflow_steps (workflow_step_uuid)
WHERE checkpoint IS NOT NULL;

-- Index for cursor-based queries (finding steps at specific checkpoint positions)
CREATE INDEX IF NOT EXISTS idx_workflow_steps_checkpoint_cursor
ON tasker_workflow_steps ((checkpoint->>'cursor'))
WHERE checkpoint IS NOT NULL;

-- Add unique constraint to prevent duplicate workflow step edges
-- Related to TAS-59 batch processing - prevents race conditions during worker creation
-- Migration created: 2025-11-18

-- First, check if there are any existing duplicate edges that would violate the constraint
DO $$
DECLARE
    duplicate_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO duplicate_count
    FROM (
        SELECT from_step_uuid, to_step_uuid, name, COUNT(*) as cnt
        FROM tasker_workflow_step_edges
        GROUP BY from_step_uuid, to_step_uuid, name
        HAVING COUNT(*) > 1
    ) duplicates;

    IF duplicate_count > 0 THEN
        RAISE WARNING 'Found % duplicate edge combinations. These will be deduplicated.', duplicate_count;
    END IF;
END $$;

-- Delete duplicate edges, keeping only the oldest one (first created)
-- This preserves the original edge and removes duplicates created by race conditions
WITH ranked_edges AS (
    SELECT
        workflow_step_edge_uuid,
        ROW_NUMBER() OVER (
            PARTITION BY from_step_uuid, to_step_uuid, name
            ORDER BY created_at ASC  -- Keep the oldest edge
        ) as rn
    FROM tasker_workflow_step_edges
)
DELETE FROM tasker_workflow_step_edges
WHERE workflow_step_edge_uuid IN (
    SELECT workflow_step_edge_uuid
    FROM ranked_edges
    WHERE rn > 1
);

-- Add the unique constraint to prevent future duplicates
-- This enforces that a specific edge (from_step -> to_step with given name) can only exist once
ALTER TABLE tasker_workflow_step_edges
ADD CONSTRAINT unique_edge_per_step_pair
UNIQUE (from_step_uuid, to_step_uuid, name);

-- Create an index to support efficient lookups during edge creation
-- This index helps the idempotency check in batch processing service
CREATE INDEX IF NOT EXISTS idx_workflow_step_edges_from_step_name
ON tasker_workflow_step_edges(from_step_uuid, name);

-- Add comment documenting the constraint
COMMENT ON CONSTRAINT unique_edge_per_step_pair ON tasker_workflow_step_edges IS
'Prevents duplicate edges between the same two steps with the same edge name. Critical for batch processing worker creation idempotency.';

-- Add UUID columns to tasks and workflow_steps tables
-- This enables UUID-based simple messaging to prevent stale queue message issues

-- Add UUID column to tasks table
ALTER TABLE tasker_tasks ADD COLUMN task_uuid UUID DEFAULT gen_random_uuid();

-- Backfill existing records (they'll get default UUIDs)
UPDATE tasker_tasks SET task_uuid = gen_random_uuid() WHERE task_uuid IS NULL;

-- Make it NOT NULL and add unique constraint  
ALTER TABLE tasker_tasks ALTER COLUMN task_uuid SET NOT NULL;
ALTER TABLE tasker_tasks ADD CONSTRAINT tasker_tasks_task_uuid_unique UNIQUE (task_uuid);

-- Add index for fast UUID lookups
CREATE INDEX index_tasker_tasks_on_task_uuid ON tasker_tasks (task_uuid);

-- Add UUID column to workflow_steps table  
ALTER TABLE tasker_workflow_steps ADD COLUMN step_uuid UUID DEFAULT gen_random_uuid();

-- Backfill existing records
UPDATE tasker_workflow_steps SET step_uuid = gen_random_uuid() WHERE step_uuid IS NULL;

-- Make it NOT NULL and add unique constraint
ALTER TABLE tasker_workflow_steps ALTER COLUMN step_uuid SET NOT NULL;
ALTER TABLE tasker_workflow_steps ADD CONSTRAINT tasker_workflow_steps_step_uuid_unique UNIQUE (step_uuid);

-- Add index for fast UUID lookups
CREATE INDEX index_tasker_workflow_steps_on_step_uuid ON tasker_workflow_steps (step_uuid);
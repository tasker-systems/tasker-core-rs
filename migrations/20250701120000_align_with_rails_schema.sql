-- Migration to align with updated Rails structure.sql
-- This updates our schema to match the production Rails Tasker engine

-- First, let's update the tasker_tasks table to match Rails schema
ALTER TABLE tasker_tasks 
    DROP COLUMN state,
    DROP COLUMN most_recent_error_message,
    DROP COLUMN most_recent_error_backtrace,
    ADD COLUMN complete BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN requested_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT NOW(),
    ADD COLUMN initiator VARCHAR(128),
    ADD COLUMN source_system VARCHAR(128),
    ADD COLUMN reason VARCHAR(128),
    ADD COLUMN bypass_steps JSON,
    ADD COLUMN tags JSONB,
    ADD COLUMN identity_hash VARCHAR(128) NOT NULL DEFAULT '';

-- Update tasker_named_tasks to use string versioning like Rails
ALTER TABLE tasker_named_tasks 
    DROP COLUMN version,
    ADD COLUMN version VARCHAR(16) NOT NULL DEFAULT '0.1.0',
    ADD COLUMN configuration JSONB DEFAULT '{}';

-- Update tasker_named_steps to match Rails schema
ALTER TABLE tasker_named_steps 
    DROP COLUMN version,
    DROP COLUMN handler_class,
    ADD COLUMN dependent_system_id INTEGER NOT NULL DEFAULT 1;

-- Update tasker_workflow_steps to match Rails schema
ALTER TABLE tasker_workflow_steps 
    DROP COLUMN state,
    DROP COLUMN context,
    DROP COLUMN output,
    DROP COLUMN retry_count,
    DROP COLUMN max_retries,
    DROP COLUMN next_retry_at,
    DROP COLUMN most_recent_error_message,
    DROP COLUMN most_recent_error_backtrace,
    ADD COLUMN retryable BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN retry_limit INTEGER DEFAULT 3,
    ADD COLUMN in_process BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN processed BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN processed_at TIMESTAMP WITHOUT TIME ZONE,
    ADD COLUMN attempts INTEGER,
    ADD COLUMN last_attempted_at TIMESTAMP WITHOUT TIME ZONE,
    ADD COLUMN backoff_request_seconds INTEGER,
    ADD COLUMN inputs JSONB,
    ADD COLUMN results JSONB,
    ADD COLUMN skippable BOOLEAN NOT NULL DEFAULT false;

-- Update tasker_workflow_step_edges to have id and name fields
ALTER TABLE tasker_workflow_step_edges 
    DROP CONSTRAINT tasker_workflow_step_edges_pkey,
    ADD COLUMN id BIGSERIAL PRIMARY KEY,
    ADD COLUMN name VARCHAR NOT NULL DEFAULT 'depends_on';

-- Update tasker_named_tasks_named_steps to match Rails schema
ALTER TABLE tasker_named_tasks_named_steps 
    DROP CONSTRAINT tasker_named_tasks_named_steps_pkey,
    ADD COLUMN id SERIAL PRIMARY KEY,
    ADD COLUMN skippable BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN default_retryable BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN default_retry_limit INTEGER NOT NULL DEFAULT 3;

-- Add missing tables that exist in Rails but not in our schema

-- Annotation Types
CREATE TABLE tasker_annotation_types (
    annotation_type_id SERIAL PRIMARY KEY,
    name VARCHAR(64) NOT NULL,
    description VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Task Annotations
CREATE TABLE tasker_task_annotations (
    task_annotation_id BIGSERIAL PRIMARY KEY,
    task_id BIGINT NOT NULL REFERENCES tasker_tasks(task_id),
    annotation_type_id INTEGER NOT NULL REFERENCES tasker_annotation_types(annotation_type_id),
    annotation JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Dependent Systems
CREATE TABLE tasker_dependent_systems (
    dependent_system_id SERIAL PRIMARY KEY,
    name VARCHAR(64) NOT NULL,
    description VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Dependent System Object Maps
CREATE TABLE tasker_dependent_system_object_maps (
    dependent_system_object_map_id BIGSERIAL PRIMARY KEY,
    dependent_system_one_id INTEGER NOT NULL REFERENCES tasker_dependent_systems(dependent_system_id),
    dependent_system_two_id INTEGER NOT NULL REFERENCES tasker_dependent_systems(dependent_system_id),
    remote_id_one VARCHAR(128) NOT NULL,
    remote_id_two VARCHAR(128) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert default dependent system
INSERT INTO tasker_dependent_systems (dependent_system_id, name, description) 
VALUES (1, 'default', 'Default dependent system') 
ON CONFLICT DO NOTHING;

-- Update the sequence to start after our manual insert
SELECT setval('tasker_dependent_systems_dependent_system_id_seq', COALESCE(MAX(dependent_system_id), 1)) FROM tasker_dependent_systems;

-- Update named_steps to reference the default dependent system
UPDATE tasker_named_steps SET dependent_system_id = 1 WHERE dependent_system_id IS NULL;

-- Drop old indexes that referenced dropped columns
DROP INDEX IF EXISTS idx_tasker_tasks_state;
DROP INDEX IF EXISTS idx_tasker_workflow_steps_state;
DROP INDEX IF EXISTS idx_tasker_workflow_steps_next_retry_at;

-- Add new indexes for the updated schema
CREATE INDEX idx_tasker_tasks_complete ON tasker_tasks(complete);
CREATE INDEX idx_tasker_tasks_identity_hash ON tasker_tasks(identity_hash);
CREATE INDEX idx_tasker_tasks_requested_at ON tasker_tasks(requested_at);

CREATE INDEX idx_tasker_workflow_steps_in_process ON tasker_workflow_steps(in_process);
CREATE INDEX idx_tasker_workflow_steps_processed ON tasker_workflow_steps(processed);
CREATE INDEX idx_tasker_workflow_steps_last_attempted_at ON tasker_workflow_steps(last_attempted_at) WHERE last_attempted_at IS NOT NULL;

CREATE INDEX idx_tasker_task_annotations_task_id ON tasker_task_annotations(task_id);
CREATE INDEX idx_tasker_task_annotations_annotation_type_id ON tasker_task_annotations(annotation_type_id);

-- Add triggers for new tables
CREATE TRIGGER update_tasker_annotation_types_updated_at BEFORE UPDATE ON tasker_annotation_types FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_task_annotations_updated_at BEFORE UPDATE ON tasker_task_annotations FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_dependent_systems_updated_at BEFORE UPDATE ON tasker_dependent_systems FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_dependent_system_object_maps_updated_at BEFORE UPDATE ON tasker_dependent_system_object_maps FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_named_tasks_named_steps_updated_at BEFORE UPDATE ON tasker_named_tasks_named_steps FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_workflow_step_edges_updated_at BEFORE UPDATE ON tasker_workflow_step_edges FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
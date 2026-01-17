-- =============================================================================
-- TAS-128: Flattened Schema Migration - Part 2: Constraints and Indexes
-- =============================================================================
-- This migration adds:
-- - Primary key constraints
-- - Unique constraints
-- - Foreign key constraints
-- - Indexes
--
-- Note: Triggers are created in migration 3 after functions are defined.
-- =============================================================================

-- Set search_path for this session
SET search_path TO tasker, public;

-- =============================================================================
-- PRIMARY KEY CONSTRAINTS
-- =============================================================================
-- TAS-136: Removed constraints for dropped tables (annotation_types, task_annotations,
-- dependent_systems, dependent_system_object_maps). See docs/architecture/schema-design.md.

ALTER TABLE ONLY tasker.named_steps
    ADD CONSTRAINT named_steps_pkey PRIMARY KEY (named_step_uuid);

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_pkey PRIMARY KEY (ntns_uuid);

ALTER TABLE ONLY tasker.named_tasks
    ADD CONSTRAINT named_tasks_pkey PRIMARY KEY (named_task_uuid);

ALTER TABLE ONLY tasker.task_namespaces
    ADD CONSTRAINT task_namespaces_pkey PRIMARY KEY (task_namespace_uuid);

ALTER TABLE ONLY tasker.task_transitions
    ADD CONSTRAINT task_transitions_pkey PRIMARY KEY (task_transition_uuid);

ALTER TABLE ONLY tasker.tasks_dlq
    ADD CONSTRAINT tasks_dlq_pkey PRIMARY KEY (dlq_entry_uuid);

ALTER TABLE ONLY tasker.tasks
    ADD CONSTRAINT tasks_pkey PRIMARY KEY (task_uuid);

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT workflow_step_edges_pkey PRIMARY KEY (workflow_step_edge_uuid);

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_pkey PRIMARY KEY (workflow_step_result_audit_uuid);

ALTER TABLE ONLY tasker.workflow_step_transitions
    ADD CONSTRAINT workflow_step_transitions_pkey PRIMARY KEY (workflow_step_transition_uuid);

ALTER TABLE ONLY tasker.workflow_steps
    ADD CONSTRAINT workflow_steps_pkey PRIMARY KEY (workflow_step_uuid);

-- =============================================================================
-- UNIQUE CONSTRAINTS
-- =============================================================================

ALTER TABLE ONLY tasker.named_steps
    ADD CONSTRAINT named_steps_name_unique UNIQUE (name);

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_unique UNIQUE (named_task_uuid, named_step_uuid);

ALTER TABLE ONLY tasker.named_tasks
    ADD CONSTRAINT named_tasks_namespace_name_version_unique UNIQUE (task_namespace_uuid, name, version);

ALTER TABLE ONLY tasker.task_namespaces
    ADD CONSTRAINT task_namespaces_name_key UNIQUE (name);

ALTER TABLE ONLY tasker.task_namespaces
    ADD CONSTRAINT task_namespaces_name_unique UNIQUE (name);

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT unique_edge_per_step_pair UNIQUE (from_step_uuid, to_step_uuid, name);

COMMENT ON CONSTRAINT unique_edge_per_step_pair ON tasker.workflow_step_edges IS 'Prevents duplicate edges between the same two steps with the same edge name. Critical for batch processing worker creation idempotency.';

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT uq_audit_step_transition UNIQUE (workflow_step_uuid, workflow_step_transition_uuid);

ALTER TABLE ONLY tasker.workflow_steps
    ADD CONSTRAINT uq_workflow_step_task_named_step UNIQUE (task_uuid, named_step_uuid);

COMMENT ON CONSTRAINT uq_workflow_step_task_named_step ON tasker.workflow_steps IS 'TAS-151: Prevents duplicate workflow steps for the same task and named step. Critical for idempotency in decision point processing where multiple orchestrators may process the same outcome concurrently.';

-- =============================================================================
-- FOREIGN KEY CONSTRAINTS
-- =============================================================================

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_named_step_uuid_fkey FOREIGN KEY (named_step_uuid) REFERENCES tasker.named_steps(named_step_uuid);

ALTER TABLE ONLY tasker.named_tasks_named_steps
    ADD CONSTRAINT named_tasks_named_steps_named_task_uuid_fkey FOREIGN KEY (named_task_uuid) REFERENCES tasker.named_tasks(named_task_uuid);

ALTER TABLE ONLY tasker.named_tasks
    ADD CONSTRAINT named_tasks_task_namespace_uuid_fkey FOREIGN KEY (task_namespace_uuid) REFERENCES tasker.task_namespaces(task_namespace_uuid);

ALTER TABLE ONLY tasker.task_transitions
    ADD CONSTRAINT task_transitions_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);

ALTER TABLE ONLY tasker.tasks_dlq
    ADD CONSTRAINT tasks_dlq_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);

ALTER TABLE ONLY tasker.tasks
    ADD CONSTRAINT tasks_named_task_uuid_fkey FOREIGN KEY (named_task_uuid) REFERENCES tasker.named_tasks(named_task_uuid);

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT workflow_step_edges_from_step_uuid_fkey FOREIGN KEY (from_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);

ALTER TABLE ONLY tasker.workflow_step_edges
    ADD CONSTRAINT workflow_step_edges_to_step_uuid_fkey FOREIGN KEY (to_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_step_transition_uuid_fkey FOREIGN KEY (workflow_step_transition_uuid) REFERENCES tasker.workflow_step_transitions(workflow_step_transition_uuid);

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);

ALTER TABLE ONLY tasker.workflow_step_result_audit
    ADD CONSTRAINT workflow_step_result_audit_workflow_step_uuid_fkey FOREIGN KEY (workflow_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);

ALTER TABLE ONLY tasker.workflow_step_transitions
    ADD CONSTRAINT workflow_step_transitions_workflow_step_uuid_fkey FOREIGN KEY (workflow_step_uuid) REFERENCES tasker.workflow_steps(workflow_step_uuid);

ALTER TABLE ONLY tasker.workflow_steps
    ADD CONSTRAINT workflow_steps_named_step_uuid_fkey FOREIGN KEY (named_step_uuid) REFERENCES tasker.named_steps(named_step_uuid);

ALTER TABLE ONLY tasker.workflow_steps
    ADD CONSTRAINT workflow_steps_task_uuid_fkey FOREIGN KEY (task_uuid) REFERENCES tasker.tasks(task_uuid);

-- =============================================================================
-- INDEXES
-- =============================================================================

-- Audit table indexes
CREATE INDEX idx_audit_correlation_id ON tasker.workflow_step_result_audit USING btree (correlation_id) WHERE (correlation_id IS NOT NULL);
CREATE INDEX idx_audit_recorded_at ON tasker.workflow_step_result_audit USING btree (recorded_at);
CREATE INDEX idx_audit_step_uuid ON tasker.workflow_step_result_audit USING btree (workflow_step_uuid);
CREATE INDEX idx_audit_success ON tasker.workflow_step_result_audit USING btree (success);
CREATE INDEX idx_audit_task_uuid ON tasker.workflow_step_result_audit USING btree (task_uuid);
CREATE INDEX idx_audit_worker_uuid ON tasker.workflow_step_result_audit USING btree (worker_uuid) WHERE (worker_uuid IS NOT NULL);

-- DLQ indexes
CREATE INDEX idx_dlq_reason ON tasker.tasks_dlq USING btree (dlq_reason);
CREATE INDEX idx_dlq_resolution_status ON tasker.tasks_dlq USING btree (resolution_status, dlq_timestamp);
CREATE INDEX idx_dlq_task_lookup ON tasker.tasks_dlq USING btree (task_uuid);
CREATE INDEX idx_dlq_timestamp ON tasker.tasks_dlq USING btree (dlq_timestamp DESC);
CREATE UNIQUE INDEX idx_dlq_unique_pending_task ON tasker.tasks_dlq USING btree (task_uuid) WHERE (resolution_status = 'pending'::tasker.dlq_resolution_status);

COMMENT ON INDEX tasker.idx_dlq_unique_pending_task IS 'TAS-49: Ensures only one pending DLQ investigation per task.
Historical DLQ entries (manually_resolved, permanently_failed, cancelled) are preserved as audit trail.
Allows multiple DLQ investigations per task over time, but only one active investigation at a time.';

-- Named tasks lifecycle config index
CREATE INDEX idx_named_tasks_lifecycle_config ON tasker.named_tasks USING gin (((configuration -> 'lifecycle'::text))) WHERE ((configuration -> 'lifecycle'::text) IS NOT NULL);

COMMENT ON INDEX tasker.idx_named_tasks_lifecycle_config IS 'TAS-49 Phase 2: GIN index for lifecycle configuration JSONB path queries.
Optimizes threshold lookups in calculate_staleness_threshold() function.';

-- Task transition indexes
CREATE INDEX idx_task_transitions_processor ON tasker.task_transitions USING btree (processor_uuid) WHERE (processor_uuid IS NOT NULL);
CREATE INDEX idx_task_transitions_state_lookup ON tasker.task_transitions USING btree (task_uuid, to_state, most_recent) WHERE (most_recent = true);
CREATE INDEX idx_task_transitions_timeout ON tasker.task_transitions USING btree (((transition_metadata ->> 'timeout_at'::text))) WHERE (most_recent = true);
CREATE UNIQUE INDEX idx_task_transitions_uuid_most_recent ON tasker.task_transitions USING btree (task_uuid, most_recent) WHERE (most_recent = true);
CREATE INDEX idx_task_transitions_uuid_sort_key ON tasker.task_transitions USING btree (task_uuid, sort_key);
CREATE INDEX idx_task_transitions_uuid_temporal ON tasker.task_transitions USING btree (task_transition_uuid, created_at);
CREATE INDEX idx_task_transitions_most_recent ON tasker.task_transitions USING btree (most_recent) WHERE (most_recent = true);
CREATE INDEX idx_task_transitions_task_uuid ON tasker.task_transitions USING btree (task_uuid);

-- Named tasks index
CREATE INDEX idx_named_tasks_namespace_uuid ON tasker.named_tasks USING btree (task_namespace_uuid);

-- Task namespaces index
CREATE INDEX idx_task_namespaces_name ON tasker.task_namespaces USING btree (name);

-- Tasks indexes
CREATE INDEX idx_tasks_complete ON tasker.tasks USING btree (complete);
CREATE INDEX idx_tasks_completed_at ON tasker.tasks USING btree (completed_at);
CREATE INDEX idx_tasks_correlation_hierarchy ON tasker.tasks USING btree (parent_correlation_id, correlation_id) WHERE (parent_correlation_id IS NOT NULL);
CREATE INDEX idx_tasks_correlation_id ON tasker.tasks USING btree (correlation_id);
CREATE INDEX idx_tasks_identity_hash ON tasker.tasks USING btree (identity_hash);
CREATE INDEX idx_tasks_named_task_uuid ON tasker.tasks USING btree (named_task_uuid);
CREATE INDEX idx_tasks_parent_correlation_id ON tasker.tasks USING btree (parent_correlation_id) WHERE (parent_correlation_id IS NOT NULL);
CREATE INDEX idx_tasks_priority ON tasker.tasks USING btree (priority);
CREATE INDEX idx_tasks_requested_at ON tasker.tasks USING btree (requested_at);
CREATE INDEX idx_tasks_source_system ON tasker.tasks USING btree (source_system);
CREATE INDEX idx_tasks_tags_gin ON tasker.tasks USING gin (tags);
CREATE INDEX idx_tasks_tags_gin_path ON tasker.tasks USING gin (tags jsonb_path_ops);
CREATE INDEX idx_tasks_active_with_priority_covering ON tasker.tasks USING btree (complete, priority, task_uuid) INCLUDE (named_task_uuid, requested_at) WHERE (complete = false);
CREATE INDEX idx_tasks_uuid_temporal ON tasker.tasks USING btree (task_uuid, created_at);

-- Workflow step edges indexes
CREATE INDEX idx_workflow_step_edges_from_step ON tasker.workflow_step_edges USING btree (from_step_uuid);
CREATE INDEX idx_workflow_step_edges_to_step ON tasker.workflow_step_edges USING btree (to_step_uuid);
CREATE INDEX idx_workflow_step_edges_from_step_name ON tasker.workflow_step_edges USING btree (from_step_uuid, name);

-- Workflow step transitions indexes
CREATE INDEX idx_workflow_step_transitions_most_recent ON tasker.workflow_step_transitions USING btree (most_recent) WHERE (most_recent = true);
CREATE INDEX idx_workflow_step_transitions_step_uuid ON tasker.workflow_step_transitions USING btree (workflow_step_uuid);
CREATE INDEX idx_workflow_step_transitions_state_lookup ON tasker.workflow_step_transitions USING btree (workflow_step_uuid, to_state, most_recent) WHERE (most_recent = true);
CREATE UNIQUE INDEX idx_workflow_step_transitions_uuid_most_recent ON tasker.workflow_step_transitions USING btree (workflow_step_uuid, most_recent) WHERE (most_recent = true);
CREATE INDEX idx_workflow_step_transitions_uuid_sort_key ON tasker.workflow_step_transitions USING btree (workflow_step_uuid, sort_key);
CREATE INDEX idx_workflow_step_transitions_uuid_temporal ON tasker.workflow_step_transitions USING btree (workflow_step_transition_uuid, created_at);

-- Workflow steps indexes
CREATE INDEX idx_workflow_steps_named_step_uuid ON tasker.workflow_steps USING btree (named_step_uuid);
CREATE INDEX idx_workflow_steps_task_uuid ON tasker.workflow_steps USING btree (task_uuid);
CREATE INDEX idx_workflow_steps_active_operations ON tasker.workflow_steps USING btree (workflow_step_uuid, task_uuid) WHERE ((processed = false) OR (processed IS NULL));
CREATE INDEX idx_workflow_steps_checkpoint_cursor ON tasker.workflow_steps USING btree (((checkpoint ->> 'cursor'::text))) WHERE (checkpoint IS NOT NULL);
CREATE INDEX idx_workflow_steps_checkpoint_exists ON tasker.workflow_steps USING btree (workflow_step_uuid) WHERE (checkpoint IS NOT NULL);
CREATE INDEX idx_workflow_steps_processed_at ON tasker.workflow_steps USING btree (processed_at);
CREATE INDEX idx_workflow_steps_processing_status ON tasker.workflow_steps USING btree (task_uuid, processed, in_process);
CREATE INDEX idx_workflow_steps_ready_covering ON tasker.workflow_steps USING btree (task_uuid, processed, in_process) INCLUDE (workflow_step_uuid, attempts, max_attempts, retryable) WHERE (processed = false);
CREATE INDEX idx_workflow_steps_retry_logic ON tasker.workflow_steps USING btree (attempts, max_attempts, retryable);
CREATE INDEX idx_workflow_steps_retry_status ON tasker.workflow_steps USING btree (attempts, max_attempts);
CREATE INDEX idx_workflow_steps_task_covering ON tasker.workflow_steps USING btree (task_uuid) INCLUDE (workflow_step_uuid, processed, in_process, attempts, max_attempts);
CREATE INDEX idx_workflow_steps_task_grouping_active ON tasker.workflow_steps USING btree (task_uuid, workflow_step_uuid) WHERE ((processed = false) OR (processed IS NULL));
CREATE INDEX idx_workflow_steps_task_readiness ON tasker.workflow_steps USING btree (task_uuid, processed, workflow_step_uuid) WHERE (processed = false);
CREATE INDEX idx_workflow_steps_transitive_deps ON tasker.workflow_steps USING btree (workflow_step_uuid, named_step_uuid) INCLUDE (task_uuid, processed);

-- ============================================================================
-- TAS-49 Phase 1.1: Dead Letter Queue (DLQ) Tables and Archive Infrastructure
-- ============================================================================
-- Migration: 20251115000000_add_dlq_tables.sql
-- Description: Creates DLQ investigation tracking and archive tables
-- Dependencies: 20250912000000_tas41_richer_task_states.sql (12-state machine)
--
-- Architecture:
-- - DLQ = Investigation tracker (NOT task manipulation layer)
-- - Tasks remain in tasker_tasks (typically Error state)
-- - DLQ entries track "why stuck" and "what operator did"
-- - Archive tables for long-term storage (simple, no partitioning)
-- ============================================================================

-- ============================================================================
-- Utility Functions
-- ============================================================================

-- Function: Automatically update updated_at timestamp
--
-- Standard PostgreSQL function for maintaining updated_at columns.
-- Used by triggers to automatically set updated_at to current timestamp.
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION update_updated_at_column() IS
'TAS-49: Trigger function to automatically update updated_at timestamp on row modification';

-- ============================================================================
-- Enums for Type Safety
-- ============================================================================

-- DLQ Resolution Status Enum
-- Tracks the lifecycle of a DLQ investigation (NOT task state)
CREATE TYPE dlq_resolution_status AS ENUM (
    'pending',              -- Investigation in progress
    'manually_resolved',   -- Operator fixed problem steps, task progressed
    'permanently_failed',  -- Unfixable issue (e.g., bad template, data corruption)
    'cancelled'            -- Investigation cancelled (duplicate, false positive, etc.)
);

COMMENT ON TYPE dlq_resolution_status IS
'TAS-49: DLQ investigation status (separate from task state).

State Machine:
  pending → manually_resolved (operator fixed problem via step APIs, task progressed)
  pending → permanently_failed (unfixable issue, task stays in Error state)
  pending → cancelled (investigation no longer needed)

Key Principle: This tracks the INVESTIGATION workflow, not task state.
- Task state is managed by TAS-41 state machine
- Resolution happens at step level via existing step APIs
- DLQ entry tracks "what operator did to investigate"

Example: Task stuck on payment step → DLQ entry created (pending) → operator
resets payment step retry count via step API → task progresses → DLQ entry
updated (manually_resolved).

A task can have multiple DLQ entries over time (investigation history), but
only one pending entry at a time.';

-- DLQ Reason Enum
-- Why was the task sent to DLQ?
CREATE TYPE dlq_reason AS ENUM (
    'staleness_timeout',        -- Exceeded state timeout threshold
    'max_retries_exceeded',     -- TAS-42 retry limit hit
    'dependency_cycle_detected', -- Circular dependency discovered
    'worker_unavailable',       -- No worker available for extended period
    'manual_dlq'               -- Operator manually sent to DLQ
);

COMMENT ON TYPE dlq_reason IS
'TAS-49: Reasons a task is sent to DLQ. Determines investigation priority and remediation approach.';

-- ============================================================================
-- Dead Letter Queue Table
-- ============================================================================

-- Purpose: Investigation tracking and audit trail for stuck tasks
-- Note: Tasks remain in tasker_tasks (in Error state), DLQ tracks investigation workflow
CREATE TABLE tasker_tasks_dlq (
    dlq_entry_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    task_uuid UUID NOT NULL REFERENCES tasker_tasks(task_uuid),

    -- DLQ metadata
    original_state VARCHAR(50) NOT NULL,  -- Task state when sent to DLQ
    dlq_reason dlq_reason NOT NULL,
    dlq_timestamp TIMESTAMP NOT NULL DEFAULT NOW(),

    -- Investigation tracking (NOT task retry tracking - that's at step level)
    resolution_status dlq_resolution_status NOT NULL DEFAULT 'pending',
    resolution_timestamp TIMESTAMP,
    resolution_notes TEXT,
    resolved_by VARCHAR(255),

    -- Task snapshot for investigation (full task + steps state at DLQ time)
    task_snapshot JSONB NOT NULL,
    metadata JSONB,  -- For extensibility (e.g., investigation tags, related tickets)

    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tasker_tasks_dlq IS
'TAS-49: Dead Letter Queue investigation tracking and audit trail.

Architecture: DLQ is an INVESTIGATION TRACKER, not a task manipulation layer
- Tasks remain in tasker_tasks table (typically in Error state)
- DLQ entries track "why stuck" and "what operator did"
- Multiple DLQ entries per task allowed (historical trail across multiple instances)
- Only one "pending" investigation per task at a time

Workflow Example:
1. Task stuck 60min → staleness detector → task Error state + DLQ entry (pending)
2. Operator investigates via DLQ API → reviews task_snapshot JSONB
3. Operator fixes problem steps using existing step APIs (PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid})
4. Task state machine automatically progresses when steps fixed
5. Operator updates DLQ entry to track resolution (PATCH /api/v1/dlq/{dlq_entry_uuid})

Query patterns:
- Active investigations: WHERE resolution_status = ''pending'' ORDER BY dlq_timestamp
- DLQ history for task: WHERE task_uuid = $1 ORDER BY created_at
- Pattern analysis: GROUP BY dlq_reason to identify systemic issues';

COMMENT ON COLUMN tasker_tasks_dlq.task_snapshot IS
'Complete snapshot of task state when moved to DLQ.
Includes task details, steps, transitions, and execution context.
Enables investigation without modifying main task tables.';

COMMENT ON COLUMN tasker_tasks_dlq.metadata IS
'Additional context about DLQ entry.
Examples: error stack trace, detection method, related tasks, investigation notes.';

-- ============================================================================
-- DLQ Indexes
-- ============================================================================

-- Fast task lookup for investigation
CREATE INDEX idx_dlq_task_lookup ON tasker_tasks_dlq(task_uuid);

-- Active investigations dashboard
CREATE INDEX idx_dlq_resolution_status ON tasker_tasks_dlq(resolution_status, dlq_timestamp);

-- Pattern analysis and alerting
CREATE INDEX idx_dlq_reason ON tasker_tasks_dlq(dlq_reason);

-- Investigation timestamp range queries
CREATE INDEX idx_dlq_timestamp ON tasker_tasks_dlq(dlq_timestamp DESC);

-- ============================================================================
-- DLQ Constraints
-- ============================================================================

-- Constraint: One pending DLQ entry per task
-- Historical entries (manually_resolved, permanently_failed, cancelled) preserved as audit trail
CREATE UNIQUE INDEX idx_dlq_unique_pending_task
    ON tasker_tasks_dlq(task_uuid)
    WHERE resolution_status = 'pending';

COMMENT ON INDEX idx_dlq_unique_pending_task IS
'TAS-49: Ensures only one pending DLQ investigation per task.
Historical DLQ entries (manually_resolved, permanently_failed, cancelled) are preserved as audit trail.
Allows multiple DLQ investigations per task over time, but only one active investigation at a time.';

-- ============================================================================
-- DLQ Triggers
-- ============================================================================

-- Trigger: Update updated_at on row modification
CREATE TRIGGER update_dlq_updated_at
    BEFORE UPDATE ON tasker_tasks_dlq
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Archive Tables (Simple, No Partitioning)
-- ============================================================================

-- Archive table for completed tasks
-- Note: Pre-alpha simplification - no partitioning initially
-- Partitioning can be added later if table growth becomes issue
CREATE TABLE tasker_tasks_archive (
    LIKE tasker_tasks INCLUDING ALL,
    archived_at TIMESTAMP NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tasker_tasks_archive IS
'TAS-49: Long-term storage for completed/failed tasks past retention period.
Simple archive strategy without partitioning (partitioning deferred to future work).
Preserves complete task state for compliance/audit.';

-- Archive table for workflow steps
CREATE TABLE tasker_workflow_steps_archive (
    LIKE tasker_workflow_steps INCLUDING ALL,
    archived_at TIMESTAMP NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tasker_workflow_steps_archive IS
'TAS-49: Archive storage for workflow steps of archived tasks.';

-- Archive table for task transitions
CREATE TABLE tasker_task_transitions_archive (
    LIKE tasker_task_transitions INCLUDING ALL,
    archived_at TIMESTAMP NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE tasker_task_transitions_archive IS
'TAS-49: Archive storage for state transitions of archived tasks.
Preserves complete audit trail for historical analysis.';

-- ============================================================================
-- Archive Indexes
-- ============================================================================

-- Fast archived task lookup by task_uuid
CREATE INDEX idx_archive_task_uuid ON tasker_tasks_archive(task_uuid);
CREATE INDEX idx_archive_steps_task_uuid ON tasker_workflow_steps_archive(task_uuid);
CREATE INDEX idx_archive_transitions_task_uuid ON tasker_task_transitions_archive(task_uuid);

-- Archival timestamp range queries
CREATE INDEX idx_archive_archived_at ON tasker_tasks_archive(archived_at DESC);

-- Named task analysis in archive
CREATE INDEX idx_archive_named_task ON tasker_tasks_archive(named_task_uuid);

-- ============================================================================
-- Migration Validation
-- ============================================================================

-- Verify enums created
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'dlq_resolution_status') THEN
        RAISE EXCEPTION 'dlq_resolution_status enum not created';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'dlq_reason') THEN
        RAISE EXCEPTION 'dlq_reason enum not created';
    END IF;

    RAISE NOTICE 'TAS-49 Phase 1.1: DLQ tables and enums created successfully';
END $$;

-- =============================================================================
-- TAS-128: Flattened Schema Migration - Part 1: Schema and Tables
-- =============================================================================
-- This migration creates the tasker schema and all tables with their basic
-- column definitions. Foreign keys and indexes are added in the next migration.
--
-- Compatible with PostgreSQL 17 (pg_uuidv7 extension) and PostgreSQL 18+ (native uuidv7)
-- =============================================================================

-- Set search_path for this session
SET search_path TO tasker, public;

-- =============================================================================
-- SCHEMA CREATION
-- =============================================================================

CREATE SCHEMA IF NOT EXISTS tasker;

-- =============================================================================
-- UUID v7 COMPATIBILITY LAYER
-- =============================================================================
-- Universal UUID v7 support that works on both:
-- - PostgreSQL 17: Uses pg_uuidv7 extension
-- - PostgreSQL 18+: Uses native uuidv7() function with compatibility alias

DO $$
BEGIN
  -- Check if pg_uuidv7 extension is available (PostgreSQL 17)
  IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_uuidv7') THEN
    CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;
  ELSE
    -- PostgreSQL 18+: create alias for native uuidv7()
    -- VOLATILE is required because UUID generation must return a new value on each call
    CREATE OR REPLACE FUNCTION tasker.uuid_generate_v7() RETURNS uuid
    AS $fn$ SELECT uuidv7(); $fn$ LANGUAGE SQL VOLATILE PARALLEL SAFE;
  END IF;
END $$;

-- =============================================================================
-- ENUM TYPES
-- =============================================================================

CREATE TYPE tasker.dlq_reason AS ENUM (
    'staleness_timeout',
    'max_retries_exceeded',
    'dependency_cycle_detected',
    'worker_unavailable',
    'manual_dlq'
);

CREATE TYPE tasker.dlq_resolution_status AS ENUM (
    'pending',
    'manually_resolved',
    'permanently_failed',
    'cancelled'
);

-- =============================================================================
-- TABLES
-- =============================================================================
-- Note: Foreign key constraints are added in the next migration to ensure
-- proper table creation order.
--
-- TAS-136: Removed legacy tables (annotation_types, task_annotations,
-- dependent_systems, dependent_system_object_maps) and deprecated columns
-- (bypass_steps, skippable, dependent_system_uuid). See docs/architecture/schema-design.md.

--
-- Name: named_steps; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.named_steps (
    named_step_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    name character varying(128) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: named_tasks; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.named_tasks (
    named_task_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    task_namespace_uuid uuid NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    version character varying(16) DEFAULT '0.1.0'::character varying NOT NULL,
    configuration jsonb DEFAULT '{}'::jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: named_tasks_named_steps; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.named_tasks_named_steps (
    ntns_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    named_task_uuid uuid NOT NULL,
    named_step_uuid uuid NOT NULL,
    default_retryable boolean DEFAULT true NOT NULL,
    default_max_attempts integer DEFAULT 3 NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: workflow_step_edges; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_step_edges (
    workflow_step_edge_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    from_step_uuid uuid NOT NULL,
    to_step_uuid uuid NOT NULL,
    name character varying NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: workflow_steps; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_steps (
    workflow_step_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    named_step_uuid uuid NOT NULL,
    retryable boolean DEFAULT true NOT NULL,
    max_attempts integer DEFAULT 3,
    in_process boolean DEFAULT false NOT NULL,
    processed boolean DEFAULT false NOT NULL,
    processed_at timestamp without time zone,
    attempts integer,
    last_attempted_at timestamp without time zone,
    backoff_request_seconds integer,
    inputs jsonb,
    results jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    checkpoint jsonb
);


--
-- Name: COLUMN workflow_steps.checkpoint; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_steps.checkpoint IS 'Handler-driven checkpoint data for batch processing resumability (TAS-125)';


--
-- Name: step_dag_relationships; Type: VIEW; Schema: tasker; Owner: -
--

CREATE VIEW tasker.step_dag_relationships AS
 SELECT ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    COALESCE(parent_data.parent_uuids, '[]'::jsonb) AS parent_step_uuids,
    COALESCE(child_data.child_uuids, '[]'::jsonb) AS child_step_uuids,
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
   FROM (((tasker.workflow_steps ws
     LEFT JOIN ( SELECT workflow_step_edges.to_step_uuid,
            jsonb_agg(workflow_step_edges.from_step_uuid ORDER BY workflow_step_edges.from_step_uuid) AS parent_uuids,
            count(*) AS parent_count
           FROM tasker.workflow_step_edges
          GROUP BY workflow_step_edges.to_step_uuid) parent_data ON ((parent_data.to_step_uuid = ws.workflow_step_uuid)))
     LEFT JOIN ( SELECT workflow_step_edges.from_step_uuid,
            jsonb_agg(workflow_step_edges.to_step_uuid ORDER BY workflow_step_edges.to_step_uuid) AS child_uuids,
            count(*) AS child_count
           FROM tasker.workflow_step_edges
          GROUP BY workflow_step_edges.from_step_uuid) child_data ON ((child_data.from_step_uuid = ws.workflow_step_uuid)))
     LEFT JOIN ( WITH RECURSIVE step_depths AS (
                 SELECT ws_inner.workflow_step_uuid,
                    0 AS depth_from_root,
                    ws_inner.task_uuid
                   FROM tasker.workflow_steps ws_inner
                  WHERE (NOT (EXISTS ( SELECT 1
                           FROM tasker.workflow_step_edges e
                          WHERE (e.to_step_uuid = ws_inner.workflow_step_uuid))))
                UNION ALL
                 SELECT e.to_step_uuid,
                    (sd.depth_from_root + 1),
                    sd.task_uuid
                   FROM (step_depths sd
                     JOIN tasker.workflow_step_edges e ON ((e.from_step_uuid = sd.workflow_step_uuid)))
                  WHERE (sd.depth_from_root < 50)
                )
         SELECT step_depths.workflow_step_uuid,
            min(step_depths.depth_from_root) AS min_depth_from_root
           FROM step_depths
          GROUP BY step_depths.workflow_step_uuid) depth_info ON ((depth_info.workflow_step_uuid = ws.workflow_step_uuid)));


--
-- Name: task_namespaces; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.task_namespaces (
    task_namespace_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);


--
-- Name: task_transitions; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.task_transitions (
    task_transition_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    processor_uuid uuid,
    transition_metadata jsonb DEFAULT '{}'::jsonb,
    CONSTRAINT chk_task_transitions_from_state CHECK (((from_state IS NULL) OR ((from_state)::text = ANY ((ARRAY['pending'::character varying, 'initializing'::character varying, 'enqueuing_steps'::character varying, 'steps_in_process'::character varying, 'evaluating_results'::character varying, 'waiting_for_dependencies'::character varying, 'waiting_for_retry'::character varying, 'blocked_by_failures'::character varying, 'complete'::character varying, 'error'::character varying, 'cancelled'::character varying, 'resolved_manually'::character varying])::text[])))),
    CONSTRAINT chk_task_transitions_to_state CHECK (((to_state)::text = ANY ((ARRAY['pending'::character varying, 'initializing'::character varying, 'enqueuing_steps'::character varying, 'steps_in_process'::character varying, 'evaluating_results'::character varying, 'waiting_for_dependencies'::character varying, 'waiting_for_retry'::character varying, 'blocked_by_failures'::character varying, 'complete'::character varying, 'error'::character varying, 'cancelled'::character varying, 'resolved_manually'::character varying])::text[])))
);


--
-- Name: tasks; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.tasks (
    task_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    named_task_uuid uuid NOT NULL,
    complete boolean DEFAULT false NOT NULL,
    requested_at timestamp without time zone NOT NULL,
    completed_at timestamp without time zone,
    initiator character varying(128),
    source_system character varying(128),
    reason character varying(128),
    tags jsonb,
    context jsonb,
    identity_hash character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    correlation_id uuid NOT NULL,
    parent_correlation_id uuid
);


--
-- Name: COLUMN tasks.correlation_id; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks.correlation_id IS 'UUID v7 correlation ID for distributed tracing (NOT NULL). Auto-generated if not provided at task creation. Enables end-to-end request tracking across orchestration and workers.';


--
-- Name: COLUMN tasks.parent_correlation_id; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks.parent_correlation_id IS 'Optional correlation ID of parent workflow. Enables tracking of nested/chained workflow relationships.';


--
-- Name: tasks_dlq; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.tasks_dlq (
    dlq_entry_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    original_state character varying(50) NOT NULL,
    dlq_reason tasker.dlq_reason NOT NULL,
    dlq_timestamp timestamp without time zone DEFAULT now() NOT NULL,
    resolution_status tasker.dlq_resolution_status DEFAULT 'pending'::tasker.dlq_resolution_status NOT NULL,
    resolution_timestamp timestamp without time zone,
    resolution_notes text,
    resolved_by character varying(255),
    task_snapshot jsonb NOT NULL,
    metadata jsonb,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE tasks_dlq; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON TABLE tasker.tasks_dlq IS 'TAS-49: Dead Letter Queue investigation tracking and audit trail.

Architecture: DLQ is an INVESTIGATION TRACKER, not a task manipulation layer
- Tasks remain in tasks table (typically in Error state)
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


--
-- Name: COLUMN tasks_dlq.task_snapshot; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks_dlq.task_snapshot IS 'Complete snapshot of task state when moved to DLQ.
Includes task details, steps, transitions, and execution context.
Enables investigation without modifying main task tables.';


--
-- Name: COLUMN tasks_dlq.metadata; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.tasks_dlq.metadata IS 'Additional context about DLQ entry.
Examples: error stack trace, detection method, related tasks, investigation notes.';


--
-- Name: workflow_step_result_audit; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_step_result_audit (
    workflow_step_result_audit_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    workflow_step_uuid uuid NOT NULL,
    workflow_step_transition_uuid uuid NOT NULL,
    task_uuid uuid NOT NULL,
    recorded_at timestamp(6) without time zone DEFAULT now() NOT NULL,
    worker_uuid uuid,
    correlation_id uuid,
    success boolean NOT NULL,
    execution_time_ms bigint,
    created_at timestamp(6) without time zone DEFAULT now() NOT NULL,
    updated_at timestamp(6) without time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE workflow_step_result_audit; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON TABLE tasker.workflow_step_result_audit IS 'Lightweight audit trail for workflow step execution results. Stores references and attribution only - full results accessed via JOIN to transitions table. Created by TAS-62.';


--
-- Name: COLUMN workflow_step_result_audit.worker_uuid; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.worker_uuid IS 'UUID of the worker that processed this step (attribution for SOC2 compliance)';


--
-- Name: COLUMN workflow_step_result_audit.correlation_id; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.correlation_id IS 'Correlation ID for distributed tracing across the orchestration system';


--
-- Name: COLUMN workflow_step_result_audit.success; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.success IS 'Whether the step execution succeeded (extracted from transition for indexing)';


--
-- Name: COLUMN workflow_step_result_audit.execution_time_ms; Type: COMMENT; Schema: tasker; Owner: -
--

COMMENT ON COLUMN tasker.workflow_step_result_audit.execution_time_ms IS 'Step execution time in milliseconds (extracted from transition for filtering)';


--
-- Name: workflow_step_transitions; Type: TABLE; Schema: tasker; Owner: -
--

CREATE TABLE tasker.workflow_step_transitions (
    workflow_step_transition_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    workflow_step_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    CONSTRAINT chk_workflow_step_transitions_from_state CHECK (((from_state IS NULL) OR ((from_state)::text = ANY ((ARRAY['pending'::character varying, 'enqueued'::character varying, 'in_progress'::character varying, 'enqueued_for_orchestration'::character varying, 'enqueued_as_error_for_orchestration'::character varying, 'waiting_for_retry'::character varying, 'complete'::character varying, 'error'::character varying, 'cancelled'::character varying, 'resolved_manually'::character varying])::text[])))),
    CONSTRAINT chk_workflow_step_transitions_to_state CHECK (((to_state)::text = ANY ((ARRAY['pending'::character varying, 'enqueued'::character varying, 'in_progress'::character varying, 'enqueued_for_orchestration'::character varying, 'enqueued_as_error_for_orchestration'::character varying, 'waiting_for_retry'::character varying, 'complete'::character varying, 'error'::character varying, 'cancelled'::character varying, 'resolved_manually'::character varying])::text[])))
);


--
-- Name: v_dlq_dashboard; Type: VIEW; Schema: tasker; Owner: -

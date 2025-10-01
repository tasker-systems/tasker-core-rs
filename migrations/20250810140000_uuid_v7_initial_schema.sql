-- UUID v7 Initial Schema Migration for Tasker Core Rust
--
-- This migration creates the complete schema with UUID v7 primary keys
-- for the 8 primary tables identified in TAS-33:
--
-- PRIMARY SCOPE (4 tables):
-- - tasker_tasks: Main workflow instances
-- - tasker_workflow_steps: Individual workflow steps
-- - tasker_task_transitions: Task state change history
-- - tasker_workflow_step_transitions: Step state change history
--
-- REGISTRY SCOPE (4 tables):
-- - tasker_task_namespaces: Workflow namespace definitions
-- - tasker_named_tasks: Task type definitions
-- - tasker_named_steps: Step type definitions
-- - tasker_dependent_systems: External system integrations
--
-- BENEFITS:
-- - Time-ordered UUIDs maintain chronological sorting like BIGINT
-- - Distributed-system friendly identifiers prevent ID collisions
-- - 128-bit keyspace eliminates overflow concerns
-- - Better performance than UUID v4 due to time-ordering
-- - Improved scalability for multi-tenant and distributed deployments

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;
CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;

--
-- PGMQ Headers
--

-- Function to add headers column to a pgmq queue table if it doesn't exist
CREATE OR REPLACE FUNCTION pgmq_ensure_headers_column(queue_name TEXT)
RETURNS VOID AS $$
DECLARE
    full_table_name TEXT;
    column_exists BOOLEAN;
BEGIN
    -- Construct full table name (pgmq queues are prefixed with 'q_')
    full_table_name := 'q_' || queue_name;

    -- Check if headers column exists
    SELECT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'pgmq'
        AND table_name = full_table_name
        AND column_name = 'headers'
    ) INTO column_exists;

    -- Add headers column if it doesn't exist
    IF NOT column_exists THEN
        EXECUTE format('ALTER TABLE pgmq.%I ADD COLUMN headers JSONB', full_table_name);
        RAISE NOTICE 'Added headers column to pgmq.%', full_table_name;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Add headers column to existing queue tables that might be missing it
-- This handles any queues created by the Rust pgmq-rs crate

DO $$
DECLARE
    queue_record RECORD;
    table_name TEXT;
BEGIN
    -- Find all pgmq queue tables and ensure they have headers column
    FOR queue_record IN
        SELECT schemaname, tablename
        FROM pg_tables
        WHERE schemaname = 'pgmq'
        AND tablename LIKE 'q_%'
    LOOP
        -- Extract queue name by removing 'q_' prefix
        SELECT substring(queue_record.tablename from 3) INTO table_name;

        -- Use our function to ensure headers column exists
        PERFORM pgmq_ensure_headers_column(table_name);
    END LOOP;
END;
$$;

-- Create a trigger function to automatically add headers column to new queue tables
-- This ensures future queue tables created by pgmq-rs will be compatible
CREATE OR REPLACE FUNCTION pgmq_auto_add_headers_trigger()
RETURNS event_trigger AS $$
DECLARE
    obj RECORD;
    queue_name TEXT;
BEGIN
    -- Only process CREATE TABLE events in pgmq schema
    FOR obj IN SELECT * FROM pg_event_trigger_ddl_commands()
    WHERE command_tag = 'CREATE TABLE' AND schema_name = 'pgmq'
    LOOP
        -- Extract queue name from table name (remove 'q_' prefix)
        IF obj.object_identity LIKE 'pgmq.q_%' THEN
            queue_name := substring(obj.object_identity from 8);  -- Remove 'pgmq.q_'

            -- Ensure the new table has headers column
            PERFORM pgmq_ensure_headers_column(queue_name);
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Create the event trigger to automatically add headers to new pgmq tables
DROP EVENT TRIGGER IF EXISTS pgmq_headers_compatibility_trigger;
CREATE EVENT TRIGGER pgmq_headers_compatibility_trigger
    ON ddl_command_end
    WHEN TAG IN ('CREATE TABLE')
    EXECUTE FUNCTION pgmq_auto_add_headers_trigger();

COMMENT ON FUNCTION pgmq_ensure_headers_column(TEXT) IS
'Ensures a pgmq queue table has the headers JSONB column required by pgmq extension v1.5.1+';

COMMENT ON FUNCTION pgmq_auto_add_headers_trigger() IS
'Event trigger function to automatically add headers column to new pgmq queue tables';

COMMENT ON EVENT TRIGGER pgmq_headers_compatibility_trigger IS
'Automatically adds headers column to new pgmq queue tables for pgmq-rs compatibility';

-- =============================================================================
-- HELPER FUNCTIONS
-- =============================================================================

--
-- Name: calculate_dependency_levels(uuid); Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.calculate_dependency_levels(input_task_uuid uuid)
RETURNS TABLE(workflow_step_uuid uuid, dependency_level integer)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH RECURSIVE dependency_levels AS (
    -- Base case: Find root nodes (steps with no dependencies)
    SELECT
      ws.workflow_step_uuid,
      0 as level
    FROM tasker_workflow_steps ws
    WHERE ws.task_uuid = input_task_uuid
      AND NOT EXISTS (
        SELECT 1
        FROM tasker_workflow_step_edges wse
        WHERE wse.to_step_uuid = ws.workflow_step_uuid
      )

    UNION ALL

    -- Recursive case: Find children of current level nodes
    SELECT
      wse.to_step_uuid as workflow_step_uuid,
      dl.level + 1 as level
    FROM dependency_levels dl
    JOIN tasker_workflow_step_edges wse ON wse.from_step_uuid = dl.workflow_step_uuid
    JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.to_step_uuid
    WHERE ws.task_uuid = input_task_uuid
  )
  SELECT
    dl.workflow_step_uuid,
    MAX(dl.level) as dependency_level  -- Use MAX to handle multiple paths to same node
  FROM dependency_levels dl
  GROUP BY dl.workflow_step_uuid
  ORDER BY dependency_level, workflow_step_uuid;
END;
$$;

--
-- Name: get_analytics_metrics(timestamp with time zone); Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_analytics_metrics(
    since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone
)
RETURNS TABLE(
    active_tasks_count bigint,
    total_namespaces_count bigint,
    unique_task_types_count bigint,
    system_health_score numeric,
    task_throughput bigint,
    completion_count bigint,
    error_count bigint,
    completion_rate numeric,
    error_rate numeric,
    avg_task_duration numeric,
    avg_step_duration numeric,
    step_throughput bigint,
    analysis_period_start timestamp with time zone,
    calculated_at timestamp with time zone
)
LANGUAGE plpgsql STABLE AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    -- Set analysis start time (default to 1 hour ago if not provided)
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '1 hour');

    RETURN QUERY
    WITH active_tasks AS (
        SELECT COUNT(DISTINCT t.task_uuid) as active_count
        FROM tasker_tasks t
        INNER JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE wst.most_recent = true
          AND wst.to_state NOT IN ('complete', 'error', 'skipped', 'resolved_manually')
    ),
    namespace_summary AS (
        SELECT COUNT(DISTINCT tn.name) as namespace_count
        FROM tasker_task_namespaces tn
        INNER JOIN tasker_named_tasks nt ON nt.task_namespace_uuid = tn.task_namespace_uuid
        INNER JOIN tasker_tasks t ON t.named_task_uuid = nt.named_task_uuid
    ),
    task_type_summary AS (
        SELECT COUNT(DISTINCT nt.name) as task_type_count
        FROM tasker_named_tasks nt
        INNER JOIN tasker_tasks t ON t.named_task_uuid = nt.named_task_uuid
    ),
    recent_task_health AS (
        SELECT
            COUNT(DISTINCT t.task_uuid) as total_recent_tasks,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE wst.to_state = 'complete' AND wst.most_recent = true
            ) as completed_tasks,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE wst.to_state = 'error' AND wst.most_recent = true
            ) as error_tasks
        FROM tasker_tasks t
        INNER JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE t.created_at > NOW() - INTERVAL '1 hour'
    ),
    period_metrics AS (
        SELECT
            COUNT(DISTINCT t.task_uuid) as throughput,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
            ) as completions,
            COUNT(DISTINCT t.task_uuid) FILTER (
                WHERE error_wst.to_state = 'error' AND error_wst.most_recent = true
            ) as errors,
            COUNT(DISTINCT ws.workflow_step_uuid) as step_count,
            AVG(
                CASE
                    WHEN completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
                    THEN EXTRACT(EPOCH FROM (completed_wst.created_at - t.created_at))
                    ELSE NULL
                END
            ) as avg_task_seconds,
            AVG(
                CASE
                    WHEN step_completed.to_state = 'complete' AND step_completed.most_recent = true
                    THEN EXTRACT(EPOCH FROM (step_completed.created_at - ws.created_at))
                    ELSE NULL
                END
            ) as avg_step_seconds
        FROM tasker_tasks t
        LEFT JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        LEFT JOIN tasker_workflow_step_transitions completed_wst ON completed_wst.workflow_step_uuid = ws.workflow_step_uuid
            AND completed_wst.to_state = 'complete' AND completed_wst.most_recent = true
        LEFT JOIN tasker_workflow_step_transitions error_wst ON error_wst.workflow_step_uuid = ws.workflow_step_uuid
            AND error_wst.to_state = 'error' AND error_wst.most_recent = true
        LEFT JOIN tasker_workflow_step_transitions step_completed ON step_completed.workflow_step_uuid = ws.workflow_step_uuid
            AND step_completed.to_state = 'complete' AND step_completed.most_recent = true
        WHERE t.created_at > analysis_start
    )
    SELECT
        at.active_count,
        ns.namespace_count,
        tts.task_type_count,
        CASE
            WHEN (rth.completed_tasks + rth.error_tasks) > 0
            THEN ROUND((rth.completed_tasks::NUMERIC / (rth.completed_tasks + rth.error_tasks)), 3)
            ELSE 1.0
        END as health_score,
        pm.throughput,
        pm.completions,
        pm.errors,
        CASE
            WHEN pm.throughput > 0
            THEN ROUND((pm.completions::NUMERIC / pm.throughput * 100), 2)
            ELSE 0.0
        END as completion_rate_pct,
        CASE
            WHEN pm.throughput > 0
            THEN ROUND((pm.errors::NUMERIC / pm.throughput * 100), 2)
            ELSE 0.0
        END as error_rate_pct,
        ROUND(COALESCE(pm.avg_task_seconds, 0), 3),
        ROUND(COALESCE(pm.avg_step_seconds, 0), 3),
        pm.step_count,
        analysis_start,
        NOW()
    FROM active_tasks at
    CROSS JOIN namespace_summary ns
    CROSS JOIN task_type_summary tts
    CROSS JOIN recent_task_health rth
    CROSS JOIN period_metrics pm;
END;
$$;

-- =============================================================================
-- REGISTRY TABLES (UUID v7 Primary Keys)
-- =============================================================================

--
-- Name: tasker_task_namespaces; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_task_namespaces (
    task_namespace_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

--
-- Name: tasker_named_tasks; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_named_tasks (
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
-- Name: tasker_dependent_systems; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_dependent_systems (
    dependent_system_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

--
-- Name: tasker_named_steps; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_named_steps (
    named_step_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    dependent_system_uuid uuid NOT NULL,
    name character varying(128) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

-- =============================================================================
-- PRIMARY TABLES (UUID v7 Primary Keys)
-- =============================================================================

--
-- Name: tasker_tasks; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_tasks (
    task_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    named_task_uuid uuid NOT NULL,
    complete boolean DEFAULT false NOT NULL,
    requested_at timestamp without time zone NOT NULL,
    completed_at timestamp without time zone,
    initiator character varying(128),
    source_system character varying(128),
    reason character varying(128),
    bypass_steps json,
    tags jsonb,
    context jsonb,
    identity_hash character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    priority integer DEFAULT 0 NOT NULL
);

--
-- Name: tasker_workflow_steps; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_workflow_steps (
    workflow_step_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    named_step_uuid uuid NOT NULL,
    retryable boolean DEFAULT true NOT NULL,
    retry_limit integer DEFAULT 3,
    in_process boolean DEFAULT false NOT NULL,
    processed boolean DEFAULT false NOT NULL,
    processed_at timestamp without time zone,
    attempts integer,
    last_attempted_at timestamp without time zone,
    backoff_request_seconds integer,
    inputs jsonb,
    results jsonb,
    skippable boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    priority integer DEFAULT 0 NOT NULL
);

--
-- Name: tasker_task_transitions; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_task_transitions (
    task_transition_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    task_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

--
-- Name: tasker_workflow_step_transitions; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_workflow_step_transitions (
    workflow_step_transition_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    workflow_step_uuid uuid NOT NULL,
    to_state character varying NOT NULL,
    from_state character varying,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

-- =============================================================================
-- SUPPORTING TABLES (Keep existing integer PKs - no UUID migration needed)
-- =============================================================================

--
-- Name: tasker_annotation_types; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_annotation_types (
    annotation_type_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

CREATE SEQUENCE IF NOT EXISTS public.tasker_annotation_types_annotation_type_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.tasker_annotation_types_annotation_type_id_seq OWNED BY public.tasker_annotation_types.annotation_type_id;
ALTER TABLE ONLY public.tasker_annotation_types ALTER COLUMN annotation_type_id SET DEFAULT nextval('public.tasker_annotation_types_annotation_type_id_seq'::regclass);

--
-- Name: tasker_task_annotations; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_task_annotations (
    task_annotation_id bigint NOT NULL,
    task_uuid uuid NOT NULL,  -- Foreign key to UUID table
    annotation_type_id integer NOT NULL,
    annotation jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

CREATE SEQUENCE IF NOT EXISTS public.tasker_task_annotations_task_annotation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.tasker_task_annotations_task_annotation_id_seq OWNED BY public.tasker_task_annotations.task_annotation_id;
ALTER TABLE ONLY public.tasker_task_annotations ALTER COLUMN task_annotation_id SET DEFAULT nextval('public.tasker_task_annotations_task_annotation_id_seq'::regclass);

--
-- Name: tasker_workflow_step_edges; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_workflow_step_edges (
    workflow_step_edge_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,  -- UUID PK for edges
    from_step_uuid uuid NOT NULL,  -- Foreign key to UUID table
    to_step_uuid uuid NOT NULL,    -- Foreign key to UUID table
    name character varying NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

--
-- Name: tasker_named_tasks_named_steps; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_named_tasks_named_steps (
    ntns_uuid uuid DEFAULT uuid_generate_v7() NOT NULL,
    named_task_uuid uuid NOT NULL,  -- Foreign key to UUID table
    named_step_uuid uuid NOT NULL,  -- Foreign key to UUID table
    skippable boolean DEFAULT false NOT NULL,
    default_retryable boolean DEFAULT true NOT NULL,
    default_retry_limit integer DEFAULT 3 NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

--
-- Name: tasker_dependent_system_object_maps; Type: TABLE; Schema: public; Owner: -
--
CREATE TABLE IF NOT EXISTS public.tasker_dependent_system_object_maps (
    dependent_system_object_map_id bigint NOT NULL,
    dependent_system_one_uuid uuid NOT NULL,  -- Foreign key to UUID table
    dependent_system_two_uuid uuid NOT NULL,  -- Foreign key to UUID table
    remote_id_one character varying(128) NOT NULL,
    remote_id_two character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);

CREATE SEQUENCE IF NOT EXISTS public.tasker_dependent_system_objec_dependent_system_object_map_i_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.tasker_dependent_system_objec_dependent_system_object_map_i_seq OWNED BY public.tasker_dependent_system_object_maps.dependent_system_object_map_id;
ALTER TABLE ONLY public.tasker_dependent_system_object_maps ALTER COLUMN dependent_system_object_map_id SET DEFAULT nextval('public.tasker_dependent_system_objec_dependent_system_object_map_i_seq'::regclass);

-- =============================================================================
-- PRIMARY KEY CONSTRAINTS
-- =============================================================================

-- Registry tables (UUID v7 PKs)
ALTER TABLE ONLY public.tasker_task_namespaces
    ADD CONSTRAINT tasker_task_namespaces_pkey PRIMARY KEY (task_namespace_uuid);

ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_pkey PRIMARY KEY (named_task_uuid);

ALTER TABLE ONLY public.tasker_dependent_systems
    ADD CONSTRAINT tasker_dependent_systems_pkey PRIMARY KEY (dependent_system_uuid);

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT tasker_named_steps_pkey PRIMARY KEY (named_step_uuid);

-- Primary tables (UUID v7 PKs)
ALTER TABLE ONLY public.tasker_tasks
    ADD CONSTRAINT tasker_tasks_pkey PRIMARY KEY (task_uuid);

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT tasker_workflow_steps_pkey PRIMARY KEY (workflow_step_uuid);

ALTER TABLE ONLY public.tasker_task_transitions
    ADD CONSTRAINT tasker_task_transitions_pkey PRIMARY KEY (task_transition_uuid);

ALTER TABLE ONLY public.tasker_workflow_step_transitions
    ADD CONSTRAINT tasker_workflow_step_transitions_pkey PRIMARY KEY (workflow_step_transition_uuid);

-- Supporting tables (mixed PKs)
ALTER TABLE ONLY public.tasker_annotation_types
    ADD CONSTRAINT tasker_annotation_types_pkey PRIMARY KEY (annotation_type_id);

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT tasker_task_annotations_pkey PRIMARY KEY (task_annotation_id);

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT tasker_workflow_step_edges_pkey PRIMARY KEY (workflow_step_edge_uuid);

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_pkey PRIMARY KEY (ntns_uuid);

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT tasker_dependent_system_object_maps_pkey PRIMARY KEY (dependent_system_object_map_id);

-- =============================================================================
-- UNIQUE CONSTRAINTS
-- =============================================================================

-- Registry uniqueness
ALTER TABLE ONLY public.tasker_task_namespaces
    ADD CONSTRAINT tasker_task_namespaces_name_key UNIQUE (name);

ALTER TABLE ONLY public.tasker_dependent_systems
    ADD CONSTRAINT tasker_dependent_systems_name_key UNIQUE (name);

ALTER TABLE ONLY public.tasker_annotation_types
    ADD CONSTRAINT tasker_annotation_types_name_key UNIQUE (name);

-- Composite uniqueness
ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_namespace_name_unique UNIQUE (task_namespace_uuid, name);

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT tasker_named_steps_system_name_unique UNIQUE (dependent_system_uuid, name);

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_unique UNIQUE (named_task_uuid, named_step_uuid);

-- =============================================================================
-- FOREIGN KEY CONSTRAINTS
-- =============================================================================

-- Registry foreign keys
ALTER TABLE ONLY public.tasker_named_tasks
    ADD CONSTRAINT tasker_named_tasks_task_namespace_uuid_fkey
    FOREIGN KEY (task_namespace_uuid) REFERENCES public.tasker_task_namespaces(task_namespace_uuid);

ALTER TABLE ONLY public.tasker_named_steps
    ADD CONSTRAINT tasker_named_steps_dependent_system_uuid_fkey
    FOREIGN KEY (dependent_system_uuid) REFERENCES public.tasker_dependent_systems(dependent_system_uuid);

-- Primary table foreign keys
ALTER TABLE ONLY public.tasker_tasks
    ADD CONSTRAINT tasker_tasks_named_task_uuid_fkey
    FOREIGN KEY (named_task_uuid) REFERENCES public.tasker_named_tasks(named_task_uuid);

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT tasker_workflow_steps_task_uuid_fkey
    FOREIGN KEY (task_uuid) REFERENCES public.tasker_tasks(task_uuid);

ALTER TABLE ONLY public.tasker_workflow_steps
    ADD CONSTRAINT tasker_workflow_steps_named_step_uuid_fkey
    FOREIGN KEY (named_step_uuid) REFERENCES public.tasker_named_steps(named_step_uuid);

ALTER TABLE ONLY public.tasker_task_transitions
    ADD CONSTRAINT tasker_task_transitions_task_uuid_fkey
    FOREIGN KEY (task_uuid) REFERENCES public.tasker_tasks(task_uuid);

ALTER TABLE ONLY public.tasker_workflow_step_transitions
    ADD CONSTRAINT tasker_workflow_step_transitions_workflow_step_uuid_fkey
    FOREIGN KEY (workflow_step_uuid) REFERENCES public.tasker_workflow_steps(workflow_step_uuid);

-- Supporting table foreign keys
ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT tasker_task_annotations_task_uuid_fkey
    FOREIGN KEY (task_uuid) REFERENCES public.tasker_tasks(task_uuid);

ALTER TABLE ONLY public.tasker_task_annotations
    ADD CONSTRAINT tasker_task_annotations_annotation_type_id_fkey
    FOREIGN KEY (annotation_type_id) REFERENCES public.tasker_annotation_types(annotation_type_id);

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT tasker_workflow_step_edges_from_step_uuid_fkey
    FOREIGN KEY (from_step_uuid) REFERENCES public.tasker_workflow_steps(workflow_step_uuid);

ALTER TABLE ONLY public.tasker_workflow_step_edges
    ADD CONSTRAINT tasker_workflow_step_edges_to_step_uuid_fkey
    FOREIGN KEY (to_step_uuid) REFERENCES public.tasker_workflow_steps(workflow_step_uuid);

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_named_task_uuid_fkey
    FOREIGN KEY (named_task_uuid) REFERENCES public.tasker_named_tasks(named_task_uuid);

ALTER TABLE ONLY public.tasker_named_tasks_named_steps
    ADD CONSTRAINT tasker_named_tasks_named_steps_named_step_uuid_fkey
    FOREIGN KEY (named_step_uuid) REFERENCES public.tasker_named_steps(named_step_uuid);

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT tasker_dependent_system_object_maps_dependent_system_one_uuid_fkey
    FOREIGN KEY (dependent_system_one_uuid) REFERENCES public.tasker_dependent_systems(dependent_system_uuid);

ALTER TABLE ONLY public.tasker_dependent_system_object_maps
    ADD CONSTRAINT tasker_dependent_system_object_maps_dependent_system_two_uuid_fkey
    FOREIGN KEY (dependent_system_two_uuid) REFERENCES public.tasker_dependent_systems(dependent_system_uuid);

-- =============================================================================
-- INDEXES (Optimized for UUID v7 time-ordering and frequent queries)
-- =============================================================================

-- Primary UUID indexes (automatically created by PK constraints)

-- Time-based query indexes (leverage UUID v7 time-ordering)
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_requested_at ON public.tasker_tasks USING btree (requested_at);
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_completed_at ON public.tasker_tasks USING btree (completed_at);
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_complete ON public.tasker_tasks USING btree (complete);
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_priority ON public.tasker_tasks USING btree (priority);

-- Additional task indexes for original functionality
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_source_system ON public.tasker_tasks USING btree (source_system);
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_tags_gin ON public.tasker_tasks USING gin (tags);
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_tags_gin_path ON public.tasker_tasks USING gin (tags jsonb_path_ops);
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_identity_hash ON public.tasker_tasks USING btree (identity_hash);

-- Foreign key indexes for efficient joins
CREATE INDEX IF NOT EXISTS idx_tasker_named_tasks_namespace_uuid ON public.tasker_named_tasks USING btree (task_namespace_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_named_steps_system_uuid ON public.tasker_named_steps USING btree (dependent_system_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_named_task_uuid ON public.tasker_tasks USING btree (named_task_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_workflow_steps_task_uuid ON public.tasker_workflow_steps USING btree (task_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_workflow_steps_named_step_uuid ON public.tasker_workflow_steps USING btree (named_step_uuid);

-- State transition indexes for orchestration queries
CREATE INDEX IF NOT EXISTS idx_tasker_task_transitions_task_uuid ON public.tasker_task_transitions USING btree (task_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_task_transitions_most_recent ON public.tasker_task_transitions USING btree (most_recent) WHERE most_recent = true;
CREATE INDEX IF NOT EXISTS idx_tasker_workflow_step_transitions_step_uuid ON public.tasker_workflow_step_transitions USING btree (workflow_step_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_workflow_step_transitions_most_recent ON public.tasker_workflow_step_transitions USING btree (most_recent) WHERE most_recent = true;

-- Workflow dependency indexes
CREATE INDEX IF NOT EXISTS idx_tasker_workflow_step_edges_from_step ON public.tasker_workflow_step_edges USING btree (from_step_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_workflow_step_edges_to_step ON public.tasker_workflow_step_edges USING btree (to_step_uuid);

-- Critical workflow step processing indexes
CREATE INDEX IF NOT EXISTS idx_workflow_steps_active_operations ON public.tasker_workflow_steps USING btree (workflow_step_uuid, task_uuid) WHERE ((processed = false) OR (processed IS NULL));
CREATE INDEX IF NOT EXISTS idx_workflow_steps_task_grouping_active ON public.tasker_workflow_steps USING btree (task_uuid, workflow_step_uuid) WHERE ((processed = false) OR (processed IS NULL));
CREATE INDEX IF NOT EXISTS idx_workflow_steps_task_readiness ON public.tasker_workflow_steps USING btree (task_uuid, processed, workflow_step_uuid) WHERE (processed = false);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_processing_status ON public.tasker_workflow_steps USING btree (task_uuid, processed, in_process);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_retry_logic ON public.tasker_workflow_steps USING btree (attempts, retry_limit, retryable);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_retry_status ON public.tasker_workflow_steps USING btree (attempts, retry_limit);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_task_covering ON public.tasker_workflow_steps USING btree (task_uuid) INCLUDE (workflow_step_uuid, processed, in_process, attempts, retry_limit);
CREATE INDEX IF NOT EXISTS idx_workflow_steps_processed_at ON public.tasker_workflow_steps USING btree (processed_at);

-- Transitive dependency optimization index
CREATE INDEX IF NOT EXISTS idx_workflow_steps_transitive_deps ON public.tasker_workflow_steps USING btree (workflow_step_uuid, named_step_uuid) INCLUDE (task_uuid, results, processed);

-- Task orchestration indexes
CREATE INDEX IF NOT EXISTS idx_tasker_task_annotations_task_uuid ON public.tasker_task_annotations USING btree (task_uuid);
CREATE INDEX IF NOT EXISTS idx_tasker_task_annotations_type_id ON public.tasker_task_annotations USING btree (annotation_type_id);

-- Registry name lookup indexes
CREATE INDEX IF NOT EXISTS idx_tasker_task_namespaces_name ON public.tasker_task_namespaces USING btree (name);
CREATE INDEX IF NOT EXISTS idx_tasker_dependent_systems_name ON public.tasker_dependent_systems USING btree (name);

-- =============================================================================
-- CRITICAL FUNCTIONS (Updated for UUID v7)
-- =============================================================================

--
-- Name: get_step_readiness_status_uuid; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_step_readiness_status(
    input_task_uuid uuid,
    step_uuids uuid[] DEFAULT NULL::uuid[]
)
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
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
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM tasker_workflow_step_transitions wst
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),
  dependency_counts AS (
    SELECT
      wse.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_state.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM tasker_workflow_step_edges wse
    LEFT JOIN step_states parent_state ON parent_state.workflow_step_uuid = wse.from_step_uuid
    WHERE wse.to_step_uuid IN (
      SELECT ws.workflow_step_uuid
      FROM tasker_workflow_steps ws
      WHERE ws.task_uuid = input_task_uuid
    )
    GROUP BY wse.to_step_uuid
  ),
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM tasker_workflow_step_transitions wst
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  )
  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied
    CASE
      WHEN dc.total_deps IS NULL THEN true  -- No dependencies
      WHEN dc.completed_deps = dc.total_deps THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Retry eligibility
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Ready for execution
    CASE
      WHEN COALESCE(ss.to_state, 'pending') = 'pending'
      AND (ws.processed = false OR ws.processed IS NULL)
      AND (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps)
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))
      AND (COALESCE(ws.retryable, true) = true)
      AND (ws.in_process = false OR ws.in_process IS NULL)
      AND (
        -- Check explicit backoff timing (most restrictive)
        -- If backoff is set, the backoff period must have expired
        CASE
          WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
            ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') <= NOW()
          ELSE true  -- No explicit backoff set
        END
        AND
        -- Then check failure-based backoff
        (lf.failure_time IS NULL OR
         lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    lf.failure_time as last_failure_at,

    -- Next retry calculation
    CASE
      WHEN ws.last_attempted_at IS NOT NULL AND ws.backoff_request_seconds IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN lf.failure_time IS NOT NULL THEN
        lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = input_task_uuid
    AND (step_uuids IS NULL OR ws.workflow_step_uuid = ANY(step_uuids))
  ORDER BY ws.workflow_step_uuid;
END;
$$;

--
-- Name: get_task_execution_context; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_task_execution_context(input_task_uuid uuid)
RETURNS TABLE(
    task_uuid uuid,
    named_task_uuid uuid,
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
    health_status text,
    enqueued_steps bigint
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    SELECT * FROM get_step_readiness_status(input_task_uuid, NULL)
  ),
  task_info AS (
    SELECT
      t.task_uuid,
      t.named_task_uuid,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM tasker_tasks t
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
    WHERE t.task_uuid = input_task_uuid
  ),
  aggregated_stats AS (
    SELECT
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      -- NEW: Track steps awaiting orchestration processing separately
      COUNT(CASE WHEN sd.current_state = 'enqueued_for_orchestration' THEN 1 END) as enqueued_for_orchestration_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      -- Count PERMANENTLY blocked failures (exhausted retries OR explicitly marked as not retryable)
      COUNT(CASE WHEN sd.current_state = 'error'
                  AND (sd.attempts >= sd.retry_limit) THEN 1 END) as permanently_blocked_steps
    FROM step_data sd
  )
  SELECT
    ti.task_uuid,
    ti.named_task_uuid,
    ti.current_status as status,

    -- Step Statistics
    COALESCE(ast.total_steps, 0) as total_steps,
    COALESCE(ast.pending_steps, 0) as pending_steps,
    COALESCE(ast.in_progress_steps, 0) as in_progress_steps,
    COALESCE(ast.completed_steps, 0) as completed_steps,
    COALESCE(ast.failed_steps, 0) as failed_steps,
    COALESCE(ast.ready_steps, 0) as ready_steps,

    -- FIXED: Execution State Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0
           OR COALESCE(ast.enqueued_steps, 0) > 0
           OR COALESCE(ast.enqueued_for_orchestration_steps, 0) > 0 THEN 'processing'  -- UPDATED
      -- OLD BUG: WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      -- NEW FIX: Only blocked if failed steps are NOT retry-eligible
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- FIXED: Recommended Action Logic
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'execute_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 THEN 'wait_for_completion'
      -- OLD BUG: WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'handle_failures'
      -- NEW FIX: Only handle failures if they're truly blocked
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'handle_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'finalize_task'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Progress Metrics
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 0.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::decimal / COALESCE(ast.total_steps, 1)::decimal) * 100, 2)
    END as completion_percentage,

    -- FIXED: Health Status Logic
    CASE
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      -- NEW FIX: Only blocked if failures are truly not retry-eligible
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      -- NEW: Waiting state for retry-eligible failures with backoff
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    -- Enqueued steps (must be last to match RETURNS TABLE declaration)
    COALESCE(ast.enqueued_steps, 0) + COALESCE(ast.enqueued_for_orchestration_steps, 0) as enqueued_steps

  FROM task_info ti
  CROSS JOIN aggregated_stats ast;
END;
$$;

-- =============================================================================
-- SECTION 2: ADD MISSING BATCH FUNCTIONS
-- =============================================================================

--
-- Name: get_step_readiness_status_batch; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_step_readiness_status_batch(input_task_uuids uuid[])
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
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
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_states AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.to_state,
      wst.created_at as state_created_at
    FROM tasker_workflow_step_transitions wst
    WHERE wst.most_recent = true
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  ),
  dependency_counts AS (
    SELECT
      wse.to_step_uuid,
      COUNT(*) as total_deps,
      COUNT(CASE WHEN parent_state.to_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_deps
    FROM tasker_workflow_step_edges wse
    LEFT JOIN step_states parent_state ON parent_state.workflow_step_uuid = wse.from_step_uuid
    WHERE wse.to_step_uuid IN (
      SELECT ws.workflow_step_uuid
      FROM tasker_workflow_steps ws
      WHERE ws.task_uuid = ANY(input_task_uuids)
    )
    GROUP BY wse.to_step_uuid
  ),
  last_failures AS (
    SELECT DISTINCT ON (wst.workflow_step_uuid)
      wst.workflow_step_uuid,
      wst.created_at as failure_time
    FROM tasker_workflow_step_transitions wst
    WHERE wst.to_state = 'error'
    ORDER BY wst.workflow_step_uuid, wst.created_at DESC
  )
  SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    ns.name::TEXT,
    COALESCE(ss.to_state, 'pending')::TEXT as current_state,

    -- Dependencies satisfied
    CASE
      WHEN dc.total_deps IS NULL THEN true
      WHEN dc.completed_deps = dc.total_deps THEN true
      ELSE false
    END as dependencies_satisfied,

    -- Retry eligibility
    CASE
      WHEN COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3) THEN true
      ELSE false
    END as retry_eligible,

    -- Ready for execution
    CASE
      WHEN COALESCE(ss.to_state, 'pending') = 'pending'
      AND (ws.processed = false OR ws.processed IS NULL)
      AND (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps)
      AND (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3))
      AND (COALESCE(ws.retryable, true) = true)
      AND (ws.in_process = false OR ws.in_process IS NULL)
      AND (
        -- Check explicit backoff timing (most restrictive)
        -- If backoff is set, the backoff period must have expired
        CASE
          WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
            ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') <= NOW()
          ELSE true  -- No explicit backoff set
        END
        AND
        -- Then check failure-based backoff
        (lf.failure_time IS NULL OR
         lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds')) <= NOW())
      )
      THEN true
      ELSE false
    END as ready_for_execution,

    lf.failure_time as last_failure_at,

    -- Next retry calculation
    CASE
      WHEN ws.last_attempted_at IS NOT NULL AND ws.backoff_request_seconds IS NOT NULL THEN
        ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
      WHEN lf.failure_time IS NOT NULL THEN
        lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
      ELSE NULL
    END as next_retry_at,

    COALESCE(dc.total_deps, 0)::INTEGER as total_parents,
    COALESCE(dc.completed_deps, 0)::INTEGER as completed_parents,
    COALESCE(ws.attempts, 0)::INTEGER as attempts,
    COALESCE(ws.retry_limit, 3) as retry_limit,
    ws.backoff_request_seconds,
    ws.last_attempted_at

  FROM tasker_workflow_steps ws
  JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
  LEFT JOIN step_states ss ON ss.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN dependency_counts dc ON dc.to_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
  WHERE ws.task_uuid = ANY(input_task_uuids)
  ORDER BY ws.workflow_step_uuid;
END;
$$;

--
-- Name: get_task_execution_contexts_batch; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_task_execution_contexts_batch(input_task_uuids uuid[])
RETURNS TABLE(
    task_uuid uuid,
    named_task_uuid uuid,
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
    health_status text,
    enqueued_steps bigint
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH step_data AS (
    SELECT * FROM get_step_readiness_status_batch(input_task_uuids)
  ),
  task_info AS (
    SELECT
      t.task_uuid,
      t.named_task_uuid,
      COALESCE(task_state.to_state, 'pending')::TEXT as current_status
    FROM unnest(input_task_uuids) AS task_list(task_uuid)
    JOIN tasker_tasks t ON t.task_uuid = task_list.task_uuid
    LEFT JOIN tasker_task_transitions task_state
      ON task_state.task_uuid = t.task_uuid
      AND task_state.most_recent = true
  ),
  aggregated_stats AS (
    SELECT
      sd.task_uuid,
      COUNT(*) as total_steps,
      COUNT(CASE WHEN sd.current_state = 'pending' THEN 1 END) as pending_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps,
      COUNT(CASE WHEN sd.current_state = 'in_progress' THEN 1 END) as in_progress_steps,
      COUNT(CASE WHEN sd.current_state = 'enqueued_for_orchestration' THEN 1 END) as enqueued_for_orchestration_steps,
      COUNT(CASE WHEN sd.current_state IN ('complete', 'resolved_manually') THEN 1 END) as completed_steps,
      COUNT(CASE WHEN sd.current_state = 'error' THEN 1 END) as failed_steps,
      COUNT(CASE WHEN sd.ready_for_execution = true THEN 1 END) as ready_steps,
      COUNT(CASE WHEN sd.current_state = 'error' AND (sd.attempts >= sd.retry_limit) THEN 1 END) as permanently_blocked_steps
    FROM step_data sd
    GROUP BY sd.task_uuid
  )
  SELECT
    ti.task_uuid,
    ti.named_task_uuid,
    ti.current_status as status,
    COALESCE(ast.total_steps, 0) as total_steps,
    COALESCE(ast.pending_steps, 0) as pending_steps,
    COALESCE(ast.in_progress_steps, 0) as in_progress_steps,
    COALESCE(ast.completed_steps, 0) as completed_steps,
    COALESCE(ast.failed_steps, 0) as failed_steps,
    COALESCE(ast.ready_steps, 0) as ready_steps,

    -- Execution status
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'has_ready_steps'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0
           OR COALESCE(ast.enqueued_steps, 0) > 0
           OR COALESCE(ast.enqueued_for_orchestration_steps, 0) > 0 THEN 'processing'  -- UPDATED
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked_by_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'all_complete'
      ELSE 'waiting_for_dependencies'
    END as execution_status,

    -- Recommended action
    CASE
      WHEN COALESCE(ast.ready_steps, 0) > 0 THEN 'process_ready_steps'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 THEN 'review_failures'
      WHEN COALESCE(ast.completed_steps, 0) = COALESCE(ast.total_steps, 0) AND COALESCE(ast.total_steps, 0) > 0 THEN 'mark_complete'
      WHEN COALESCE(ast.in_progress_steps, 0) > 0 OR COALESCE(ast.enqueued_steps, 0) > 0 THEN 'wait_for_completion'
      ELSE 'wait_for_dependencies'
    END as recommended_action,

    -- Completion percentage
    CASE
      WHEN COALESCE(ast.total_steps, 0) = 0 THEN 100.0
      ELSE ROUND((COALESCE(ast.completed_steps, 0)::numeric / ast.total_steps::numeric) * 100, 2)
    END as completion_percentage,

    -- Health Status Logic
    CASE
      WHEN COALESCE(ast.failed_steps, 0) = 0 THEN 'healthy'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) > 0 THEN 'recovering'
      WHEN COALESCE(ast.permanently_blocked_steps, 0) > 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'blocked'
      WHEN COALESCE(ast.failed_steps, 0) > 0 AND COALESCE(ast.permanently_blocked_steps, 0) = 0 AND COALESCE(ast.ready_steps, 0) = 0 THEN 'recovering'
      ELSE 'unknown'
    END as health_status,

    -- Enqueued steps (must be last to match RETURNS TABLE declaration)
    COALESCE(ast.enqueued_steps, 0) + COALESCE(ast.enqueued_for_orchestration_steps, 0) as enqueued_steps

  FROM task_info ti
  LEFT JOIN aggregated_stats ast ON ast.task_uuid = ti.task_uuid;
END;
$$;

-- =============================================================================
-- SECTION 3: ADD MISSING ANALYTICS FUNCTIONS
-- =============================================================================

--
-- Name: get_slowest_steps; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_slowest_steps(
    since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone,
    limit_count integer DEFAULT 10,
    namespace_filter text DEFAULT NULL::text,
    task_name_filter text DEFAULT NULL::text,
    version_filter text DEFAULT NULL::text
)
RETURNS TABLE(
    workflow_step_uuid uuid,
    task_uuid uuid,
    step_name character varying,
    task_name character varying,
    namespace_name character varying,
    version character varying,
    duration_seconds numeric,
    attempts integer,
    created_at timestamp without time zone,
    completed_at timestamp without time zone,
    retryable boolean,
    step_status character varying
)
LANGUAGE plpgsql STABLE AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH step_durations AS (
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ns.name as step_name,
            nt.name as task_name,
            tn.name as namespace_name,
            nt.version,
            ws.created_at,
            ws.attempts,
            ws.retryable,
            wst.created_at as completion_time,
            wst.to_state as final_state,
            EXTRACT(EPOCH FROM (wst.created_at - ws.created_at)) as duration_seconds
        FROM tasker_workflow_steps ws
        INNER JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        INNER JOIN tasker_tasks t ON t.task_uuid = ws.task_uuid
        INNER JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE ws.created_at > analysis_start
          AND wst.most_recent = true
          AND wst.to_state = 'complete'
          AND (namespace_filter IS NULL OR tn.name = namespace_filter)
          AND (task_name_filter IS NULL OR nt.name = task_name_filter)
          AND (version_filter IS NULL OR nt.version = version_filter)
    )
    SELECT
        sd.workflow_step_uuid,
        sd.task_uuid,
        sd.step_name,
        sd.task_name,
        sd.namespace_name,
        sd.version,
        ROUND(sd.duration_seconds, 3),
        sd.attempts,
        sd.created_at,
        sd.completion_time,
        sd.retryable,
        sd.final_state
    FROM step_durations sd
    WHERE sd.duration_seconds IS NOT NULL
      AND sd.duration_seconds > 0
    ORDER BY sd.duration_seconds DESC
    LIMIT limit_count;
END;
$$;

--
-- Name: get_slowest_tasks; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_slowest_tasks(
    since_timestamp timestamp with time zone DEFAULT NULL::timestamp with time zone,
    limit_count integer DEFAULT 10,
    namespace_filter text DEFAULT NULL::text,
    task_name_filter text DEFAULT NULL::text,
    version_filter text DEFAULT NULL::text
)
RETURNS TABLE(
    task_uuid uuid,
    task_name character varying,
    namespace_name character varying,
    version character varying,
    duration_seconds numeric,
    step_count bigint,
    completed_steps bigint,
    error_steps bigint,
    created_at timestamp without time zone,
    completed_at timestamp without time zone,
    initiator character varying,
    source_system character varying
)
LANGUAGE plpgsql STABLE AS $$
DECLARE
    analysis_start TIMESTAMPTZ;
BEGIN
    analysis_start := COALESCE(since_timestamp, NOW() - INTERVAL '24 hours');

    RETURN QUERY
    WITH task_durations AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            tn.name as namespace_name,
            nt.version,
            t.created_at,
            t.initiator,
            t.source_system,
            MAX(wst.created_at) FILTER (
                WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
            ) as latest_completion,
            COUNT(DISTINCT ws.workflow_step_uuid) as step_count,
            COUNT(DISTINCT ws.workflow_step_uuid) FILTER (
                WHERE wst.to_state = 'complete' AND wst.most_recent = true
            ) as completed_steps,
            COUNT(DISTINCT ws.workflow_step_uuid) FILTER (
                WHERE wst.to_state = 'error' AND wst.most_recent = true
            ) as error_steps
        FROM tasker_tasks t
        INNER JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        INNER JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN tasker_workflow_steps ws ON ws.task_uuid = t.task_uuid
        LEFT JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid
        WHERE t.created_at > analysis_start
          AND (namespace_filter IS NULL OR tn.name = namespace_filter)
          AND (task_name_filter IS NULL OR nt.name = task_name_filter)
          AND (version_filter IS NULL OR nt.version = version_filter)
        GROUP BY t.task_uuid, nt.name, tn.name, nt.version, t.created_at, t.initiator, t.source_system
        HAVING MAX(wst.created_at) FILTER (
            WHERE wst.to_state IN ('complete', 'error') AND wst.most_recent = true
        ) IS NOT NULL
    )
    SELECT
        td.task_uuid,
        td.task_name,
        td.namespace_name,
        td.version,
        ROUND(EXTRACT(EPOCH FROM (td.latest_completion - td.created_at)), 3) as duration_seconds,
        td.step_count,
        td.completed_steps,
        td.error_steps,
        td.created_at,
        td.latest_completion,
        td.initiator,
        td.source_system
    FROM task_durations td
    WHERE EXTRACT(EPOCH FROM (td.latest_completion - td.created_at)) > 0
    ORDER BY EXTRACT(EPOCH FROM (td.latest_completion - td.created_at)) DESC
    LIMIT limit_count;
END;
$$;

--
-- Name: get_system_health_counts; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_system_health_counts()
RETURNS TABLE(
    total_tasks bigint,
    pending_tasks bigint,
    in_progress_tasks bigint,
    complete_tasks bigint,
    error_tasks bigint,
    cancelled_tasks bigint,
    total_steps bigint,
    pending_steps bigint,
    in_progress_steps bigint,
    complete_steps bigint,
    error_steps bigint,
    retryable_error_steps bigint,
    exhausted_retry_steps bigint,
    in_backoff_steps bigint,
    active_connections bigint,
    max_connections bigint,
    enqueued_steps bigint
)
LANGUAGE plpgsql STABLE AS $$
BEGIN
    RETURN QUERY
    WITH task_counts AS (
        SELECT
            COUNT(*) as total_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'pending') as pending_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'in_progress') as in_progress_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'complete') as complete_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'error') as error_tasks,
            COUNT(*) FILTER (WHERE task_state.to_state = 'cancelled') as cancelled_tasks
        FROM tasker_tasks t
        LEFT JOIN tasker_task_transitions task_state ON task_state.task_uuid = t.task_uuid
            AND task_state.most_recent = true
    ),
    step_counts AS (
        SELECT
            COUNT(*) as total_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'pending') as pending_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'enqueued') as enqueued_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'in_progress') as in_progress_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'complete') as complete_steps,
            COUNT(*) FILTER (WHERE step_state.to_state = 'error') as error_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts < ws.retry_limit
                AND COALESCE(ws.retryable, true) = true
            ) as retryable_error_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.attempts >= ws.retry_limit
            ) as exhausted_retry_steps,
            COUNT(*) FILTER (
                WHERE step_state.to_state = 'error'
                AND ws.backoff_request_seconds IS NOT NULL
                AND ws.last_attempted_at IS NOT NULL
                AND ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second') > NOW()
            ) as in_backoff_steps
        FROM tasker_workflow_steps ws
        LEFT JOIN tasker_workflow_step_transitions step_state ON step_state.workflow_step_uuid = ws.workflow_step_uuid
            AND step_state.most_recent = true
    ),
    connection_info AS (
        SELECT
            (SELECT count(*) FROM pg_stat_activity WHERE state != 'idle') as active_connections,
            (SELECT setting::bigint FROM pg_settings WHERE name = 'max_connections') as max_connections
    )
    SELECT
        tc.total_tasks,
        tc.pending_tasks,
        tc.in_progress_tasks,
        tc.complete_tasks,
        tc.error_tasks,
        tc.cancelled_tasks,
        sc.total_steps,
        sc.pending_steps,
        sc.in_progress_steps,
        sc.complete_steps,
        sc.error_steps,
        sc.retryable_error_steps,
        sc.exhausted_retry_steps,
        sc.in_backoff_steps,
        ci.active_connections,
        ci.max_connections,
        sc.enqueued_steps
    FROM task_counts tc
    CROSS JOIN step_counts sc
    CROSS JOIN connection_info ci;
END;
$$;

-- =============================================================================
-- SECTION 4: ADD MISSING VIEW
-- =============================================================================

--
-- Name: tasker_step_dag_relationships; Type: VIEW; Schema: public; Owner: -
--
CREATE OR REPLACE VIEW public.tasker_step_dag_relationships AS
SELECT
    ws.workflow_step_uuid,
    ws.task_uuid,
    ws.named_step_uuid,
    COALESCE(parent_data.parent_uuids, '[]'::jsonb) AS parent_step_uuids,
    COALESCE(child_data.child_uuids, '[]'::jsonb) AS child_step_uuids,
    COALESCE(parent_data.parent_count, 0) AS parent_count,
    COALESCE(child_data.child_count, 0) AS child_count,
    CASE
        WHEN COALESCE(parent_data.parent_count, 0) = 0 THEN true
        ELSE false
    END AS is_root_step,
    CASE
        WHEN COALESCE(child_data.child_count, 0) = 0 THEN true
        ELSE false
    END AS is_leaf_step,
    depth_info.min_depth_from_root
FROM tasker_workflow_steps ws
LEFT JOIN (
    SELECT
        to_step_uuid,
        jsonb_agg(from_step_uuid ORDER BY from_step_uuid) AS parent_uuids,
        count(*) AS parent_count
    FROM tasker_workflow_step_edges
    GROUP BY to_step_uuid
) parent_data ON parent_data.to_step_uuid = ws.workflow_step_uuid
LEFT JOIN (
    SELECT
        from_step_uuid,
        jsonb_agg(to_step_uuid ORDER BY to_step_uuid) AS child_uuids,
        count(*) AS child_count
    FROM tasker_workflow_step_edges
    GROUP BY from_step_uuid
) child_data ON child_data.from_step_uuid = ws.workflow_step_uuid
LEFT JOIN (
    WITH RECURSIVE step_depths AS (
        -- Root nodes (no parents)
        SELECT
            ws_inner.workflow_step_uuid,
            0 AS depth_from_root,
            ws_inner.task_uuid
        FROM tasker_workflow_steps ws_inner
        WHERE NOT EXISTS (
            SELECT 1
            FROM tasker_workflow_step_edges e
            WHERE e.to_step_uuid = ws_inner.workflow_step_uuid
        )
        UNION ALL
        -- Recursive step
        SELECT
            e.to_step_uuid,
            sd.depth_from_root + 1,
            sd.task_uuid
        FROM step_depths sd
        JOIN tasker_workflow_step_edges e ON e.from_step_uuid = sd.workflow_step_uuid
        WHERE sd.depth_from_root < 50  -- Prevent infinite recursion
    )
    SELECT
        workflow_step_uuid,
        min(depth_from_root) AS min_depth_from_root
    FROM step_depths
    GROUP BY workflow_step_uuid
) depth_info ON depth_info.workflow_step_uuid = ws.workflow_step_uuid;

-- =============================================================================
-- SECTION 5: ADD MISSING UNIQUE CONSTRAINTS
-- =============================================================================

-- Registry table unique constraints
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tasker_task_namespaces_name_unique') THEN
        ALTER TABLE tasker_task_namespaces ADD CONSTRAINT tasker_task_namespaces_name_unique UNIQUE (name);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tasker_dependent_systems_name_unique') THEN
        ALTER TABLE tasker_dependent_systems ADD CONSTRAINT tasker_dependent_systems_name_unique UNIQUE (name);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tasker_annotation_types_name_unique') THEN
        ALTER TABLE tasker_annotation_types ADD CONSTRAINT tasker_annotation_types_name_unique UNIQUE (name);
    END IF;
END $$;

-- Composite unique constraints for named tasks and steps
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tasker_named_tasks_namespace_name_version_unique') THEN
        ALTER TABLE tasker_named_tasks ADD CONSTRAINT tasker_named_tasks_namespace_name_version_unique
        UNIQUE (task_namespace_uuid, name, version);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'tasker_named_steps_system_name_unique') THEN
        ALTER TABLE tasker_named_steps ADD CONSTRAINT tasker_named_steps_system_name_unique
        UNIQUE (dependent_system_uuid, name);
    END IF;
END $$;

-- =============================================================================
-- SECTION 6: ADD MISSING COMPOSITE INDEXES FOR STATE TRANSITIONS
-- =============================================================================

-- Composite unique indexes for most_recent state tracking
CREATE UNIQUE INDEX IF NOT EXISTS idx_task_transitions_uuid_most_recent
    ON tasker_task_transitions (task_uuid, most_recent)
    WHERE most_recent = true;

CREATE UNIQUE INDEX IF NOT EXISTS idx_workflow_step_transitions_uuid_most_recent
    ON tasker_workflow_step_transitions (workflow_step_uuid, most_recent)
    WHERE most_recent = true;

-- Sort key indexes for transition ordering
CREATE INDEX IF NOT EXISTS idx_task_transitions_uuid_sort_key
    ON tasker_task_transitions (task_uuid, sort_key);

CREATE INDEX IF NOT EXISTS idx_workflow_step_transitions_uuid_sort_key
    ON tasker_workflow_step_transitions (workflow_step_uuid, sort_key);

-- =============================================================================
-- SECTION 7: ADD MISSING PERFORMANCE INDEXES
-- =============================================================================

-- UUID v7 time-ordering optimization indexes
CREATE INDEX IF NOT EXISTS idx_tasks_uuid_temporal
    ON tasker_tasks (task_uuid, created_at);

CREATE INDEX IF NOT EXISTS idx_task_transitions_uuid_temporal
    ON tasker_task_transitions (task_transition_uuid, created_at);

CREATE INDEX IF NOT EXISTS idx_workflow_step_transitions_uuid_temporal
    ON tasker_workflow_step_transitions (workflow_step_transition_uuid, created_at);

-- Hot path covering indexes for active task queries
CREATE INDEX IF NOT EXISTS idx_tasks_active_with_priority_covering
    ON tasker_tasks (complete, priority, task_uuid)
    INCLUDE (named_task_uuid, requested_at)
    WHERE complete = false;

-- Step processing covering indexes
CREATE INDEX IF NOT EXISTS idx_workflow_steps_ready_covering
    ON tasker_workflow_steps (task_uuid, processed, in_process)
    INCLUDE (workflow_step_uuid, attempts, retry_limit, retryable)
    WHERE processed = false;

-- Transition state lookup indexes
CREATE INDEX IF NOT EXISTS idx_task_transitions_state_lookup
    ON tasker_task_transitions (task_uuid, to_state, most_recent)
    WHERE most_recent = true;

CREATE INDEX IF NOT EXISTS idx_workflow_step_transitions_state_lookup
    ON tasker_workflow_step_transitions (workflow_step_uuid, to_state, most_recent)
    WHERE most_recent = true;

--
-- Name: get_step_transitive_dependencies; Type: FUNCTION; Schema: public; Owner: -
--
CREATE OR REPLACE FUNCTION public.get_step_transitive_dependencies(target_step_uuid uuid)
RETURNS TABLE (
    workflow_step_uuid uuid,
    task_uuid uuid,
    named_step_uuid uuid,
    step_name character varying(128),
    results jsonb,
    processed boolean,
    distance integer
)
LANGUAGE plpgsql STABLE
AS $$
BEGIN
    RETURN QUERY
    WITH RECURSIVE transitive_deps AS (
        -- Base case: direct parents of the target step
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ws.named_step_uuid,
            ns.name as step_name,
            ws.results,
            ws.processed,
            1 as distance
        FROM tasker_workflow_step_edges wse
        JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        WHERE wse.to_step_uuid = target_step_uuid

        UNION ALL

        -- Recursive case: parents of parents
        SELECT
            ws.workflow_step_uuid,
            ws.task_uuid,
            ws.named_step_uuid,
            ns.name as step_name,
            ws.results,
            ws.processed,
            td.distance + 1
        FROM transitive_deps td
        JOIN tasker_workflow_step_edges wse ON wse.to_step_uuid = td.workflow_step_uuid
        JOIN tasker_workflow_steps ws ON ws.workflow_step_uuid = wse.from_step_uuid
        JOIN tasker_named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
        WHERE td.distance < 50  -- Prevent infinite recursion
    )
    SELECT
        td.workflow_step_uuid,
        td.task_uuid,
        td.named_step_uuid,
        td.step_name,
        td.results,
        td.processed,
        td.distance
    FROM transitive_deps td
    ORDER BY td.distance ASC, td.workflow_step_uuid ASC;
END;
$$;

-- TAS-32: Add Enqueued State Validation Migration
--
-- This migration adds proper state validation for the new 'enqueued' state
-- introduced in TAS-32 for queue state management improvements.
--
-- GOALS:
-- 1. Add CHECK constraints to validate workflow step state values
-- 2. Include 'enqueued' as a valid state in the validation
-- 3. Ensure both to_state and from_state columns have proper validation
-- 4. Maintain backward compatibility with existing data
--
-- ARCHITECTURE CHANGE:
-- - Steps are now marked 'enqueued' when placed in queue (not 'in_progress')
-- - Workers transition steps from 'enqueued' to 'in_progress' when claiming
-- - Database becomes single source of truth for step processing state
-- - Queue used only for messaging, not state management

-- =============================================================================
-- ADD STATE VALIDATION CONSTRAINTS
-- =============================================================================

-- Valid workflow step states including the new 'enqueued' state
-- These match the constants defined in both Rust and Ruby state machines
DO $$
BEGIN
    -- Add constraint for to_state column (required field)
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'chk_workflow_step_transitions_to_state'
        AND table_name = 'tasker_workflow_step_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_workflow_step_transitions
        ADD CONSTRAINT chk_workflow_step_transitions_to_state
        CHECK (to_state IN (
            'pending',
            'enqueued',
            'in_progress',
            'enqueued_for_orchestration',
            'enqueued_as_error_for_orchestration',
            'complete',
            'error',
            'cancelled',
            'resolved_manually'
        ));

        RAISE NOTICE 'Added to_state validation constraint with enqueued state';
    ELSE
        RAISE NOTICE 'to_state constraint already exists, skipping';
    END IF;

    -- Add constraint for from_state column (nullable field)
    -- NULL values are allowed for initial transitions
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'chk_workflow_step_transitions_from_state'
        AND table_name = 'tasker_workflow_step_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_workflow_step_transitions
        ADD CONSTRAINT chk_workflow_step_transitions_from_state
        CHECK (from_state IS NULL OR from_state IN (
            'pending',
            'enqueued',
            'in_progress',
            'enqueued_for_orchestration',
            'enqueued_as_error_for_orchestration',
            'complete',
            'error',
            'cancelled',
            'resolved_manually'
        ));

        RAISE NOTICE 'Added from_state validation constraint with enqueued state';
    ELSE
        RAISE NOTICE 'from_state constraint already exists, skipping';
    END IF;
END
$$;

-- =============================================================================
-- TASK TRANSITIONS STATE VALIDATION (FOR CONSISTENCY)
-- =============================================================================

-- Also add state validation for task transitions to maintain consistency
-- Task states do not include 'enqueued' as that is specific to workflow steps
DO $$
BEGIN
    -- Add constraint for task to_state column
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'chk_task_transitions_to_state'
        AND table_name = 'tasker_task_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_task_transitions
        ADD CONSTRAINT chk_task_transitions_to_state
        CHECK (to_state IN (
            'pending',
            'in_progress',
            'complete',
            'error',
            'cancelled',
            'resolved_manually'
        ));

        RAISE NOTICE 'Added task to_state validation constraint';
    ELSE
        RAISE NOTICE 'Task to_state constraint already exists, skipping';
    END IF;

    -- Add constraint for task from_state column
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE constraint_name = 'chk_task_transitions_from_state'
        AND table_name = 'tasker_task_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_task_transitions
        ADD CONSTRAINT chk_task_transitions_from_state
        CHECK (from_state IS NULL OR from_state IN (
            'pending',
            'in_progress',
            'complete',
            'error',
            'cancelled',
            'resolved_manually'
        ));

        RAISE NOTICE 'Added task from_state validation constraint';
    ELSE
        RAISE NOTICE 'Task from_state constraint already exists, skipping';
    END IF;
END
$$;

-- =============================================================================
-- VALIDATION QUERIES FOR EXISTING DATA
-- =============================================================================

-- Verify that existing data is compatible with new constraints
-- This query will help identify any invalid states in existing data
DO $$
DECLARE
    invalid_step_to_states INTEGER;
    invalid_step_from_states INTEGER;
    invalid_task_to_states INTEGER;
    invalid_task_from_states INTEGER;
BEGIN
    -- Check for invalid workflow step to_states
    SELECT COUNT(*) INTO invalid_step_to_states
    FROM public.tasker_workflow_step_transitions
    WHERE to_state NOT IN ('pending', 'enqueued', 'in_progress', 'enqueued_for_orchestration', 'enqueued_as_error_for_orchestration', 'complete', 'error', 'cancelled', 'resolved_manually');

    -- Check for invalid workflow step from_states
    SELECT COUNT(*) INTO invalid_step_from_states
    FROM public.tasker_workflow_step_transitions
    WHERE from_state IS NOT NULL
    AND from_state NOT IN ('pending', 'enqueued', 'in_progress', 'enqueued_for_orchestration', 'enqueued_as_error_for_orchestration', 'complete', 'error', 'cancelled', 'resolved_manually');

    -- Check for invalid task to_states
    SELECT COUNT(*) INTO invalid_task_to_states
    FROM public.tasker_task_transitions
    WHERE to_state NOT IN ('pending', 'in_progress', 'complete', 'error', 'cancelled', 'resolved_manually');

    -- Check for invalid task from_states
    SELECT COUNT(*) INTO invalid_task_from_states
    FROM public.tasker_task_transitions
    WHERE from_state IS NOT NULL
    AND from_state NOT IN ('pending', 'in_progress', 'complete', 'error', 'cancelled', 'resolved_manually');

    -- Report validation results
    RAISE NOTICE 'DATA VALIDATION RESULTS:';
    RAISE NOTICE '  Invalid workflow step to_states: %', invalid_step_to_states;
    RAISE NOTICE '  Invalid workflow step from_states: %', invalid_step_from_states;
    RAISE NOTICE '  Invalid task to_states: %', invalid_task_to_states;
    RAISE NOTICE '  Invalid task from_states: %', invalid_task_from_states;

    IF invalid_step_to_states > 0 OR invalid_step_from_states > 0 OR
       invalid_task_to_states > 0 OR invalid_task_from_states > 0 THEN
        RAISE WARNING 'Found invalid state values in existing data. Review before applying constraints.';
    ELSE
        RAISE NOTICE 'All existing data is compatible with new state constraints.';
    END IF;
END
$$;

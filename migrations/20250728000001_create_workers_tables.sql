-- Create worker registration tables for database-backed TaskHandler architecture
-- This supports distributed worker registration with explicit task associations
-- Phase 1: Database-backed worker registry replacing in-memory TaskHandlerRegistry

-- 1. Core worker identity and metadata table
CREATE TABLE IF NOT EXISTS public.tasker_workers (
    worker_id SERIAL PRIMARY KEY,
    worker_name character varying(255) UNIQUE NOT NULL,
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp(6) without time zone NOT NULL DEFAULT NOW(),
    updated_at timestamp(6) without time zone NOT NULL DEFAULT NOW()
);

-- 2. Explicit associations between workers and named tasks they support
CREATE TABLE IF NOT EXISTS public.tasker_worker_named_tasks (
    id SERIAL PRIMARY KEY,
    worker_id integer NOT NULL REFERENCES public.tasker_workers(worker_id) ON DELETE CASCADE,
    named_task_id integer NOT NULL REFERENCES public.tasker_named_tasks(named_task_id) ON DELETE CASCADE,
    configuration jsonb NOT NULL DEFAULT '{}'::jsonb,
    priority integer DEFAULT 100,
    created_at timestamp(6) without time zone NOT NULL DEFAULT NOW(),
    updated_at timestamp(6) without time zone NOT NULL DEFAULT NOW(),
    UNIQUE(worker_id, named_task_id)
);

-- 3. Worker registration lifecycle and health tracking
CREATE TABLE IF NOT EXISTS public.tasker_worker_registrations (
    id SERIAL PRIMARY KEY,
    worker_id integer NOT NULL REFERENCES public.tasker_workers(worker_id) ON DELETE CASCADE,
    status character varying(50) NOT NULL CHECK (status IN ('registered', 'healthy', 'unhealthy', 'disconnected')),
    connection_type character varying(50) NOT NULL,
    connection_details jsonb NOT NULL DEFAULT '{}'::jsonb,
    last_heartbeat_at timestamp(6) without time zone,
    registered_at timestamp(6) without time zone NOT NULL DEFAULT NOW(),
    unregistered_at timestamp(6) without time zone,
    failure_reason text
);

-- 4. Worker command audit trail for observability and debugging
CREATE TABLE IF NOT EXISTS public.tasker_worker_command_audit (
    id BIGSERIAL PRIMARY KEY,
    worker_id integer NOT NULL REFERENCES public.tasker_workers(worker_id) ON DELETE CASCADE,
    command_type character varying(100) NOT NULL,
    command_direction character varying(20) NOT NULL CHECK (command_direction IN ('sent', 'received')),
    correlation_id character varying(255),
    batch_id bigint REFERENCES public.tasker_step_execution_batches(batch_id) ON DELETE SET NULL,
    task_id bigint REFERENCES public.tasker_tasks(task_id) ON DELETE SET NULL,
    step_count integer,
    execution_time_ms bigint,
    response_status character varying(50),
    error_message text,
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp(6) without time zone NOT NULL DEFAULT NOW()
);

-- Performance indexes following established patterns
-- Core worker lookups
CREATE INDEX IF NOT EXISTS idx_tasker_workers_worker_name ON public.tasker_workers(worker_name);
CREATE INDEX IF NOT EXISTS idx_tasker_workers_created_at ON public.tasker_workers(created_at);

-- Worker-task association lookups - critical for worker selection
CREATE INDEX IF NOT EXISTS idx_tasker_worker_named_tasks_worker ON public.tasker_worker_named_tasks(worker_id);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_named_tasks_task ON public.tasker_worker_named_tasks(named_task_id);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_named_tasks_priority ON public.tasker_worker_named_tasks(priority DESC);

-- Worker registration and health tracking
CREATE INDEX IF NOT EXISTS idx_tasker_worker_registrations_active ON public.tasker_worker_registrations(worker_id, status, unregistered_at);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_registrations_heartbeat ON public.tasker_worker_registrations(last_heartbeat_at) WHERE unregistered_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_tasker_worker_registrations_status ON public.tasker_worker_registrations(status);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_registrations_connection_type ON public.tasker_worker_registrations(connection_type);

-- Worker selection optimization - compound indexes for frequent queries
CREATE INDEX IF NOT EXISTS idx_tasker_worker_selection_active ON public.tasker_worker_registrations(status, last_heartbeat_at) WHERE unregistered_at IS NULL;

-- Worker command audit indexes for observability queries
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_worker ON public.tasker_worker_command_audit(worker_id);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_command_type ON public.tasker_worker_command_audit(command_type);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_direction ON public.tasker_worker_command_audit(command_direction);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_created_at ON public.tasker_worker_command_audit(created_at);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_correlation ON public.tasker_worker_command_audit(correlation_id) WHERE correlation_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_batch ON public.tasker_worker_command_audit(batch_id) WHERE batch_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_task ON public.tasker_worker_command_audit(task_id) WHERE task_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasker_worker_command_audit_status ON public.tasker_worker_command_audit(response_status) WHERE response_status IS NOT NULL;

-- Comments for documentation following established pattern
COMMENT ON TABLE public.tasker_workers IS 'Registered worker instances that can execute tasks in distributed architecture';
COMMENT ON COLUMN public.tasker_workers.worker_name IS 'Unique identifier for the worker (e.g., payment_processor_1)';
COMMENT ON COLUMN public.tasker_workers.metadata IS 'Worker metadata: version, runtime, custom capabilities';

COMMENT ON TABLE public.tasker_worker_named_tasks IS 'Explicit mapping between workers and named tasks they support with configuration';
COMMENT ON COLUMN public.tasker_worker_named_tasks.configuration IS 'Task-specific configuration for this worker (converted from YAML)';
COMMENT ON COLUMN public.tasker_worker_named_tasks.priority IS 'Priority for worker selection (higher = preferred)';
COMMENT ON CONSTRAINT tasker_worker_named_tasks_worker_id_named_task_id_key ON public.tasker_worker_named_tasks IS 'Ensures each named task is registered only once per worker';

COMMENT ON TABLE public.tasker_worker_registrations IS 'Worker registration lifecycle and health status tracking';
COMMENT ON COLUMN public.tasker_worker_registrations.status IS 'Current worker status: registered, healthy, unhealthy, disconnected';
COMMENT ON COLUMN public.tasker_worker_registrations.connection_type IS 'Connection type: tcp, unix, etc.';
COMMENT ON COLUMN public.tasker_worker_registrations.connection_details IS 'Connection info: host, port, listener_port, etc.';
COMMENT ON COLUMN public.tasker_worker_registrations.last_heartbeat_at IS 'Timestamp of last successful heartbeat for health monitoring';

COMMENT ON TABLE public.tasker_worker_command_audit IS 'Audit trail of all worker commands for observability and debugging';
COMMENT ON COLUMN public.tasker_worker_command_audit.command_type IS 'Type of command: register_worker, execute_batch, heartbeat, etc.';
COMMENT ON COLUMN public.tasker_worker_command_audit.command_direction IS 'Direction: sent (worker to orchestrator) or received (orchestrator to worker)';
COMMENT ON COLUMN public.tasker_worker_command_audit.correlation_id IS 'Correlation ID for tracking request-response pairs';
COMMENT ON COLUMN public.tasker_worker_command_audit.batch_id IS 'Optional link to step execution batch if command relates to batch processing';
COMMENT ON COLUMN public.tasker_worker_command_audit.task_id IS 'Optional link to task if command relates to specific task execution';
COMMENT ON COLUMN public.tasker_worker_command_audit.step_count IS 'Number of steps processed in batch commands';
COMMENT ON COLUMN public.tasker_worker_command_audit.execution_time_ms IS 'Command execution time in milliseconds';
COMMENT ON COLUMN public.tasker_worker_command_audit.response_status IS 'Response status: success, error, timeout, etc.';
COMMENT ON COLUMN public.tasker_worker_command_audit.metadata IS 'Additional command metadata without full payload content';

-- 5. Worker transport availability for distributed core instance awareness
-- Tracks which core instances can actually reach which workers via which transports
CREATE TABLE IF NOT EXISTS public.tasker_worker_transport_availability (
    id SERIAL PRIMARY KEY,
    worker_id integer NOT NULL REFERENCES public.tasker_workers(worker_id) ON DELETE CASCADE,
    core_instance_id character varying(255) NOT NULL,
    transport_type character varying(50) NOT NULL CHECK (transport_type IN ('tcp', 'unix')),
    connection_details jsonb NOT NULL,
    is_reachable boolean NOT NULL DEFAULT false,
    last_verified_at timestamp(6) without time zone NOT NULL DEFAULT NOW(),
    created_at timestamp(6) without time zone NOT NULL DEFAULT NOW(),
    updated_at timestamp(6) without time zone NOT NULL DEFAULT NOW(),
    UNIQUE(worker_id, core_instance_id, transport_type)
);

-- Transport availability indexes for efficient worker selection
CREATE INDEX IF NOT EXISTS idx_tasker_worker_transport_availability_worker ON public.tasker_worker_transport_availability(worker_id);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_transport_availability_core ON public.tasker_worker_transport_availability(core_instance_id);
CREATE INDEX IF NOT EXISTS idx_tasker_worker_transport_availability_reachable ON public.tasker_worker_transport_availability(is_reachable, last_verified_at) WHERE is_reachable = true;
CREATE INDEX IF NOT EXISTS idx_tasker_worker_transport_availability_transport ON public.tasker_worker_transport_availability(transport_type);

-- Compound index for worker selection queries
CREATE INDEX IF NOT EXISTS idx_tasker_worker_transport_selection ON public.tasker_worker_transport_availability(core_instance_id, is_reachable, last_verified_at) WHERE is_reachable = true;

-- Comments for transport availability documentation
COMMENT ON TABLE public.tasker_worker_transport_availability IS 'Tracks which core instances can reach which workers via which transports for distributed deployments';
COMMENT ON COLUMN public.tasker_worker_transport_availability.worker_id IS 'Reference to the worker';
COMMENT ON COLUMN public.tasker_worker_transport_availability.core_instance_id IS 'Unique identifier of the core instance (e.g., container hostname or instance ID)';
COMMENT ON COLUMN public.tasker_worker_transport_availability.transport_type IS 'Transport mechanism: tcp or unix socket';
COMMENT ON COLUMN public.tasker_worker_transport_availability.connection_details IS 'Transport-specific connection details (host/port for TCP, socket path for Unix)';
COMMENT ON COLUMN public.tasker_worker_transport_availability.is_reachable IS 'Whether the core instance can currently reach this worker via this transport';
COMMENT ON COLUMN public.tasker_worker_transport_availability.last_verified_at IS 'When reachability was last successfully verified';
-- Note: PostgreSQL truncates long constraint names to 63 characters
-- The unique constraint ensures one transport entry per worker-core-transport combination


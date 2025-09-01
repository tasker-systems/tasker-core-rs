-- =============================================================================
-- PostgreSQL Initialization Script for Tasker Integration Testing
-- =============================================================================
-- This script sets up the database for integration testing with:
-- 1. PGMQ extension for message queue functionality
-- 2. Required extensions for Tasker operation
-- 3. Performance optimizations for testing
-- 4. Integration test specific configurations

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "btree_gin";

-- Enable PGMQ extension for message queue functionality
-- This is critical for the Rust worker orchestration system
CREATE EXTENSION IF NOT EXISTS "pgmq";

-- Create PGMQ queues for different namespaces
-- These queues will be used by the worker for processing tasks
SELECT pgmq.create('orchestration_step_results_queue');
SELECT pgmq.create('orchestration_task_requests_queue');
SELECT pgmq.create('orchestration_task_finalizations_queue');
SELECT pgmq.create('worker_linear_workflow_queue');
SELECT pgmq.create('worker_diamond_workflow_queue');
SELECT pgmq.create('worker_tree_workflow_queue');
SELECT pgmq.create('worker_mixed_dag_workflow_queue');
SELECT pgmq.create('worker_order_fulfillment_queue');
SELECT pgmq.create('worker_inventory_queue');
SELECT pgmq.create('worker_notifications_queue');
SELECT pgmq.create('worker_payments_queue');

-- Grant necessary permissions to the tasker user
GRANT ALL PRIVILEGES ON DATABASE tasker_rust_test TO tasker;
GRANT ALL ON SCHEMA public TO tasker;
GRANT ALL ON ALL TABLES IN SCHEMA public TO tasker;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO tasker;
GRANT ALL ON ALL FUNCTIONS IN SCHEMA public TO tasker;

-- Grant permissions for PGMQ schema
GRANT USAGE ON SCHEMA pgmq TO tasker;
GRANT ALL ON ALL TABLES IN SCHEMA pgmq TO tasker;
GRANT ALL ON ALL SEQUENCES IN SCHEMA pgmq TO tasker;
GRANT ALL ON ALL FUNCTIONS IN SCHEMA pgmq TO tasker;

-- Ensure future objects also have proper permissions
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO tasker;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO tasker;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON FUNCTIONS TO tasker;
ALTER DEFAULT PRIVILEGES IN SCHEMA pgmq GRANT ALL ON TABLES TO tasker;
ALTER DEFAULT PRIVILEGES IN SCHEMA pgmq GRANT ALL ON SEQUENCES TO tasker;
ALTER DEFAULT PRIVILEGES IN SCHEMA pgmq GRANT ALL ON FUNCTIONS TO tasker;

-- Performance optimizations for integration testing
-- Reduce checkpoint frequency for faster testing
ALTER SYSTEM SET checkpoint_segments = 64;
ALTER SYSTEM SET checkpoint_completion_target = 0.9;
ALTER SYSTEM SET wal_buffers = '16MB';
ALTER SYSTEM SET shared_buffers = '256MB';
ALTER SYSTEM SET effective_cache_size = '1GB';

-- Optimize for testing workloads
ALTER SYSTEM SET random_page_cost = 1.1;
ALTER SYSTEM SET seq_page_cost = 1.0;
ALTER SYSTEM SET cpu_tuple_cost = 0.01;
ALTER SYSTEM SET cpu_index_tuple_cost = 0.005;
ALTER SYSTEM SET cpu_operator_cost = 0.0025;

-- Logging configuration for integration testing
ALTER SYSTEM SET log_statement = 'mod';
ALTER SYSTEM SET log_duration = on;
ALTER SYSTEM SET log_min_duration_statement = 1000; -- Log slow queries (>1s)

-- Connection and authentication settings
ALTER SYSTEM SET max_connections = 200;
ALTER SYSTEM SET max_prepared_transactions = 100;

-- Reload configuration
SELECT pg_reload_conf();

-- Verify PGMQ installation and queue creation
DO $$
BEGIN
    -- Check if PGMQ extension is properly installed
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pgmq') THEN
        RAISE EXCEPTION 'PGMQ extension failed to install';
    END IF;

    -- Verify queues were created
    IF NOT EXISTS (SELECT 1 FROM pgmq.meta WHERE queue_name = 'orchestration_step_results_queue') THEN
        RAISE EXCEPTION 'Failed to create orchestration_step_results queue';
    END IF;

    -- Log successful initialization
    RAISE NOTICE 'Tasker integration database initialized successfully with PGMQ support';
    RAISE NOTICE 'Created queues: orchestration_step_results, workflow queues, and namespace queues';
END $$;

# TAS-78: Migration Split Reference

This document details the specific SQL objects and their classification for the PGMQ database separation.

## Migration Classification Matrix

### 20250810140000_uuid_v7_initial_schema.sql

This migration needs to be **split** because it contains both Tasker and PGMQ objects.

#### PGMQ-Specific Objects (Move to `migrations/pgmq/`)

```sql
-- Extension
CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;

-- Headers column management
CREATE OR REPLACE FUNCTION pgmq_ensure_headers_column(queue_name TEXT)
-- Queue headers trigger
CREATE OR REPLACE FUNCTION pgmq_auto_add_headers_trigger()
-- Event trigger for DDL
CREATE EVENT TRIGGER pgmq_headers_event_trigger ON ddl_command_end
```

#### Tasker-Specific Objects (Keep in `migrations/tasker/`)

```sql
-- Extension (also needed for UUIDs)
CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;

-- All tables:
-- tasker_task_namespaces, tasker_dependent_systems, tasker_named_tasks,
-- tasker_named_steps, tasker_tasks, tasker_workflow_steps,
-- tasker_task_transitions, tasker_workflow_step_transitions,
-- tasker_dependent_system_object_maps, tasker_named_tasks_named_steps,
-- tasker_workflow_step_edges, tasker_annotation_types, tasker_task_annotations

-- All views:
-- tasker_step_dag_relationships

-- All task-related functions:
-- get_task_execution_context, get_task_execution_contexts_batch,
-- get_step_readiness_status, get_step_readiness_status_batch,
-- calculate_dependency_levels
```

---

### 20250826180921_add_pgmq_notifications.sql

**Classification**: 100% PGMQ → Move entirely to `migrations/pgmq/`

All objects in this migration are PGMQ wrapper functions:

```sql
-- Move to pgmq migrations
extract_queue_namespace(queue_name TEXT)
pgmq_notify_queue_created()
pgmq_send_with_notify(queue_name TEXT, message JSONB, delay_seconds INTEGER)
pgmq_send_batch_with_notify(queue_name TEXT, messages JSONB[], delay_seconds INTEGER)
pgmq_read_specific_message(queue_name text, target_msg_id bigint, vt_seconds integer)
pgmq_delete_specific_message(queue_name text, target_msg_id bigint)
pgmq_extend_vt_specific_message(queue_name text, target_msg_id bigint, additional_vt_seconds integer)
```

---

### 20251115000000_comprehensive_dlq_system.sql

**Classification**: Tasker (with PGMQ fallback consideration)

This migration is Tasker-specific but has one indirect PGMQ reference:

```sql
-- In v_dlq_dashboard, references pgmq.metrics() for queue stats
-- Need to add fallback when PGMQ is on separate DB
```

**Recommended Change**:
```sql
-- Before
SELECT queue_length FROM pgmq.metrics($1);

-- After (with fallback)
SELECT COALESCE(
    (SELECT queue_length FROM pgmq.metrics($1) WHERE pgmq_available()),
    0
) as queue_length;
```

Or remove direct PGMQ.metrics() call from views and handle at application layer.

---

## Proposed PGMQ Migration File

```sql
-- migrations/pgmq/20250810140001_pgmq_extensions.sql
--
-- PGMQ Database Setup
-- This migration should be applied to the PGMQ database
-- (may be same as Tasker database in single-DB deployments)
--
-- Dependencies:
--   - PostgreSQL 14+ (for PGMQ extension)
--   - pgmq extension installed in the database

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;
CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;

-- ============================================================================
-- PGMQ HEADERS COLUMN SUPPORT
-- ============================================================================

-- Function to add headers column to a pgmq queue table if it doesn't exist
CREATE OR REPLACE FUNCTION pgmq_ensure_headers_column(queue_name TEXT)
RETURNS VOID AS $$
DECLARE
    full_table_name TEXT;
    column_exists BOOLEAN;
BEGIN
    full_table_name := 'q_' || queue_name;
    
    SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'pgmq'
        AND table_name = full_table_name
        AND column_name = 'headers'
    ) INTO column_exists;
    
    IF NOT column_exists THEN
        EXECUTE format('ALTER TABLE pgmq.%I ADD COLUMN headers JSONB', full_table_name);
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Trigger function for auto-adding headers to new queue tables
CREATE OR REPLACE FUNCTION pgmq_auto_add_headers_trigger()
RETURNS event_trigger AS $$
DECLARE
    obj RECORD;
    queue_name TEXT;
BEGIN
    FOR obj IN SELECT * FROM pg_event_trigger_ddl_commands()
    WHERE command_tag = 'CREATE TABLE' AND schema_name = 'pgmq'
    LOOP
        IF obj.object_identity LIKE 'pgmq.q_%' THEN
            queue_name := substring(obj.object_identity from 8);
            PERFORM pgmq_ensure_headers_column(queue_name);
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Install event trigger
DROP EVENT TRIGGER IF EXISTS pgmq_headers_event_trigger;
CREATE EVENT TRIGGER pgmq_headers_event_trigger
ON ddl_command_end
WHEN TAG IN ('CREATE TABLE')
EXECUTE FUNCTION pgmq_auto_add_headers_trigger();
```

---

## Proposed PGMQ Notification Functions File

```sql
-- migrations/pgmq/20250826180921_notification_functions.sql
--
-- PGMQ Notification Wrapper Functions
-- Provides atomic message send + pg_notify for event-driven processing

-- ============================================================================
-- NAMESPACE EXTRACTION
-- ============================================================================

CREATE OR REPLACE FUNCTION extract_queue_namespace(queue_name TEXT)
RETURNS TEXT AS $$
BEGIN
    IF queue_name ~ '^orchestration' THEN
        RETURN 'orchestration';
    END IF;
    
    IF queue_name ~ '^worker_.*_queue$' THEN
        RETURN COALESCE(
            (regexp_match(queue_name, '^worker_(.+?)_queue$'))[1],
            'worker'
        );
    END IF;
    
    IF queue_name ~ '^[a-zA-Z][a-zA-Z0-9_]*_queue$' THEN
        RETURN COALESCE(
            (regexp_match(queue_name, '^([a-zA-Z][a-zA-Z0-9_]*)_queue$'))[1],
            'default'
        );
    END IF;
    
    RETURN 'default';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- ============================================================================
-- QUEUE CREATION NOTIFICATION
-- ============================================================================

CREATE OR REPLACE FUNCTION pgmq_notify_queue_created()
RETURNS trigger AS $$
DECLARE
    event_payload TEXT;
    namespace_name TEXT;
BEGIN
    namespace_name := extract_queue_namespace(NEW.queue_name);
    
    event_payload := json_build_object(
        'event_type', 'queue_created',
        'queue_name', NEW.queue_name,
        'namespace', namespace_name,
        'created_at', NOW()::timestamptz
    )::text;
    
    IF length(event_payload) > 7800 THEN
        event_payload := substring(event_payload, 1, 7790) || '...}';
    END IF;
    
    PERFORM pg_notify('pgmq_queue_created', event_payload);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Install trigger on pgmq.meta
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'pgmq' AND table_name = 'meta') THEN
        DROP TRIGGER IF EXISTS pgmq_queue_created_trigger ON pgmq.meta;
        CREATE TRIGGER pgmq_queue_created_trigger
            AFTER INSERT ON pgmq.meta
            FOR EACH ROW
            EXECUTE FUNCTION pgmq_notify_queue_created();
    END IF;
END;
$$;

-- ============================================================================
-- ATOMIC SEND WITH NOTIFY
-- ============================================================================

CREATE OR REPLACE FUNCTION pgmq_send_with_notify(
    queue_name TEXT,
    message JSONB,
    delay_seconds INTEGER DEFAULT 0
) RETURNS BIGINT AS $$
DECLARE
    msg_id BIGINT;
    namespace_name TEXT;
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
BEGIN
    SELECT pgmq.send(queue_name, message, delay_seconds) INTO msg_id;
    
    namespace_name := extract_queue_namespace(queue_name);
    namespace_channel := 'pgmq_message_ready.' || namespace_name;
    
    event_payload := json_build_object(
        'event_type', 'message_ready',
        'msg_id', msg_id,
        'queue_name', queue_name,
        'namespace', namespace_name,
        'ready_at', NOW()::timestamptz,
        'delay_seconds', delay_seconds
    )::text;
    
    IF length(event_payload) > 7800 THEN
        event_payload := substring(event_payload, 1, 7790) || '...}';
    END IF;
    
    PERFORM pg_notify(namespace_channel, event_payload);
    
    IF namespace_channel != global_channel THEN
        PERFORM pg_notify(global_channel, event_payload);
    END IF;
    
    RETURN msg_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- BATCH SEND WITH NOTIFY
-- ============================================================================

CREATE OR REPLACE FUNCTION pgmq_send_batch_with_notify(
    queue_name TEXT,
    messages JSONB[],
    delay_seconds INTEGER DEFAULT 0
) RETURNS SETOF BIGINT AS $$
DECLARE
    msg_id BIGINT;
    msg_ids BIGINT[];
    namespace_name TEXT;
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
BEGIN
    SELECT ARRAY_AGG(t.msg_id) INTO msg_ids
    FROM pgmq.send_batch(queue_name, messages, delay_seconds) AS t(msg_id);
    
    namespace_name := extract_queue_namespace(queue_name);
    namespace_channel := 'pgmq_message_ready.' || namespace_name;
    
    event_payload := json_build_object(
        'event_type', 'batch_ready',
        'msg_ids', msg_ids,
        'queue_name', queue_name,
        'namespace', namespace_name,
        'message_count', array_length(msg_ids, 1),
        'ready_at', NOW()::timestamptz,
        'delay_seconds', delay_seconds
    )::text;
    
    IF length(event_payload) > 7800 THEN
        event_payload := substring(event_payload, 1, 7790) || '...}';
    END IF;
    
    PERFORM pg_notify(namespace_channel, event_payload);
    
    IF namespace_channel != global_channel THEN
        PERFORM pg_notify(global_channel, event_payload);
    END IF;
    
    FOREACH msg_id IN ARRAY msg_ids LOOP
        RETURN NEXT msg_id;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- SPECIFIC MESSAGE OPERATIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION pgmq_read_specific_message(
    queue_name text,
    target_msg_id bigint,
    vt_seconds integer DEFAULT 30
) RETURNS TABLE (
    msg_id bigint,
    read_ct integer,
    enqueued_at timestamp with time zone,
    vt timestamp with time zone,
    message jsonb
) AS $$
DECLARE
    queue_table_name text;
    sql_query text;
    result_record record;
BEGIN
    IF queue_name ~ '[^a-zA-Z0-9_]' THEN
        RAISE EXCEPTION 'Invalid queue name: %', queue_name;
    END IF;
    
    queue_table_name := 'pgmq.q_' || queue_name;
    
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'pgmq' AND table_name = 'q_' || queue_name
    ) THEN
        RAISE EXCEPTION 'Queue does not exist: %', queue_name;
    END IF;
    
    sql_query := format('
        UPDATE %s
        SET vt = (now() + interval ''%s seconds''), read_ct = read_ct + 1
        WHERE msg_id = %L AND vt <= now()
        RETURNING msg_id, read_ct, enqueued_at, vt, message
    ', queue_table_name, vt_seconds, target_msg_id);
    
    FOR result_record IN EXECUTE sql_query LOOP
        RETURN QUERY SELECT
            result_record.msg_id,
            result_record.read_ct,
            result_record.enqueued_at,
            result_record.vt,
            result_record.message;
    END LOOP;
    
    RETURN;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq_delete_specific_message(
    queue_name text,
    target_msg_id bigint
) RETURNS boolean AS $$
DECLARE
    queue_table_name text;
    deleted_count integer;
BEGIN
    IF queue_name ~ '[^a-zA-Z0-9_]' THEN
        RAISE EXCEPTION 'Invalid queue name: %', queue_name;
    END IF;
    
    queue_table_name := 'pgmq.q_' || queue_name;
    
    EXECUTE format('DELETE FROM %s WHERE msg_id = %L', queue_table_name, target_msg_id);
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    
    RETURN deleted_count > 0;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION pgmq_extend_vt_specific_message(
    queue_name text,
    target_msg_id bigint,
    additional_vt_seconds integer DEFAULT 30
) RETURNS boolean AS $$
DECLARE
    queue_table_name text;
    updated_count integer;
BEGIN
    IF queue_name ~ '[^a-zA-Z0-9_]' THEN
        RAISE EXCEPTION 'Invalid queue name: %', queue_name;
    END IF;
    
    queue_table_name := 'pgmq.q_' || queue_name;
    
    EXECUTE format('
        UPDATE %s SET vt = vt + interval ''%s seconds'' WHERE msg_id = %L
    ', queue_table_name, additional_vt_seconds, target_msg_id);
    
    GET DIAGNOSTICS updated_count = ROW_COUNT;
    RETURN updated_count > 0;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- VALIDATION
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE 'PGMQ notification functions installed successfully';
    RAISE NOTICE 'Use pgmq_send_with_notify() for atomic send + notification';
END;
$$;
```

---

## SQLx Cache Considerations

### Development Workflow

When developing with split databases locally, use a single combined database:

```bash
# Development: Single database with all schemas
DATABASE_URL=postgresql://localhost/tasker_dev
# PGMQ_DATABASE_URL not set - falls back to DATABASE_URL

# Generate SQLx cache
cargo sqlx prepare --workspace
```

### CI Workflow

```yaml
# .github/workflows/test.yml
jobs:
  test:
    strategy:
      matrix:
        db-mode: [single, split]
    
    steps:
      - name: Setup databases
        run: |
          if [ "${{ matrix.db-mode }}" == "split" ]; then
            createdb tasker_test
            createdb pgmq_test
            echo "DATABASE_URL=postgresql://localhost/tasker_test" >> $GITHUB_ENV
            echo "PGMQ_DATABASE_URL=postgresql://localhost/pgmq_test" >> $GITHUB_ENV
          else
            createdb tasker_test
            echo "DATABASE_URL=postgresql://localhost/tasker_test" >> $GITHUB_ENV
          fi
```

### Production Verification

```bash
# Verify cache works with production config
SQLX_OFFLINE=true cargo build --release
```

---

## Migrator Updates

The `Migrator` in `tasker-shared/src/database/migrator.rs` needs updates:

```rust
pub struct Migrator {
    tasker_migrations: Vec<Migration>,
    pgmq_migrations: Vec<Migration>,
}

impl Migrator {
    /// Load migrations from both directories
    pub fn new() -> Self {
        Self {
            tasker_migrations: load_migrations("migrations/tasker"),
            pgmq_migrations: load_migrations("migrations/pgmq"),
        }
    }
    
    /// Run migrations against appropriate pools
    pub async fn run(&self, pools: &DatabasePools) -> Result<(), MigrationError> {
        // Always run Tasker migrations against Tasker pool
        self.run_migrations(&pools.tasker, &self.tasker_migrations).await?;
        
        // Run PGMQ migrations against PGMQ pool
        // (same pool in single-DB mode, separate pool in split mode)
        self.run_migrations(&pools.pgmq, &self.pgmq_migrations).await?;
        
        Ok(())
    }
}
```

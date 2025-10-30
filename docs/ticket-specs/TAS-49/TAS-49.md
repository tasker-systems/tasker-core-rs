# TAS-49: Comprehensive Task Dead Letter Queue (DLQ) & Lifecycle Management

> **⚠️ DEPRECATED - Consolidated into plan.md**
>
> **This document represents the original specification and contains outdated architectural decisions.**
>
> **📖 For current implementation plan, see: [`plan.md`](./plan.md)**
>
> **Key changes from original spec:**
> - DLQ is investigation tracking, not task manipulation (no requeue function)
> - Table partitioning deferred (Phase 4 complexity not needed for pre-alpha)
> - Configuration consolidated into `orchestration.toml` (not separate DLQ files)
> - Per-template lifecycle config via TaskTemplate serialization (not separate DB columns)
>
> **Last Updated**: 2025-10-29 (archived after plan.md consolidation)
>
> ---

## Executive Summary

Implement a comprehensive Dead Letter Queue (DLQ) system with automatic staleness detection, table partitioning, and archival strategies to address task lifecycle management at scale. Building on TAS-48's immediate relief for discovery priority drowning, this ticket delivers enterprise-grade task lifecycle management including: (1) automatic detection and transition of stale tasks to DLQ, (2) time-based table partitioning for unbounded growth, (3) multi-tier DLQ with requeue workflows, (4) automatic archival of completed tasks, and (5) per-template lifecycle configuration.

**Dependencies**: TAS-48 (immediate staleness relief)
**Impact**: Critical - Enables production scale, prevents table growth issues, provides operational DLQ workflows
**Effort**: High - 6-8 weeks implementation across 4 phases
**Type**: Feature / Architecture

---

## Problem Statement

### Problem Context

TAS-48 resolved immediate discovery blocking by excluding stale tasks from `get_next_ready_tasks()`. However, **stale tasks still exist in the database** and will continue to accumulate, creating four critical long-term issues:

### Problem 1: No Automatic Staleness Resolution

Tasks can become stuck in waiting states indefinitely:
- Worker goes offline for specific namespace
- External dependency never resolves
- Configuration error prevents step execution
- Dependency cycle in task definition

**Current State**: Tasks remain in `waiting_for_dependencies` or `waiting_for_retry` forever, consuming database resources and requiring manual intervention.

**Impact**:
- Database bloat from zombie tasks
- Operational burden investigating and cleaning up
- No visibility into why tasks failed
- No automated recovery workflows

### Problem 2: Unbounded Table Growth

The `tasker_tasks` table grows indefinitely:
- Every task creates 1 task row + N step rows + transitions
- No archival strategy for completed tasks
- Single table contains all historical data
- Query performance degrades over time

**Projected Growth** (production estimates):
- 10k tasks/day × 365 days = 3.65M tasks/year
- With 4 steps/task average = 14.6M workflow_steps rows
- With 8 transitions/task average = 29.2M transition rows
- **Total: ~47.5M rows/year across 3 tables**

**Performance Impact**:
- Index size grows linearly
- VACUUM/ANALYZE operations take longer
- Point queries degrade without partitioning
- Backup/restore windows increase

### Problem 3: No Dead Letter Queue

No mechanism to:
- Capture failed tasks for investigation
- Manually retry tasks after fixing root cause
- Track DLQ retry attempts and outcomes
- Distinguish between temporary and permanent failures

**Current Manual Process**:
1. Discover task is stuck via monitoring
2. Query database to understand state
3. Manually update transitions table
4. Hope it gets picked up on next poll
5. Repeat if it fails again

**Needed**: First-class DLQ with automated workflows.

### Problem 4: No Per-Template Lifecycle Configuration

Different workflows have different SLAs:
- Payment processing: 30-minute timeout
- Batch reporting: 24-hour timeout
- Data sync: 1-week timeout (external dependencies)

**Current State**: Global timeouts apply to all tasks regardless of business requirements.

---

## Background

### Current Architecture

From `migrations/20250912000000_tas41_richer_task_states.sql`:

**Task States** (12 states):
- Initial: `pending`, `initializing`
- Active: `enqueuing_steps`, `steps_in_process`, `evaluating_results`
- Waiting: `waiting_for_dependencies`, `waiting_for_retry`, `blocked_by_failures`
- Terminal: `complete`, `error`, `cancelled`, `resolved_manually`

**Current Staleness Configuration** (from `config/tasker/base/state_machine.toml`):

```toml
[state_machine.waiting_states]
max_waiting_for_dependencies_minutes = 60
retry_intervals = [30, 60, 120, 300, 600]
max_blocked_time_minutes = 60
blocked_resolution_strategy = "manual_only"

[state_machine.recovery]
enable_stuck_task_detection = true
stuck_task_detection_interval_seconds = 120
max_stuck_duration_minutes = 10
recovery_strategy = "reset_to_waiting"
max_recoveries_per_task = 3
```

**Gap**: Configuration exists but no enforcement mechanism. Tasks can exceed these limits without automatic action.

### Existing Systems (Related)

- **TAS-48**: Excludes stale tasks from discovery (quick relief)
- **TAS-41**: 12 task states with processor ownership
- **TAS-34**: Component-based configuration system
- **TAS-37**: Finalization claiming with atomic operations
- **PGMQ**: Message queue with visibility timeout and DLQ support

### Design Principles

1. **Non-Disruptive**: Legitimate tasks unaffected
2. **Observable**: Full visibility into DLQ operations
3. **Recoverable**: Manual and automatic retry workflows
4. **Scalable**: Handles millions of tasks per year
5. **Configurable**: Per-environment and per-template tuning

---

## Architecture Overview

### Multi-Tier Task Lifecycle

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Active Tasks                                 │
│  (tasker_tasks + tasker_workflow_steps + transitions)               │
│  - Normal processing via orchestration                               │
│  - TAS-48 excludes stale tasks from discovery                       │
└────────────────────────┬────────────────────────────────────────────┘
                         │
                         │ Staleness detection
                         │ (automatic transition)
                         ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Dead Letter Queue (DLQ)                           │
│  (tasker_tasks_dlq table)                                           │
│  - Tasks that exceeded staleness thresholds                         │
│  - Manual investigation and retry workflows                         │
│  - Automatic requeue with backoff                                   │
│  - Prometheus metrics for alerting                                  │
└────────────────────────┬────────────────────────────────────────────┘
                         │
                         │ After resolution or permanent failure
                         ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Archive                                      │
│  (tasker_tasks_archive - detached partitions or separate table)    │
│  - Completed tasks past retention period                            │
│  - Failed tasks past investigation period                           │
│  - Compressed JSONB for long-term storage                           │
│  - Read-only for compliance/audit                                   │
└─────────────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Normal Execution**: Task → steps → completion → remains in main table for retention period
2. **Staleness Detection**: Task exceeds threshold → automatic transition to DLQ
3. **DLQ Investigation**: Manual review → requeue or mark permanently failed
4. **Archival**: Completed/failed tasks past retention → move to archive

---

## Solution 2: Table Partitioning & Growth Management

### Overview

Implement PostgreSQL native partitioning on `tasker_tasks`, `tasker_workflow_steps`, and `tasker_task_transitions` to:
- Maintain query performance as data grows
- Enable efficient archival of old partitions
- Optimize VACUUM/ANALYZE operations
- Support compliance requirements (data retention)

### Partitioning Strategy

**Partition Key**: `created_at` (time-ordered via UUID v7)
**Partition Interval**: Monthly for production, weekly for high-volume

**Rationale**:
- Most queries filter by recent tasks
- Natural boundary for archival (e.g., "archive tasks >6 months old")
- Aligns with reporting periods
- Manageable partition count (12-24 partitions online)

### Implementation

#### Phase 4.1: Convert Main Tables to Partitioned

**Migration**: `20251020000000_add_table_partitioning.sql`

```sql
-- ============================================================================
-- TAS-49 Phase 4.1: Convert tasker_tasks to partitioned table
-- ============================================================================

-- Step 1: Create new partitioned table
CREATE TABLE tasker_tasks_partitioned (
    LIKE tasker_tasks INCLUDING DEFAULTS INCLUDING CONSTRAINTS INCLUDING INDEXES
) PARTITION BY RANGE (created_at);

-- Step 2: Create initial partitions (3 months: last, current, next)
-- This allows migration of existing data plus buffer for new data
CREATE TABLE tasker_tasks_2025_10 PARTITION OF tasker_tasks_partitioned
    FOR VALUES FROM ('2025-10-01 00:00:00') TO ('2025-11-01 00:00:00');

CREATE TABLE tasker_tasks_2025_11 PARTITION OF tasker_tasks_partitioned
    FOR VALUES FROM ('2025-11-01 00:00:00') TO ('2025-12-01 00:00:00');

CREATE TABLE tasker_tasks_2025_12 PARTITION OF tasker_tasks_partitioned
    FOR VALUES FROM ('2025-12-01 00:00:00') TO ('2026-01-01 00:00:00');

-- Step 3: Migrate existing data (done in batches to avoid long locks)
-- NOTE: This would be done via a separate background job, not in migration
-- INSERT INTO tasker_tasks_partitioned SELECT * FROM tasker_tasks;

-- Step 4: Rename tables (requires downtime or blue-green deployment)
-- ALTER TABLE tasker_tasks RENAME TO tasker_tasks_old;
-- ALTER TABLE tasker_tasks_partitioned RENAME TO tasker_tasks;

-- Step 5: Recreate foreign keys pointing to tasker_tasks
-- (foreign keys must be recreated after partition conversion)

-- ============================================================================
-- Partition management functions
-- ============================================================================

-- Automatic partition creation (runs monthly via cron or scheduled job)
CREATE OR REPLACE FUNCTION create_next_month_partition()
RETURNS TABLE(partition_name TEXT, created BOOLEAN) AS $$
DECLARE
    v_next_month DATE;
    v_following_month DATE;
    v_partition_name TEXT;
    v_partition_exists BOOLEAN;
BEGIN
    -- Calculate next month
    v_next_month := date_trunc('month', NOW() + INTERVAL '2 months');
    v_following_month := v_next_month + INTERVAL '1 month';
    v_partition_name := 'tasker_tasks_' || to_char(v_next_month, 'YYYY_MM');

    -- Check if partition already exists
    SELECT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = v_partition_name
    ) INTO v_partition_exists;

    IF NOT v_partition_exists THEN
        -- Create partition
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF tasker_tasks
             FOR VALUES FROM (%L) TO (%L)',
            v_partition_name,
            v_next_month,
            v_following_month
        );

        RETURN QUERY SELECT v_partition_name, true;
    ELSE
        RETURN QUERY SELECT v_partition_name, false;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Detach old partition for archival
CREATE OR REPLACE FUNCTION detach_old_partition(p_retention_months INTEGER DEFAULT 6)
RETURNS TABLE(partition_name TEXT, detached BOOLEAN) AS $$
DECLARE
    v_cutoff_date DATE;
    v_partition_name TEXT;
    v_partition_exists BOOLEAN;
BEGIN
    -- Calculate cutoff date
    v_cutoff_date := date_trunc('month', NOW() - (p_retention_months || ' months')::INTERVAL);
    v_partition_name := 'tasker_tasks_' || to_char(v_cutoff_date, 'YYYY_MM');

    -- Check if partition exists
    SELECT EXISTS (
        SELECT 1 FROM pg_inherits
        JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
        JOIN pg_class child ON pg_inherits.inhrelid = child.oid
        WHERE parent.relname = 'tasker_tasks'
          AND child.relname = v_partition_name
    ) INTO v_partition_exists;

    IF v_partition_exists THEN
        -- Detach partition (makes it a standalone table)
        EXECUTE format(
            'ALTER TABLE tasker_tasks DETACH PARTITION %I',
            v_partition_name
        );

        RETURN QUERY SELECT v_partition_name, true;
    ELSE
        RETURN QUERY SELECT v_partition_name, false;
    END IF;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION create_next_month_partition IS
'TAS-49: Automatically creates next month''s partition for tasker_tasks.
Run monthly via scheduled job.';

COMMENT ON FUNCTION detach_old_partition IS
'TAS-49: Detaches old partitions for archival.
Detached partitions become standalone tables that can be moved to archive or dropped.';
```

#### Phase 4.2: Apply Partitioning to Related Tables

Repeat for `tasker_workflow_steps` and `tasker_task_transitions`:

```sql
-- Similar approach for workflow_steps (partitioned by task.created_at via join)
CREATE TABLE tasker_workflow_steps_partitioned (
    LIKE tasker_workflow_steps INCLUDING ALL
) PARTITION BY RANGE (created_at);

-- Similar approach for task_transitions (partitioned by created_at)
CREATE TABLE tasker_task_transitions_partitioned (
    LIKE tasker_task_transitions INCLUDING ALL
) PARTITION BY RANGE (created_at);
```

**Note**: Steps and transitions should use same partition boundaries as tasks for co-location benefits.

### Configuration

Add to `config/tasker/base/database.toml`:

```toml
[database.partitioning]
enabled = true
partition_strategy = "monthly"  # monthly | weekly | daily
partition_retention_months = 6  # Keep partitions online for 6 months
auto_create_partitions = true
auto_create_schedule = "0 0 1 * *"  # Cron: 1st of month at midnight
auto_detach_partitions = true
auto_detach_schedule = "0 2 1 * *"  # Cron: 1st of month at 2am

# Migration settings
partition_migration_batch_size = 10000
partition_migration_delay_ms = 100  # Pause between batches to avoid load spike
```

### Benefits

1. **Query Performance**: Queries only scan relevant partitions (10x-100x speedup for recent data)
2. **Maintenance**: VACUUM/ANALYZE operates on individual partitions (faster, less locking)
3. **Archival**: Detach old partitions in O(1) time (vs scanning entire table)
4. **Storage**: Drop old partitions frees space immediately
5. **Compliance**: Partition boundaries enable data retention policies

### Risks & Mitigation

**Risk**: Partition migration requires downtime
**Mitigation**: Blue-green deployment OR use pg_partman for online migration

**Risk**: Queries without created_at filter scan all partitions
**Mitigation**: Add created_at to WHERE clauses; monitor slow query log

**Risk**: Partition overflow if creation job fails
**Mitigation**: Pre-create 2 future partitions; alerting on partition creation failures

---

## Solution 3: Automatic Staleness Detection & Transition

### Overview

Implement background job that periodically detects stale tasks and automatically transitions them to appropriate states (DLQ or error) based on configurable thresholds.

### Staleness Detection Function

**Migration**: `20251015000000_add_staleness_detection.sql`

```sql
-- ============================================================================
-- TAS-49 Phase 3: Automatic staleness detection and transition
-- ============================================================================

-- Function: Detect and transition stale tasks
CREATE OR REPLACE FUNCTION detect_and_transition_stale_tasks(
    p_dry_run BOOLEAN DEFAULT true,
    p_batch_size INTEGER DEFAULT 100
)
RETURNS TABLE(
    task_uuid UUID,
    namespace_name VARCHAR,
    task_name VARCHAR,
    current_state VARCHAR,
    time_in_state_minutes INTEGER,
    staleness_threshold_minutes INTEGER,
    action_taken VARCHAR,
    moved_to_dlq BOOLEAN,
    transition_success BOOLEAN
) AS $$
DECLARE
    v_waiting_deps_threshold_minutes INTEGER;
    v_waiting_retry_threshold_minutes INTEGER;
    v_steps_in_process_threshold_minutes INTEGER;
    v_task_max_lifetime_hours INTEGER;
    v_auto_dlq_enabled BOOLEAN;
BEGIN
    -- Load configuration from system (or use defaults)
    -- TODO: Load from tasker_config table
    v_waiting_deps_threshold_minutes := 60;
    v_waiting_retry_threshold_minutes := 30;
    v_steps_in_process_threshold_minutes := 30;
    v_task_max_lifetime_hours := 24;
    v_auto_dlq_enabled := true;

    RETURN QUERY
    WITH stale_tasks AS (
        SELECT
            t.task_uuid,
            tns.name as namespace_name,
            nt.name as task_name,
            tt.to_state as current_state,
            EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as time_in_state_minutes,
            EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 as task_age_minutes,
            -- Determine threshold based on state and template config
            CASE tt.to_state
                WHEN 'waiting_for_dependencies' THEN
                    COALESCE(
                        (nt.configuration->>'max_waiting_for_dependencies_minutes')::INTEGER,
                        v_waiting_deps_threshold_minutes
                    )
                WHEN 'waiting_for_retry' THEN v_waiting_retry_threshold_minutes
                WHEN 'steps_in_process' THEN
                    COALESCE(
                        (nt.configuration->>'max_steps_in_process_minutes')::INTEGER,
                        v_steps_in_process_threshold_minutes
                    )
                ELSE 1440  -- 24 hours default
            END as threshold_minutes,
            -- Overall task max lifetime check
            COALESCE(
                (nt.configuration->>'max_duration_minutes')::INTEGER,
                v_task_max_lifetime_hours * 60
            ) as task_lifetime_threshold_minutes
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
        WHERE tt.most_recent = true
          -- Only non-terminal states
          AND tt.to_state NOT IN ('complete', 'error', 'cancelled', 'resolved_manually')
          -- Task is stale in current state OR exceeded max lifetime
          AND (
              EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 > CASE tt.to_state
                  WHEN 'waiting_for_dependencies' THEN v_waiting_deps_threshold_minutes
                  WHEN 'waiting_for_retry' THEN v_waiting_retry_threshold_minutes
                  WHEN 'steps_in_process' THEN v_steps_in_process_threshold_minutes
                  ELSE 1440
              END
              OR
              EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 > (v_task_max_lifetime_hours * 60)
          )
        LIMIT p_batch_size
    ),
    transition_results AS (
        SELECT
            st.*,
            -- Perform transition if not dry run
            CASE
                WHEN p_dry_run THEN false
                ELSE (
                    -- Move to DLQ if enabled
                    CASE
                        WHEN v_auto_dlq_enabled THEN
                            -- Insert into DLQ table
                            EXISTS (
                                INSERT INTO tasker_tasks_dlq (
                                    task_uuid,
                                    original_state,
                                    dlq_reason,
                                    dlq_timestamp,
                                    task_snapshot,
                                    metadata
                                )
                                SELECT
                                    st.task_uuid,
                                    st.current_state,
                                    'staleness_timeout',
                                    NOW(),
                                    jsonb_build_object(
                                        'task_uuid', st.task_uuid,
                                        'namespace', st.namespace_name,
                                        'task_name', st.task_name,
                                        'time_in_state_minutes', st.time_in_state_minutes,
                                        'threshold_minutes', st.threshold_minutes
                                    ),
                                    jsonb_build_object(
                                        'detection_time', NOW(),
                                        'detector', 'automatic_staleness_detection'
                                    )
                                RETURNING true
                            )
                        ELSE false
                    END
                )
            END as dlq_inserted,
            -- Transition task to error state
            CASE
                WHEN p_dry_run THEN false
                ELSE (
                    -- Mark old transition as not most_recent
                    UPDATE tasker_task_transitions
                    SET most_recent = false
                    WHERE task_uuid = st.task_uuid
                      AND most_recent = true
                    RETURNING true
                ) AND (
                    -- Insert new error transition
                    INSERT INTO tasker_task_transitions (
                        task_uuid,
                        from_state,
                        to_state,
                        most_recent,
                        created_at,
                        reason,
                        transition_metadata
                    )
                    VALUES (
                        st.task_uuid,
                        st.current_state,
                        'error',
                        true,
                        NOW(),
                        'staleness_timeout',
                        jsonb_build_object(
                            'time_in_state_minutes', st.time_in_state_minutes,
                            'threshold_minutes', st.threshold_minutes,
                            'automatic_transition', true
                        )
                    )
                    RETURNING true
                )
            END as transition_completed
        FROM stale_tasks st
    )
    SELECT
        tr.task_uuid,
        tr.namespace_name,
        tr.task_name,
        tr.current_state,
        tr.time_in_state_minutes::INTEGER,
        tr.threshold_minutes,
        CASE
            WHEN p_dry_run THEN 'would_transition_to_dlq_and_error'
            WHEN tr.dlq_inserted AND tr.transition_completed THEN 'transitioned_to_dlq_and_error'
            WHEN tr.dlq_inserted THEN 'moved_to_dlq_only'
            WHEN tr.transition_completed THEN 'transitioned_to_error_only'
            ELSE 'transition_failed'
        END as action_taken,
        COALESCE(tr.dlq_inserted, false) as moved_to_dlq,
        COALESCE(tr.transition_completed, false) as transition_success
    FROM transition_results tr;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION detect_and_transition_stale_tasks IS
'TAS-49: Detects tasks that have exceeded staleness thresholds and transitions them to DLQ and error state.
Respects per-template configuration for staleness thresholds.
p_dry_run: If true, only returns what would be done without making changes.
p_batch_size: Maximum number of stale tasks to process in single call.';
```

### Background Job Implementation

Add to `tasker-orchestration/src/orchestration/staleness_detector.rs`:

```rust
use std::time::Duration;
use tokio::time::interval;
use sqlx::PgPool;
use tracing::{info, warn, error};

pub struct StalenessDetector {
    pool: PgPool,
    interval: Duration,
    batch_size: i32,
    dry_run: bool,
}

impl StalenessDetector {
    pub fn new(pool: PgPool, config: &StalenessConfig) -> Self {
        Self {
            pool,
            interval: Duration::from_secs(config.detection_interval_seconds as u64),
            batch_size: config.batch_size,
            dry_run: config.dry_run,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(self.interval);

        loop {
            interval.tick().await;

            match self.detect_and_transition_stale_tasks().await {
                Ok(results) => {
                    let total = results.len();
                    let moved_to_dlq = results.iter().filter(|r| r.moved_to_dlq).count();
                    let transitioned = results.iter().filter(|r| r.transition_success).count();

                    if total > 0 {
                        info!(
                            total = total,
                            moved_to_dlq = moved_to_dlq,
                            transitioned = transitioned,
                            dry_run = self.dry_run,
                            "Staleness detection completed"
                        );

                        // Update metrics
                        metrics::counter!("staleness_detector.tasks_detected").increment(total as u64);
                        metrics::counter!("staleness_detector.tasks_moved_to_dlq").increment(moved_to_dlq as u64);
                        metrics::counter!("staleness_detector.tasks_transitioned").increment(transitioned as u64);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Staleness detection failed");
                    metrics::counter!("staleness_detector.errors").increment(1);
                }
            }
        }
    }

    async fn detect_and_transition_stale_tasks(&self) -> Result<Vec<StalenessResult>, sqlx::Error> {
        sqlx::query_as!(
            StalenessResult,
            r#"
            SELECT
                task_uuid,
                namespace_name,
                task_name,
                current_state,
                time_in_state_minutes,
                staleness_threshold_minutes,
                action_taken,
                moved_to_dlq,
                transition_success
            FROM detect_and_transition_stale_tasks($1, $2)
            "#,
            self.dry_run,
            self.batch_size
        )
        .fetch_all(&self.pool)
        .await
    }
}

#[derive(Debug)]
struct StalenessResult {
    task_uuid: uuid::Uuid,
    namespace_name: String,
    task_name: String,
    current_state: String,
    time_in_state_minutes: i32,
    staleness_threshold_minutes: i32,
    action_taken: String,
    moved_to_dlq: bool,
    transition_success: bool,
}
```

### Configuration

Add to `config/tasker/base/dlq.toml`:

```toml
[staleness_detection]
enabled = true
detection_interval_seconds = 300  # Run every 5 minutes
batch_size = 100                  # Process 100 stale tasks per run
dry_run = false                   # Set to true to test without making changes

# Staleness thresholds (defaults, can be overridden per-template)
[staleness_detection.thresholds]
waiting_for_dependencies_minutes = 60
waiting_for_retry_minutes = 30
steps_in_process_minutes = 30
task_max_lifetime_hours = 24

# Automatic actions
[staleness_detection.actions]
auto_transition_to_error = true
auto_move_to_dlq = true
emit_events = true
event_channel = "task_staleness_detected"

# Notifications
[staleness_detection.notifications]
emit_warnings = true
warning_threshold_minutes = 45  # Warn at 75% of threshold
slack_webhook_url = ""          # Optional: Slack notifications
```

### Per-Template Staleness Configuration

Add to task template YAML schema:

```yaml
name: payment_processing
namespace_name: payments
version: 2.0.0

# TAS-49: Per-template lifecycle configuration
lifecycle:
  max_duration_minutes: 30  # Override default 24h
  max_waiting_for_dependencies_minutes: 10  # Critical path - fail fast
  max_steps_in_process_minutes: 20
  auto_fail_on_timeout: true
  auto_dlq_on_timeout: true
  staleness_action: "dlq"  # dlq | error | manual_review

# Existing fields...
steps:
  - name: validate_payment
    # ... step config
```

**Implementation**: Store in `tasker_named_tasks.configuration` JSONB column, read by staleness detection function.

---

## Solution 4: Dead Letter Queue (DLQ) Architecture

### Overview

Implement comprehensive DLQ system with:
1. DLQ table for failed/stale tasks
2. Manual and automatic requeue workflows
3. Retry limits and backoff strategies
4. DLQ observability and metrics

### DLQ Table Design

**Migration**: `20251012000000_create_dlq_table.sql`

```sql
-- ============================================================================
-- TAS-49 Phase 2: Create Dead Letter Queue table
-- ============================================================================

CREATE TABLE tasker_tasks_dlq (
    -- Primary key
    dlq_entry_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),

    -- Task reference
    task_uuid UUID NOT NULL,
    CONSTRAINT fk_dlq_task FOREIGN KEY (task_uuid) REFERENCES tasker_tasks(task_uuid),

    -- DLQ metadata
    original_state VARCHAR(50) NOT NULL,
    dlq_reason VARCHAR(100) NOT NULL,
    dlq_timestamp TIMESTAMP NOT NULL DEFAULT NOW(),

    -- Retry tracking
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_retry_at TIMESTAMP,
    next_retry_at TIMESTAMP,  -- For automatic retry workflows

    -- Resolution tracking
    resolution_status VARCHAR(50) NOT NULL DEFAULT 'pending',
    -- Values: pending | requeued | permanently_failed | manually_resolved | cancelled
    resolution_timestamp TIMESTAMP,
    resolution_notes TEXT,
    resolved_by VARCHAR(255),  -- User or system that resolved

    -- Snapshot of task state when moved to DLQ
    task_snapshot JSONB NOT NULL,
    -- Contains: task details, step details, transition history

    -- Additional context
    metadata JSONB,
    -- Contains: error details, detection method, original priority, etc.

    -- Timestamps
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for DLQ operations
CREATE INDEX idx_dlq_task_lookup ON tasker_tasks_dlq(task_uuid);
CREATE INDEX idx_dlq_resolution_status ON tasker_tasks_dlq(resolution_status, dlq_timestamp);
CREATE INDEX idx_dlq_reason ON tasker_tasks_dlq(dlq_reason);
CREATE INDEX idx_dlq_next_retry ON tasker_tasks_dlq(next_retry_at) WHERE resolution_status = 'pending';
CREATE INDEX idx_dlq_timestamp ON tasker_tasks_dlq(dlq_timestamp DESC);

-- Constraint: Only one pending DLQ entry per task
CREATE UNIQUE INDEX idx_dlq_unique_pending_task
    ON tasker_tasks_dlq(task_uuid)
    WHERE resolution_status = 'pending';

-- Trigger: Update updated_at on row modification
CREATE TRIGGER update_dlq_updated_at
    BEFORE UPDATE ON tasker_tasks_dlq
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tasker_tasks_dlq IS
'TAS-49: Dead Letter Queue for failed and stale tasks.
Tasks are moved here automatically by staleness detection or manually by operators.
Supports retry workflows, investigation, and permanent failure tracking.';

COMMENT ON COLUMN tasker_tasks_dlq.task_snapshot IS
'Complete snapshot of task state when moved to DLQ.
Includes task details, steps, transitions, and execution context.
Enables investigation without modifying main task tables.';

COMMENT ON COLUMN tasker_tasks_dlq.metadata IS
'Additional context about DLQ entry.
Examples: error stack trace, detection method, related tasks, etc.';
```

### DLQ Requeue Function

```sql
-- ============================================================================
-- Requeue task from DLQ with retry limits
-- ============================================================================

CREATE OR REPLACE FUNCTION requeue_from_dlq(
    p_task_uuid UUID,
    p_max_dlq_retries INTEGER DEFAULT 3,
    p_priority_boost INTEGER DEFAULT 10,
    p_resolved_by VARCHAR DEFAULT 'system_auto_requeue'
)
RETURNS TABLE(
    success BOOLEAN,
    message TEXT,
    new_task_state VARCHAR,
    dlq_retry_count INTEGER
) AS $$
DECLARE
    v_retry_count INTEGER;
    v_dlq_reason VARCHAR;
    v_dlq_entry_uuid UUID;
BEGIN
    -- Get current DLQ entry
    SELECT retry_count, dlq_reason, dlq_entry_uuid
    INTO v_retry_count, v_dlq_reason, v_dlq_entry_uuid
    FROM tasker_tasks_dlq
    WHERE task_uuid = p_task_uuid
      AND resolution_status = 'pending'
    LIMIT 1;

    IF NOT FOUND THEN
        RETURN QUERY SELECT false, 'Task not found in DLQ or already resolved'::TEXT, NULL::VARCHAR, 0;
        RETURN;
    END IF;

    -- Check retry limit
    IF v_retry_count >= p_max_dlq_retries THEN
        -- Mark as permanently failed
        UPDATE tasker_tasks_dlq
        SET resolution_status = 'permanently_failed',
            resolution_timestamp = NOW(),
            resolution_notes = format('Max DLQ retries (%s) exceeded', p_max_dlq_retries),
            resolved_by = 'system_max_retries',
            updated_at = NOW()
        WHERE dlq_entry_uuid = v_dlq_entry_uuid;

        RETURN QUERY SELECT false, 'Max DLQ retries exceeded'::TEXT, 'error'::VARCHAR, v_retry_count;
        RETURN;
    END IF;

    -- Boost task priority for requeue
    UPDATE tasker_tasks
    SET priority = priority + p_priority_boost
    WHERE task_uuid = p_task_uuid;

    -- Reset task state to pending
    UPDATE tasker_task_transitions
    SET most_recent = false
    WHERE task_uuid = p_task_uuid;

    INSERT INTO tasker_task_transitions (
        task_uuid,
        from_state,
        to_state,
        most_recent,
        created_at,
        reason,
        transition_metadata
    )
    VALUES (
        p_task_uuid,
        'error',
        'pending',
        true,
        NOW(),
        'dlq_requeue',
        jsonb_build_object(
            'dlq_entry_uuid', v_dlq_entry_uuid,
            'retry_count', v_retry_count + 1,
            'priority_boost', p_priority_boost,
            'resolved_by', p_resolved_by
        )
    );

    -- Update DLQ entry
    UPDATE tasker_tasks_dlq
    SET retry_count = retry_count + 1,
        last_retry_at = NOW(),
        resolution_status = 'requeued',
        resolution_timestamp = NOW(),
        resolved_by = p_resolved_by,
        updated_at = NOW()
    WHERE dlq_entry_uuid = v_dlq_entry_uuid;

    RETURN QUERY SELECT true, 'Task requeued successfully'::TEXT, 'pending'::VARCHAR, v_retry_count + 1;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION requeue_from_dlq IS
'TAS-49: Requeue a task from DLQ back to pending state.
Increments retry counter, boosts priority, and tracks resolution.
Returns success status and updated retry count.';
```

### DLQ API Endpoints

Add to `tasker-orchestration/src/web/handlers/dlq.rs`:

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// GET /api/v1/dlq - List DLQ entries
pub async fn list_dlq_entries(
    State(pool): State<PgPool>,
    Query(params): Query<DlqListParams>,
) -> Result<Json<Vec<DlqEntry>>, (StatusCode, String)> {
    let entries = sqlx::query_as!(
        DlqEntry,
        r#"
        SELECT
            dlq_entry_uuid,
            task_uuid,
            original_state,
            dlq_reason,
            dlq_timestamp,
            retry_count,
            resolution_status,
            task_snapshot
        FROM tasker_tasks_dlq
        WHERE resolution_status = COALESCE($1, resolution_status)
        ORDER BY dlq_timestamp DESC
        LIMIT $2
        OFFSET $3
        "#,
        params.resolution_status,
        params.limit.unwrap_or(50),
        params.offset.unwrap_or(0)
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(entries))
}

// POST /api/v1/dlq/{task_uuid}/requeue - Requeue from DLQ
pub async fn requeue_from_dlq(
    State(pool): State<PgPool>,
    Path(task_uuid): Path<Uuid>,
    Json(payload): Json<RequeueRequest>,
) -> Result<Json<RequeueResponse>, (StatusCode, String)> {
    let result = sqlx::query!(
        r#"
        SELECT success, message, new_task_state, dlq_retry_count
        FROM requeue_from_dlq($1, $2, $3, $4)
        "#,
        task_uuid,
        payload.max_retries.unwrap_or(3),
        payload.priority_boost.unwrap_or(10),
        payload.resolved_by.as_deref().unwrap_or("api_user")
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.success.unwrap_or(false) {
        Ok(Json(RequeueResponse {
            success: true,
            message: result.message.unwrap_or_default(),
            new_state: result.new_task_state,
            retry_count: result.dlq_retry_count,
        }))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            result.message.unwrap_or_else(|| "Requeue failed".to_string()),
        ))
    }
}

// GET /api/v1/dlq/stats - DLQ statistics
pub async fn get_dlq_stats(
    State(pool): State<PgPool>,
) -> Result<Json<DlqStats>, (StatusCode, String)> {
    let stats = sqlx::query_as!(
        DlqStats,
        r#"
        SELECT
            dlq_reason,
            COUNT(*) as total_entries,
            COUNT(*) FILTER (WHERE resolution_status = 'pending') as pending,
            COUNT(*) FILTER (WHERE resolution_status = 'requeued') as requeued,
            COUNT(*) FILTER (WHERE resolution_status = 'permanently_failed') as permanent_failures,
            AVG(retry_count) as avg_retries,
            MIN(dlq_timestamp) as oldest_entry,
            MAX(dlq_timestamp) as newest_entry
        FROM tasker_tasks_dlq
        GROUP BY dlq_reason
        "#
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(stats))
}

#[derive(Deserialize)]
pub struct DlqListParams {
    resolution_status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct RequeueRequest {
    max_retries: Option<i32>,
    priority_boost: Option<i32>,
    resolved_by: Option<String>,
}

#[derive(Serialize)]
pub struct RequeueResponse {
    success: bool,
    message: String,
    new_state: Option<String>,
    retry_count: Option<i32>,
}
```

### Configuration

Add to `config/tasker/base/dlq.toml`:

```toml
[dlq]
enabled = true
auto_dlq_on_staleness = true
max_dlq_retries = 3
dlq_priority_boost = 10  # Add to priority when requeuing from DLQ

# DLQ reasons configuration
[dlq.reasons]
staleness_timeout = true
max_retries_exceeded = true
worker_unavailable = true
dependency_cycle_detected = true
invalid_state_transition = false  # Hard fail, no DLQ

# Automatic requeue settings
[dlq.auto_requeue]
enabled = false  # Manual by default for safety
requeue_interval_minutes = 60
max_auto_requeue_attempts = 2
requeue_backoff_multiplier = 2.0
```

---

## Solution 5: Archival Strategy

### Overview

Implement automatic archival of completed/failed tasks past retention period to:
- Reduce main table size
- Maintain query performance
- Support compliance requirements
- Enable long-term analytics

### Archival Table Design

```sql
-- ============================================================================
-- TAS-49 Phase 4.3: Create archive table (or use detached partitions)
-- ============================================================================

-- Option 1: Separate archive table (for non-partitioned setup)
CREATE TABLE tasker_tasks_archive (
    LIKE tasker_tasks INCLUDING ALL
);

-- Option 2: For partitioned setup, just detach old partitions
-- (partitions become standalone tables, can be renamed to _archive suffix)

-- ============================================================================
-- Archival function
-- ============================================================================

CREATE OR REPLACE FUNCTION archive_completed_tasks(
    p_retention_days INTEGER DEFAULT 30,
    p_batch_size INTEGER DEFAULT 1000,
    p_dry_run BOOLEAN DEFAULT true
)
RETURNS TABLE(
    tasks_archived INTEGER,
    steps_archived INTEGER,
    transitions_archived INTEGER,
    execution_time_ms INTEGER
) AS $$
DECLARE
    v_cutoff_timestamp TIMESTAMP;
    v_start_time TIMESTAMP;
    v_tasks_archived INTEGER := 0;
    v_steps_archived INTEGER := 0;
    v_transitions_archived INTEGER := 0;
BEGIN
    v_start_time := clock_timestamp();
    v_cutoff_timestamp := NOW() - (p_retention_days || ' days')::INTERVAL;

    IF NOT p_dry_run THEN
        -- Archive tasks (and cascade to steps/transitions via foreign keys)
        WITH tasks_to_archive AS (
            SELECT t.task_uuid
            FROM tasker_tasks t
            JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
            WHERE tt.most_recent = true
              AND tt.to_state IN ('complete', 'error', 'cancelled')
              AND t.created_at < v_cutoff_timestamp
              -- Exclude tasks still in DLQ
              AND NOT EXISTS (
                  SELECT 1 FROM tasker_tasks_dlq dlq
                  WHERE dlq.task_uuid = t.task_uuid
                    AND dlq.resolution_status = 'pending'
              )
            LIMIT p_batch_size
        )
        -- Insert into archive
        INSERT INTO tasker_tasks_archive
        SELECT t.* FROM tasker_tasks t
        WHERE t.task_uuid IN (SELECT task_uuid FROM tasks_to_archive);

        GET DIAGNOSTICS v_tasks_archived = ROW_COUNT;

        -- Archive steps
        INSERT INTO tasker_workflow_steps_archive
        SELECT ws.* FROM tasker_workflow_steps ws
        WHERE ws.task_uuid IN (SELECT task_uuid FROM tasks_to_archive);

        GET DIAGNOSTICS v_steps_archived = ROW_COUNT;

        -- Archive transitions
        INSERT INTO tasker_task_transitions_archive
        SELECT tt.* FROM tasker_task_transitions tt
        WHERE tt.task_uuid IN (SELECT task_uuid FROM tasks_to_archive);

        GET DIAGNOSTICS v_transitions_archived = ROW_COUNT;

        -- Delete from main tables (cascade deletes steps and transitions)
        DELETE FROM tasker_tasks
        WHERE task_uuid IN (SELECT task_uuid FROM tasks_to_archive);
    ELSE
        -- Dry run: just count what would be archived
        SELECT COUNT(*) INTO v_tasks_archived
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        WHERE tt.most_recent = true
          AND tt.to_state IN ('complete', 'error', 'cancelled')
          AND t.created_at < v_cutoff_timestamp;
    END IF;

    RETURN QUERY SELECT
        v_tasks_archived,
        v_steps_archived,
        v_transitions_archived,
        EXTRACT(MILLISECONDS FROM (clock_timestamp() - v_start_time))::INTEGER;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION archive_completed_tasks IS
'TAS-49: Archives completed/failed tasks older than retention period.
Moves tasks, steps, and transitions to archive tables.
p_dry_run: Set to true to preview without making changes.';
```

### Configuration

Add to `config/tasker/base/dlq.toml`:

```toml
[archive]
enabled = true
retention_days = 30
archive_batch_size = 1000
archive_interval_minutes = 60
enable_compression = true  # Use JSONB compression for archived data

# What to archive
[archive.policies]
archive_completed = true
archive_failed = true
archive_cancelled = false  # Keep for audit
archive_dlq_resolved = true
archive_dlq_permanent_failures = false  # Keep for analysis

# Archive strategy
[archive.strategy]
use_separate_archive_table = false  # If false, use detached partitions
archive_storage_class = "cold"      # For cloud storage tiers
enable_analytics_export = false     # Export to data warehouse
```

---

## Observability & Monitoring

### Metrics

Add to `tasker-shared/src/metrics/lifecycle.rs`:

```rust
use prometheus::{Counter, Histogram, Gauge};

pub struct TaskLifecycleMetrics {
    // Staleness detection
    pub stale_tasks_detected: Counter,
    pub stale_tasks_transitioned: Counter,
    pub staleness_detection_errors: Counter,
    pub time_until_stale_histogram: Histogram,

    // DLQ operations
    pub tasks_moved_to_dlq: Counter,
    pub dlq_requeue_attempts: Counter,
    pub dlq_requeue_successes: Counter,
    pub dlq_requeue_failures: Counter,
    pub dlq_permanent_failures: Counter,
    pub dlq_size: Gauge,

    // Archival
    pub tasks_archived: Counter,
    pub archive_execution_time_histogram: Histogram,
    pub archive_failures: Counter,

    // Partitioning
    pub partitions_created: Counter,
    pub partitions_detached: Counter,
    pub partition_creation_failures: Counter,
}
```

### Dashboard Views

```sql
-- DLQ Dashboard View
CREATE OR REPLACE VIEW v_dlq_dashboard AS
SELECT
    dlq_reason,
    COUNT(*) as total_entries,
    COUNT(*) FILTER (WHERE resolution_status = 'pending') as pending,
    COUNT(*) FILTER (WHERE resolution_status = 'requeued') as requeued,
    COUNT(*) FILTER (WHERE resolution_status = 'permanently_failed') as permanent_failures,
    AVG(retry_count) as avg_retries,
    MIN(dlq_timestamp) as oldest_entry,
    MAX(dlq_timestamp) as newest_entry
FROM tasker_tasks_dlq
GROUP BY dlq_reason;

-- Staleness Monitoring View
CREATE OR REPLACE VIEW v_task_staleness_monitoring AS
WITH current_states AS (
    SELECT
        t.task_uuid,
        nt.name as task_name,
        tns.name as namespace_name,
        tt.to_state as current_state,
        EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as minutes_in_state,
        t.priority,
        t.created_at as task_created_at
    FROM tasker_tasks t
    JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid AND tt.most_recent = true
    JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
    JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
    WHERE tt.to_state NOT IN ('complete', 'error', 'cancelled', 'resolved_manually')
)
SELECT
    current_state,
    COUNT(*) as task_count,
    AVG(minutes_in_state) as avg_minutes_in_state,
    MAX(minutes_in_state) as max_minutes_in_state,
    COUNT(*) FILTER (WHERE minutes_in_state > 60) as stale_over_1h,
    COUNT(*) FILTER (WHERE minutes_in_state > 1440) as stale_over_1day,
    array_agg(task_uuid ORDER BY minutes_in_state DESC LIMIT 10) as top_stale_tasks
FROM current_states
GROUP BY current_state
ORDER BY stale_over_1h DESC;

-- Archive Statistics View
CREATE OR REPLACE VIEW v_archive_statistics AS
SELECT
    DATE_TRUNC('month', created_at) as month,
    COUNT(*) as tasks_archived,
    SUM(CASE WHEN (task_snapshot->>'total_steps')::INTEGER IS NOT NULL
             THEN (task_snapshot->>'total_steps')::INTEGER ELSE 0 END) as steps_archived,
    pg_size_pretty(pg_total_relation_size('tasker_tasks_archive')) as archive_size
FROM tasker_tasks_archive
GROUP BY month
ORDER BY month DESC;
```

### Alerts

Prometheus alert rules:

```yaml
# config/alerts/dlq.yml
groups:
  - name: task_lifecycle
    rules:
      - alert: DLQSizeHigh
        expr: dlq_size > 100
        for: 15m
        labels:
          severity: warning
        annotations:
          summary: "DLQ has {{ $value }} pending entries"
          description: "High number of tasks in DLQ may indicate systemic issues"

      - alert: StalenessDetectionFailing
        expr: rate(staleness_detection_errors[5m]) > 0
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "Staleness detection is failing"
          description: "Stale tasks are not being detected and transitioned"

      - alert: ArchivalFailing
        expr: rate(archive_failures[5m]) > 0
        for: 30m
        labels:
          severity: warning
        annotations:
          summary: "Task archival is failing"
          description: "Main tables will continue growing without archival"

      - alert: PartitionCreationFailed
        expr: partition_creation_failures > 0
        labels:
          severity: critical
        annotations:
          summary: "Failed to create next partition"
          description: "Tasks may fail to insert when current partition fills"

      - alert: HighStaleTaskRate
        expr: (stale_tasks_detected / (stale_tasks_detected + tasks_discovered)) > 0.05
        for: 30m
        labels:
          severity: warning
        annotations:
          summary: "{{ $value }}% of tasks are becoming stale"
          description: "High staleness rate indicates configuration or infrastructure issues"
```

---

## Implementation Phases

### Phase 2: DLQ Foundation (Week 1)
**Goal**: Basic DLQ infrastructure

1. **Day 1-2**: Database schema
   - Create `tasker_tasks_dlq` table
   - Add indexes and constraints
   - Write basic insert/query tests

2. **Day 3-4**: DLQ API endpoints
   - List DLQ entries
   - Requeue from DLQ
   - Mark permanently failed
   - DLQ statistics

3. **Day 5**: Metrics and observability
   - Add DLQ metrics
   - Create dashboard views
   - Test alerting

**Deliverable**: Manual DLQ workflows functional

### Phase 3: Automatic Lifecycle Management (Weeks 2-3)
**Goal**: Automatic staleness detection and DLQ transitions

1. **Week 2, Days 1-2**: Staleness detection SQL
   - `detect_and_transition_stale_tasks()` function
   - Per-template threshold support
   - Comprehensive testing

2. **Week 2, Days 3-4**: Background job
   - `StalenessDetector` Rust implementation
   - Configuration loading
   - Integration with orchestration bootstrap

3. **Week 2, Day 5**: Per-template configuration
   - Extend task template YAML schema
   - Update template registration
   - Migration for existing templates

4. **Week 3**: Testing and tuning
   - Integration tests for automatic detection
   - Load testing with stale task scenarios
   - Tune detection intervals and thresholds

**Deliverable**: Automatic staleness detection and DLQ transition

### Phase 4: Partitioning & Archival (Weeks 4-6)
**Goal**: Table partitioning and automatic archival

1. **Week 4, Days 1-3**: Partition migration
   - Create partitioned table schema
   - Write partition management functions
   - Test partition creation/detachment

2. **Week 4, Days 4-5**: Partition migration execution
   - Blue-green deployment plan
   - Data migration scripts
   - Rollback procedures

3. **Week 5, Days 1-2**: Archival functions
   - `archive_completed_tasks()` function
   - Archive table/partition strategy
   - Compression configuration

4. **Week 5, Days 3-5**: Archival background job
   - Schedule archival execution
   - Monitoring and metrics
   - Recovery procedures

5. **Week 6**: Testing and validation
   - Load testing with partitioned tables
   - Archive/restore procedures
   - Performance benchmarking

**Deliverable**: Partitioned tables with automatic archival

### Phase 5: Advanced Priority Management (Weeks 7-8)
**Goal**: Adaptive pooling and advanced priority features

1. **Week 7**: Adaptive pooling
   - Implement dynamic pool size adjustment
   - Based on stale task ratio
   - A/B testing framework

2. **Week 8**: Priority enhancements
   - Priority decay tuning based on metrics
   - Per-namespace priority policies
   - Dashboard for priority analysis

**Deliverable**: Production-optimized priority and discovery

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_staleness_detection_respects_thresholds() {
        // Create tasks at various ages
        // Run staleness detection
        // Assert only tasks exceeding threshold are detected
    }

    #[tokio::test]
    async fn test_dlq_requeue_increments_retry_counter() {
        // Move task to DLQ
        // Requeue
        // Assert retry_count incremented
    }

    #[tokio::test]
    async fn test_dlq_requeue_respects_max_retries() {
        // Move task to DLQ with retry_count = max
        // Attempt requeue
        // Assert marked permanently_failed
    }

    #[tokio::test]
    async fn test_archive_only_archives_old_completed_tasks() {
        // Create mix of old/new, complete/active tasks
        // Run archival
        // Assert only old completed tasks archived
    }

    #[tokio::test]
    async fn test_partition_creation_idempotent() {
        // Create partition
        // Attempt to create same partition again
        // Assert no error, no duplicate
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_dlq_workflow() {
    let manager = DockerIntegrationManager::setup().await?;

    // Create task that will become stale
    let task = create_task_with_dependencies(&manager.orchestration_client, "stale_test").await?;

    // Wait for staleness threshold + detection interval
    tokio::time::sleep(Duration::from_secs(400)).await;

    // Verify task moved to DLQ
    let dlq_entries = get_dlq_entries(&manager.pool).await?;
    assert!(dlq_entries.iter().any(|e| e.task_uuid == task.task_uuid));

    // Requeue from DLQ
    requeue_from_dlq(&manager.pool, task.task_uuid).await?;

    // Verify task back in pending state with boosted priority
    let task_state = get_task_state(&manager.pool, task.task_uuid).await?;
    assert_eq!(task_state.current_state, "pending");
    assert!(task_state.priority > task.priority);  // Boosted
}
```

### Performance Tests

```rust
#[tokio::test]
async fn test_staleness_detection_performance_with_10k_tasks() {
    let pool = setup_test_db().await;

    // Create 10k tasks in various states
    for i in 0..10_000 {
        create_test_task(&pool, i).await?;
    }

    // Measure staleness detection execution time
    let start = Instant::now();
    let results = detect_and_transition_stale_tasks(&pool, false, 1000).await?;
    let duration = start.elapsed();

    // Should complete in < 5 seconds for 10k tasks
    assert!(duration < Duration::from_secs(5));
    println!("Detected {} stale tasks in {:?}", results.len(), duration);
}

#[tokio::test]
async fn test_query_performance_with_partitioned_tables() {
    // Compare query performance:
    // - Non-partitioned table with 1M rows
    // - Partitioned table with 1M rows (10 partitions)

    // Measure: SELECT * FROM tasker_tasks WHERE created_at > NOW() - INTERVAL '7 days'

    // Assert: Partitioned query 10x+ faster
}
```

---

## Success Metrics

### Pre-Implementation Baseline
- Stale task count: 4,459 (from TAS-48 investigation)
- Table size: X MB (`tasker_tasks` + steps + transitions)
- Query latency (recent tasks): Y ms
- Manual DLQ interventions: Z per week

### Post-Implementation Targets

#### Phase 2 (DLQ Foundation)
- DLQ API response time: < 100ms (p95)
- Manual DLQ interventions: Reduced by 80%
- DLQ requeue success rate: > 95%

#### Phase 3 (Automatic Lifecycle)
- Staleness detection latency: < 5s for 10k tasks
- Stale task accumulation: < 100 at any time
- False positive rate: < 1%
- Automatic DLQ transition rate: > 90% of stale tasks

#### Phase 4 (Partitioning & Archival)
- Query performance (recent data): Within 10% of baseline (despite 10x table size)
- Archive success rate: > 99%
- Table growth rate: 0 (steady state via archival)
- Partition operations: 100% automated

#### Phase 5 (Advanced Priority)
- Task discovery fairness: Fresh tasks always discoverable within 10s
- Stale task discovery rate: < 5%
- Adaptive pooling efficiency: Pool size auto-adjusts based on load

---

## Dependencies & Risks

### Dependencies
- **TAS-48**: Immediate staleness relief (priority drowning fix)
- **TAS-41**: 12-state task state machine with processor ownership
- **TAS-34**: Component-based configuration system
- **PostgreSQL 12+**: Partitioning support, UUID v7, JSONB

### Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Partition migration requires extended downtime | Medium | High | Blue-green deployment; online migration tool (pg_partman) |
| Staleness detection too aggressive | Low | High | Conservative thresholds; per-template overrides; feature flag |
| DLQ grows unbounded | Low | Medium | Max retry limits; automatic permanent failure marking; alerting |
| Archive corruption | Very Low | Critical | Backup before archive; dry-run testing; incremental archival |
| Performance degradation from staleness checks | Low | Medium | Indexed queries; batch processing; configurable intervals |

### Rollback Strategy

Each phase independently rollbackable:
- **Phase 2**: Drop DLQ table, disable DLQ features
- **Phase 3**: Disable staleness detection job, no schema changes needed
- **Phase 4**: Revert to non-partitioned table (complex, prefer forward fix)
- **Phase 5**: Disable adaptive pooling, revert to static multiplier

---

## Future Enhancements (Post-TAS-49)

1. **Machine Learning for Staleness Prediction**
   - Predict which tasks likely to become stale
   - Proactive intervention before staleness

2. **Advanced DLQ Analytics**
   - Pattern detection in DLQ entries
   - Automatic root cause suggestions
   - Integration with error tracking systems (Sentry, Datadog)

3. **Multi-Tier Archival**
   - Hot storage: Last 30 days (PostgreSQL)
   - Warm storage: 30-365 days (S3)
   - Cold storage: > 1 year (Glacier)

4. **Cross-Namespace DLQ Policies**
   - Per-namespace DLQ thresholds
   - Namespace-specific retry strategies
   - Priority inheritance rules

5. **Automatic Dependency Resolution**
   - Detect dependency cycles in `waiting_for_dependencies`
   - Suggest manual interventions
   - Auto-resolve simple cases

---

## References

- **TAS-48**: Task Staleness Immediate Relief (prerequisite)
- **TAS-41**: Richer Task States (`docs/ticket-specs/TAS-41/`)
- **TAS-34**: Component-Based Configuration
- **TAS-37**: Dynamic Orchestration with Finalization Claiming
- **PGMQ Documentation**: PostgreSQL Message Queue patterns
- **PostgreSQL Partitioning**: https://www.postgresql.org/docs/current/ddl-partitioning.html

---

## Appendix: Configuration Complete Reference

```toml
# config/tasker/base/dlq.toml

[staleness_detection]
enabled = true
detection_interval_seconds = 300
batch_size = 100
dry_run = false

[staleness_detection.thresholds]
waiting_for_dependencies_minutes = 60
waiting_for_retry_minutes = 30
steps_in_process_minutes = 30
task_max_lifetime_hours = 24

[staleness_detection.actions]
auto_transition_to_error = true
auto_move_to_dlq = true
emit_events = true
event_channel = "task_staleness_detected"

[dlq]
enabled = true
auto_dlq_on_staleness = true
max_dlq_retries = 3
dlq_priority_boost = 10

[dlq.reasons]
staleness_timeout = true
max_retries_exceeded = true
worker_unavailable = true
dependency_cycle_detected = true

[dlq.auto_requeue]
enabled = false
requeue_interval_minutes = 60
max_auto_requeue_attempts = 2
requeue_backoff_multiplier = 2.0

[archive]
enabled = true
retention_days = 30
archive_batch_size = 1000
archive_interval_minutes = 60
enable_compression = true

[archive.policies]
archive_completed = true
archive_failed = true
archive_cancelled = false
archive_dlq_resolved = true

# config/tasker/base/database.toml (additions)

[database.partitioning]
enabled = true
partition_strategy = "monthly"
partition_retention_months = 6
auto_create_partitions = true
auto_detach_partitions = true
```

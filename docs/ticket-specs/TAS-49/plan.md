# TAS-49: Comprehensive Task Dead Letter Queue (DLQ) & Lifecycle Management

**Status**: In Progress
**Created**: 2025-10-29
**Last Updated**: 2025-10-29
**Estimated Effort**: 3-4 weeks
**Type**: Feature / Architecture
**Impact**: Critical - Enables production scale, prevents table growth, provides operational workflows

---

## Executive Summary

Implement a comprehensive Dead Letter Queue (DLQ) system with automatic staleness detection and archival strategies to address task lifecycle management at scale. Building on TAS-48's immediate relief for discovery priority drowning, this ticket delivers enterprise-grade task lifecycle management.

### Scope

**Implemented Phases**:
- ✅ **Phase 1**: DLQ Foundation & Configuration Consolidation (Week 1)
- ✅ **Phase 2**: Automatic Staleness Detection (Week 2)
- ✅ **Phase 3**: Per-Template Lifecycle Configuration (Week 3)
- ✅ **Phase 4**: DLQ Operations & Observability (Week 4)

**Key Architectural Decisions**:
1. **DLQ = Investigation Tracking**: Not task manipulation - resolution happens at step level via existing APIs
2. **Configuration Consolidation**: Move TAS-48 hardcoded thresholds (60min/30min) to `orchestration.toml`
3. **Pre-Alpha Simplification**: Drop/recreate database as needed (no migration required)
4. **Archive API with HTTP 301 Redirects**: Permanent task_uuid persistence across archival
5. **Leverage Existing Infrastructure**: TaskTemplate serialization handles lifecycle config automatically

**Deferred**:
- ❌ **Phase 4 (Original Spec)**: Table partitioning complexity - use simple archival instead
  - **Reason**: Pre-alpha project, simple archival sufficient initially
  - **Revisit When**: Production table growth > 10M rows or query performance degrades

### Dependencies

- **TAS-48**: Immediate staleness relief (prerequisite - completed)
- **TAS-41**: 12-state task state machine with processor ownership
- **TAS-34**: Component-based configuration system
- **TAS-37**: Dynamic orchestration with finalization claiming
- **PostgreSQL 12+**: JSONB, UUID v7, enum types

---

## Problem Statement

### Context

TAS-48 resolved immediate discovery blocking by excluding stale tasks from `get_next_ready_tasks()`. However, **stale tasks still exist in the database** and will continue to accumulate, creating four critical long-term issues.

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
- Track investigation status and outcomes
- Distinguish between temporary and permanent failures
- Provide visibility into systemic issues

**Current Manual Process**:
1. Discover task is stuck via monitoring
2. Query database to understand state
3. Manually fix problem steps via step APIs
4. Update investigation notes somewhere external
5. Hope similar issues don't recur

**Needed**: First-class DLQ with investigation tracking.

### Problem 4: No Per-Template Lifecycle Configuration

Different workflows have different SLAs:
- Payment processing: 30-minute timeout (fail fast)
- Batch reporting: 24-hour timeout (patient execution)
- Data sync: 1-week timeout (external dependencies)

**Current State**: Global timeouts apply to all tasks regardless of business requirements.

**Impact**: Either too aggressive (false positives) or too lenient (delayed detection).

---

## Background

### Current Architecture

From `migrations/20250912000000_tas41_richer_task_states.sql`:

**Task States** (12 states):
- Initial: `pending`, `initializing`
- Active: `enqueuing_steps`, `steps_in_process`, `evaluating_results`
- Waiting: `waiting_for_dependencies`, `waiting_for_retry`, `blocked_by_failures`
- Terminal: `complete`, `error`, `cancelled`, `resolved_manually`

**TAS-48 Staleness Configuration** (hardcoded in SQL):
- `waiting_for_dependencies`: 60 minutes (hardcoded)
- `waiting_for_retry`: 30 minutes (hardcoded)
- **Gap**: Configuration exists but no enforcement mechanism beyond discovery exclusion

### Existing Systems

- **TAS-48**: Excludes stale tasks from discovery (quick relief)
- **TAS-41**: 12 task states with processor ownership tracking
- **TAS-34**: Component-based configuration system
- **TAS-37**: Finalization claiming with atomic operations
- **PGMQ**: Message queue with visibility timeout and DLQ support

### Design Principles

1. **Non-Disruptive**: Legitimate tasks unaffected
2. **Observable**: Full visibility into DLQ operations
3. **Recoverable**: Investigation tracking enables manual resolution
4. **Scalable**: Handles millions of tasks per year
5. **Configurable**: Per-environment and per-template tuning

---

## Context: What We're Building On

### Already Implemented (TAS-48 - October 2025)
- **Staleness Exclusion**: Tasks stuck >60min (dependencies) or >30min (retry) excluded from discovery
- **Priority Decay**: Exponential decay prevents infinite priority accumulation
- **Metrics**: Counters for excluded tasks, priority distribution, discovery latency
- **Problem**: TAS-48 prevents stale tasks from blocking fresh tasks but doesn't provide lifecycle management
- **Note**: Pre-alpha project - can drop/recreate database as needed for clean testing

### Foundation Available (Other Recent Work)
- **12-State Task Machine** (TAS-41): All DLQ transition states already exist
- **Exponential Backoff** (TAS-42/TAS-57): Retry logic with configurable delays
- **Component Config** (TAS-34): Ready for DLQ integration in `orchestration.toml`
- **Actor Pattern** (TAS-46): `StalenessDetectorActor` can integrate cleanly
- **Idempotency** (TAS-54): State guards handle concurrent operations safely

### What's Missing (TAS-49 Must Add)
- ❌ No DLQ tables (`tasker_tasks_dlq`, `tasker_tasks_archive`)
- ❌ No automatic staleness detection (tasks sit in waiting states forever)
- ❌ No configurable thresholds (hardcoded 60/30min in SQL)
- ❌ No per-template lifecycle config (all tasks use global defaults)
- ❌ No DLQ requeue workflows
- ❌ No archival strategy (unbounded table growth)

---

## Phase 1: DLQ Foundation & Configuration Consolidation (Week 1)

### 1.1 Database Schema

**File**: `migrations/20251115000000_add_dlq_tables.sql`

Create core DLQ infrastructure with proper enum types:

```sql
-- DLQ Resolution Status Enum
-- This tracks the lifecycle of a DLQ investigation (NOT task state)
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

Example: Task stuck on payment step → DLQ entry created (pending) → operator resets payment step retry count via step API → task progresses → DLQ entry updated (manually_resolved).

A task can have multiple DLQ entries over time (investigation history), but only one pending entry at a time.';

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

-- Dead Letter Queue Table
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

-- Indexes
CREATE INDEX idx_dlq_task_lookup ON tasker_tasks_dlq(task_uuid);
CREATE INDEX idx_dlq_resolution_status ON tasker_tasks_dlq(resolution_status, dlq_timestamp);
CREATE INDEX idx_dlq_reason ON tasker_tasks_dlq(dlq_reason);

-- Constraint: One pending DLQ entry per task
CREATE UNIQUE INDEX idx_dlq_unique_pending_task
    ON tasker_tasks_dlq(task_uuid)
    WHERE resolution_status = 'pending';

COMMENT ON INDEX idx_dlq_unique_pending_task IS
'TAS-49: Ensures only one pending DLQ investigation per task.
Historical DLQ entries (manually_resolved, permanently_failed, cancelled) are preserved as audit trail.';

-- Archive Tables (simple, no partitioning)
CREATE TABLE tasker_tasks_archive (
    LIKE tasker_tasks INCLUDING ALL
);

CREATE TABLE tasker_workflow_steps_archive (
    LIKE tasker_workflow_steps INCLUDING ALL
);

CREATE TABLE tasker_task_transitions_archive (
    LIKE tasker_task_transitions INCLUDING ALL
);
```

### 1.1.1 DLQ Workflow Examples

To clarify the architecture, here are concrete use cases:

#### Workflow 1: Stale Task → DLQ → Step-Level Resolution

```
Time 0: Task created (Pending state)
Time 1: Task initializing → discovers step "process_payment" waiting on upstream task
  → Step "process_payment" state: Pending (blocked by dependency)
  → Task state: waiting_for_dependencies
Time 61min: Staleness detector runs
  → Task still waiting_for_dependencies (stuck!)
  → transition_task_state_atomic(task_uuid, 'waiting_for_dependencies', 'error')
  → INSERT INTO tasker_tasks_dlq (
      task_uuid,
      original_state='waiting_for_dependencies',
      dlq_reason='staleness_timeout',
      resolution_status='pending',
      task_snapshot='{...full task + steps state...}'
    )

Time 62min: Operator investigates via DLQ API
  → GET /api/v1/dlq/{task_uuid} → sees pending entry
  → Reviews task_snapshot JSONB
  → Identifies issue: Step "process_payment" blocked on cancelled upstream task
  → Operator recreates upstream task manually
  → Uses existing step resolution API:
     PATCH /v1/tasks/{task_uuid}/workflow_steps/{process_payment_uuid}
     body: {
       "action": "retry",
       "notes": "Upstream task recreated, ready to proceed"
     }

Step resolution triggers task state machine:
  → Step "process_payment" transitions from Pending → Enqueued
  → Task state machine detects progress → transitions from Error → StepsInProcess
  → Step executes successfully → Complete
  → Task transitions → Complete

Time 65min: Operator closes investigation:
  → PATCH /api/v1/dlq/{dlq_entry_uuid}
     body: {
       "resolution_status": "manually_resolved",
       "resolution_notes": "Fixed blocked step by recreating upstream dependency",
       "resolved_by": "operator@example.com"
     }

DLQ history for this task:
  Entry 1: dlq_reason='staleness_timeout', resolution_status='manually_resolved'
  Note: Task fixed at step level, DLQ entry tracks investigation only
```

#### Workflow 2: Repeated Step Failures → Investigation History

```
Time 0: Task created
  → Step "charge_card" executes → hits transient payment gateway error
  → Step state: Error (retryable)
  → Task state: EvaluatingResults
Time 5min: TAS-42 retry mechanism
  → Step "charge_card" retries automatically → fails again
  → Step retry_count: 2
  → Step state: waiting_for_retry with backoff
  → Task state: waiting_for_retry
Time 35min: Staleness detector runs
  → Task stuck in waiting_for_retry > 30min threshold
  → transition_task_state_atomic(task_uuid, 'waiting_for_retry', 'error')
  → INSERT DLQ entry 1 (pending, dlq_reason='staleness_timeout')

Time 40min: Operator investigates
  → Reviews task_snapshot → sees step "charge_card" at retry_count=2
  → Checks payment gateway → transient issue resolved
  → Uses step API to reset retry: PATCH /v1/tasks/{uuid}/workflow_steps/{charge_card_uuid}
     body: { "action": "reset_retry_count" }
  → Step retries → succeeds → Complete
  → Task progresses → Complete
  → Updates DLQ: PATCH /api/v1/dlq/{dlq_entry_1_uuid}
     body: { "resolution_status": "manually_resolved", "notes": "Reset step retry after gateway fix" }

Time 120min: SAME task template used for different payment
  → Step "charge_card" fails with SAME payment gateway issue
  → After retries, task stuck in waiting_for_retry again
Time 155min: Staleness detector runs
  → INSERT DLQ entry 2 (pending)  # New investigation for new task instance
  → DLQ entry 1 still exists as audit trail

Time 160min: Operator investigates again
  → Sees pattern: payment gateway repeatedly failing
  → Identifies root cause: gateway API version deprecated
  → Updates step handler code to use new API
  → Uses step API to retry with new code:
     PATCH /v1/tasks/{uuid}/workflow_steps/{charge_card_uuid}
     body: { "action": "retry", "notes": "Using updated gateway API" }
  → Step succeeds with new code
  → Updates DLQ: PATCH /api/v1/dlq/{dlq_entry_2_uuid}
     body: {
       "resolution_status": "manually_resolved",
       "notes": "Fixed underlying handler code - gateway API updated"
     }

DLQ history across multiple task instances:
  Entry 1 (task A): resolution_status='manually_resolved' (temporary fix - reset retry)
  Entry 2 (task B): resolution_status='manually_resolved' (permanent fix - updated code)
  Note: DLQ provides visibility into recurring issues, helping identify systemic problems
```

#### Workflow 3: Dependency Cycle Detection and Template Fix

```
Time 0: Task created from template "order_fulfillment_v1"
  → Step "ship_order" depends on "process_payment"
  → Step "process_payment" depends on "validate_shipping" (circular!)
  → Cycle detection during initialization → some steps blocked
Time 30min: Task stuck in waiting_for_dependencies
  → transition_task_state_atomic(task_uuid, 'waiting_for_dependencies', 'error')
  → INSERT DLQ entry (dlq_reason='dependency_cycle_detected', pending)

Time 35min: Operator investigates
  → GET /api/v1/dlq/{task_uuid}
  → Reviews task_snapshot JSONB
  → Identifies circular dependency in workflow template definition
  → This is a template bug, not a runtime issue
  → Operator actions:
     1. Updates task template YAML to fix dependency cycle
     2. Marks affected steps as cancelled (no way to proceed with broken template):
        PATCH /v1/tasks/{uuid}/workflow_steps/{ship_order_uuid}
        body: { "action": "cancel", "notes": "Template had circular dependency" }
     3. Marks task investigation complete:
        PATCH /api/v1/dlq/{dlq_entry_uuid}
        body: {
          "resolution_status": "manually_resolved",
          "resolution_notes": "Fixed template circular dependency - future tasks will use corrected template",
          "resolved_by": "ops-team"
        }
  → Task remains in Error state (steps cancelled, template fixed for future use)
  → Future tasks using updated template will not hit this issue

DLQ history:
  Entry 1: dlq_reason='dependency_cycle_detected', resolution_status='manually_resolved'
  Note: This task instance failed, but template fix prevents future occurrences
```

#### Key Architectural Points

1. **Tasks Never Leave tasker_tasks**: DLQ entries are metadata/audit records only
2. **DLQ is Investigation Tracking, Not Resolution**:
   - DLQ tracks "why task is stuck" and "who investigated"
   - **Resolution happens at step level** using existing APIs
   - No task-level "requeue" - fix the problem steps instead
3. **Task State vs DLQ Investigation Status**: Independent concerns
   - Task state: Managed by TAS-41 12-state machine
   - DLQ investigation: Managed by operators tracking their work
4. **Multiple DLQ Entries**: Allowed over time as audit trail
5. **task_snapshot**: JSONB snapshot enables root cause analysis
6. **Resolution Workflow**:
   ```
   Task stuck → DLQ entry created
   Operator investigates → identifies problem step(s)
   Operator uses existing step APIs:
     - PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid} (manual resolution)
     - Step retry mechanisms (TAS-42)
   Task state machine naturally progresses when steps fixed
   Operator updates DLQ entry with resolution notes
   ```

**Critical Design Principle**:
**Tasks are not aware of other tasks. Steps are aware of other steps.**
- Resolution must happen at step level (where the actual problem is)
- DLQ provides visibility and investigation tracking
- Existing step management APIs handle actual fixes

**Design Notes**:
- `task_snapshot` JSONB contains complete task state for investigation
- Archive tables are simple copies (no partitioning complexity)
- Enum types provide compile-time safety for state transitions

### 1.2 Configuration Consolidation

**File**: `config/tasker/base/orchestration.toml` (UPDATE - Add DLQ section)

All DLQ configuration belongs in `orchestration.toml` following the component-based architecture. Add new sections:

```toml
# ... existing orchestration config ...

# TAS-49: Dead Letter Queue and Lifecycle Management
[orchestration.staleness_detection]
enabled = true
detection_interval_seconds = 300  # Run every 5 minutes
batch_size = 100
dry_run = false  # Start true for validation

# CONSOLIDATES TAS-48 hardcoded SQL values
[orchestration.staleness_detection.thresholds]
waiting_for_dependencies_minutes = 60  # Was hardcoded in get_next_ready_tasks()
waiting_for_retry_minutes = 30         # Was hardcoded in get_next_ready_tasks()
steps_in_process_minutes = 30
task_max_lifetime_hours = 24

[orchestration.staleness_detection.actions]
auto_transition_to_error = true
auto_move_to_dlq = true
emit_events = true
event_channel = "task_staleness_detected"

[orchestration.dlq]
enabled = true
auto_dlq_on_staleness = true
include_full_task_snapshot = true  # Include complete task+steps state in JSONB
max_pending_age_hours = 168        # Alert if investigation pending > 1 week

[orchestration.dlq.reasons]
# Which conditions trigger DLQ entry creation
staleness_timeout = true
max_retries_exceeded = true
worker_unavailable = true
dependency_cycle_detected = true

[orchestration.archive]
enabled = true
retention_days = 30
archive_batch_size = 1000
archive_interval_hours = 24

[orchestration.archive.policies]
archive_completed = true
archive_failed = true
archive_cancelled = false  # Keep for audit
archive_dlq_resolved = true
```

**Why `orchestration.toml`?**
- Staleness detection runs as orchestration background service
- DLQ investigation tracking is orchestration-layer concern
- Archival runs as orchestration background service
- Worker doesn't need DLQ configuration (only reads task templates)

**Key Config Changes from Original Design**:
- ❌ Removed `max_dlq_retries` - DLQ doesn't retry tasks (that's step-level)
- ❌ Removed `dlq_priority_boost` - DLQ doesn't manipulate priority (step-level)
- ✅ Added `max_pending_age_hours` - Alert on stale investigations
- ✅ Added `include_full_task_snapshot` - Control snapshot detail level

**Refactoring Required**: Update `migrations/20251010000000_add_stale_task_discovery_fixes.sql`:
- Remove hardcoded `v_max_waiting_for_deps_minutes := 60`
- Remove hardcoded `v_max_waiting_for_retry_minutes := 30`
- Load from configuration via parameter or system table

### 1.3 DLQ SQL Functions

**File**: `migrations/20251115000001_add_dlq_functions.sql`

Implement core DLQ operations focused on detection and archival (NOT task manipulation):

```sql
-- Detect and transition stale tasks to DLQ
-- This function ONLY moves tasks to Error state and creates DLQ audit entries
-- Resolution happens at step level via existing step APIs
CREATE OR REPLACE FUNCTION detect_and_transition_stale_tasks(
    p_dry_run BOOLEAN DEFAULT true,
    p_batch_size INTEGER DEFAULT 100,
    p_waiting_deps_threshold_minutes INTEGER DEFAULT 60,
    p_waiting_retry_threshold_minutes INTEGER DEFAULT 30,
    p_steps_in_process_threshold_minutes INTEGER DEFAULT 30,
    p_task_max_lifetime_hours INTEGER DEFAULT 24
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
BEGIN
    -- CTE finds stale tasks
    -- Transitions to Error state via transition_task_state_atomic()
    -- Inserts DLQ entry with task_snapshot for investigation
    -- Returns result set for metrics/logging
    -- Implementation details: TAS-49 spec Section 1.3
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION detect_and_transition_stale_tasks IS
'TAS-49: Detect stale tasks and create DLQ investigation entries.

This function does NOT resolve tasks - it only:
1. Identifies tasks stuck beyond thresholds
2. Transitions them to Error state
3. Creates DLQ entries for operator investigation

Resolution workflow:
1. Operator reviews DLQ entry via API (GET /api/v1/dlq/{task_uuid})
2. Operator uses existing step APIs to fix problem steps (PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid})
3. Task state machine automatically progresses when steps fixed
4. Operator updates DLQ entry to track resolution (PATCH /api/v1/dlq/{entry_uuid})';

-- Archive completed tasks past retention period
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
BEGIN
    -- Move old completed/failed tasks to archive tables
    -- Preserves full audit trail in separate archive
    -- Returns statistics for monitoring
    -- Implementation details: TAS-49 spec Section 1.3
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION archive_completed_tasks IS
'TAS-49: Archive completed tasks to reduce main table growth.

Archives tasks in terminal states (complete, error, cancelled) that are older than retention period.
Preserves complete audit trail in archive tables.';
```

**Key Design Decision**: Removed `requeue_from_dlq()` function
- **Why**: Tasks are not aware of other tasks, resolution happens at step level
- **How resolution works**: Operators use existing step APIs (`PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}`)
- **DLQ role**: Investigation tracking only, not task manipulation

### 1.4 Rust Configuration Types

**File**: `tasker-shared/src/config/orchestration.rs` (UPDATE - Add DLQ types)

Add DLQ configuration types to the existing `OrchestrationConfig` structure:

```rust
use serde::Deserialize;
use std::collections::HashMap;

// Add to existing OrchestrationConfig
#[derive(Debug, Clone, Deserialize)]
pub struct OrchestrationConfig {
    pub web: WebConfig,
    pub mpsc_channels: MpscChannelsConfig,
    pub event_systems: EventSystemsConfig,
    pub shutdown: ShutdownConfig,

    // TAS-49: Add DLQ configuration sections
    pub staleness_detection: StalenessDetectionConfig,
    pub dlq: DlqOperationsConfig,
    pub archive: ArchivalConfig,
}

// TAS-49: New configuration types
#[derive(Debug, Clone, Deserialize)]
pub struct StalenessDetectionConfig {
    pub enabled: bool,
    pub detection_interval_seconds: u64,
    pub batch_size: i32,
    pub dry_run: bool,
    pub thresholds: StalenessThresholds,
    pub actions: StalenessActions,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StalenessThresholds {
    pub waiting_for_dependencies_minutes: i32,
    pub waiting_for_retry_minutes: i32,
    pub steps_in_process_minutes: i32,
    pub task_max_lifetime_hours: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StalenessActions {
    pub auto_transition_to_error: bool,
    pub auto_move_to_dlq: bool,
    pub emit_events: bool,
    pub event_channel: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DlqOperationsConfig {
    pub enabled: bool,
    pub auto_dlq_on_staleness: bool,
    pub include_full_task_snapshot: bool,
    pub max_pending_age_hours: u64,
    pub reasons: HashMap<String, bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArchivalConfig {
    pub enabled: bool,
    pub retention_days: i32,
    pub archive_batch_size: i32,
    pub archive_interval_hours: u64,
    pub policies: ArchivalPolicies,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArchivalPolicies {
    pub archive_completed: bool,
    pub archive_failed: bool,
    pub archive_cancelled: bool,
    pub archive_dlq_resolved: bool,
}
```

**Why in `OrchestrationConfig`?**
- DLQ operations are orchestration-specific (not shared with worker)
- Follows component-based architecture pattern
- No separate `dlq.rs` module needed

### Phase 1 Deliverables

- ✅ DLQ tables created with proper indexes and constraints
- ✅ Archive tables created (simple copies, no partitioning)
- ✅ TAS-48 thresholds moved from hardcoded SQL to `dlq.toml`
- ✅ DLQ SQL functions implemented and tested with `dry_run=true`
- ✅ Rust configuration types created and integrated
- ✅ Configuration loading validated for test/development/production
- ✅ `get_next_ready_tasks()` refactored to accept threshold parameters

---

## Phase 2: Automatic Staleness Detection (Week 2)

### 2.1 StalenessDetector Background Service

**File**: `tasker-orchestration/src/orchestration/staleness_detector.rs` (NEW)

```rust
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn, error};
use opentelemetry::KeyValue;
use tasker_shared::metrics::orchestration;

pub struct StalenessDetector {
    pool: PgPool,
    config: StalenessDetectionConfig,
}

#[derive(Debug, Clone)]
pub struct StalenessResult {
    pub task_uuid: Uuid,
    pub namespace_name: String,
    pub task_name: String,
    pub current_state: String,
    pub time_in_state_minutes: i32,
    pub staleness_threshold_minutes: i32,
    pub action_taken: String,
    pub moved_to_dlq: bool,
    pub transition_success: bool,
}

impl StalenessDetector {
    pub fn new(pool: PgPool, config: StalenessDetectionConfig) -> Self {
        Self { pool, config }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(self.config.detection_interval_seconds));

        loop {
            interval.tick().await;

            let start = std::time::Instant::now();
            match self.detect_and_transition_stale_tasks().await {
                Ok(results) => {
                    let total = results.len();
                    let moved_to_dlq = results.iter().filter(|r| r.moved_to_dlq).count();
                    let transitioned = results.iter().filter(|r| r.transition_success).count();

                    // Record detection duration
                    let duration_ms = start.elapsed().as_millis() as f64;
                    orchestration::staleness_detection_duration().record(
                        duration_ms,
                        &[KeyValue::new("dry_run", self.config.dry_run)],
                    );

                    if total > 0 {
                        info!(
                            total = total,
                            moved_to_dlq = moved_to_dlq,
                            transitioned = transitioned,
                            dry_run = self.config.dry_run,
                            "Staleness detection completed"
                        );

                        // Update metrics with labels
                        for result in &results {
                            let state_labels = &[
                                KeyValue::new("state", result.current_state.clone()),
                                KeyValue::new("action", result.action_taken.clone()),
                            ];
                            orchestration::tasks_detected_stale_total().add(1, state_labels);

                            if result.moved_to_dlq {
                                let dlq_labels = &[
                                    KeyValue::new("reason", "staleness_timeout"),
                                    KeyValue::new("original_state", result.current_state.clone()),
                                ];
                                orchestration::tasks_moved_to_dlq_total().add(1, dlq_labels);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Staleness detection failed");
                    let error_labels = &[KeyValue::new("error_type", "database")];
                    orchestration::staleness_detection_errors_total().add(1, error_labels);
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
            FROM detect_and_transition_stale_tasks($1, $2, $3, $4, $5, $6)
            "#,
            self.config.dry_run,
            self.config.batch_size,
            self.config.thresholds.waiting_for_dependencies_minutes,
            self.config.thresholds.waiting_for_retry_minutes,
            self.config.thresholds.steps_in_process_minutes,
            self.config.thresholds.task_max_lifetime_hours
        )
        .fetch_all(&self.pool)
        .await
    }
}
```

**Integration**: Add to `OrchestrationCore::build()`:

```rust
// In tasker-orchestration/src/core.rs
pub async fn build(config: SystemConfig, pool: PgPool) -> Result<Self> {
    // ... existing setup

    // TAS-49: Staleness detection service
    if config.orchestration.staleness_detection.enabled {
        let staleness_detector = StalenessDetector::new(
            pool.clone(),
            config.orchestration.staleness_detection.clone(),
        );

        // Spawn background task
        tokio::spawn(async move {
            if let Err(e) = staleness_detector.run().await {
                error!(error = %e, "StalenessDetector failed");
            }
        });

        info!("StalenessDetector background service started");
    }

    // ... rest of bootstrap
}
```

### 2.2 DLQ Metrics

**File**: `tasker-shared/src/metrics/orchestration.rs` (UPDATE - Add DLQ metrics)

Following the existing OpenTelemetry metrics pattern (TAS-48, TAS-53 precedent), add DLQ metrics to the orchestration module:

```rust
// ============================================================================
// TAS-49: DLQ and Lifecycle Metrics
// ============================================================================

/// Total number of tasks detected as stale
///
/// Labels:
/// - state: waiting_for_dependencies, waiting_for_retry, steps_in_process
/// - action: transition_to_error, move_to_dlq
pub fn tasks_detected_stale_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.dlq.tasks_detected.total")
        .with_description("Total number of tasks detected as stale")
        .init()
}

/// Total number of tasks moved to DLQ
///
/// Labels:
/// - reason: staleness_timeout, max_retries_exceeded, dependency_cycle
/// - original_state: Task state before DLQ
pub fn tasks_moved_to_dlq_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.dlq.tasks_moved.total")
        .with_description("Total number of tasks moved to DLQ")
        .init()
}

/// Total number of DLQ investigations resolved
///
/// Labels:
/// - resolution_status: manually_resolved, permanently_failed, cancelled
/// - dlq_reason: staleness_timeout, max_retries_exceeded, etc.
pub fn dlq_investigations_resolved_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.dlq.investigations_resolved.total")
        .with_description("Total number of DLQ investigations resolved")
        .init()
}

/// Total number of tasks archived
///
/// Labels:
/// - final_state: complete, error, cancelled, dlq_resolved
pub fn tasks_archived_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.archive.tasks.total")
        .with_description("Total number of tasks archived")
        .init()
}

/// Total number of staleness detection errors
///
/// Labels:
/// - error_type: database, configuration, validation
pub fn staleness_detection_errors_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.dlq.detection_errors.total")
        .with_description("Total number of staleness detection errors")
        .init()
}

/// Time from task creation until detected as stale
///
/// Tracks how long tasks run before becoming stale.
///
/// Labels:
/// - namespace: Task namespace
/// - task_name: Template name
pub fn task_time_until_stale() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.task.time_until_stale")
        .with_description("Time from task creation until stale detection in seconds")
        .with_unit("s")
        .init()
}

/// Staleness detection cycle duration
///
/// Tracks time to run one complete staleness detection cycle.
///
/// Labels:
/// - dry_run: true/false
pub fn staleness_detection_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.dlq.detection.duration")
        .with_description("Staleness detection cycle duration in milliseconds")
        .with_unit("ms")
        .init()
}

/// Archive operation duration
///
/// Tracks time to archive tasks and related data.
///
/// Labels:
/// - batch_size: Number of tasks in batch
pub fn archive_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.archive.duration")
        .with_description("Archive operation duration in milliseconds")
        .with_unit("ms")
        .init()
}

/// DLQ investigation duration in seconds
///
/// Tracks time from DLQ entry creation (pending) to resolution.
/// Helps identify how long investigations take on average.
///
/// Labels:
/// - resolution_status: manually_resolved, permanently_failed, cancelled
/// - dlq_reason: staleness_timeout, max_retries_exceeded, etc.
pub fn dlq_investigation_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.dlq.investigation.duration")
        .with_description("DLQ investigation duration from pending to resolved in seconds")
        .with_unit("s")
        .init()
}

/// Distribution of pending investigation ages
///
/// Tracks how long DLQ entries stay in pending status.
/// Helps identify stale investigations that need attention.
///
/// Labels:
/// - dlq_reason: staleness_timeout, max_retries_exceeded, etc.
pub fn dlq_pending_age_histogram() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.dlq.pending_age")
        .with_description("Age of pending DLQ investigations in hours")
        .with_unit("h")
        .init()
}

/// Current number of pending DLQ entries
///
/// Tracks DLQ backlog size for alerting.
///
/// Labels: none
pub fn dlq_pending_entries() -> Gauge<u64> {
    meter()
        .u64_gauge("tasker.dlq.pending_entries")
        .with_description("Current number of pending DLQ entries")
        .init()
}

// TAS-49: DLQ metrics statics

/// Static counter: tasks_detected_stale_total
pub static TASKS_DETECTED_STALE_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: tasks_moved_to_dlq_total
pub static TASKS_MOVED_TO_DLQ_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: dlq_investigations_resolved_total
pub static DLQ_INVESTIGATIONS_RESOLVED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: tasks_archived_total
pub static TASKS_ARCHIVED_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static counter: staleness_detection_errors_total
pub static STALENESS_DETECTION_ERRORS_TOTAL: OnceLock<Counter<u64>> = OnceLock::new();

/// Static histogram: task_time_until_stale
pub static TASK_TIME_UNTIL_STALE: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: staleness_detection_duration
pub static STALENESS_DETECTION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: archive_duration
pub static ARCHIVE_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: dlq_investigation_duration
pub static DLQ_INVESTIGATION_DURATION: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static histogram: dlq_pending_age_histogram
pub static DLQ_PENDING_AGE_HISTOGRAM: OnceLock<Histogram<f64>> = OnceLock::new();

/// Static gauge: dlq_pending_entries
pub static DLQ_PENDING_ENTRIES: OnceLock<Gauge<u64>> = OnceLock::new();
```

**Update `init()` function** in same file to initialize DLQ metrics:

```rust
pub fn init() {
    // ... existing metrics initialization ...

    // TAS-49: DLQ and lifecycle metrics
    TASKS_DETECTED_STALE_TOTAL.get_or_init(tasks_detected_stale_total);
    TASKS_MOVED_TO_DLQ_TOTAL.get_or_init(tasks_moved_to_dlq_total);
    DLQ_INVESTIGATIONS_RESOLVED_TOTAL.get_or_init(dlq_investigations_resolved_total);
    TASKS_ARCHIVED_TOTAL.get_or_init(tasks_archived_total);
    STALENESS_DETECTION_ERRORS_TOTAL.get_or_init(staleness_detection_errors_total);
    TASK_TIME_UNTIL_STALE.get_or_init(task_time_until_stale);
    STALENESS_DETECTION_DURATION.get_or_init(staleness_detection_duration);
    ARCHIVE_DURATION.get_or_init(archive_duration);
    DLQ_INVESTIGATION_DURATION.get_or_init(dlq_investigation_duration);
    DLQ_PENDING_AGE_HISTOGRAM.get_or_init(dlq_pending_age_histogram);
    DLQ_PENDING_ENTRIES.get_or_init(dlq_pending_entries);
}
```

**Why `orchestration.rs` not `dlq.rs`?**
- Follows TAS-48 precedent: staleness metrics added to `orchestration.rs` (not separate file)
- Follows TAS-53 precedent: decision point metrics added to `orchestration.rs`
- DLQ operations are orchestration-specific (detection, investigation tracking, archival)
- Matches existing meter name: "tasker-orchestration"
- Keeps all task lifecycle metrics in one cohesive module

**Key Metrics Changes from Original Design**:
- ❌ Removed `dlq_requeue_attempts_total` - No requeue operations (step-level resolution)
- ✅ Added `dlq_investigations_resolved_total` - Tracks investigation outcomes
- ✅ Added `dlq_investigation_duration` - Time to resolve investigations
- ✅ Added `dlq_pending_age_histogram` - Stale investigation alerting

### 2.3 Manual Testing Tool (Optional)

**Note**: Since this is pre-alpha greenfield work, we can simply drop/recreate the database rather than migrate existing data. However, a CLI tool is useful for testing staleness detection during development.

**Binary**: `tasker-orchestration/src/bin/staleness-detector.rs` (NEW - Optional for testing)

```rust
use clap::Parser;
use tasker_orchestration::orchestration::staleness_detector::StalenessDetector;
use tasker_shared::config::load_config;

#[derive(Parser)]
#[command(name = "staleness-detector")]
#[command(about = "Test staleness detection functionality", long_about = None)]
struct Args {
    /// Run in dry-run mode (no actual changes)
    #[arg(long, default_value = "true")]
    dry_run: bool,

    /// Batch size for detection
    #[arg(long)]
    batch_size: Option<i32>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let config = load_config()?;
    let pool = create_pool(&config.database).await?;

    let mut detector_config = config.orchestration.staleness_detection.clone();
    detector_config.dry_run = args.dry_run;
    if let Some(batch_size) = args.batch_size {
        detector_config.batch_size = batch_size;
    }

    let detector = StalenessDetector::new(pool.clone(), detector_config);

    println!("Running staleness detection (dry_run={})", detector_config.dry_run);
    let results = detector.detect_and_transition_stale_tasks().await?;

    println!("\nResults:");
    println!("  Total detected: {}", results.len());
    println!("  Moved to DLQ: {}", results.iter().filter(|r| r.moved_to_dlq).count());
    println!("  Transitioned: {}", results.iter().filter(|r| r.transition_success).count());

    if !results.is_empty() {
        println!("\nSample results:");
        for (i, result) in results.iter().take(5).enumerate() {
            println!("  {}. {} ({}) - {} minutes in state",
                i + 1,
                result.task_uuid,
                result.current_state,
                result.time_in_state_minutes
            );
        }
    }

    Ok(())
}
```

**Usage**:
```bash
# Test staleness detection (dry-run, no changes)
cargo run --bin staleness-detector

# Run with custom batch size
cargo run --bin staleness-detector -- --batch-size 50

# Actually perform detection (remove dry-run)
cargo run --bin staleness-detector -- --dry-run=false
```

### Phase 2 Deliverables

- ✅ `StalenessDetector` background service implemented
- ✅ Service integrated into `OrchestrationCore` lifecycle
- ✅ DLQ metrics added to `orchestration.rs` following OpenTelemetry pattern
- ✅ Metrics properly labeled and integrated with existing OTLP export
- ✅ Dry-run mode validated with logging
- ✅ Integration tests cover detection → DLQ transition flow
- ✅ (Optional) CLI tool for manual staleness detection testing

---

## Phase 3: Per-Template Lifecycle Configuration (Week 3)

### 3.1 TaskTemplate Struct Extension

**File**: `tasker-shared/src/models/core/task_template.rs` (UPDATE - Add lifecycle config field)

The lifecycle configuration is stored **within the existing TaskTemplate structure**, not as a separate database column. This leverages the existing template serialization mechanism.

```rust
/// Complete task template with all workflow configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub name: String,
    pub namespace_name: String,
    pub version: String,
    pub description: Option<String>,
    pub metadata: Option<TemplateMetadata>,
    pub task_handler: Option<HandlerDefinition>,
    pub system_dependencies: SystemDependencies,
    pub domain_events: Vec<DomainEventDefinition>,
    pub input_schema: Option<Value>,
    pub steps: Vec<StepDefinition>,
    pub environments: HashMap<String, EnvironmentOverride>,

    // TAS-49: Add lifecycle configuration (OPTIONAL)
    #[serde(default)]
    pub lifecycle: Option<LifecycleConfig>,
}

/// TAS-49: Lifecycle configuration for task templates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifecycleConfig {
    /// Maximum task duration in minutes
    pub max_duration_minutes: Option<i32>,

    /// Maximum time in waiting_for_dependencies state
    pub max_waiting_for_dependencies_minutes: Option<i32>,

    /// Maximum time in waiting_for_retry state
    pub max_waiting_for_retry_minutes: Option<i32>,

    /// Maximum time in steps_in_process state
    pub max_steps_in_process_minutes: Option<i32>,

    /// Action to take when stale
    #[serde(default = "default_staleness_action")]
    pub staleness_action: String, // "dlq", "error", "manual_review"

    /// Automatically transition to error state on timeout
    #[serde(default)]
    pub auto_fail_on_timeout: Option<bool>,

    /// Automatically move to DLQ on timeout
    #[serde(default)]
    pub auto_dlq_on_timeout: Option<bool>,
}

fn default_staleness_action() -> String {
    "dlq".to_string()
}

impl LifecycleConfig {
    /// Get threshold value with fallback to system default
    pub fn get_threshold(&self, field: &str, system_default: i32) -> i32 {
        match field {
            "max_duration_minutes" => self.max_duration_minutes.unwrap_or(system_default),
            "max_waiting_for_dependencies_minutes" => {
                self.max_waiting_for_dependencies_minutes.unwrap_or(system_default)
            }
            "max_waiting_for_retry_minutes" => {
                self.max_waiting_for_retry_minutes.unwrap_or(system_default)
            }
            "max_steps_in_process_minutes" => {
                self.max_steps_in_process_minutes.unwrap_or(system_default)
            }
            _ => system_default,
        }
    }
}
```

**Key Design Point**: No database schema changes needed! The lifecycle config is automatically serialized into the existing `tasker_named_tasks.configuration` JSONB column when `TaskHandlerRegistry::register_task_template()` is called.

### 3.2 Template YAML Schema Extension

**File**: `docs/template-schema.md` (UPDATE)

Add **optional** lifecycle section to task template schema:

```yaml
# Task Template with Lifecycle Configuration (OPTIONAL)
name: payment_processing
namespace_name: payments
version: 2.0.0

# TAS-49: OPTIONAL per-template lifecycle configuration
# If omitted, uses system defaults from orchestration.toml
lifecycle:
  # Maximum task duration (overrides global default)
  max_duration_minutes: 30

  # State-specific timeouts
  max_waiting_for_dependencies_minutes: 10  # Fail fast for payment dependencies
  max_steps_in_process_minutes: 20          # Payment processing shouldn't hang
  max_waiting_for_retry_minutes: 5          # Quick retry cycle for transient errors

  # Automatic actions on timeout
  auto_fail_on_timeout: true     # Transition to Error state
  auto_dlq_on_timeout: true      # Move to DLQ for investigation

  # DLQ behavior
  staleness_action: "dlq"        # Options: dlq | error | manual_review

# Existing fields
steps:
  - name: validate_payment
    handler: PaymentValidator
    # ... step config
```

**Example Templates**:

1. **Using system defaults** (most common):
```yaml
name: standard_workflow
namespace_name: workflows
version: 1.0.0
# No lifecycle config - uses orchestration.toml defaults
steps:
  - name: process_data
    handler: DataProcessor
```

2. **Fast-fail workflow** (payments):
```yaml
lifecycle:
  max_duration_minutes: 30
  max_waiting_for_dependencies_minutes: 10
  staleness_action: "dlq"
```

3. **Patient workflow** (batch reporting):
```yaml
lifecycle:
  max_duration_minutes: 1440  # 24 hours
  max_waiting_for_dependencies_minutes: 480  # 8 hours
  staleness_action: "manual_review"
```

4. **Partial override** (only override specific values):
```yaml
lifecycle:
  # Only override waiting time, use system defaults for rest
  max_waiting_for_dependencies_minutes: 120
```

**Configuration Precedence**:
1. Template-specific `lifecycle` config (highest priority)
2. System defaults from `orchestration.toml` (fallback)

### 3.3 Update Staleness Detection Function

**File**: `migrations/20251122000001_update_staleness_detection_per_template.sql`

Update `detect_and_transition_stale_tasks()` to extract lifecycle config from the serialized `TaskTemplate` in the existing `configuration` JSONB column:

```sql
CREATE OR REPLACE FUNCTION detect_and_transition_stale_tasks(
    p_dry_run BOOLEAN DEFAULT true,
    p_batch_size INTEGER DEFAULT 100,
    p_default_waiting_deps_threshold INTEGER DEFAULT 60,
    p_default_waiting_retry_threshold INTEGER DEFAULT 30,
    p_default_steps_in_process_threshold INTEGER DEFAULT 30,
    p_default_task_max_lifetime_hours INTEGER DEFAULT 24
)
RETURNS TABLE(...) AS $$
BEGIN
    RETURN QUERY
    WITH stale_tasks AS (
        SELECT
            t.task_uuid,
            tns.name as namespace_name,
            nt.name as task_name,
            tt.to_state as current_state,
            EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as time_in_state_minutes,
            EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 as task_age_minutes,

            -- TAS-49: Extract lifecycle config from TaskTemplate in configuration JSONB
            -- TaskHandlerRegistry::register_task_template() serializes full TaskTemplate to configuration
            -- Lifecycle config is at: configuration->'lifecycle'->'field_name'
            CASE tt.to_state
                WHEN 'waiting_for_dependencies' THEN
                    COALESCE(
                        (nt.configuration->'lifecycle'->>'max_waiting_for_dependencies_minutes')::INTEGER,
                        p_default_waiting_deps_threshold
                    )
                WHEN 'waiting_for_retry' THEN
                    COALESCE(
                        (nt.configuration->'lifecycle'->>'max_waiting_for_retry_minutes')::INTEGER,
                        p_default_waiting_retry_threshold
                    )
                WHEN 'steps_in_process' THEN
                    COALESCE(
                        (nt.configuration->'lifecycle'->>'max_steps_in_process_minutes')::INTEGER,
                        p_default_steps_in_process_threshold
                    )
                ELSE 1440  -- 24 hours default
            END as threshold_minutes,

            -- Overall task max lifetime check
            COALESCE(
                (nt.configuration->'lifecycle'->>'max_duration_minutes')::INTEGER,
                p_default_task_max_lifetime_hours * 60
            ) as task_lifetime_threshold_minutes,

            -- TAS-49: Template-specific staleness action
            COALESCE(
                nt.configuration->'lifecycle'->>'staleness_action',
                'dlq'
            ) as staleness_action

        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
        WHERE tt.most_recent = true
          AND tt.to_state NOT IN ('complete', 'error', 'cancelled', 'resolved_manually')
          AND (
              EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 > threshold_minutes
              OR EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 > task_lifetime_threshold_minutes
          )
        LIMIT p_batch_size
    )
    -- ... rest of function with per-template action handling
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION detect_and_transition_stale_tasks IS
'TAS-49: Detect stale tasks using per-template lifecycle configuration.

Lifecycle Configuration Sources (in precedence order):
1. Template-specific: nt.configuration->''lifecycle''->''field_name''
2. System defaults: p_default_* parameters from orchestration.toml

Key Design: No separate lifecycle_config column needed!
- Full TaskTemplate serialized to configuration JSONB by TaskHandlerRegistry
- Lifecycle config is nested inside: configuration->''lifecycle''
- If template YAML omits lifecycle section, configuration->''lifecycle'' is NULL
- COALESCE falls back to system defaults from orchestration.toml

Example configuration JSONB structure:
{
  "name": "payment_processing",
  "namespace_name": "payments",
  "version": "2.0.0",
  "lifecycle": {
    "max_duration_minutes": 30,
    "max_waiting_for_dependencies_minutes": 10,
    "max_waiting_for_retry_minutes": 5,
    "max_steps_in_process_minutes": 20,
    "auto_fail_on_timeout": true,
    "auto_dlq_on_timeout": true,
    "staleness_action": "dlq"
  },
  "steps": [...]
}

If template omits lifecycle, configuration->''lifecycle'' is NULL and system defaults apply.';
```

### 3.4 Template Registration - No Changes Needed!

**Files**: No changes required to existing infrastructure

The lifecycle configuration is automatically handled by the existing template loading and registration infrastructure. Here's what happens:

#### Existing Infrastructure (Already Works!)

**1. YAML Loading** (`TaskTemplate::from_yaml()`)
```rust
// Already in tasker-shared/src/models/core/task_template.rs
impl TaskTemplate {
    pub fn from_yaml(yaml_str: &str) -> Result<Self, TaskerError> {
        // Serde automatically deserializes lifecycle field if present in YAML
        // If YAML omits lifecycle section, lifecycle field is None (due to #[serde(default)])
        serde_yaml::from_str(yaml_str)
            .map_err(|e| TaskerError::YamlError(e.to_string()))
    }
}
```

**2. Template Registration** (`TaskHandlerRegistry::register_task_template()`)
```rust
// Already in tasker-shared/src/registry/task_handler_registry.rs
impl TaskHandlerRegistry {
    pub async fn register_task_template(&self, template: &TaskTemplate) -> TaskerResult<()> {
        // Validate template
        template.validate()?;

        // Convert ENTIRE template (including lifecycle field) to JSON
        let configuration = serde_json::to_value(template)?;

        // Store in tasker_named_tasks.configuration JSONB
        // This includes lifecycle config automatically!
        NamedTask::create(&self.db_pool, NewNamedTask {
            name: template.name.clone(),
            namespace_name: template.namespace_name.clone(),
            version: template.version.clone(),
            configuration,  // ← Full TaskTemplate serialized here (includes lifecycle)
        }).await?;

        Ok(())
    }
}

// What gets stored in configuration JSONB:
// {
//   "name": "payment_processing",
//   "namespace_name": "payments",
//   "version": "2.0.0",
//   "lifecycle": { ... },  ← Automatically included if present in TaskTemplate
//   "steps": [ ... ],
//   "metadata": { ... },
//   ... all other TaskTemplate fields
// }
```

**3. Template Discovery** (`TaskTemplateManager::discover_and_register_templates()`)
```rust
// Already in tasker-worker/src/worker/task_template_manager.rs
impl TaskTemplateManager {
    pub async fn discover_and_register_templates(
        &self,
        config_directory: &Path,
    ) -> TaskerResult<TaskTemplateDiscoveryResult> {
        // Walk directory, find YAML files
        for yaml_file in find_yaml_files(config_directory) {
            let yaml_content = std::fs::read_to_string(&yaml_file)?;

            // Parse YAML to TaskTemplate (lifecycle included if present)
            let template = TaskTemplate::from_yaml(&yaml_content)?;

            // Register to database (full template serialized)
            self.registry.register_task_template(&template).await?;
        }

        Ok(discovery_result)
    }
}
```

#### What This Means for TAS-49

**Zero code changes needed!** Just add the lifecycle field to TaskTemplate struct (Section 3.1 ✅) and:

1. **YAML files** can optionally include lifecycle section
2. **from_yaml()** automatically deserializes lifecycle (or sets to None if omitted)
3. **register_task_template()** automatically serializes full TaskTemplate (including lifecycle) to configuration JSONB
4. **SQL functions** (Section 3.3) extract lifecycle from configuration->'lifecycle'

#### Example Flow

```yaml
# File: config/tasker/tasks/payment_processing.yaml
name: payment_processing
namespace_name: payments
version: 2.0.0

lifecycle:                    # ← Optional section
  max_duration_minutes: 30
  max_waiting_for_dependencies_minutes: 10

steps:
  - name: charge_card
    handler: CardCharger
```

```rust
// Automatic processing (no code changes needed):

// 1. YAML → TaskTemplate
let template = TaskTemplate::from_yaml(yaml_str)?;
// template.lifecycle = Some(LifecycleConfig { max_duration_minutes: Some(30), ... })

// 2. TaskTemplate → Database
registry.register_task_template(&template).await?;
// Stores in tasker_named_tasks.configuration:
// {
//   "name": "payment_processing",
//   "lifecycle": { "max_duration_minutes": 30, ... },
//   "steps": [ ... ]
// }

// 3. SQL reads from configuration JSONB
SELECT (configuration->'lifecycle'->>'max_duration_minutes')::INTEGER
FROM tasker_named_tasks
WHERE name = 'payment_processing';
-- Returns: 30
```

#### Migration Path

**No database migration needed!** The existing `tasker_named_tasks.configuration` JSONB column already holds the full TaskTemplate. We're just:

1. Adding an optional field to the Rust struct (Section 3.1)
2. Updating SQL to extract from a nested path (Section 3.3)
3. Documenting the YAML schema (Section 3.2)

**Verification**:

```bash
# After adding lifecycle to TaskTemplate struct:
# 1. Register a template with lifecycle config
cargo run --bin tasker-worker -- discover-templates

# 2. Check database
psql $DATABASE_URL -c "
  SELECT name, jsonb_pretty(configuration->'lifecycle')
  FROM tasker_named_tasks
  WHERE name = 'payment_processing';
"

# Expected output:
#       name        |           jsonb_pretty
# ------------------+----------------------------------
#  payment_processing | {
#                     |     "max_duration_minutes": 30,
#                     |     "max_waiting_for_dependencies_minutes": 10,
#                     |     ...
#                     | }
```

**Key Insight**: We discovered that adding a new database column (`lifecycle_config`) was unnecessary. The existing infrastructure already serializes the entire TaskTemplate struct to the `configuration` JSONB column. By adding the lifecycle field to TaskTemplate, it's automatically included in that serialization with zero additional code!

### Phase 3 Deliverables

- ✅ `lifecycle: Option<LifecycleConfig>` field added to `TaskTemplate` struct
- ✅ `LifecycleConfig` struct defined with optional fields and helper methods
- ✅ **No database migration needed** - leverages existing `configuration` JSONB column
- ✅ **No registration code changes** - existing `register_task_template()` handles serialization automatically
- ✅ Template YAML schema supports optional `lifecycle` section
- ✅ System defaults from `orchestration.toml` used when template doesn't specify lifecycle
- ✅ Staleness detection SQL updated to extract from `configuration->'lifecycle'` with COALESCE fallback
- ✅ YAML parsing handles missing lifecycle config gracefully via `#[serde(default)]`
- ✅ Example templates demonstrate both default and custom lifecycle configs
- ✅ Tests validate template-specific threshold application and default fallback
- ✅ Verification script for testing lifecycle config extraction from database

---

## Phase 4: DLQ Operations & Observability (Week 4)

### 4.1 DLQ API Endpoints (Investigation Tracking Only)

**File**: `tasker-shared/src/models/orchestration/dlq.rs` (NEW - DLQ domain types)

First, define the Rust enums that map to PostgreSQL enum types:

```rust
use serde::{Deserialize, Serialize};
use sqlx::Type;

/// DLQ investigation lifecycle status
///
/// Maps to PostgreSQL enum: dlq_resolution_status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "dlq_resolution_status", rename_all = "snake_case")]
pub enum DlqResolutionStatus {
    /// Investigation in progress
    Pending,
    /// Operator fixed problem steps, task progressed
    ManuallyResolved,
    /// Unfixable issue (bad template, data corruption)
    PermanentlyFailed,
    /// Investigation cancelled (duplicate, false positive)
    Cancelled,
}

impl std::fmt::Display for DlqResolutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::ManuallyResolved => write!(f, "manually_resolved"),
            Self::PermanentlyFailed => write!(f, "permanently_failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Reason a task was sent to DLQ
///
/// Maps to PostgreSQL enum: dlq_reason
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "dlq_reason", rename_all = "snake_case")]
pub enum DlqReason {
    /// Exceeded state timeout threshold
    StalenessTimeout,
    /// TAS-42 retry limit hit
    MaxRetriesExceeded,
    /// Circular dependency discovered
    DependencyCycleDetected,
    /// No worker available for extended period
    WorkerUnavailable,
    /// Operator manually sent to DLQ
    ManualDlq,
}

impl std::fmt::Display for DlqReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StalenessTimeout => write!(f, "staleness_timeout"),
            Self::MaxRetriesExceeded => write!(f, "max_retries_exceeded"),
            Self::DependencyCycleDetected => write!(f, "dependency_cycle_detected"),
            Self::WorkerUnavailable => write!(f, "worker_unavailable"),
            Self::ManualDlq => write!(f, "manual_dlq"),
        }
    }
}
```

**File**: `tasker-orchestration/src/web/handlers/dlq.rs` (NEW - API endpoints)

DLQ API focuses on investigation tracking, NOT task manipulation. Task resolution happens via existing step APIs.

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use sqlx::PgPool;
use tasker_shared::models::orchestration::dlq::{DlqResolutionStatus, DlqReason};

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
            dlq_reason as "dlq_reason: DlqReason",
            dlq_timestamp,
            resolution_status as "resolution_status: DlqResolutionStatus",
            resolution_timestamp,
            resolution_notes,
            resolved_by,
            created_at,
            updated_at
        FROM tasker_tasks_dlq
        WHERE resolution_status = COALESCE($1, resolution_status)
        ORDER BY dlq_timestamp DESC
        LIMIT $2
        OFFSET $3
        "#,
        params.resolution_status.map(|s| s as DlqResolutionStatus),
        params.limit.unwrap_or(50),
        params.offset.unwrap_or(0)
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(entries))
}

// GET /api/v1/dlq/{task_uuid} - Get DLQ entry with full task snapshot
pub async fn get_dlq_entry(
    State(pool): State<PgPool>,
    Path(task_uuid): Path<Uuid>,
) -> Result<Json<DlqEntryDetail>, (StatusCode, String)> {
    let entry = sqlx::query_as!(
        DlqEntryDetail,
        r#"
        SELECT
            dlq_entry_uuid,
            task_uuid,
            original_state,
            dlq_reason as "dlq_reason: DlqReason",
            dlq_timestamp,
            resolution_status as "resolution_status: DlqResolutionStatus",
            resolution_timestamp,
            resolution_notes,
            resolved_by,
            task_snapshot,
            metadata,
            created_at,
            updated_at
        FROM tasker_tasks_dlq
        WHERE task_uuid = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        task_uuid
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "DLQ entry not found".to_string()))?;

    Ok(Json(entry))
}

// PATCH /api/v1/dlq/{dlq_entry_uuid} - Update investigation status
// This endpoint tracks INVESTIGATION workflow, not task resolution
// Task resolution happens via step APIs (PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid})
pub async fn update_dlq_investigation(
    State(pool): State<PgPool>,
    Path(dlq_entry_uuid): Path<Uuid>,
    Json(payload): Json<UpdateInvestigationRequest>,
) -> Result<Json<UpdateInvestigationResponse>, (StatusCode, String)> {
    let updated = sqlx::query!(
        r#"
        UPDATE tasker_tasks_dlq
        SET resolution_status = COALESCE($2, resolution_status),
            resolution_timestamp = CASE WHEN $2 IS NOT NULL THEN NOW() ELSE resolution_timestamp END,
            resolution_notes = COALESCE($3, resolution_notes),
            resolved_by = COALESCE($4, resolved_by),
            metadata = COALESCE($5, metadata),
            updated_at = NOW()
        WHERE dlq_entry_uuid = $1
        RETURNING dlq_entry_uuid
        "#,
        dlq_entry_uuid,
        payload.resolution_status,
        payload.resolution_notes,
        payload.resolved_by,
        payload.metadata
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "DLQ entry not found".to_string()))?;

    Ok(Json(UpdateInvestigationResponse {
        success: true,
        message: "Investigation status updated".to_string(),
        dlq_entry_uuid: updated.dlq_entry_uuid,
    }))
}

// GET /api/v1/dlq/stats - DLQ statistics
pub async fn get_dlq_stats(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<DlqStats>>, (StatusCode, String)> {
    let stats = sqlx::query_as!(
        DlqStats,
        r#"
        SELECT
            dlq_reason as "dlq_reason: DlqReason",
            COUNT(*) as "total_entries!",
            COUNT(*) FILTER (WHERE resolution_status = 'pending') as "pending!",
            COUNT(*) FILTER (WHERE resolution_status = 'manually_resolved') as "manually_resolved!",
            COUNT(*) FILTER (WHERE resolution_status = 'permanently_failed') as "permanent_failures!",
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

// Request/Response types

#[derive(Deserialize)]
pub struct DlqListParams {
    resolution_status: Option<DlqResolutionStatus>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
pub struct DlqEntry {
    dlq_entry_uuid: Uuid,
    task_uuid: Uuid,
    original_state: String,
    dlq_reason: DlqReason,
    dlq_timestamp: chrono::DateTime<chrono::Utc>,
    resolution_status: DlqResolutionStatus,
    resolution_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    resolution_notes: Option<String>,
    resolved_by: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct DlqEntryDetail {
    dlq_entry_uuid: Uuid,
    task_uuid: Uuid,
    original_state: String,
    dlq_reason: DlqReason,
    dlq_timestamp: chrono::DateTime<chrono::Utc>,
    resolution_status: DlqResolutionStatus,
    resolution_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    resolution_notes: Option<String>,
    resolved_by: Option<String>,
    task_snapshot: Value,  // Full task + steps state for investigation
    metadata: Option<Value>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct UpdateInvestigationRequest {
    resolution_status: Option<DlqResolutionStatus>,
    resolution_notes: Option<String>,
    resolved_by: Option<String>,
    metadata: Option<Value>,
}

#[derive(Serialize)]
pub struct UpdateInvestigationResponse {
    success: bool,
    message: String,
    dlq_entry_uuid: Uuid,
}

#[derive(Serialize)]
pub struct DlqStats {
    dlq_reason: DlqReason,
    total_entries: i64,
    pending: i64,
    manually_resolved: i64,
    permanent_failures: i64,
    oldest_entry: Option<chrono::DateTime<chrono::Utc>>,
    newest_entry: Option<chrono::DateTime<chrono::Utc>>,
}
```

**Critical Design Notes**:

1. **No Task Manipulation**: DLQ API does NOT provide `requeue` endpoint
   - Task resolution happens at step level via existing APIs
   - DLQ tracks investigation workflow only

2. **Resolution Workflow**:
   ```
   1. Operator: GET /api/v1/dlq/{task_uuid} → review task_snapshot
   2. Operator: PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid} → fix problem steps
   3. Task state machine: Automatically progresses when steps fixed
   4. Operator: PATCH /api/v1/dlq/{dlq_entry_uuid} → update investigation status
   ```

3. **investigation_status vs task_state**: Independent concerns
   - Task state: Managed by TAS-41 state machine (orchestration layer)
   - DLQ resolution_status: Tracks operator investigation workflow

**Router Integration**: Add to `tasker-orchestration/src/web/routes.rs`:
```rust
.route("/api/v1/dlq", get(dlq::list_dlq_entries))
.route("/api/v1/dlq/:task_uuid", get(dlq::get_dlq_entry))
.route("/api/v1/dlq/:dlq_entry_uuid", patch(dlq::update_dlq_investigation))
.route("/api/v1/dlq/stats", get(dlq::get_dlq_stats))
```

### 4.2 Archive API Endpoints (Read-Only Access to Archived Tasks)

**File**: `tasker-orchestration/src/web/handlers/archive.rs` (NEW)

Provide read-only API access to archived tasks with automatic redirects from primary endpoints.

**Architecture Decision**: Separate `/v1/archive/*` endpoints with HTTP 301 redirects from primary `/v1/tasks/*` endpoints when tasks are archived.

```rust
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// Archive Task Endpoints (Read-Only)
// ============================================================================

/// GET /v1/archive/tasks/{task_uuid}
///
/// Retrieve archived task with full context
pub async fn get_archived_task(
    State(pool): State<PgPool>,
    Path(task_uuid): Path<Uuid>,
) -> Result<Json<ArchivedTask>, (StatusCode, String)> {
    let task = sqlx::query_as!(
        ArchivedTask,
        r#"
        SELECT
            task_uuid,
            named_task_uuid,
            context,
            priority,
            status,
            created_at,
            updated_at,
            archived_at
        FROM tasker_tasks_archive
        WHERE task_uuid = $1
        "#,
        task_uuid
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Archived task not found".to_string()))?;

    Ok(Json(task))
}

/// GET /v1/archive/tasks/{task_uuid}/workflow_steps
///
/// Retrieve archived workflow steps for a task
pub async fn get_archived_workflow_steps(
    State(pool): State<PgPool>,
    Path(task_uuid): Path<Uuid>,
) -> Result<Json<Vec<ArchivedWorkflowStep>>, (StatusCode, String)> {
    let steps = sqlx::query_as!(
        ArchivedWorkflowStep,
        r#"
        SELECT
            workflow_step_uuid,
            task_uuid,
            step_name,
            handler_name,
            status,
            retry_count,
            max_retries,
            created_at,
            updated_at,
            archived_at
        FROM tasker_workflow_steps_archive
        WHERE task_uuid = $1
        ORDER BY created_at ASC
        "#,
        task_uuid
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(steps))
}

/// GET /v1/archive/tasks/{task_uuid}/workflow_steps/{step_uuid}
///
/// Retrieve specific archived workflow step
pub async fn get_archived_workflow_step(
    State(pool): State<PgPool>,
    Path((task_uuid, step_uuid)): Path<(Uuid, Uuid)>,
) -> Result<Json<ArchivedWorkflowStep>, (StatusCode, String)> {
    let step = sqlx::query_as!(
        ArchivedWorkflowStep,
        r#"
        SELECT
            workflow_step_uuid,
            task_uuid,
            step_name,
            handler_name,
            status,
            retry_count,
            max_retries,
            created_at,
            updated_at,
            archived_at
        FROM tasker_workflow_steps_archive
        WHERE task_uuid = $1 AND workflow_step_uuid = $2
        "#,
        task_uuid,
        step_uuid
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Archived step not found".to_string()))?;

    Ok(Json(step))
}

/// GET /v1/archive/tasks/{task_uuid}/transitions
///
/// Retrieve archived task transitions (full state history)
pub async fn get_archived_task_transitions(
    State(pool): State<PgPool>,
    Path(task_uuid): Path<Uuid>,
) -> Result<Json<Vec<ArchivedTaskTransition>>, (StatusCode, String)> {
    let transitions = sqlx::query_as!(
        ArchivedTaskTransition,
        r#"
        SELECT
            task_transition_uuid,
            task_uuid,
            to_state,
            from_state,
            metadata,
            sort_key,
            created_at
        FROM tasker_task_transitions_archive
        WHERE task_uuid = $1
        ORDER BY sort_key ASC
        "#,
        task_uuid
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(transitions))
}

// ============================================================================
// Archive Helper Functions
// ============================================================================

/// Check if a task exists in the archive
///
/// Used by primary task endpoints to determine if they should redirect
pub async fn check_if_archived(pool: &PgPool, task_uuid: Uuid) -> Result<bool, sqlx::Error> {
    let exists = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM tasker_tasks_archive WHERE task_uuid = $1
        ) as "exists!"
        "#,
        task_uuid
    )
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

/// Create 301 redirect response to archive endpoint
pub fn redirect_to_archive(task_uuid: Uuid, path_suffix: &str) -> impl IntoResponse {
    let location = format!("/v1/archive/tasks/{}{}", task_uuid, path_suffix);

    (
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, location)],
        "Task has been archived. Redirecting to archive endpoint.",
    )
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Serialize)]
pub struct ArchivedTask {
    pub task_uuid: Uuid,
    pub named_task_uuid: Uuid,
    pub context: serde_json::Value,
    pub priority: i32,
    pub status: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub archived_at: Option<chrono::NaiveDateTime>,
}

#[derive(Serialize)]
pub struct ArchivedWorkflowStep {
    pub workflow_step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub step_name: String,
    pub handler_name: String,
    pub status: String,
    pub retry_count: i32,
    pub max_retries: Option<i32>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub archived_at: Option<chrono::NaiveDateTime>,
}

#[derive(Serialize)]
pub struct ArchivedTaskTransition {
    pub task_transition_uuid: Uuid,
    pub task_uuid: Uuid,
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub sort_key: i32,
    pub created_at: chrono::NaiveDateTime,
}
```

**File**: `tasker-orchestration/src/web/handlers/tasks.rs` (UPDATE - Add redirect logic)

Update existing task endpoints to check archive and redirect if needed:

```rust
use crate::web::handlers::archive::{check_if_archived, redirect_to_archive};

/// GET /v1/tasks/{task_uuid}
///
/// Retrieve task by UUID, with automatic redirect to archive if archived
pub async fn get_task(
    State(pool): State<PgPool>,
    Path(task_uuid): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Try to fetch from active tasks
    let task_result = sqlx::query_as!(
        Task,
        r#"SELECT * FROM tasker_tasks WHERE task_uuid = $1"#,
        task_uuid
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(task) = task_result {
        // Task found in active tasks
        return Ok(Json(task).into_response());
    }

    // Task not in active tasks - check archive
    let is_archived = check_if_archived(&pool, task_uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_archived {
        // Return 301 Moved Permanently with Location header
        Ok(redirect_to_archive(task_uuid, "").into_response())
    } else {
        // Task doesn't exist anywhere
        Err((StatusCode::NOT_FOUND, "Task not found".to_string()))
    }
}

/// GET /v1/tasks/{task_uuid}/workflow_steps
///
/// List workflow steps for a task, with redirect to archive if needed
pub async fn list_workflow_steps(
    State(pool): State<PgPool>,
    Path(task_uuid): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Try to fetch from active workflow_steps
    let steps = sqlx::query_as!(
        WorkflowStep,
        r#"SELECT * FROM tasker_workflow_steps WHERE task_uuid = $1"#,
        task_uuid
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !steps.is_empty() {
        // Steps found in active table
        return Ok(Json(steps).into_response());
    }

    // No steps in active table - check if task is archived
    let is_archived = check_if_archived(&pool, task_uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_archived {
        // Return 301 redirect
        Ok(redirect_to_archive(task_uuid, "/workflow_steps").into_response())
    } else {
        // Task doesn't exist
        Err((StatusCode::NOT_FOUND, "Task not found".to_string()))
    }
}

/// GET /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}
///
/// Get specific workflow step, with redirect to archive if needed
pub async fn get_workflow_step(
    State(pool): State<PgPool>,
    Path((task_uuid, step_uuid)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Try to fetch from active workflow_steps
    let step_result = sqlx::query_as!(
        WorkflowStep,
        r#"SELECT * FROM tasker_workflow_steps WHERE task_uuid = $1 AND workflow_step_uuid = $2"#,
        task_uuid,
        step_uuid
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(step) = step_result {
        return Ok(Json(step).into_response());
    }

    // Step not in active table - check if task is archived
    let is_archived = check_if_archived(&pool, task_uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if is_archived {
        // Return 301 redirect
        Ok(redirect_to_archive(task_uuid, &format!("/workflow_steps/{}", step_uuid)).into_response())
    } else {
        Err((StatusCode::NOT_FOUND, "Task or step not found".to_string()))
    }
}
```

**Router Integration**: Add archive routes to `tasker-orchestration/src/web/routes.rs`:

```rust
use crate::web::handlers::archive;

// Archive endpoints (read-only)
.route("/v1/archive/tasks/:task_uuid", get(archive::get_archived_task))
.route("/v1/archive/tasks/:task_uuid/workflow_steps", get(archive::get_archived_workflow_steps))
.route("/v1/archive/tasks/:task_uuid/workflow_steps/:step_uuid", get(archive::get_archived_workflow_step))
.route("/v1/archive/tasks/:task_uuid/transitions", get(archive::get_archived_task_transitions))
```

**Key Design Principles**:

1. **Archive endpoints are read-only**: No PATCH/DELETE operations allowed
2. **HTTP 301 Moved Permanently**: Correct semantic for "resource has moved to new location"
3. **Client transparency**: Most HTTP clients auto-follow redirects
4. **Performance**: Active task lookups don't query archive (single table lookup)
5. **Audit trail**: Archive includes full state history (transitions preserved)

**Client Behavior**:

```typescript
// Client code (JavaScript example)
const response = await fetch(`/v1/tasks/${taskUuid}`);

// If task is active: 200 OK with task data
// If task is archived: 301 Moved Permanently
//   → Most HTTP clients automatically follow redirect
//   → Client receives 200 OK from /v1/archive/tasks/{uuid}
// If task doesn't exist: 404 Not Found

// Client can also explicitly check archive:
const archivedTask = await fetch(`/v1/archive/tasks/${taskUuid}`);
```

**Benefits**:

- ✅ Clients retain task_uuid indefinitely (works across archival)
- ✅ No breaking changes to existing clients (redirects are transparent)
- ✅ Fast path for active tasks (no UNION query overhead)
- ✅ Clear separation for monitoring/permissions
- ✅ HTTP semantics correctly reflect resource state

### 4.3 Archival Background Service

**File**: `tasker-orchestration/src/orchestration/archival_service.rs` (NEW)

```rust
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, error};
use opentelemetry::KeyValue;
use tasker_shared::metrics::orchestration;

pub struct ArchivalService {
    pool: PgPool,
    config: ArchivalConfig,
}

#[derive(Debug, Clone)]
pub struct ArchivalStats {
    pub tasks_archived: i32,
    pub steps_archived: i32,
    pub transitions_archived: i32,
    pub execution_time_ms: i32,
}

impl ArchivalService {
    pub fn new(pool: PgPool, config: ArchivalConfig) -> Self {
        Self { pool, config }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(self.config.archive_interval_hours * 3600));

        loop {
            interval.tick().await;

            if !self.config.enabled {
                continue;
            }

            let start = std::time::Instant::now();
            match self.archive_tasks().await {
                Ok(stats) => {
                    info!(
                        tasks = stats.tasks_archived,
                        steps = stats.steps_archived,
                        transitions = stats.transitions_archived,
                        execution_ms = stats.execution_time_ms,
                        "Archival completed"
                    );

                    // Record archival metrics
                    let duration_ms = start.elapsed().as_millis() as f64;
                    let batch_labels = &[KeyValue::new("batch_size", self.config.archive_batch_size as i64)];
                    orchestration::archive_duration().record(duration_ms, batch_labels);

                    // Track archived tasks (would need to determine final_state from query)
                    let archive_labels = &[KeyValue::new("final_state", "complete")];
                    orchestration::tasks_archived_total().add(stats.tasks_archived as u64, archive_labels);
                }
                Err(e) => {
                    error!(error = %e, "Archival failed");
                    let error_labels = &[KeyValue::new("error_type", "database")];
                    orchestration::staleness_detection_errors_total().add(1, error_labels);
                }
            }
        }
    }

    async fn archive_tasks(&self) -> Result<ArchivalStats, sqlx::Error> {
        sqlx::query_as!(
            ArchivalStats,
            r#"
            SELECT
                tasks_archived,
                steps_archived,
                transitions_archived,
                execution_time_ms
            FROM archive_completed_tasks($1, $2, false)
            "#,
            self.config.retention_days,
            self.config.archive_batch_size
        )
        .fetch_one(&self.pool)
        .await
    }
}
```

**Integration**: Add to `OrchestrationCore::build()` alongside `StalenessDetector`

### 4.3 Dashboard SQL Views

**File**: `migrations/20251122000002_add_dlq_views.sql`

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
    array_agg(task_uuid ORDER BY minutes_in_state DESC) FILTER (WHERE minutes_in_state > 60) as top_stale_tasks
FROM current_states
GROUP BY current_state
ORDER BY stale_over_1h DESC;

-- Archive Statistics View
CREATE OR REPLACE VIEW v_archive_statistics AS
SELECT
    DATE_TRUNC('month', created_at) as month,
    COUNT(*) as tasks_archived,
    pg_size_pretty(pg_total_relation_size('tasker_tasks_archive')) as archive_size
FROM tasker_tasks_archive
GROUP BY month
ORDER BY month DESC;
```

### 4.4 Prometheus Alerting Rules

**File**: `config/alerts/dlq.yml` (NEW)

```yaml
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
        expr: rate(staleness_detector_errors[5m]) > 0
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

      - alert: HighStaleTaskRate
        expr: (staleness_detector_tasks_detected / (staleness_detector_tasks_detected + tasks_discovered)) > 0.05
        for: 30m
        labels:
          severity: warning
        annotations:
          summary: "{{ $value | humanizePercentage }} of tasks are becoming stale"
          description: "High staleness rate indicates configuration or infrastructure issues"
```

### Phase 4 Deliverables

- ✅ DLQ API endpoints implemented and tested
- ✅ Archival service runs on schedule
- ✅ Dashboard SQL views created
- ✅ Prometheus alerts configured
- ✅ End-to-end DLQ workflow validated
- ✅ API documentation generated
- ✅ Operational runbook created

---

## Testing Strategy

### Unit Tests

**File**: `tasker-orchestration/tests/dlq_tests.rs` (NEW)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_staleness_detection_respects_thresholds() {
        // Create tasks at various ages
        // Run staleness detection with specific thresholds
        // Assert only tasks exceeding threshold are detected
    }

    #[tokio::test]
    async fn test_dlq_requeue_increments_retry_counter() {
        // Move task to DLQ
        // Requeue from DLQ
        // Assert retry_count incremented
        // Assert task state back to Pending
    }

    #[tokio::test]
    async fn test_dlq_requeue_respects_max_retries() {
        // Move task to DLQ with retry_count = max
        // Attempt requeue
        // Assert task marked permanently_failed
    }

    #[tokio::test]
    async fn test_archive_only_archives_old_completed_tasks() {
        // Create mix of old/new, complete/active tasks
        // Run archival with retention_days=30
        // Assert only old completed tasks archived
        // Assert active tasks untouched
    }

    #[tokio::test]
    async fn test_per_template_thresholds_override_defaults() {
        // Create task with template-specific lifecycle config
        // Create task without lifecycle config (NULL in DB)
        // Run staleness detection
        // Assert template-specific threshold used vs system default
    }

    #[tokio::test]
    async fn test_missing_lifecycle_config_uses_system_defaults() {
        // Create template with lifecycle_config = NULL
        // Create task from this template
        // Wait until system default threshold exceeded
        // Run staleness detection
        // Assert task detected as stale based on system defaults
        // Verify system default values were used (check logs)
    }

    #[tokio::test]
    async fn test_partial_lifecycle_config_merges_with_defaults() {
        // Create template with only max_waiting_for_dependencies_minutes
        // Create task from this template
        // Verify override value used for waiting_for_dependencies
        // Verify system defaults used for other thresholds (retry, steps_in_process)
    }
}
```

### Integration Tests

**File**: `tasker-orchestration/tests/integration/dlq_workflow_test.rs` (NEW)

```rust
#[tokio::test]
async fn test_end_to_end_dlq_workflow() {
    let manager = DockerIntegrationManager::setup().await.unwrap();

    // 1. Create task that will become stale
    let task = create_stale_task(&manager.orchestration_client).await.unwrap();

    // 2. Wait for staleness threshold + detection interval
    tokio::time::sleep(Duration::from_secs(400)).await;

    // 3. Verify task moved to DLQ
    let dlq_entries = get_dlq_entries(&manager.pool).await.unwrap();
    assert!(dlq_entries.iter().any(|e| e.task_uuid == task.task_uuid));

    // 4. Requeue from DLQ
    requeue_from_dlq(&manager.pool, task.task_uuid).await.unwrap();

    // 5. Verify task back in Pending with boosted priority
    let task_state = get_task_state(&manager.pool, task.task_uuid).await.unwrap();
    assert_eq!(task_state.current_state, "pending");
    assert!(task_state.priority > task.priority);  // Priority boosted
}

#[tokio::test]
async fn test_archival_preserves_data_integrity() {
    let manager = DockerIntegrationManager::setup().await.unwrap();

    // 1. Create and complete old tasks
    let old_tasks = create_completed_tasks(&manager.pool, 100).await.unwrap();

    // 2. Run archival
    archive_completed_tasks(&manager.pool, 30).await.unwrap();

    // 3. Verify tasks in archive
    let archived = get_archived_tasks(&manager.pool).await.unwrap();
    assert_eq!(archived.len(), 100);

    // 4. Verify task data matches original
    for task in old_tasks {
        let archived_task = archived.iter()
            .find(|t| t.task_uuid == task.task_uuid)
            .unwrap();
        assert_eq!(archived_task.context, task.context);
    }
}
```

### Development Database Reset

Since this is pre-alpha work, developers can easily reset to a clean state:

```bash
# Drop and recreate test database
dropdb tasker_rust_test
createdb tasker_rust_test
psql tasker_rust_test -c "CREATE EXTENSION IF NOT EXISTS pgmq;"

# Run all migrations
cargo sqlx migrate run

# Verify schema
psql $DATABASE_URL -c "\d tasker_tasks_dlq"
psql $DATABASE_URL -c "\d tasker_tasks_archive"

echo "✅ Clean database ready for testing"
```

---

## Implementation Order

### Week 1: Foundation
**Goal**: Database schema, config consolidation, SQL functions

**Day 1-2**: Database schema
- Create DLQ and archive tables
- Add indexes and constraints
- Create validation functions
- Test with sample data

**Day 3-4**: Configuration
- Add DLQ sections to `config/tasker/base/orchestration.toml`
- Add Rust config types to `OrchestrationConfig`
- Refactor `get_next_ready_tasks()` to use config
- Validate config loading across environments

**Day 5**: SQL functions
- Implement `detect_and_transition_stale_tasks()`
- Implement `requeue_from_dlq()`
- Implement `archive_completed_tasks()`
- Test all functions with dry_run=true

### Week 2: Detection
**Goal**: Automatic staleness detection with testing

**Day 1-2**: StalenessDetector service
- Implement background service
- Add DLQ metrics to `orchestration.rs`
- Add to OrchestrationCore lifecycle
- Unit tests

**Day 3**: Testing and validation
- (Optional) Create CLI tool for manual testing
- Test staleness detection with various scenarios
- Validate dry-run mode
- Test with fresh database (drop/recreate)

**Day 4-5**: Integration testing
- End-to-end detection tests
- Concurrent operation tests
- Crash recovery tests
- Performance validation with realistic task loads

### Week 3: Customization
**Goal**: Per-template lifecycle configuration with system defaults

**Day 1**: Rust struct updates (SIMPLE!)
- Add `lifecycle: Option<LifecycleConfig>` field to `TaskTemplate` struct
- Add `LifecycleConfig` struct with optional fields and helper methods
- Add `#[serde(default)]` attribute for automatic YAML parsing
- Verify existing `from_yaml()` and `register_task_template()` handle new field automatically
- **Key realization**: No registration code changes needed!

**Day 2**: SQL function updates
- Update `detect_and_transition_stale_tasks()` to extract from `configuration->'lifecycle'`
- Add COALESCE fallback to system defaults passed as parameters
- Add SQL comments documenting JSONB structure
- Test SQL extraction with sample data (NULL lifecycle vs populated lifecycle)

**Day 3**: Template examples and documentation
- Create example YAML templates showing both approaches:
  - Templates with no lifecycle config (use system defaults)
  - Templates with partial lifecycle config (merge with defaults)
  - Templates with complete lifecycle config (override all)
- Update `docs/template-schema.md` with lifecycle section documentation
- Create verification script for database extraction testing

**Day 4-5**: Integration testing
- Unit tests for `LifecycleConfig::get_threshold()` fallback logic
- Integration tests for template registration and database serialization
- SQL tests for staleness detection with different lifecycle configs:
  - NULL lifecycle (system defaults)
  - Partial lifecycle (mixed template + defaults)
  - Complete lifecycle (all template values)
- Verify JSONB structure in database matches expectations
- Test template discovery and registration workflow end-to-end

### Week 4: Operations
**Goal**: DLQ APIs, archival, observability

**Day 1-2**: DLQ API
- Implement endpoints
- Add to router
- OpenAPI documentation
- API tests

**Day 3**: Archival service
- Implement background service
- Add to OrchestrationCore
- Scheduling logic
- Archival tests

**Day 4**: Observability
- Create SQL views
- Configure Prometheus alerts
- Create Grafana dashboard
- Operational runbook

**Day 5**: Final validation
- End-to-end workflow tests
- Performance benchmarking
- Documentation review
- Production readiness checklist

---

## Success Metrics

### Baseline (Pre-TAS-49)
- Stale task accumulation: Unbounded (tasks sit in waiting states forever)
- Manual DLQ interventions: None (no DLQ exists)
- Table growth rate: Unbounded (no archival)
- Development workflow: Manual database cleanup required

### Post-Implementation Targets

**Phase 1 (Foundation)**:
- ✅ All migrations run cleanly
- ✅ Configuration loads correctly in all environments
- ✅ SQL functions tested with dry_run=true

**Phase 2 (Detection)**:
- ✅ Stale task accumulation < 100 at any time (with 5min detection interval)
- ✅ Detection latency < 5s for 10k tasks
- ✅ Background service starts automatically with orchestration
- ✅ False positive rate < 1% (validated through integration tests)

**Phase 3 (Per-Template)**:
- ✅ Template-specific thresholds correctly applied
- ✅ Template registration supports lifecycle config
- ✅ Example templates documented and tested

**Phase 4 (Operations)**:
- ✅ DLQ API response time < 100ms (p95)
- ✅ DLQ requeue success rate > 95%
- ✅ Archival success rate > 99%
- ✅ Prometheus alerts firing correctly

**Overall System**:
- ✅ Automatic DLQ transition rate > 90%
- ✅ Manual DLQ interventions reduced by 80%
- ✅ Table growth rate controlled via archival
- ✅ No production incidents from DLQ implementation

---

## Risks & Mitigation

### Risk: TAS-48 threshold change breaks existing behavior
**Impact**: Different discovery behavior after consolidation
**Probability**: Low
**Mitigation**:
- Keep same default values (60min/30min)
- Add comprehensive config validation
- Test discovery behavior before/after
- Feature flag for gradual rollout

### Risk: Per-template config conflicts with global defaults
**Impact**: Unexpected staleness behavior
**Probability**: Medium
**Mitigation**:
- Clear precedence: template > namespace > global
- Validation prevents invalid configs
- Logging shows which threshold was applied
- Template-specific tests

### Risk: Archival without partitioning insufficient long-term
**Impact**: Table growth eventually becomes problem
**Probability**: Medium
**Mitigation**:
- Monitor table growth monthly
- Revisit partitioning if growth > 10M rows/year
- Document partitioning migration path
- Archival buys time for proper partitioning

### Risk: DLQ API security vulnerabilities
**Impact**: Unauthorized task manipulation
**Probability**: Low
**Mitigation**:
- Authentication/authorization on all endpoints
- Audit logging for all DLQ operations
- Rate limiting on requeue operations
- Input validation and SQL injection prevention

### Risk: Staleness detection too aggressive
**Impact**: False positives move legitimate long-running tasks to DLQ
**Probability**: Low
**Mitigation**:
- Conservative default thresholds (60min/30min from TAS-48)
- Per-template overrides for workflows with long SLAs
- Dry-run mode for threshold tuning
- Feature flag to disable automatic DLQ transition
- Monitor false positive rate via metrics
- Start with `dry_run = true` to validate thresholds

### Risk: Performance degradation from staleness detection
**Impact**: Periodic staleness checks impact database performance
**Probability**: Low
**Mitigation**:
- Indexed queries on task state and timestamps
- Batch processing with configurable limits (default: 100 tasks/run)
- Configurable detection interval (default: 5 minutes)
- Monitor query execution time via metrics
- Kill switch if detection latency exceeds threshold

---

## Deferred for Future Work

### Table Partitioning (TAS-49 Phase 4)
**Reason**: Complex migration, simple archival sufficient initially
**Revisit When**: Production table growth > 10M rows or query performance degrades
**Complexity**: High (4-6 weeks, requires migration strategy)

### Advanced DLQ Analytics
**Reason**: Basic DLQ sufficient for initial needs
**Future Features**:
- Pattern detection in DLQ entries
- Automatic root cause suggestions
- Integration with error tracking (Sentry, DataDog)
- ML-based staleness prediction

### Multi-Tier Archival (Hot/Warm/Cold)
**Reason**: Simple single-tier archival sufficient
**Future Features**:
- Hot: Last 30 days (PostgreSQL)
- Warm: 30-365 days (S3)
- Cold: > 1 year (Glacier)

### Cross-Namespace DLQ Policies
**Reason**: Global DLQ policies sufficient initially
**Future Features**:
- Per-namespace DLQ thresholds
- Namespace-specific retry strategies
- Priority inheritance rules

---

## Appendix: File Changes Summary

### New Files Created

**Migrations**:
- `migrations/20251115000000_add_dlq_tables.sql`
- `migrations/20251115000001_add_dlq_functions.sql`
- `migrations/20251122000001_update_staleness_detection_per_template.sql` (SQL function update only - no schema changes)
- `migrations/20251122000002_add_dlq_views.sql`

**Configuration**:
- `config/tasker/base/orchestration.toml` (updated with DLQ sections)
- `config/alerts/dlq.yml`

**Rust Source**:
- `tasker-shared/src/config/orchestration.rs` (updated with DLQ types)
- `tasker-shared/src/metrics/orchestration.rs` (updated with DLQ metrics)
- `tasker-shared/src/models/orchestration/dlq.rs` (NEW - DLQ enums: DlqResolutionStatus, DlqReason)
- `tasker-orchestration/src/orchestration/staleness_detector.rs`
- `tasker-orchestration/src/orchestration/archival_service.rs`
- `tasker-orchestration/src/web/handlers/dlq.rs`
- `tasker-orchestration/src/bin/staleness-detector.rs`

**Tests**:
- `tasker-orchestration/tests/dlq_tests.rs`
- `tasker-orchestration/tests/integration/dlq_workflow_test.rs`

**Documentation**:
- `docs/ticket-specs/TAS-49/plan.md` (this file)
- `docs/template-schema.md` (updated)
- `docs/operations/dlq-runbook.md` (new)

### Modified Files

**Configuration**:
- `tasker-shared/src/config/orchestration.rs` - Add DLQ configuration types to `OrchestrationConfig`

**Orchestration Bootstrap**:
- `tasker-orchestration/src/core.rs` - Add `StalenessDetector` and `ArchivalService`

**Models**:
- `tasker-shared/src/models/core/task_template.rs` - Add `lifecycle: Option<LifecycleConfig>` field and `LifecycleConfig` struct

**Existing Migrations** (refactored):
- `migrations/20251010000000_add_stale_task_discovery_fixes.sql` - Parameterize hardcoded thresholds

**Web Routes**:
- `tasker-orchestration/src/web/routes.rs` - Add DLQ endpoints

---

## Notes for Implementation

### Pre-Alpha Context
This is a greenfield pre-alpha project with no production dependencies. We can:
- ✅ Make breaking schema changes without migration path for existing users
- ✅ Refactor SQL functions directly without backward compatibility
- ✅ Change configuration structure aggressively
- ✅ Bulk-migrate existing data without gradual rollout

### Development Approach
- **Test-driven**: Write tests first, then implementation
- **Incremental**: Each day produces working, testable code
- **Observable**: Add metrics and logging from day 1
- **Documented**: Update docs as code evolves

### Code Review Checklist
- [ ] All SQL functions have `COMMENT ON` documentation
- [ ] All Rust public APIs have rustdoc
- [ ] All configuration has validation
- [ ] All metrics have descriptions
- [ ] All database changes have migrations
- [ ] All tests pass with `--all-features`
- [ ] Clippy warnings addressed
- [ ] No hardcoded values (use config)

---

## References

### Related Tickets
- **TAS-48**: Task Staleness Immediate Relief (prerequisite - completed)
  - Implemented staleness exclusion from discovery
  - Hardcoded 60min/30min thresholds (to be consolidated in TAS-49)
  - Priority decay to prevent infinite accumulation
- **TAS-41**: Richer Task States (`docs/states-and-lifecycles.md`)
  - 12-state task machine provides all DLQ transition states
  - Processor ownership tracking (audit-only after TAS-54)
  - Complete audit trail via transitions table
- **TAS-34**: Component-Based Configuration (`docs/configuration-management.md`)
  - 3-file architecture (common/orchestration/worker)
  - Environment-specific overrides
  - Type-safe validation
- **TAS-37**: Dynamic Orchestration with Finalization Claiming
  - Atomic finalization preventing race conditions
  - Auto-scaling executor pools
  - Context-aware health monitoring
- **TAS-42/TAS-57**: Exponential Backoff Verification
  - Retry logic with configurable delays
  - Backoff calculation consistency
  - Foundation for staleness thresholds
- **TAS-46**: Actor Pattern Implementation (`docs/actors.md`)
  - `StalenessDetectorActor` integration point
  - Message-based coordination
  - Lifecycle management hooks
- **TAS-54**: Processor Ownership Removal
  - Idempotency via state guards
  - Automatic stale task recovery
  - Simplified concurrent orchestration

### External Documentation
- **PostgreSQL Enums**: https://www.postgresql.org/docs/current/datatype-enum.html
- **PostgreSQL JSONB**: https://www.postgresql.org/docs/current/datatype-json.html
- **UUID v7 Specification**: https://datatracker.ietf.org/doc/html/draft-peabody-dispatch-new-uuid-format
- **PGMQ**: https://github.com/tembo-io/pgmq

### Codebase Documentation
- `docs/states-and-lifecycles.md` - Task/step state machines
- `docs/configuration-management.md` - Component-based TOML configuration
- `docs/actors.md` - Actor pattern architecture
- `docs/task-and-step-readiness-and-execution.md` - SQL function documentation
- `docs/template-schema.md` - TaskTemplate YAML format (updated with lifecycle config)

---

**End of Plan**

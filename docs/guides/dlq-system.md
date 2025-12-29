# Dead Letter Queue (DLQ) System Architecture

**Purpose**: Investigation tracking system for stuck, stale, or problematic tasks

**Last Updated**: 2025-11-01 (TAS-49 Phase 4 Complete)

---

## Executive Summary

The DLQ (Dead Letter Queue) system is an **investigation tracking system**, NOT a task manipulation layer.

**Key Principles**:
- DLQ tracks "why task is stuck" and "who investigated"
- Resolution happens at **step level** via step APIs
- No task-level "requeue" - fix the problem steps instead
- Steps carry their own retry, attempt, and state lifecycles independent of DLQ
- DLQ is for audit, visibility, and investigation only

**Architecture**: PostgreSQL-based system with:
- `tasker_tasks_dlq` table for investigation tracking
- 3 database views for monitoring and analysis
- 6 REST endpoints for operator interaction
- Background staleness detection service

---

## DLQ vs Step Resolution

### What DLQ Does

✅ **Investigation Tracking**:
- Record when and why task became stuck
- Capture complete task snapshot for debugging
- Track operator investigation workflow
- Provide visibility into systemic issues

✅ **Visibility and Monitoring**:
- Dashboard statistics by DLQ reason
- Prioritized investigation queue for triage
- Proactive staleness monitoring (before DLQ)
- Alerting integration for high-priority entries

### What DLQ Does NOT Do

❌ **Task Manipulation**:
- Does NOT retry failed steps
- Does NOT requeue tasks
- Does NOT modify step state
- Does NOT execute business logic

### Why This Separation Matters

**Steps are mutable** - Operators can:
- Manually resolve failed steps: `PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}`
- View step readiness status: `GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}`
- Check retry eligibility and dependency satisfaction
- Trigger next steps by completing blocked steps

**DLQ is immutable audit trail** - Operators should:
- Review task snapshot to understand what went wrong
- Use step endpoints to fix the underlying problem
- Update DLQ investigation status to track resolution
- Analyze DLQ patterns to prevent future occurrences

---

## DLQ Reasons

### staleness_timeout

**Definition**: Task exceeded state-specific staleness threshold

**States**:
- `waiting_for_dependencies` - Default 60 minutes
- `waiting_for_retry` - Default 30 minutes
- `steps_in_process` - Default 30 minutes

**Template Override**: Configure per-template thresholds:
```yaml
lifecycle:
  max_waiting_for_dependencies_minutes: 120
  max_waiting_for_retry_minutes: 45
  max_steps_in_process_minutes: 60
  max_duration_minutes: 1440  # 24 hours
```

**Resolution Pattern**:
1. Operator: `GET /v1/dlq/task/{task_uuid}` - Review task snapshot
2. Identify stuck steps: Check `current_state` in snapshot
3. Fix steps: `PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}`
4. Task state machine automatically progresses when steps fixed
5. Operator: `PATCH /v1/dlq/entry/{dlq_entry_uuid}` - Mark investigation resolved

**Prevention**: Use `/v1/dlq/staleness` endpoint for proactive monitoring

### max_retries_exceeded

**Definition**: Step exhausted all retry attempts and remains in Error state

**Resolution Pattern**:
1. Review step results: `GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}`
2. Analyze `last_failure_at` and error details
3. Fix underlying issue (infrastructure, data, etc.)
4. Manually resolve step: `PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}`
5. Update DLQ investigation status

### dependency_cycle_detected

**Definition**: Circular dependency detected in workflow step graph

**Resolution Pattern**:
1. Review task template configuration
2. Identify cycle in step dependencies
3. Update template to break cycle
4. Manually cancel affected tasks
5. Re-submit tasks with corrected template

### worker_unavailable

**Definition**: No worker available for task's namespace

**Resolution Pattern**:
1. Check worker service health
2. Verify namespace configuration
3. Scale worker capacity if needed
4. Tasks automatically progress when worker available

### manual_dlq

**Definition**: Operator manually sent task to DLQ for investigation

**Resolution Pattern**: Custom per-investigation

---

## Database Schema

### tasker_tasks_dlq Table

```sql
CREATE TABLE tasker_tasks_dlq (
    dlq_entry_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    task_uuid UUID NOT NULL UNIQUE,  -- One pending entry per task
    original_state VARCHAR(50) NOT NULL,
    dlq_reason dlq_reason NOT NULL,
    dlq_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    task_snapshot JSONB,  -- Complete task state for debugging
    resolution_status dlq_resolution_status NOT NULL DEFAULT 'pending',
    resolution_notes TEXT,
    resolved_at TIMESTAMPTZ,
    resolved_by VARCHAR(255),
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Unique constraint: Only one pending DLQ entry per task
CREATE UNIQUE INDEX idx_dlq_unique_pending_task
    ON tasker_tasks_dlq (task_uuid)
    WHERE resolution_status = 'pending';
```

**Key Fields**:
- `dlq_entry_uuid` - UUID v7 (time-ordered) for investigation tracking
- `task_uuid` - Foreign key to task (unique for pending entries)
- `original_state` - Task state when sent to DLQ
- `task_snapshot` - JSONB snapshot with debugging context
- `resolution_status` - Investigation workflow status

### Database Views

#### v_dlq_dashboard

**Purpose**: Aggregated statistics for monitoring dashboard

**Columns**:
- `dlq_reason` - Why tasks are in DLQ
- `total_entries` - Count of entries
- `pending`, `manually_resolved`, `permanent_failures`, `cancelled` - Breakdown by status
- `oldest_entry`, `newest_entry` - Time range
- `avg_resolution_time_minutes` - Average time to resolve

**Use Case**: High-level DLQ health monitoring

#### v_dlq_investigation_queue

**Purpose**: Prioritized queue for operator triage

**Columns**:
- Task and DLQ entry UUIDs
- `priority_score` - Composite score (base reason priority + age factor)
- `minutes_in_dlq` - How long entry has been pending
- Task metadata for context

**Ordering**: Priority score DESC (most urgent first)

**Use Case**: Operator dashboard showing "what to investigate next"

#### v_task_staleness_monitoring

**Purpose**: Proactive staleness monitoring BEFORE tasks hit DLQ

**Columns**:
- `task_uuid`, `namespace_name`, `task_name`
- `current_state`, `time_in_state_minutes`
- `staleness_threshold_minutes` - Threshold for this state
- `health_status` - healthy | warning | stale
- `priority` - Task priority for ordering

**Health Status Classification**:
- `healthy` - < 80% of threshold
- `warning` - 80-99% of threshold
- `stale` - ≥ 100% of threshold

**Use Case**: Alerting at 80% threshold to prevent DLQ entries

---

## REST API Endpoints

### 1. List DLQ Entries

```
GET /v1/dlq?resolution_status=pending&limit=50
```

**Purpose**: Browse DLQ entries with filtering

**Query Parameters**:
- `resolution_status` - Filter by status (optional)
- `limit` - Max entries (default: 50)
- `offset` - Pagination offset (default: 0)

**Response**: Array of `DlqEntry` objects

**Use Case**: General DLQ browsing and pagination

---

### 2. Get DLQ Entry with Task Snapshot

```
GET /v1/dlq/task/{task_uuid}
```

**Purpose**: Retrieve most recent DLQ entry for a task with complete snapshot

**Response**: `DlqEntry` with full `task_snapshot` JSONB

**Task Snapshot Contains**:
- Task UUID, namespace, name
- Current state and time in state
- Staleness threshold
- Task age and priority
- Template configuration
- Detection time

**Use Case**: Investigation starting point - "why is this task stuck?"

---

### 3. Update DLQ Investigation Status

```
PATCH /v1/dlq/entry/{dlq_entry_uuid}
```

**Purpose**: Track investigation workflow

**Request Body**:
```json
{
  "resolution_status": "manually_resolved",
  "resolution_notes": "Fixed by manually completing stuck step using step API",
  "resolved_by": "operator@example.com",
  "metadata": {
    "fixed_step_uuid": "...",
    "root_cause": "database connection timeout"
  }
}
```

**Use Case**: Document investigation findings and resolution

---

### 4. Get DLQ Statistics

```
GET /v1/dlq/stats
```

**Purpose**: Aggregated statistics for monitoring

**Response**: Statistics grouped by `dlq_reason`

**Use Case**: Dashboard metrics, identifying systemic issues

---

### 5. Get Investigation Queue

```
GET /v1/dlq/investigation-queue?limit=100
```

**Purpose**: Prioritized queue for operator triage

**Response**: Array of `DlqInvestigationQueueEntry` ordered by priority

**Priority Factors**:
- Base reason priority (staleness_timeout: 10, max_retries: 20, etc.)
- Age multiplier (older entries = higher priority)

**Use Case**: "What should I investigate next?"

---

### 6. Get Staleness Monitoring

```
GET /v1/dlq/staleness?limit=100
```

**Purpose**: Proactive monitoring BEFORE tasks hit DLQ

**Response**: Array of `StalenessMonitoring` with health status

**Ordering**: Stale first, then warning, then healthy

**Use Case**: Alerting and prevention

**Alert Integration**:
```bash
# Alert when warning count exceeds threshold
curl /v1/dlq/staleness | jq '[.[] | select(.health_status == "warning")] | length'
```

---

## Step Endpoints and Resolution Workflow

### Step Endpoints

#### 1. List Task Steps

```
GET /v1/tasks/{uuid}/workflow_steps
```

**Returns**: Array of steps with readiness status

**Key Fields**:
- `current_state` - Step state (pending, enqueued, in_progress, complete, error)
- `dependencies_satisfied` - Can step execute?
- `retry_eligible` - Can step retry?
- `ready_for_execution` - Ready to enqueue?
- `attempts` / `max_attempts` - Retry tracking
- `last_failure_at` - When step last failed
- `next_retry_at` - When step eligible for retry

**Use Case**: Understand task execution status

---

#### 2. Get Step Details

```
GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}
```

**Returns**: Single step with full readiness analysis

**Use Case**: Deep dive into specific step

---

#### 3. Manually Resolve Step

```
PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}
```

**Purpose**: Operator actions to handle stuck or failed steps

**Action Types**:

1. **ResetForRetry** - Reset attempt counter and return to pending for automatic retry:
```json
{
  "action_type": "reset_for_retry",
  "reset_by": "operator@example.com",
  "reason": "Database connection restored, resetting attempts"
}
```

2. **ResolveManually** - Mark step as manually resolved without results:
```json
{
  "action_type": "resolve_manually",
  "resolved_by": "operator@example.com",
  "reason": "Non-critical step, bypassing for workflow continuation"
}
```

3. **CompleteManually** - Complete step with execution results for dependent steps:
```json
{
  "action_type": "complete_manually",
  "completion_data": {
    "result": {
      "validated": true,
      "score": 95
    },
    "metadata": {
      "manually_verified": true,
      "verification_method": "manual_inspection"
    }
  },
  "reason": "Manual verification completed after infrastructure fix",
  "completed_by": "operator@example.com"
}
```

**Behavior by Action Type**:
- `reset_for_retry`: Clears attempt counter, transitions to `pending`, enables automatic retry
- `resolve_manually`: Transitions to `resolved_manually` (terminal state)
- `complete_manually`: Transitions to `complete` with results available for dependent steps

**Common Effects**:
- Triggers task state machine re-evaluation
- Task automatically discovers next ready steps
- Task progresses when all dependencies satisfied

**Use Case**: Unblock stuck workflow by fixing problem step

---

### Complete Resolution Workflow

#### Scenario: Task Stuck in waiting_for_dependencies

**1. Operator receives DLQ alert**

```bash
GET /v1/dlq/investigation-queue
# Response shows task_uuid: abc-123 with high priority
```

**2. Operator reviews task snapshot**

```bash
GET /v1/dlq/task/abc-123
# Response:
{
  "dlq_entry_uuid": "xyz-789",
  "task_uuid": "abc-123",
  "original_state": "waiting_for_dependencies",
  "dlq_reason": "staleness_timeout",
  "task_snapshot": {
    "task_uuid": "abc-123",
    "namespace": "order_processing",
    "task_name": "fulfill_order",
    "current_state": "error",
    "time_in_state_minutes": 65,
    "threshold_minutes": 60
  }
}
```

**3. Operator checks task steps**

```bash
GET /v1/tasks/abc-123/workflow_steps
# Response shows:
# step_1: complete
# step_2: error (blocked, max_attempts exceeded)
# step_3: waiting_for_dependencies (blocked by step_2)
```

**4. Operator investigates step_2 failure**

```bash
GET /v1/tasks/abc-123/workflow_steps/{step_2_uuid}
# Response shows last_failure_at and error details
# Root cause: database connection timeout
```

**5. Operator fixes infrastructure issue**

```bash
# Fix database connection pool configuration
# Verify database connectivity
```

**6. Operator chooses resolution strategy**

Option A: **Reset for retry** (infrastructure fixed, retry should work):
```bash
PATCH /v1/tasks/abc-123/workflow_steps/{step_2_uuid}
{
  "action_type": "reset_for_retry",
  "reset_by": "operator@example.com",
  "reason": "Database connection pool fixed, resetting attempts for automatic retry"
}
```

Option B: **Resolve manually** (bypass step entirely):
```bash
PATCH /v1/tasks/abc-123/workflow_steps/{step_2_uuid}
{
  "action_type": "resolve_manually",
  "resolved_by": "operator@example.com",
  "reason": "Non-critical validation step, bypassing"
}
```

Option C: **Complete manually** (provide results for dependent steps):
```bash
PATCH /v1/tasks/abc-123/workflow_steps/{step_2_uuid}
{
  "action_type": "complete_manually",
  "completion_data": {
    "result": {
      "validation_status": "passed",
      "score": 100
    },
    "metadata": {
      "manually_verified": true
    }
  },
  "reason": "Manual validation completed",
  "completed_by": "operator@example.com"
}
```

**7. Task state machine automatically progresses**

Outcome depends on action type chosen:

**If Option A (reset_for_retry)**:
- Step 2 → `pending` (attempts reset to 0)
- Automatic retry begins when dependencies satisfied
- Step 2 re-enqueued to worker
- If successful, workflow continues normally

**If Option B (resolve_manually)**:
- Step 2 → `resolved_manually` (terminal state)
- Step 3 dependencies satisfied (manual resolution counts as success)
- Task transitions: `error` → `enqueuing_steps`
- Step 3 enqueued to worker
- Task resumes normal execution

**If Option C (complete_manually)**:
- Step 2 → `complete` (with operator-provided results)
- Step 3 can consume results from completion_data
- Task transitions: `error` → `enqueuing_steps`
- Step 3 enqueued to worker with access to step 2 results
- Task resumes normal execution

**8. Operator updates DLQ investigation**

```bash
PATCH /v1/dlq/entry/xyz-789
{
  "resolution_status": "manually_resolved",
  "resolution_notes": "Fixed database connection pool configuration. Manually resolved step_2 to unblock workflow. Task resumed execution.",
  "resolved_by": "operator@example.com",
  "metadata": {
    "root_cause": "database_connection_timeout",
    "fixed_step_uuid": "{step_2_uuid}",
    "infrastructure_fix": "increased_connection_pool_size"
  }
}
```

---

## Step Retry and Attempt Lifecycles

### Step State Machine

**States**:
- `pending` - Initial state, awaiting dependencies
- `enqueued` - Sent to worker queue
- `in_progress` - Worker actively processing
- `enqueued_for_orchestration` - Result submitted, awaiting orchestration
- `complete` - Successfully finished
- `error` - Failed (may be retryable)
- `cancelled` - Manually cancelled
- `resolved_manually` - Operator intervention

### Retry Logic

**Configured per step in template**:
```yaml
retry:
  retryable: true
  max_attempts: 3
  backoff: exponential
  backoff_base_ms: 1000
  max_backoff_ms: 30000
```

**Retry Eligibility Criteria**:
1. `retryable: true` in configuration
2. `attempts < max_attempts`
3. Current state is `error`
4. `next_retry_at` timestamp has passed (backoff elapsed)

**Backoff Calculation**:
```
backoff_ms = min(backoff_base_ms * (2 ^ (attempts - 1)), max_backoff_ms)
```

Example (base=1000ms, max=30000ms):
- Attempt 1 fails → wait 1s
- Attempt 2 fails → wait 2s
- Attempt 3 fails → wait 4s

**SQL Function**: `get_step_readiness_status()` calculates `retry_eligible` and `next_retry_at`

### Attempt Tracking

**Fields** (on `tasker_workflow_steps` table):
- `attempts` - Current attempt count
- `max_attempts` - Configuration limit
- `last_attempted_at` - Timestamp of last execution
- `last_failure_at` - Timestamp of last failure

**Workflow**:
1. Step enqueued → `attempts++`
2. Step fails → Record `last_failure_at`, calculate `next_retry_at`
3. Backoff elapses → Step becomes `retry_eligible: true`
4. Orchestration discovers ready steps → Step re-enqueued
5. Repeat until success or `attempts >= max_attempts`

**Max Attempts Exceeded**:
- Step remains in `error` state
- `retry_eligible: false`
- Task transitions to `error` state
- May trigger DLQ entry with reason `max_retries_exceeded`

### Independence from DLQ

**Key Point**: Step retry logic is INDEPENDENT of DLQ

- Steps retry automatically based on configuration
- DLQ does NOT trigger retries
- DLQ does NOT modify retry counters
- DLQ is pure observation and investigation

**Why This Matters**:
- Retry logic is predictable and configuration-driven
- DLQ doesn't interfere with normal workflow execution
- Operators can manually resolve to bypass retry limits
- DLQ provides visibility into retry exhaustion patterns

---

## Staleness Detection

### Background Service

**Component**: `tasker-orchestration/src/orchestration/staleness_detector.rs`

**Configuration**:
```toml
[staleness_detection]
enabled = true
batch_size = 100
detection_interval_seconds = 300  # 5 minutes
```

**Operation**:
1. Timer triggers every 5 minutes
2. Calls `detect_and_transition_stale_tasks()` SQL function
3. Function identifies tasks exceeding thresholds
4. Creates DLQ entries for stale tasks
5. Transitions tasks to `error` state
6. Records OpenTelemetry metrics

### Staleness Thresholds

**Per-State Defaults** (configurable):
- `waiting_for_dependencies`: 60 minutes
- `waiting_for_retry`: 30 minutes
- `steps_in_process`: 30 minutes

**Per-Template Override**:
```yaml
lifecycle:
  max_waiting_for_dependencies_minutes: 120
  max_waiting_for_retry_minutes: 45
  max_steps_in_process_minutes: 60
```

**Precedence**: Template config > Global defaults

### Staleness SQL Function

**Function**: `detect_and_transition_stale_tasks()`

**Architecture**:
```
v_task_state_analysis (base view)
    │
    ├── get_stale_tasks_for_dlq() (discovery function)
    │       │
    │       └── detect_and_transition_stale_tasks() (main orchestration)
    │               ├── create_dlq_entry() (DLQ creation)
    │               └── transition_stale_task_to_error() (state transition)
```

**Performance Optimization**:
- Expensive joins happen ONCE in base view
- Discovery function filters stale tasks
- Main function processes results in loop
- LEFT JOIN anti-join pattern for excluding tasks with pending DLQ entries

**Output**: Returns `StalenessResult` records with:
- Task identification (UUID, namespace, name)
- State and timing information
- `action_taken` - What happened (enum: TransitionedToDlqAndError, MovedToDlqOnly, etc.)
- `moved_to_dlq` - Boolean
- `transition_success` - Boolean

---

## OpenTelemetry Metrics

### Metrics Exported

**Counters**:
- `tasker.dlq.entries_created.total` - DLQ entries created
- `tasker.staleness.tasks_detected.total` - Stale tasks detected
- `tasker.staleness.tasks_transitioned_to_error.total` - Tasks moved to Error
- `tasker.staleness.detection_runs.total` - Detection cycles

**Histograms**:
- `tasker.staleness.detection.duration` - Detection execution time (ms)
- `tasker.dlq.time_in_queue` - Time in DLQ before resolution (hours)

**Gauges**:
- `tasker.dlq.pending_investigations` - Current pending DLQ count

### Alert Examples

**Prometheus Alerting Rules**:

```yaml
# Alert on high pending investigations
- alert: HighPendingDLQInvestigations
  expr: tasker_dlq_pending_investigations > 50
  for: 15m
  labels:
    severity: warning
  annotations:
    summary: "High number of pending DLQ investigations ({{ $value }})"

# Alert on slow detection cycles
- alert: SlowStalenessDetection
  expr: tasker_staleness_detection_duration > 5000
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Staleness detection taking >5s ({{ $value }}ms)"

# Alert on high stale task rate
- alert: HighStalenessRate
  expr: rate(tasker_staleness_tasks_detected_total[5m]) > 10
  for: 10m
  labels:
    severity: critical
  annotations:
    summary: "High rate of stale task detection ({{ $value }}/sec)"
```

---

## CLI Usage Examples

The `tasker-cli` tool provides commands for managing workflow steps directly from the command line.

### List Workflow Steps

```bash
# List all steps for a task
tasker-cli task steps <TASK_UUID>

# Example output:
# ✓ Found 3 workflow steps:
#
#   Step: validate_input (01933d7c-...)
#     State: complete
#     Dependencies satisfied: true
#     Ready for execution: false
#     Attempts: 1/3
#
#   Step: process_order (01933d7c-...)
#     State: error
#     Dependencies satisfied: true
#     Ready for execution: false
#     Attempts: 3/3
#     ⚠ Retry eligible
```

### Get Step Details

```bash
# Get detailed information about a specific step
tasker-cli task step <TASK_UUID> <STEP_UUID>

# Example output:
# ✓ Step Details:
#
#   UUID: 01933d7c-...
#   Name: process_order
#   State: error
#   Dependencies satisfied: true
#   Ready for execution: false
#   Retry eligible: false
#   Attempts: 3/3
#   Last failure: 2025-11-02T14:23:45Z
```

### Reset Step for Retry

When infrastructure is fixed and you want to reset attempt counter:

```bash
tasker-cli task reset-step <TASK_UUID> <STEP_UUID> \
  --reason "Database connection pool increased" \
  --reset-by "ops-team@example.com"

# Example output:
# ✓ Step reset successfully!
#   New state: pending
#   Reason: Database connection pool increased
#   Reset by: ops-team@example.com
```

### Resolve Step Manually

When you want to bypass a non-critical step:

```bash
tasker-cli task resolve-step <TASK_UUID> <STEP_UUID> \
  --reason "Non-critical validation, bypassing" \
  --resolved-by "ops-team@example.com"

# Example output:
# ✓ Step resolved manually!
#   New state: resolved_manually
#   Reason: Non-critical validation, bypassing
#   Resolved by: ops-team@example.com
```

### Complete Step Manually with Results

When you've manually performed the step's work and need to provide results:

```bash
tasker-cli task complete-step <TASK_UUID> <STEP_UUID> \
  --result '{"validated": true, "score": 95}' \
  --metadata '{"verification_method": "manual_review"}' \
  --reason "Manual verification after infrastructure fix" \
  --completed-by "ops-team@example.com"

# Example output:
# ✓ Step completed manually with results!
#   New state: complete
#   Reason: Manual verification after infrastructure fix
#   Completed by: ops-team@example.com
```

**JSON Formatting Tips**:

```bash
# Use single quotes around JSON to avoid shell escaping issues
--result '{"key": "value"}'

# For complex JSON, use a heredoc or file
--result "$(cat <<'EOF'
{
  "validation_status": "passed",
  "checks": ["auth", "permissions", "rate_limit"],
  "score": 100
}
EOF
)"

# Or read from a file
--result "$(cat result.json)"
```

---

## Operational Runbooks

### Runbook 1: Investigating High DLQ Count

**Trigger**: `tasker_dlq_pending_investigations > 50`

**Steps**:

1. **Check DLQ dashboard**:
```bash
curl /v1/dlq/stats | jq
```

2. **Identify dominant reason**:
```json
{
  "dlq_reason": "staleness_timeout",
  "total_entries": 45,
  "pending": 45
}
```

3. **Get investigation queue**:
```bash
curl /v1/dlq/investigation-queue?limit=10 | jq
```

4. **Check staleness monitoring**:
```bash
curl /v1/dlq/staleness | jq '.[] | select(.health_status == "stale")'
```

5. **Identify patterns**:
- Common namespace?
- Common task template?
- Common time period?

6. **Take action**:
- Infrastructure issue? → Fix and manually resolve affected tasks
- Template misconfiguration? → Update template thresholds
- Worker unavailable? → Scale worker capacity
- Systemic dependency issue? → Investigate upstream systems

### Runbook 2: Proactive Staleness Prevention

**Trigger**: Regular monitoring (not incident-driven)

**Steps**:

1. **Monitor warning threshold**:
```bash
curl /v1/dlq/staleness | jq '[.[] | select(.health_status == "warning")] | length'
```

2. **Alert when warning count exceeds baseline**:
```bash
if [ $warning_count -gt 10 ]; then
  alert "High staleness warning count: $warning_count tasks at 80%+ threshold"
fi
```

3. **Investigate early**:
```bash
curl /v1/dlq/staleness | jq '.[] | select(.health_status == "warning") | {
  task_uuid,
  current_state,
  time_in_state_minutes,
  staleness_threshold_minutes,
  threshold_percentage: ((.time_in_state_minutes / .staleness_threshold_minutes) * 100)
}'
```

4. **Intervene before DLQ**:
- Check task steps for blockages
- Review dependencies
- Manually resolve if appropriate

---

## Best Practices

### For Operators

✅ **DO**:
- Use staleness monitoring for proactive prevention
- Document investigation findings in DLQ resolution notes
- Fix root causes, not just symptoms
- Update DLQ investigation status promptly
- Use step endpoints to resolve stuck workflows
- Monitor DLQ statistics for systemic patterns

❌ **DON'T**:
- Don't try to "requeue" from DLQ - fix the steps instead
- Don't ignore warning health status - investigate early
- Don't manually resolve steps without fixing root cause
- Don't leave DLQ investigations in pending status indefinitely

### For Developers

✅ **DO**:
- Configure appropriate staleness thresholds per template
- Make steps retryable with sensible backoff
- Implement idempotent step handlers
- Add defensive timeouts to prevent hanging
- Test workflows under failure scenarios

❌ **DON'T**:
- Don't set thresholds too low (causes false positives)
- Don't set thresholds too high (delays detection)
- Don't make all steps non-retryable
- Don't ignore DLQ patterns - they indicate design issues
- Don't rely on DLQ for normal workflow control flow

---

## Testing

### Test Coverage

**Unit Tests**: SQL function testing (17 tests)
- Staleness detection logic
- DLQ entry creation
- Threshold calculation with template overrides
- View query correctness

**Integration Tests**: Lifecycle testing (4 tests)
- Waiting for dependencies staleness (test_dlq_lifecycle_waiting_for_dependencies_staleness)
- Steps in process staleness (test_dlq_lifecycle_steps_in_process_staleness)
- Proactive monitoring with health status progression (test_dlq_lifecycle_proactive_monitoring)
- Complete investigation workflow (test_dlq_investigation_workflow)

**Metrics Tests**: OpenTelemetry integration (1 test)
- Staleness detection metrics recording
- DLQ investigation metrics recording
- Pending investigations gauge query

**Test Template**: `tests/fixtures/task_templates/rust/dlq_staleness_test.yaml`
- 2-step linear workflow
- 2-minute staleness thresholds for fast test execution
- Test-only template for lifecycle validation

**Performance**: All 22 tests complete in 0.95s (< 1s target)

---

## Implementation Notes

**File Locations**:
- Staleness detector: `tasker-orchestration/src/orchestration/staleness_detector.rs`
- DLQ models: `tasker-shared/src/models/orchestration/dlq.rs`
- SQL functions: `migrations/20251122000004_add_dlq_discovery_function.sql`
- Database views: `migrations/20251122000003_add_dlq_views.sql`

**Key Design Decisions**:
- Investigation tracking only - no task manipulation
- Step-level resolution via existing step endpoints
- Proactive monitoring at 80% threshold
- Template-specific threshold overrides
- Atomic DLQ entry creation with unique constraint
- Time-ordered UUID v7 for investigation tracking

---

## Future Enhancements

Potential improvements (not currently planned):

1. **DLQ Patterns Analysis**
   - Machine learning to identify systemic issues
   - Automated root cause suggestions
   - Pattern clustering by namespace/template

2. **Advanced Alerting**
   - Anomaly detection on staleness rates
   - Predictive DLQ entry forecasting
   - Correlation with infrastructure metrics

3. **Investigation Workflow**
   - Automated triage rules
   - Escalation policies
   - Integration with incident management systems

4. **Performance Optimization**
   - Materialized views for dashboard
   - Query result caching
   - Incremental staleness detection

---

**End of Documentation**

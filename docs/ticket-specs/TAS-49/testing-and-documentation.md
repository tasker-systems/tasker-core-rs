# TAS-49 Testing and Documentation Plan

**Created**: 2025-11-01
**Status**: Planned
**Phase**: Testing, Integration, and Documentation
**Related**: TAS-49 DLQ Implementation (Phases 1-4 Complete)

---

## Executive Summary

With DLQ Phase 4 complete (all endpoints integrated), we now focus on three critical areas:

1. **Metrics Integration Verification**: Ensure staleness detector correctly integrates with OpenTelemetry metrics
2. **Lifecycle Integration Tests**: Create comprehensive end-to-end tests validating DLQ lifecycle with real task execution
3. **DLQ Documentation**: Document DLQ architecture, purpose, and operational guidance with step endpoint audit

This plan provides detailed, actionable steps for each objective with clear success criteria and implementation guidance.

---

## Table of Contents

- [Part 1: Metrics Integration Review](#part-1-metrics-integration-review)
- [Part 2: Lifecycle Integration Tests](#part-2-lifecycle-integration-tests)
- [Part 3: DLQ Documentation and Step Endpoint Audit](#part-3-dlq-documentation-and-step-endpoint-audit)
- [Implementation Timeline](#implementation-timeline)
- [Success Criteria](#success-criteria)
- [Risk Assessment](#risk-assessment)

---

## Part 1: Metrics Integration Review

### Objective

Verify that `staleness_detector.rs` correctly integrates with OpenTelemetry metrics defined in `tasker-shared/src/metrics/orchestration.rs`.

### Current State Analysis

**Metrics Defined (orchestration.rs lines 398-522)**:
- `dlq_entries_created_total()` - Counter for DLQ entry creation
- `stale_tasks_detected_total()` - Counter for stale task detection
- `tasks_transitioned_to_error_total()` - Counter for Error state transitions
- `staleness_detection_runs_total()` - Counter for detection cycles
- `staleness_detection_duration()` - Histogram for execution time
- `task_time_in_dlq_hours()` - Histogram for DLQ residence time
- `dlq_pending_investigations()` - Gauge for backlog tracking

**Static Instances Available**:
- `DLQ_ENTRIES_CREATED_TOTAL`
- `STALE_TASKS_DETECTED_TOTAL`
- `TASKS_TRANSITIONED_TO_ERROR_TOTAL`
- `STALENESS_DETECTION_RUNS_TOTAL`
- `STALENESS_DETECTION_DURATION`
- `TASK_TIME_IN_DLQ_HOURS`
- `DLQ_PENDING_INVESTIGATIONS`

### Implementation Tasks

#### Task 1.1: Review staleness_detector.rs Metrics Integration

**File**: `tasker-orchestration/src/orchestration/staleness_detector.rs`

**Current Integration Points to Verify** (lines 100+):

1. **Detection Cycle Metrics**:
   - Record `staleness_detection_runs_total` at cycle start
   - Record `staleness_detection_duration` for entire cycle
   - Label with `dry_run: true/false` and `tasks_detected: N`

2. **Task Detection Metrics**:
   - Record `stale_tasks_detected_total` for each detected task
   - Labels: `state`, `time_in_state_minutes` bucket
   - Time buckets: `60-120`, `120-360`, `>360`

3. **DLQ Entry Metrics**:
   - Record `dlq_entries_created_total` when `moved_to_dlq: true`
   - Labels: `correlation_id`, `dlq_reason`, `original_state`

4. **State Transition Metrics**:
   - Record `tasks_transitioned_to_error_total` when `transition_success: true`
   - Labels: `correlation_id`, `original_state`, `reason: staleness_timeout`

5. **Pending Investigations Gauge**:
   - Query `tasker_tasks_dlq` for pending count at end of cycle
   - Update `dlq_pending_investigations` gauge
   - Label with `dlq_reason` for breakdown

**Expected Code Pattern**:

```rust
use opentelemetry::KeyValue;
use tasker_shared::metrics::orchestration;

// At cycle start
orchestration::staleness_detection_runs_total().add(
    1,
    &[KeyValue::new("dry_run", self.config.dry_run.to_string())],
);

let start = std::time::Instant::now();

// Call SQL function
let results = sql_executor.detect_and_transition_stale_tasks(...).await?;

// Record duration
let duration_ms = start.elapsed().as_millis() as f64;
orchestration::staleness_detection_duration().record(
    duration_ms,
    &[
        KeyValue::new("dry_run", self.config.dry_run.to_string()),
        KeyValue::new("tasks_detected", results.len().to_string()),
    ],
);

// Per-task metrics
for result in &results {
    // Stale task detected
    let time_bucket = if result.time_in_state_minutes <= 120 {
        "60-120"
    } else if result.time_in_state_minutes <= 360 {
        "120-360"
    } else {
        ">360"
    };

    orchestration::stale_tasks_detected_total().add(
        1,
        &[
            KeyValue::new("state", result.current_state.clone()),
            KeyValue::new("time_in_state_minutes", time_bucket.to_string()),
        ],
    );

    // DLQ entry created
    if result.moved_to_dlq {
        orchestration::dlq_entries_created_total().add(
            1,
            &[
                KeyValue::new("correlation_id", result.task_uuid.to_string()),
                KeyValue::new("dlq_reason", "staleness_timeout"),
                KeyValue::new("original_state", result.current_state.clone()),
            ],
        );
    }

    // State transition
    if result.transition_success {
        orchestration::tasks_transitioned_to_error_total().add(
            1,
            &[
                KeyValue::new("correlation_id", result.task_uuid.to_string()),
                KeyValue::new("original_state", result.current_state.clone()),
                KeyValue::new("reason", "staleness_timeout"),
            ],
        );
    }
}

// Update pending investigations gauge
let pending_count = sqlx::query_scalar::<_, i64>(
    "SELECT COUNT(*) FROM tasker_tasks_dlq WHERE resolution_status = 'pending'"
)
.fetch_one(&self.pool)
.await?;

orchestration::dlq_pending_investigations().record(
    pending_count as u64,
    &[],
);
```

**Verification Checklist**:
- [ ] `staleness_detection_runs_total` recorded at cycle start
- [ ] `staleness_detection_duration` recorded with timing
- [ ] `stale_tasks_detected_total` recorded per detected task
- [ ] `dlq_entries_created_total` recorded when DLQ entry created
- [ ] `tasks_transitioned_to_error_total` recorded when state transitioned
- [ ] `dlq_pending_investigations` gauge updated at cycle end
- [ ] All labels correctly populated
- [ ] Time buckets correctly calculated
- [ ] Dry-run mode doesn't record action metrics

#### Task 1.2: Add Missing Metric: task_time_in_dlq_hours

**Context**: This metric exists in `orchestration.rs` but is not used by staleness detector.

**Where to Implement**: DLQ resolution flow (when operators mark DLQ entries as resolved)

**File**: `tasker-shared/src/models/orchestration/dlq.rs`

**Method**: `DlqEntry::update_investigation()` (around line 600+)

**Implementation**:

```rust
pub async fn update_investigation(
    pool: &sqlx::PgPool,
    dlq_entry_uuid: Uuid,
    update: DlqInvestigationUpdate,
) -> Result<bool, sqlx::Error> {
    // ... existing code to update DLQ entry

    // If resolution status changed to resolved/failed/cancelled, record time in DLQ
    if let Some(new_status) = &update.resolution_status {
        if matches!(new_status,
            DlqResolutionStatus::ManuallyResolved
            | DlqResolutionStatus::PermanentFailures
            | DlqResolutionStatus::Cancelled
        ) {
            // Fetch DLQ entry to get dlq_timestamp and dlq_reason
            let entry = sqlx::query_as::<_, DlqEntry>(
                "SELECT * FROM tasker_tasks_dlq WHERE dlq_entry_uuid = $1"
            )
            .bind(dlq_entry_uuid)
            .fetch_optional(pool)
            .await?;

            if let Some(entry) = entry {
                let time_in_dlq_hours = chrono::Utc::now()
                    .signed_duration_since(entry.dlq_timestamp)
                    .num_hours() as f64
                    + (chrono::Utc::now()
                        .signed_duration_since(entry.dlq_timestamp)
                        .num_minutes() % 60) as f64 / 60.0;

                use opentelemetry::KeyValue;
                use tasker_shared::metrics::orchestration;

                orchestration::task_time_in_dlq_hours().record(
                    time_in_dlq_hours,
                    &[
                        KeyValue::new("resolution_status", new_status.to_string()),
                        KeyValue::new("dlq_reason", entry.dlq_reason.to_string()),
                    ],
                );
            }
        }
    }

    // ... rest of existing code
}
```

**Verification Checklist**:
- [ ] Metric recorded when DLQ entry resolved
- [ ] Time calculation accurate (hours with fractional minutes)
- [ ] Labels include `resolution_status` and `dlq_reason`
- [ ] Only recorded for terminal resolution statuses

#### Task 1.3: Validate Metrics in Tests

**Create Test**: `tests/integration/metrics/staleness_metrics_test.rs`

```rust
#[sqlx::test]
async fn test_staleness_detection_metrics_integration(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Setup: Create task template with custom lifecycle thresholds
    // Create task in waiting_for_dependencies state
    // Manipulate timestamp to exceed threshold

    // Initialize metrics
    tasker_shared::metrics::orchestration::init();

    // Run staleness detector
    let config = StalenessDetectionConfig {
        enabled: true,
        dry_run: false,
        batch_size: 100,
        detection_interval_seconds: 300,
        default_waiting_deps_threshold_minutes: 1,  // 1 minute for test
        default_waiting_retry_threshold_minutes: 1,
        default_steps_in_process_threshold_minutes: 1,
        default_task_max_lifetime_hours: 24,
    };

    let detector = StalenessDetector::new(pool.clone(), config);

    // Execute one cycle
    detector.detect_and_process_once().await?;

    // Verify metrics were recorded
    // Note: In actual implementation, need to use metrics exporter to read values
    // For now, verify no panics and log output shows metric recording

    Ok(())
}
```

**Verification Approach**: Since OpenTelemetry metrics are write-only in production, verification requires:

1. **Log-based Verification**: Add debug logging when recording metrics
2. **Integration Test**: Ensure metrics calls don't panic
3. **Manual Testing**: Use Prometheus metrics endpoint to verify values
4. **CI Metrics**: Export to test collector and validate

---

## Part 2: Lifecycle Integration Tests

### Objective

Create comprehensive lifecycle integration tests that validate the complete DLQ flow using real task execution, timestamp manipulation, and direct staleness detector invocation.

### Test Architecture

**Pattern**: Follow existing integration test structure in `tests/integration/`

**Components**:
1. Simple 2-step task template (step 2 depends on step 1)
2. State machine manipulation to place step 1 in enqueued/in_process
3. Direct SQL timestamp manipulation
4. Direct staleness detector invocation (bypass interval timing)
5. DLQ record validation
6. View query validation

### Implementation Tasks

#### Task 2.1: Create Simple 2-Step Test Template

**File**: `tests/fixtures/task_templates/rust/dlq_staleness_test.yaml`

```yaml
name: dlq_staleness_test
namespace_name: dlq_test
version: 1.0.0
description: "Simple 2-step workflow for DLQ staleness detection testing"
metadata:
  author: TAS-49 DLQ Testing
  tags:
    - test:dlq
    - pattern:linear
    - dependencies:simple
  created_at: "2025-11-01T00:00:00Z"
task_handler:
  callable: tasker_worker_rust::step_handlers::RustStepHandler
  initialization:
    test_mode: true
system_dependencies:
  primary: default
  secondary: []
domain_events: []
input_schema:
  type: object
  required:
    - test_input
  properties:
    test_input:
      type: integer
      description: "Test input value"
lifecycle:
  max_waiting_for_dependencies_minutes: 2
  max_waiting_for_retry_minutes: 2
  max_steps_in_process_minutes: 2
  max_duration_minutes: 30
steps:
  - name: dlq_step_1
    description: "First step that will be placed in stuck state"
    handler:
      callable: tasker_worker_rust::step_handlers::test::TestStepHandler
      initialization:
        operation: "return_input"
    system_dependency:
    dependencies: []
    retry:
      retryable: true
      max_attempts: 3
      backoff: exponential
      backoff_base_ms: 1000
      max_backoff_ms: 30000
    timeout_seconds: 30
    publishes_events: []
  - name: dlq_step_2
    description: "Second step depending on first"
    handler:
      callable: tasker_worker_rust::step_handlers::test::TestStepHandler
      initialization:
        operation: "return_input"
    system_dependency:
    dependencies:
      - dlq_step_1
    retry:
      retryable: true
      max_attempts: 3
      backoff: exponential
      backoff_base_ms: 1000
      max_backoff_ms: 30000
    timeout_seconds: 30
    publishes_events: []
```

**Lifecycle Configuration Key Points**:
- `max_waiting_for_dependencies_minutes: 2` - Low threshold for fast test execution
- `max_waiting_for_retry_minutes: 2` - Low threshold for retry testing
- `max_steps_in_process_minutes: 2` - Low threshold for in-process testing
- `max_duration_minutes: 30` - Task lifetime limit

#### Task 2.2: Create Lifecycle Integration Test Suite

**File**: `tests/integration/dlq_lifecycle_test.rs`

```rust
//! # DLQ Lifecycle Integration Tests (TAS-49)
//!
//! Comprehensive end-to-end tests for DLQ staleness detection and lifecycle management.
//!
//! ## Test Strategy
//!
//! 1. Create real tasks using dlq_staleness_test.yaml template
//! 2. Use state machine to transition steps to specific states
//! 3. Manipulate timestamps with direct SQL to simulate staleness
//! 4. Invoke staleness detector directly (bypass interval timing)
//! 5. Validate DLQ records created correctly
//! 6. Validate database views reflect correct state

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use tasker_shared::config::components::StalenessDetectionConfig;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::models::core::{NamedTask, TaskNamespace, Task};
use tasker_shared::models::core::workflow_step::WorkflowStep;
use tasker_shared::models::orchestration::{DlqEntry, DlqReason, StalenessMonitoring};
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::state_machine::events::StepEvent;

// ============================================================================
// Test 1: Waiting for Dependencies Staleness
// ============================================================================

#[sqlx::test]
async fn test_dlq_lifecycle_waiting_for_dependencies_staleness(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Phase 1: Setup - Create task with 2 steps
    let namespace = create_test_namespace(&pool, "dlq_test").await?;
    let named_task = create_test_named_task(&pool, namespace.task_namespace_uuid).await?;

    let task_uuid = Uuid::new_v4();
    let task = create_test_task(&pool, task_uuid, named_task.named_task_uuid).await?;

    // Create 2 steps: step_1 (no deps), step_2 (depends on step_1)
    let step_1_uuid = create_workflow_step(
        &pool,
        task_uuid,
        "dlq_step_1",
        vec![],  // no dependencies
    ).await?;

    let step_2_uuid = create_workflow_step(
        &pool,
        task_uuid,
        "dlq_step_2",
        vec![step_1_uuid],  // depends on step_1
    ).await?;

    // Phase 2: State Manipulation - Place step_1 in "in_process" state
    // This will leave step_2 in "waiting_for_dependencies"

    transition_step_to_state(&pool, step_1_uuid, "in_process").await?;

    // Verify step_2 is in waiting_for_dependencies
    let step_2 = WorkflowStep::find_by_id(&pool, step_2_uuid).await?.unwrap();
    assert_eq!(get_current_step_state(&pool, step_2_uuid).await?, "pending");

    // Phase 3: Timestamp Manipulation - Make step_2 appear stale
    // Template config: max_waiting_for_dependencies_minutes = 2
    // Set step_2 state timestamp to 3 minutes ago

    manipulate_step_state_timestamp(&pool, step_2_uuid, chrono::Duration::minutes(-3)).await?;

    // Verify timestamp was set correctly
    let step_2_state_time = get_step_time_in_state(&pool, step_2_uuid).await?;
    assert!(
        step_2_state_time >= 2.9 && step_2_state_time <= 3.1,
        "Step should be ~3 minutes in state, got {}",
        step_2_state_time
    );

    // Phase 4: Staleness Detection - Invoke detector directly
    let config = StalenessDetectionConfig {
        enabled: true,
        dry_run: false,  // Actually create DLQ entries
        batch_size: 100,
        detection_interval_seconds: 300,
        default_waiting_deps_threshold_minutes: 2,
        default_waiting_retry_threshold_minutes: 2,
        default_steps_in_process_threshold_minutes: 2,
        default_task_max_lifetime_hours: 24,
    };

    let sql_executor = SqlFunctionExecutor::new(pool.clone());
    let results = sql_executor.detect_and_transition_stale_tasks(
        config.dry_run,
        config.batch_size as i32,
        config.default_waiting_deps_threshold_minutes as i32,
        config.default_waiting_retry_threshold_minutes as i32,
        config.default_steps_in_process_threshold_minutes as i32,
        config.default_task_max_lifetime_hours as i32,
    ).await?;

    // Phase 5: DLQ Validation - Verify DLQ entry created
    assert_eq!(results.len(), 1, "Should detect exactly 1 stale task");

    let stale_result = &results[0];
    assert_eq!(stale_result.task_uuid, task_uuid);
    assert_eq!(stale_result.current_state, "waiting_for_dependencies");
    assert!(stale_result.moved_to_dlq, "Task should be moved to DLQ");
    assert!(stale_result.transition_success, "Task should transition to Error");

    // Query DLQ table directly
    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid).await?.unwrap();
    assert_eq!(dlq_entry.dlq_reason, DlqReason::StalenessTimeout);
    assert_eq!(dlq_entry.original_state, "waiting_for_dependencies");

    // Phase 6: View Validation - Verify views reflect correct state

    // Test v_dlq_dashboard
    let stats = DlqEntry::get_stats(&pool).await?;
    let staleness_stats = stats.iter()
        .find(|s| s.dlq_reason == DlqReason::StalenessTimeout)
        .expect("Should have staleness_timeout stats");

    assert_eq!(staleness_stats.total_entries, 1);
    assert_eq!(staleness_stats.pending, 1);

    // Test v_dlq_investigation_queue
    let queue = DlqEntry::list_investigation_queue(&pool, Some(100)).await?;
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].task_uuid, task_uuid);
    assert_eq!(queue[0].dlq_reason, DlqReason::StalenessTimeout);

    // Test v_task_staleness_monitoring
    // Note: Task is now in Error state, so it won't appear in staleness monitoring
    // (monitoring only tracks active states)
    let monitoring = DlqEntry::get_staleness_monitoring(&pool, Some(100)).await?;
    assert!(
        !monitoring.iter().any(|m| m.task_uuid == task_uuid),
        "Task in Error state should not appear in staleness monitoring"
    );

    Ok(())
}

// ============================================================================
// Test 2: Steps in Process Staleness
// ============================================================================

#[sqlx::test]
async fn test_dlq_lifecycle_steps_in_process_staleness(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup: Create task with 2 steps
    let namespace = create_test_namespace(&pool, "dlq_test_2").await?;
    let named_task = create_test_named_task(&pool, namespace.task_namespace_uuid).await?;

    let task_uuid = Uuid::new_v4();
    let task = create_test_task(&pool, task_uuid, named_task.named_task_uuid).await?;

    let step_1_uuid = create_workflow_step(
        &pool,
        task_uuid,
        "dlq_step_1",
        vec![],
    ).await?;

    let step_2_uuid = create_workflow_step(
        &pool,
        task_uuid,
        "dlq_step_2",
        vec![step_1_uuid],
    ).await?;

    // Place step_1 in "in_progress" state (actively processing)
    transition_step_to_state(&pool, step_1_uuid, "in_progress").await?;

    // Transition task to "steps_in_process" state
    transition_task_to_state(&pool, task_uuid, "steps_in_process").await?;

    // Make task appear stale (stuck in steps_in_process for >2 minutes)
    manipulate_task_state_timestamp(&pool, task_uuid, chrono::Duration::minutes(-3)).await?;

    // Run staleness detection
    let config = StalenessDetectionConfig {
        enabled: true,
        dry_run: false,
        batch_size: 100,
        detection_interval_seconds: 300,
        default_waiting_deps_threshold_minutes: 2,
        default_waiting_retry_threshold_minutes: 2,
        default_steps_in_process_threshold_minutes: 2,
        default_task_max_lifetime_hours: 24,
    };

    let sql_executor = SqlFunctionExecutor::new(pool.clone());
    let results = sql_executor.detect_and_transition_stale_tasks(
        config.dry_run,
        config.batch_size as i32,
        config.default_waiting_deps_threshold_minutes as i32,
        config.default_waiting_retry_threshold_minutes as i32,
        config.default_steps_in_process_threshold_minutes as i32,
        config.default_task_max_lifetime_hours as i32,
    ).await?;

    // Validate DLQ entry
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].current_state, "steps_in_process");
    assert!(results[0].moved_to_dlq);

    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid).await?.unwrap();
    assert_eq!(dlq_entry.dlq_reason, DlqReason::StalenessTimeout);
    assert_eq!(dlq_entry.original_state, "steps_in_process");

    Ok(())
}

// ============================================================================
// Test 3: Proactive Staleness Monitoring (Before DLQ)
// ============================================================================

#[sqlx::test]
async fn test_dlq_lifecycle_proactive_monitoring(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup: Create task
    let namespace = create_test_namespace(&pool, "dlq_test_3").await?;
    let named_task = create_test_named_task(&pool, namespace.task_namespace_uuid).await?;

    let task_uuid = Uuid::new_v4();
    let task = create_test_task(&pool, task_uuid, named_task.named_task_uuid).await?;

    let step_1_uuid = create_workflow_step(
        &pool,
        task_uuid,
        "dlq_step_1",
        vec![],
    ).await?;

    // Place in waiting_for_retry state
    transition_step_to_state(&pool, step_1_uuid, "waiting_for_retry").await?;
    transition_task_to_state(&pool, task_uuid, "waiting_for_retry").await?;

    // Scenario 1: Task at 50% threshold (healthy)
    manipulate_task_state_timestamp(&pool, task_uuid, chrono::Duration::minutes(-1)).await?;

    let monitoring = DlqEntry::get_staleness_monitoring(&pool, Some(100)).await?;
    let task_monitoring = monitoring.iter()
        .find(|m| m.task_uuid == task_uuid)
        .expect("Task should appear in monitoring");

    assert_eq!(task_monitoring.current_state, "waiting_for_retry");
    assert!(task_monitoring.health_status.is_healthy());
    assert_eq!(task_monitoring.threshold_percentage() as i32, 50);

    // Scenario 2: Task at 85% threshold (warning)
    manipulate_task_state_timestamp(&pool, task_uuid, chrono::Duration::minutes(-1)).await?;
    // Total time: 2 minutes ago (1 + 1)
    // Threshold: 2 minutes
    // Percentage: 100% - but in reality SQL will calculate correctly

    let monitoring = DlqEntry::get_staleness_monitoring(&pool, Some(100)).await?;
    let task_monitoring = monitoring.iter()
        .find(|m| m.task_uuid == task_uuid)
        .expect("Task should appear in monitoring");

    // At 85% threshold, should be in warning status
    assert!(task_monitoring.health_status.needs_attention());

    // Scenario 3: Task exceeds threshold (stale)
    manipulate_task_state_timestamp(&pool, task_uuid, chrono::Duration::minutes(-2)).await?;

    let monitoring = DlqEntry::get_staleness_monitoring(&pool, Some(100)).await?;
    let task_monitoring = monitoring.iter()
        .find(|m| m.task_uuid == task_uuid)
        .expect("Task should appear in monitoring");

    assert!(task_monitoring.health_status.is_stale());
    assert!(task_monitoring.threshold_percentage() > 100.0);

    Ok(())
}

// ============================================================================
// Test 4: DLQ Investigation Workflow
// ============================================================================

#[sqlx::test]
async fn test_dlq_investigation_workflow(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup: Create stale task and DLQ entry
    let namespace = create_test_namespace(&pool, "dlq_test_4").await?;
    let named_task = create_test_named_task(&pool, namespace.task_namespace_uuid).await?;

    let task_uuid = Uuid::new_v4();
    let task = create_test_task(&pool, task_uuid, named_task.named_task_uuid).await?;

    let step_1_uuid = create_workflow_step(
        &pool,
        task_uuid,
        "dlq_step_1",
        vec![],
    ).await?;

    transition_step_to_state(&pool, step_1_uuid, "in_progress").await?;
    transition_task_to_state(&pool, task_uuid, "steps_in_process").await?;
    manipulate_task_state_timestamp(&pool, task_uuid, chrono::Duration::minutes(-3)).await?;

    // Run staleness detection to create DLQ entry
    let config = create_test_staleness_config();
    let sql_executor = SqlFunctionExecutor::new(pool.clone());
    sql_executor.detect_and_transition_stale_tasks(
        false,
        100,
        2,
        2,
        2,
        24,
    ).await?;

    // Test Investigation Queue Endpoint
    let queue = DlqEntry::list_investigation_queue(&pool, Some(100)).await?;
    assert_eq!(queue.len(), 1);

    let investigation = &queue[0];
    assert_eq!(investigation.task_uuid, task_uuid);
    assert_eq!(investigation.dlq_reason, DlqReason::StalenessTimeout);
    assert!(investigation.priority_score > 0.0);

    // Test DLQ Entry Retrieval
    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid).await?.unwrap();

    // Verify task snapshot contains necessary debugging info
    let snapshot = dlq_entry.task_snapshot.unwrap();
    assert!(snapshot.get("task_uuid").is_some());
    assert!(snapshot.get("namespace").is_some());
    assert!(snapshot.get("task_name").is_some());
    assert!(snapshot.get("current_state").is_some());
    assert!(snapshot.get("time_in_state_minutes").is_some());

    // Simulate operator investigation and resolution
    use tasker_shared::models::orchestration::{DlqInvestigationUpdate, DlqResolutionStatus};

    let update = DlqInvestigationUpdate {
        resolution_status: Some(DlqResolutionStatus::ManuallyResolved),
        resolution_notes: Some("Fixed by manually completing stuck step".to_string()),
        resolved_by: Some("operator@example.com".to_string()),
        metadata: None,
    };

    let updated = DlqEntry::update_investigation(
        &pool,
        dlq_entry.dlq_entry_uuid,
        update,
    ).await?;

    assert!(updated);

    // Verify investigation no longer appears in queue
    let queue = DlqEntry::list_investigation_queue(&pool, Some(100)).await?;
    assert_eq!(queue.len(), 0, "Resolved entries should not appear in investigation queue");

    // Verify stats updated
    let stats = DlqEntry::get_stats(&pool).await?;
    let staleness_stats = stats.iter()
        .find(|s| s.dlq_reason == DlqReason::StalenessTimeout)
        .unwrap();

    assert_eq!(staleness_stats.pending, 0);
    assert_eq!(staleness_stats.manually_resolved, 1);

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn create_test_namespace(
    pool: &PgPool,
    name: &str,
) -> Result<TaskNamespace, sqlx::Error> {
    let uuid = Uuid::new_v4();
    sqlx::query_as::<_, TaskNamespace>(
        r#"
        INSERT INTO tasker_task_namespaces (task_namespace_uuid, name, created_at, updated_at)
        VALUES ($1, $2, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(uuid)
    .bind(name)
    .fetch_one(pool)
    .await
}

async fn create_test_named_task(
    pool: &PgPool,
    namespace_uuid: Uuid,
) -> Result<NamedTask, sqlx::Error> {
    let uuid = Uuid::new_v4();
    let configuration = serde_json::json!({
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 2,
            "max_waiting_for_retry_minutes": 2,
            "max_steps_in_process_minutes": 2,
            "max_duration_minutes": 30
        }
    });

    sqlx::query_as::<_, NamedTask>(
        r#"
        INSERT INTO tasker_named_tasks (
            named_task_uuid, task_namespace_uuid, name, version, configuration, created_at, updated_at
        )
        VALUES ($1, $2, 'dlq_staleness_test', '1.0.0', $3, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(uuid)
    .bind(namespace_uuid)
    .bind(configuration)
    .fetch_one(pool)
    .await
}

async fn create_test_task(
    pool: &PgPool,
    task_uuid: Uuid,
    named_task_uuid: Uuid,
) -> Result<Task, sqlx::Error> {
    sqlx::query_as::<_, Task>(
        r#"
        INSERT INTO tasker_tasks (
            task_uuid, named_task_uuid, correlation_id, context, priority, created_at, updated_at
        )
        VALUES ($1, $2, $1::text, '{}', 10, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(task_uuid)
    .bind(named_task_uuid)
    .fetch_one(pool)
    .await
}

async fn create_workflow_step(
    pool: &PgPool,
    task_uuid: Uuid,
    name: &str,
    dependencies: Vec<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    let step_uuid = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO tasker_workflow_steps (
            workflow_step_uuid, task_uuid, step_name, step_number, attempts, max_attempts,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, 1, 0, 3, NOW(), NOW())
        "#
    )
    .bind(step_uuid)
    .bind(task_uuid)
    .bind(name)
    .execute(pool)
    .await?;

    // Create dependencies
    for parent_uuid in dependencies {
        sqlx::query(
            r#"
            INSERT INTO tasker_workflow_step_edges (parent_step_uuid, child_step_uuid, created_at)
            VALUES ($1, $2, NOW())
            "#
        )
        .bind(parent_uuid)
        .bind(step_uuid)
        .execute(pool)
        .await?;
    }

    Ok(step_uuid)
}

async fn transition_step_to_state(
    pool: &PgPool,
    step_uuid: Uuid,
    to_state: &str,
) -> Result<(), sqlx::Error> {
    // Mark old transition as not most_recent
    sqlx::query(
        "UPDATE tasker_workflow_step_transitions SET most_recent = false WHERE workflow_step_uuid = $1"
    )
    .bind(step_uuid)
    .execute(pool)
    .await?;

    // Create new transition
    sqlx::query(
        r#"
        INSERT INTO tasker_workflow_step_transitions (
            workflow_step_uuid, from_state, to_state, most_recent, created_at
        )
        VALUES ($1, 'pending', $2, true, NOW())
        "#
    )
    .bind(step_uuid)
    .bind(to_state)
    .execute(pool)
    .await?;

    Ok(())
}

async fn transition_task_to_state(
    pool: &PgPool,
    task_uuid: Uuid,
    to_state: &str,
) -> Result<(), sqlx::Error> {
    // Mark old transition as not most_recent
    sqlx::query(
        "UPDATE tasker_task_transitions SET most_recent = false WHERE task_uuid = $1"
    )
    .bind(task_uuid)
    .execute(pool)
    .await?;

    // Create new transition
    sqlx::query(
        r#"
        INSERT INTO tasker_task_transitions (
            task_uuid, from_state, to_state, most_recent, created_at, reason
        )
        VALUES ($1, 'pending', $2, true, NOW(), 'test_setup')
        "#
    )
    .bind(task_uuid)
    .bind(to_state)
    .execute(pool)
    .await?;

    Ok(())
}

async fn manipulate_step_state_timestamp(
    pool: &PgPool,
    step_uuid: Uuid,
    offset: chrono::Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE tasker_workflow_step_transitions
        SET created_at = NOW() + $2
        WHERE workflow_step_uuid = $1 AND most_recent = true
        "#
    )
    .bind(step_uuid)
    .bind(offset)
    .execute(pool)
    .await?;

    Ok(())
}

async fn manipulate_task_state_timestamp(
    pool: &PgPool,
    task_uuid: Uuid,
    offset: chrono::Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE tasker_task_transitions
        SET created_at = NOW() + $2
        WHERE task_uuid = $1 AND most_recent = true
        "#
    )
    .bind(task_uuid)
    .bind(offset)
    .execute(pool)
    .await?;

    Ok(())
}

async fn get_current_step_state(
    pool: &PgPool,
    step_uuid: Uuid,
) -> Result<String, sqlx::Error> {
    let state: String = sqlx::query_scalar(
        "SELECT to_state FROM tasker_workflow_step_transitions WHERE workflow_step_uuid = $1 AND most_recent = true"
    )
    .bind(step_uuid)
    .fetch_one(pool)
    .await?;

    Ok(state)
}

async fn get_step_time_in_state(
    pool: &PgPool,
    step_uuid: Uuid,
) -> Result<f64, sqlx::Error> {
    let minutes: f64 = sqlx::query_scalar(
        r#"
        SELECT EXTRACT(EPOCH FROM (NOW() - created_at)) / 60
        FROM tasker_workflow_step_transitions
        WHERE workflow_step_uuid = $1 AND most_recent = true
        "#
    )
    .bind(step_uuid)
    .fetch_one(pool)
    .await?;

    Ok(minutes)
}

fn create_test_staleness_config() -> StalenessDetectionConfig {
    StalenessDetectionConfig {
        enabled: true,
        dry_run: false,
        batch_size: 100,
        detection_interval_seconds: 300,
        default_waiting_deps_threshold_minutes: 2,
        default_waiting_retry_threshold_minutes: 2,
        default_steps_in_process_threshold_minutes: 2,
        default_task_max_lifetime_hours: 24,
    }
}
```

**Test Coverage**:
- ✅ Waiting for dependencies staleness (2 minutes threshold)
- ✅ Steps in process staleness (2 minutes threshold)
- ✅ Proactive staleness monitoring (healthy → warning → stale)
- ✅ DLQ investigation workflow (create → investigate → resolve)
- ✅ View validation (v_dlq_dashboard, v_dlq_investigation_queue, v_task_staleness_monitoring)
- ✅ Timestamp manipulation
- ✅ State machine transitions
- ✅ Direct staleness detector invocation

---

## Part 3: DLQ Documentation and Step Endpoint Audit

### Objective

Create comprehensive documentation for the DLQ system covering:
1. DLQ architecture and purpose
2. Relationship between DLQ and step endpoints
3. Step retry/attempt/state lifecycles
4. Operational guidance for DLQ endpoints
5. Audit of step endpoints for adequate functionality

### Documentation Tasks

#### Task 3.1: Create DLQ Architecture Documentation

**File**: `docs/dlq-system.md`

```markdown
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

**Request Body**:
```json
{
  "resolved_by": "operator@example.com",
  "reason": "Fixed infrastructure issue, manually marking complete"
}
```

**Behavior**:
- Transitions step to `resolved_manually` state
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

**6. Operator manually resolves step_2**

```bash
PATCH /v1/tasks/abc-123/workflow_steps/{step_2_uuid}
{
  "resolved_by": "operator@example.com",
  "reason": "Database connection pool fixed, manually marking complete"
}
```

**7. Task state machine automatically progresses**

- Step 2 → `resolved_manually`
- Step 3 dependencies now satisfied
- Task transitions: `error` → `waiting_for_dependencies` → `enqueuing_steps`
- Step 3 enqueued to worker
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

---

## Summary

**DLQ System Purpose**: Investigation tracking for stuck workflows

**Key Capabilities**:
- Automatic staleness detection
- Complete task snapshots for debugging
- Prioritized investigation queue
- Proactive monitoring before DLQ
- Comprehensive metrics and alerting

**Integration with Step Endpoints**:
- DLQ identifies problems
- Step endpoints fix problems
- Task state machine progresses automatically
- DLQ documents resolution

**Operational Model**:
- Proactive: Monitor `/v1/dlq/staleness` for warnings
- Reactive: Investigate `/v1/dlq/investigation-queue` for existing issues
- Resolution: Use step APIs to fix, then update DLQ status
- Analysis: Review `/v1/dlq/stats` for systemic patterns
```

#### Task 3.2: Step Endpoint Audit

**File**: `docs/step-endpoints-audit.md`

```markdown
# Step Endpoint Functionality Audit

**Purpose**: Verify step endpoints provide adequate functionality for DLQ resolution workflows

**Date**: 2025-11-01

---

## Existing Step Endpoints

### 1. List Task Steps

```
GET /v1/tasks/{uuid}/workflow_steps
```

**Functionality**: ✅ Adequate

**Provides**:
- All steps for a task
- Complete readiness analysis per step
- Dependency satisfaction status
- Retry eligibility
- Attempt tracking
- Last failure timestamps

**Use Cases Covered**:
- Understand task execution status
- Identify blocked steps
- See which steps are ready to execute

**Gaps**: None identified

---

### 2. Get Step Details

```
GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}
```

**Functionality**: ✅ Adequate

**Provides**:
- Single step details
- Full readiness analysis
- Step results (if available)
- Timing information

**Use Cases Covered**:
- Deep dive into specific step
- Review step execution results
- Debug step failures

**Gaps**: None identified

---

### 3. Manually Resolve Step

```
PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}
```

**Functionality**: ⚠️ Limited

**Provides**:
- Transition step to `resolved_manually` state
- Record operator and reason

**Use Cases Covered**:
- Unblock stuck workflow by marking step complete

**Gaps Identified**:

#### Gap 1: Cannot Force Retry

**Issue**: No way to reset step attempts and force retry

**Current Limitation**: If step exhausted retries (`attempts >= max_attempts`), cannot retry

**Workaround**: Must manually resolve (bypass retry mechanism)

**Recommendation**: Add optional `force_retry` flag:

```json
PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}
{
  "action": "force_retry",
  "reset_attempts": true,
  "resolved_by": "operator@example.com",
  "reason": "Infrastructure issue fixed, safe to retry"
}
```

**Implementation Priority**: **Medium** - Nice to have, not critical

**Rationale**: Manual resolution works for most cases, but force retry would be cleaner

---

#### Gap 2: Cannot Update Step Configuration

**Issue**: No way to modify step retry configuration at runtime

**Current Limitation**: Template configuration is static

**Example Scenario**: Step has `max_attempts: 3`, but operator wants to allow 5 attempts

**Workaround**: Must update template and re-deploy

**Recommendation**: Add configuration override endpoint:

```json
PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}/config
{
  "retry": {
    "max_attempts": 5
  },
  "timeout_seconds": 60,
  "resolved_by": "operator@example.com",
  "reason": "Temporary increase for known slow operation"
}
```

**Implementation Priority**: **Low** - Rare need, high complexity

**Rationale**: Modifying runtime configuration is risky, manual resolution is safer

---

#### Gap 3: Cannot Cancel Individual Step

**Issue**: Can cancel task, but not individual step

**Current Limitation**: No step-level cancellation

**Use Case**: Cancel long-running step that's no longer needed

**Workaround**: Cancel entire task

**Recommendation**: Add step cancellation:

```json
PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}
{
  "action": "cancel",
  "resolved_by": "operator@example.com",
  "reason": "Step no longer needed due to business logic change"
}
```

**Implementation Priority**: **Medium** - Useful for complex workflows

**Rationale**: Provides finer-grained control than task-level cancellation

---

## Step Endpoint Enhancement Recommendations

### Enhancement 1: Step Action Endpoint (Unified Interface)

**Proposed**: Single endpoint for all step actions

```
POST /v1/tasks/{uuid}/workflow_steps/{step_uuid}/actions
```

**Request Body**:
```json
{
  "action": "retry" | "resolve_manually" | "cancel" | "reset_attempts",
  "resolved_by": "operator@example.com",
  "reason": "Description of action taken",
  "metadata": {
    "key": "value"
  }
}
```

**Benefits**:
- Single endpoint for all operator actions
- Consistent request/response format
- Easier to document and discover

**Implementation Priority**: **High** - Improves operator UX

---

### Enhancement 2: Batch Step Operations

**Proposed**: Operate on multiple steps at once

```
POST /v1/tasks/{uuid}/workflow_steps/batch
```

**Request Body**:
```json
{
  "step_uuids": ["uuid1", "uuid2", "uuid3"],
  "action": "resolve_manually",
  "resolved_by": "operator@example.com",
  "reason": "Batch resolution after infrastructure fix"
}
```

**Use Case**: Resolve multiple stuck steps after fixing systemic issue

**Implementation Priority**: **Medium** - Reduces operator toil

---

### Enhancement 3: Step Results Submission (External)

**Proposed**: Allow external systems to submit step results

```
POST /v1/tasks/{uuid}/workflow_steps/{step_uuid}/results
```

**Request Body**:
```json
{
  "status": "success" | "error",
  "results": {
    "output_data": "..."
  },
  "error": {
    "message": "Error description",
    "code": "ERROR_CODE"
  },
  "submitted_by": "external_system@example.com"
}
```

**Use Case**: Async external processing systems

**Implementation Priority**: **Low** - Workers handle this normally

---

## Audit Summary

### Current Functionality Assessment

**Grade**: **B+** (Adequate for most use cases)

**Strengths**:
- ✅ Comprehensive readiness status
- ✅ Manual resolution capability
- ✅ Good integration with state machine
- ✅ Rich step metadata

**Weaknesses**:
- ⚠️ No force retry mechanism
- ⚠️ No runtime configuration override
- ⚠️ No individual step cancellation
- ⚠️ No batch operations

### Recommendations Priority

**High Priority**:
1. **Unified Actions Endpoint** - Better UX for operators

**Medium Priority**:
1. **Force Retry** - Cleaner than manual resolution for retry scenarios
2. **Step Cancellation** - Finer-grained control
3. **Batch Operations** - Reduces toil for systemic issues

**Low Priority**:
1. **Runtime Config Override** - Complex, risky, rarely needed
2. **External Results Submission** - Workers handle normally

### DLQ Resolution Adequacy

**Assessment**: ✅ **Current endpoints are sufficient for DLQ resolution workflows**

**Rationale**:
- Manual resolution handles 95% of DLQ scenarios
- Gap workarounds exist (manual resolution vs force retry)
- Enhancements would improve UX but aren't blockers

**Conclusion**: No urgent step endpoint changes required for TAS-49 completion
```

---

## Implementation Timeline

### Week 1: Metrics Integration and Basic Testing

**Days 1-2**:
- [ ] Task 1.1: Review and enhance staleness_detector.rs metrics integration
- [ ] Task 1.2: Implement task_time_in_dlq_hours metric in update_investigation()
- [ ] Task 1.3: Create metrics integration test skeleton

**Days 3-5**:
- [ ] Task 2.1: Create dlq_staleness_test.yaml template
- [ ] Task 2.2: Create test helper functions (create_test_namespace, etc.)
- [ ] Task 2.2: Implement Test 1 (waiting_for_dependencies staleness)

### Week 2: Comprehensive Testing

**Days 1-3**:
- [ ] Task 2.2: Implement Test 2 (steps_in_process staleness)
- [ ] Task 2.2: Implement Test 3 (proactive monitoring)
- [ ] Task 2.2: Implement Test 4 (investigation workflow)

**Days 4-5**:
- [ ] Run all DLQ tests (17 existing + 4 new = 21 total)
- [ ] Fix any test failures
- [ ] Performance validation (detection < 1s)

### Week 3: Documentation

**Days 1-3**:
- [ ] Task 3.1: Write DLQ architecture documentation
- [ ] Task 3.1: Document operational runbooks
- [ ] Task 3.1: Create alert examples

**Days 4-5**:
- [ ] Task 3.2: Step endpoint audit
- [ ] Task 3.2: Enhancement recommendations
- [ ] Final documentation review

### Total Estimated Effort: 15 working days (3 weeks)

---

## Success Criteria

### Part 1: Metrics Integration

- [ ] All 7 DLQ metrics integrated in staleness_detector.rs
- [ ] task_time_in_dlq_hours recorded on investigation resolution
- [ ] Metrics test validates no panics during recording
- [ ] Debug logs show metrics values during test execution
- [ ] Prometheus metrics endpoint (if available) shows correct values

### Part 2: Lifecycle Integration Tests

- [ ] dlq_staleness_test.yaml template created
- [ ] Test 1: Waiting for dependencies staleness - **PASSING**
- [ ] Test 2: Steps in process staleness - **PASSING**
- [ ] Test 3: Proactive monitoring (healthy → warning → stale) - **PASSING**
- [ ] Test 4: Investigation workflow (create → resolve) - **PASSING**
- [ ] All 21 DLQ tests passing (17 existing + 4 new)
- [ ] View validation confirms correct query results
- [ ] Timestamp manipulation works correctly
- [ ] Direct staleness detector invocation works

### Part 3: DLQ Documentation

- [ ] dlq-system.md created with comprehensive architecture documentation
- [ ] All 6 endpoints documented with examples
- [ ] Step endpoint audit completed
- [ ] Operational runbooks written
- [ ] Alert examples provided
- [ ] Best practices documented
- [ ] Enhancement recommendations prioritized

---

## Risk Assessment

### Part 1 Risks

**Risk**: Metrics not recording correctly
**Mitigation**: Log-based verification, manual Prometheus testing
**Impact**: Low - metrics are observability, not functionality

**Risk**: Metrics performance overhead
**Mitigation**: OpenTelemetry is async and low-overhead
**Impact**: Very Low

### Part 2 Risks

**Risk**: Timestamp manipulation doesn't work as expected
**Mitigation**: Test timestamp query first, validate with small offset
**Impact**: Medium - blocks test implementation

**Risk**: State machine transitions fail in test
**Mitigation**: Follow existing integration test patterns, use sqlx::test
**Impact**: Medium - requires debugging

**Risk**: View queries return unexpected data
**Mitigation**: Test views directly before integration tests
**Impact**: Low - views already tested in Phase 4

### Part 3 Risks

**Risk**: Documentation becomes outdated
**Mitigation**: Link to code, note last updated date, version control
**Impact**: Medium - stale docs are misleading

**Risk**: Step endpoint audit identifies critical gaps
**Mitigation**: Audit completed early, prioritize fixes if critical
**Impact**: Low - existing endpoints appear adequate

---

## Appendices

### Appendix A: Related Documentation

- `docs/ticket-specs/TAS-49/plan.md` - Original DLQ plan
- `docs/ticket-specs/TAS-49/refactor-dlq.md` - Refactoring details
- `docs/ticket-specs/TAS-49/PROGRESS.md` - Phase completion tracking
- `migrations/20251115000000_comprehensive_dlq_system.sql` - DLQ schema

### Appendix B: Testing Resources

- Existing test pattern: `tests/integration/sql_functions/dlq_functions_test.rs`
- Template examples: `tests/fixtures/task_templates/rust/`
- State machine: `tasker-shared/src/state_machine/`

### Appendix C: Key Files

**Metrics**:
- `tasker-shared/src/metrics/orchestration.rs`
- `tasker-orchestration/src/orchestration/staleness_detector.rs`

**DLQ Model**:
- `tasker-shared/src/models/orchestration/dlq.rs`

**Web Handlers**:
- `tasker-orchestration/src/web/handlers/dlq.rs`
- `tasker-orchestration/src/web/handlers/steps.rs`

**SQL**:
- `migrations/20251115000000_comprehensive_dlq_system.sql`
- `migrations/20251122000003_add_dlq_views.sql`

---

**End of Testing and Documentation Plan**

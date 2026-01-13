# TAS-89: Action Items

**Status**: In Progress
**Updated**: 2026-01-12

---

## Critical Priority (Fix Before Production)

### 1. Remove Vestigial `orchestration_api_reachable` Field
**Crate**: tasker-worker
**Type**: Design smell removal

Workers communicate with orchestration **exclusively via PGMQ queues** - there is no HTTP API communication. This field:
- Checks connectivity to something that doesn't exist
- Is hardcoded to `true` (meaningless)
- Is used in `is_healthy()` calculation (incorrect requirement)

**Files to modify**:
- `tasker-worker/src/health.rs` - Remove field from `WorkerHealthStatus`
- `tasker-worker/src/worker/core.rs` - Stop setting field in `get_health()`
- `tasker-worker/src/worker/services/worker_status/service.rs` - Stop setting field
- `tasker-worker/src/worker/services/health/service.rs` - Remove from `is_healthy` logic
- Update tests

**Effort**: Low (field removal)

---

### 2. Implement Real `check_event_system()` Health Check
**Crate**: tasker-worker
**Location**: `worker/services/health/service.rs:389-401`

Currently returns hardcoded `"healthy"` with zero actual checks. Should verify:
- Event publisher connectivity
- Subscriber registration status

**Effort**: Medium

---

### 3. Implement `publish_task_initialized_event()`
**Crate**: tasker-orchestration
**Location**: `orchestration/lifecycle/task_initialization/service.rs:340-347`

Called on every task initialization but does nothing (returns `Ok(())`). Events are never published.

**Effort**: Medium (depends on EventPublisher interface)

---

## High Priority

### 4. Fix `request_queue_size` to Return Actual Value
**Crate**: tasker-orchestration
**Location**: `orchestration/lifecycle/task_request_processor.rs:299`

Currently hardcoded to `-1`. Should query actual PGMQ queue depth.

**Effort**: Low-Medium

---

### 5. Implement `pool_utilization` Monitoring
**Crate**: tasker-orchestration
**Location**: `web/handlers/analytics.rs:199`

Currently hardcoded to `0.0`. Should query connection pool metrics.

**Effort**: Medium

---

### 6. Consolidate Duplicate Health Services
**Crate**: tasker-worker

Two health services with different behavior:
- `HealthService` - More complete, real DB check
- `WorkerStatusService` - Less complete, hardcoded values

Pick one canonical implementation, deprecate the other.

**Effort**: Medium

---

## Medium Priority

### 7. Migrate `#[allow]` to `#[expect]` with Reasons
**Crates**: All three core crates
**Count**: ~43 instances total

Per TAS-58 linting standards. Low effort, improves auditability.

| Crate | Count |
|-------|-------|
| tasker-shared | 14 |
| tasker-orchestration | 11 |
| tasker-worker | 18 |

---

### 8. Calculate Real `execution_duration` in Error Context
**Crate**: tasker-orchestration
**Location**: `orchestration/error_handling_service.rs:288`

Currently hardcoded to `Duration::from_secs(0)`. Affects error classification.

---

### 9. Track Actual `active_processors` Count
**Crate**: tasker-orchestration
**Location**: `orchestration/command_processor.rs:1135`

Currently hardcoded to `1`. Health endpoint reports wrong capacity.

---

### 10. Remove Orphaned Guard Structs
**Crate**: tasker-shared
**Location**: `state_machine/guards.rs:282-352`

Three guard structs from TAS-41 that are never called:
- `StepCanBeEnqueuedForOrchestrationGuard`
- `StepCanBeCompletedFromOrchestrationGuard`
- `StepCanBeFailedFromOrchestrationGuard`

---

## Low Priority

### 11. Add Missing Module Documentation Headers
**Crate**: tasker-shared
**Count**: ~20 files

Files lacking `//!` module docs (82% coverage currently).

---

### 12. Implement Event Field Type Validation
**Crate**: tasker-orchestration
**Location**: `orchestration/system_events.rs:243`

TODO stub - no type checking on event fields.

---

### 13. Complete Unified Event Coordinator
**Crate**: tasker-orchestration
**Location**: `orchestration/event_systems/unified_event_coordinator.rs`

Placeholder with TODO comments - TAS-61 Phase 4 deferred work.

---

## Testing Gaps to Address

### Add Negative Health Check Tests
Current tests don't verify health endpoints detect actual failures:

```rust
// Example test needed:
#[tokio::test]
async fn readiness_fails_when_database_unreachable() {
    let worker = start_worker_with_broken_db();
    let response = worker.get("/health/ready").await;
    assert_eq!(response.status(), 503);
}
```

---

## Summary

| Priority | Count | Effort |
|----------|-------|--------|
| Critical | 3 | Low-Medium |
| High | 3 | Medium |
| Medium | 6 | Low-Medium |
| Low | 3 | Low |
| **Total** | **15** | |

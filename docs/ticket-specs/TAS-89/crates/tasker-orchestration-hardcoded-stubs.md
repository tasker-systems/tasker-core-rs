# TAS-89: Hardcoded Values & Stub Analysis - tasker-orchestration

**Analysis Date**: 2026-01-12
**Risk Level**: HIGH

---

## Executive Summary

Found **9 hardcoded stubs/incomplete implementations**, of which **7 are actively used in production code paths**. These are silent bugs that pass tests but return fake data.

**Critical Issues**:
1. `publish_task_initialized_event()` - Called on every task but does nothing
2. `request_queue_size` - Always returns -1 (exposed to monitoring)
3. `pool_utilization` - Always returns 0.0 (exposed to analytics API)

---

## Critical Findings

### 1. Task Initialization Events Never Published

**Location**: `orchestration/lifecycle/task_initialization/service.rs:340-347`

```rust
async fn publish_task_initialized_event(
    &self,
    _task_uuid: Uuid,
    _step_count: usize,
    _task_name: &str,
) -> Result<(), TaskInitializationError> {
    // TODO: Implement event publishing once EventPublisher interface is finalized
    Ok(())
}
```

**Call Path**:
```
TaskInitializationService::process()
    └─ publish_task_initialized_event()  // Called on EVERY task
        └─ Does nothing, returns Ok(())
```

**Impact**:
- Task initialization events are NEVER published
- Event-driven workflows depending on these events will fail silently
- No logging, no warning - just silent success

**Severity**: 🔴 CRITICAL

---

### 2. Request Queue Size Always -1

**Location**: `orchestration/lifecycle/task_request_processor.rs:299`

```rust
pub async fn get_statistics(&self) -> TaskerResult<TaskRequestProcessorStats> {
    // TODO: Implement queue_size method in PgmqClient
    Ok(TaskRequestProcessorStats {
        request_queue_size: -1,  // HARDCODED
        request_queue_name: self.config.request_queue_name.clone(),
    })
}
```

**Impact**:
- Monitoring dashboards show -1 for queue depth
- Cannot detect request backlog
- Capacity planning impossible

**Severity**: 🔴 HIGH

---

### 3. Pool Utilization Always 0%

**Location**: `web/handlers/analytics.rs:199`

```rust
// TODO: Consider fetching from separate connection pool monitoring if needed
let pool_utilization = 0.0;  // HARDCODED
```

**Call Path**:
```
GET /api/analytics
    └─ get_analytics()
        └─ ResourceUtilization { pool_utilization: 0.0 }
```

**Impact**:
- Analytics API reports 0% pool utilization always
- Database saturation invisible to monitoring
- Capacity alerts will never fire

**Severity**: 🔴 HIGH

---

## High Severity Findings

### 4. Execution Duration Always 0

**Location**: `orchestration/error_handling_service.rs:288`

```rust
Ok(ErrorContext {
    execution_duration: std::time::Duration::from_secs(0),  // HARDCODED
    // ... other fields
})
```

**Impact**:
- Error classifier receives wrong duration
- Retry timeout calculations may be incorrect
- Cannot distinguish slow vs fast failures

**Severity**: 🟡 MEDIUM

---

### 5. Active Processors Always 1

**Location**: `orchestration/command_processor.rs:1135`

```rust
Ok(SystemHealth {
    active_processors: 1,  // TODO: Track actual active processors
    // ...
})
```

**Impact**:
- Health endpoint reports wrong processor count
- Load balancing decisions based on wrong data

**Severity**: 🟡 MEDIUM

---

### 6. Event Field Validation Stub

**Location**: `orchestration/system_events.rs:243`

```rust
// TODO: Add type validation for field values
Ok(())
```

**Impact**: Events with wrong field types pass validation.

**Severity**: 🟡 MEDIUM

---

## Unimplemented Features (TODO)

| Location | Feature | Status |
|----------|---------|--------|
| `unified_event_coordinator.rs:146` | Unified event notification | Placeholder only |
| `unified_event_coordinator.rs:311` | Notification handling | Not implemented |

---

## Call Graph Analysis

### What's Actually Exposed to Users

| Endpoint | Affected Field | Current Value | Should Be |
|----------|----------------|---------------|-----------|
| `/api/analytics` | pool_utilization | 0.0 | Real % |
| Stats API | request_queue_size | -1 | Actual count |
| `/health` | active_processors | 1 | Actual count |
| Internal | execution_duration | 0s | Real duration |

---

## Summary

| Category | Count | Actually Used? |
|----------|-------|----------------|
| Hardcoded returns | 4 | YES - API exposed |
| Empty stub functions | 2 | YES - called in flows |
| TODO placeholders | 6 | Mixed |
| **Total Silent Bugs** | **7** | **YES** |

---

## Action Items

### Priority 1 (Critical)
- [ ] **Implement `publish_task_initialized_event()`** - Events never published
- [ ] **Fix `request_queue_size`** - Return actual queue depth
- [ ] **Fix `pool_utilization`** - Query actual pool metrics

### Priority 2 (High)
- [ ] Calculate real `execution_duration` from timestamps
- [ ] Track actual `active_processors` count
- [ ] Implement event field type validation

### Priority 3 (Medium)
- [ ] Complete unified event coordinator
- [ ] Add integration tests that verify these values are real

---

## Testing Gap

These stubs pass tests because:
1. Tests don't verify the returned values are real
2. Tests only check that functions return without error
3. No integration tests simulate real monitoring scenarios

**Recommendation**: Add tests that:
- Verify `request_queue_size > 0` after enqueueing tasks
- Verify `pool_utilization > 0` under load
- Verify events are actually published (mock subscriber)

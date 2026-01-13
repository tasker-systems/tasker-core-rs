# TAS-89-2: Implement Real Worker Health Checks

**Parent**: TAS-89 (Codebase Evaluation)
**Type**: Bug Fix
**Priority**: High
**Effort**: 2-4 hours

---

## Summary

Worker health checks currently return hardcoded values, causing Kubernetes to route traffic to potentially broken workers. Implement real health checks that verify actual component connectivity.

---

## Problem Statement

### Current State (Broken)

| Check | Current Value | Reality |
|-------|---------------|---------|
| `database_connected` | Always `true` | No actual check performed |
| `check_event_system()` | Always "healthy" | No publisher/subscriber verification |

### Impact

- **Kubernetes probes** return "ready" even when components are down
- **Load balancers** route traffic to broken workers
- **Tasks get stuck** with no error indication
- **Monitoring shows green** when system is degraded

---

## Scope

### 1. Implement Real Database Connectivity Check

**Location**: `tasker-worker/src/worker/services/worker_status/service.rs:108`

**Current**:
```rust
database_connected: true, // TODO: Add actual DB connectivity check
```

**Target**:
```rust
database_connected: self.check_database_connectivity().await,
```

**Implementation**:
```rust
async fn check_database_connectivity(&self) -> bool {
    match sqlx::query("SELECT 1")
        .fetch_one(&self.pool)
        .await
    {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(error = %e, "Database connectivity check failed");
            false
        }
    }
}
```

---

### 2. Implement Real Event System Health Check

**Location**: `tasker-worker/src/worker/services/health/service.rs:389-401`

**Current**:
```rust
async fn check_event_system(&self) -> ComponentHealth {
    // TODO: Check event publisher/subscriber health
    ComponentHealth {
        name: "event_system".to_string(),
        status: "healthy".to_string(),
        message: Some("Event system available".to_string()),
        details: None,
    }
}
```

**Target**: Verify event publisher connectivity and subscriber registration.

**Implementation approach**:
1. Check if event publisher is connected
2. Check if subscriber registration is valid
3. Return degraded/unhealthy status if checks fail

---

### 3. Consolidate Duplicate Health Services (Optional)

**Issue**: Two health services exist with inconsistent behavior:
- `HealthService` - Has real DB check
- `WorkerStatusService` - Has hardcoded values

**Recommendation**: Ensure both use the same underlying checks, or deprecate one.

---

## Files to Modify

| File | Change |
|------|--------|
| `tasker-worker/src/worker/services/worker_status/service.rs` | Add real DB check |
| `tasker-worker/src/worker/services/health/service.rs` | Implement `check_event_system()` |
| `tasker-worker/src/health.rs` | May need updates to `is_healthy()` logic |

---

## Acceptance Criteria

- [ ] `database_connected` reflects actual database connectivity
- [ ] `check_event_system()` verifies publisher connectivity
- [ ] Health endpoints return 503 when database is unreachable
- [ ] Health endpoints return degraded when event system is down
- [ ] Tests simulate database failure and verify unhealthy response
- [ ] Tests simulate event system failure and verify degraded response

---

## Testing

### Unit Tests
```rust
#[tokio::test]
async fn database_check_returns_false_when_unreachable() {
    let service = create_service_with_broken_db();
    let status = service.get_status().await;
    assert!(!status.database_connected);
}

#[tokio::test]
async fn event_system_check_returns_unhealthy_when_broken() {
    let service = create_service_with_broken_events();
    let health = service.check_event_system().await;
    assert_ne!(health.status, "healthy");
}
```

### Integration Tests
```rust
#[tokio::test]
async fn readiness_probe_returns_503_when_database_down() {
    // Start worker with broken database
    let worker = start_worker_with_broken_db().await;

    let response = worker.get("/health/ready").await;
    assert_eq!(response.status(), 503);
}
```

---

## Risk Assessment

**Risk**: Medium
- Changes health check behavior (intentionally)
- May cause workers to report unhealthy that previously reported healthy
- This is **correct behavior** - surfacing real problems

**Mitigation**:
- Add logging when health checks fail
- Consider grace period for transient failures
- Document expected behavior change

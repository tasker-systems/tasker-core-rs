# TAS-89: Hardcoded Values & Stub Analysis - tasker-worker

**Analysis Date**: 2026-01-12
**Risk Level**: CRITICAL

---

## Executive Summary

Found **critical silent bugs** in health check code that is **exposed to Kubernetes probes**. The worker reports "healthy" and "ready" even when:
- It cannot reach the orchestration API
- The event system is broken
- Database connectivity is unknown

**These bugs could cause Kubernetes to route traffic to broken workers.**

---

## Critical Findings

### 1. Orchestration API Connectivity - HARDCODED TRUE

**Locations**:
- `worker/core.rs:659`
- `worker/services/worker_status/service.rs:109`

```rust
// worker/core.rs:659
// TODO: Could check actual API connectivity
orchestration_api_reachable: true,  // HARDCODED
```

**Call Path (Kubernetes Readiness Probe)**:
```
GET /health/ready
    └─ HealthService.readiness()
        └─ check_command_processor()
            └─ WorkerCore.get_health()
                └─ orchestration_api_reachable: true  // ALWAYS TRUE
```

**Impact**:
- Kubernetes readiness probe returns "ready" even if orchestration API is down
- Load balancer routes traffic to workers that can't process tasks
- Tasks get stuck with no error indication

**Severity**: 🔴 CRITICAL

---

### 2. Event System Health Check - STUB RETURNS "HEALTHY"

**Location**: `worker/services/health/service.rs:389-401`

```rust
async fn check_event_system(&self) -> ComponentHealth {
    // TODO: Check event publisher/subscriber health
    ComponentHealth {
        name: "event_system".to_string(),
        status: "healthy".to_string(),  // HARDCODED - NO CHECKS
        message: Some("Event system available".to_string()),
        details: None,
    }
}
```

**Call Path (Detailed Health Endpoint)**:
```
GET /health/detailed
    └─ HealthService.detailed_health()
        └─ check_event_system()
            └─ Returns "healthy" WITHOUT ANY CHECKS
```

**Impact**:
- Event router could be completely broken
- Health endpoint reports "Event system: healthy"
- Domain events fail silently while monitoring shows green

**Severity**: 🔴 CRITICAL

---

### 3. Database Connectivity - HARDCODED TRUE

**Location**: `worker/services/worker_status/service.rs:108`

```rust
// TODO: Add actual DB connectivity check
database_connected: true,  // HARDCODED
```

**Note**: `HealthService` has a real database check, but `WorkerStatusService` doesn't. Two different services, inconsistent behavior.

**Severity**: 🔴 HIGH (if WorkerStatusService is used anywhere)

---

## High Severity Findings

### 4. Duplicate Health Services with Different Behavior

The codebase has TWO health service implementations:

| Service | Database Check | API Check | Event Check |
|---------|----------------|-----------|-------------|
| `HealthService` | ✅ Real SQL | ❌ Hardcoded | ❌ Stub |
| `WorkerStatusService` | ❌ Hardcoded | ❌ Hardcoded | N/A |

**Risk**: Different code paths use different services, leading to inconsistent health reporting.

---

### 5. Pending Correlations Always 0

**Location**: `worker/services/worker_status/service.rs:143,153`

```rust
EventIntegrationStatus {
    pending_correlations: 0,  // HARDCODED - never tracked
    // ...
}
```

**Impact**: Cannot detect stuck event correlations.

**Severity**: 🟡 MEDIUM

---

## Call Graph Analysis

### Kubernetes Health Probe Exposure

```
┌─────────────────────────────────────────────────────────────┐
│  Kubernetes Probes                                          │
├─────────────────────────────────────────────────────────────┤
│  GET /health/ready  (readiness probe)                       │
│      └─ is_healthy() checks:                                │
│          ✅ database_connected (real check)                 │
│          ❌ orchestration_api_reachable (HARDCODED TRUE)    │
│                                                             │
│  GET /health/detailed  (monitoring)                         │
│      └─ Components checked:                                 │
│          ✅ database (real check)                           │
│          ✅ circuit_breakers (real check)                   │
│          ✅ queue (real check)                              │
│          ❌ event_system (STUB - always "healthy")          │
│          ⚠️  command_processor (uses WorkerCore.get_health) │
└─────────────────────────────────────────────────────────────┘
```

---

## Summary

| Category | Count | Exposed to K8s? | Risk |
|----------|-------|-----------------|------|
| Hardcoded `true` returns | 5 | YES | CRITICAL |
| Stub functions | 1 | YES | CRITICAL |
| Hardcoded `0` returns | 2 | Indirectly | MEDIUM |
| Dead code markers | 1 | No | LOW |

---

## Action Items

### Priority 1 (Critical - Before Production)

- [ ] **Implement `orchestration_api_reachable` check**
  - Add HTTP health check to orchestration API
  - Cache result with short TTL
  - Location: `worker/core.rs:659`

- [ ] **Implement `check_event_system()`**
  - Check event publisher connectivity
  - Check subscriber registration
  - Location: `worker/services/health/service.rs:389`

### Priority 2 (High)

- [ ] **Consolidate health services**
  - Pick one: `HealthService` or `WorkerStatusService`
  - Deprecate the other
  - Ensure consistent behavior

- [ ] **Add health check tests**
  - Test that simulates orchestration API failure
  - Verify health endpoint returns unhealthy
  - Test event system failure detection

### Priority 3 (Medium)

- [ ] Implement correlation tracking for `pending_correlations`
- [ ] Complete dead code cleanup (TAS-69 field)

---

## Testing Gap Analysis

### Why These Pass Tests

1. **No negative tests**: Tests don't simulate failures
2. **Mock everything**: Tests mock health checks instead of verifying real behavior
3. **Assert on structure, not values**: Tests check response shape, not content truth

### Recommended Tests

```rust
#[tokio::test]
async fn readiness_fails_when_orchestration_unreachable() {
    // Arrange: Start worker with unreachable orchestration URL
    let worker = start_worker_with_broken_orchestration();

    // Act: Check readiness
    let response = worker.get("/health/ready").await;

    // Assert: Should NOT be ready
    assert_eq!(response.status(), 503);
    assert!(!response.json::<Health>().is_healthy);
}

#[tokio::test]
async fn detailed_health_detects_event_system_failure() {
    // Arrange: Break event publisher
    let worker = start_worker_with_broken_events();

    // Act: Check detailed health
    let response = worker.get("/health/detailed").await;

    // Assert: event_system should NOT be healthy
    let health = response.json::<DetailedHealth>();
    let event_component = health.components.get("event_system");
    assert_ne!(event_component.status, "healthy");
}
```

---

## Risk Assessment

| Scenario | Current Behavior | Correct Behavior |
|----------|------------------|------------------|
| Orchestration API down | Worker reports ready | Worker reports NOT ready |
| Event publisher broken | Worker reports healthy | Worker reports degraded |
| Database unreachable | Worker reports ready (via HealthService) | Worker reports NOT ready |

**Production Risk**: HIGH - Kubernetes will route traffic to broken workers.

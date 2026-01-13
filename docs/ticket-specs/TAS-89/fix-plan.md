# TAS-89: Fix Plan

**Date**: 2026-01-12
**Purpose**: Categorize findings into immediate fixes vs Linear tickets

---

## Executive Summary

| Category | Items | Action |
|----------|-------|--------|
| **Fix Now** (trivial, low-risk) | 8 | PR in this branch |
| **Linear Tickets** (need design/effort) | 7 | Create tickets |
| **No Action** (aspirational) | 32 | Document only |

---

## Part 1: Fix Now (This Branch)

These are trivial fixes that can be done immediately with minimal risk:

### 1.1 Remove Vestigial `orchestration_api_reachable` Field

**Effort**: 30 minutes
**Risk**: Low (field is never meaningfully used)

**Files to modify**:
```
tasker-worker/src/health.rs                              - Remove from WorkerHealthStatus
tasker-worker/src/worker/core.rs:659                     - Remove field setting
tasker-worker/src/worker/services/worker_status/service.rs:109 - Remove field setting
tasker-worker/src/worker/services/health/service.rs      - Remove from is_healthy() logic
```

**Rationale**: Workers communicate via PGMQ only. This field checks nothing and lies.

---

### 1.2 Remove Orphaned State Machine Guards

**Effort**: 15 minutes
**Risk**: None (code has zero call sites)

**Files to modify**:
```
tasker-shared/src/state_machine/guards.rs:282-352
  - StepCanBeEnqueuedForOrchestrationGuard
  - StepCanBeCompletedFromOrchestrationGuard
  - StepCanBeFailedFromOrchestrationGuard
```

**Rationale**: TAS-41 artifacts with zero call sites across entire workspace.

---

### 1.3 Migrate `#[allow]` to `#[expect]` (Sample)

**Effort**: 1 hour for initial batch
**Risk**: None (lint annotation only)

**Approach**: Start with highest-visibility files:
```
workers/rust/src/step_handlers/*.rs     - 28 instances (api compatibility pattern)
tasker-shared/src/models/factories/*.rs - 6 instances (test factories)
```

**Pattern**:
```rust
// Before
#[allow(dead_code)] // api compatibility
config: StepHandlerConfig,

// After
#[expect(dead_code, reason = "api compatibility - config available for future handler enhancements")]
config: StepHandlerConfig,
```

**Note**: Full migration can be a separate small ticket.

---

### 1.4 Update TAS-89 References in Action Items

**Effort**: 10 minutes
**Risk**: None

Cross-reference action-items.md with this fix plan to avoid duplication.

---

## Part 2: Linear Tickets (Need Design/Effort)

### Ticket 1: Implement Real Worker Health Checks

**Title**: `[TAS-XXX] Implement real health checks in tasker-worker`

**Scope**:
| Component | Current | Target |
|-----------|---------|--------|
| `database_connected` | Hardcoded `true` | Actual SQL ping |
| `check_event_system()` | Returns "healthy" | Check publisher/subscriber |

**Files**:
- `tasker-worker/src/worker/services/worker_status/service.rs`
- `tasker-worker/src/worker/services/health/service.rs`

**Effort**: 2-4 hours
**Priority**: HIGH (affects K8s probe accuracy)

**Acceptance Criteria**:
- [ ] Database health check performs actual connectivity test
- [ ] Event system health check verifies publisher connectivity
- [ ] Health endpoints return 503 when components are down
- [ ] Tests simulate failures and verify unhealthy response

---

### Ticket 2: Implement Real Worker Metrics

**Title**: `[TAS-XXX] Implement real metrics in worker event system`

**Scope**:
| Metric | Current | Target |
|--------|---------|--------|
| `processing_rate` | 0.0 | Events/second calculation |
| `average_latency_ms` | 0.0 | Moving average of processing time |
| `deployment_mode_score` | 1.0 | Effectiveness calculation |
| `handler_count` | 0 | Actual registered handler count |

**Files**:
- `tasker-worker/src/worker/event_systems/worker_event_system.rs:366-368`
- `workers/python/src/observability.rs:177`

**Effort**: 3-4 hours
**Priority**: MEDIUM (affects observability, not correctness)

**Acceptance Criteria**:
- [ ] Metrics reflect actual system behavior
- [ ] Tests verify metrics change under load

---

### Ticket 3: Implement Task Initialization Event Publishing

**Title**: `[TAS-XXX] Implement publish_task_initialized_event()`

**Scope**:
Currently `publish_task_initialized_event()` returns `Ok(())` without publishing.

**Files**:
- `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs:340-347`

**Effort**: 2-3 hours (depends on EventPublisher interface)
**Priority**: HIGH (events silently lost)

**Acceptance Criteria**:
- [ ] Events actually published to event system
- [ ] Event contains task_uuid, step_count, task_name
- [ ] Integration test verifies event delivery

---

### Ticket 4: Implement Real Orchestration Metrics

**Title**: `[TAS-XXX] Implement real metrics in tasker-orchestration`

**Scope**:
| Metric | Current | Target |
|--------|---------|--------|
| `active_processors` | 1 | Actual processor count |
| `pool_utilization` | 0.0 | Connection pool usage % |
| `request_queue_size` | -1 | Actual PGMQ queue depth |

**Files**:
- `tasker-orchestration/src/orchestration/command_processor.rs:1135`
- `tasker-orchestration/src/web/handlers/analytics.rs:198`
- `tasker-orchestration/src/orchestration/lifecycle/task_request_processor.rs:297`

**Effort**: 3-4 hours
**Priority**: MEDIUM (affects capacity planning)

**Acceptance Criteria**:
- [ ] `active_processors` reflects actual count
- [ ] `pool_utilization` queries SQLx pool metrics
- [ ] `request_queue_size` queries PGMQ queue depth

---

### Ticket 5: Implement Template Cache Refresh

**Title**: `[TAS-XXX] Implement template cache refresh commands`

**Scope**:
Cache refresh commands are currently no-ops.

**Files**:
- `tasker-worker/src/worker/actors/template_cache_actor.rs:94-99`

**Effort**: 1-2 hours
**Priority**: MEDIUM (requires restart for config changes)

**Acceptance Criteria**:
- [ ] Namespace-specific cache refresh clears entries for namespace
- [ ] Full cache refresh clears all entries
- [ ] Test verifies cache is cleared after command

---

### Ticket 6: Complete `#[expect]` Migration

**Title**: `[TAS-XXX] Migrate remaining #[allow] to #[expect] per TAS-58`

**Scope**: 96 instances across workspace

**Effort**: 2-3 hours (mechanical)
**Priority**: LOW (code hygiene)

**Acceptance Criteria**:
- [ ] All `#[allow(dead_code)]` converted to `#[expect]` with reasons
- [ ] All `#[allow(unused)]` converted to `#[expect]` with reasons
- [ ] CI passes

---

### Ticket 7: Documentation Quality Improvements

**Title**: `[TAS-XXX] Add missing module documentation per TAS-89 review`

**Scope**:
- Configuration system module docs (20 files)
- tasker-client public item docs
- Doc examples in core modules

**Effort**: 4-6 hours
**Priority**: LOW (developer experience)

**Acceptance Criteria**:
- [ ] Configuration modules have `//!` headers
- [ ] tasker-client public APIs documented
- [ ] At least 10 new doc examples added

---

## Part 3: No Action (Aspirational TODOs)

These 32 TODOs are documented but do not require immediate action:

| Category | Count | Notes |
|----------|-------|-------|
| RabbitMQ support (TAS-35) | 2 | Feature not yet needed |
| Configuration validators | 2 | Nice-to-have |
| Query optimizations | 5 | Performance, not correctness |
| Workflow templating | 5 | Future feature |
| Graph analysis | 2 | Future tooling |
| Memoization clearing | 4 | Memory optimization |
| Validation system | 2 | Framework integration |
| Parallel processing | 1 | TAS-67 optimization |
| CLI improvements | 2 | API cleanup |
| Test infrastructure | 7 | Test quality |

---

## Recommended Execution Order

### Phase 1: This Branch (TAS-89)

1. ✅ Complete documentation and analysis
2. [ ] Remove `orchestration_api_reachable` field
3. [ ] Remove orphaned state machine guards
4. [ ] Initial `#[expect]` migration (28 step handler configs)
5. [ ] PR and merge

### Phase 2: Critical Tickets (Next Sprint)

1. Create and prioritize tickets 1-5
2. Ticket 1 (Worker Health Checks) - HIGH
3. Ticket 3 (Event Publishing) - HIGH
4. Ticket 4 (Orchestration Metrics) - MEDIUM

### Phase 3: Hygiene (Ongoing)

1. Ticket 6 (`#[expect]` migration) - LOW
2. Ticket 7 (Documentation) - LOW
3. Ticket 2 (Worker Metrics) - MEDIUM
4. Ticket 5 (Cache Refresh) - MEDIUM

---

## Summary

| Priority | Ticket | Effort | Impact |
|----------|--------|--------|--------|
| **Fix Now** | (this branch) | 2 hours | Remove lies from codebase |
| **HIGH** | Worker Health Checks | 2-4h | K8s routing correctness |
| **HIGH** | Event Publishing | 2-3h | Event-driven workflows |
| **MEDIUM** | Orchestration Metrics | 3-4h | Capacity planning |
| **MEDIUM** | Worker Metrics | 3-4h | Observability |
| **MEDIUM** | Cache Refresh | 1-2h | Config propagation |
| **LOW** | `#[expect]` Migration | 2-3h | Code hygiene |
| **LOW** | Documentation | 4-6h | Developer experience |

**Total Remediation Effort**: ~20-30 hours across all tickets

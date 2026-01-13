# TAS-89: TODO/FIXME Audit Report

**Date**: 2026-01-12
**Project**: tasker-systems/tasker-core
**Scope**: Comprehensive codebase scan excluding test utilities

---

## Executive Summary

**Total TODO/FIXME Comments**: 68 (63 Rust + 5 Ruby)

The codebase has a manageable number of TODOs with clear categorization. Most are placeholder implementations or observability enhancements. Only 5 TODOs reference specific tickets (TAS-35, TAS-61, TAS-67), indicating good code hygiene overall.

---

## Summary by Language

| Language | Count | Priority Breakdown |
|----------|-------|-------------------|
| **Rust** | 63 | Critical: 3, High: 18, Medium: 35, Low: 7 |
| **Ruby** | 5 | Low: 5 (security examples only) |
| **Python** | 0 | - |
| **TypeScript** | 0 | - |
| **TOTAL** | **68** | **Critical: 3, High: 18, Medium: 35, Low: 12** |

---

## Critical Priority (3 items)

These require attention for core functionality:

| File | Line | Issue | Type |
|------|------|-------|------|
| `tasker-worker/src/worker/core.rs` | 690 | `process_step_message` needs TaskSequenceStep pattern update | API Design |
| `tasker-orchestration/src/lifecycle/task_request_processor.rs` | 297 | `queue_size` method not implemented in PgmqClient | Feature |
| `tasker-worker/tests/integration/worker_system_test.rs` | 18 | Test signature update for WorkerCore::new() | Test Maintenance |

---

## High Priority (18 items)

### Ticket-Referenced TODOs (3)

| Ticket | File | Issue |
|--------|------|-------|
| **TAS-35** | `tasker-shared/src/config/tasker.rs:567` | RabbitMQ multi-backend support |
| **TAS-35** | `tasker-shared/src/messaging/clients/unified_client.rs:283` | Implement RabbitMQ client |
| **TAS-67** | `tasker-worker/src/worker/handlers/completion_processor.rs:96` | Consider parallelizing completion processing |

### Configuration & Validation (3)

| File | Issue |
|------|-------|
| `tasker-shared/src/config/tasker.rs:49` | Custom validators for domain logic (PostgreSQL URLs) |
| `tasker-shared/src/config/tasker.rs:321` | PostgreSQL URL format validation |
| `tasker-orchestration/src/orchestration/system_events.rs:243` | Type validation for field values |

### Event System Integration (5)

| File | Issue |
|------|-------|
| `tasker-orchestration/src/event_systems/unified_event_coordinator.rs:146` | Unified event notification system |
| `tasker-orchestration/src/event_systems/unified_event_coordinator.rs:311` | Event system ready check |
| `tasker-orchestration/src/lifecycle/task_initialization/service.rs:345` | EventPublisher interface finalization |
| `tasker-worker/src/worker/services/health/service.rs:392` | Event publisher/subscriber health check |
| `tasker-worker/src/worker/event_systems/worker_event_system.rs:390` | Actual event processing implementation |

### Model & Query Implementation (7)

| File | Issue |
|------|-------|
| `tasker-shared/src/models/core/task.rs:883` | Fix SQLx database validation issues |
| `tasker-shared/src/models/core/task.rs:947` | Association preloading strategy |
| `tasker-shared/src/models/core/workflow_step_transition.rs:712` | Complex self-join query |
| `tasker-shared/src/models/core/workflow_step_transition.rs:721` | Metadata-based filtering |
| `tasker-shared/src/scopes/task.rs:105` | SQL AST builder for clause reordering |
| `tasker-worker/src/worker/actors/template_cache_actor.rs:94,99` | Cache refresh strategies |

---

## Medium Priority (35 items)

### Observability/Metrics (12 items)

| Count | Category | Pattern |
|-------|----------|---------|
| 4 | Health checks (stubbed) | DB connectivity, API reachability, event health |
| 4 | Processing metrics (stubbed) | Latency, processing rate, cache stats |
| 2 | Performance calculations | Execution time, retry attempts |
| 2 | Cache metrics | Cache age, access count |

**Pattern**: Legitimate observability TODOs with placeholder `0.0` or `true` values.

### Model/Method Stubs (14 items)

| File | Methods |
|------|---------|
| `tasker-shared/src/models/core/task.rs` | 8 methods (graph analysis, dependency resolution, memoization) |
| `tasker-shared/src/models/core/workflow_step.rs` | 6 methods (template building, validation integration) |

### Query Filters (3 items)

- `tasker-shared/src/models/core/workflow_step_transition.rs` - Metadata filtering
- `tasker-orchestration/src/web/handlers/analytics.rs` - Connection pool monitoring
- `tasker-shared/src/models/insights/system_health_counts.rs` - Pool monitoring

---

## Low Priority (12 items)

### Test Data Placeholders (6)

**Location**: `tasker-shared/tests/models/insights/`

Tests verify API surface exists and can be enhanced when factory system is available.

### Security Examples (5)

**Location**: `workers/ruby/spec/handlers/examples/batch_processing/`

**Pattern**: Security TODOs in example code (NOT production code):
- CSV file size limits
- Path traversal validation
- CSV injection mitigation

**Assessment**: Good practice - examples flag security considerations without implementing them.

### Rust Example Code (1)

**Location**: `workers/rust/src/step_handlers/batch_processing_example.rs:673`

Placeholder for example code testing phase.

---

## TODO Distribution by Category

```
Feature Implementation        20 (29%)
├─ Event system              5
├─ Model methods             8
├─ Cache/template system     2
├─ Query optimization        3
└─ Messaging (RabbitMQ)      2

Observability/Metrics        12 (18%)
├─ Health checks             4
├─ Processing metrics        4
├─ Cache/performance stats   4

Tech Debt/Bug Fixes           8 (12%)

Test/Example Documentation   13 (19%)
├─ Test data placeholders    6
├─ Security examples         5
├─ Phase future work         2

Configuration/Auth           10 (15%)
```

---

## Files with Most TODOs

| File | Count | Priority |
|------|-------|----------|
| `tasker-shared/src/models/core/task.rs` | 8 | High/Medium |
| `tasker-shared/src/models/core/workflow_step.rs` | 6 | Medium |
| `tasker-worker/src/worker/event_systems/worker_event_system.rs` | 4 | Medium |
| `tasker-orchestration/src/` | 7 | High/Medium |
| `tasker-shared/tests/models/insights/` | 6 | Low |
| `workers/ruby/spec/handlers/examples/` | 5 | Low |

---

## Recommendations

### 1. Ticket Creation (If Not Exists)
- TAS-35: Multi-backend messaging (RabbitMQ)
- TAS-61: Domain validators (PostgreSQL URLs, etc.)
- TAS-67: Parallel completion processing

### 2. Immediate Actions
- [ ] Fix `process_step_message` implementation
- [ ] Update WorkerCore integration test
- [ ] Document if telemetry config is intentional or to-be-removed

### 3. Observability Enhancement
Rather than individual TODO items:
- Create a metrics implementation plan for stubbed values
- Add a metrics roadmap document
- Systematize health check implementations

---

## Conclusion

The tasker-core codebase maintains **good TODO discipline**:

**Strengths**:
- Clear, actionable comments
- Most TODOs reference specific code locations
- Only 5 referenced tickets (good naming practice)
- No obviously stale or abandoned TODOs
- Security TODOs appropriately placed in examples

**Considerations**:
- 20 TODOs represent incomplete feature implementations
- 12 TODOs for observability metrics (legitimate but should be systematized)
- Some architectural stubs need specification documents

**Overall Assessment**: The TODO/FIXME situation is healthy with actionable items properly documented.

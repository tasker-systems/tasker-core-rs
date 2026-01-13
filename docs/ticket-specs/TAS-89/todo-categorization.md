# TAS-89: TODO Categorization - Real Gaps vs Aspirational

**Date**: 2026-01-12
**Purpose**: Distinguish between TODOs that represent **broken functionality today** vs **future enhancements**

---

## Category Definitions

### Category A: "Real Gaps" (Broken/Missing Now)
Functions that are **actually called** in production code paths but:
- Return hardcoded/fake values instead of real data
- Claim to do something but don't
- Cause silent failures or data loss
- Report incorrect information to monitoring/health systems

### Category B: "Aspirational" (Future Enhancement)
Features that:
- Would be nice to have but aren't needed for correctness
- Are optimization opportunities
- Represent alternative implementations not yet required
- Are placeholders for future phases

---

## Category A: Real Gaps (27 items)

### A1. Health Checks That Lie (CRITICAL)

These return hardcoded values and are **exposed to Kubernetes probes**:

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-worker/src/worker/core.rs:659` | `orchestration_api_reachable: true` | Always reports true | K8s thinks worker is ready when orchestration could be down |
| `tasker-worker/src/worker/services/worker_status/service.rs:108` | `database_connected: true` | Always reports true | K8s routes traffic to workers with broken DB connections |
| `tasker-worker/src/worker/services/worker_status/service.rs:109` | `orchestration_api_reachable: true` | Always reports true | Same as above - duplicate |
| `tasker-worker/src/worker/services/health/service.rs:392` | Check event publisher/subscriber health | Returns "healthy" without checking | Monitoring shows green when event system is broken |

**Verdict**: These are **silent bugs**. Health endpoints lie to orchestrators.

---

### A2. Metrics That Report Fake Data (HIGH)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-worker/src/worker/event_systems/worker_event_system.rs:366` | `processing_rate: 0.0` | Always 0.0 | Cannot measure throughput |
| `tasker-worker/src/worker/event_systems/worker_event_system.rs:367` | `average_latency_ms: 0.0` | Always 0.0 | Cannot detect slow processing |
| `tasker-worker/src/worker/event_systems/worker_event_system.rs:368` | `deployment_mode_score: 1.0` | Always 1.0 | Cannot evaluate deployment effectiveness |
| `tasker-orchestration/src/orchestration/command_processor.rs:1135` | `active_processors: 1` | Always 1 | Load balancing decisions based on wrong data |
| `tasker-orchestration/src/web/handlers/analytics.rs:198` | Pool utilization hardcoded | Always 0.0 | Database saturation invisible |
| `tasker-shared/src/models/insights/system_health_counts.rs:182` | Pool monitoring | Always 0/0 | Same issue |
| `workers/python/src/observability.rs:177` | `handler_count: 0` | Always 0 | Cannot count registered handlers |

**Verdict**: These affect **observability and capacity planning**. Dashboards show lies.

---

### A3. Events That Never Fire (HIGH)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs:345` | Implement event publishing | `publish_task_initialized_event()` returns `Ok(())` without publishing | Event-driven consumers never notified |

**Verdict**: Event publishing is **silently broken**. Downstream systems waiting for events will wait forever.

---

### A4. Cache That Doesn't Refresh (MEDIUM-HIGH)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-worker/src/worker/actors/template_cache_actor.rs:94` | Namespace-specific cache refresh | Command accepted but ignored | Template changes don't propagate |
| `tasker-worker/src/worker/actors/template_cache_actor.rs:99` | Full cache refresh | Command accepted but ignored | Same issue |

**Verdict**: Cache refresh commands are **no-ops**. Config changes require restart.

---

### A5. Event Processing That's Stubbed (MEDIUM-HIGH)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-worker/src/worker/event_systems/worker_event_system.rs:390` | Implement actual event processing | Events accepted but not processed | Event-driven mode doesn't work |
| `tasker-worker/src/worker/event_systems/worker_event_system.rs:481` | Implement component health checks | Health checks skipped | Components can fail silently |
| `tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:146` | Implement unified event notification | Coordinator exists but does nothing | Event coordination broken |
| `tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:311` | Notification handling | Not implemented | Same issue |

**Verdict**: Event-driven architecture is **partially hollow**. The plumbing exists but doesn't flow.

---

### A6. Error Context Missing Real Data (MEDIUM)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-orchestration/src/orchestration/error_handling_service.rs:288` | `execution_duration: Duration::from_secs(0)` | Always 0 seconds | Error classification can't distinguish slow vs fast failures |

**Verdict**: Error analysis uses **wrong duration data**. May affect retry decisions.

---

### A7. Queue Size Unknown (MEDIUM)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-orchestration/src/orchestration/lifecycle/task_request_processor.rs:297` | `queue_size` not implemented | Returns -1 | Cannot monitor request backlog |

**Verdict**: **Capacity monitoring broken**. Can't detect queue buildup.

---

### A8. Cache Stats Missing (LOW-MEDIUM)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-worker/src/worker/services/template_query/service.rs:171` | `cache_age_seconds: None` | Always None | Can't detect stale caches |
| `tasker-worker/src/worker/services/template_query/service.rs:172` | `access_count: None` | Always None | Can't measure cache effectiveness |

**Verdict**: Cache observability is **incomplete** but not broken.

---

### A9. Transition Analytics Incomplete (LOW-MEDIUM)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-shared/src/models/core/workflow_step_transition.rs:770` | `retry_attempts: 0` | Always 0 | Can't track actual retries |
| `tasker-shared/src/models/core/workflow_step_transition.rs:771` | `average_execution_time: None` | Always None | Can't measure step performance |
| `tasker-shared/src/models/core/workflow_step_transition.rs:772` | `average_time_between_transitions: None` | Always None | Can't measure transition delays |

**Verdict**: **Workflow analytics are incomplete**. Data exists in DB but not surfaced.

---

### A10. Type Validation Missing (LOW)

| File:Line | TODO | What Happens Today | Impact |
|-----------|------|-------------------|--------|
| `tasker-orchestration/src/orchestration/system_events.rs:243` | Add type validation for field values | Events with wrong field types pass validation | Potential runtime errors downstream |

**Verdict**: Event validation is **incomplete**. Wrong data can slip through.

---

## Category B: Aspirational (32 items)

### B1. RabbitMQ Support (Future Feature)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/messaging/clients/unified_client.rs:283` | TAS-35 RabbitMQ client | Feature not yet needed - PGMQ works |
| `tasker-shared/src/config/tasker.rs:567` | TAS-35 multi-backend support | Documentation note |

**Verdict**: **Planned feature**, not a gap. PGMQ meets current needs.

---

### B2. Configuration Validators (Enhancement)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/config/tasker.rs:49` | TAS-61 Custom validators | Nice-to-have validation |
| `tasker-shared/src/config/tasker.rs:321` | PostgreSQL URL format validation | Would catch config errors earlier |

**Verdict**: **Defensive improvement**, not required for correctness.

---

### B3. Query Optimization (Performance)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/scopes/task.rs:105` | SQL AST builder for clause reordering | Technical debt, not broken |
| `tasker-shared/src/models/core/task.rs:883` | Fix SQLx database validation issues | May be documentation or edge case |
| `tasker-shared/src/models/core/task.rs:947` | Association preloading strategy | Performance optimization |
| `tasker-shared/src/models/core/workflow_step_transition.rs:712` | Complex self-join query | Feature not yet needed |
| `tasker-shared/src/models/core/workflow_step_transition.rs:721` | Metadata-based filtering | Feature not yet needed |

**Verdict**: **Performance optimizations**, not correctness issues.

---

### B4. Workflow Templating (Future Feature)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/models/core/workflow_step.rs:905` | Template-based step creation | Future templating system |
| `tasker-shared/src/models/core/workflow_step.rs:916` | DAG relationship setup | Future workflow feature |
| `tasker-shared/src/models/core/workflow_step.rs:927` | Template-based step building | Future templating system |
| `tasker-shared/src/models/core/workflow_step.rs:1038` | StepReadinessStatus integration | Future integration |
| `tasker-shared/src/models/core/task.rs:1192` | Default option generation | Future feature |

**Verdict**: **Templating system not yet built**. Current manual approach works.

---

### B5. Graph Analysis (Future Feature)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/models/core/task.rs:1252` | RuntimeGraphAnalyzer | Future analysis capability |
| `tasker-shared/src/models/core/task.rs:1258` | Dependency graph analysis | Future visualization |

**Verdict**: **Analysis tooling**, not required for execution.

---

### B6. Memoization Management (Optimization)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/models/core/task.rs:1347` | Memoization clearing | Memory optimization |
| `tasker-shared/src/models/core/task.rs:1353` | Clear memoized instances | Same |
| `tasker-shared/src/models/core/workflow_step.rs:1231` | Memoization clearing | Same |
| `tasker-shared/src/models/core/workflow_step.rs:1237` | Clear memoized instances | Same |

**Verdict**: **Memory optimization**, not a bug.

---

### B7. Validation System (Future Enhancement)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/models/core/workflow_step.rs:1242` | Validation system integration | Future validation framework |
| `tasker-shared/src/models/core/task.rs:1375` | TaskExecutionContext.workflow_summary | Future integration |

**Verdict**: **Framework integration**, not missing functionality.

---

### B8. Parallel Processing (Optimization)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-worker/src/worker/handlers/completion_processor.rs:96` | TAS-67 Parallelization | Optimization opportunity |

**Verdict**: **Performance improvement**, current sequential approach works.

---

### B9. CLI Auth Improvements (Enhancement)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-client/src/bin/cli/commands/task.rs:17` | Convert auth_token to WebAuthConfig | API consistency improvement |
| `tasker-client/src/bin/cli/commands/worker.rs:12` | Same | Same |

**Verdict**: **API cleanup**, auth still works.

---

### B10. Test Infrastructure (Test Quality)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-worker/tests/integration/worker_system_test.rs:18` | Update test signature | Test maintenance |
| `tasker-orchestration/src/orchestration/error_handling_service.rs:347` | Add integration tests | Test coverage |
| `workers/rust/src/step_handlers/batch_processing_example.rs:673` | Add unit tests Phase 7 | Example testing |
| `tasker-shared/tests/models/insights/slowest_tasks.rs:*` | Test data factories | Test infrastructure |
| `tasker-shared/tests/models/insights/slowest_steps.rs:*` | Same | Same |

**Verdict**: **Test improvements**, not production issues.

---

### B11. API Method Stubs (Future Methods)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-worker/src/worker/core.rs:690` | Update to TaskSequenceStep | API migration |

**Verdict**: **API evolution**, marked for future pattern update.

---

### B12. Telemetry Config (Unused)

| File:Line | TODO | Status |
|-----------|------|--------|
| `tasker-shared/src/config/tasker.rs:946` | Telemetry struct loaded but unused | Config prepared for future use |

**Verdict**: **Prepared infrastructure**, intentionally unused currently.

---

## Summary Table

| Category | Count | Description |
|----------|-------|-------------|
| **A: Real Gaps** | **27** | Broken/misleading functionality |
| **B: Aspirational** | **32** | Future enhancements |
| **Total** | **59** | (excludes 9 test-only TODOs) |

### Category A Breakdown

| Subcategory | Count | Severity |
|-------------|-------|----------|
| Health checks that lie | 4 | CRITICAL |
| Metrics that report fake data | 7 | HIGH |
| Events that never fire | 1 | HIGH |
| Cache that doesn't refresh | 2 | MEDIUM-HIGH |
| Event processing stubbed | 4 | MEDIUM-HIGH |
| Error context missing data | 1 | MEDIUM |
| Queue size unknown | 1 | MEDIUM |
| Cache stats missing | 2 | LOW-MEDIUM |
| Transition analytics incomplete | 3 | LOW-MEDIUM |
| Type validation missing | 1 | LOW |
| **Total Real Gaps** | **27** | |

---

## Recommendations

### Immediate Action (Category A - Critical/High)

1. **Remove `orchestration_api_reachable` field entirely** - It's vestigial
2. **Implement real database health check** in WorkerStatusService
3. **Implement `check_event_system()` with actual checks**
4. **Implement `publish_task_initialized_event()`** - events are silently lost
5. **Implement cache refresh commands** - currently no-ops
6. **Fix metrics to return real values** or mark them clearly as "not implemented"

### Medium-Term (Category A - Medium)

7. **Calculate real `execution_duration`** in error context
8. **Implement `queue_size` method** in PgmqClient
9. **Implement event processing** in worker event system
10. **Complete transition analytics** (retry_attempts, execution_time)

### Low Priority (Category B)

- RabbitMQ support (when needed)
- Configuration validators (defensive)
- Query optimizations (performance)
- Workflow templating (future feature)

---

## Key Insight

**The pattern**: Many TODOs represent **"the plumbing exists but nothing flows through it"**:
- Health endpoints exist but return hardcoded values
- Event publishing methods exist but do nothing
- Cache refresh commands exist but are ignored
- Metrics endpoints exist but return zeros

This is more insidious than missing features because:
1. Tests pass (the methods return successfully)
2. Monitoring looks green (endpoints respond)
3. Architecture reviews see the right structure
4. But **actual functionality is hollow**

# TAS-89 Child Tickets

**Parent**: TAS-89 (Codebase Evaluation and Best Practices Codification)
**Last Updated**: 2026-01-12

---

## Critical Review Completed

A critical review was performed on all proposed tickets to evaluate whether each is actually needed or superseded by existing architecture. See [critical-review.md](./critical-review.md) for full analysis.

### Key Findings & Actions

| Original Ticket | Finding | Action |
|----------------|---------|--------|
| Event Publishing | **Superseded** by logging, OpenTelemetry, PGMQ | **Merged** into Fix Now (Item 4) |
| Template Cache Refresh | **Questionable** use case | **Merged** into Fix Now (Item 5) |

---

## Final Ticket List

After critical review and consolidation, **6 tickets** will be created (down from 8):

| # | Title | Priority | Effort | Status | Spec |
|---|-------|----------|--------|--------|------|
| 1 | Fix Now - Code Cleanup | High | 3h | **Ready** | [fix-now-cleanup.md](./fix-now-cleanup.md) |
| 2 | Implement Real Worker Health Checks | High | 2-4h | Ready | [worker-health-checks.md](./worker-health-checks.md) |
| 3 | Implement Real Worker Metrics | Medium | 3-4h | Ready | [worker-metrics.md](./worker-metrics.md) |
| 4 | ~~Event Publishing~~ | - | - | **Merged** → #1 | [event-publishing.md](./event-publishing.md) |
| 5 | Implement Real Orchestration Metrics | Medium | 3-4h | Ready | [orchestration-metrics.md](./orchestration-metrics.md) |
| 6 | ~~Template Cache Refresh~~ | - | - | **Merged** → #1 | [template-cache-refresh.md](./template-cache-refresh.md) |
| 7 | Complete #[expect] Migration | Low | 2-3h | Ready | [expect-migration.md](./expect-migration.md) |
| 8 | Documentation Quality Improvements | Low | 4-6h | Ready | [documentation-improvements.md](./documentation-improvements.md) |

---

## Linear Tickets Created

| Linear Ticket | Spec | Priority | Effort |
|---------------|------|----------|--------|
| [TAS-139](https://linear.app/tasker-systems/issue/TAS-139) Fix Now - Code Cleanup | fix-now-cleanup.md | High | 3h |
| [TAS-140](https://linear.app/tasker-systems/issue/TAS-140) Worker Health Checks | worker-health-checks.md | High | 2-4h |
| [TAS-141](https://linear.app/tasker-systems/issue/TAS-141) Worker Metrics | worker-metrics.md | Medium | 3-4h |
| [TAS-142](https://linear.app/tasker-systems/issue/TAS-142) Orchestration Metrics | orchestration-metrics.md | Medium | 3-4h |
| [TAS-143](https://linear.app/tasker-systems/issue/TAS-143) #[expect] Migration | expect-migration.md | Low | 2-3h |
| [TAS-144](https://linear.app/tasker-systems/issue/TAS-144) Documentation | documentation-improvements.md | Low | 4-6h |

---

## Priority Summary (Final)

### High Priority (Do First)
1. **Fix Now - Code Cleanup** - Vestigial code removal, stub cleanup, initial `#[expect]` migration
2. **Worker Health Checks** - Critical for K8s probe accuracy

### Medium Priority (Next Sprint)
3. **Worker Metrics** - Observability improvements
4. **Orchestration Metrics** - Capacity planning

### Low Priority (Backlog)
5. **#[expect] Migration** - Code hygiene (remainder after fix-now)
6. **Documentation** - Developer experience

---

## Total Effort (Final)

| Priority | Tickets | Effort |
|----------|---------|--------|
| High | 2 | 5-7 hours |
| Medium | 2 | 6-8 hours |
| Low | 2 | 6-9 hours |
| **Total** | **6** | **17-24 hours** |

---

## Execution Order

```
Sprint 1 (Current):
  └─ TAS-89-1: Fix Now Cleanup (this branch)
     - Remove orchestration_api_reachable
     - Remove orphaned guards
     - Initial #[expect] migration (step handlers)
     - Remove publish_task_initialized stub
     - Remove template cache refresh TODOs

Sprint 2:
  └─ TAS-89-2: Worker Health Checks

Sprint 3:
  ├─ TAS-89-3: Worker Metrics
  └─ TAS-89-5: Orchestration Metrics

Ongoing/Backlog:
  ├─ TAS-89-7: #[expect] Migration (remainder)
  └─ TAS-89-8: Documentation
```

---

## Document Index

| Document | Description |
|----------|-------------|
| [critical-review.md](./critical-review.md) | Critical analysis of ticket necessity |
| [fix-now-cleanup.md](./fix-now-cleanup.md) | **Consolidated** - 5 cleanup items |
| [worker-health-checks.md](./worker-health-checks.md) | Real K8s health probes |
| [worker-metrics.md](./worker-metrics.md) | Handler execution metrics |
| [event-publishing.md](./event-publishing.md) | ~~Superseded~~ - Merged into fix-now |
| [orchestration-metrics.md](./orchestration-metrics.md) | Orchestration capacity metrics |
| [template-cache-refresh.md](./template-cache-refresh.md) | ~~Questionable~~ - Merged into fix-now |
| [expect-migration.md](./expect-migration.md) | #[allow] → #[expect] migration |
| [documentation-improvements.md](./documentation-improvements.md) | Doc quality improvements |

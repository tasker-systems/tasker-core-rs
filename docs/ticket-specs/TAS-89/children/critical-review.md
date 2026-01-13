# TAS-89 Child Tickets: Critical Review

**Date**: 2026-01-12
**Purpose**: Evaluate whether each proposed ticket is actually needed or superseded by existing architecture
**Status**: Complete - Decisions applied

---

## Summary of Findings

| Ticket | Original Recommendation | Review Finding | Final Action |
|--------|------------------------|----------------|--------------|
| TAS-89-1: Fix Now Cleanup | Implement | **Keep** - vestigial code removal is straightforward | Keep (expanded) |
| TAS-89-2: Worker Health Checks | Implement | **Keep** - real K8s probe accuracy needed | Keep |
| TAS-89-3: Worker Metrics | Implement | **Keep** - observability improvement | Keep |
| TAS-89-4: Event Publishing | Implement | **SUPERSEDED** - remove stub instead | **Merged → Fix Now** |
| TAS-89-5: Orchestration Metrics | Implement | **Keep** - capacity planning needed | Keep |
| TAS-89-6: Template Cache Refresh | Implement | **QUESTIONABLE** - unrealistic use case | **Merged → Fix Now** |
| TAS-89-7: #[expect] Migration | Implement | **Keep** - code hygiene | Keep |
| TAS-89-8: Documentation | Implement | **Keep** - developer experience | Keep |

### Consolidation Result
- **8 original tickets** → **6 final tickets**
- Event Publishing and Template Cache Refresh merged into Fix Now Cleanup as Items 4 and 5

---

## Detailed Analysis

### TAS-89-4: Event Publishing - **SUPERSEDED**

#### Original Proposal
Implement `publish_task_initialized_event` to publish a domain event when tasks are initialized.

#### Analysis

**Current Code** (`tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs:336-347`):
```rust
async fn publish_task_initialized(
    &self,
    _task_uuid: Uuid,
    _step_count: usize,
    _task_name: &str,
) -> Result<(), TaskInitializationError> {
    // TODO: Implement event publishing once EventPublisher interface is finalized
    Ok(())
}
```

**Why It's Superseded:**

1. **Comprehensive Logging Already Exists**
   The same file (lines 184-248) shows extensive structured logging:
   - `TASK_TEMPLATE_LOADED` / `TASK_TEMPLATE_FAILED`
   - `WORKFLOW_STEPS_CREATION_START`
   - `WORKFLOW_STEPS_CREATED`
   - `TASK_INITIALIZATION_COMPLETE`

2. **OpenTelemetry Instrumentation Exists**
   Per `docs/observability/metrics-reference.md:105-107`:
   > **Instrumented In**: `task_initializer.rs:start_task_initialization()`

3. **Domain Events Are For Business Observability, Not System Events**
   Per `docs/architecture/domain-events.md:26-31`:
   > **Domain events** enable business observability: payment processed, order fulfilled, inventory updated. Step handlers publish these events to enable external systems to react to business outcomes.

   Task initialization is a **system event**, not a business event. System events are already handled via PGMQ commands.

4. **System Event Architecture Already Handles This**
   Per `docs/architecture/events-and-commands.md:149`:
   ```rust
   InitializeTask { request: TaskRequestMessage, resp: CommandResponder<TaskInitializeResult> }
   ```

   Task initialization flows through PGMQ → Command → Actor, which IS the event system for internal coordination.

**Recommendation**: **Remove the stub** rather than implement it. The functionality was superseded by:
- Comprehensive structured logging
- OpenTelemetry metrics
- PGMQ-based system event architecture

---

### TAS-89-6: Template Cache Refresh - **QUESTIONABLE USE CASE**

#### Original Proposal
Implement `RefreshNamespace` and `RefreshAll` commands in `TemplateCacheActor`.

#### Analysis

**Current Code** (`tasker-worker/src/worker/actors/template_cache_actor.rs:87-101`):
```rust
match msg.namespace {
    Some(ref ns) => {
        // TODO: Implement namespace-specific cache refresh
    }
    None => {
        // TODO: Implement full cache refresh
    }
}
```

**Why It's Questionable:**

1. **Cache Operations Already Exist**
   `TemplateQueryService` (`tasker-worker/src/worker/services/template_query/service.rs`) already has:
   - `clear_cache()` - Clears entire cache (lines 283-294)
   - `maintain_cache()` - Cache maintenance (lines 299-310)
   - `refresh_template()` - Single template refresh (lines 315-351)

2. **Kubernetes Deployment Model**
   In production Kubernetes deployments:
   - Templates are mounted as read-only ConfigMaps
   - Template changes require re-deployment anyway
   - Hot-reloading templates mid-production is risky (could break running tasks)

3. **Use Case Analysis**
   The only scenarios for runtime template refresh:
   - **Development hot-reload**: Valid but niche
   - **Mutable filesystem templates**: Non-standard deployment
   - **Dynamic template sources**: Not currently supported

4. **Implementation Is Trivial If Needed**
   The actor just needs to call existing `TaskTemplateManager` methods:
   ```rust
   self.task_template_manager.clear_cache().await;  // Already exists
   ```

**Recommendation**: Two options:

**Option A - Remove (Recommended)**:
- Delete the `RefreshNamespace`/`RefreshAll` commands
- Remove the no-op handler
- Document that cache management is via HTTP API (`/templates/cache`)

**Option B - Wire Up Existing Methods**:
- Connect actor to existing `TaskTemplateManager.clear_cache()`
- Minimal effort (~5 lines of code)
- Provides programmatic cache control

The user should decide based on whether programmatic cache refresh (vs HTTP API) is a real requirement.

---

### Tickets to Keep Unchanged

#### TAS-89-1: Fix Now Cleanup
**Keep**: Removing `orchestration_api_reachable` and orphaned guards is straightforward cleanup with clear value.

#### TAS-89-2: Worker Health Checks
**Keep**: Real database connectivity checks for Kubernetes probes are critical for operational reliability.

#### TAS-89-3: Worker Metrics
**Keep**: OpenTelemetry metrics for handler execution times improve observability.

#### TAS-89-5: Orchestration Metrics
**Keep**: Capacity planning metrics for step enqueueing and result processing are valuable.

#### TAS-89-7: #[expect] Migration
**Keep**: Code hygiene improvement with no runtime impact.

#### TAS-89-8: Documentation
**Keep**: Developer experience improvement with no runtime impact.

---

## Updated Ticket Count

| Priority | Original | After Review |
|----------|----------|--------------|
| High | 3 | 2 (removed Event Publishing) |
| Medium | 3 | 2-3 (Template Cache depends on decision) |
| Low | 2 | 2 |
| **Total** | **8** | **6-7** |

---

## Recommended Actions

1. **Update TAS-89-4 spec**: Change from "implement" to "remove stub"
2. **Update TAS-89-6 spec**: Add decision point for user (remove vs wire-up)
3. **Update children/README.md**: Reflect revised ticket list
4. **Add removal tasks to Fix Now ticket**: Add event publishing stub removal

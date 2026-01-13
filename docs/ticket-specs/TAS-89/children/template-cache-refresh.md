# TAS-89-6: Template Cache Refresh - MERGED INTO FIX-NOW

**Parent**: TAS-89 (Codebase Evaluation)
**Type**: Cleanup
**Priority**: N/A - Merged
**Effort**: N/A - Merged
**Status**: **MERGED** into [fix-now-cleanup.md](./fix-now-cleanup.md) as Item 5

---

## Decision Made

**Decision**: Option A (Remove) selected per stakeholder approval.

This ticket has been **merged into TAS-89-1 (Fix Now - Code Cleanup)** as Item 5.

See [fix-now-cleanup.md](./fix-now-cleanup.md) for the consolidated scope.

---

## Original Critical Review Finding

**Original Recommendation**: Implement cache refresh
**Revised Recommendation**: Remove (Option A)

See [critical-review.md](./critical-review.md) for full analysis.

---

## Summary

The `TemplateCacheActor` has no-op handlers for `RefreshNamespace` and `RefreshAll` commands. However:

1. **Cache operations already exist** in `TemplateQueryService` via HTTP API
2. **Production deployment model** uses read-only template mounts
3. **Hot-reloading templates** in production is risky

---

## Current State

**Location**: `tasker-worker/src/worker/actors/template_cache_actor.rs:87-101`

```rust
match msg.namespace {
    Some(ref ns) => {
        info!(/* ... */ "Refreshing template cache for namespace");
        // TODO: Implement namespace-specific cache refresh
    }
    None => {
        info!(/* ... */ "Refreshing entire template cache");
        // TODO: Implement full cache refresh
    }
}
```

---

## Existing Cache Operations

`TemplateQueryService` already provides via HTTP:

| Endpoint | Method | Function | Status |
|----------|--------|----------|--------|
| `DELETE /templates/cache` | Clear cache | `clear_cache()` | **Works** |
| `POST /templates/cache/maintain` | Maintenance | `maintain_cache()` | **Works** |
| `POST /templates/{n}/{name}/{v}/refresh` | Single template | `refresh_template()` | **Works** |

---

## Use Case Analysis

### When Would You Need Runtime Template Refresh?

| Scenario | Realistic? | Notes |
|----------|------------|-------|
| Development hot-reload | Yes | Useful during dev |
| ConfigMap update in K8s | No | Read-only mount, needs pod restart |
| Template stored in database | No | Not current architecture |
| External template source | No | Not current architecture |

### Production Reality

In typical Kubernetes deployments:
- Templates mounted as read-only ConfigMaps
- Template changes require re-deployment
- Hot-reloading mid-execution is risky (could break running tasks)

---

## Decision Options

### Option A: Remove (Recommended)

**Remove the no-op handlers entirely.**

**Rationale**:
- Cache operations available via HTTP API
- No realistic production use case for actor-level refresh
- Removes dead code

**Changes**:
1. Remove `RefreshNamespace`/`RefreshAll` command variants
2. Remove no-op handler in `TemplateCacheActor`
3. Document that cache management is via HTTP API

**Effort**: 30 minutes

---

### Option B: Wire Up Existing Methods

**Connect actor to existing `TaskTemplateManager` methods.**

**Rationale**:
- Provides programmatic cache control for FFI consumers
- Trivial to implement
- May have niche development use case

**Changes**:
```rust
// In template_cache_actor.rs
match msg.namespace {
    Some(_ns) => {
        // Namespace-specific not supported, clear all
        self.task_template_manager.clear_cache().await;
    }
    None => {
        self.task_template_manager.clear_cache().await;
    }
}
```

**Effort**: 15 minutes

---

## Recommendation

**Option A (Remove)** unless there's a specific requirement for programmatic cache control.

The HTTP API already provides:
- `DELETE /templates/cache` for clearing
- `POST /templates/cache/maintain` for maintenance

If an FFI consumer needs cache control, they can call the HTTP API.

---

## Files to Modify

### Option A (Remove)
| File | Change |
|------|--------|
| `tasker-worker/src/worker/actors/template_cache_actor.rs` | Remove handler TODOs, log-only |
| `tasker-worker/src/worker/actors/messages.rs` | Keep message (for future use) or remove |

### Option B (Wire Up)
| File | Change |
|------|--------|
| `tasker-worker/src/worker/actors/template_cache_actor.rs` | Call `clear_cache()` |

---

## Acceptance Criteria

### Option A
- [ ] No-op TODOs removed
- [ ] Handler either removed or returns Ok(()) with no TODO
- [ ] Tests pass
- [ ] Documentation notes cache management via HTTP

### Option B
- [ ] Handler calls `task_template_manager.clear_cache().await`
- [ ] Tests verify cache is cleared
- [ ] Integration test for FFI cache control

---

## Risk Assessment

**Risk**: None (either option)
- Option A: Removing no-op code
- Option B: Wiring to existing, tested functionality

---

## Questions for Stakeholder

1. Is there a requirement for programmatic (non-HTTP) cache control?
2. Are there FFI consumers that need cache refresh without HTTP calls?
3. Is development hot-reload a valued use case?

If all answers are "no", proceed with **Option A (Remove)**.

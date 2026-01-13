# TAS-89-1: Fix Now - Code Cleanup

**Parent**: TAS-89 (Codebase Evaluation)
**Type**: Cleanup
**Priority**: High
**Effort**: 3 hours
**Updated**: 2026-01-12 (added items 4-5 from critical review)

---

## Summary

Remove vestigial code and dead code identified during TAS-89 codebase evaluation. These are low-risk, immediate fixes that eliminate "lies" in the codebase.

---

## Scope

### 1. Remove `orchestration_api_reachable` Field

**Rationale**: Workers communicate with orchestration **exclusively via PGMQ queues** - there is no HTTP API communication. This field:
- Checks connectivity to something that doesn't exist
- Is hardcoded to `true` (meaningless)
- Is incorrectly used in `is_healthy()` calculation

**Files to modify**:
| File | Change |
|------|--------|
| `tasker-worker/src/health.rs` | Remove field from `WorkerHealthStatus` struct |
| `tasker-worker/src/worker/core.rs:659` | Remove field setting |
| `tasker-worker/src/worker/services/worker_status/service.rs:109` | Remove field setting |
| `tasker-worker/src/worker/services/health/service.rs` | Remove from `is_healthy()` logic |

**Tests to update**: Any tests that assert on `orchestration_api_reachable`

---

### 2. Remove Orphaned State Machine Guards

**Rationale**: Three guard structs from TAS-41 are fully implemented but have **zero call sites** across the entire workspace. They are dead code.

**Files to modify**:
| File | Change |
|------|--------|
| `tasker-shared/src/state_machine/guards.rs:282-352` | Remove three structs |

**Structs to remove**:
- `StepCanBeEnqueuedForOrchestrationGuard`
- `StepCanBeCompletedFromOrchestrationGuard`
- `StepCanBeFailedFromOrchestrationGuard`

---

### 3. Initial `#[expect]` Migration (Step Handlers)

**Rationale**: Per TAS-58 linting standards, `#[expect]` with reasons is preferred over `#[allow]`. Start with the most visible pattern.

**Files to modify**:
| File | Count |
|------|-------|
| `workers/rust/src/step_handlers/linear_workflow.rs` | 3 |
| `workers/rust/src/step_handlers/diamond_workflow.rs` | 4 |
| `workers/rust/src/step_handlers/parallel_workflow.rs` | 5 |
| `workers/rust/src/step_handlers/complex_tree_workflow.rs` | 7 |
| `workers/rust/src/step_handlers/mixed_dependency_workflow.rs` | 4 |
| `workers/rust/src/step_handlers/multi_root_workflow.rs` | 3 |
| `workers/rust/src/step_handlers/capability_examples.rs` | 2 |

**Pattern**:
```rust
// Before
#[allow(dead_code)] // api compatibility
config: StepHandlerConfig,

// After
#[expect(dead_code, reason = "API compatibility - config available for future handler enhancements")]
config: StepHandlerConfig,
```

---

### 4. Remove Superseded `publish_task_initialized` Stub

**Added from critical review**: See [critical-review.md](./critical-review.md)

**Rationale**: This function is a no-op placeholder that was superseded by:
- Comprehensive structured logging (already in place)
- OpenTelemetry instrumentation (already in place)
- Domain events architecture (for business events, not system events)
- PGMQ-based command system (IS the system event architecture)

**Files to modify**:
| File | Change |
|------|--------|
| `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs` | Remove method (lines 336-347) and call (line 228) |

**Before**:
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

**After**: Method deleted entirely.

---

### 5. Remove Template Cache Refresh No-Op Handlers

**Added from critical review**: See [critical-review.md](./critical-review.md)

**Rationale**: The `TemplateCacheActor` has no-op handlers for refresh commands, but:
- Cache operations already exist via HTTP API (`DELETE /templates/cache`, etc.)
- Kubernetes deployments use read-only ConfigMap mounts for templates
- Hot-reloading templates in production is risky and not a realistic use case

**Files to modify**:
| File | Change |
|------|--------|
| `tasker-worker/src/worker/actors/template_cache_actor.rs` | Remove TODOs, make handler log-only or remove entirely |

**Before**:
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

**After**: Either remove handler entirely OR keep as log-only acknowledgment (no TODO).

---

## Acceptance Criteria

- [ ] `orchestration_api_reachable` field removed from all locations
- [ ] `is_healthy()` no longer references the removed field
- [ ] Three orphaned guard structs removed
- [ ] 28 `#[allow(dead_code)]` converted to `#[expect]` in step handlers
- [ ] `publish_task_initialized` method and call site removed
- [ ] Template cache refresh TODOs removed from `TemplateCacheActor`
- [ ] All tests pass
- [ ] `cargo clippy --all-targets --all-features` passes
- [ ] No new warnings introduced

---

## Testing

```bash
# Verify compilation
cargo build --all-features

# Run tests
cargo test --all-features

# Verify no clippy warnings
cargo clippy --all-targets --all-features
```

---

## Risk Assessment

**Risk**: Low
- Removing vestigial field eliminates a lie, doesn't change behavior
- Removing dead code has no runtime impact
- `#[expect]` migration is lint-only, no runtime change
- Removing `publish_task_initialized` stub removes a no-op with no observable behavior
- Removing template cache refresh TODOs removes no-ops; cache management remains available via HTTP API

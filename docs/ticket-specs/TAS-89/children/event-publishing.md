# TAS-89-4: Remove Superseded Task Initialization Event Stub - MERGED INTO FIX-NOW

**Parent**: TAS-89 (Codebase Evaluation)
**Type**: Code Cleanup
**Priority**: N/A - Merged
**Effort**: N/A - Merged
**Status**: **MERGED** into [fix-now-cleanup.md](./fix-now-cleanup.md) as Item 4

---

## Decision Made

**Decision**: Remove stub (as recommended by critical review).

This ticket has been **merged into TAS-89-1 (Fix Now - Code Cleanup)** as Item 4.

See [fix-now-cleanup.md](./fix-now-cleanup.md) for the consolidated scope.

---

## Original Critical Review Finding

**Original Recommendation**: Implement event publishing
**Revised Recommendation**: **Remove the stub entirely**

See [critical-review.md](./critical-review.md) for full analysis.

---

## Summary

The `publish_task_initialized()` function was a placeholder from early design that has been superseded by:

1. **Comprehensive structured logging** already in place
2. **OpenTelemetry metrics** already instrumented
3. **Domain events architecture** - which is for **business observability** (step handler events), not system events
4. **PGMQ-based command system** - which IS the system event architecture for internal coordination

This is NOT a bug - it's a vestigial placeholder that should be removed.

---

## Why This Is Superseded

### 1. Extensive Logging Already Exists

The same file (`service.rs`) already logs:
- `TASK_TEMPLATE_LOADED` / `TASK_TEMPLATE_FAILED` (lines 152-170)
- `WORKFLOW_STEPS_CREATION_START` (lines 184-194)
- `WORKFLOW_STEPS_CREATED` (lines 202-211)
- `TASK_INITIALIZATION_COMPLETE` (lines 238-248)

### 2. OpenTelemetry Instrumentation Exists

Per `docs/observability/metrics-reference.md`:
> **Instrumented In**: `task_initializer.rs:start_task_initialization()`

### 3. Domain Events Are For Business Observability

Per `docs/architecture/domain-events.md`:
> **Domain events** enable business observability: payment processed, order fulfilled, inventory updated. Step handlers publish these events to enable external systems to react to business outcomes.

Task initialization is a **system event**, not a business event.

### 4. System Events Use PGMQ Commands

Per `docs/architecture/events-and-commands.md`, task initialization flows through:
```
PGMQ TaskRequestMessage → InitializeTask Command → TaskRequestActor
```

This IS the event system for internal coordination.

---

## Current State (Vestigial)

**Location**: `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs:336-347`

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

---

## Scope

### Remove the Stub

1. Delete `publish_task_initialized()` method
2. Remove the call to it in `process()` (line 228)
3. Remove any unused imports

---

## Files to Modify

| File | Change |
|------|--------|
| `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs` | Remove method and call |

---

## Acceptance Criteria

- [ ] `publish_task_initialized()` method removed
- [ ] Call site at line 228 removed
- [ ] `cargo test --all-features` passes
- [ ] `cargo clippy --all-targets --all-features` passes

---

## Testing

```bash
# Verify removal doesn't break anything
cargo test --all-features --package tasker-orchestration

# Verify no dead code warnings
cargo clippy --all-targets --all-features
```

---

## Risk Assessment

**Risk**: None
- Removing a no-op function that does nothing
- Existing logging and metrics provide full observability
- No runtime behavior change

---

## Alternative Considered

**Alternative**: Implement the event publishing as originally planned.

**Why Rejected**:
- Would duplicate existing logging coverage
- Domain events are for step handlers, not orchestration internals
- System event architecture (PGMQ commands) already handles task initialization
- Would add complexity without value

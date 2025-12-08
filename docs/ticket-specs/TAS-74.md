# Lock `.unwrap()` Audit - Technical Debt

**Date**: 2025-12-06
**Branch**: jcoletaylor/tas-67-rust-worker-dual-event-system
**Status**: Documented for future cleanup

## Summary

Production code contains **96 instances** of `.lock().unwrap()`, `.read().unwrap()`, or `.write().unwrap()` on mutex/rwlock operations. These can panic if a thread panicked while holding the lock (lock poisoning).

## Risk Level

**MEDIUM** - Lock poisoning is rare but can cause cascading failures in production:
- If thread A panics while holding a lock, the lock becomes "poisoned"
- Thread B calling `.unwrap()` on the poisoned lock will also panic
- This can cascade through the system

## Recommended Fix

Replace all lock `.unwrap()` calls with poison recovery:

```rust
// Before (panics on poisoned lock):
self.stats.write().unwrap()

// After (recovers from poison):
self.stats.write().unwrap_or_else(|poisoned| poisoned.into_inner())
```

This pattern:
1. Recovers the lock guard even if poisoned
2. Allows the system to continue operating
3. Is safe for simple data structures like stats counters and HashMaps

## Files Requiring Cleanup (by instance count)

| File | Count | Priority |
|------|-------|----------|
| `pgmq-notify/src/listener.rs` | 23 | High |
| `tasker-orchestration/src/orchestration/command_processor.rs` | 21 | High |
| `workers/rust/src/event_subscribers/metrics_subscriber.rs` | 9 | Medium |
| `tasker-worker/src/worker/event_subscriber.rs` | 7 | Medium |
| `tasker-worker/src/worker/event_router.rs` | 7 | Medium |
| `tasker-worker/src/worker/in_process_event_bus.rs` | 6 | Medium |
| `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs` | 6 | Medium |
| `workers/rust/src/step_handlers/registry.rs` | 5 | Medium |
| `tasker-worker/src/worker/handlers/traits.rs` | 4 | Low |
| `tasker-worker/src/worker/handlers/dispatch_service.rs` | 4 | Low |
| `tasker-shared/src/events/worker_events.rs` | 4 | Low |

**Total: 96 instances**

## Already Fixed (TAS-67)

- `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` - 6 instances fixed

## Implementation Notes

### For Stats/Metrics (Simple Recovery)
```rust
// Stats are simple counters - recovery is safe
self.stats.write().unwrap_or_else(|p| p.into_inner()).counter += 1;
```

### For Handler Registries (Recovery + Logging)
```rust
// Log when recovering from poison for debugging
let handlers = self.handlers.read().unwrap_or_else(|poisoned| {
    warn!("Handler registry lock was poisoned, recovering");
    poisoned.into_inner()
});
```

### For Critical State (Consider Error Propagation)
```rust
// For critical operations, consider returning an error instead
pub fn get_handler(&self, name: &str) -> Result<Option<Arc<dyn StepHandler>>, LockPoisonError> {
    let handlers = self.handlers.read()
        .map_err(|_| LockPoisonError::new("handler registry"))?;
    Ok(handlers.get(name).cloned())
}
```

## Search Command

To find all instances:
```bash
rg '\.(lock|read|write)\(\)\.unwrap\(\)' --type rust -n | grep -v '^tests/' | grep -v '/tests/'
```

## Related

- TAS-67: Initial fix for `ffi_dispatch_channel.rs`
- TAS-58: Rust standards update (Microsoft guidelines)

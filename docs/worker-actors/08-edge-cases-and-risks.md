# TAS-69 Edge Cases and Risks Analysis

## Overview

This document identifies potential gaps, edge cases, and risks in the TAS-69 migration from monolithic `WorkerProcessor` to actor-based architecture.

## Identified Gaps (Fixed)

### Gap #1: Domain Event Dispatch Timing

**Issue:** Domain events were not being dispatched after step completion in the initial migration.

**Root Cause:** The `StepExecutorService` returned results without triggering event dispatch, which was previously inline in `WorkerProcessor`.

**Fix Applied:**
```rust
// In StepExecutorActor or ActorCommandProcessor
let result = service.execute_step(message, queue_name).await;
if result.is_ok() {
    self.dispatch_domain_events(step_uuid, &step_result, correlation_id).await;
}
```

**Verification:** E2E tests pass with event subscribers receiving notifications.

**Risk Level:** Low (fixed)

---

### Gap #2: Silent Error Handling in Orchestration Send

**Issue:** Errors when sending results to orchestration were logged but silently swallowed.

**Root Cause:** Legacy code pattern of logging errors without propagation.

**Fix Applied:**
```rust
// FFICompletionService now propagates errors
pub async fn send_result(&self, result: StepExecutionResult) -> TaskerResult<()> {
    self.orchestration_result_sender.send(result).await.map_err(|e| {
        tracing::error!("Failed to send result to orchestration: {}", e);
        e  // Propagate instead of swallow
    })
}
```

**Verification:** Error paths tested in integration tests.

**Risk Level:** Low (fixed)

---

### Gap #3: TaskTemplateManager Namespace Sharing

**Issue:** Registry created a new `TaskTemplateManager`, losing discovered namespaces.

**Root Cause:** Constructor chain didn't support passing pre-initialized manager.

**Fix Applied:**
```rust
// Added constructors for sharing
WorkerActorRegistry::build_with_task_template_manager(
    context, worker_id, task_template_manager
)

ActorCommandProcessor::with_task_template_manager(
    context, worker_id, task_template_manager, buffer_size, monitor
)
```

**Verification:** Namespace validation tests pass.

**Risk Level:** Low (fixed)

---

## Potential Risks

### Risk 1: RwLock Contention

**Description:** `StepExecutorService` uses `RwLock` for interior mutability, which could cause contention under high load.

**Current Implementation:**
```rust
service: Arc<RwLock<StepExecutorService>>

// Every step execution acquires write lock
let mut service = self.service.write().await;
service.execute_step(...).await
```

**Risk Level:** Medium

**Impact:** Under very high concurrent step execution, write lock contention could serialized execution.

**Mitigation:**
1. Step execution is inherently serialized per worker
2. Read locks used where possible (status queries)
3. Consider lock-free alternatives if contention observed

**Monitoring:** Track lock acquisition times in production.

---

### Risk 2: Two-Phase Initialization Complexity

**Description:** Event publisher and domain event handle are set after actor creation.

**Current Implementation:**
```rust
// Phase 1: Create actors
let (mut processor, command_sender) = ActorCommandProcessor::new(...).await?;

// Phase 2: Wire dependencies
processor.set_event_publisher(event_publisher).await;
processor.set_domain_event_handle(domain_event_handle).await;
```

**Risk Level:** Low

**Impact:** If phase 2 fails or is skipped, actors won't have event capabilities.

**Mitigation:**
1. Initialization sequence is deterministic
2. Missing handles result in no-op (not crashes)
3. Tests verify complete initialization

---

### Risk 3: Actor Shutdown Ordering

**Description:** Registry shutdown must occur in correct order to avoid dangling references.

**Current Implementation:**
```rust
pub async fn shutdown(&mut self) {
    // Reverse initialization order
    tracing::debug!("Shutting down WorkerStatusActor");
    tracing::debug!("Shutting down DomainEventActor");
    // ... etc
}
```

**Risk Level:** Low

**Impact:** Incorrect shutdown order could cause use-after-free or deadlocks.

**Mitigation:**
1. Actors are Arc-wrapped (reference counted)
2. Shutdown order documented and tested
3. Actors handle missing dependencies gracefully

---

### Risk 4: Message Handler Error Propagation

**Description:** Errors in deep handler chains may be transformed or lost.

**Error Chain:**
```
ActorCommandProcessor
    → StepExecutorActor.handle()
        → StepExecutorService.execute_step()
            → TaskTemplateManager.get_handler()
                → Database query failure
```

**Risk Level:** Low

**Impact:** Original error context may be lost in long chains.

**Mitigation:**
1. `TaskerResult<T>` preserves error types
2. Logging at each layer captures context
3. E2E tests verify error propagation

---

### Risk 5: Stats Tracking Accuracy

**Description:** Step execution stats are tracked in `WorkerStatusService`, but step execution happens in `StepExecutorService`.

**Risk Level:** Medium

**Impact:** Stats may not accurately reflect execution if services aren't coordinated.

**Current Mitigation:**
1. Stats updated after successful execution
2. E2E tests verify stat increments

**Future Improvement:** Consider shared atomic counters or event-based stat updates.

---

## Edge Cases

### Edge Case 1: Concurrent Step Execution

**Scenario:** Multiple steps for same task execute concurrently.

**Handling:**
- Main branch: Sequential processing via single processor
- Feature branch: Same (RwLock enforces serialization)

**Status:** ✅ Behavior preserved

---

### Edge Case 2: Step Already Claimed

**Scenario:** Step claimed by another worker before execution.

**Handling:**
- `step_claim.claim_step()` returns error
- Service propagates error to actor
- Actor returns error to command processor

**Status:** ✅ Behavior preserved

---

### Edge Case 3: Invalid Namespace

**Scenario:** Step has namespace not supported by worker.

**Handling:**
- `TaskTemplateManager.get_handler()` returns None
- Service returns validation error
- Error propagated to orchestration

**Status:** ✅ Fixed (Gap #3)

---

### Edge Case 4: FFI Handler Timeout

**Scenario:** Ruby/Python handler exceeds execution timeout.

**Handling:**
- Event publisher tracks correlation with timeout
- Timeout results in error result
- Error propagated to orchestration

**Status:** ✅ Behavior preserved

---

### Edge Case 5: Database Connection Loss

**Scenario:** Database becomes unavailable mid-execution.

**Handling:**
- Circuit breaker trips after threshold
- Subsequent requests fail fast
- Health check reports unhealthy

**Status:** ✅ Behavior preserved (circuit breakers unchanged)

---

### Edge Case 6: PGMQ Message Already Deleted

**Scenario:** Message deleted between read and processing.

**Handling:**
- `delete_message()` returns success (idempotent)
- Execution continues normally

**Status:** ✅ Behavior preserved

---

### Edge Case 7: Orchestration Queue Full

**Scenario:** Orchestration result queue at capacity.

**Handling:**
- `send_result()` blocks or times out
- Error propagated (after Gap #2 fix)
- Step may need retry

**Status:** ✅ Fixed (Gap #2)

---

### Edge Case 8: Worker Shutdown During Execution

**Scenario:** Shutdown command received while step executing.

**Handling:**
- Current step completes
- `is_shutting_down` flag checked in loop
- No new steps accepted

**Status:** ✅ Behavior preserved

---

## Test Coverage for Edge Cases

| Edge Case | Unit Test | E2E Test | Manual Test |
|-----------|-----------|----------|-------------|
| Concurrent execution | ⚠️ | ✅ | - |
| Step already claimed | ⚠️ | ✅ | - |
| Invalid namespace | ⚠️ | ✅ | - |
| FFI handler timeout | - | ✅ | - |
| Database connection loss | - | ⚠️ | ✅ |
| Message already deleted | - | ⚠️ | - |
| Orchestration queue full | - | ⚠️ | ✅ |
| Shutdown during execution | - | ✅ | - |

Legend:
- ✅ Covered
- ⚠️ Partial coverage
- `-` Not covered

---

## Recommendations

### Immediate Actions

1. **Add Edge Case Tests**
   - Create focused tests for database connection loss
   - Test orchestration queue backpressure
   - Test concurrent step claims

2. **Enhance Monitoring**
   - Add RwLock contention metrics
   - Track stats update latency
   - Monitor actor message queue depth

### Future Improvements

1. **Lock-Free Stats**
   - Use `AtomicU64` for counters
   - Event-based stat aggregation

2. **Actor Health Checks**
   - Per-actor health reporting
   - Actor restart on failure

3. **Graceful Degradation**
   - Continue processing if domain events fail
   - Fallback modes for event system failures

---

## Summary

| Category | Count | Status |
|----------|-------|--------|
| Identified Gaps | 3 | All Fixed |
| Potential Risks | 5 | Monitored |
| Edge Cases | 8 | Mostly Covered |

The migration is complete with all identified gaps fixed. Remaining risks are documented and monitored. Edge cases are handled consistently with the original implementation.

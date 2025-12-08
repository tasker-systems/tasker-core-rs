# TAS-67 Edge Cases and Risks Analysis

**Last Updated**: 2025-12-06
**Comparison**: Main branch (`tasker-core-main`) vs Feature branch (`tasker-core`)
**Grounded In**: Tasker design principles (idempotency, atomicity, state machines, retry semantics)

---

## Executive Summary

The TAS-67 dual-channel refactor introduces architectural improvements that **reduce** many edge case risks present in the main branch, while introducing new concerns that require careful mitigation. This analysis maps each edge case to Tasker's core design principles.

| Risk Category | Main Branch | Feature Branch | Net Impact |
|---------------|-------------|----------------|------------|
| Race Conditions | High (domain events) | Low (ordered guarantee) | **Improved** |
| Resource Exhaustion | High (unbounded tasks) | Low (bounded semaphore) | **Improved** |
| Timeout Detection | None | Configurable (30s) | **Improved** |
| Panic Handling | Silent failure | Explicit failure result | **Improved** |
| Lock Poisoning | Panic on poison | Recovers from poison | **Improved** |
| Backpressure | Unbounded | Bounded with blocking | **Trade-off** |
| FFI Thread Safety | Implicit | Explicit runtime handle | **Trade-off** |

---

## Edge Case Analysis

### 1. Dispatch Channel Backpressure

**Scenario**: Dispatch channel fills faster than `HandlerDispatchService` can process.

**Main Branch Behavior**:
- Broadcast channel with unbounded spawning
- No backpressure - unlimited concurrent tasks
- Memory exhaustion under load

**Feature Branch Behavior**:
- Bounded MPSC channel (`dispatch_buffer_size = 1000`)
- `sender.send()` blocks when channel full
- Upstream (`StepExecutorActor`) blocked until space available

**Tasker Principle**: **State-Before-Queue Pattern (TAS-29)**
- Step is already in `InProgress` state before dispatch
- If dispatch blocks, step remains `InProgress`
- Polling fallback will eventually re-discover step
- **No lost work** - step persisted in database before queue submission

**Risk Assessment**: LOW
- Blocking is intentional backpressure
- State machine guards prevent duplicate execution
- Graceful degradation under load

**Mitigation**:
- Configure appropriate buffer sizes via TOML
- Monitor channel saturation via `ChannelMonitor`
- Alert at 80% capacity threshold

---

### 2. Handler Timeout

**Scenario**: Handler execution exceeds configured timeout (default 30s).

**Main Branch Behavior**:
- No timeout mechanism
- Handlers can hang indefinitely
- Listener task blocked forever
- No failure result generated

**Feature Branch Behavior**:
- `tokio::time::timeout()` wraps handler execution
- Timeout generates `StepExecutionResult::failure()`
- Failure result sent to completion channel
- Step transitions to appropriate failure state

**Tasker Principle**: **Retry Semantics (max_attempts + retryable)**
- Timeout failure is treated as retryable failure
- Step moves to `WaitingForRetry` if `retryable=true` and `attempts < max_attempts`
- Backoff calculated by `calculate_backoff_delay()` SQL function
- DLQ entry created if permanently failed (TAS-49)

**Risk Assessment**: LOW
- Timeout is configurable per environment
- Failure result ensures orchestration awareness
- Retry system handles transient timeouts

**Mitigation**:
- Configure `handler_timeout_ms` appropriately for handler complexity
- Use longer timeouts in production vs test
- Monitor timeout frequency for capacity planning

---

### 3. Handler Panic

**Scenario**: Handler code panics during execution.

**Main Branch Behavior**:
- Tokio catches panic silently
- No failure result generated
- Orchestration unaware of failure
- Step stuck in `InProgress`

**Feature Branch Behavior**:
- `std::panic::catch_unwind()` catches panic
- Panic message extracted from payload
- `StepExecutionResult::failure()` generated with panic details
- Result sent to completion channel

**Tasker Principle**: **State Machine Guards**
- Panic failure result triggers state transition
- `TaskStateMachine` validates transition from `InProgress`
- Step moves to `Error` or `WaitingForRetry` based on configuration
- Audit trail includes panic information

**Risk Assessment**: LOW
- Panics cannot escape undetected
- Full audit trail preserved
- Retry system can retry after panic

**Mitigation**:
- Log panic details with full backtrace
- Include step context in panic failure result
- Monitor panic rate per handler type

---

### 4. Lock Poisoning Recovery

**Scenario**: Thread panics while holding lock, poisoning it for future acquirers.

**Main Branch Behavior**:
- `.unwrap()` on lock acquisition
- Panic cascade if lock poisoned
- Worker process crash

**Feature Branch Behavior**:
- `unwrap_or_else(|poisoned| poisoned.into_inner())` pattern
- Recovers lock guard even when poisoned
- Warning logged but execution continues
- No panic cascade

**Tasker Principle**: **Crash Recovery (TAS-54)**
- Lock poisoning doesn't crash worker
- Worker continues processing other steps
- Orchestrator can recover any in-progress work
- Processor UUID tracking for debugging (audit only, not enforced)

**Risk Assessment**: LOW
- Poisoning is recovered, not propagated
- Worker remains operational
- Multiple orchestrators provide redundancy

**Mitigation**:
- Log poison recovery events
- Monitor for repeated poisoning (indicates underlying issue)
- Investigate root cause of panics causing poisoning

---

### 5. Concurrent Step Execution

**Scenario**: Multiple handlers execute concurrently under semaphore control.

**Main Branch Behavior**:
- Unbounded concurrent execution
- No coordination between handlers
- Resource exhaustion possible

**Feature Branch Behavior**:
- Semaphore limits concurrent handlers (`max_concurrent_handlers = 10`)
- Each handler acquires permit before execution
- Permits automatically released on completion/panic

**Tasker Principle**: **Idempotency Through State Guards**
- Each step has unique `step_uuid`
- State machine prevents duplicate execution
- `transition_step_state_atomic()` enforces single execution
- Out-of-order completion is safe

**Risk Assessment**: LOW
- Semaphore prevents resource exhaustion
- State guards prevent duplicate work
- Concurrent execution is safe by design

**Mitigation**:
- Configure `max_concurrent_handlers` based on system capacity
- Monitor semaphore wait times
- Scale horizontally for higher concurrency

---

### 6. Semaphore Acquisition Failure

**Scenario**: Semaphore closed or dropped during acquisition.

**Main Branch Behavior**: N/A (no semaphore)

**Feature Branch Behavior**:
- `semaphore.acquire_owned().await` returns `Err` if semaphore closed
- Handler task exits silently (no failure result)
- Step remains in dispatched state

**Tasker Principle**: **Polling Fallback (Hybrid Mode)**
- Step remains in `InProgress` state
- Polling fallback detects stuck steps
- Step re-discovered and re-dispatched
- DLQ entry if repeatedly stuck (TAS-49)

**Risk Assessment**: MEDIUM
- Silent exit during semaphore acquisition
- No explicit failure result generated
- Relies on polling fallback for recovery

**Potential Concern**: Step could be re-dispatched while still "in flight" if semaphore closes mid-acquisition.

**Mitigation**:
- Ensure semaphore lifecycle matches service lifecycle
- Add logging when semaphore acquisition fails
- Consider generating failure result on acquisition failure

---

### 7. FFI Polling Starvation

**Scenario**: FFI language (Ruby) doesn't poll frequently enough.

**Main Branch Behavior**:
- Push-based event forwarding
- Ruby receives events via broadcast subscription
- No explicit polling required

**Feature Branch Behavior**:
- Pull-based polling via `FfiDispatchChannel.poll()`
- Ruby must call `poll_step_events()` to receive work
- Steps buffered in dispatch channel

**Tasker Principle**: **State-Before-Queue Pattern**
- Steps already in `InProgress` state
- If Ruby stops polling, steps remain buffered
- Timeout detection (30s) generates failure results
- Steps eligible for retry via normal mechanisms

**Risk Assessment**: MEDIUM
- Polling is Ruby application's responsibility
- No push mechanism to wake Ruby
- Timeout provides safety net

**Mitigation**:
- Document polling frequency requirements (recommend 10ms)
- `cleanup_timeouts()` detects stuck events
- Timeout generates failure result for retry

---

### 8. FFI Completion Correlation

**Scenario**: Ruby sends completion with wrong or duplicate `event_id`.

**Main Branch Behavior**:
- No explicit correlation
- `send_step_completion_event(data)` has no event ID
- Completion relies on data payload for routing

**Feature Branch Behavior**:
- Explicit `event_id` parameter required
- `pending_events` map tracks dispatched events
- Double-complete detected and rejected

**Tasker Principle**: **Atomic Operations**
- First completion: removes from pending, sends result, invokes callback
- Second completion: lookup fails, returns `false`, logged as warning
- Database state machine provides ultimate protection

**Risk Assessment**: LOW
- Explicit correlation prevents misrouting
- Double-complete safely rejected
- State guards prevent duplicate state transitions

**Mitigation**:
- Validate event_id format (UUID) in Ruby
- Log duplicate completion attempts
- Monitor for patterns indicating Ruby bugs

---

### 9. Domain Event Ordering

**Scenario**: Domain events must fire AFTER completion is committed.

**Main Branch Behavior**:
- Events fire during handler execution
- Race condition: events may fire before result stored
- Downstream systems see events before orchestration processes result

**Feature Branch Behavior**:
- Result sent to completion channel FIRST
- Callback invoked AFTER successful send
- Domain events only fire after result committed

**Tasker Principle**: **Domain Events (Fire-and-Forget)**
- Domain event publishing NEVER fails the step
- Events fire only after result is in pipeline
- If domain event fails, logged but step succeeds
- Ordering: `result.send() → callback.on_handler_complete() → events`

**Risk Assessment**: LOW (IMPROVED from main)
- Explicit ordering eliminates race condition
- Downstream systems see consistent state
- Fire-and-forget preserves step success

**Mitigation**:
- Completion channel send is blocking
- Callback only invoked on successful send
- Domain events decoupled from step outcome

---

### 10. Graceful Shutdown

**Scenario**: Worker shutdown while handlers are in flight.

**Main Branch Behavior**:
- Broadcast receiver dropped
- In-flight handlers may complete
- Results may not be published

**Feature Branch Behavior**:
- `HandlerDispatchService` stops receiving new messages
- In-flight handlers continue (semaphore permits held)
- Completion channel remains open for result delivery
- `CompletionProcessorService` drains remaining results

**Tasker Principle**: **Crash Recovery (TAS-54)**
- Steps in `InProgress` survive restart
- Any orchestrator can resume processing
- Processor UUID tracked but not enforced
- Automatic recovery without manual intervention

**Risk Assessment**: LOW
- Graceful draining of completion channel
- In-flight handlers complete normally
- Crashed handlers recovered by polling fallback

**Mitigation**:
- Implement `stopped()` lifecycle hook
- Wait for completion channel drain on shutdown
- Set reasonable timeout for drain operation

---

### 11. Completion Channel Backpressure

**Scenario**: Completion channel fills faster than `CompletionProcessorService` processes.

**Main Branch Behavior**: N/A (fire-and-forget to event system)

**Feature Branch Behavior**:
- Bounded completion channel (`completion_buffer_size = 1000`)
- Handler tasks block on `sender.send(result).await`
- Semaphore permits held while blocking
- Effective concurrency reduced

**Tasker Principle**: **Backpressure Handling (TAS-51)**
- Blocking is intentional backpressure
- Prevents memory exhaustion
- Signals capacity issues to operators
- Does NOT lose results

**Risk Assessment**: MEDIUM
- Blocking holds semaphore permits
- Can starve new handler dispatches
- Cascades backpressure through system

**Mitigation**:
- Monitor completion channel saturation
- Alert at 80% capacity
- Scale `CompletionProcessorService` if needed
- Consider async result batching

---

### 12. Handler Registry Race Condition

**Scenario**: Handler registration during step execution.

**Main Branch Behavior**:
- Registry accessed directly
- Potential race during registration

**Feature Branch Behavior**:
- `Arc<dyn StepHandlerRegistry>` shared across tasks
- Registry trait defines thread-safe contract
- Registration must be atomic (implementation-dependent)

**Tasker Principle**: **Thread-Safe Registries**
- Registry pattern from Ruby engine (HandlerFactory)
- Registration typically at startup only
- Runtime registration requires careful synchronization

**Risk Assessment**: LOW
- Handlers registered at worker startup
- No dynamic registration during execution
- `Arc` ensures safe sharing

**Mitigation**:
- Register all handlers before starting dispatch service
- Use `RwLock` in registry implementation
- Avoid runtime registration

---

### 13. FFI Thread Runtime Context

**Scenario**: Ruby FFI thread needs to call async callback.

**Main Branch Behavior**:
- Async handled internally
- Ruby receives via channel polling

**Feature Branch Behavior**:
- Ruby thread calls `complete_step_event()`
- Callback requires Tokio runtime
- `runtime_handle.block_on()` used

**Tasker Principle**: **FFI Design**
- Runtime handle stored in `FfiDispatchChannelConfig`
- Required at construction time
- Blocks Ruby thread until callback completes

**Risk Assessment**: MEDIUM
- `block_on()` from non-Tokio thread can deadlock
- Callback must not acquire locks held by calling code
- Ruby thread blocked during callback

**Mitigation**:
- Ensure callback is lock-free
- Keep callback execution short
- Document blocking behavior for Ruby developers
- Consider spawn-then-join pattern for complex callbacks

---

## Risk Matrix Summary

| Edge Case | Likelihood | Impact | Risk Level | Mitigation Status |
|-----------|------------|--------|------------|-------------------|
| Dispatch channel backpressure | Medium | Low | LOW | Configured |
| Handler timeout | Low | Medium | LOW | Implemented |
| Handler panic | Low | Medium | LOW | Implemented |
| Lock poisoning | Very Low | High | LOW | Implemented |
| Concurrent execution | Medium | Low | LOW | Implemented |
| Semaphore acquisition failure | Very Low | Medium | MEDIUM | Partial |
| FFI polling starvation | Low | Medium | MEDIUM | Timeout safety |
| FFI completion correlation | Low | Low | LOW | Implemented |
| Domain event ordering | N/A | N/A | LOW | Implemented |
| Graceful shutdown | Low | Medium | LOW | Implemented |
| Completion channel backpressure | Medium | Medium | MEDIUM | Monitor needed |
| Handler registry race | Very Low | Low | LOW | By design |
| FFI thread runtime context | Low | High | MEDIUM | Documented |

---

## Recommendations

### Immediate Actions
1. **Add semaphore acquisition failure logging** - Currently silent exit
2. **Document FFI polling requirements** - Ruby must poll at 10ms intervals
3. **Add completion channel saturation alerting** - Critical for capacity planning

### Monitoring Requirements
1. Channel saturation metrics (dispatch + completion)
2. Semaphore wait time histogram
3. Handler timeout frequency
4. Lock poisoning recovery events
5. Double-complete attempt frequency

### Future Improvements
1. Consider spawn-then-notify pattern for FFI callbacks (avoid `block_on`)
2. Add explicit failure result on semaphore acquisition failure
3. Implement graceful shutdown draining with configurable timeout

---

## Conclusion

The TAS-67 dual-channel refactor **significantly improves** edge case handling compared to the main branch:

**Major Improvements**:
- Domain event ordering guaranteed (eliminates race condition)
- Handler timeout detection (prevents stuck handlers)
- Panic handling with failure results (enables retry)
- Lock poisoning recovery (prevents cascade failures)
- Bounded concurrency (prevents resource exhaustion)

**Areas Requiring Attention**:
- Semaphore acquisition failure generates no failure result
- FFI polling relies on Ruby application discipline
- `block_on()` from FFI thread has deadlock potential
- Backpressure can cascade through system

All edge cases are **grounded in Tasker's design principles**:
- State machine guards ensure idempotency
- Atomic operations prevent race conditions
- Retry semantics handle transient failures
- Crash recovery enables automatic resumption
- Fire-and-forget domain events preserve step success

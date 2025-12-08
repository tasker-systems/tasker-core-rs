# TAS-67: Rust Worker Dual Event System - Summary

**Date**: 2025-12-07
**Status**: Complete
**Branch**: `jcoletaylor/tas-67-rust-worker-dual-event-system`

---

## Executive Summary

TAS-67 refactored the Rust worker to use a non-blocking, event-driven pattern matching Ruby's FFI architecture. The implementation introduced a dual-channel command pattern where handler invocation is fire-and-forget, and completions flow back through a separate channel.

**Key Achievements**:
1. Fire-and-forget handler dispatch with semaphore-bounded concurrency
2. Domain events only fire after results are committed to the pipeline
3. Comprehensive error handling (panics, timeouts, handler errors)
4. All four MEDIUM-risk edge cases mitigated
5. Discovery and fix of latent SQL precedence bug in `get_task_execution_context()`

---

## Problem Statement

The pre-TAS-67 Rust worker used a blocking `.call()` pattern in the event handler:

```rust
// workers/rust/src/event_handler.rs (before)
let result = handler.call(&event.payload.task_sequence_step).await;  // BLOCKS
```

This created **effectively sequential execution** even for independent steps, preventing true concurrency and causing domain event race conditions.

---

## Solution Architecture

### Dual-Channel Pattern

```
┌────────────────────────────────────────────────────────────────────────┐
│                        STEP EXECUTION FLOW                              │
└────────────────────────────────────────────────────────────────────────┘

[1] WorkerEventSystem receives StepExecutionEvent
         ↓
[2] ActorCommandProcessor routes to StepExecutorActor
         ↓
[3] StepExecutorActor claims step, publishes to HANDLER DISPATCH CHANNEL
         ↓ (fire-and-forget, non-blocking)
[4] HandlerDispatchService receives from channel
         ↓
[5] Resolves handler from registry, invokes handler.call()
         ↓
[6] Handler completes, publishes to COMPLETION CHANNEL
         ↓
[7] CompletionProcessorService receives from channel
         ↓
[8] Routes to FFICompletionService → Orchestration queue

TWO SEPARATE CHANNELS:
- Handler Dispatch Channel: StepExecutorActor → HandlerDispatchService
- Completion Channel: HandlerDispatchService → CompletionProcessorService
```

### Key Components Created

| Component | Location | Purpose |
|-----------|----------|---------|
| `HandlerDispatchService` | `tasker-worker/src/worker/handlers/dispatch_service.rs` | Non-blocking handler invocation with bounded concurrency |
| `CompletionProcessorService` | `tasker-worker/src/worker/handlers/completion_processor.rs` | Routes results to orchestration queue |
| `FfiDispatchChannel` | `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` | Ruby FFI polling interface |
| `PostHandlerCallback` trait | `tasker-worker/src/worker/handlers/dispatch_service.rs` | Domain event publishing hook |
| `DomainEventCallback` | `tasker-worker/src/worker/domain_event_callback.rs` | Implements PostHandlerCallback for domain events |

---

## Design Decisions

### 1. Bounded Parallel Execution

Handler concurrency is bounded by a semaphore (configurable via TOML):

```rust
let _permit = semaphore.acquire().await?;
let result = handler.call(&step).await;
drop(permit);  // Release BEFORE sending to completion channel
sender.send(result).await;
```

### 2. Ordered Domain Events

Domain events fire AFTER result is committed to completion channel:

```
handler.call() → sender.send(result) → callback.on_handler_complete() → domain events
```

This eliminates the race condition where downstream systems saw events before orchestration processed results.

### 3. Comprehensive Error Handling

| Scenario | Result |
|----------|--------|
| Handler timeout | `StepExecutionResult::failure()` with `error_type=handler_timeout` |
| Handler panic | `StepExecutionResult::failure()` with `error_type=handler_panic` |
| Handler error | `StepExecutionResult::failure()` with `error_type=handler_error` |
| Semaphore closed | `StepExecutionResult::failure()` with `error_type=semaphore_acquisition_failed` |

### 4. Fire-and-Forget FFI Callbacks

Ruby FFI callbacks use `runtime_handle.spawn()` instead of `block_on()` to prevent:
- Ruby thread blocking
- Deadlock risks from nested locks
- Callback failure cascades

---

## Risk Mitigations Implemented

All four MEDIUM-risk edge cases were addressed:

| Risk | Mitigation | Status |
|------|------------|--------|
| Semaphore acquisition failure | Generate failure result instead of silent exit | Complete |
| FFI polling starvation | Metrics + starvation warnings + timeout | Complete |
| Completion channel backpressure | Release permit before send | Complete |
| FFI thread runtime context | Fire-and-forget callbacks | Complete |

---

## Critical Bug Discovery

During TAS-67 implementation, a latent precedence bug was discovered in `get_task_execution_context()`:

**Bug**: When a task had BOTH permanently blocked steps AND ready steps, the SQL function returned `has_ready_steps` instead of `blocked_by_failures`.

**Root Cause**: The CASE statement checked `ready_steps > 0` BEFORE `permanently_blocked_steps > 0`.

**Fix**: Migration `20251207000000_fix_execution_status_priority.sql` corrects the precedence.

**Why It Surfaced**: TAS-67's true parallel execution changed the probability distribution of state combinations, transforming a Heisenbug into a Bohrbug.

See: `docs/architecture-decisions/rca-parallel-execution-timing-bugs.md`

---

## Configuration

```toml
# config/tasker/base/worker.toml
[worker.mpsc_channels.handler_dispatch]
dispatch_buffer_size = 1000
completion_buffer_size = 1000
max_concurrent_handlers = 10
handler_timeout_ms = 30000

[worker.mpsc_channels.ffi_dispatch]
dispatch_buffer_size = 1000
completion_timeout_ms = 30000
starvation_warning_threshold_ms = 10000
callback_timeout_ms = 5000
```

---

## Test Coverage

- **1185 tests passed**, 7 skipped
- All E2E tests pass consistently (including previously flaky `test_mixed_workflow_scenario`)
- Zero "channel closed" warnings in Ruby worker logs
- Domain events working for both Ruby and Rust workers

---

## Future Enhancements

Documented in `docs/ticket-specs/TAS-67/future-enhancements.md`:

1. **Parallel Completion Processor**: Apply semaphore-bounded parallelism to `CompletionProcessorService` (deferred - handler execution is typically the bottleneck)
2. **Channel Saturation Monitoring Alerts**: Add Prometheus/Grafana alerting rules
3. **Comprehensive Handler Dispatch Test Suite**: Targeted tests for concurrent limits and backpressure

---

## Files Changed

### New Files

| File | Purpose |
|------|---------|
| `tasker-worker/src/worker/handlers/dispatch_service.rs` | Handler dispatch service |
| `tasker-worker/src/worker/handlers/completion_processor.rs` | Completion processor service |
| `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` | FFI polling interface |
| `tasker-worker/src/worker/domain_event_callback.rs` | Domain event callback |
| `docs/ticket-specs/TAS-67/future-enhancements.md` | Deferred enhancements |
| `docs/development/ffi-callback-safety.md` | FFI callback guidelines |
| `docs/architecture-decisions/rca-parallel-execution-timing-bugs.md` | RCA document |
| `migrations/20251207000000_fix_execution_status_priority.sql` | Precedence fix |

### Modified Files

| File | Changes |
|------|---------|
| `tasker-worker/src/worker/actors/step_executor_actor.rs` | Fire-and-forget dispatch |
| `tasker-worker/src/worker/actor_command_processor.rs` | Channel routing |
| `workers/ruby/ext/tasker_core/src/bootstrap.rs` | Fixed orphaned event bus bug |
| `config/tasker/base/worker.toml` | New channel configurations |

---

## Documentation

| Document | Purpose |
|----------|---------|
| `docs/ticket-specs/TAS-67/plan.md` | Implementation plan |
| `docs/ticket-specs/TAS-67/00-foundations-context.md` | Design principles |
| `docs/ticket-specs/TAS-67/01-dispatch-flow-mapping.md` | Dispatch flow comparison |
| `docs/ticket-specs/TAS-67/02-completion-flow-mapping.md` | Completion flow comparison |
| `docs/ticket-specs/TAS-67/03-ffi-flow-mapping.md` | FFI flow comparison |
| `docs/ticket-specs/TAS-67/04-edge-cases-and-risks.md` | Risk analysis |
| `docs/ticket-specs/TAS-67/risk-mitigation-plan.md` | Mitigation implementation |
| `docs/worker-event-systems.md` | Architecture overview |

---

## Conclusion

TAS-67 successfully refactored the Rust worker to use a non-blocking, event-driven architecture that:

1. **Improves concurrency**: True parallel handler execution with bounded concurrency
2. **Eliminates race conditions**: Domain events only fire after results are committed
3. **Handles errors comprehensively**: Panics, timeouts, and errors all generate proper failure results
4. **Mitigates all identified risks**: Four MEDIUM-risk edge cases addressed
5. **Surfaces latent bugs**: Parallel execution revealed and enabled fixing a SQL precedence bug

The implementation provides a foundation for TAS-72 (Python/PyO3 workers) by creating reusable abstractions in `tasker-worker`.

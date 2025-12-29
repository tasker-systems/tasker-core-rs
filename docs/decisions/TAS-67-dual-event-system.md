# ADR: Worker Dual-Channel Event System

**Status**: Accepted
**Date**: 2025-12
**Ticket**: [TAS-67](https://linear.app/tasker-systems/issue/TAS-67)

## Context

The pre-TAS-67 Rust worker used a blocking `.call()` pattern in the event handler:

```rust
let result = handler.call(&event.payload.task_sequence_step).await;  // BLOCKS
```

This created **effectively sequential execution** even for independent steps, preventing true concurrency and causing domain event race conditions where downstream systems saw events before orchestration processed results.

## Decision

Adopt a **dual-channel command pattern** where handler invocation is fire-and-forget, and completions flow back through a separate channel.

**Architecture**:
```
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
```

**Key Design Decisions**:

1. **Bounded Parallel Execution**: Semaphore-bounded concurrency (configurable via TOML)
2. **Ordered Domain Events**: Events fire AFTER result is committed to completion channel
3. **Comprehensive Error Handling**: Panics, timeouts, handler errors all generate proper failure results
4. **Fire-and-Forget FFI Callbacks**: `runtime_handle.spawn()` instead of `block_on()` prevents deadlocks

## Consequences

### Positive

- **True parallelism**: Parallel handler execution with bounded concurrency
- **Eliminated race conditions**: Domain events only fire after results committed
- **Comprehensive error handling**: All failure modes produce proper step failures
- **Foundation for FFI**: Reusable abstractions for Ruby/Python/TypeScript workers
- **Bug discovery**: Parallel execution surfaced latent SQL precedence bug

### Negative

- **Increased complexity**: Two channels to manage instead of one
- **Debugging complexity**: Tracing flow across multiple channels requires structured logging

### Neutral

- Channel saturation monitoring available via metrics
- Configurable buffer sizes per environment

## Risk Mitigations Implemented

| Risk | Mitigation |
|------|------------|
| Semaphore acquisition failure | Generate failure result instead of silent exit |
| FFI polling starvation | Metrics + starvation warnings + timeout |
| Completion channel backpressure | Release permit before send |
| FFI thread runtime context | Fire-and-forget callbacks |

## Alternatives Considered

### Alternative 1: Thread Pool Pattern

Use dedicated thread pool for handler execution.

**Rejected**: Tokio already provides excellent async runtime; adding threads increases complexity without benefit.

### Alternative 2: Single Channel with Priority Queue

Priority queue for completions within single channel.

**Rejected**: Doesn't address the fundamental blocking issue; still couples dispatch and completion.

### Alternative 3: Keep Blocking Pattern with Larger Buffer

Increase buffer size to mask sequential execution.

**Rejected**: Doesn't solve concurrency; just delays the problem.

## References

- For historical implementation details, see [TAS-67](https://linear.app/tasker-systems/issue/TAS-67)
- [Worker Event Systems](../architecture/worker-event-systems.md) - Architecture documentation
- [RCA: Parallel Execution Timing Bugs](./rca-parallel-execution-timing-bugs.md) - Bug discovered during implementation
- [FFI Callback Safety](../development/ffi-callback-safety.md) - FFI patterns established

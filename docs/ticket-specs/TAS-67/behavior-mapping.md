# TAS-67 Behavior Mapping Index

**Status**: Superseded by comprehensive analysis (2025-12-06)

This document has been expanded into a comprehensive multi-part analysis grounded in Tasker's design principles.

## Comprehensive Documentation

| Document | Description |
|----------|-------------|
| [00-foundations-context.md](./00-foundations-context.md) | Tasker design principles informing the analysis |
| [01-dispatch-flow-mapping.md](./01-dispatch-flow-mapping.md) | Handler dispatch flow comparison (main vs feature) |
| [02-completion-flow-mapping.md](./02-completion-flow-mapping.md) | Completion flow and domain event ordering |
| [03-ffi-flow-mapping.md](./03-ffi-flow-mapping.md) | Ruby FFI dispatch and completion changes |
| [04-edge-cases-and-risks.md](./04-edge-cases-and-risks.md) | Edge cases, risks, and mitigations |

## Key Findings Summary

### Architectural Improvements (Feature Branch)

1. **Non-blocking dispatch** - Fire-and-forget pattern via dual-channel architecture
2. **Bounded concurrency** - Semaphore-controlled parallel handlers (configurable)
3. **Timeout safety** - Configurable handler timeout with failure result generation
4. **Domain event ordering** - Events fire AFTER completion channel succeeds
5. **Lock poisoning recovery** - Recovers from poisoned locks instead of panicking

### API Breaking Changes

| Aspect | Main Branch | Feature Branch |
|--------|-------------|----------------|
| Completion function | `send_step_completion_event(data)` | `complete_step_event(event_id, data)` |
| Return type | `nil` (fire-and-forget) | `bool` (success indicator) |
| Event correlation | Implicit | Explicit UUID |

### Risk Assessment

- **Overall**: Net improvement in edge case handling
- **Main concerns**: FFI polling responsibility, backpressure cascading, `block_on` from FFI thread
- **All risks mitigated** by Tasker's state machine guards, retry semantics, and crash recovery

---

*See individual documents for detailed analysis.*

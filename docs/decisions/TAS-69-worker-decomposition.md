# ADR: Worker Actor-Service Decomposition

**Status**: Accepted
**Date**: 2025-12
**Ticket**: [TAS-69](https://linear.app/tasker-systems/issue/TAS-69)

## Context

The tasker-worker crate had a monolithic command processor architecture:

- `WorkerProcessor`: 1,575 lines of code
- All command handling inline
- Difficult to test individual behaviors
- Inconsistent with orchestration architecture (TAS-46)

## Decision

Transform the worker from monolithic command processor to **actor-based design**, mirroring the orchestration pattern from TAS-46.

**Before: Monolithic Design**
```
WorkerCore
    └── WorkerProcessor (1,575 LOC)
            └── All command handling inline
```

**After: Actor-Based Design**
```
WorkerCore
    └── ActorCommandProcessor (~350 LOC)
            └── WorkerActorRegistry
                    ├── StepExecutorActor → StepExecutorService
                    ├── FFICompletionActor → FFICompletionService
                    ├── TemplateCacheActor → TaskTemplateManager
                    ├── DomainEventActor → DomainEventSystem
                    └── WorkerStatusActor → WorkerStatusService
```

**Five Actors**:
| Actor | Responsibility | Messages |
|-------|----------------|----------|
| StepExecutorActor | Step execution coordination | 4 |
| FFICompletionActor | FFI completion handling | 2 |
| TemplateCacheActor | Template cache management | 2 |
| DomainEventActor | Event dispatching | 1 |
| WorkerStatusActor | Status and health | 4 |

**Three Services**:
| Service | Lines | Purpose |
|---------|-------|---------|
| StepExecutorService | ~400 | Step claiming, verification, FFI invocation |
| FFICompletionService | ~200 | Result delivery to orchestration |
| WorkerStatusService | ~200 | Stats tracking, health reporting |

## Consequences

### Positive

- **92% reduction** in command processor complexity (1,575 LOC → 123 LOC main file)
- **Single responsibility**: Each file handles one concern
- **Testability**: Services testable in isolation, actors via message handlers
- **Consistency**: Mirrors orchestration architecture
- **Extensibility**: New actors/services follow established pattern

### Negative

- **Two-phase initialization**: Registry requires careful startup ordering
- **Actor shutdown ordering**: Must coordinate graceful shutdown
- **Learning curve**: New pattern to understand for contributors

### Neutral

- Public API unchanged (`WorkerCore::new()`, `send_command()`, `stop()`)
- Internal restructuring transparent to users

## Gaps Identified and Fixed

| Gap | Issue | Fix |
|-----|-------|-----|
| Domain Event Dispatch | Events not dispatched after step completion | Explicit dispatch call in actor |
| Silent Error Handling | Orchestration send errors swallowed | Explicit error propagation |
| Namespace Sharing | Registry created new manager, losing namespaces | Shared pre-initialized manager |

## Alternatives Considered

### Alternative 1: Service-Only Pattern

Extract services without actor layer.

**Rejected**: Loses message-based interfaces that enable testing and future distributed execution.

### Alternative 2: Keep Monolithic with Better Organization

Refactor `WorkerProcessor` into methods without extraction.

**Rejected**: Doesn't address testability or architectural consistency goals.

### Alternative 3: Full Actor Framework (Actix)

Use production actor framework.

**Rejected**: Too heavyweight; we need lifecycle hooks and message-based testing, not distributed supervision.

## References

- For historical implementation details, see [TAS-69](https://linear.app/tasker-systems/issue/TAS-69)
- [Worker Actors](../architecture/worker-actors.md) - Architecture documentation
- [TAS-46 Actor Pattern](./TAS-46-actor-pattern.md) - Orchestration actor precedent

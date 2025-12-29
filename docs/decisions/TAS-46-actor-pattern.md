# ADR: Actor-Based Orchestration Architecture

**Status**: Accepted
**Date**: 2025-10
**Ticket**: [TAS-46](https://linear.app/tasker-systems/issue/TAS-46)

## Context

The orchestration system used a command pattern with direct service delegation, but lacked formal boundaries between commands and lifecycle components. This created:

1. **Testing Complexity**: Lifecycle components tightly coupled to command processor
2. **Unclear Boundaries**: No formal interface between commands and lifecycle operations
3. **Limited Supervision**: No standardized lifecycle hooks for resource management
4. **Inconsistent Patterns**: Each component had different initialization patterns
5. **Coupling**: Command processor had direct dependencies on multiple service instances

The command processor was 1,164 lines, mixing routing, hydration, validation, and delegation.

## Decision

Adopt a lightweight actor pattern with message-based interfaces:

**Core Abstractions**:
1. `OrchestrationActor` trait with lifecycle hooks (`started()`, `stopped()`)
2. `Message` trait for type-safe messages with associated `Response` type
3. `Handler<M>` trait for async message processing
4. `ActorRegistry` for centralized actor management

**Four Orchestration Actors**:
1. **TaskRequestActor**: Task initialization and request processing
2. **ResultProcessorActor**: Step result processing
3. **StepEnqueuerActor**: Step enqueueing coordination
4. **TaskFinalizerActor**: Task finalization with atomic claiming

**Implementation Approach**:
- Greenfield migration (no dual support)
- Actors wrap existing services, not replace them
- Arc-wrapped actors for efficient cloning across threads
- No full actor framework (keeping it lightweight)

## Consequences

### Positive

- **92% reduction** in command processor complexity (1,575 LOC → 123 LOC main file)
- **Clear boundaries**: Each actor handles specific message types
- **Testability**: Message-based interfaces enable isolated testing
- **Consistent patterns**: Established migration pattern for all actors
- **Lifecycle management**: Standardized `started()`/`stopped()` hooks
- **Thread safety**: Arc-wrapped actors with Send+Sync guarantees

### Negative

- **Additional abstraction**: One more layer between commands and services
- **Learning curve**: New pattern to understand
- **Message overhead**: ~100-500ns per actor call (acceptable for our use case)
- **Not a full framework**: Lacks supervision trees, mailboxes, etc.

### Neutral

- Services remain unchanged; actors are thin wrappers
- Performance impact minimal (<1μs per operation)

## Alternatives Considered

### Alternative 1: Full Actor Framework (Actix)

Would provide supervision, mailboxes, and advanced patterns.

**Rejected**: Too heavyweight for our needs. We need lifecycle hooks and message-based testing, not a full distributed actor system.

### Alternative 2: Keep Direct Service Delegation

Continue with command processor calling services directly.

**Rejected**: Doesn't address testing complexity, unclear boundaries, or lifecycle management needs.

### Alternative 3: Trait-Based Service Abstraction

Define `Service` trait and implement on each lifecycle component.

**Partially adopted**: Combined with actor pattern. Services implement business logic; actors provide message-based coordination.

## References

- [TAS-46](https://linear.app/tasker-systems/issue/TAS-46) - Full implementation details
- [Actors Architecture](../architecture/actors.md) - Actor pattern documentation
- [Events and Commands](../architecture/events-and-commands.md) - Integration context

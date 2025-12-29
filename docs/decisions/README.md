# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records that document significant design decisions in Tasker Core. Each ADR captures the context, decision, and consequences of a specific architectural choice.

## ADR Index

### Active Decisions

| ADR | Title | Date | Status |
|-----|-------|------|--------|
| [TAS-46](./TAS-46-actor-pattern.md) | Actor-Based Orchestration Architecture | 2025-10 | Accepted |
| [TAS-51](./TAS-51-bounded-mpsc-channels.md) | Bounded MPSC Channels | 2025-10 | Accepted |
| [TAS-54](./TAS-54-ownership-removal.md) | Processor UUID Ownership Removal | 2025-10 | Accepted |
| [TAS-57](./TAS-57-backoff-consolidation.md) | Backoff Strategy Consolidation | 2025-10 | Accepted |
| [TAS-67](./TAS-67-dual-event-system.md) | Worker Dual-Channel Event System | 2025-12 | Accepted |
| [TAS-69](./TAS-69-worker-decomposition.md) | Worker Actor-Service Decomposition | 2025-12 | Accepted |
| [TAS-100](./TAS-100-ffi-over-wasm.md) | FFI Over WASM for Language Workers | 2025-12 | Accepted |
| [TAS-112](./TAS-112-composition-pattern.md) | Handler Composition Pattern | 2025-12 | Accepted |

### Root Cause Analyses

| Document | Title | Date |
|----------|-------|------|
| [RCA](./rca-parallel-execution-timing-bugs.md) | Parallel Execution Timing Bugs | 2025-12 |

---

## ADR Template

When creating a new ADR, use this template:

```markdown
# ADR: [Title]

**Status**: [Proposed | Accepted | Deprecated | Superseded]
**Date**: YYYY-MM-DD
**Ticket**: [TAS-XXX](https://linear.app/tasker-systems/issue/TAS-XXX)

## Context

What is the issue that we're seeing that is motivating this decision or change?

## Decision

What is the change that we're proposing and/or doing?

## Consequences

What becomes easier or more difficult to do because of this change?

### Positive

- Benefit 1
- Benefit 2

### Negative

- Trade-off 1
- Trade-off 2

### Neutral

- Side effect that is neither positive nor negative

## Alternatives Considered

What other options were considered and why were they rejected?

### Alternative 1: [Name]

Description and why it was rejected.

### Alternative 2: [Name]

Description and why it was rejected.

## References

- Related documents
- External references
```

---

## When to Create an ADR

Create an ADR when:

1. **Making a significant architectural change** that affects multiple components
2. **Choosing between alternatives** with meaningful trade-offs
3. **Establishing a pattern** that should be followed consistently
4. **Removing or deprecating** an existing pattern or approach
5. **Learning from an incident** (RCA format)

Don't create an ADR for:

- Minor implementation details
- Bug fixes without architectural impact
- Documentation updates
- Routine refactoring

---

## Related Documentation

- [Tasker Core Tenets](../principles/tasker-core-tenets.md) - Core design principles
- [CHRONOLOGY](../CHRONOLOGY.md) - Development timeline
- [Ticket Specifications](../ticket-specs/) - Detailed feature specs

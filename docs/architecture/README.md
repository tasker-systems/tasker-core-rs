# Tasker Core Architecture

This directory contains architectural reference documentation describing how Tasker Core's components work together.

## Documents

| Document | Description |
|----------|-------------|
| [Crate Architecture](./crate-architecture.md) | Workspace structure and crate responsibilities |
| [Actors](./actors.md) | Actor-based orchestration lifecycle components |
| [Worker Actors](./worker-actors.md) | Actor pattern for worker step execution |
| [States and Lifecycles](./states-and-lifecycles.md) | Dual state machine architecture (Task + Step) |
| [Events and Commands](./events-and-commands.md) | Event-driven coordination patterns |
| [Domain Events](./domain-events.md) | Business event publishing (durable/fast/broadcast) |
| [Idempotency and Atomicity](./idempotency-and-atomicity.md) | Defense-in-depth guarantees |
| [Backpressure Architecture](./backpressure-architecture.md) | Unified resilience and flow control |
| [Circuit Breakers](./circuit-breakers.md) | Fault isolation and cascade prevention |
| [Deployment Patterns](./deployment-patterns.md) | Hybrid, EventDriven, and PollingOnly modes |

## When to Read These

- **Designing new features**: Understand how components interact
- **Debugging flow issues**: Trace message paths through actors
- **Understanding trade-offs**: See why patterns were chosen
- **Onboarding**: Build mental model of the system

## Related Documentation

- [Principles](../principles/) - The "why" behind architectural decisions
- [Guides](../guides/) - Practical "how-to" documentation
- [CHRONOLOGY](../CHRONOLOGY.md) - Historical context for decisions

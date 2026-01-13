# TAS-133 Child Tickets

This directory contains detailed specifications for each phase of TAS-133 (Messaging Service Strategy Pattern Abstraction).

## Branching Strategy

```
main
  └── jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction (feature branch)
        ├── jcoletaylor/tas-133a-trait-definitions
        ├── jcoletaylor/tas-133b-pgmq-implementation
        ├── jcoletaylor/tas-133c-message-client-struct
        ├── jcoletaylor/tas-133d-rabbitmq-implementation
        ├── jcoletaylor/tas-133e-system-context-integration
        └── jcoletaylor/tas-133f-migration-cleanup
```

Each child branch merges back to TAS-133 after passing its validation gate. Only when all milestones are complete does TAS-133 merge to main.

## Phase Overview

| Phase | Ticket | Summary | Dependencies |
|-------|--------|---------|--------------|
| 1 | [TAS-133a](./TAS-133a-trait-definitions.md) | Trait definitions, types, errors, router | None |
| 2 | [TAS-133b](./TAS-133b-pgmq-implementation.md) | PgmqMessagingService + InMemory | TAS-133a |
| 3 | [TAS-133c](./TAS-133c-message-client-struct.md) | MessageClient struct, StepMessage rename | TAS-133b |
| 4 | [TAS-133d](./TAS-133d-rabbitmq-implementation.md) | RabbitMqMessagingService | TAS-133b |
| 5 | [TAS-133e](./TAS-133e-system-context-integration.md) | SystemContext integration, TOML config | TAS-133c, TAS-133d |
| 6 | [TAS-133f](./TAS-133f-migration-cleanup.md) | Delete old code, final cleanup | TAS-133e |

## Dependency Graph

```
TAS-133a (Traits)
    │
    ├─────────────────┐
    ▼                 ▼
TAS-133b (PGMQ)   TAS-133d (RabbitMQ)
    │                 │
    ▼                 │
TAS-133c (Client)     │
    │                 │
    └────────┬────────┘
             ▼
    TAS-133e (Integration)
             │
             ▼
    TAS-133f (Cleanup)
```

Note: TAS-133b and TAS-133d can be worked in parallel after TAS-133a completes.

## Validation Gates

Each phase has specific validation criteria that must pass before merging:

- **TAS-133a**: Compiles, existing tests pass, new types importable
- **TAS-133b**: PGMQ conformance tests pass, existing orchestration works
- **TAS-133c**: Domain operations work through new client, StepMessage renamed
- **TAS-133d**: RabbitMQ conformance tests pass (docker-compose)
- **TAS-133e**: Full integration tests pass with both providers
- **TAS-133f**: No old abstractions remain, all tests green, clippy clean

## Quick Reference

### Key Decisions

- **Enum dispatch** instead of `Arc<dyn>` for hot-path performance
- **MessagingProvider** enum with variants: Pgmq, RabbitMq, InMemory
- **MessageRouterKind** enum for queue name routing
- **MessageClient** is a struct (not trait) wrapping provider + router
- **No feature flags** for RabbitMQ - runtime config selection

### Key Types

```rust
// Provider enum (zero-cost dispatch)
pub enum MessagingProvider {
    Pgmq(PgmqMessagingService),
    RabbitMq(RabbitMqMessagingService),
    InMemory(InMemoryMessagingService),
}

// Domain facade
pub struct MessageClient {
    provider: Arc<MessagingProvider>,
    router: MessageRouterKind,
}

// Router enum
pub enum MessageRouterKind {
    Default(DefaultMessageRouter),
}
```

### Key Files

| New File | Purpose |
|----------|---------|
| `messaging/service/traits.rs` | MessagingService, QueueMessage traits |
| `messaging/service/provider.rs` | MessagingProvider enum |
| `messaging/service/router.rs` | MessageRouter + MessageRouterKind |
| `messaging/service/pgmq.rs` | PgmqMessagingService |
| `messaging/service/rabbitmq.rs` | RabbitMqMessagingService |
| `messaging/client.rs` | MessageClient struct |

### Files to Delete (Phase 6)

| File | Contains |
|------|----------|
| `messaging/clients/traits.rs` | Old MessageClient trait |
| `messaging/clients/unified_client.rs` | UnifiedMessageClient enum |
| `messaging/mod.rs` (wrapper parts) | UnifiedPgmqClient |

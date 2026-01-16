# TAS-133a: Trait Definitions & Types

**Parent**: TAS-133 (Messaging Service Strategy Pattern Abstraction)
**Status**: Ready for Implementation
**Branch**: `jcoletaylor/tas-133a-trait-definitions`
**Merges To**: `jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction`

---

## Overview

Define the new messaging abstraction layer without breaking existing code. This phase creates the foundational types, traits, and module structure that all subsequent phases build upon.

## Validation Gate

- [ ] Compiles with `cargo check --all-features`
- [ ] Existing test suite passes (`cargo test --all-features`)
- [ ] New types can be imported from `tasker_shared::messaging::service`
- [ ] No changes to existing messaging code paths

## Scope

### Create Module Structure

```
tasker-shared/src/messaging/service/
├── mod.rs           # Module exports
├── traits.rs        # MessagingService, QueueMessage, MessageRouter traits
├── types.rs         # MessageId, ReceiptHandle, QueuedMessage, QueueStats
├── errors.rs        # MessagingError enum
├── router.rs        # DefaultMessageRouter + MessageRouterKind enum
└── provider.rs      # MessagingProvider enum (stub variants for now)
```

### Key Deliverables

1. **`MessagingService` trait** (`traits.rs`)
   - `ensure_queue()`, `ensure_queues()`, `verify_queues()`
   - `send_message<T>()`, `send_batch<T>()`
   - `receive_messages<T>()`
   - `ack_message()`, `nack_message()`, `extend_visibility()`
   - `queue_stats()`, `health_check()`, `provider_name()`

2. **`QueueMessage` trait** (`traits.rs`)
   - `to_bytes()` / `from_bytes()` for serialization abstraction
   - Enables future MessagePack support

3. **`MessageRouter` trait + `MessageRouterKind` enum** (`router.rs`)
   - Namespace-to-queue-name mapping
   - `DefaultMessageRouter` struct implementation
   - Enum dispatch pattern (no `Arc<dyn>`)

4. **Core types** (`types.rs`)
   - `MessageId(String)` - provider-specific message identifier
   - `ReceiptHandle(String)` - handle for ack/nack/extend operations
   - `QueuedMessage<T>` - received message with metadata
   - `QueueStats` - queue depth and health metrics
   - `QueueHealthReport` - startup verification results

5. **Error types** (`errors.rs`)
   - `MessagingError` enum covering all failure modes
   - Integrates with existing `TaskerError`

6. **`MessagingProvider` enum stub** (`provider.rs`)
   - Define variants: `Pgmq`, `RabbitMq`, `InMemory`
   - Stub implementations that compile but panic (real impl in Phase 2)

## Implementation Notes

### Enum Dispatch Pattern

We use enum dispatch instead of `Arc<dyn MessagingService>` for hot-path performance:

```rust
pub enum MessagingProvider {
    Pgmq(PgmqMessagingService),
    RabbitMq(RabbitMqMessagingService),
    InMemory(InMemoryMessagingService),
}

impl MessagingProvider {
    pub async fn send_message<T: QueueMessage>(&self, queue: &str, msg: &T) -> Result<MessageId, MessagingError> {
        match self {
            Self::Pgmq(s) => s.send_message(queue, msg).await,
            Self::RabbitMq(s) => s.send_message(queue, msg).await,
            Self::InMemory(s) => s.send_message(queue, msg).await,
        }
    }
}
```

### MessageRouter Enum Pattern

Same pattern for router to maintain consistency:

```rust
pub enum MessageRouterKind {
    Default(DefaultMessageRouter),
}

impl MessageRouterKind {
    pub fn step_queue(&self, namespace: &str) -> String {
        match self {
            Self::Default(r) => r.step_queue(namespace),
        }
    }
}
```

### QueueMessage Trait

Designed to support both JSON (current) and binary formats (future):

```rust
pub trait QueueMessage: Send + Sync + Clone + 'static {
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError> where Self: Sized;
}
```

## Files to Create

| File | Purpose |
|------|---------|
| `tasker-shared/src/messaging/service/mod.rs` | Module exports and re-exports |
| `tasker-shared/src/messaging/service/traits.rs` | Core trait definitions |
| `tasker-shared/src/messaging/service/types.rs` | Type definitions |
| `tasker-shared/src/messaging/service/errors.rs` | Error types |
| `tasker-shared/src/messaging/service/router.rs` | Router trait and implementations |
| `tasker-shared/src/messaging/service/provider.rs` | Provider enum with stubs |

## Files to Modify

| File | Change |
|------|--------|
| `tasker-shared/src/messaging/mod.rs` | Add `pub mod service;` export |
| `tasker-shared/src/lib.rs` | Ensure messaging module is exported |

## Testing Strategy

1. **Unit tests for DefaultMessageRouter**
   - Queue name generation matches existing patterns
   - Namespace extraction works correctly

2. **Compile-time verification**
   - All traits are object-safe where needed
   - Generic bounds are satisfied

3. **Integration with existing code**
   - New types can be imported alongside old types
   - No conflicts with existing `MessageClient` trait

## Dependencies

- None (this is the foundation phase)

## Estimated Complexity

**Low** - Primarily type definitions and trait design. No runtime behavior changes.

---

## References

- [implementation-plan.md](../implementation-plan.md) - Phase 1 section
- [trait-design.md](../trait-design.md) - Detailed trait semantics
- [research-pgmq-trait-comparison.md](../research-pgmq-trait-comparison.md) - Method mapping

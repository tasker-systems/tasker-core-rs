# TAS-133b: PGMQ Implementation

**Parent**: TAS-133 (Messaging Service Strategy Pattern Abstraction)
**Status**: Ready for Implementation
**Branch**: `jcoletaylor/tas-133b-pgmq-implementation`
**Merges To**: `jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction`
**Depends On**: TAS-133a

---

## Overview

Implement `PgmqMessagingService` that wraps existing pgmq-notify functionality, conforming to the `MessagingService` trait defined in TAS-133a. This phase also adds a prerequisite method to `pgmq-notify` for visibility timeout extension.

## Validation Gate

- [ ] `PgmqMessagingService` implements all `MessagingService` trait methods
- [ ] Conformance test suite passes for PGMQ provider
- [ ] Existing orchestration/worker tests pass unchanged
- [ ] `set_visibility_timeout` added to `pgmq-notify/src/client.rs`
- [ ] `InMemoryMessagingService` implemented for testing

## Scope

### Prerequisite: pgmq-notify Update

Add `set_visibility_timeout` method to `PgmqClient`:

```rust
// pgmq-notify/src/client.rs

/// Extend visibility timeout for a message (heartbeat during long processing)
#[instrument(skip(self), fields(queue = %queue_name, message_id = %message_id))]
pub async fn set_visibility_timeout(
    &self,
    queue_name: &str,
    message_id: i64,
    vt_offset_seconds: i32,
) -> Result<()> {
    self.pgmq.set_vt(queue_name, message_id, vt_offset_seconds).await?;
    Ok(())
}
```

### PgmqMessagingService Implementation

```rust
// tasker-shared/src/messaging/service/pgmq.rs

pub struct PgmqMessagingService {
    client: PgmqClient,  // From pgmq-notify
}
```

Key method implementations:

| Trait Method | PGMQ Implementation |
|--------------|---------------------|
| `ensure_queue()` | `client.create_queue()` |
| `send_message<T>()` | `client.send_json_message()` with `QueueMessage::to_bytes()` |
| `receive_messages<T>()` | `client.read_messages()` → wrap in `QueuedMessage<T>` |
| `ack_message()` | `client.delete_message()` |
| `nack_message(requeue=true)` | No-op (visibility timeout handles retry) |
| `nack_message(requeue=false)` | `client.archive_message()` → moves to `a_{queue}` |
| `extend_visibility()` | `client.set_visibility_timeout()` (new method) |
| `queue_stats()` | `client.queue_metrics()` |
| `health_check()` | Ping query to verify connection |
| `provider_name()` | `"pgmq"` |

### InMemoryMessagingService Implementation

For testing without database:

```rust
// tasker-shared/src/messaging/service/in_memory.rs

pub struct InMemoryMessagingService {
    queues: Arc<RwLock<HashMap<String, VecDeque<StoredMessage>>>>,
    in_flight: Arc<RwLock<HashMap<String, StoredMessage>>>,
}
```

Features:
- Thread-safe queue storage
- Visibility timeout simulation via `in_flight` map
- Receipt handle generation
- Useful for unit tests that don't need real PGMQ

### Update MessagingProvider Enum

Replace stubs with real implementations:

```rust
// tasker-shared/src/messaging/service/provider.rs

pub enum MessagingProvider {
    Pgmq(PgmqMessagingService),
    RabbitMq(RabbitMqMessagingService),  // Still stub
    InMemory(InMemoryMessagingService),
}
```

## Implementation Notes

### Receipt Handle Mapping

PGMQ uses `i64` message IDs. We wrap in `ReceiptHandle(String)`:

```rust
// Sending: i64 → ReceiptHandle
ReceiptHandle(msg_id.to_string())

// Receiving: ReceiptHandle → i64
let msg_id: i64 = receipt_handle.0.parse()
    .map_err(|_| MessagingError::MessageNotFound(...))?;
```

### PGMQ Archive vs DLQ

`nack_message(requeue=false)` archives to PGMQ's built-in `a_{queue_name}` table:

```rust
async fn nack_message(&self, queue_name: &str, receipt_handle: &ReceiptHandle, requeue: bool) -> Result<(), MessagingError> {
    if requeue {
        Ok(())  // VT expiry handles requeue
    } else {
        self.client.archive_message(queue_name, msg_id).await?
    }
}
```

This preserves messages for audit while removing from active queue.

### QueueMessage Implementation for Domain Types

Implement `QueueMessage` for existing domain types:

```rust
impl QueueMessage for StepMessage {
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError> {
        serde_json::to_vec(self)
            .map_err(|e| MessagingError::Serialization(e.to_string()))
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError> {
        serde_json::from_slice(bytes)
            .map_err(|e| MessagingError::Deserialization(e.to_string()))
    }
}
```

Apply same pattern to:
- `StepMessage` (formerly `SimpleStepMessage`)
- `StepExecutionResult`
- `TaskRequestMessage`
- `TaskFinalizationMessage`

## Files to Create

| File | Purpose |
|------|---------|
| `tasker-shared/src/messaging/service/pgmq.rs` | PGMQ provider implementation |
| `tasker-shared/src/messaging/service/in_memory.rs` | Testing provider |

## Files to Modify

| File | Change |
|------|--------|
| `pgmq-notify/src/client.rs` | Add `set_visibility_timeout()` method |
| `tasker-shared/src/messaging/service/provider.rs` | Replace stubs with real impls |
| `tasker-shared/src/messaging/service/mod.rs` | Export new modules |
| `tasker-shared/src/messaging/message.rs` | Implement `QueueMessage` for domain types |

## Testing Strategy

### Conformance Test Suite

Create reusable test functions that work with any `MessagingService`:

```rust
async fn test_send_receive_roundtrip<S: MessagingService>(service: &S) {
    service.ensure_queue("test_queue").await.unwrap();

    let msg = TestMessage { id: 1, data: "hello".into() };
    let msg_id = service.send_message("test_queue", &msg).await.unwrap();

    let received = service.receive_messages::<TestMessage>("test_queue", 1, Duration::from_secs(30)).await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].message.data, "hello");
}
```

Apply to both `PgmqMessagingService` and `InMemoryMessagingService`.

### Integration Tests

- Existing orchestration tests should pass with PGMQ provider
- Worker event consumer should work unchanged

## Dependencies

- TAS-133a (trait definitions must exist)
- Running PostgreSQL with PGMQ extension

## Estimated Complexity

**Medium** - Wrapping existing functionality with new interface. Main complexity is ensuring all edge cases map correctly.

---

## References

- [implementation-plan.md](../implementation-plan.md) - Phase 2 section
- [research-pgmq-trait-comparison.md](../research-pgmq-trait-comparison.md) - Method mapping details
- [pgmq-notify/src/client.rs](../../../../pgmq-notify/src/client.rs) - Existing client

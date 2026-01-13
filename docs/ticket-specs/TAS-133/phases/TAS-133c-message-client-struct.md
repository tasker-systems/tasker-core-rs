# TAS-133c: MessageClient Struct (Domain Facade)

**Parent**: TAS-133 (Messaging Service Strategy Pattern Abstraction)
**Status**: Ready for Implementation
**Branch**: `jcoletaylor/tas-133c-message-client-struct`
**Merges To**: `jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction`
**Depends On**: TAS-133b

---

## Overview

Create the domain-level `MessageClient` struct that provides convenient, Tasker-specific messaging methods. This phase also consolidates `SimpleStepMessage` → `StepMessage` naming.

## Validation Gate

- [ ] `MessageClient` struct compiles and provides all domain operations
- [ ] `SimpleStepMessage` renamed to `StepMessage`
- [ ] Old `StepMessage` (with embedded context) deleted
- [ ] Orchestration step enqueuer works through new `MessageClient`
- [ ] All existing tests pass with renamed type

## Scope

### MessageClient Struct

```rust
// tasker-shared/src/messaging/client.rs

/// Domain-level messaging client
///
/// This is a struct (NOT a trait) that provides convenient domain-specific
/// methods for Tasker's messaging needs. It wraps a MessagingProvider (enum)
/// and a MessageRouterKind (enum) - no trait objects, all enum dispatch.
pub struct MessageClient {
    provider: Arc<MessagingProvider>,
    router: MessageRouterKind,
}
```

### Domain Methods

| Method | Purpose |
|--------|---------|
| `send_step_message(namespace, msg)` | Send step to worker queue |
| `send_step_result(result)` | Send result to orchestration |
| `send_task_request(request)` | Send task init request |
| `send_task_finalization(msg)` | Send finalization message |
| `initialize_namespace_queues(namespaces)` | Bootstrap worker queues |
| `delete_message(queue, receipt)` | Ack processed message |
| `get_queue_metrics(queue)` | Get queue stats |
| `provider_name()` | Get provider identifier |

### Accessor Methods

```rust
impl MessageClient {
    /// Get the underlying messaging provider for advanced operations
    pub fn provider(&self) -> &Arc<MessagingProvider> {
        &self.provider
    }

    /// Get the router for queue name lookups
    pub fn router(&self) -> &MessageRouterKind {
        &self.router
    }
}
```

### StepMessage Consolidation

**Current state**:
- `SimpleStepMessage` - UUID-based, actively used
- `StepMessage` - Embedded context, legacy/deprecated

**After this phase**:
- `StepMessage` - UUID-based (renamed from `SimpleStepMessage`)
- Old `StepMessage` deleted

```rust
// tasker-shared/src/messaging/message.rs

/// Step message with UUID references (workers query DB for context)
///
/// NOTE: As of TAS-133, this replaces the old StepMessage that had embedded
/// execution context. Workers now query the database using these UUIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepMessage {
    pub task_uuid: Uuid,
    pub step_uuid: Uuid,
    pub correlation_id: Uuid,
}
```

## Implementation Notes

### MessageClient vs Old MessageClient Trait

The new `MessageClient` is a **struct**, not a trait. This is intentional:

| Old Pattern | New Pattern |
|-------------|-------------|
| `MessageClient` trait with methods | `MessageClient` struct with methods |
| `impl MessageClient for PgmqClient` | `MessageClient` wraps `MessagingProvider` |
| Trait object possible | No trait object needed |

The struct pattern is simpler and the trait was only used polymorphically in one place (which is being refactored).

### Namespace Queue Routing

`MessageClient` uses `MessageRouterKind` for queue name resolution:

```rust
pub async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()> {
    let queue = self.router.step_queue(namespace);  // e.g., "worker_payments_queue"
    self.provider.send_message(&queue, &message).await?;
    Ok(())
}
```

### Error Conversion

`MessageClient` methods return `TaskerResult<T>`, converting from `MessagingError`:

```rust
impl From<MessagingError> for TaskerError {
    fn from(e: MessagingError) -> Self {
        TaskerError::MessagingError(e.to_string())
    }
}
```

## Files to Create

| File | Purpose |
|------|---------|
| `tasker-shared/src/messaging/client.rs` | MessageClient struct |

## Files to Modify

| File | Change |
|------|--------|
| `tasker-shared/src/messaging/message.rs` | Rename `SimpleStepMessage` → `StepMessage`, delete old |
| `tasker-shared/src/messaging/mod.rs` | Export new client module |
| `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs` | Use renamed `StepMessage` |
| `tasker-worker/src/worker/event_consumer.rs` | Use renamed `StepMessage` |
| All files referencing `SimpleStepMessage` | Rename to `StepMessage` |

## Migration Checklist

- [ ] Create `MessageClient` struct in `client.rs`
- [ ] Implement all domain methods
- [ ] Rename `SimpleStepMessage` → `StepMessage`
- [ ] Delete old `StepMessage` struct
- [ ] Update all imports across codebase
- [ ] Update `send_simple_step_message` → `send_step_message` references
- [ ] Verify step enqueuer compiles
- [ ] Verify event consumer compiles

## Testing Strategy

### Unit Tests

```rust
#[tokio::test]
async fn test_message_client_send_step() {
    let provider = Arc::new(MessagingProvider::InMemory(InMemoryMessagingService::new()));
    let router = MessageRouterKind::Default(DefaultMessageRouter::default());
    let client = MessageClient::new(provider.clone(), router);

    let msg = StepMessage {
        task_uuid: Uuid::new_v4(),
        step_uuid: Uuid::new_v4(),
        correlation_id: Uuid::new_v4(),
    };

    client.send_step_message("payments", msg).await.unwrap();

    // Verify message landed in correct queue
    let in_memory = provider.as_in_memory().unwrap();
    assert!(in_memory.queue_has_messages("worker_payments_queue"));
}
```

### Rename Verification

- Global search for `SimpleStepMessage` should return 0 results
- Global search for `send_simple_step_message` should return 0 results

## Dependencies

- TAS-133b (PGMQ and InMemory implementations must exist)

## Estimated Complexity

**Medium** - Creating the struct is straightforward. The rename operation requires careful search-and-replace across the codebase.

---

## References

- [implementation-plan.md](../implementation-plan.md) - Phase 3 section
- [research-message-client-usage.md](../research-message-client-usage.md) - Current usage patterns

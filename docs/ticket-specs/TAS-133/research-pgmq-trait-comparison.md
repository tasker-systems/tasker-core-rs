# Research: PgmqClientTrait vs TAS-133 MessagingService Comparison

**Last Updated**: 2026-01-13
**Status**: Complete

---

## Summary

The current `PgmqClientTrait` (in `tasker-shared/src/messaging/clients/traits.rs`) is a low-level, PGMQ-specific abstraction with raw queue operations. The TAS-133 `MessagingService` trait is a provider-agnostic, high-level abstraction designed for multi-provider support (PGMQ, RabbitMQ, SQS, Redis, Kafka). Key differences:

1. **Scope**: `PgmqClientTrait` is PGMQ-specific; `MessagingService` is provider-neutral
2. **Message Types**: `PgmqClientTrait` uses PGMQ-native types; `MessagingService` uses generic types
3. **Operation Level**: `PgmqClientTrait` is low-level (read/delete); `MessagingService` is high-level (ack/nack/extend)
4. **Architecture**: `PgmqClientTrait` is trait-only; `MessagingService` includes supporting types (MessageId, ReceiptHandle, QueuedMessage)

---

## Method Comparison Table

| Method | PgmqClientTrait | MessagingService | Notes |
|--------|-----------------|------------------|-------|
| **Queue Creation** | `create_queue()` | `ensure_queue()` + `ensure_queues()` | Spec adds bulk operation; name updated for idempotent semantics |
| | `initialize_namespace_queues()` | (Not in spec) | PGMQ-specific; handled via configuration in spec |
| **Sending Messages** | `send_message()` | `send_message<T>()` | Spec is generic over `QueueMessage` trait |
| | `send_json_message<T>()` | (Subsumed by generic) | Spec unifies both with `QueueMessage` trait |
| | `enqueue_step()` | (Removed; routing via MessageRouter) | Spec uses external MessageRouter trait for queue selection |
| **Receiving Messages** | `read_messages()` | `receive_messages<T>()` | Spec returns `QueuedMessage<T>` (receipt handle + metadata) |
| | `process_namespace_queue()` | (Removed; handled by MessageRouter) | Routing separated into MessageRouter trait |
| **Message Acknowledgment** | `delete_message()` | `ack_message()` | Same semantic; spec name is clearer for message queue patterns |
| | (Not present) | `nack_message()` | **NEW**: Negative ack with optional requeue |
| | `archive_message()` | (Not in spec) | **PGMQ-SPECIFIC**: Archive pattern; DLQ handled via `SupportsDeadLetter` marker trait |
| | (Not present) | `extend_visibility()` | **NEW**: Heartbeat for long-running processing |
| **Queue Management** | `purge_queue()` | (Not in spec) | Removed; not recommended for production |
| | `drop_queue()` | (Not in spec) | Removed; queue lifecycle managed via configuration |
| **Metrics** | `queue_metrics()` | `queue_stats()` | Spec adds `oldest_message_age` and `in_flight_count` fields |
| **Health/Status** | (Not present) | `health_check()` | **NEW**: Provider-level health check |
| | (Not present) | `verify_queues()` | **NEW**: Startup validation (returns detailed health report) |
| | (Not present) | `provider_name()` | **NEW**: Debugging/observability; returns `&'static str` |

---

## PGMQ-Specific Functionality to Preserve

### 1. Archive Pattern (`archive_message()`)

**Current**: Direct method in `PgmqClientTrait`
```rust
async fn archive_message(&self, queue_name: &str, message_id: i64) -> MessagingResult<()>;
```

**PGMQ Capability**: PGMQ provides built-in archive tables and `pgmq.archive()` SQL function for moving messages to a separate archive queue without deleting them.

**Spec Strategy**: Use `SupportsDeadLetter` marker trait instead. Archive becomes a DLQ pattern:
- Configure with `[messaging.dead_letter]` section
- Failed messages automatically move to `{queue_name}_dlq` after `max_receive_count`
- This generalizes across providers (PGMQ, RabbitMQ both support this)

### 2. Namespace Queue Initialization (`initialize_namespace_queues()`)

**Current**:
```rust
async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> MessagingResult<()>;
```

**PGMQ Specific**: Creates queues matching pattern `worker_{namespace}_queue` for known namespaces.

**Spec Strategy**: Removed from trait; handled via:
- Task template discovery mechanism discovers namespaces at startup
- `ensure_queues()` called with discovered queue names during worker bootstrap
- MessageRouter trait handles queue naming pattern (pluggable)

### 3. Namespace-Based Queue Operations

**Current**: Dedicated methods that bundle queue name calculation with operations
```rust
async fn enqueue_step(&self, namespace: &str, step_message: PgmqStepMessage) -> MessagingResult<i64>;
async fn process_namespace_queue(&self, namespace: &str, ...) -> MessagingResult<Vec<...>>;
async fn complete_message(&self, namespace: &str, message_id: i64) -> MessagingResult<()>;
```

**Spec Strategy**: Separated concerns:
- `MessageRouter` trait handles namespace → queue_name mapping
- Basic `send_message()`, `receive_messages()` use router externally
- More composable; enables custom routing strategies

### 4. Raw JSON Message Handling (`send_json_message<T>()`)

**Current**: Generic JSON serialization in `PgmqClientTrait`
```rust
async fn send_json_message<T: Serialize + Clone + Send + Sync>(...) -> MessagingResult<i64>;
```

**Spec Strategy**: Unified via `QueueMessage` trait
```rust
pub trait QueueMessage: Send + Sync + Clone + 'static {
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError>;
}
```

This allows:
- Custom serialization per message type (JSON, bincode, protobuf, etc.)
- Better type safety
- Cross-language FFI serialization (critical for Ruby/Python workers)

---

## Mapping Strategy: PgmqClientTrait → MessagingService

### Layer 1: Direct Mapping (No Changes)

| PgmqClientTrait | MessagingService |
|-----------------|-----------------|
| `create_queue()` | `ensure_queue()` |
| `delete_message()` | `ack_message()` |
| `queue_metrics()` | `queue_stats()` (return type expanded) |

### Layer 2: Semantic Mapping (Behavior Changes)

| PgmqClientTrait | MessagingService | Strategy |
|-----------------|-----------------|----------|
| `read_messages()` | `receive_messages()` | Wrap `Vec<PgmqMessage<JSON>>` into `Vec<QueuedMessage<T>>` with receipt handle |
| `send_message()` | `send_message()` | Implement `QueueMessage` for `PgmqStepMessage`; convert at call site |
| `send_json_message()` | `send_message()` | Consolidate; use `QueueMessage` impl for JSON types |

### Layer 3: Decomposed Mapping (Extracted Concerns)

| PgmqClientTrait | Spec Component | Strategy |
|-----------------|----------------|----------|
| `enqueue_step()` | `MessageRouter + send_message()` | Extract queue name calculation into router; use base operation |
| `process_namespace_queue()` | `MessageRouter + receive_messages()` | Same as above |
| `complete_message()` | `MessageRouter + ack_message()` | Same as above |
| `initialize_namespace_queues()` | (Task template discovery) | Move to bootstrap phase; call `ensure_queues()` |

### Layer 4: Provider-Specific Mapping (Via Marker Traits)

| PgmqClientTrait | Spec Marker Trait | Strategy |
|-----------------|-------------------|----------|
| `archive_message()` | `SupportsDeadLetter::configure_dead_letter()` | PGMQ: Configure DLQ; automatic archival on `nack()` |
| (N/A) | `SupportsPushNotifications::subscribe()` | PGMQ: Leverage `pgmq-notify` for pg_notify streaming |

---

## Critical Preservation Points

### 1. Receipt Handles for PGMQ

PGMQ doesn't have traditional receipt handles (unlike RabbitMQ/SQS). The spec handles this with:
```rust
pub struct ReceiptHandle(pub String);
```

**PGMQ Implementation**:
```rust
// For PGMQ, ReceiptHandle = message_id (as string)
// Implementation detail hidden in PgmqMessagingService
```

### 2. Message Metadata Forwarding

`PgmqStepMessage` includes metadata: `enqueued_at`, `retry_count`, `max_retries`, `timeout_seconds`, `correlation_id`.

**Spec Handling**: Via `QueuedMessage<T>` wrapper:
```rust
pub struct QueuedMessage<T> {
    pub receipt_handle: ReceiptHandle,
    pub message: T,
    pub receive_count: u32,  // Maps to retry_count
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}
```

- Spec's `receive_count` maps to PGMQ's retry tracking
- PGMQ's `timeout_seconds` and `correlation_id` stay inside the message payload (T)

### 3. Visibility Timeout Semantics

PGMQ uses `visibility_timeout` (seconds). Spec uses `Duration`.

**Mapping**:
```rust
// Current PGMQ
read_messages(queue_name, Some(30), Some(10))  // 30 sec timeout, 10 messages

// Spec equivalent
receive_messages(queue_name, 10, Duration::from_secs(30))
```

### 4. Atomic Notification (`pgmq_send_with_notify()`)

PGMQ achieves push notifications via atomic `pgmq_send_with_notify()` wrapper function.

**Spec Integration**:
- Handled via `SupportsPushNotifications` marker trait
- PGMQ implementation uses existing `pgmq-notify` crate's notification mechanism
- Callers don't need to know about this; it's transparent

---

## Gaps and Concerns

### Gap 1: Nack with Requeue Logic

**Current State**: PGMQ has `archive_message()` but no explicit nack

**Spec Addition**:
```rust
async fn nack_message(
    &self,
    queue_name: &str,
    receipt_handle: &ReceiptHandle,
    requeue: bool,
) -> Result<(), MessagingError>;
```

**PGMQ Implementation Options**:
- `nack(requeue=true)`: No-op - PGMQ visibility timeout handles this automatically
- `nack(requeue=false)`: Archive to DLQ if configured

### Gap 2: Visibility Extension During Processing

**Current State**: No extend/heartbeat in `PgmqClientTrait`

**Spec Addition**:
```rust
async fn extend_visibility(
    &self,
    queue_name: &str,
    receipt_handle: &ReceiptHandle,
    extension: Duration,
) -> Result<(), MessagingError>;
```

**PGMQ Implementation**:
- PGMQ has `pgmq.set_vt()` function that can reset visibility timeout
- Can be implemented using this

### Gap 3: Health Check Startup Validation

**Current State**: No startup validation in `PgmqClientTrait`

**Spec Addition**:
```rust
async fn verify_queues(&self, queue_names: &[String]) -> Result<QueueHealthReport, MessagingError>;
```

**PGMQ Implementation**:
- Query `pgmq.meta` table to verify queues exist
- Straightforward addition

### Gap 4: Push Notification Abstraction

**Current State**: Handled by pgmq-notify crate separately

**Spec Addition**: `SupportsPushNotifications` trait with `subscribe()` and `subscribe_pattern()`

**PGMQ Implementation**:
- Wrap existing `pgmq-notify` PgmqNotifyListener
- Return stream of `MessageNotification::Available` events

---

## Conclusion

The `PgmqClientTrait` is a solid PGMQ-specific foundation. The TAS-133 `MessagingService` trait is a superset abstraction that:

1. **Generalizes** PGMQ operations (read → receive, delete → ack)
2. **Adds missing patterns** (nack, extend_visibility, health checks)
3. **Separates concerns** (routing via `MessageRouter`, capabilities via marker traits)
4. **Enables providers** (PGMQ, RabbitMQ become first-class with same interface)

The migration preserves PGMQ's strengths (atomic notifications, archive pattern) while making room for alternatives.

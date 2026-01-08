# Trait Design Deep Dive

**Last Updated**: 2026-01-07

This document provides detailed design rationale for the messaging service traits.

---

## Design Principles

### 1. Lowest Common Denominator

The `MessagingService` trait exposes only operations that ALL providers can implement:

| Operation | PGMQ | RabbitMQ | SQS | Redis Streams |
|-----------|------|----------|-----|---------------|
| send_message | ✅ | ✅ | ✅ | ✅ |
| receive_messages | ✅ | ✅ | ✅ | ✅ |
| ack_message | ✅ (delete) | ✅ (ack) | ✅ (delete) | ✅ (xack) |
| nack_message | ✅ (archive) | ✅ (nack) | ✅ (change visibility) | ✅ (xclaim) |
| extend_visibility | ⚠️ | ⚠️ | ✅ | ⚠️ |
| queue_stats | ✅ | ✅ | ✅ | ✅ |

Provider-specific features use marker traits (`SupportsDeadLetter`, etc.).

### 2. Visibility Timeout as Universal Concept

All work queue systems need a way to handle "message in flight but worker died":

| Provider | Native Mechanism | Our Abstraction |
|----------|------------------|-----------------|
| PGMQ | `vt` column, `read()` with timeout | visibility_timeout parameter |
| RabbitMQ | Consumer timeout, manual ack | Timeout on consumer, nack on timeout |
| SQS | Native visibility timeout | Direct mapping |
| Redis | `XPENDING` + `XCLAIM` | Periodic claim check |

### 3. Receipt Handle Opacity

The `ReceiptHandle` type is opaque to callers:

```rust
// PGMQ: message ID (i64 as string)
ReceiptHandle("12345")

// RabbitMQ: delivery tag (u64 as string)  
ReceiptHandle("67890")

// SQS: actual receipt handle string
ReceiptHandle("AQEBwJnKyr...")
```

Callers never parse or construct receipt handles - they're returned from `receive_messages` and passed to `ack_message`/`nack_message`.

---

## Trait Method Details

### `ensure_queue`

```rust
async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError>;
```

**Semantics**: Create queue if it doesn't exist, no-op if it does.

| Provider | Implementation |
|----------|----------------|
| PGMQ | `pgmq.create(queue_name)` (idempotent) |
| RabbitMQ | `channel.queue_declare(queue_name, durable: true)` |
| SQS | `create_queue` with idempotency |

**Rationale**: Tasker creates queues per namespace at startup. Idempotent creation simplifies orchestration restarts.

### `send_message` / `send_batch`

```rust
async fn send_message<T: QueueMessage>(
    &self,
    queue_name: &str,
    message: &T,
) -> Result<MessageId, MessagingError>;
```

**Semantics**: Enqueue message, return provider-specific message ID.

| Provider | Implementation |
|----------|----------------|
| PGMQ | `pgmq.send(queue_name, json)` returns `msg_id` |
| RabbitMQ | `basic_publish` to default exchange with routing_key = queue_name |
| SQS | `send_message` returns `MessageId` |

**Batch Optimization**: `send_batch` allows providers to optimize (PGMQ `send_batch`, RabbitMQ publisher confirms, SQS `send_message_batch`).

### `receive_messages`

```rust
async fn receive_messages<T: QueueMessage>(
    &self,
    queue_name: &str,
    max_messages: usize,
    visibility_timeout: Duration,
) -> Result<Vec<QueuedMessage<T>>, MessagingError>;
```

**Semantics**: Fetch up to `max_messages`, making them invisible for `visibility_timeout`.

| Provider | Implementation |
|----------|----------------|
| PGMQ | `pgmq.read(queue_name, vt, qty)` |
| RabbitMQ | `basic_consume` with prefetch, timeout on consumer |
| SQS | `receive_message` with `VisibilityTimeout` |

**Important**: This is a pull operation. Providers MUST NOT push messages to handlers.

### `ack_message`

```rust
async fn ack_message(
    &self,
    queue_name: &str,
    receipt_handle: &ReceiptHandle,
) -> Result<(), MessagingError>;
```

**Semantics**: Permanently remove message from queue (successful processing).

| Provider | Implementation |
|----------|----------------|
| PGMQ | `pgmq.delete(queue_name, msg_id)` |
| RabbitMQ | `basic_ack(delivery_tag)` |
| SQS | `delete_message(receipt_handle)` |

### `nack_message`

```rust
async fn nack_message(
    &self,
    queue_name: &str,
    receipt_handle: &ReceiptHandle,
    requeue: bool,
) -> Result<(), MessagingError>;
```

**Semantics**: 
- `requeue = true`: Return message to queue for retry
- `requeue = false`: Discard (or send to DLQ if configured)

| Provider | Implementation |
|----------|----------------|
| PGMQ | `requeue=true`: `archive` then `send`; `requeue=false`: `archive` |
| RabbitMQ | `basic_nack(delivery_tag, requeue)` |
| SQS | `requeue=true`: wait for visibility timeout; `requeue=false`: delete |

### `extend_visibility`

```rust
async fn extend_visibility(
    &self,
    queue_name: &str,
    receipt_handle: &ReceiptHandle,
    extension: Duration,
) -> Result<(), MessagingError>;
```

**Semantics**: Extend the visibility timeout for long-running handlers.

| Provider | Implementation |
|----------|----------------|
| PGMQ | `pgmq.set_vt(queue_name, msg_id, new_vt)` |
| RabbitMQ | Not directly supported - use longer initial timeout |
| SQS | `change_message_visibility` |

**Note**: Some providers don't support this. Handlers needing heartbeat should check provider capabilities or use generous initial timeouts.

---

## QueueMessage Trait

```rust
pub trait QueueMessage: Send + Sync + Clone + 'static {
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError>
    where
        Self: Sized;
}
```

### Why Bytes, Not JSON?

1. **Provider Flexibility**: Some providers have size limits or prefer binary
2. **Compression**: Allows transparent compression at trait boundary
3. **Non-JSON Formats**: MessagePack, Protocol Buffers possible

### Default JSON Implementation

```rust
impl<T> QueueMessage for T
where
    T: Serialize + DeserializeOwned + Send + Sync + Clone + 'static,
{
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

---

## Capability Marker Traits

### Pattern: Trait + Runtime Check

```rust
// Compile-time capability
pub trait SupportsDeadLetter: MessagingService { ... }

// Runtime check for dynamic dispatch
impl dyn MessagingService {
    pub fn supports_dead_letter(&self) -> bool {
        // Use Any or provider_name() check
    }
}
```

### Dead Letter Queues

```rust
pub trait SupportsDeadLetter: MessagingService {
    async fn configure_dead_letter(
        &self,
        source_queue: &str,
        dead_letter_queue: &str,
        max_receive_count: u32,
    ) -> Result<(), MessagingError>;

    async fn get_dead_letter_messages<T: QueueMessage>(
        &self,
        dead_letter_queue: &str,
        max_messages: usize,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError>;
}
```

| Provider | DLQ Support |
|----------|-------------|
| PGMQ | Manual via `archive` table or separate queue |
| RabbitMQ | Native dead letter exchanges |
| SQS | Native DLQ configuration |

### Priority Queues

```rust
pub trait SupportsPriority: MessagingService {
    async fn send_with_priority<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
        priority: u8,  // 0-255, higher = more urgent
    ) -> Result<MessageId, MessagingError>;
}
```

| Provider | Priority Support |
|----------|------------------|
| PGMQ | Not native, could use separate queues |
| RabbitMQ | Native priority queues |
| SQS | Not supported |

---

## Error Handling Strategy

### Retryable vs Fatal Errors

```rust
impl MessagingError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, 
            MessagingError::Connection(_) |
            MessagingError::Provider(_)  // Depends on inner error
        )
    }

    pub fn is_fatal(&self) -> bool {
        matches!(self,
            MessagingError::Serialization(_) |
            MessagingError::Deserialization(_)
        )
    }
}
```

### Circuit Breaker Integration

```rust
impl<T: MessagingService> CircuitBreakerWrapper<T> {
    pub async fn send_message<M: QueueMessage>(
        &self,
        queue_name: &str,
        message: &M,
    ) -> Result<MessageId, MessagingError> {
        if self.breaker.is_open() {
            return Err(MessagingError::CircuitBreakerOpen);
        }
        
        match self.inner.send_message(queue_name, message).await {
            Ok(id) => {
                self.breaker.record_success();
                Ok(id)
            }
            Err(e) if e.is_retryable() => {
                self.breaker.record_failure();
                Err(e)
            }
            Err(e) => Err(e),  // Don't count fatal errors against breaker
        }
    }
}
```

---

## FFI Considerations

### Message Types Cross FFI

These types are used by Ruby/Python/TypeScript workers:

```rust
// tasker-shared/src/messaging/mod.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleStepMessage {
    pub task_uuid: Uuid,
    pub workflow_step_uuid: Uuid,
    pub task_namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionResult {
    pub workflow_step_uuid: Uuid,
    pub status: StepStatus,
    pub output: Option<Value>,
    pub error_message: Option<String>,
}
```

These MUST remain unchanged regardless of messaging provider.

### What Doesn't Cross FFI

- `ReceiptHandle` - Rust infrastructure handles ack/nack
- `QueueStats` - Used by Rust orchestration only
- Provider-specific configuration

---

## Push-Based Notifications

### Why Push Matters

Tasker's `pgmq-notify` integration demonstrates that push-based notifications dramatically improve throughput:

| Metric | Poll-Only | Push + Fallback Poll |
|--------|-----------|----------------------|
| 7-step DAG completion | ~500ms+ | < 133ms |
| Latency variance | High (poll interval) | Low (immediate) |
| CPU efficiency | Wasteful polling | Event-driven |

### Provider Push Models

**PGMQ + pg_notify** (Signal-based):
```rust
// Notification contains queue name only
LISTEN pgmq_message_ready.fulfillment_queue;

// On notification:
//   1. Wake up listener
//   2. Call pgmq.read() to fetch actual messages
//   3. Process messages
```

**RabbitMQ via lapin** (Message-based):
```rust
// Consumer receives full messages
let consumer = channel.basic_consume("queue", "tag", opts, args).await?;

while let Some(delivery) = consumer.next().await {
    let delivery = delivery?;
    // delivery.data contains full message - no separate fetch needed
    process(&delivery.data).await;
    delivery.ack(BasicAckOptions::default()).await?;
}
```

### Trait Design

The `SupportsPushNotifications` trait abstracts over these models:

```rust
pub enum MessageNotification {
    /// PGMQ-style: "messages exist, go fetch"
    Available { queue_name: String },
    
    /// RabbitMQ-style: "here's the message"
    Message(QueuedMessage<Vec<u8>>),
}

pub trait SupportsPushNotifications: MessagingService {
    fn subscribe(
        &self,
        queue_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;
}
```

### Hybrid Consumer Pattern

Tasker uses push as primary path with poll as fallback:

```rust
pub struct HybridConsumer<S: MessagingService + SupportsPushNotifications> {
    service: Arc<S>,
    push_stream: Pin<Box<dyn Stream<Item = MessageNotification> + Send>>,
    fallback_interval: Duration,
}

impl<S> HybridConsumer<S>
where
    S: MessagingService + SupportsPushNotifications,
{
    pub async fn next_batch(&mut self) -> Result<Vec<QueuedMessage<Vec<u8>>>, MessagingError> {
        tokio::select! {
            // Primary: immediate notification
            Some(notification) = self.push_stream.next() => {
                self.handle_notification(notification).await
            }
            // Fallback: catch up on any missed messages
            _ = tokio::time::sleep(self.fallback_interval) => {
                self.service.receive_messages("*", 10, Duration::from_secs(30)).await
            }
        }
    }
}
```

### Provider-Specific Considerations

**PGMQ**:
- pg_notify fires in same transaction as message insert (atomic)
- Notification payload limited to 8KB (just signals, doesn't include message)
- Requires PostgreSQL connection per listener

**RabbitMQ**:
- Native push, no enhancement needed
- Prefetch controls backpressure
- Connection multiplexing via channels
- Consumer cancellation for graceful shutdown

**SQS**:
- No native push
- Long polling (20s max) is best option
- Could use SNS → SQS → Lambda for pseudo-push (adds latency)

---

## Testing Strategy

### Conformance Tests

```rust
// tests/messaging/conformance.rs

async fn test_basic_send_receive<S: MessagingService>(service: &S) {
    let queue = "test_conformance";
    service.ensure_queue(queue).await.unwrap();

    let message = TestMessage { id: 1, data: "test".into() };
    let msg_id = service.send_message(queue, &message).await.unwrap();
    assert!(!msg_id.0.is_empty());

    let received = service.receive_messages::<TestMessage>(
        queue, 10, Duration::from_secs(30)
    ).await.unwrap();
    
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].message.id, 1);

    service.ack_message(queue, &received[0].receipt_handle).await.unwrap();

    // Verify message is gone
    let empty = service.receive_messages::<TestMessage>(
        queue, 10, Duration::from_secs(1)
    ).await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn pgmq_conformance() {
    let service = PgmqMessagingService::new(...).await.unwrap();
    test_basic_send_receive(&service).await;
}

#[tokio::test]
async fn rabbitmq_conformance() {
    let service = RabbitMqMessagingService::new(...).await.unwrap();
    test_basic_send_receive(&service).await;
}
```

### Property-Based Tests

```rust
#[tokio::test]
async fn messages_are_delivered_exactly_once() {
    // Send N messages, receive and ack, verify count matches
}

#[tokio::test]
async fn visibility_timeout_returns_unacked_messages() {
    // Receive without ack, wait for timeout, receive again
}

#[tokio::test]
async fn nack_with_requeue_makes_message_available() {
    // Receive, nack with requeue=true, receive again
}
```

# TAS-133d: RabbitMQ Implementation

**Parent**: TAS-133 (Messaging Service Strategy Pattern Abstraction)
**Status**: Ready for Implementation
**Branch**: `jcoletaylor/tas-133d-rabbitmq-implementation`
**Merges To**: `jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction`
**Depends On**: TAS-133b

---

## Overview

Implement `RabbitMqMessagingService` using the `lapin` crate, providing an alternative message broker to PGMQ. This enables Tasker deployments that prefer RabbitMQ's operational model.

## Validation Gate

- [ ] `RabbitMqMessagingService` implements all `MessagingService` trait methods
- [ ] Same conformance test suite passes for RabbitMQ as for PGMQ
- [ ] RabbitMQ runs in `docker/docker-compose.test.yml`
- [ ] Provider can be selected via TOML configuration
- [ ] Basic send/receive works end-to-end with RabbitMQ

## Scope

### Add lapin Dependency

```toml
# tasker-shared/Cargo.toml
[dependencies]
lapin = "2.3"  # AMQP 0.9.1 client
```

### RabbitMqMessagingService Implementation

```rust
// tasker-shared/src/messaging/service/rabbitmq.rs

use lapin::{Connection, Channel, BasicProperties, options::*, types::FieldTable};

pub struct RabbitMqMessagingService {
    connection: Connection,
    channel: Channel,
    config: RabbitMqConfig,
}
```

### Method Mapping

| Trait Method | RabbitMQ Implementation |
|--------------|-------------------------|
| `ensure_queue()` | `channel.queue_declare()` with durable=true |
| `send_message<T>()` | `channel.basic_publish()` with persistent delivery |
| `receive_messages<T>()` | `channel.basic_get()` with visibility via prefetch |
| `ack_message()` | `channel.basic_ack()` |
| `nack_message(requeue=true)` | `channel.basic_nack(requeue=true)` |
| `nack_message(requeue=false)` | `channel.basic_nack(requeue=false)` + DLX routing |
| `extend_visibility()` | No direct equivalent - log warning, return Ok |
| `queue_stats()` | `channel.queue_declare_passive()` for message count |
| `health_check()` | Connection status check |
| `provider_name()` | `"rabbitmq"` |

### Key Differences from PGMQ

| Feature | PGMQ | RabbitMQ |
|---------|------|----------|
| Visibility timeout | Native `set_vt()` | Via consumer prefetch |
| Archive/DLQ | `a_{queue}` tables | Dead Letter Exchange (DLX) |
| Message ID | `i64` from sequence | Delivery tag (u64) |
| Batch receive | Single query | Loop with `basic_get()` |

### RabbitMQ Configuration

```rust
// tasker-shared/src/config/components/messaging.rs

#[derive(Debug, Clone, Deserialize)]
pub struct RabbitMqConfig {
    pub url: String,              // "amqp://guest:guest@localhost:5672/"
    pub vhost: String,            // "/"
    pub prefetch_count: u16,      // 10
    pub heartbeat_seconds: u16,   // 60
    pub connection_timeout_seconds: u32,  // 30
}
```

### Docker Compose Update

```yaml
# docker/docker-compose.test.yml

services:
  rabbitmq:
    image: rabbitmq:3-management
    ports:
      - "5672:5672"
      - "15672:15672"  # Management UI
    environment:
      RABBITMQ_DEFAULT_USER: guest
      RABBITMQ_DEFAULT_PASS: guest
    healthcheck:
      test: ["CMD", "rabbitmq-diagnostics", "check_running"]
      interval: 10s
      timeout: 5s
      retries: 5
```

## Implementation Notes

### Connection Management

```rust
impl RabbitMqMessagingService {
    pub async fn new(config: &RabbitMqConfig) -> Result<Self, MessagingError> {
        let connection = Connection::connect(
            &config.url,
            ConnectionProperties::default()
                .with_connection_name("tasker".into()),
        )
        .await
        .map_err(|e| MessagingError::Connection(e.to_string()))?;

        let channel = connection.create_channel()
            .await
            .map_err(|e| MessagingError::Connection(e.to_string()))?;

        // Set prefetch for backpressure
        channel.basic_qos(config.prefetch_count, BasicQosOptions::default())
            .await
            .map_err(|e| MessagingError::Provider(e.to_string()))?;

        Ok(Self { connection, channel, config })
    }
}
```

### Visibility Timeout Limitation

RabbitMQ doesn't have a direct visibility timeout extension like PGMQ's `set_vt()`. Options:

1. **Log and continue** - `extend_visibility()` logs a warning and returns Ok
2. **Nack and requeue** - More disruptive, message goes to back of queue
3. **Future: Consumer acknowledgement timeout** - RabbitMQ 3.12+ has per-consumer timeout

For now, we use option 1:

```rust
async fn extend_visibility(&self, ...) -> Result<(), MessagingError> {
    tracing::warn!(
        "RabbitMQ does not support visibility extension; message may timeout"
    );
    Ok(())
}
```

### Dead Letter Exchange Setup

For `nack_message(requeue=false)`:

```rust
async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError> {
    // Create DLX for this queue
    let dlx_name = format!("{}_dlx", queue_name);
    let dlq_name = format!("{}_dlq", queue_name);

    self.channel.exchange_declare(&dlx_name, ExchangeKind::Direct, ...)
        .await?;
    self.channel.queue_declare(&dlq_name, ...)
        .await?;
    self.channel.queue_bind(&dlq_name, &dlx_name, queue_name, ...)
        .await?;

    // Create main queue with DLX
    let mut args = FieldTable::default();
    args.insert("x-dead-letter-exchange".into(), dlx_name.into());

    self.channel.queue_declare(queue_name, QueueDeclareOptions { durable: true, .. }, args)
        .await?;

    Ok(())
}
```

### Receipt Handle Mapping

RabbitMQ uses `DeliveryTag` (u64). We store channel reference in receipt handle:

```rust
// Simple approach: just delivery tag
ReceiptHandle(delivery.delivery_tag.to_string())

// Ack uses stored channel
async fn ack_message(&self, _queue_name: &str, receipt_handle: &ReceiptHandle) -> Result<(), MessagingError> {
    let tag: u64 = receipt_handle.0.parse()
        .map_err(|_| MessagingError::MessageNotFound(...))?;
    self.channel.basic_ack(tag, BasicAckOptions::default())
        .await
        .map_err(|e| MessagingError::Provider(e.to_string()))
}
```

## Files to Create

| File | Purpose |
|------|---------|
| `tasker-shared/src/messaging/service/rabbitmq.rs` | RabbitMQ provider implementation |

## Files to Modify

| File | Change |
|------|--------|
| `tasker-shared/Cargo.toml` | Add `lapin` dependency |
| `tasker-shared/src/messaging/service/provider.rs` | Replace RabbitMq stub with real impl |
| `tasker-shared/src/messaging/service/mod.rs` | Export rabbitmq module |
| `tasker-shared/src/config/components/messaging.rs` | Add RabbitMqConfig |
| `docker/docker-compose.test.yml` | Add RabbitMQ service |

## Testing Strategy

### Conformance Tests

Run same test suite as PGMQ:

```rust
#[tokio::test]
async fn test_rabbitmq_send_receive() {
    let config = RabbitMqConfig::from_env();
    let service = RabbitMqMessagingService::new(&config).await.unwrap();
    test_send_receive_roundtrip(&service).await;
}
```

### Docker Integration

Tests require RabbitMQ running:

```bash
docker compose -f docker/docker-compose.test.yml up -d rabbitmq
cargo test --all-features --package tasker-shared rabbitmq
```

### Comparison Tests

Verify both providers produce equivalent behavior:

```rust
#[tokio::test]
async fn test_provider_equivalence() {
    let pgmq = PgmqMessagingService::new(pool).await.unwrap();
    let rabbit = RabbitMqMessagingService::new(&config).await.unwrap();

    // Same test, both providers
    run_equivalence_suite(&pgmq, &rabbit).await;
}
```

## Dependencies

- TAS-133b (conformance test suite must exist)
- Docker with RabbitMQ image

## Estimated Complexity

**Medium-High** - New external dependency, different operational model. DLX setup adds complexity.

---

## References

- [implementation-plan.md](../implementation-plan.md) - Phase 4 section
- [lapin documentation](https://docs.rs/lapin/latest/lapin/)
- [RabbitMQ Dead Letter Exchanges](https://www.rabbitmq.com/dlx.html)

# Push-Based Notification Research

**Last Updated**: 2026-01-07
**Status**: Research Required

This document explores push-based message notification capabilities across messaging providers, motivated by Tasker's exceptional throughput with `pgmq-notify` + `pg_notify`.

---

## Context: Tasker's Current Architecture

Tasker achieves remarkable throughput through a **hybrid event-driven architecture**:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     TASKER EVENT-DRIVEN MODEL                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   PRIMARY PATH (Push)                 FALLBACK PATH (Poll)              │
│   ──────────────────                  ────────────────────              │
│                                                                          │
│   PGMQ Queue                          PGMQ Queue                        │
│       │                                   │                              │
│       ▼                                   │                              │
│   pg_notify trigger                       │                              │
│       │                                   │                              │
│       ▼                                   ▼                              │
│   LISTEN channel ◄─────────────────► Periodic Poll                      │
│       │                               (catch-up)                        │
│       ▼                                   │                              │
│   Immediate Processing                    │                              │
│       │                                   │                              │
│       └───────────────┬──────────────────┘                              │
│                       ▼                                                  │
│               Handler Execution                                          │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Performance Characteristics

With `pg_notify` push notifications:
- **Complex 7-step DAG completion**: < 133ms end-to-end
- **Concurrency**: Hundreds of tasks simultaneously
- **Hardware**: Even on resource-constrained laptops
- **Discovery**: Sub-millisecond transaction races exposed (previously invisible bugs)

This performance comes from eliminating polling latency on the critical path.

---

## Provider Push Capabilities

### PGMQ + pg_notify (Current)

**Model**: Poll-native with push enhancement via PostgreSQL NOTIFY/LISTEN

```rust
// pgmq-notify crate provides this abstraction
pub struct PgmqNotifyListener {
    // LISTEN on pgmq_message_ready.{namespace}
}

// Message flow:
// 1. pgmq.send() → trigger fires → NOTIFY pgmq_message_ready.{namespace}
// 2. Listener receives notification immediately
// 3. Listener calls pgmq.read() to get actual message
```

**Pros**:
- Sub-millisecond notification latency
- Atomic with message insertion (same transaction)
- No additional infrastructure

**Cons**:
- PostgreSQL connection per listener
- NOTIFY payload size limited (8KB)
- Notification is "message exists", not message content

---

### RabbitMQ (via lapin)

**Model**: **Natively push-based** - This is actually RabbitMQ's strength!

```rust
use lapin::{options::*, Consumer};
use futures_lite::StreamExt;

// RabbitMQ consumers receive messages as they arrive
let consumer: Consumer = channel
    .basic_consume(
        "queue_name",
        "consumer_tag",
        BasicConsumeOptions::default(),
        FieldTable::default(),
    )
    .await?;

// This is an async Stream - messages pushed as they arrive
while let Some(delivery) = consumer.next().await {
    let delivery = delivery?;
    // Message content is immediately available
    process_message(&delivery.data).await;
    delivery.ack(BasicAckOptions::default()).await?;
}
```

**Key Insight**: RabbitMQ doesn't need a "pg_notify equivalent" - it's push by default!

**lapin Push Features**:
| Feature | Support | Notes |
|---------|---------|-------|
| Async Stream consumer | ✅ | `futures::Stream` impl |
| Prefetch control | ✅ | `basic_qos` for backpressure |
| Multiple consumers | ✅ | Competing consumers pattern |
| Consumer cancellation | ✅ | Graceful shutdown |
| Heartbeats | ✅ | Connection health |

**Pros**:
- Native push - no enhancement needed
- Message content delivered immediately (not just notification)
- Mature, battle-tested

**Cons**:
- Requires RabbitMQ infrastructure
- Connection management complexity
- No transactional coupling with PostgreSQL

---

### AWS SQS

**Model**: **Poll-only** - No native push

```rust
// SQS requires polling
loop {
    let response = sqs_client
        .receive_message()
        .queue_url(&queue_url)
        .wait_time_seconds(20)  // Long polling (max 20s)
        .send()
        .await?;
    
    for message in response.messages.unwrap_or_default() {
        process_message(&message).await;
    }
}
```

**Push Workaround**: SNS → SQS → Lambda, but adds latency and complexity.

**Characteristics**:
| Aspect | Value |
|--------|-------|
| Best-case latency | ~20-100ms (long poll return) |
| Typical latency | 100-500ms |
| Push support | ❌ (requires SNS/Lambda) |

---

### Redis Streams

**Model**: **Blocking read** - Pseudo-push

```rust
// XREAD with BLOCK provides push-like semantics
let result = redis_client
    .xread_options(
        &["mystream"],
        &[">"],  // Only new messages
        &StreamReadOptions::default()
            .block(0)  // Block indefinitely
            .count(10),
    )
    .await?;
```

**Characteristics**:
- `BLOCK 0` provides immediate notification on new messages
- Consumer groups for competing consumers
- Not true push (still a blocking call), but low latency

---

## Trait Design Implications

### Current Trait (Pull-Only)

```rust
#[async_trait]
pub trait MessagingService: Send + Sync + 'static {
    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError>;
}
```

This works for all providers but doesn't leverage push capabilities.

### Proposed: Push-Capable Extension

```rust
/// Marker trait for providers supporting push notifications
pub trait SupportsPushNotifications: MessagingService {
    /// Subscribe to message arrival notifications
    /// 
    /// Returns a stream that yields when messages are available.
    /// The notification may or may not include message content
    /// depending on provider capabilities.
    fn subscribe(
        &self,
        queue_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;
}

/// What a push notification contains
pub enum MessageNotification {
    /// Just a signal that messages are available (pg_notify style)
    Available { queue_name: String },
    
    /// Full message content (RabbitMQ style)
    Message(QueuedMessage<Vec<u8>>),
}

/// Combined push + pull for hybrid operation
pub struct HybridConsumer<S: MessagingService + SupportsPushNotifications> {
    service: Arc<S>,
    push_stream: Pin<Box<dyn Stream<Item = MessageNotification> + Send>>,
    fallback_interval: Duration,
}

impl<S: MessagingService + SupportsPushNotifications> HybridConsumer<S> {
    /// Consume messages with push as primary, poll as fallback
    pub async fn next_batch(&mut self) -> Result<Vec<QueuedMessage<Vec<u8>>>, MessagingError> {
        tokio::select! {
            // Primary: wait for push notification
            Some(notification) = self.push_stream.next() => {
                match notification {
                    MessageNotification::Available { queue_name } => {
                        // pg_notify style: notification received, now fetch
                        self.service.receive_messages(&queue_name, 10, Duration::from_secs(30)).await
                    }
                    MessageNotification::Message(msg) => {
                        // RabbitMQ style: message already delivered
                        Ok(vec![msg])
                    }
                }
            }
            // Fallback: periodic poll for missed messages
            _ = tokio::time::sleep(self.fallback_interval) => {
                self.service.receive_messages("*", 10, Duration::from_secs(30)).await
            }
        }
    }
}
```

---

## Provider Capability Matrix

| Provider | Native Model | Push Support | Notification Type | Fallback Needed |
|----------|--------------|--------------|-------------------|-----------------|
| PGMQ | Poll | ✅ (pg_notify) | Signal only | Yes (catch-up) |
| RabbitMQ | **Push** | ✅ (native) | Full message | No |
| SQS | Poll | ❌ | N/A | N/A (poll only) |
| Redis Streams | Block | ⚠️ (pseudo) | Full message | Optional |
| Kafka | Push | ✅ (native) | Full message | No |

---

## Research Questions

### For lapin/RabbitMQ

1. **Consumer recovery**: How does lapin handle connection drops mid-stream?
2. **Prefetch tuning**: Optimal prefetch for Tasker's workload patterns?
3. **Multiple queues**: Can one connection consume from multiple queues efficiently?
4. **Benchmarks**: What latency can we expect vs pg_notify?

### For Trait Design

1. **Unified interface**: Can we abstract over "signal + fetch" vs "full message push"?
2. **Backpressure**: How to handle push backpressure uniformly?
3. **Graceful degradation**: Should push failure auto-switch to poll?

### For Implementation

1. **Feature gating**: Should push support be a separate feature flag?
2. **Testing**: How to test push behavior deterministically?
3. **Metrics**: What observability do we need for push vs poll paths?

---

## Recommended Next Steps

1. **Spike**: Implement basic lapin consumer, benchmark against pg_notify
2. **Design**: Finalize `SupportsPushNotifications` trait shape
3. **Prototype**: Build `HybridConsumer` abstraction
4. **Benchmark**: Compare throughput across providers with realistic workloads

---

## References

- [lapin documentation](https://docs.rs/lapin/latest/lapin/)
- [RabbitMQ Consumer documentation](https://www.rabbitmq.com/consumers.html)
- [PostgreSQL NOTIFY](https://www.postgresql.org/docs/current/sql-notify.html)
- Tasker's `pgmq-notify` crate (workspace)

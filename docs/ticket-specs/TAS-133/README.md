# TAS-133: Messaging Service Strategy Pattern Abstraction

**Status**: Planning
**Created**: 2026-01-07
**Supersedes**: TAS-35 (Message Service Abstraction Implementation Plan)
**Related**: TAS-43 (Event-Driven Task Claiming with pg_notify)

---

## Executive Summary

This ticket implements a strategy pattern abstraction for Tasker's messaging layer, enabling provider-agnostic queue operations while maintaining PGMQ as the default, batteries-included implementation. The abstraction enables RabbitMQ (via `lapin`) as a first-class alternative in tasker-core, with a clear trait boundary for community-contributed providers (SQS, Redis, Kafka) in tasker-contrib.

**Key Evolution from TAS-35**:
- Reflects Core/Contrib architectural split
- Multi-language worker support (not Ruby-centric)
- Integrates with existing `UnifiedMessageClient` 
- Explicit `lapin` recommendation over `rabbitmq-stream-rust-client`
- FFI boundary considerations for cross-language consistency
- Push-based notification support (leveraging Tasker's pg_notify architecture)

---

## Motivation

### Current State

Tasker is tightly coupled to PGMQ through the `UnifiedMessageClient` abstraction. While PGMQ provides excellent PostgreSQL-native capabilities with minimal dependencies, this coupling limits:

1. **Operational Flexibility**: Organizations with existing RabbitMQ/Kafka infrastructure cannot leverage those investments
2. **Throughput Options**: Dedicated message brokers offer different performance characteristics
3. **Scaling Patterns**: Different queue systems excel at different scaling scenarios
4. **Feature Sets**: Some providers offer unique features (e.g., RabbitMQ's flexible routing, Kafka's replay)

### Design Goals

1. **PostgreSQL Remains Source of Truth**: Task/step state lives in PostgreSQL; queues are transport
2. **Pull-Based Consistency**: All providers use pull model for uniform behavior
3. **Zero Handler Changes**: Switching providers requires only configuration changes
4. **Defense in Depth**: Maintain existing backpressure and circuit breaker patterns
5. **Core/Contrib Boundary**: Traits in core, alternative implementations pluggable from contrib

---

## Architecture

### Responsibility Boundaries

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           TASKER-CORE                                    │
├─────────────────────────────────────────────────────────────────────────┤
│  MessagingService trait          │  Provider-agnostic contract          │
│  QueueMessage trait              │  Message serialization contract      │
│  MessageRouter trait             │  Queue routing strategy              │
│  MessagingError enum             │  Unified error types                 │
│  PgmqMessagingService            │  Default implementation              │
│  RabbitMqMessagingService        │  First alternative (lapin-based)     │
│  MessagingServiceFactory         │  Config-driven instantiation         │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                          TASKER-CONTRIB                                  │
├─────────────────────────────────────────────────────────────────────────┤
│  SqsMessagingService             │  AWS SQS implementation              │
│  RedisMessagingService           │  Redis Streams implementation        │
│  KafkaMessagingService           │  Kafka implementation                │
│  [Community Contributions]       │  Additional providers                │
└─────────────────────────────────────────────────────────────────────────┘
```

### Why RabbitMQ in Core (Not Contrib)?

RabbitMQ represents a meaningfully different operational model that benefits from first-party support:
- Validates the trait abstraction is genuinely provider-agnostic
- Provides a "reference alternative" for contrib implementers
- Common enough that first-party support adds significant value
- `lapin` is mature and well-maintained

### Client Library Choice: `lapin`

**Decision**: Use `lapin` (AMQP 0.9.1) over `rabbitmq-stream-rust-client`

| Aspect | lapin | rabbitmq-stream-rust-client |
|--------|-------|------------------------------|
| Protocol | AMQP 0.9.1 | RabbitMQ Streams |
| Pattern | Work queues | Event streaming |
| Maturity | 6+ years, stable | Newer |
| Fit for Tasker | ✅ Perfect | Overkill |

Tasker's messaging model is work queues (dequeue, process, ack), not event streaming. `lapin` aligns with:
- `basic_consume` with manual ack (matches PGMQ visibility timeout semantics)
- Dead letter exchanges for failed messages
- Direct queue routing (no complex exchange topologies needed)

### Push-Based Notifications

Tasker achieves exceptional throughput via `pgmq-notify` + `pg_notify` push notifications:
- Complex 7-step DAG completion: **< 133ms** end-to-end
- Hundreds of concurrent tasks on resource-constrained hardware
- Sub-millisecond latency exposed previously invisible transaction races

**Key Insight**: RabbitMQ is **natively push-based** via `basic_consume`, unlike PGMQ which is poll-native with push enhancement. This is actually a strength - lapin consumers receive messages immediately as they arrive.

| Provider | Native Model | Push Support | Notes |
|----------|--------------|--------------|-------|
| PGMQ | Poll | ✅ (pg_notify) | Signal-based, requires fetch |
| RabbitMQ | **Push** | ✅ (native) | Full message delivery |
| SQS | Poll | ❌ | Long polling only |
| Redis Streams | Block | ⚠️ | Blocking read (pseudo-push) |

The trait design includes `SupportsPushNotifications` marker trait for providers with push capabilities, enabling Tasker's hybrid event-driven architecture (push primary, poll fallback).

See [Push Notifications Research](./push-notifications-research.md) for detailed analysis.

---

## Trait Design

### Core Traits

```rust
// tasker-core/src/services/messaging/traits.rs

use async_trait::async_trait;
use std::time::Duration;

/// Unique identifier for a queued message (provider-specific format)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

/// Handle for acknowledging/extending a received message
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReceiptHandle(pub String);

/// Queue depth and health statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub queue_name: String,
    pub message_count: u64,
    pub in_flight_count: Option<u64>,
    pub oldest_message_age: Option<Duration>,
}

/// A message received from a queue
#[derive(Debug, Clone)]
pub struct QueuedMessage<T> {
    pub receipt_handle: ReceiptHandle,
    pub message: T,
    pub receive_count: u32,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

/// Core messaging service trait - provider-agnostic
#[async_trait]
pub trait MessagingService: Send + Sync + 'static {
    /// Create a queue if it doesn't exist
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError>;

    /// Send a message to a queue
    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError>;

    /// Send a batch of messages (provider may optimize)
    async fn send_batch<T: QueueMessage>(
        &self,
        queue_name: &str,
        messages: &[T],
    ) -> Result<Vec<MessageId>, MessagingError>;

    /// Receive messages with visibility timeout
    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError>;

    /// Acknowledge successful processing (delete message)
    async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError>;

    /// Negative acknowledge (return to queue, optionally with delay)
    async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> Result<(), MessagingError>;

    /// Extend visibility timeout (heartbeat during long processing)
    async fn extend_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        extension: Duration,
    ) -> Result<(), MessagingError>;

    /// Get queue statistics for backpressure decisions
    async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats, MessagingError>;

    /// Health check
    async fn health_check(&self) -> Result<bool, MessagingError>;

    /// Provider name for logging/metrics
    fn provider_name(&self) -> &'static str;
}

/// Message serialization contract
pub trait QueueMessage: Send + Sync + Clone + 'static {
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError>
    where
        Self: Sized;
}

/// Namespace-based queue routing
pub trait MessageRouter: Send + Sync {
    fn route_to_queue(&self, namespace: &str, message_type: &str) -> String;
    fn extract_namespace(&self, queue_name: &str) -> Option<String>;
}
```

### Capability Marker Traits

```rust
use futures::Stream;
use std::pin::Pin;

/// What a push notification contains (varies by provider)
pub enum MessageNotification {
    /// Signal that messages are available (pg_notify style)
    /// Caller must fetch via receive_messages()
    Available { queue_name: String },
    
    /// Full message content (RabbitMQ style)
    /// Message is immediately usable
    Message(QueuedMessage<Vec<u8>>),
}

/// Provider supports push-based notifications
/// 
/// This is critical for Tasker's high-throughput architecture.
/// Providers implement this to enable event-driven consumption
/// with polling fallback for missed messages.
pub trait SupportsPushNotifications: MessagingService {
    /// Subscribe to message arrival notifications
    /// 
    /// Returns a stream that yields when messages are available.
    /// The notification type depends on provider capabilities:
    /// - PGMQ: `Available` (signal only, requires fetch)
    /// - RabbitMQ: `Message` (full content delivered)
    fn subscribe(
        &self,
        queue_name: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;
    
    /// Subscribe to multiple queues (for namespace patterns)
    fn subscribe_pattern(
        &self,
        queue_pattern: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;
}

/// Provider supports dead letter queues
pub trait SupportsDeadLetter: MessagingService {
    async fn configure_dead_letter(
        &self,
        source_queue: &str,
        dead_letter_queue: &str,
        max_receive_count: u32,
    ) -> Result<(), MessagingError>;
}

/// Provider supports message priority
pub trait SupportsPriority: MessagingService {
    async fn send_with_priority<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
        priority: u8,
    ) -> Result<MessageId, MessagingError>;
}

/// Provider supports delayed/scheduled delivery
pub trait SupportsDelayedDelivery: MessagingService {
    async fn send_delayed<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
        delay: Duration,
    ) -> Result<MessageId, MessagingError>;
}
```

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum MessagingError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Queue not found: {0}")]
    QueueNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Message not found: {0}")]
    MessageNotFound(String),

    #[error("Visibility timeout expired")]
    VisibilityExpired,

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Circuit breaker open")]
    CircuitBreakerOpen,

    #[error("Queue depth exceeded: {current}/{limit}")]
    QueueDepthExceeded { current: u64, limit: u64 },
}
```

---

## Integration with Existing Systems

### UnifiedMessageClient Migration

The existing `UnifiedMessageClient` becomes a thin wrapper that delegates to `MessagingService`:

```rust
pub struct UnifiedMessageClient {
    inner: Arc<dyn MessagingService>,
    router: Arc<dyn MessageRouter>,
}

impl UnifiedMessageClient {
    pub fn new(service: Arc<dyn MessagingService>, router: Arc<dyn MessageRouter>) -> Self {
        Self { inner: service, router }
    }

    // Existing methods delegate to inner service
    pub async fn send_step_message(&self, namespace: &str, message: &SimpleStepMessage) -> Result<()> {
        let queue = self.router.route_to_queue(namespace, "step");
        self.inner.send_message(&queue, message).await?;
        Ok(())
    }
}
```

### SystemContext Integration

```rust
pub struct SystemContext {
    pub db_pool: Arc<PgPool>,
    pub config: Arc<TaskerConfig>,
    pub messaging: Arc<dyn MessagingService>,  // Add messaging service
    // ... existing fields
}
```

### Actor Integration

Actors access messaging through `SystemContext`, maintaining the established pattern:

```rust
impl StepEnqueuerActor {
    async fn enqueue_step(&self, step: &WorkflowStep) -> TaskerResult<()> {
        let messaging = self.context().messaging.clone();
        let queue = format!("{}_queue", step.namespace);
        messaging.send_message(&queue, &step.to_message()).await?;
        Ok(())
    }
}
```

---

## Configuration

### TOML Configuration

Messaging configuration follows Tasker's standard pattern: settings live in `common.toml` (shared), `orchestration.toml` (orchestrator-specific), and `worker.toml` (worker-specific), NOT in a separate `messaging.toml` file.

```toml
# config/tasker/base/common.toml

[messaging]
# Provider selection: "pgmq" (default) | "rabbitmq"
provider = "pgmq"

# Common settings (provider-agnostic)
default_visibility_timeout_seconds = 30
default_batch_size = 10
max_batch_size = 100

# PGMQ-specific settings (uses database connection from [database] section)
[messaging.pgmq]
# Additional PGMQ tuning if needed
enable_pg_notify = true  # Enable push-based notifications

# RabbitMQ-specific settings
[messaging.rabbitmq]
url = "amqp://guest:guest@localhost:5672/"
vhost = "/"
prefetch_count = 10
heartbeat_seconds = 60
connection_timeout_seconds = 30

# Dead letter configuration (for providers that support it)
[messaging.dead_letter]
enabled = true
max_receive_count = 3
queue_suffix = "_dlq"
```

```toml
# config/tasker/base/orchestration.toml

# Orchestration-specific messaging settings
[messaging.orchestration]
# Event buffer sizes for pg_notify / push consumers
push_event_buffer_size = 50000
# Fallback polling interval when push is unavailable or for catch-up
fallback_poll_interval_ms = 1000
```

```toml
# config/tasker/base/worker.toml

# Worker-specific messaging settings  
[messaging.worker]
# Consumer prefetch for providers that support it
prefetch_count = 10
```

### Factory Pattern

```rust
pub struct MessagingServiceFactory;

impl MessagingServiceFactory {
    pub async fn create(config: &MessagingConfig) -> Result<Arc<dyn MessagingService>, MessagingError> {
        match config.provider.as_str() {
            "pgmq" => {
                let service = PgmqMessagingService::new(&config.pgmq).await?;
                Ok(Arc::new(service))
            }
            "rabbitmq" => {
                let service = RabbitMqMessagingService::new(&config.rabbitmq).await?;
                Ok(Arc::new(service))
            }
            other => Err(MessagingError::Provider(format!("Unknown provider: {}", other))),
        }
    }
}
```

---

## Implementation Phases

### Phase 1: Trait Definition & PGMQ Refactor (Week 1)

**Goal**: Extract traits from existing PGMQ implementation without breaking changes

- [ ] Define `MessagingService`, `QueueMessage`, `MessageRouter` traits
- [ ] Define `MessagingError` enum
- [ ] Refactor `UnifiedMessageClient` to use traits internally
- [ ] Implement `PgmqMessagingService` conforming to new traits
- [ ] Add `messaging` field to `SystemContext`
- [ ] Update configuration schema

**Deliverables**:
- `tasker-core/src/services/messaging/traits.rs`
- `tasker-core/src/services/messaging/pgmq.rs` (refactored)
- `tasker-core/src/services/messaging/factory.rs`
- Configuration updates

### Phase 2: RabbitMQ Implementation (Week 2)

**Goal**: Implement RabbitMQ provider using `lapin`

- [ ] Add `lapin` dependency (feature-gated)
- [ ] Implement `RabbitMqMessagingService`
- [ ] Implement receipt handle tracking for ack/nack
- [ ] Implement visibility timeout via consumer timeout
- [ ] Add dead letter exchange configuration
- [ ] Integration tests against local RabbitMQ

**Deliverables**:
- `tasker-core/src/services/messaging/rabbitmq.rs`
- Docker Compose addition for RabbitMQ testing
- Integration test suite

### Phase 3: Testing & Documentation (Week 3)

**Goal**: Comprehensive testing and documentation

- [ ] Provider conformance test suite (runs against all providers)
- [ ] Backpressure integration tests
- [ ] Circuit breaker integration verification
- [ ] Performance benchmarks (PGMQ vs RabbitMQ)
- [ ] Migration guide documentation
- [ ] Architecture documentation updates

**Deliverables**:
- `tests/services/messaging/conformance.rs`
- `docs/architecture/messaging-abstraction.md`
- `docs/guides/messaging-provider-migration.md`

### Phase 4: Contrib Preparation (Week 4)

**Goal**: Prepare for community-contributed providers

- [ ] Document trait requirements for contrib implementations
- [ ] Create `tasker-contrib/messaging/` directory structure
- [ ] Stub `SqsMessagingService` as reference for contributors
- [ ] CLI command for provider health check

**Deliverables**:
- Contrib documentation
- Reference stub implementation
- CLI enhancements

---

## Success Criteria

### Technical Criteria

| Criterion | Measurement |
|-----------|-------------|
| Zero handler changes required | Existing handlers work with both providers |
| Provider conformance | Both providers pass identical test suite |
| Configuration-only switching | Change `provider = "rabbitmq"` and restart |
| Backpressure preserved | Queue depth monitoring works for both |
| Performance acceptable | RabbitMQ within 2x of PGMQ for typical loads |

### Architectural Criteria

| Criterion | Measurement |
|-----------|-------------|
| Trait boundary is clean | Contrib implementations require only trait impl |
| No PGMQ-specific leakage | Core orchestration code uses only trait methods |
| FFI boundary preserved | Message types serialize identically |

---

## Out of Scope

1. **Kafka implementation** - Different paradigm (event streaming vs work queues), better as contrib
2. **Push-based delivery** - Tasker uses pull model consistently
3. **Multi-provider routing** - Single provider per deployment
4. **Message transformation** - Messages are passed through unchanged
5. **Ruby/Python/TypeScript service classes** - FFI to Rust traits only

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| RabbitMQ visibility timeout semantics differ | Use consumer timeout + nack for requeue |
| Performance regression | Benchmark early, optimize hot paths |
| Breaking change to UnifiedMessageClient API | Wrapper maintains backward compatibility |
| lapin async runtime conflicts | Verify tokio compatibility early |

---

## References

- [TAS-35](https://linear.app/tasker-systems/issue/TAS-35) - Original ticket (superseded)
- [TAS-43](https://linear.app/tasker-systems/issue/TAS-43) - Event-Driven Task Claiming
- [Backpressure Architecture](../../architecture/backpressure-architecture.md)
- [Events and Commands](../../architecture/events-and-commands.md)
- [lapin documentation](https://docs.rs/lapin/latest/lapin/)
- [PGMQ documentation](https://github.com/tembo-io/pgmq)

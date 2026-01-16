# Messaging Abstraction Architecture

**Last Updated**: 2026-01-15
**Audience**: Architects, Developers
**Status**: Active (TAS-133 Complete)
**Related Docs**: [Documentation Hub](README.md) | [Events and Commands](events-and-commands.md) | [Deployment Patterns](deployment-patterns.md) | [Crate Architecture](crate-architecture.md)

<- Back to [Documentation Hub](README.md)

---

## Overview

TAS-133 introduced a **provider-agnostic messaging abstraction** that enables Tasker Core to support multiple messaging backends through a unified interface. This architecture allows switching between PGMQ (PostgreSQL Message Queue) and RabbitMQ without changes to business logic.

**Key Benefits**:
- **Zero handler changes required**: Switching providers requires only configuration changes
- **Provider-specific optimizations**: Each backend can leverage its native strengths
- **Testability**: In-memory provider for fast unit testing
- **Gradual migration**: Systems can transition between providers incrementally

---

## Core Concepts

### Message Delivery Models

Different messaging providers have fundamentally different delivery models:

| Provider | Native Model | Push Support | Notification Type | Fallback Needed |
|----------|--------------|--------------|-------------------|-----------------|
| **PGMQ** | Poll | Yes (pg_notify) | **Signal only** | Yes (catch-up) |
| **RabbitMQ** | **Push** | Yes (native) | **Full message** | **No** |
| **InMemory** | Push | Yes | Full message | No |

**PGMQ (Signal-Only)**:
- `pg_notify` sends a signal that a message exists
- Worker must fetch the message after receiving the signal
- Fallback polling catches missed signals

**RabbitMQ (Full Message Push)**:
- `basic_consume()` delivers complete messages
- No separate fetch required
- Protocol guarantees delivery

---

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Application Layer                                  │
│  (Orchestration, Workers, Event Systems)                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ Uses MessageClient
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           MessageClient                                      │
│  Domain-level facade with queue classification                              │
│  Location: tasker-shared/src/messaging/client.rs                           │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ Delegates to
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         MessagingProvider Enum                               │
│  Runtime dispatch without trait objects (zero-cost abstraction)             │
│  Location: tasker-shared/src/messaging/service/provider.rs                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    │               │               │
                    ▼               ▼               ▼
            ┌───────────┐   ┌───────────┐   ┌───────────┐
            │   PGMQ    │   │ RabbitMQ  │   │ InMemory  │
            │ Provider  │   │ Provider  │   │ Provider  │
            └───────────┘   └───────────┘   └───────────┘
```

---

## Core Traits and Types

### MessagingService Trait

**Location**: `tasker-shared/src/messaging/service/traits.rs`

The foundational trait defining queue operations:

```rust
#[async_trait]
pub trait MessagingService: Send + Sync {
    // Queue lifecycle
    async fn create_queue(&self, name: &str) -> Result<(), MessagingError>;
    async fn delete_queue(&self, name: &str) -> Result<(), MessagingError>;
    async fn queue_exists(&self, name: &str) -> Result<bool, MessagingError>;
    async fn list_queues(&self) -> Result<Vec<String>, MessagingError>;

    // Message operations
    async fn send_message(&self, queue: &str, payload: &[u8]) -> Result<i64, MessagingError>;
    async fn send_message_with_delay(&self, queue: &str, payload: &[u8], delay_seconds: i64) -> Result<i64, MessagingError>;
    async fn receive_messages(&self, queue: &str, limit: i32, visibility_timeout: i32) -> Result<Vec<QueuedMessage<Vec<u8>>>, MessagingError>;
    async fn ack_message(&self, queue: &str, msg_id: i64) -> Result<(), MessagingError>;
    async fn nack_message(&self, queue: &str, msg_id: i64) -> Result<(), MessagingError>;

    // Provider information
    fn provider_name(&self) -> &'static str;
}
```

### SupportsPushNotifications Trait

**Location**: `tasker-shared/src/messaging/service/traits.rs`

Extends `MessagingService` with push notification capabilities:

```rust
#[async_trait]
pub trait SupportsPushNotifications: MessagingService {
    /// Subscribe to messages on a single queue
    fn subscribe(&self, queue_name: &str)
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;

    /// Subscribe to messages on multiple queues
    fn subscribe_many(&self, queue_names: &[String])
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;

    /// Whether this provider requires fallback polling for reliability
    fn requires_fallback_polling(&self) -> bool;

    /// Suggested polling interval if fallback is needed
    fn fallback_polling_interval(&self) -> Option<Duration>;

    /// Whether this provider supports fetching by message ID
    fn supports_fetch_by_message_id(&self) -> bool;
}
```

### MessageNotification Enum

**Location**: `tasker-shared/src/messaging/service/traits.rs`

Abstracts the two notification models:

```rust
pub enum MessageNotification {
    /// Signal-only notification (PGMQ style)
    /// Indicates a message is available but requires separate fetch
    Available {
        queue_name: String,
        msg_id: Option<i64>,
    },

    /// Full message notification (RabbitMQ style)
    /// Contains the complete message payload
    Message(QueuedMessage<Vec<u8>>),
}
```

---

## Provider Implementations

### PGMQ Provider

**Location**: `tasker-shared/src/messaging/service/providers/pgmq.rs`

PostgreSQL-based message queue with LISTEN/NOTIFY integration:

```rust
impl SupportsPushNotifications for PgmqMessagingService {
    fn subscribe_many(&self, queue_names: &[String])
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>
    {
        // Uses PgmqNotifyListener for pg_notify subscription
        // Returns MessageNotification::Available (signal-only) for large messages
        // Returns MessageNotification::Message for small messages (<7KB)
    }

    fn requires_fallback_polling(&self) -> bool {
        true  // pg_notify can miss signals during connection issues
    }

    fn supports_fetch_by_message_id(&self) -> bool {
        true  // PGMQ supports read_specific_message()
    }
}
```

**Characteristics**:
- Uses PostgreSQL for storage and delivery
- `pg_notify` for real-time notifications
- Fallback polling required for reliability
- Supports visibility timeout for message claiming

### RabbitMQ Provider

**Location**: `tasker-shared/src/messaging/service/providers/rabbitmq.rs`

AMQP-based message broker with native push delivery:

```rust
impl SupportsPushNotifications for RabbitMqMessagingService {
    fn subscribe_many(&self, queue_names: &[String])
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>
    {
        // Uses lapin basic_consume() for native push delivery
        // Always returns MessageNotification::Message (full payload)
    }

    fn requires_fallback_polling(&self) -> bool {
        false  // AMQP protocol guarantees delivery
    }

    fn supports_fetch_by_message_id(&self) -> bool {
        false  // RabbitMQ doesn't support fetch-by-ID
    }
}
```

**Characteristics**:
- Native push delivery via AMQP protocol
- No fallback polling needed
- Higher throughput for high-volume scenarios
- Requires separate infrastructure (RabbitMQ server)

### InMemory Provider

**Location**: `tasker-shared/src/messaging/service/providers/in_memory.rs`

In-process message queue for testing:

```rust
impl SupportsPushNotifications for InMemoryMessagingService {
    fn requires_fallback_polling(&self) -> bool {
        false  // In-memory is reliable within process
    }
}
```

**Use Cases**:
- Unit testing without external dependencies
- Integration testing with controlled timing
- Development environments

---

## MessagingProvider Enum

**Location**: `tasker-shared/src/messaging/service/provider.rs`

Enum dispatch pattern for runtime provider selection without trait objects:

```rust
pub enum MessagingProvider {
    Pgmq(PgmqMessagingService),
    RabbitMq(RabbitMqMessagingService),
    InMemory(InMemoryMessagingService),
}

impl MessagingProvider {
    /// Delegate all MessagingService methods to the underlying provider
    pub async fn send_message(&self, queue: &str, payload: &[u8]) -> Result<i64, MessagingError> {
        match self {
            Self::Pgmq(p) => p.send_message(queue, payload).await,
            Self::RabbitMq(p) => p.send_message(queue, payload).await,
            Self::InMemory(p) => p.send_message(queue, payload).await,
        }
    }

    /// Subscribe to push notifications
    pub fn subscribe_many(&self, queue_names: &[String])
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>
    {
        match self {
            Self::Pgmq(p) => p.subscribe_many(queue_names),
            Self::RabbitMq(p) => p.subscribe_many(queue_names),
            Self::InMemory(p) => p.subscribe_many(queue_names),
        }
    }

    /// Check if fallback polling is required
    pub fn requires_fallback_polling(&self) -> bool {
        match self {
            Self::Pgmq(p) => p.requires_fallback_polling(),
            Self::RabbitMq(p) => p.requires_fallback_polling(),
            Self::InMemory(p) => p.requires_fallback_polling(),
        }
    }
}
```

**Benefits**:
- Zero-cost abstraction (no vtable indirection)
- Exhaustive match ensures all providers handled
- Easy to add new providers

---

## MessageClient Facade

**Location**: `tasker-shared/src/messaging/client.rs`

Domain-level facade providing high-level queue operations:

```rust
pub struct MessageClient {
    provider: Arc<MessagingProvider>,
    classifier: QueueClassifier,
}

impl MessageClient {
    /// Send a step message to the appropriate namespace queue
    pub async fn send_step_message(
        &self,
        namespace: &str,
        step: &SimpleStepMessage,
    ) -> Result<i64, MessagingError> {
        let queue_name = self.classifier.step_queue_for_namespace(namespace);
        let payload = serde_json::to_vec(step)?;
        self.provider.send_message(&queue_name, &payload).await
    }

    /// Send a step result to the orchestration queue
    pub async fn send_step_result(
        &self,
        result: &StepExecutionResult,
    ) -> Result<i64, MessagingError> {
        let queue_name = self.classifier.orchestration_results_queue();
        let payload = serde_json::to_vec(result)?;
        self.provider.send_message(&queue_name, &payload).await
    }

    /// Access the underlying provider for advanced operations
    pub fn provider(&self) -> &MessagingProvider {
        &self.provider
    }
}
```

---

## Event System Integration

### Provider-Agnostic Queue Listeners

Both orchestration and worker queue listeners use `provider.subscribe_many()`:

```rust
// tasker-orchestration/src/orchestration/orchestration_queues/listener.rs
impl OrchestrationQueueListener {
    pub async fn start(&mut self) -> Result<(), MessagingError> {
        let queues = vec![
            self.classifier.orchestration_results_queue(),
            self.classifier.orchestration_requests_queue(),
            self.classifier.orchestration_finalization_queue(),
        ];

        // Provider-agnostic subscription
        let stream = self.provider.subscribe_many(&queues)?;

        // Process notifications
        while let Some(notification) = stream.next().await {
            match notification {
                MessageNotification::Available { queue_name, msg_id } => {
                    // PGMQ style: send event command to fetch message
                    self.send_event_command(queue_name, msg_id).await;
                }
                MessageNotification::Message(msg) => {
                    // RabbitMQ style: send message command with full payload
                    self.send_message_command(msg).await;
                }
            }
        }
    }
}
```

### Deployment Mode Selection

Event systems select the appropriate mode based on provider capabilities:

```rust
// Determine effective deployment mode for this provider
let effective_mode = deployment_mode.effective_for_provider(provider.provider_name());

match effective_mode {
    DeploymentMode::EventDrivenOnly => {
        // Start queue listener only (no fallback poller)
        // RabbitMQ typically uses this mode
    }
    DeploymentMode::Hybrid => {
        // Start both listener and fallback poller
        // PGMQ uses this mode for reliability
    }
    DeploymentMode::PollingOnly => {
        // Start fallback poller only
        // For restricted environments
    }
}
```

---

## Command Routing

### Dual Command Variants

Command processors handle both notification types:

```rust
pub enum OrchestrationCommand {
    // For full message notifications (RabbitMQ)
    ProcessStepResultFromMessage {
        queue_name: String,
        message: QueuedMessage<Vec<u8>>,
        resp: CommandResponder<StepProcessResult>,
    },

    // For signal-only notifications (PGMQ)
    ProcessStepResultFromMessageEvent {
        message_event: MessageReadyEvent,
        resp: CommandResponder<StepProcessResult>,
    },
}
```

**Routing Logic**:
- `MessageNotification::Message` -> `ProcessStepResultFromMessage`
- `MessageNotification::Available` -> `ProcessStepResultFromMessageEvent`

---

## Type-Safe Channel Wrappers

TAS-133 introduced NewType wrappers for MPSC channels to prevent accidental misuse:

### Orchestration Channels

**Location**: `tasker-orchestration/src/orchestration/channels.rs`

```rust
/// Strongly-typed sender for orchestration commands
#[derive(Debug, Clone)]
pub struct OrchestrationCommandSender(pub(crate) mpsc::Sender<OrchestrationCommand>);

/// Strongly-typed receiver for orchestration commands
#[derive(Debug)]
pub struct OrchestrationCommandReceiver(pub(crate) mpsc::Receiver<OrchestrationCommand>);

/// Strongly-typed sender for orchestration notifications
#[derive(Debug, Clone)]
pub struct OrchestrationNotificationSender(pub(crate) mpsc::Sender<OrchestrationNotification>);

/// Strongly-typed receiver for orchestration notifications
#[derive(Debug)]
pub struct OrchestrationNotificationReceiver(pub(crate) mpsc::Receiver<OrchestrationNotification>);
```

### Worker Channels

**Location**: `tasker-worker/src/worker/channels.rs`

```rust
/// Strongly-typed sender for worker commands
#[derive(Debug, Clone)]
pub struct WorkerCommandSender(pub(crate) mpsc::Sender<WorkerCommand>);

/// Strongly-typed receiver for worker commands
#[derive(Debug)]
pub struct WorkerCommandReceiver(pub(crate) mpsc::Receiver<WorkerCommand>);
```

### Channel Factory

```rust
pub struct ChannelFactory;

impl ChannelFactory {
    /// Create type-safe orchestration command channel pair
    pub fn orchestration_command_channel(buffer_size: usize)
        -> (OrchestrationCommandSender, OrchestrationCommandReceiver)
    {
        let (tx, rx) = mpsc::channel(buffer_size);
        (OrchestrationCommandSender(tx), OrchestrationCommandReceiver(rx))
    }
}
```

**Benefits**:
- Compile-time prevention of channel misuse
- Self-documenting function signatures
- Zero runtime overhead (NewTypes compile away)

---

## Configuration

### Provider Selection

```toml
# config/dotenv/test.env
# Valid values: pgmq (default), rabbitmq
TASKER_MESSAGING_BACKEND=pgmq

# RabbitMQ connection (only used when backend=rabbitmq)
RABBITMQ_URL=amqp://tasker:tasker@localhost:5672/%2F
```

### Provider-Specific Settings

```toml
# config/tasker/base/common.toml
[pgmq]
visibility_timeout_seconds = 60
max_message_size_bytes = 1048576
batch_size = 100

[rabbitmq]
prefetch_count = 100
connection_timeout_seconds = 30
heartbeat_seconds = 60
```

---

## Migration Guide

### Switching from PGMQ to RabbitMQ

1. **Deploy RabbitMQ infrastructure**
2. **Update configuration**:
   ```bash
   export TASKER_MESSAGING_BACKEND=rabbitmq
   export RABBITMQ_URL=amqp://user:pass@rabbitmq:5672/%2F
   ```
3. **Restart services** - No code changes required

### Gradual Migration

For zero-downtime migration:

1. Deploy new services with RabbitMQ configuration
2. Gradually shift traffic to new services
3. Monitor for any issues
4. Decommission PGMQ-based services

---

## Testing

### Provider-Agnostic Tests

Most tests should use `InMemoryMessagingService` for speed:

```rust
#[tokio::test]
async fn test_step_execution() {
    let provider = MessagingProvider::InMemory(InMemoryMessagingService::new());
    let client = MessageClient::new(Arc::new(provider));

    // Test with in-memory provider
    client.send_step_message("payments", &step_msg).await.unwrap();
}
```

### Provider-Specific Tests

For integration tests that need specific provider behavior:

```rust
#[tokio::test]
#[cfg(feature = "integration-tests")]
async fn test_pgmq_notifications() {
    let provider = MessagingProvider::Pgmq(PgmqMessagingService::new(pool).await?);
    // Test PGMQ-specific behavior
}
```

---

## Best Practices

### 1. Use MessageClient for Application Code

```rust
// Good: Use domain-level facade
let client = context.message_client();
client.send_step_result(&result).await?;

// Avoid: Direct provider access unless necessary
let provider = context.messaging_provider();
provider.send_message("queue", &payload).await?;
```

### 2. Handle Both Notification Types

```rust
match notification {
    MessageNotification::Available { queue_name, msg_id } => {
        // Signal-only: need to fetch message
    }
    MessageNotification::Message(msg) => {
        // Full message: can process immediately
    }
}
```

### 3. Respect Provider Capabilities

```rust
if provider.supports_fetch_by_message_id() {
    // Can use read_specific_message()
} else {
    // Must use alternative approach
}
```

### 4. Configure Fallback Appropriately

```rust
if provider.requires_fallback_polling() {
    // Start fallback poller for reliability
}
```

---

## Related Documentation

- [Events and Commands](events-and-commands.md) - Command pattern details
- [Deployment Patterns](deployment-patterns.md) - Deployment modes and configuration
- [Worker Event Systems](worker-event-systems.md) - Worker event architecture
- [Crate Architecture](crate-architecture.md) - Workspace structure

---

<- Back to [Documentation Hub](README.md)

**Next**: [Events and Commands](events-and-commands.md) | [Deployment Patterns](deployment-patterns.md)

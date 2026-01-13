# TAS-133 Implementation Plan: Consolidated Approach

**Last Updated**: 2026-01-13
**Status**: Ready for Review
**Approach**: Option C - Consolidate to Two Components

---

## Executive Summary

Based on research of the existing codebase, we're implementing **Option C: Consolidate** - a simplified architecture that eliminates abstraction proliferation while enabling multi-provider support.

### Current State (3+ abstractions)
```
MessageClient (trait)         ← Domain-specific methods
PgmqClientTrait (trait)       ← Low-level PGMQ operations
UnifiedMessageClient (enum)   ← Dispatches to variants
UnifiedPgmqClient (struct)    ← Thin wrapper
```

### Target State (2 components)
```
MessagingService (trait)      ← Provider contract (generic)
MessageClient (struct)        ← Domain facade (NOT a trait)
```

---

## Architecture Overview

### Core Components

```
┌─────────────────────────────────────────────────────────────────────┐
│                        tasker-shared                                │
├─────────────────────────────────────────────────────────────────────┤
│  messaging/service/                                                  │
│  ├── traits.rs          MessagingService, QueueMessage traits       │
│  ├── types.rs           MessageId, ReceiptHandle, QueuedMessage     │
│  ├── errors.rs          MessagingError enum                         │
│  ├── provider.rs        MessagingProvider enum (dispatch)           │
│  ├── router.rs          MessageRouter trait + MessageRouterKind enum│
│  ├── pgmq.rs            PgmqMessagingService impl                   │
│  ├── rabbitmq.rs        RabbitMqMessagingService impl               │
│  ├── in_memory.rs       InMemoryMessagingService (testing)          │
│  └── factory.rs         MessagingServiceFactory                     │
│                                                                      │
│  messaging/client.rs    MessageClient struct (domain helpers)        │
└─────────────────────────────────────────────────────────────────────┘
```

### Dependency Flow

```
SystemContext
    ↓ owns
Arc<MessagingProvider>     ← Enum dispatch (NOT trait object)
    ↑ variants
MessagingProvider::Pgmq(PgmqMessagingService)
MessagingProvider::RabbitMq(RabbitMqMessagingService)
MessagingProvider::InMemory(InMemoryMessagingService)

MessageClient (struct)     ← Domain facade
    ↓ uses
Arc<MessagingProvider> + MessageRouterKind (enum)
    ↑ variants
MessageRouterKind::Default(DefaultMessageRouter)
```

### Why Enum Dispatch (Not `Arc<dyn>`)

We chose **enum dispatch** over trait objects for hot-path performance:

1. **No vtable indirection** - enum match is a single branch vs pointer chase
2. **Inlining possible** - compiler can inline provider methods
3. **No generic proliferation** - `SystemContext` stays non-generic (no `<M>` marker everywhere)
4. **Zero-cost abstraction** - same pattern as current `UnifiedMessageClient`

```rust
/// Provider enum for zero-cost dispatch
pub enum MessagingProvider {
    Pgmq(PgmqMessagingService),
    RabbitMq(RabbitMqMessagingService),
    InMemory(InMemoryMessagingService),
}

impl MessagingProvider {
    pub async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError> {
        match self {
            Self::Pgmq(s) => s.send_message(queue_name, message).await,
            Self::RabbitMq(s) => s.send_message(queue_name, message).await,
            Self::InMemory(s) => s.send_message(queue_name, message).await,
        }
    }
    // ... delegate all MessagingService methods
}
```

---

---

## Milestones & Validation Gates

This implementation is structured as a series of milestones, each with clear validation gates. The TAS-133 branch serves as the foundation feature branch, with child ticket branches for each milestone.

### Branching Strategy

```
main
  └── jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction (feature branch)
        ├── jcoletaylor/tas-133a-trait-definitions (Phase 1)
        ├── jcoletaylor/tas-133b-pgmq-implementation (Phase 2)
        ├── jcoletaylor/tas-133c-message-client-struct (Phase 3)
        ├── jcoletaylor/tas-133d-rabbitmq-implementation (Phase 4)
        ├── jcoletaylor/tas-133e-system-context-integration (Phase 5)
        └── jcoletaylor/tas-133f-migration-cleanup (Phase 6)
```

Each child branch merges back to TAS-133 after passing its validation gate. Only when all milestones are complete does TAS-133 merge to main.

### Milestone Summary

| Milestone | Child Ticket | Scope | Validation Gate |
|-----------|--------------|-------|-----------------|
| **M1** | TAS-133a | Trait definitions, types, errors, router | Compiles, existing tests pass, new types importable |
| **M2** | TAS-133b | PgmqMessagingService + pgmq-notify updates | PGMQ conformance tests pass, existing orchestration works |
| **M3** | TAS-133c | MessageClient struct, StepMessage rename | Domain operations work through new client |
| **M4** | TAS-133d | RabbitMqMessagingService | RabbitMQ conformance tests pass (docker-compose) |
| **M5** | TAS-133e | SystemContext integration, config TOML | Full integration tests pass with both providers |
| **M6** | TAS-133f | Delete old code, final cleanup | No old abstractions remain, all tests green |

---

## Phase 1: Trait Definition & Types

**Goal**: Define the new abstraction layer without breaking existing code
**Child Ticket**: TAS-133a
**Validation Gate**: Compiles with `--all-features`, existing test suite passes, new types can be imported

### 1.1 Create `messaging/service/` Module Structure

```
tasker-shared/src/messaging/service/
├── mod.rs
├── traits.rs
├── types.rs
├── errors.rs
└── router.rs
```

### 1.2 Define Core Traits (`traits.rs`)

```rust
use async_trait::async_trait;
use std::time::Duration;

/// Core messaging service trait - provider-agnostic
#[async_trait]
pub trait MessagingService: Send + Sync + 'static {
    /// Create a queue if it doesn't exist (idempotent)
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError>;

    /// Bulk queue creation (called during worker bootstrap)
    async fn ensure_queues(&self, queue_names: &[String]) -> Result<(), MessagingError> {
        for queue_name in queue_names {
            self.ensure_queue(queue_name).await?;
        }
        Ok(())
    }

    /// Verify expected queues exist (startup health check)
    async fn verify_queues(&self, queue_names: &[String]) -> Result<QueueHealthReport, MessagingError>;

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
    ) -> Result<Vec<MessageId>, MessagingError> {
        let mut ids = Vec::with_capacity(messages.len());
        for message in messages {
            ids.push(self.send_message(queue_name, message).await?);
        }
        Ok(ids)
    }

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

    /// Negative acknowledge (return to queue or DLQ)
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
    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError> where Self: Sized;
}

/// Namespace-based queue routing
pub trait MessageRouter: Send + Sync {
    fn step_queue(&self, namespace: &str) -> String;
    fn result_queue(&self) -> String;
    fn task_request_queue(&self) -> String;
    fn task_finalization_queue(&self) -> String;
    fn domain_event_queue(&self, namespace: &str) -> String;
    fn extract_namespace(&self, queue_name: &str) -> Option<String>;
}

/// Default router implementation using config-based queue names
pub struct DefaultMessageRouter {
    worker_queue_prefix: String,
    result_queue: String,
    task_request_queue: String,
    task_finalization_queue: String,
}

impl DefaultMessageRouter {
    pub fn from_config(queues_config: &QueuesConfig) -> Self {
        Self {
            worker_queue_prefix: queues_config.worker_queue_prefix.clone(),
            result_queue: queues_config.orchestration_queues.step_results.clone(),
            task_request_queue: queues_config.orchestration_queues.task_requests.clone(),
            task_finalization_queue: queues_config.orchestration_queues.task_finalizations.clone(),
        }
    }
}

impl MessageRouter for DefaultMessageRouter {
    fn step_queue(&self, namespace: &str) -> String {
        format!("{}_{}_queue", self.worker_queue_prefix, namespace)
    }

    fn result_queue(&self) -> String {
        self.result_queue.clone()
    }

    fn task_request_queue(&self) -> String {
        self.task_request_queue.clone()
    }

    fn task_finalization_queue(&self) -> String {
        self.task_finalization_queue.clone()
    }

    fn domain_event_queue(&self, namespace: &str) -> String {
        format!("{}_domain_events", namespace)
    }

    fn extract_namespace(&self, queue_name: &str) -> Option<String> {
        queue_name
            .strip_prefix(&format!("{}_", self.worker_queue_prefix))
            .and_then(|s| s.strip_suffix("_queue"))
            .map(String::from)
    }
}

/// Enum dispatch for MessageRouter (avoids dyn trait objects)
///
/// Like MessagingProvider, we use enum dispatch for consistency and to avoid
/// vtable overhead. While router operations are cheap (string formatting),
/// using enums keeps the pattern uniform across the messaging layer.
pub enum MessageRouterKind {
    Default(DefaultMessageRouter),
    // Future variants can be added as needed
}

impl MessageRouterKind {
    pub fn step_queue(&self, namespace: &str) -> String {
        match self {
            Self::Default(r) => r.step_queue(namespace),
        }
    }

    pub fn result_queue(&self) -> String {
        match self {
            Self::Default(r) => r.result_queue(),
        }
    }

    pub fn task_request_queue(&self) -> String {
        match self {
            Self::Default(r) => r.task_request_queue(),
        }
    }

    pub fn task_finalization_queue(&self) -> String {
        match self {
            Self::Default(r) => r.task_finalization_queue(),
        }
    }

    pub fn domain_event_queue(&self, namespace: &str) -> String {
        match self {
            Self::Default(r) => r.domain_event_queue(namespace),
        }
    }

    pub fn extract_namespace(&self, queue_name: &str) -> Option<String> {
        match self {
            Self::Default(r) => r.extract_namespace(queue_name),
        }
    }
}
```

### 1.3 Define Types (`types.rs`)

```rust
/// Unique identifier for a queued message (provider-specific format)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

/// Handle for acknowledging/extending a received message
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReceiptHandle(pub String);

/// A message received from a queue
#[derive(Debug, Clone)]
pub struct QueuedMessage<T> {
    pub receipt_handle: ReceiptHandle,
    pub message: T,
    pub receive_count: u32,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

/// Queue depth and health statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub queue_name: String,
    pub message_count: u64,
    pub in_flight_count: Option<u64>,
    pub oldest_message_age: Option<Duration>,
}

/// Result of verify_queues health check
#[derive(Debug, Clone)]
pub struct QueueHealthReport {
    pub healthy: Vec<String>,
    pub missing: Vec<String>,
    pub errors: Vec<(String, String)>,
}
```

### 1.4 Define Errors (`errors.rs`)

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

    #[error("Operation not supported by provider: {0}")]
    NotSupported(String),
}
```

---

## Phase 2: PGMQ Implementation

**Goal**: Implement `PgmqMessagingService` that wraps existing pgmq-notify functionality
**Child Ticket**: TAS-133b
**Validation Gate**:
- `PgmqMessagingService` passes conformance test suite
- Existing orchestration/worker tests pass unchanged
- `set_visibility_timeout` added to `pgmq-notify/src/client.rs`

### 2.0 Prerequisite: Add `set_visibility_timeout` to PgmqClient

Before implementing `PgmqMessagingService`, add a wrapper method to `pgmq-notify/src/client.rs`:

```rust
/// Extend visibility timeout for a message (heartbeat during long processing)
///
/// This resets the visibility timeout for a message, preventing it from
/// becoming visible to other consumers while still being processed.
#[instrument(skip(self), fields(queue = %queue_name, message_id = %message_id))]
pub async fn set_visibility_timeout(
    &self,
    queue_name: &str,
    message_id: i64,
    vt_offset_seconds: i32,
) -> Result<()> {
    debug!(
        "⏰ Extending visibility timeout for message {} in queue: {} by {}s",
        message_id, queue_name, vt_offset_seconds
    );

    // Delegate to underlying pgmq crate's set_vt method
    self.pgmq.set_vt(queue_name, message_id, vt_offset_seconds).await?;

    debug!("Visibility timeout extended for message: {}", message_id);
    Ok(())
}
```

This keeps our boundary of responsibility at the `PgmqClient` level rather than leaking raw SQL into the messaging service layer.

### 2.1 Create `pgmq.rs`

```rust
pub struct PgmqMessagingService {
    client: PgmqClient,  // From pgmq-notify
}

impl PgmqMessagingService {
    pub async fn new(pool: PgPool) -> Result<Self, MessagingError> {
        let client = PgmqClient::new_with_pool(pool).await;
        Ok(Self { client })
    }

    pub fn client(&self) -> &PgmqClient {
        &self.client
    }
}

#[async_trait]
impl MessagingService for PgmqMessagingService {
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError> {
        self.client.create_queue(queue_name).await
            .map_err(|e| MessagingError::Provider(e.to_string()))
    }

    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError> {
        let bytes = message.to_bytes()?;
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| MessagingError::Serialization(e.to_string()))?;

        let msg_id = self.client.send_json_message(queue_name, &json).await
            .map_err(|e| MessagingError::Provider(e.to_string()))?;

        Ok(MessageId(msg_id.to_string()))
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, MessagingError> {
        let vt_secs = visibility_timeout.as_secs() as i32;
        let messages = self.client.read_messages(queue_name, Some(vt_secs), Some(max_messages as i32))
            .await
            .map_err(|e| MessagingError::Provider(e.to_string()))?;

        messages.into_iter()
            .map(|msg| {
                let bytes = serde_json::to_vec(&msg.message)
                    .map_err(|e| MessagingError::Serialization(e.to_string()))?;
                let message = T::from_bytes(&bytes)?;

                Ok(QueuedMessage {
                    receipt_handle: ReceiptHandle(msg.msg_id.to_string()),
                    message,
                    receive_count: msg.read_ct as u32,
                    enqueued_at: msg.enqueued_at,
                })
            })
            .collect()
    }

    async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> Result<(), MessagingError> {
        let msg_id: i64 = receipt_handle.0.parse()
            .map_err(|_| MessagingError::MessageNotFound(receipt_handle.0.clone()))?;

        self.client.delete_message(queue_name, msg_id).await
            .map_err(|e| MessagingError::Provider(e.to_string()))
    }

    async fn nack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        requeue: bool,
    ) -> Result<(), MessagingError> {
        let msg_id: i64 = receipt_handle.0.parse()
            .map_err(|_| MessagingError::MessageNotFound(receipt_handle.0.clone()))?;

        if requeue {
            // No-op: PGMQ's visibility timeout handles requeue automatically.
            // When VT expires, message reappears in the queue for retry.
            Ok(())
        } else {
            // Archive to a_{queue_name} table (PGMQ's built-in DLQ/archive).
            // The archive table is created automatically when the queue is created.
            // This removes the message from q_{queue_name} and preserves it in a_{queue_name}.
            self.client.archive_message(queue_name, msg_id).await
                .map_err(|e| MessagingError::Provider(e.to_string()))
        }
    }

    async fn extend_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
        extension: Duration,
    ) -> Result<(), MessagingError> {
        let msg_id: i64 = receipt_handle.0.parse()
            .map_err(|_| MessagingError::MessageNotFound(receipt_handle.0.clone()))?;

        // Use PgmqClient wrapper method (delegates to underlying pgmq crate's set_vt)
        let vt_offset_seconds = extension.as_secs() as i32;
        self.client.set_visibility_timeout(queue_name, msg_id, vt_offset_seconds).await
            .map_err(|e| MessagingError::Provider(e.to_string()))
    }

    // ... remaining methods
}
```

---

## Phase 3: MessageClient Struct (Domain Facade)

**Goal**: Create the domain-level helper struct that uses MessagingService
**Child Ticket**: TAS-133c
**Validation Gate**:
- `MessageClient` struct compiles and provides all domain operations
- `SimpleStepMessage` renamed to `StepMessage`, old `StepMessage` deleted
- Orchestration step enqueuer works through new `MessageClient`

### 3.1 Create `messaging/client.rs`

```rust
/// Domain-level messaging client
///
/// This is a struct (NOT a trait) that provides convenient domain-specific
/// methods for Tasker's messaging needs. It wraps a MessagingProvider (enum)
/// and a MessageRouterKind (enum) - no trait objects, all enum dispatch.
pub struct MessageClient {
    provider: Arc<MessagingProvider>,
    router: MessageRouterKind,
}

impl MessageClient {
    pub fn new(provider: Arc<MessagingProvider>, router: MessageRouterKind) -> Self {
        Self { provider, router }
    }

    /// Get the underlying messaging provider for advanced operations
    pub fn provider(&self) -> &Arc<MessagingProvider> {
        &self.provider
    }

    /// Get the router for queue name lookups
    pub fn router(&self) -> &MessageRouterKind {
        &self.router
    }

    /// Send a step message to the appropriate namespace queue
    ///
    /// NOTE: As of TAS-133, `StepMessage` is the UUID-based message (formerly `SimpleStepMessage`).
    /// The old `StepMessage` with embedded execution context has been removed - workers query
    /// the database using UUIDs instead of relying on embedded context.
    pub async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()> {
        let queue = self.router.step_queue(namespace);
        self.provider.send_message(&queue, &message).await?;
        Ok(())
    }

    /// Send step execution result to orchestration
    pub async fn send_step_result(&self, result: StepExecutionResult) -> TaskerResult<()> {
        let queue = self.router.result_queue();
        self.provider.send_message(&queue, &result).await?;
        Ok(())
    }

    /// Send task request for initialization
    pub async fn send_task_request(&self, request: TaskRequestMessage) -> TaskerResult<()> {
        let queue = self.router.task_request_queue();
        self.provider.send_message(&queue, &request).await?;
        Ok(())
    }

    /// Initialize queues for discovered namespaces
    pub async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        let queue_names: Vec<String> = namespaces
            .iter()
            .map(|ns| self.router.step_queue(ns))
            .collect();

        self.provider.ensure_queues(&queue_names).await?;
        Ok(())
    }

    /// Delete/acknowledge a processed message
    pub async fn delete_message(&self, queue_name: &str, receipt_handle: &ReceiptHandle) -> TaskerResult<()> {
        self.provider.ack_message(queue_name, receipt_handle).await?;
        Ok(())
    }

    /// Get queue metrics
    pub async fn get_queue_metrics(&self, queue_name: &str) -> TaskerResult<QueueStats> {
        Ok(self.provider.queue_stats(queue_name).await?)
    }

    /// Get provider name
    pub fn provider_name(&self) -> &'static str {
        self.provider.provider_name()
    }
}
```

---

## Phase 4: RabbitMQ Implementation

**Goal**: Implement RabbitMQ provider using lapin
**Child Ticket**: TAS-133d
**Validation Gate**:
- `RabbitMqMessagingService` passes same conformance test suite as PGMQ
- RabbitMQ runs in docker-compose.test.yml
- Provider can be selected via TOML configuration

### 4.1 Add lapin Dependency

```toml
# tasker-shared/Cargo.toml
[dependencies]
lapin = "2.3"  # AMQP 0.9.1 client
```

### 4.2 Create `rabbitmq.rs`

```rust
use lapin::{Connection, Channel, BasicProperties, options::*, types::FieldTable};

pub struct RabbitMqMessagingService {
    connection: Connection,
    channel: Channel,
    config: RabbitMqConfig,
}

impl RabbitMqMessagingService {
    pub async fn new(config: &RabbitMqConfig) -> Result<Self, MessagingError> {
        let connection = Connection::connect(&config.url, ConnectionProperties::default())
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

#[async_trait]
impl MessagingService for RabbitMqMessagingService {
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError> {
        self.channel.queue_declare(
            queue_name,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|e| MessagingError::Provider(e.to_string()))?;

        Ok(())
    }

    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<MessageId, MessagingError> {
        let bytes = message.to_bytes()?;

        let confirm = self.channel.basic_publish(
            "",  // default exchange
            queue_name,  // routing key = queue name
            BasicPublishOptions::default(),
            &bytes,
            BasicProperties::default()
                .with_delivery_mode(2)  // persistent
                .with_content_type("application/json".into()),
        )
        .await
        .map_err(|e| MessagingError::Provider(e.to_string()))?
        .await
        .map_err(|e| MessagingError::Provider(e.to_string()))?;

        Ok(MessageId(format!("{}", confirm.delivery_tag().unwrap_or(0))))
    }

    // ... remaining implementations
}
```

---

## Phase 5: SystemContext Integration

**Goal**: Update SystemContext to use the new abstractions
**Child Ticket**: TAS-133e
**Validation Gate**:
- `SystemContext` uses `Arc<MessagingProvider>` and `Arc<MessageClient>`
- `[common.messaging]` TOML config section works
- Full integration test suite passes with PGMQ provider
- Can switch to RabbitMQ via config and basic operations work

### 5.1 Update SystemContext Fields

```rust
pub struct SystemContext {
    pub processor_uuid: Uuid,
    pub tasker_config: Arc<TaskerConfig>,

    /// Low-level messaging provider (enum dispatch, NOT trait object)
    pub messaging_provider: Arc<MessagingProvider>,

    /// Domain-level message client (convenience methods)
    pub message_client: Arc<MessageClient>,

    // ... other fields unchanged
}
```

### 5.2 Update Factory Method

```rust
impl SystemContext {
    async fn create_messaging(
        config: &TaskerConfig,
        pgmq_pool: PgPool,
    ) -> TaskerResult<(Arc<MessagingProvider>, Arc<MessageClient>)> {
        // Create provider enum variant based on config
        let provider = match config.common.messaging.provider.as_str() {
            "pgmq" => MessagingProvider::Pgmq(PgmqMessagingService::new(pgmq_pool).await?),
            "rabbitmq" => MessagingProvider::RabbitMq(
                RabbitMqMessagingService::new(&config.common.messaging.rabbitmq).await?
            ),
            other => return Err(TaskerError::ConfigurationError(
                format!("Unknown messaging provider: {}", other)
            )),
        };
        let provider = Arc::new(provider);

        // Create router enum variant (DefaultMessageRouter for now)
        let router = MessageRouterKind::Default(
            DefaultMessageRouter::from_config(&config.common.queues)
        );
        let client = Arc::new(MessageClient::new(provider.clone(), router));

        Ok((provider, client))
    }
}
```

---

## Phase 6: Migration & Cleanup

**Goal**: Remove legacy abstractions and finalize the refactor
**Child Ticket**: TAS-133f
**Validation Gate**:
- All old traits/structs deleted (see 6.2)
- No `UnifiedPgmqClient`, `UnifiedMessageClient`, old `MessageClient` trait references
- `EventConsumer` uses `MessagingProvider` directly (no downcast pattern)
- Full test suite passes
- `cargo clippy --all-features` clean

### 6.1 Migration Checklist

- [ ] Create `messaging/service/` module with traits and types
- [ ] Implement `PgmqMessagingService`
- [ ] Implement `InMemoryMessagingService` for testing
- [ ] Create `MessageClient` struct
- [ ] Update `SystemContext` to use new types
- [ ] Add `[messaging]` config section to TOML
- [ ] Migrate `EventConsumer` away from downcast pattern
- [ ] Add RabbitMQ to docker-compose.test.yml (DONE)
- [ ] Implement `RabbitMqMessagingService`
- [ ] Delete old traits: `MessageClient` (trait), `PgmqClientTrait`
- [ ] Delete old structs: `UnifiedMessageClient` (enum), `UnifiedPgmqClient`
- [ ] Rename: struct `MessageClient` takes the name of deleted trait
- [ ] **Rename `SimpleStepMessage` → `StepMessage`** (delete old `StepMessage` with embedded context)
- [ ] Remove `send_simple_step_message` method (consolidated into `send_step_message`)
- [ ] Conformance test suite

### 6.2 Files to Delete (Post-Migration)

```
tasker-shared/src/messaging/clients/traits.rs     # Old MessageClient trait
tasker-shared/src/messaging/clients/unified_client.rs  # Old enum
tasker-shared/src/messaging/mod.rs                # UnifiedPgmqClient wrapper
```

### 6.3 Files to Create

```
tasker-shared/src/messaging/service/mod.rs
tasker-shared/src/messaging/service/traits.rs     # MessagingService trait
tasker-shared/src/messaging/service/types.rs
tasker-shared/src/messaging/service/errors.rs
tasker-shared/src/messaging/service/router.rs
tasker-shared/src/messaging/service/provider.rs   # MessagingProvider enum (dispatch)
tasker-shared/src/messaging/service/pgmq.rs       # PgmqMessagingService impl
tasker-shared/src/messaging/service/rabbitmq.rs   # RabbitMqMessagingService impl
tasker-shared/src/messaging/service/in_memory.rs  # InMemoryMessagingService impl
tasker-shared/src/messaging/service/factory.rs    # Factory for provider creation
tasker-shared/src/messaging/client.rs             # MessageClient struct (domain facade)
```

---

## Configuration

### TOML Schema Addition

```toml
# config/tasker/base/common.toml

[common.messaging]
provider = "pgmq"  # "pgmq" | "rabbitmq"
default_visibility_timeout_seconds = 30
default_batch_size = 10
max_batch_size = 100

[common.messaging.pgmq]
enable_pg_notify = true

[common.messaging.rabbitmq]
url = "amqp://guest:guest@localhost:5672/"
vhost = "/"
prefetch_count = 10
heartbeat_seconds = 60
connection_timeout_seconds = 30

[common.messaging.dead_letter]
enabled = true
max_receive_count = 3
queue_suffix = "_dlq"
```

---

## Conceptual Clarification: Archive vs DLQ

There are **three distinct concepts** that are often confused. This section documents each:

### 1. PGMQ Archive Tables (`a_{queue_name}`)

**What**: PGMQ-specific feature. When you call `pgmq.archive(queue, msg_id)`, the message moves from `q_{queue_name}` to `a_{queue_name}` for audit/history.

**When**: Used for compliance, debugging, or audit trail requirements. The message is "done" but preserved.

**Provider mapping**:
- **PGMQ**: Native `archive_message()` function
- **RabbitMQ**: Not directly supported (would need separate archive queue pattern)

### 2. Message Queue DLQ (`{queue_name}_dlq`)

**What**: Dead Letter Queue for messages that fail processing repeatedly. After `max_receive_count` failures, the message moves to DLQ instead of cycling infinitely.

**When**: Automatic after repeated nack/timeout. Used for poison message isolation.

**Provider mapping**:
- **PGMQ**: Configurable via `nack_message(requeue=false)` → archive or DLQ
- **RabbitMQ**: Native DLX (Dead Letter Exchange) with `x-dead-letter-exchange` argument

### 3. Task-Level DLQ (`tasker.tasks` with `dead_letter` flag)

**What**: Domain-level concept in Tasker. A task marked as "dead letter" at the application level (not message queue level).

**When**: Business logic determines task cannot proceed (missing data, permanent failure). This is independent of message queue mechanics.

**Where**: `tasker.tasks.dead_letter` boolean column, separate from message queue DLQs.

### Summary Table

| Concept | Scope | Trigger | Location |
|---------|-------|---------|----------|
| PGMQ Archive | Message queue | Explicit `archive()` call | `a_{queue}` table |
| Message DLQ | Message queue | Auto after max_receive_count | `{queue}_dlq` queue |
| Task DLQ | Domain/Application | Business logic decision | `tasker.tasks.dead_letter` |

---

## Serialization Strategy

### Current: JSON via serde_json

All message serialization currently uses JSON (`serde_json`). PGMQ stores messages in `jsonb` columns which provides:
- Human readability for debugging
- PostgreSQL JSON operators for ad-hoc queries
- Cross-language compatibility (Ruby/Python workers)

### Future: MessagePack Option

Upcoming tickets will add MessagePack support for performance-sensitive paths:
- Binary format (smaller payloads, faster serialization)
- `QueueMessage` trait abstracts this - implementations can choose format
- PGMQ: Store as `bytea` or base64-encoded string in jsonb
- RabbitMQ: Native binary payload support

The `QueueMessage` trait design anticipates this:
```rust
pub trait QueueMessage: Send + Sync + Clone + 'static {
    fn to_bytes(&self) -> Result<Vec<u8>, MessagingError>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, MessagingError> where Self: Sized;
}
```

JSON implementations serialize to UTF-8 bytes; MessagePack implementations serialize to binary. The provider doesn't care - it just passes bytes.

---

## Feature Flags & Build Considerations

### Decision: No Feature Flags for RabbitMQ

Based on discussion, we're **NOT** using feature flags for RabbitMQ to avoid:
- Multiple binary builds for FFI workers (Ruby gem, Python package, TypeScript)
- Complexity for end users who just want "it to work"

The `lapin` dependency is always included. Provider selection happens at runtime via TOML configuration.

### Future Consideration

If binary size becomes a concern, we could add a `messaging-minimal` feature that excludes RabbitMQ. But this is a future optimization, not an initial requirement.

---

## Success Criteria

| Criterion | Measurement |
|-----------|-------------|
| Zero breaking changes during migration | Existing tests pass at each phase |
| Provider conformance | Both PGMQ and RabbitMQ pass identical test suite |
| Configuration-only switching | Change `provider = "rabbitmq"` and restart |
| Abstraction count reduced | From 4+ (MessageClient trait, PgmqClientTrait, UnifiedMessageClient, UnifiedPgmqClient) to 2 (MessagingService trait + MessagingProvider enum, MessageClient struct) |
| No downcast patterns | EventConsumer uses MessagingProvider directly |
| Hot-path performance | Enum dispatch avoids vtable overhead on send/receive paths |

---

## Child Tickets

Detailed specifications for each phase are in the [phases/](./phases/) directory:

| Phase | Ticket | Summary |
|-------|--------|---------|
| 1 | [TAS-133a](./phases/TAS-133a-trait-definitions.md) | Trait definitions, types, errors, router |
| 2 | [TAS-133b](./phases/TAS-133b-pgmq-implementation.md) | PgmqMessagingService + InMemory |
| 3 | [TAS-133c](./phases/TAS-133c-message-client-struct.md) | MessageClient struct, StepMessage rename |
| 4 | [TAS-133d](./phases/TAS-133d-rabbitmq-implementation.md) | RabbitMqMessagingService |
| 5 | [TAS-133e](./phases/TAS-133e-system-context-integration.md) | SystemContext integration, TOML config |
| 6 | [TAS-133f](./phases/TAS-133f-migration-cleanup.md) | Delete old code, final cleanup |

See [phases/README.md](./phases/README.md) for dependency graph and branching strategy.

---

## Related Documents

- [README.md](./README.md) - Original TAS-133 specification
- [trait-design.md](./trait-design.md) - Trait semantics deep dive
- [push-notifications-research.md](./push-notifications-research.md) - Provider push capabilities
- [queue-lifecycle.md](./queue-lifecycle.md) - Queue creation patterns
- [message-lifecycle-patterns.md](./message-lifecycle-patterns.md) - Read→Evaluate→Delete patterns
- [research-message-client-usage.md](./research-message-client-usage.md) - Current MessageClient usage
- [research-pgmq-trait-comparison.md](./research-pgmq-trait-comparison.md) - Trait method comparison
- [research-system-context-messaging.md](./research-system-context-messaging.md) - SystemContext patterns

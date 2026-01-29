# Research: SystemContext Messaging Architecture

**Last Updated**: 2026-01-13
**Status**: Complete

---

## Summary

The `SystemContext` struct serves as a centralized dependency injection container for the tasker-core framework. It manages messaging through two key abstractions:

1. **`UnifiedPgmqClient`** (primary PGMQ-specific client with notify capabilities)
2. **`UnifiedMessageClient`** (enum-based abstraction supporting multiple backends)

These messaging clients are injected into all actors and services through the shared `SystemContext`, enabling a flexible, testable, and extensible messaging infrastructure for workflow orchestration.

---

## SystemContext Definition

**File:** `tasker-shared/src/system_context.rs`

```rust
pub struct SystemContext {
    /// System instance ID
    pub processor_uuid: Uuid,

    /// Context-based Tasker Config (TAS-61 Phase 6C/6D: canonical config)
    pub tasker_config: Arc<TaskerConfig>,

    /// Unified message queue client (PGMQ/RabbitMQ abstraction)
    pub message_client: Arc<UnifiedPgmqClient>,

    /// Database connection pools (TAS-78: supports separate PGMQ database)
    database_pools: DatabasePools,

    /// Task handler registry
    pub task_handler_registry: Arc<TaskHandlerRegistry>,

    /// Circuit breaker manager (optional)
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,

    /// Event publisher
    pub event_publisher: Arc<EventPublisher>,
}
```

---

## Messaging Field Analysis

### UnifiedPgmqClient (Primary Field)

**Type:** `Arc<UnifiedPgmqClient>`
**Location:** `tasker-shared/src/messaging/mod.rs`

```rust
pub struct UnifiedPgmqClient {
    /// The underlying PGMQ client from tasker-pgmq
    client: PgmqClient,
}
```

**Responsibilities:**
- Wraps `PgmqClient` from the pgmq-notify crate
- Implements `PgmqClientTrait` for low-level message queue operations
- Provides notification-based event processing for LISTEN/NOTIFY patterns
- Handles queue management (create, read, write, delete messages)

**Key Methods:**
- `create_queue(queue_name)` - Create PGMQ queue
- `send_message(queue_name, message)` - Send step execution message
- `read_messages(queue_name, timeout, limit)` - Receive messages from queue
- `delete_message(queue_name, message_id)` - Acknowledge message processing
- `read_specific_message(queue_name, message_id)` - Event-driven message reading
- `initialize_namespace_queues(namespaces)` - Set up namespace-specific queues
- `get_queue_length(queue_name)` - Queue metrics

---

## Construction/Factory Patterns

### 1. Production Initialization

```rust
pub async fn new_for_orchestration() -> TaskerResult<Self> {
    let environment = crate::config::ConfigLoader::detect_environment();
    let config_manager = ConfigManager::load_from_env(&environment)?;
    Self::from_config(config_manager).await
}
```

### 2. Configuration-Based Construction

```rust
pub async fn from_config(config_manager: Arc<ConfigManager>) -> TaskerResult<Self> {
    let config = config_manager.config();
    let database_pools = DatabasePools::from_config(config).await?;
    Self::from_pools_and_config(database_pools, config_manager).await
}
```

### 3. Core Construction with Messaging Client Creation

```rust
async fn get_circuit_breaker_and_queue_client(
    config: &TaskerConfig,
    pgmq_pool: PgPool,
) -> (Option<Arc<CircuitBreakerManager>>, Arc<UnifiedPgmqClient>) {
    // Check if circuit breakers enabled in config
    let circuit_breaker_manager = if config.common.circuit_breakers.enabled {
        Some(Arc::new(CircuitBreakerManager::from_config(&config.common.circuit_breakers)))
    } else {
        None
    };

    // Create PGMQ client from pool
    let standard_client = PgmqClient::new_with_pool(pgmq_pool).await;
    let message_client = Arc::new(UnifiedPgmqClient::new_standard(standard_client));

    (circuit_breaker_manager, message_client)
}
```

### 4. Testing Construction

```rust
#[cfg(any(test, feature = "test-utils"))]
pub async fn with_pool(database_pool: sqlx::PgPool) -> TaskerResult<Self> {
    let message_client = Arc::new(UnifiedPgmqClient::new_standard(
        PgmqClient::new_with_pool(database_pools.pgmq().clone()).await,
    ));

    Ok(Self {
        message_client,
        circuit_breaker_manager: None, // Disabled for testing
        // ...
    })
}
```

---

## Actor Usage Patterns

### 1. ActorRegistry - Centralized Actor Construction

**File:** `tasker-orchestration/src/actors/registry.rs`

```rust
pub struct ActorRegistry {
    context: Arc<SystemContext>,
    pub task_request_actor: Arc<TaskRequestActor>,
    pub result_processor_actor: Arc<ResultProcessorActor>,
    pub step_enqueuer_actor: Arc<StepEnqueuerActor>,
    pub task_finalizer_actor: Arc<TaskFinalizerActor>,
    pub decision_point_actor: Arc<DecisionPointActor>,
    pub batch_processing_actor: Arc<BatchProcessingActor>,
}

impl ActorRegistry {
    pub async fn build(context: Arc<SystemContext>) -> TaskerResult<Self> {
        let task_request_processor = Arc::new(TaskRequestProcessor::new(
            context.message_client.clone(),  // Extracted from SystemContext
            context.task_handler_registry.clone(),
            task_initializer,
            TaskRequestProcessorConfig::default(),
        ));
        // ... create actors with injected message_client
    }
}
```

### 2. Service Layer - Direct Injection

```rust
pub struct TaskRequestProcessor {
    pgmq_client: Arc<UnifiedPgmqClient>,  // Injected from SystemContext
    task_handler_registry: Arc<TaskHandlerRegistry>,
    task_initializer: Arc<TaskInitializer>,
    config: TaskRequestProcessorConfig,
}

impl TaskRequestProcessor {
    pub async fn process_batch(&self) -> TaskerResult<usize> {
        let messages = self
            .pgmq_client
            .read_messages(
                &self.config.request_queue_name,
                Some(self.config.visibility_timeout_seconds),
                Some(self.config.batch_size),
            )
            .await?;
        // Process messages...
        Ok(messages.len())
    }
}
```

### 3. Worker Event Consumer Pattern

```rust
pub struct EventConsumer {
    message_client: Arc<UnifiedMessageClient>,
}

impl EventConsumer {
    async fn listen_to_pgmq(&mut self) -> TaskerResult<()> {
        let pgmq_client = self.message_client.as_pgmq()
            .ok_or_else(|| TaskerError::ConfigurationError(...))?;

        // Event-driven message processing via downcast
        // ...
    }
}
```

---

## Queue Initialization Methods

```rust
impl SystemContext {
    /// Initialize namespace queues (worker queues)
    pub async fn initialize_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        self.message_client
            .initialize_namespace_queues(namespaces)
            .await?;
        Ok(())
    }

    /// Initialize orchestration-owned queues
    pub async fn initialize_orchestration_owned_queues(&self) -> TaskerResult<()> {
        let queue_config = self.tasker_config.common.queues.clone();

        let orchestration_owned_queues = vec![
            queue_config.orchestration_queues.step_results.as_str(),
            queue_config.orchestration_queues.task_requests.as_str(),
            queue_config.orchestration_queues.task_finalizations.as_str(),
        ];

        for queue_name in orchestration_owned_queues {
            self.message_client.create_queue(queue_name).await?;
        }
        Ok(())
    }

    /// Initialize domain event queues for namespaces
    pub async fn initialize_domain_event_queues(&self, namespaces: &[&str]) -> TaskerResult<()> {
        for namespace in namespaces {
            let main_queue = format!("{}_domain_events", namespace);
            let dlq_queue = format!("{}_domain_events_dlq", namespace);

            self.message_client.create_queue(&main_queue).await?;
            self.message_client.create_queue(&dlq_queue).await?;
        }
        Ok(())
    }
}
```

---

## Recommendations for TAS-133

### 1. Update SystemContext Field Type

**Current:**
```rust
pub message_client: Arc<UnifiedPgmqClient>,
```

**Proposed (Enum Dispatch):**
```rust
pub messaging_provider: Arc<MessagingProvider>,
pub message_client: Arc<MessageClient>,
```

We use **enum dispatch** instead of `Arc<dyn MessagingService>` for hot-path performance:
- No vtable indirection on every send/receive
- Compiler can inline provider methods
- No generic proliferation (`SystemContext` stays non-generic)
- Same pattern as current `UnifiedMessageClient`

### 2. MessagingProvider Enum Definition

```rust
/// Provider enum for zero-cost dispatch (no vtable overhead)
pub enum MessagingProvider {
    Pgmq(PgmqMessagingService),
    RabbitMq(RabbitMqMessagingService),
    InMemory(InMemoryMessagingService),
}

impl MessagingProvider {
    /// Delegate to underlying provider
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

    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Pgmq(_) => "pgmq",
            Self::RabbitMq(_) => "rabbitmq",
            Self::InMemory(_) => "in_memory",
        }
    }

    // ... delegate all MessagingService trait methods
}
```

### 3. Factory Pattern for Provider Selection

```rust
impl SystemContext {
    async fn create_messaging(
        config: &TaskerConfig,
        pgmq_pool: PgPool,
    ) -> TaskerResult<(Arc<MessagingProvider>, Arc<MessageClient>)> {
        // Create provider enum variant based on config
        let provider = match config.common.messaging.provider.as_str() {
            "pgmq" => MessagingProvider::Pgmq(
                PgmqMessagingService::new(pgmq_pool).await?
            ),
            "rabbitmq" => MessagingProvider::RabbitMq(
                RabbitMqMessagingService::new(&config.common.messaging.rabbitmq).await?
            ),
            other => return Err(TaskerError::ConfigurationError(
                format!("Unknown messaging provider: {}", other)
            )),
        };
        let provider = Arc::new(provider);

        // Create router enum variant (no trait objects)
        let router = MessageRouterKind::Default(
            DefaultMessageRouter::from_config(&config.common.queues)
        );
        let client = Arc::new(MessageClient::new(provider.clone(), router));

        Ok((provider, client))
    }
}
```

### 4. Preserve Domain Helpers via MessageClient Struct

After TAS-133, `MessageClient` becomes a **struct** (not trait) that wraps both enums - no trait objects anywhere:

```rust
pub struct MessageClient {
    provider: Arc<MessagingProvider>,
    router: MessageRouterKind,  // Also enum dispatch, not Arc<dyn>
}

impl MessageClient {
    pub fn new(provider: Arc<MessagingProvider>, router: MessageRouterKind) -> Self {
        Self { provider, router }
    }

    /// Get the underlying provider for advanced operations
    pub fn provider(&self) -> &Arc<MessagingProvider> {
        &self.provider
    }

    /// Get the router for queue name lookups
    pub fn router(&self) -> &MessageRouterKind {
        &self.router
    }

    /// Send a step message to the appropriate namespace queue
    ///
    /// NOTE: `StepMessage` is the UUID-based message (formerly `SimpleStepMessage`).
    /// The old `StepMessage` with embedded execution context has been removed.
    pub async fn send_step_message(&self, namespace: &str, msg: StepMessage) -> TaskerResult<()> {
        let queue = self.router.step_queue(namespace);
        self.provider.send_message(&queue, &msg).await?;
        Ok(())
    }

    pub async fn send_step_result(&self, result: StepExecutionResult) -> TaskerResult<()> {
        let queue = self.router.result_queue();
        self.provider.send_message(&queue, &result).await?;
        Ok(())
    }
}
```

SystemContext exposes both:
```rust
pub struct SystemContext {
    pub messaging_provider: Arc<MessagingProvider>,  // Low-level enum dispatch
    pub message_client: Arc<MessageClient>,          // Domain helpers (also uses enums internally)
}
```

### 5. Actor Migration Path

1. **Phase 1**: Add `messaging_provider` and new `message_client` fields alongside old `message_client`
2. **Phase 2**: Migrate services to use new `message_client` (struct) or `messaging_provider` directly
3. **Phase 3**: Remove old `UnifiedPgmqClient` field reference
4. **Phase 4**: Delete old traits/structs (`UnifiedPgmqClient`, old `MessageClient` trait, `PgmqClientTrait`)

### 6. Testing Improvements

```rust
// Easy to test with InMemory variant - no trait objects needed
pub async fn with_in_memory_messaging() -> TaskerResult<Self> {
    let provider = Arc::new(MessagingProvider::InMemory(
        InMemoryMessagingService::new()
    ));
    let router = MessageRouterKind::Default(DefaultMessageRouter::default());
    let client = Arc::new(MessageClient::new(provider.clone(), router));

    Ok(Self {
        messaging_provider: provider,
        message_client: client,
        // ...
    })
}

// For custom test scenarios, use InMemory variant with inspection
#[cfg(test)]
impl MessagingProvider {
    pub fn as_in_memory(&self) -> Option<&InMemoryMessagingService> {
        match self {
            Self::InMemory(s) => Some(s),
            _ => None,
        }
    }
}
```

### 7. Why Enum Dispatch Over Trait Objects

We use enum dispatch for both `MessagingProvider` and `MessageRouterKind`:

| Aspect | `Arc<dyn Trait>` | Enum Dispatch |
|--------|------------------|---------------|
| Dispatch cost | vtable lookup per call | Branch prediction (near-free) |
| Inlining | Not possible | Compiler can inline |
| Generic marker | None needed | None needed |
| Adding variants | Just implement trait | Add enum variant + match arms |
| Binary size | Smaller | Slightly larger (all variants compiled) |

**When to use `dyn` trait objects:**
- Plugin systems where types are unknown at compile time
- Runtime-loaded extensions
- When you need unbounded extensibility

**When to use enum dispatch:**
- Closed set of known variants
- Hot-path operations (messaging, routing)
- When all variants are known at compile time
- When inlining and optimization matter

For TAS-133, both `MessagingProvider` (PGMQ/RabbitMQ/InMemory) and `MessageRouterKind` (Default) are closed sets known at compile time, making enum dispatch the better choice.

---

## Key Files for TAS-133 Updates

| File | Change |
|------|--------|
| `tasker-shared/src/system_context.rs` | Add `messaging_provider` field, update factory |
| `tasker-shared/src/messaging/service/traits.rs` | `MessagingService` trait definition |
| `tasker-shared/src/messaging/service/provider.rs` | `MessagingProvider` enum with dispatch |
| `tasker-shared/src/messaging/service/pgmq.rs` | `PgmqMessagingService` implementation |
| `tasker-shared/src/messaging/service/rabbitmq.rs` | `RabbitMqMessagingService` implementation |
| `tasker-shared/src/messaging/service/in_memory.rs` | `InMemoryMessagingService` for testing |
| `tasker-shared/src/messaging/client.rs` | New `MessageClient` struct (domain facade) |
| `tasker-orchestration/src/actors/registry.rs` | Update actor construction |
| `tasker-worker/src/worker/event_consumer.rs` | Remove downcast pattern, use provider directly |

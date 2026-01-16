# TAS-133e: SystemContext Integration

**Parent**: TAS-133 (Messaging Service Strategy Pattern Abstraction)
**Status**: Ready for Implementation
**Branch**: `jcoletaylor/tas-133e-system-context-integration`
**Merges To**: `jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction`
**Depends On**: TAS-133c, TAS-133d

---

## Overview

Update `SystemContext` to use the new messaging abstractions (`MessagingProvider`, `MessageClient`) and add TOML configuration for provider selection. This is the integration phase where all components come together.

## Validation Gate

- [ ] `SystemContext` uses `Arc<MessagingProvider>` and `Arc<MessageClient>`
- [ ] `[common.messaging]` TOML config section works
- [ ] Full integration test suite passes with PGMQ provider
- [ ] Can switch to RabbitMQ via config and basic operations work
- [ ] ActorRegistry uses new MessageClient
- [ ] Worker EventConsumer works without downcast pattern

## Scope

### SystemContext Field Updates

```rust
// tasker-shared/src/system_context.rs

pub struct SystemContext {
    pub processor_uuid: Uuid,
    pub tasker_config: Arc<TaskerConfig>,

    /// Low-level messaging provider (enum dispatch)
    pub messaging_provider: Arc<MessagingProvider>,

    /// Domain-level message client (convenience methods)
    pub message_client: Arc<MessageClient>,

    // ... other fields unchanged
    database_pools: DatabasePools,
    pub task_handler_registry: Arc<TaskHandlerRegistry>,
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,
    pub event_publisher: Arc<EventPublisher>,
}
```

### Factory Method Update

```rust
impl SystemContext {
    async fn create_messaging(
        config: &TaskerConfig,
        pgmq_pool: PgPool,
    ) -> TaskerResult<(Arc<MessagingProvider>, Arc<MessageClient>)> {
        // Create provider based on config
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

        // Create router
        let router = MessageRouterKind::Default(
            DefaultMessageRouter::from_config(&config.common.queues)
        );
        let client = Arc::new(MessageClient::new(provider.clone(), router));

        Ok((provider, client))
    }

    pub async fn from_pools_and_config(
        database_pools: DatabasePools,
        config_manager: Arc<ConfigManager>,
    ) -> TaskerResult<Self> {
        let config = config_manager.config();
        let (messaging_provider, message_client) =
            Self::create_messaging(config, database_pools.pgmq().clone()).await?;

        // ... rest of construction
    }
}
```

### TOML Configuration

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

### Configuration Types

```rust
// tasker-shared/src/config/components/messaging.rs

#[derive(Debug, Clone, Deserialize)]
pub struct MessagingConfig {
    pub provider: String,
    pub default_visibility_timeout_seconds: u32,
    pub default_batch_size: usize,
    pub max_batch_size: usize,
    pub pgmq: PgmqConfig,
    pub rabbitmq: RabbitMqConfig,
    pub dead_letter: DeadLetterConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PgmqConfig {
    pub enable_pg_notify: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RabbitMqConfig {
    pub url: String,
    pub vhost: String,
    pub prefetch_count: u16,
    pub heartbeat_seconds: u16,
    pub connection_timeout_seconds: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeadLetterConfig {
    pub enabled: bool,
    pub max_receive_count: u32,
    pub queue_suffix: String,
}
```

### ActorRegistry Update

```rust
// tasker-orchestration/src/actors/registry.rs

impl ActorRegistry {
    pub async fn build(context: Arc<SystemContext>) -> TaskerResult<Self> {
        // Use new MessageClient instead of old message_client
        let task_request_processor = Arc::new(TaskRequestProcessor::new(
            context.message_client.clone(),  // New MessageClient struct
            context.task_handler_registry.clone(),
            task_initializer,
            TaskRequestProcessorConfig::default(),
        ));
        // ...
    }
}
```

### EventConsumer Refactor

Remove downcast pattern:

```rust
// tasker-worker/src/worker/event_consumer.rs

// OLD (with downcast)
let pgmq_client = self.message_client.as_pgmq()
    .ok_or_else(|| TaskerError::MessagingError("Expected PGMQ client".to_string()))?;
pgmq_client.read_messages(queue, timeout, batch).await?

// NEW (direct provider access)
let messages = self.messaging_provider
    .receive_messages::<StepMessage>(queue, batch, Duration::from_secs(timeout))
    .await?;
```

## Implementation Notes

### Backward Compatibility During Migration

Temporarily keep both old and new fields:

```rust
pub struct SystemContext {
    // New fields
    pub messaging_provider: Arc<MessagingProvider>,
    pub message_client: Arc<MessageClient>,

    // Old field (deprecated, remove in Phase 6)
    #[deprecated(note = "Use message_client instead")]
    pub old_message_client: Arc<UnifiedPgmqClient>,
}
```

This allows gradual migration of call sites.

### Testing Mode

```rust
impl SystemContext {
    #[cfg(any(test, feature = "test-utils"))]
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
}
```

### Provider Accessor for Advanced Use

```rust
impl SystemContext {
    /// Get the underlying messaging provider for advanced operations
    ///
    /// Prefer using `message_client` for domain operations. Use this only
    /// when you need provider-specific capabilities.
    pub fn messaging_provider(&self) -> &Arc<MessagingProvider> {
        &self.messaging_provider
    }
}
```

## Files to Modify

| File | Change |
|------|--------|
| `tasker-shared/src/system_context.rs` | Add new fields, update factory |
| `tasker-shared/src/config/components/mod.rs` | Add messaging config |
| `tasker-shared/src/config/components/messaging.rs` | Create config structs |
| `config/tasker/base/common.toml` | Add `[common.messaging]` section |
| `tasker-orchestration/src/actors/registry.rs` | Use new MessageClient |
| `tasker-worker/src/worker/event_consumer.rs` | Remove downcast, use provider |
| Various service files | Update to use new client |

## Testing Strategy

### Integration Tests

```rust
#[tokio::test]
async fn test_system_context_with_pgmq() {
    let context = SystemContext::new_for_worker().await.unwrap();
    assert_eq!(context.messaging_provider.provider_name(), "pgmq");

    // Verify operations work
    context.message_client.initialize_namespace_queues(&["test"]).await.unwrap();
}
```

### Provider Switching Test

```rust
#[tokio::test]
async fn test_provider_selection_via_config() {
    // With PGMQ config
    std::env::set_var("TASKER_MESSAGING_PROVIDER", "pgmq");
    let ctx1 = SystemContext::new_for_worker().await.unwrap();
    assert_eq!(ctx1.messaging_provider.provider_name(), "pgmq");

    // With RabbitMQ config
    std::env::set_var("TASKER_MESSAGING_PROVIDER", "rabbitmq");
    let ctx2 = SystemContext::new_for_worker().await.unwrap();
    assert_eq!(ctx2.messaging_provider.provider_name(), "rabbitmq");
}
```

### Full Integration Suite

Run complete orchestration + worker tests with new abstractions:

```bash
cargo test --all-features --package tasker-orchestration integration
cargo test --all-features --package tasker-worker integration
```

## Dependencies

- TAS-133c (MessageClient struct must exist)
- TAS-133d (RabbitMQ implementation for switching test)

## Estimated Complexity

**High** - Touches many files across multiple crates. Integration testing is critical.

---

## References

- [implementation-plan.md](../implementation-plan.md) - Phase 5 section
- [research-system-context-messaging.md](../research-system-context-messaging.md) - Current patterns
- [configuration-management.md](../../../../docs/guides/configuration-management.md) - Config system

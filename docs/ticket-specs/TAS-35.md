# TAS-35: Message Service Abstraction Implementation Plan

## Executive Summary

Implement a trait-based abstraction layer for message queue operations to enable pluggable queue providers (PGMQ, RabbitMQ, SQS, etc.) while maintaining backward compatibility with existing PGMQ-based system and integrating with TAS-34's OrchestrationExecutor architecture.

## Background & Motivation

The current Tasker system is tightly coupled to PGMQ (PostgreSQL Message Queue), which provides excellent "all-in-one" capabilities with minimal dependencies. However, this coupling presents limitations:

- **Throughput Constraints**: PGMQ, while reliable, may not match the throughput of dedicated message brokers
- **Operational Flexibility**: Organizations may have existing investments in RabbitMQ, Kafka, SQS, or Redis
- **Scaling Patterns**: Different queue systems offer different scaling characteristics
- **Feature Sets**: Some providers offer unique features (e.g., Kafka's event streaming, RabbitMQ's flexible routing)

## Design Principles

1. **PostgreSQL as Source of Truth**: The database remains the authoritative data store
2. **Simple UUID Messages**: Maintain the 3-field UUID message structure
3. **Pull-Based Consistency**: All providers use pull model for uniform behavior
4. **Provider Agnostic**: Core business logic remains independent of queue provider
5. **Zero Downtime Migration**: Support gradual migration between providers
6. **Test Parity**: All providers must pass the same test suite

## Phase 1: Core Trait Architecture (Week 1)

**Prerequisites**: TAS-34 implementation must be complete, particularly:
- OrchestrationExecutor trait and ExecutorType enum
- CircuitBreakerManager and resilience patterns
- SystemResourceLimits and ResourceBudget validation
- GlobalBackpressureController (or at least interfaces)

### 1.1 Rust Service Traits

Create the foundational messaging service traits that abstract queue operations:

```rust
// src/services/messaging/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Core message trait for all queue messages
pub trait QueueMessage: Send + Sync + Clone {
    fn message_id(&self) -> &str;
    fn message_type(&self) -> &str;
    fn correlation_id(&self) -> Option<&str>;
    fn to_json(&self) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>;
    fn from_json(value: serde_json::Value) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        Self: Sized;
}

/// Metadata about a queued message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessageMetadata {
    pub queue_id: String,           // Provider-specific ID (i64 for PGMQ, String for others)
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
    pub receive_count: i32,
    pub visibility_timeout_expires: Option<chrono::DateTime<chrono::Utc>>,
    pub dead_letter_count: Option<i32>,
}

/// Wrapper for messages retrieved from queue
#[derive(Debug, Clone)]
pub struct QueuedMessage<T> {
    pub metadata: QueuedMessageMetadata,
    pub message: T,
}

/// Queue statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub queue_name: String,
    pub message_count: u64,
    pub in_flight_count: Option<u64>,
    pub oldest_message_age_seconds: Option<i64>,
    pub dead_letter_count: Option<u64>,
}

/// Core messaging service trait
#[async_trait]
pub trait MessagingService: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Initialize the service (create connections, etc.)
    async fn initialize(&mut self) -> Result<(), Self::Error>;

    /// Check if service is healthy
    async fn health_check(&self) -> Result<bool, Self::Error>;

    /// Create queue if it doesn't exist
    async fn create_queue(&self, queue_name: &str) -> Result<(), Self::Error>;

    /// Send a message to queue
    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<String, Self::Error>;

    /// Send batch of messages
    async fn send_batch<T: QueueMessage>(
        &self,
        queue_name: &str,
        messages: &[T],
    ) -> Result<Vec<String>, Self::Error>;

    /// Receive messages from queue (pull model)
    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, Self::Error>;

    /// Delete message after successful processing
    async fn delete_message(
        &self,
        queue_name: &str,
        receipt_handle: &str,
    ) -> Result<(), Self::Error>;

    /// Change visibility timeout for a message
    async fn change_message_visibility(
        &self,
        queue_name: &str,
        receipt_handle: &str,
        visibility_timeout: Duration,
    ) -> Result<(), Self::Error>;

    /// Get queue statistics
    async fn get_queue_stats(&self, queue_name: &str) -> Result<QueueStats, Self::Error>;

    /// Purge all messages from queue
    async fn purge_queue(&self, queue_name: &str) -> Result<u64, Self::Error>;

    /// Delete queue
    async fn delete_queue(&self, queue_name: &str) -> Result<(), Self::Error>;
}

/// AMQP-specific extensions for advanced messaging patterns
#[async_trait]
pub trait AmqpExtensions: MessagingService {
    /// Declare an exchange with specified type
    async fn declare_exchange(
        &self,
        name: &str,
        exchange_type: &str,  // "direct", "topic", "fanout", "headers"
        durable: bool,
    ) -> Result<(), Self::Error>;

    /// Bind a queue to an exchange with routing key
    async fn bind_queue(
        &self,
        queue: &str,
        exchange: &str,
        routing_key: &str,
    ) -> Result<(), Self::Error>;

    /// Publish to a specific exchange with routing
    async fn publish_to_exchange<M: QueueMessage>(
        &self,
        exchange: &str,
        routing_key: &str,
        message: M,
    ) -> Result<String, Self::Error>;

    /// Set QoS prefetch count for fair dispatch
    async fn set_qos(&self, prefetch_count: u16) -> Result<(), Self::Error>;

    /// Create a stream-based consumer for async iteration
    async fn create_consumer_stream(
        &self,
        queue: &str,
    ) -> Result<Box<dyn Stream<Item = Result<QueuedMessage<Box<dyn QueueMessage>>, Self::Error>> + Send>, Self::Error>;
}

/// PGMQ-specific extensions for PostgreSQL message queues
#[async_trait]
pub trait PgmqExtensions: MessagingService {
    /// Create queue with visibility timeout
    async fn create_queue_with_vt(&self, name: &str, vt: i64) -> Result<(), Self::Error>;

    /// Read batch of messages without removing
    async fn read_batch(
        &self,
        queue: &str,
        vt: i64,
        limit: i32,
    ) -> Result<Vec<QueuedMessage<Box<dyn QueueMessage>>>, Self::Error>;

    /// Archive messages to archive table
    async fn archive_messages(&self, queue: &str, msg_ids: &[i64]) -> Result<(), Self::Error>;

    /// Get queue metrics
    async fn get_metrics(&self, queue: &str) -> Result<serde_json::Value, Self::Error>;
}

/// Message routing strategy
#[async_trait]
pub trait MessageRouter: Send + Sync {
    /// Route a message to the appropriate queue based on namespace and executor type
    fn route_to_queue(&self, namespace: &str, executor_type: &ExecutorType) -> String;

    /// Extract namespace from queue name
    fn extract_namespace(&self, queue_name: &str) -> Option<String>;

    /// Get queue for specific executor type (TAS-34 integration)
    fn get_executor_queue(&self, executor_type: &ExecutorType) -> String;
}

/// Standard namespace-based router
#[derive(Debug, Clone)]
pub struct NamespaceQueueRouter {
    queue_suffix: String,
}

impl NamespaceQueueRouter {
    pub fn new(queue_suffix: &str) -> Self {
        Self {
            queue_suffix: queue_suffix.to_string(),
        }
    }
}

impl MessageRouter for NamespaceQueueRouter {
    fn route_to_queue(&self, namespace: &str, _message_type: &str) -> String {
        format!("{}_{}", namespace, self.queue_suffix)
    }

    fn extract_namespace(&self, queue_name: &str) -> Option<String> {
        queue_name
            .strip_suffix(&format!("_{}", self.queue_suffix))
            .map(|s| s.to_string())
    }
}

// Test integration with TAS-34 executor types
#[tokio::test]
async fn test_executor_queue_routing() {
    use crate::orchestration::executor::traits::ExecutorType;

    let router = NamespaceQueueRouter::new("queue");

    // Test each executor type gets appropriate queue
    assert_eq!(
        router.get_executor_queue(&ExecutorType::TaskRequestProcessor),
        "task_requests_queue"
    );
    assert_eq!(
        router.get_executor_queue(&ExecutorType::StepResultProcessor),
        "step_results_queue"
    );

    // Test namespace routing with executor type
    assert_eq!(
        router.route_to_queue("payments", &ExecutorType::StepEnqueuer),
        "payments_StepEnqueuer_queue"
    );
}

// Test circuit breaker integration
#[tokio::test]
async fn test_circuit_breaker_degradation() {
    use crate::resilience::{CircuitBreaker, CircuitBreakerConfig};

    let cb_config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout_ms: 1000,
        half_open_max_calls: 1,
    };

    let circuit_breaker = CircuitBreaker::new("test_service", cb_config);

    // Simulate failures to open circuit
    for _ in 0..3 {
        circuit_breaker.record_failure().await;
    }

    // Circuit should be open
    assert!(matches!(
        circuit_breaker.state(),
        crate::resilience::CircuitState::Open
    ));
}
```

### 4.3 Migration Strategy with TAS-34 Considerations

Migration must coordinate with TAS-34 executor pools:

1. **Phase 1**: Deploy MessagingService trait with PGMQ implementation
   - Maintain backward compatibility with existing PgmqClient
   - Integrate with ProtectedPgmqClient for circuit breaker support
   - Validate against SystemResourceLimits

2. **Phase 2**: Update OrchestrationCore to use MessagingService
   - Modify executor pools to use abstracted messaging
   - Ensure health monitoring aligns with ExecutorHealth states
   - Test scaling behavior with different providers

3. **Phase 3**: Gradual provider rollout
   - RabbitMQ for high-throughput namespaces
   - SQS for cloud deployments
   - Monitor resource usage via TAS-34 metrics
   - Coordinate scaling decisions with OrchestrationLoopCoordinator

### 4.4 Resource Management Integration

The messaging service must respect TAS-34's resource constraints:

```rust
impl MessagingService {
    async fn validate_resources(&self, limits: &SystemResourceLimits) -> Result<()> {
        let required = self.estimate_resource_usage();

        // Check database connections
        if required.db_connections > limits.available_db_connections() {
            return Err(MessagingError::InsufficientResources);
        }

        // Check memory
        if required.memory_mb > limits.available_memory_mb {
            return Err(MessagingError::InsufficientMemory);
        }

        Ok(())
    }
}
```

## Success Criteria

1. **Seamless Integration**: MessagingService works within TAS-34 executor framework
2. **Resource Compliance**: Respects SystemResourceLimits validation
3. **Health Alignment**: Health states compatible with ExecutorHealth enum
4. **Circuit Breaker Support**: Consistent with TAS-34's resilience patterns
5. **Performance**: No degradation when integrated with executor pools
6. **Backpressure Coordination**: Global backpressure works across messaging and executors
7. **Scaling Harmony**: Messaging service scales in coordination with executor pools
8. **Infrastructure Ready**:
   - Docker Compose includes both PostgreSQL/PGMQ and RabbitMQ services
   - CI/CD pipeline tests both messaging providers in parallel
   - Local development setup script for quick initialization
   - Management UIs accessible for debugging (RabbitMQ: http://localhost:15672)
9. **Provider Flexibility**:
   - Runtime switching between PGMQ and RabbitMQ via configuration
   - Graceful fallback when primary provider unavailable
   - Zero code changes required to switch providers
10. **Testing Coverage**:
   - Integration tests pass for both PGMQ and RabbitMQ
   - Performance benchmarks document provider differences
   - Failover scenarios tested and documented
   - CI matrix tests all provider combinations

## Risks and Mitigations

### Risk: Resource Contention with Executor Pools
**Mitigation**: Implement resource budgeting between messaging and executors using TAS-34's ResourceBudget

### Risk: Circuit Breaker Cascade
**Mitigation**: Coordinate circuit breaker states through CircuitBreakerManager

### Risk: Queue Routing Complexity
**Mitigation**: Use ExecutorType-based routing for clarity and consistency

### Risk: Backpressure Oscillation
**Mitigation**: Use TAS-34's hysteresis in GlobalBackpressureController

### Risk: Memory Leak from Message Buffering
**Mitigation**: Apply TAS-34's weak reference pattern for long-lived handlers

### 1.2 Configuration Schema

Configuration must align with TAS-34's resource management and circuit breaker patterns:

Extend the configuration system to support multiple queue providers:

```rust
// src/config/messaging.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageServiceProvider {
    Pgmq,
    RabbitMq,
    Sqs,
    Redis,
    Kafka,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingConfig {
    pub provider: MessageServiceProvider,
    pub connection: ConnectionConfig,
    pub queues: QueueConfig,
    pub retry_policy: RetryPolicy,
    pub circuit_breaker: CircuitBreakerConfig, // Uses TAS-34's CircuitBreakerConfig
    pub resource_validation: ResourceValidationConfig, // TAS-34 integration
}

/// Resource validation configuration (TAS-34 integration)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceValidationConfig {
    pub enforce_limits: bool,
    pub max_connection_percentage: f32, // Max % of DB connections this service can use
    pub memory_per_queue_mb: u64,       // Memory estimate per queue
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConnectionConfig {
    Pgmq(PgmqConnectionConfig),
    RabbitMq(RabbitMqConnectionConfig),
    Sqs(SqsConnectionConfig),
    Redis(RedisConnectionConfig),
    Kafka(KafkaConnectionConfig),
}

/// Integration with TAS-34 resource limits
impl ConnectionConfig {
    /// Calculate connection pool requirements for resource validation
    pub fn required_connections(&self) -> u32 {
        match self {
            ConnectionConfig::Pgmq(c) => c.pool_size,
            ConnectionConfig::RabbitMq(_) => 1, // Single channel multiplexed
            ConnectionConfig::Sqs(_) => 10,     // HTTP connection pool
            ConnectionConfig::Redis(c) => c.pool_size,
            ConnectionConfig::Kafka(_) => 5,    // Producer + consumer connections
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgmqConnectionConfig {
    pub database_url: String,
    pub pool_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RabbitMqConnectionConfig {
    pub url: String,
    pub vhost: Option<String>,
    pub connection_timeout_ms: Option<u64>,
    pub heartbeat_interval_secs: Option<u64>,
    pub prefetch_count: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    pub namespace_pattern: String,  // e.g., "{namespace}_queue"
    pub namespaces: Vec<String>,
    pub default_visibility_timeout_secs: u64,
    pub max_receive_count: Option<i32>,
    pub dead_letter_queue_suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: i32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub backoff_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout_ms: u64,
    pub half_open_max_calls: u32,
}
```

## Phase 2: Provider Implementations (Weeks 2-3)

**Dependencies on TAS-34**:
- ProtectedPgmqClient for circuit breaker integration
- ExecutorHealth enum for health state alignment
- ResourceValidator for connection pool validation

### 2.1 PGMQ Implementation

The PGMQ implementation will wrap the existing pgmq client and integrate with TAS-34's ProtectedPgmqClient for circuit breaker support:
Adapt existing PGMQ client to new trait:

```rust
// src/services/messaging/pgmq.rs
use super::{MessagingService, QueueMessage, QueueStats, QueuedMessage, QueuedMessageMetadata};
use async_trait::async_trait;
use pgmq::PGMQueue;
use std::time::Duration;

pub struct PgmqMessageService {
    client: PGMQueue,
    connection_string: String,
}

impl PgmqMessageService {
    pub fn new(connection_string: String) -> Self {
        Self {
            client: PGMQueue::new(connection_string.clone()).expect("Failed to create PGMQ client"),
            connection_string,
        }
    }
}

#[async_trait]
impl MessagingService for PgmqMessageService {
    type Error = PgmqError;

    async fn initialize(&self, resource_limits: &SystemResourceLimits) -> Result<(), Self::Error> {
        // Validate against TAS-34 resource limits
        let required = self.connection_string.parse::<PgmqConnectionConfig>()?.pool_size;
        if required > resource_limits.database_max_connections {
            return Err(PgmqError::ResourceExhaustion(format!(
                "Requires {} connections but only {} available",
                required, resource_limits.database_max_connections
            )));
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, Self::Error> {
        // Check connection by querying pgmq metadata
        match self.client.list_queues().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn create_queue(&self, queue_name: &str) -> Result<(), Self::Error> {
        self.client
            .create(queue_name)
            .await
            .map_err(|e| PgmqError::QueueCreation(e.to_string()))?;
        Ok(())
    }

    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<String, Self::Error> {
        let json_value = message.to_json()
            .map_err(|e| PgmqError::Serialization(e.to_string()))?;

        let msg_id = self.client
            .send(queue_name, &json_value, 0)
            .await
            .map_err(|e| PgmqError::SendMessage(e.to_string()))?;

        Ok(msg_id.to_string())
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, Self::Error> {
        let messages = self.client
            .read::<serde_json::Value>(
                queue_name,
                Some(visibility_timeout.as_secs() as i32),
                Some(max_messages as i32),
            )
            .await
            .map_err(|e| PgmqError::ReceiveMessage(e.to_string()))?;

        let mut result = Vec::new();
        for msg in messages {
            let message = T::from_json(msg.message.clone())
                .map_err(|e| PgmqError::Deserialization(e.to_string()))?;

            let metadata = QueuedMessageMetadata {
                queue_id: msg.msg_id.to_string(),
                enqueued_at: msg.enqueued_at,
                receive_count: msg.read_ct,
                visibility_timeout_expires: Some(msg.vt),
                dead_letter_count: None,
            };

            result.push(QueuedMessage { metadata, message });
        }

        Ok(result)
    }

    // ... implement remaining methods
}

#[derive(Debug, thiserror::Error)]
pub enum PgmqError {
    #[error("Failed to create queue: {0}")]
    QueueCreation(String),
    #[error("Failed to send message: {0}")]
    SendMessage(String),
    #[error("Failed to receive message: {0}")]
    ReceiveMessage(String),
    #[error("Failed to serialize message: {0}")]
    Serialization(String),
    #[error("Failed to deserialize message: {0}")]
    Deserialization(String),
    #[error("Resource exhaustion: {0}")]
    ResourceExhaustion(String),
}

/// Health status compatible with TAS-34 ExecutorHealth
#[derive(Debug, Clone)]
pub enum MessagingHealth {
    Healthy { queue_depth: u64, in_flight: u64 },
    Degraded { reason: String },
    Unhealthy { error: String },
}
```

### 2.2 RabbitMQ Implementation

New RabbitMQ provider implementation:

```rust
// src/services/messaging/rabbitmq.rs
use super::{MessagingService, QueueMessage, QueueStats, QueuedMessage, QueuedMessageMetadata};
use async_trait::async_trait;
use lapin::{
    options::*,
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct RabbitMqMessageService {
    connection: Option<Connection>,
    channel: Option<Channel>,
    config: RabbitMqConfig,
    // Track message receipts for acknowledgment
    receipt_handles: Arc<RwLock<HashMap<String, MessageReceipt>>>,
}

struct MessageReceipt {
    delivery_tag: u64,
    queue_name: String,
}

impl RabbitMqMessageService {
    pub fn new(config: RabbitMqConnectionConfig) -> Self {
        Self {
            connection: None,
            channel: None,
            config,
            receipt_handles: Arc::new(Mutex::new(HashMap::new())),
            consumers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn ensure_channel(&mut self) -> Result<&Channel, RabbitMqError> {
        if self.channel.is_none() {
            self.initialize().await?;
        }
        self.channel.as_ref()
            .ok_or_else(|| RabbitMqError::NotConnected)
    }
}

#[async_trait]
impl MessagingService for RabbitMqMessageService {
    type Error = RabbitMqError;

    async fn initialize(&mut self, resource_limits: Option<&SystemResourceLimits>) -> Result<(), Self::Error> {
        // Validate resources if limits provided (TAS-34 integration)
        if let Some(limits) = resource_limits {
            // RabbitMQ uses minimal database connections (just for state)
            // But needs to validate memory for channel buffers
            let required_memory_mb = self.config.prefetch_count as u64 * 10; // Estimate
            if required_memory_mb > limits.available_memory_mb / 10 {
                return Err(RabbitMqError::ResourceExhaustion(format!(
                    "Requires {}MB but system constrained",
                    required_memory_mb
                )));
            }
        }
        let uri = format!(
            "amqp://{}:{}@{}:{}/{}",
            self.config.username,
            self.config.password,
            self.config.host,
            self.config.port,
            self.config.vhost
        );

        let connection = Connection::connect(
            &uri,
            ConnectionProperties::default()
                .with_connection_name("tasker-rabbitmq".into())
        )
        .await
        .map_err(|e| RabbitMqError::Connection(e.to_string()))?;

        let channel = connection.create_channel().await
            .map_err(|e| RabbitMqError::Channel(e.to_string()))?;

        // Set QoS for fair dispatch
        channel
            .basic_qos(self.config.prefetch_count.unwrap_or(10), BasicQosOptions::default())
            .await
            .map_err(|e| RabbitMqError::Channel(e.to_string()))?;

        self.connection = Some(connection);
        self.channel = Some(channel);

        Ok(())
    }

    async fn create_queue(&self, queue_name: &str) -> Result<(), Self::Error> {
        let channel = self.channel.as_ref()
            .ok_or(RabbitMqError::NotConnected)?;

        channel
            .queue_declare(
                queue_name,
                QueueDeclareOptions {
                    durable: true,
                    auto_delete: false,
                    exclusive: false,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| RabbitMqError::QueueCreation(e.to_string()))?;

        Ok(())
    }

    async fn receive_messages<M: QueueMessage + 'static>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout_secs: Option<u32>,
    ) -> Result<Vec<QueuedMessage<Box<dyn QueueMessage>>>, Self::Error> {
        let channel = self.channel.as_ref()
            .ok_or(RabbitMqError::NotConnected)?;

        let json_value = message.to_json()
            .map_err(|e| RabbitMqError::Serialization(e.to_string()))?;

        let payload = serde_json::to_vec(&json_value)
            .map_err(|e| RabbitMqError::Serialization(e.to_string()))?;

        let props = BasicProperties::default()
            .with_message_id(message.message_id().to_string().into())
            .with_correlation_id(message.correlation_id().map(|s| s.to_string().into()))
            .with_delivery_mode(2); // Persistent

        channel
            .basic_publish(
                "",  // exchange
                queue_name,
                BasicPublishOptions::default(),
                &payload,
                props,
            )
            .await
            .map_err(|e| RabbitMqError::SendMessage(e.to_string()))?;

        Ok(message.message_id().to_string())
    }

    async fn receive_messages<T: QueueMessage>(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout: Duration,
    ) -> Result<Vec<QueuedMessage<T>>, Self::Error> {
        let channel = self.channel.as_ref()
            .ok_or(RabbitMqError::NotConnected)?;

        let mut consumer = channel
            .basic_consume(
                queue_name,
                "tasker-consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| RabbitMqError::ReceiveMessage(e.to_string()))?;

        let mut messages = Vec::new();

        for _ in 0..max_messages {
            if let Ok(Some(delivery)) = tokio::time::timeout(
                Duration::from_millis(100),
                consumer.next()
            ).await {
                let (_, delivery) = delivery.expect("error in consumer");

                let message = T::from_json(
                    serde_json::from_slice(&delivery.data)
                        .map_err(|e| RabbitMqError::Deserialization(e.to_string()))?
                ).map_err(|e| RabbitMqError::Deserialization(e.to_string()))?;

                let receipt_handle = format!("{}", delivery.delivery_tag);
                let mut receipt_handles = self.receipt_handles.write().await;
                receipt_handles.insert(
                    receipt_handle.clone(),
                    MessageReceipt {
                        delivery_tag: delivery.delivery_tag,
                        queue_name: queue_name.to_string(),
                    }
                );

                let metadata = QueuedMessageMetadata {
                    queue_id: receipt_handle,
                    enqueued_at: chrono::Utc::now(),
                    receive_count: if delivery.redelivered { 2 } else { 1 },
                    visibility_timeout_expires: Some(chrono::Utc::now() + chrono::Duration::from_std(visibility_timeout).unwrap()),
                    dead_letter_count: None,
                };

                messages.push(QueuedMessage { metadata, message });
            } else {
                break;
            }
        }

        Ok(messages)
    }

    async fn delete_message(
        &self,
        queue_name: &str,
        receipt_handle: &str,
    ) -> Result<(), Self::Error> {
        let channel = self.channel.as_ref()
            .ok_or(RabbitMqError::NotConnected)?;

        let mut receipt_handles = self.receipt_handles.write().await;
        if let Some(receipt) = receipt_handles.remove(receipt_handle) {
            channel
                .basic_ack(receipt.delivery_tag, BasicAckOptions::default())
                .await
                .map_err(|e| RabbitMqError::DeleteMessage(e.to_string()))?;
        }

        Ok(())
    }

    // ... implement remaining methods
}

#[derive(Debug, thiserror::Error)]
pub enum RabbitMqError {
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Channel error: {0}")]
    Channel(String),
    #[error("Not connected")]
    NotConnected,
    #[error("Queue creation failed: {0}")]
    QueueCreation(String),
    #[error("Failed to send message: {0}")]
    SendMessage(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Deserialization error: {0}")]
    Deserialization(String),
    #[error("Failed to receive message: {0}")]
    ReceiveMessage(String),
    #[error("Failed to delete message: {0}")]
    DeleteMessage(String),
    #[error("Resource exhaustion: {0}")]
    ResourceExhaustion(String),
}

/// Stream wrapper for RabbitMQ consumer
pub struct RabbitMqConsumerStream {
    inner: lapin::Consumer,
    queue_name: String,
}

impl RabbitMqConsumerStream {
    fn new(consumer: lapin::Consumer, queue_name: String) -> Self {
        Self {
            inner: consumer,
            queue_name,
        }
    }
}

impl Stream for RabbitMqConsumerStream {
    type Item = Result<QueuedMessage<Box<dyn QueueMessage>>, RabbitMqError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // Delegate to inner consumer stream
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(delivery))) => {
                // Convert lapin Delivery to QueuedMessage
                let metadata = QueuedMessageMetadata {
                    queue_id: delivery.delivery_tag.to_string(),
                    enqueued_at: chrono::Utc::now(),
                    receive_count: delivery.redelivered as u32,
                    visibility_timeout_expires: None,
                    dead_letter_count: None,
                };

                // Deserialize message
                match serde_json::from_slice(&delivery.data) {
                    Ok(message) => Poll::Ready(Some(Ok(QueuedMessage {
                        metadata,
                        message,
                    }))),
                    Err(e) => Poll::Ready(Some(Err(
                        RabbitMqError::Deserialization(e.to_string())
                    ))),
                }
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Some(Err(RabbitMqError::ReceiveMessage(e.to_string()))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// AMQP extensions implementation for RabbitMQ
#[async_trait]
impl AmqpExtensions for RabbitMqMessageService {
    async fn declare_exchange(
        &self,
        name: &str,
        exchange_type: &str,
        durable: bool,
    ) -> Result<(), Self::Error> {
        let channel = self.ensure_channel().await?;

        let kind = match exchange_type {
            "direct" => lapin::ExchangeKind::Direct,
            "topic" => lapin::ExchangeKind::Topic,
            "fanout" => lapin::ExchangeKind::Fanout,
            "headers" => lapin::ExchangeKind::Headers,
            _ => return Err(RabbitMqError::Channel(
                format!("Unknown exchange type: {}", exchange_type)
            )),
        };

        channel
            .exchange_declare(
                name,
                kind,
                lapin::options::ExchangeDeclareOptions {
                    durable,
                    ..Default::default()
                },
                lapin::types::FieldTable::default(),
            )
            .await
            .map_err(|e| RabbitMqError::Channel(e.to_string()))?;

        Ok(())
    }

    async fn bind_queue(
        &self,
        queue: &str,
        exchange: &str,
        routing_key: &str,
    ) -> Result<(), Self::Error> {
        let channel = self.ensure_channel().await?;

        channel
            .queue_bind(
                queue,
                exchange,
                routing_key,
                lapin::options::QueueBindOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await
            .map_err(|e| RabbitMqError::Channel(e.to_string()))?;

        Ok(())
    }

    async fn publish_to_exchange<M: QueueMessage>(
        &self,
        exchange: &str,
        routing_key: &str,
        message: M,
    ) -> Result<String, Self::Error> {
        let channel = self.ensure_channel().await?;

        let payload = message.to_json()
            .map_err(|e| RabbitMqError::Serialization(e.to_string()))?;

        let message_id = format!("{}_{}", routing_key, uuid::Uuid::new_v4());

        let confirm = channel
            .basic_publish(
                exchange,
                routing_key,
                lapin::options::BasicPublishOptions::default(),
                payload.as_bytes(),
                lapin::BasicProperties::default()
                    .with_message_id(message_id.clone().into())
                    .with_correlation_id(message.correlation_id().into())
                    .with_delivery_mode(2),
            )
            .await
            .map_err(|e| RabbitMqError::SendMessage(e.to_string()))?
            .await
            .map_err(|e| RabbitMqError::SendMessage(
                format!("Confirmation failed: {}", e)
            ))?;

        tracing::debug!("Exchange message sent with confirmation: {:?}", confirm);

        Ok(message_id)
    }

    async fn set_qos(&self, prefetch_count: u16) -> Result<(), Self::Error> {
        let channel = self.ensure_channel().await?;

        channel
            .basic_qos(prefetch_count, lapin::options::BasicQosOptions::default())
            .await
            .map_err(|e| RabbitMqError::Channel(e.to_string()))?;

        Ok(())
    }
}

### 2.3 RabbitMQ Configuration

#### TOML Configuration File: `config/tasker/base/rabbit_mq.toml`

```toml
# RabbitMQ Configuration for Tasker Core
# This file defines the base RabbitMQ/AMQP messaging configuration

[rabbit_mq]
# Connection settings
url = "amqp://localhost:5672"
vhost = "/"
username = "guest"  # Override in environment-specific config
password = "guest"  # Override in environment-specific config

# Connection pool settings
[rabbit_mq.connection_pool]
max_connections = 10
min_connections = 2
connection_timeout_ms = 5000
heartbeat_interval_secs = 60
automatic_recovery = true
network_recovery_interval_ms = 5000

# Channel settings
[rabbit_mq.channels]
max_channels_per_connection = 100
channel_max = 2047  # 0 means no limit

# Consumer settings
[rabbit_mq.consumer]
prefetch_count = 10
consumer_tag_prefix = "tasker_"
no_ack = false  # Always use manual acknowledgment
exclusive = false
no_local = false  # Don't receive messages published by same connection

# Publisher settings
[rabbit_mq.publisher]
confirm_mode = true  # Wait for broker confirmations
mandatory = true  # Return message if can't route
immediate = false  # Deprecated in AMQP 0-9-1
persistent = true  # Messages survive broker restart

# Queue defaults
[rabbit_mq.queue_defaults]
durable = true
auto_delete = false
exclusive = false
max_length = 100000  # Maximum number of messages
max_length_bytes = 1073741824  # 1GB
message_ttl_ms = 86400000  # 24 hours
overflow = "drop-head"  # or "reject-publish"
dead_letter_exchange = "tasker_dlx"
dead_letter_routing_key_prefix = "dlq."

# Exchange configuration
[[rabbit_mq.exchanges]]
name = "tasker_direct"
type = "direct"
durable = true
auto_delete = false
internal = false

[[rabbit_mq.exchanges]]
name = "tasker_events"
type = "topic"
durable = true
auto_delete = false
internal = false

[[rabbit_mq.exchanges]]
name = "tasker_dlx"
type = "direct"
durable = true
auto_delete = false
internal = false
description = "Dead letter exchange for failed messages"

# Queue bindings
[[rabbit_mq.bindings]]
queue = "fulfillment_queue"
exchange = "tasker_direct"
routing_key = "fulfillment"

[[rabbit_mq.bindings]]
queue = "inventory_queue"
exchange = "tasker_direct"
routing_key = "inventory"

[[rabbit_mq.bindings]]
queue = "notifications_queue"
exchange = "tasker_direct"
routing_key = "notifications"

# Topic bindings for event-driven patterns
[[rabbit_mq.bindings]]
queue = "order_events"
exchange = "tasker_events"
routing_key = "order.*"

[[rabbit_mq.bindings]]
queue = "inventory_events"
exchange = "tasker_events"
routing_key = "inventory.*"

# Dead letter queue bindings
[[rabbit_mq.bindings]]
queue = "fulfillment_dlq"
exchange = "tasker_dlx"
routing_key = "dlq.fulfillment"

[[rabbit_mq.bindings]]
queue = "inventory_dlq"
exchange = "tasker_dlx"
routing_key = "dlq.inventory"

# Retry policy
[rabbit_mq.retry]
max_retries = 3
initial_backoff_ms = 1000
max_backoff_ms = 30000
backoff_multiplier = 2.0
jitter = true  # Add randomization to prevent thundering herd

# Circuit breaker (aligned with TAS-34)
[rabbit_mq.circuit_breaker]
enabled = true
failure_threshold = 5
success_threshold = 2
timeout_ms = 30000
half_open_max_calls = 3
reset_timeout_ms = 60000

# Monitoring and metrics
[rabbit_mq.monitoring]
enable_metrics = true
metrics_collection_interval_secs = 10
log_level = "info"  # debug, info, warn, error
trace_messages = false  # Enable for debugging only

# Resource limits (aligned with TAS-34 resource management)
[rabbit_mq.resources]
max_memory_bytes = 536870912  # 512MB
max_message_size_bytes = 10485760  # 10MB
connection_memory_limit_bytes = 104857600  # 100MB per connection
```

#### Environment-Specific Override: `config/tasker/environments/production/rabbit_mq.toml`

```toml
# Production overrides for RabbitMQ configuration

[rabbit_mq]
url = "${RABBITMQ_URL}"  # From environment variable
username = "${RABBITMQ_USERNAME}"
password = "${RABBITMQ_PASSWORD}"
vhost = "${RABBITMQ_VHOST:-/tasker}"

[rabbit_mq.connection_pool]
max_connections = 50
min_connections = 10

[rabbit_mq.consumer]
prefetch_count = 50  # Higher throughput in production

[rabbit_mq.queue_defaults]
max_length = 1000000  # Higher limits in production
message_ttl_ms = 259200000  # 72 hours

[rabbit_mq.monitoring]
log_level = "warn"
trace_messages = false
```

### 2.4 Configuration System Updates

To support RabbitMQ configuration, the following updates are needed to the configuration system:

#### Update to `src/config/messaging.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MessagingConfig {
    #[serde(flatten)]
    pub provider_config: ProviderConfig,
    pub retry: RetryConfig,
    pub circuit_breaker: CircuitBreakerConfig,
    pub resources: ResourceConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ProviderConfig {
    Pgmq(PgmqConfig),
    RabbitMq(RabbitMqConfig),
    // Future: Sqs, Kafka, etc.
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RabbitMqConfig {
    pub url: String,
    pub vhost: String,
    pub username: String,
    pub password: String,
    pub connection_pool: ConnectionPoolConfig,
    pub channels: ChannelConfig,
    pub consumer: ConsumerConfig,
    pub publisher: PublisherConfig,
    pub queue_defaults: QueueDefaultsConfig,
    pub exchanges: Vec<ExchangeConfig>,
    pub bindings: Vec<BindingConfig>,
    pub retry: RetryConfig,
    pub circuit_breaker: CircuitBreakerConfig,
    pub monitoring: MonitoringConfig,
    pub resources: ResourceConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionPoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout_ms: u64,
    pub heartbeat_interval_secs: u64,
    pub automatic_recovery: bool,
    pub network_recovery_interval_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelConfig {
    pub max_channels_per_connection: u32,
    pub channel_max: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConsumerConfig {
    pub prefetch_count: u16,
    pub consumer_tag_prefix: String,
    pub no_ack: bool,
    pub exclusive: bool,
    pub no_local: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PublisherConfig {
    pub confirm_mode: bool,
    pub mandatory: bool,
    pub immediate: bool,
    pub persistent: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueDefaultsConfig {
    pub durable: bool,
    pub auto_delete: bool,
    pub exclusive: bool,
    pub max_length: Option<i64>,
    pub max_length_bytes: Option<i64>,
    pub message_ttl_ms: Option<i64>,
    pub overflow: String,
    pub dead_letter_exchange: String,
    pub dead_letter_routing_key_prefix: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExchangeConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub exchange_type: String,
    pub durable: bool,
    pub auto_delete: bool,
    pub internal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BindingConfig {
    pub queue: String,
    pub exchange: String,
    pub routing_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringConfig {
    pub enable_metrics: bool,
    pub metrics_collection_interval_secs: u64,
    pub log_level: String,
    pub trace_messages: bool,
}
```

#### Update to `ComponentConfigLoader` to handle messaging config:

```rust
impl ComponentConfigLoader {
    pub async fn load_messaging_config(
        &self,
        environment: &str,
    ) -> Result<Option<MessagingConfig>, ConfigError> {
        // Check for RabbitMQ config
        if let Some(rabbit_config) = self.load_component_config::<RabbitMqConfig>(
            "rabbit_mq",
            environment,
        ).await? {
            return Ok(Some(MessagingConfig {
                provider_config: ProviderConfig::RabbitMq(rabbit_config),
                // ... map other fields
            }));
        }

        // Check for PGMQ config
        if let Some(pgmq_config) = self.load_component_config::<PgmqConfig>(
            "pgmq",
            environment,
        ).await? {
            return Ok(Some(MessagingConfig {
                provider_config: ProviderConfig::Pgmq(pgmq_config),
                // ... map other fields
            }));
        }

        Ok(None)
    }
}
```

#### Integration with TAS-34 Resource Management:

The RabbitMQ configuration includes resource limits that align with TAS-34's resource management:

1. **Connection Pool Limits**: Ensures RabbitMQ connections don't exceed database connection limits
2. **Memory Limits**: Per-connection and total memory limits prevent OOM conditions
3. **Circuit Breaker**: Aligned with TAS-34's circuit breaker patterns for coordinated degradation
4. **Prefetch Count**: Controls message buffering to prevent memory exhaustion

### 2.5 Messaging Provider Selection

The messaging provider is determined through a hierarchical configuration approach:

#### Provider Detection Logic:

```rust
impl ComponentConfigLoader {
    pub async fn detect_messaging_provider(
        &self,
        environment: &str,
    ) -> Result<MessageServiceProvider, ConfigError> {
        // 1. Check environment variable override
        if let Ok(provider) = std::env::var("TASKER_MESSAGING_PROVIDER") {
            return match provider.to_lowercase().as_str() {
                "rabbitmq" | "rabbit_mq" | "amqp" => Ok(MessageServiceProvider::RabbitMq),
                "pgmq" | "postgres" => Ok(MessageServiceProvider::Pgmq),
                "sqs" => Ok(MessageServiceProvider::Sqs),
                "redis" => Ok(MessageServiceProvider::Redis),
                "kafka" => Ok(MessageServiceProvider::Kafka),
                _ => Err(ConfigError::InvalidProvider(provider)),
            };
        }

        // 2. Check for provider-specific config files
        let config_path = self.config_dir.join("tasker");

        // Check base configs
        if config_path.join("base/rabbit_mq.toml").exists() {
            return Ok(MessageServiceProvider::RabbitMq);
        }

        if config_path.join("base/pgmq.toml").exists() {
            return Ok(MessageServiceProvider::Pgmq);
        }

        // 3. Check environment-specific overrides
        let env_path = config_path.join(format!("environments/{}", environment));
        if env_path.join("rabbit_mq.toml").exists() {
            return Ok(MessageServiceProvider::RabbitMq);
        }

        if env_path.join("pgmq.toml").exists() {
            return Ok(MessageServiceProvider::Pgmq);
        }

        // 4. Default to PGMQ (current implementation)
        Ok(MessageServiceProvider::Pgmq)
    }

    pub async fn load_messaging_service(
        &self,
        environment: &str,
    ) -> Result<Box<dyn MessagingService>, ConfigError> {
        let provider = self.detect_messaging_provider(environment).await?;

        match provider {
            MessageServiceProvider::RabbitMq => {
                let config = self.load_component_config::<RabbitMqConfig>(
                    "rabbit_mq",
                    environment,
                ).await?
                .ok_or(ConfigError::MissingConfig("rabbit_mq"))?;

                let service = RabbitMqMessageService::new(config);
                service.initialize().await
                    .map_err(|e| ConfigError::ServiceInit(e.to_string()))?;
                Ok(Box::new(service))
            },
            MessageServiceProvider::Pgmq => {
                let config = self.load_component_config::<PgmqConfig>(
                    "pgmq",
                    environment,
                ).await?
                .ok_or(ConfigError::MissingConfig("pgmq"))?;

                let service = PgmqMessageService::new(config);
                service.initialize().await
                    .map_err(|e| ConfigError::ServiceInit(e.to_string()))?;
                Ok(Box::new(service))
            },
            _ => Err(ConfigError::UnsupportedProvider(format!("{:?}", provider))),
        }
    }
}
```

#### Migration from Existing PGMQ Configuration:

For smooth migration, the system supports both configurations simultaneously:

```toml
# config/tasker/base/messaging.toml - Unified messaging config
[messaging]
# Primary provider selection
provider = "pgmq"  # or "rabbitmq"

# Fallback configuration for high availability
[messaging.fallback]
enabled = true
provider = "pgmq"
failover_timeout_ms = 5000

# Provider-specific configs are loaded from their respective files:
# - config/tasker/base/pgmq.toml
# - config/tasker/base/rabbit_mq.toml
```

#### Queue Name Mapping for Migration:

To support gradual migration from PGMQ to RabbitMQ:

```rust
pub struct QueueNameMapper {
    mappings: HashMap<String, QueueMapping>,
}

pub struct QueueMapping {
    pub pgmq_name: String,
    pub rabbitmq_exchange: String,
    pub rabbitmq_routing_key: String,
    pub rabbitmq_queue: String,
}

impl QueueNameMapper {
    pub fn new() -> Self {
        let mut mappings = HashMap::new();

        // Define standard mappings
        mappings.insert("fulfillment_queue".to_string(), QueueMapping {
            pgmq_name: "fulfillment_queue".to_string(),
            rabbitmq_exchange: "tasker_direct".to_string(),
            rabbitmq_routing_key: "fulfillment".to_string(),
            rabbitmq_queue: "fulfillment_queue".to_string(),
        });

        mappings.insert("inventory_queue".to_string(), QueueMapping {
            pgmq_name: "inventory_queue".to_string(),
            rabbitmq_exchange: "tasker_direct".to_string(),
            rabbitmq_routing_key: "inventory".to_string(),
            rabbitmq_queue: "inventory_queue".to_string(),
        });

        Self { mappings }
    }

    pub fn map_for_provider(
        &self,
        logical_name: &str,
        provider: &MessageServiceProvider,
    ) -> QueueDestination {
        let mapping = self.mappings.get(logical_name)
            .unwrap_or_else(|| self.default_mapping(logical_name));

        match provider {
            MessageServiceProvider::Pgmq => {
                QueueDestination::Simple(mapping.pgmq_name.clone())
            },
            MessageServiceProvider::RabbitMq => {
                QueueDestination::Routed {
                    exchange: mapping.rabbitmq_exchange.clone(),
                    routing_key: mapping.rabbitmq_routing_key.clone(),
                    queue: mapping.rabbitmq_queue.clone(),
                }
            },
            _ => QueueDestination::Simple(logical_name.to_string()),
        }
    }
}
```

#### Environment Variable Configuration:

Support for environment-based configuration without changing code:

```bash
# Development - use PGMQ
export TASKER_MESSAGING_PROVIDER=pgmq
export PGMQ_DATABASE_URL=postgresql://localhost/tasker_dev

# Staging - use RabbitMQ
export TASKER_MESSAGING_PROVIDER=rabbitmq
export RABBITMQ_URL=amqp://staging.rabbitmq.local:5672
export RABBITMQ_USERNAME=tasker
export RABBITMQ_PASSWORD=secure_password

# Production - use RabbitMQ with PGMQ fallback
export TASKER_MESSAGING_PROVIDER=rabbitmq
export TASKER_MESSAGING_FALLBACK=pgmq
export RABBITMQ_URL=amqp://prod.rabbitmq.cluster:5672
export PGMQ_DATABASE_URL=postgresql://prod.db.cluster/tasker
```

```

### 2.6 Integration with TAS-34 Component Configuration System

The messaging configuration follows the component-based configuration pattern established in TAS-34:

#### Component File Structure:
```
config/tasker/
├── base/
│   ├── pgmq.toml                 # PGMQ base configuration
│   ├── rabbit_mq.toml             # RabbitMQ base configuration
│   └── messaging.toml             # Provider selection and common settings
├── environments/
│   ├── development/
│   │   ├── pgmq.toml             # Dev PGMQ overrides
│   │   └── rabbit_mq.toml        # Dev RabbitMQ overrides
│   ├── staging/
│   │   ├── pgmq.toml             # Staging PGMQ overrides
│   │   └── rabbit_mq.toml        # Staging RabbitMQ overrides
│   └── production/
│       ├── pgmq.toml             # Production PGMQ overrides
│       └── rabbit_mq.toml        # Production RabbitMQ overrides
```

#### Loading Order (following TAS-34 pattern):
1. Load base configuration from `config/tasker/base/{provider}.toml`
2. Check for environment-specific overrides in `config/tasker/environments/{env}/{provider}.toml`
3. Merge configurations with environment overrides taking precedence
4. Apply environment variable substitutions (e.g., `${RABBITMQ_URL}`)
5. Validate against schema and resource constraints

#### Resource Validation Integration:
The messaging service configuration participates in TAS-34's global resource validation:

```rust
impl ResourceValidator {
    pub async fn validate_messaging_resources(
        &self,
        messaging_config: &MessagingConfig,
        global_resources: &GlobalResourceLimits,
    ) -> Result<(), ResourceValidationError> {
        match &messaging_config.provider_config {
            ProviderConfig::RabbitMq(config) => {
                // Validate RabbitMQ connections don't exceed limits
                let required_connections = config.connection_pool.max_connections;
                if required_connections > global_resources.max_database_connections / 4 {
                    return Err(ResourceValidationError::ExcessiveConnections {
                        service: "RabbitMQ",
                        requested: required_connections,
                        available: global_resources.max_database_connections / 4,
                    });
                }

                // Validate memory allocation
                let required_memory = config.resources.max_memory_bytes;
                if required_memory > global_resources.max_memory_bytes / 10 {
                    return Err(ResourceValidationError::ExcessiveMemory {
                        service: "RabbitMQ",
                        requested: required_memory,
                        available: global_resources.max_memory_bytes / 10,
                    });
                }
            },
            ProviderConfig::Pgmq(config) => {
                // PGMQ uses existing database connections
                // Validate within database connection pool limits
                self.validate_pgmq_resources(config, global_resources)?;
            },
        }
        Ok(())
    }
}
```

#### Configuration Loader Extension:
```rust
impl ComponentConfigLoader {
    /// Load messaging configuration with provider auto-detection
    pub async fn load_messaging(
        &self,
        environment: &str,
    ) -> Result<MessagingConfig, ConfigError> {
        // First, try to load messaging.toml for provider selection
        let messaging_meta = self.load_component_config::<MessagingMetaConfig>(
            "messaging",
            environment,
        ).await?;

        // Determine provider from meta or auto-detect
        let provider = messaging_meta
            .as_ref()
            .and_then(|m| m.provider.clone())
            .or_else(|| self.detect_provider_from_files(environment))
            .unwrap_or(MessageServiceProvider::Pgmq);

        // Load provider-specific configuration
        let provider_config = match provider {
            MessageServiceProvider::RabbitMq => {
                let config = self.load_component_config::<RabbitMqConfig>(
                    "rabbit_mq",
                    environment,
                ).await?
                .ok_or(ConfigError::MissingComponent("rabbit_mq"))?;
                ProviderConfig::RabbitMq(config)
            },
            MessageServiceProvider::Pgmq => {
                let config = self.load_component_config::<PgmqConfig>(
                    "pgmq",
                    environment,
                ).await?
                .ok_or(ConfigError::MissingComponent("pgmq"))?;
                ProviderConfig::Pgmq(config)
            },
            _ => return Err(ConfigError::UnsupportedProvider(provider)),
        };

        Ok(MessagingConfig {
            provider_config,
            // Common fields from messaging_meta if present
            retry: messaging_meta.as_ref()
                .and_then(|m| m.retry.clone())
                .unwrap_or_default(),
            circuit_breaker: messaging_meta.as_ref()
                .and_then(|m| m.circuit_breaker.clone())
                .unwrap_or_default(),
            resources: messaging_meta.as_ref()
                .and_then(|m| m.resources.clone())
                .unwrap_or_default(),
        })
    }
}
```

## Phase 2.5: TAS-34 Coordinator Integration (Week 3.5)

**Critical Path**: This phase blocks on TAS-34 code review fixes being implemented:
- Memory leak fix in BaseExecutor::start() (weak references)
- Pool scaling race condition resolution
- Health state machine implementation
- Async lock migration from std::sync to tokio::sync

### Integrate with OrchestrationLoopCoordinator

The messaging service must integrate with TAS-34's coordinator for resource management and scaling:

```rust
// src/orchestration/coordinator/messaging_integration.rs

use crate::messaging::{MessagingService, MessagingHealth};
use crate::orchestration::coordinator::OrchestrationLoopCoordinator;

impl OrchestrationLoopCoordinator {
    /// Configure messaging service with resource constraints
    pub async fn configure_messaging_service(
        &mut self,
        service: Arc<dyn MessagingService>,
    ) -> Result<()> {
        // Validate against resource limits
        let limits = self.resource_validator.get_limits();
        service.initialize(Some(&limits)).await?;

        // Register for health monitoring
        self.health_monitor.register_component(
            "messaging_service",
            Box::new(move || {
                let svc = service.clone();
                Box::pin(async move {
                    match svc.health_check().await {
                        Ok(MessagingHealth::Healthy { .. }) => ExecutorHealth::Healthy,
                        Ok(MessagingHealth::Degraded { reason }) => {
                            ExecutorHealth::Degraded { reason }
                        }
                        Ok(MessagingHealth::Unhealthy { error }) => {
                            ExecutorHealth::Unhealthy { error }
                        }
                        Err(e) => ExecutorHealth::Unhealthy {
                            error: format!("Health check failed: {}", e),
                        },
                    }
                })
            }),
        )?;

        Ok(())
    }

    /// Coordinate backpressure between messaging and executors
    pub async fn apply_global_backpressure(&mut self) -> Result<()> {
        let pressure = self.calculate_system_pressure().await?;

        // Apply to messaging service
        if let Some(messaging) = &self.messaging_service {
            messaging.apply_backpressure(pressure).await?;
        }

        // Apply to executor pools (existing TAS-34 logic)
        for pool in self.executor_pools.values_mut() {
            pool.apply_backpressure(pressure).await?;
        }

        Ok(())
    }
}
```

### Update ExecutorPool for Messaging Abstraction

```rust
// src/orchestration/executor/base.rs

impl BaseExecutor {
    /// Use abstracted messaging service instead of direct PGMQ
    pub async fn process_with_messaging(
        &self,
        messaging: Arc<dyn MessagingService>,
    ) -> Result<ProcessBatchResult> {
        let queue_name = self.get_queue_name();

        // Receive messages through abstraction
        let messages = messaging
            .receive_messages(
                &queue_name,
                self.config.batch_size,
                self.config.visibility_timeout_seconds,
            )
            .await?;

        let mut processed = 0;
        let mut failed = 0;

        for message in messages {
            match self.process_message(message).await {
                Ok(_) => {
                    messaging.delete_message(&queue_name, message.metadata.queue_id).await?;
                    processed += 1;
                }
                Err(e) if self.is_retryable(&e) => {
                    // Change visibility for retry
                    messaging
                        .change_message_visibility(
                            &queue_name,
                            message.metadata.queue_id,
                            self.calculate_retry_delay(),
                        )
                        .await?;
                }
                Err(_) => {
                    // Send to DLQ after max retries
                    if message.metadata.receive_count >= self.config.max_retries {
                        messaging
                            .dead_letter_message(&queue_name, message.metadata.queue_id)
                            .await?;
                    }
                    failed += 1;
                }
            }
        }

        Ok(ProcessBatchResult {
            items_processed: processed,
            items_failed: failed,
            items_skipped: 0,
        })
    }
}
```

## Phase 3: Ruby Integration Layer (Week 4)

**Note**: Can proceed in parallel with Phase 2.5 since Ruby layer is largely independent

### 3.1 Ruby Service Interface

Create Ruby-side abstraction matching the Rust traits:

```ruby
# bindings/ruby/lib/tasker_core/services/message_service.rb
module TaskerCore
  module Services
    # Base message service interface
    class MessageService
      attr_reader :provider, :config, :logger

      def initialize(config = {})
        @config = config
        @logger = TaskerCore::Logging::Logger.instance
        @provider = config[:provider] || :pgmq
      end

      # Initialize the service
      def initialize_service
        raise NotImplementedError, "Subclasses must implement initialize_service"
      end

      # Health check
      def healthy?
        raise NotImplementedError, "Subclasses must implement healthy?"
      end

      # Create queue if it doesn't exist
      def create_queue(queue_name)
        raise NotImplementedError, "Subclasses must implement create_queue"
      end

      # Send a message to queue
      def send_message(queue_name, message)
        raise NotImplementedError, "Subclasses must implement send_message"
      end

      # Send batch of messages
      def send_batch(queue_name, messages)
        raise NotImplementedError, "Subclasses must implement send_batch"
      end

      # Receive messages from queue
      def receive_messages(queue_name, max_messages: 10, visibility_timeout: 30)
        raise NotImplementedError, "Subclasses must implement receive_messages"
      end

      # Delete message after processing
      def delete_message(queue_name, receipt_handle)
        raise NotImplementedError, "Subclasses must implement delete_message"
      end

      # Change visibility timeout
      def change_message_visibility(queue_name, receipt_handle, visibility_timeout)
        raise NotImplementedError, "Subclasses must implement change_message_visibility"
      end

      # Get queue statistics
      def queue_stats(queue_name)
        raise NotImplementedError, "Subclasses must implement queue_stats"
      end

      # Purge all messages from queue
      def purge_queue(queue_name)
        raise NotImplementedError, "Subclasses must implement purge_queue"
      end

      # Delete queue
      def delete_queue(queue_name)
        raise NotImplementedError, "Subclasses must implement delete_queue"
      end

      protected

      # Helper to serialize message to JSON
      def serialize_message(message)
        case message
        when TaskerCore::Types::SimpleStepMessage
          message.to_h
        when Hash
          message
        else
          raise ArgumentError, "Unsupported message type: #{message.class}"
        end
      end

      # Helper to deserialize message from JSON
      def deserialize_message(json_data)
        return nil unless json_data

        data = json_data.is_a?(String) ? JSON.parse(json_data) : json_data

        # Determine message type and construct appropriate object
        case data['type']
        when 'simple_step'
          TaskerCore::Types::SimpleStepMessage.from_hash(data)
        else
          # Return as hash if type unknown
          data
        end
      rescue JSON::ParserError => e
        logger.error("Failed to deserialize message: #{e.message}")
        nil
      end
    end

    # Factory for creating message services
    class MessageServiceFactory
      def self.create(config = {})
        provider = config[:provider] || detect_provider

        case provider.to_sym
        when :pgmq
          PgmqMessageService.new(config)
        when :rabbitmq
          RabbitMqMessageService.new(config)
        when :sqs
          SqsMessageService.new(config)
        when :redis
          RedisMessageService.new(config)
        else
          raise ArgumentError, "Unknown message service provider: #{provider}"
        end
      end

      private

      def self.detect_provider
        # Default to PGMQ if available
        :pgmq
      end
    end
  end
end
```

### 3.2 Provider-Specific Ruby Implementations

```ruby
# bindings/ruby/lib/tasker_core/services/pgmq_message_service.rb
module TaskerCore
  module Services
    class PgmqMessageService < MessageService
      def initialize(config = {})
        super
        @pgmq_client = TaskerCore::Messaging::PgmqClient.new
      end

      def initialize_service
        # PGMQ initializes on creation
        true
      end

      def healthy?
        # Try a simple operation to check health
        connection.exec('SELECT 1')
        true
      rescue => e
        logger.error("PGMQ health check failed: #{e.message}")
        false
      end

      def create_queue(queue_name)
        @pgmq_client.create_queue(queue_name)
      end

      def send_message(queue_name, message)
        serialized = serialize_message(message)
        @pgmq_client.send_message(queue_name, serialized)
      end

      def send_batch(queue_name, messages)
        messages.map do |message|
          send_message(queue_name, message)
        end
      end

      def receive_messages(queue_name, max_messages: 10, visibility_timeout: 30)
        raw_messages = @pgmq_client.read_messages(
          queue_name,
          visibility_timeout: visibility_timeout,
          qty: max_messages
        )

        raw_messages.map do |raw_msg|
          {
            receipt_handle: raw_msg['msg_id'].to_s,
            message: deserialize_message(raw_msg['message']),
            metadata: {
              enqueued_at: raw_msg['enqueued_at'],
              receive_count: raw_msg['read_ct'],
              visibility_timeout_expires: raw_msg['vt']
            }
          }
        end
      end

      def delete_message(queue_name, receipt_handle)
        @pgmq_client.delete_message(queue_name, receipt_handle.to_i)
      end

      def change_message_visibility(queue_name, receipt_handle, visibility_timeout)
        # PGMQ doesn't have direct visibility change, would need to implement
        # via delete and re-send with delay
        raise NotImplementedError, "PGMQ doesn't support changing visibility timeout"
      end

      def queue_stats(queue_name)
        @pgmq_client.queue_metrics(queue_name)
      end

      def purge_queue(queue_name)
        @pgmq_client.purge_queue(queue_name)
      end

      def delete_queue(queue_name)
        @pgmq_client.drop_queue(queue_name)
      end
    end
  end
end
```

```ruby
# bindings/ruby/lib/tasker_core/services/rabbitmq_message_service.rb
module TaskerCore
  module Services
    class RabbitMqMessageService < MessageService
      def initialize(config = {})
        super
        @connection = nil
        @channel = nil
        @config = config
        @receipt_handles = {}
      end

      def initialize_service
        require 'bunny'

        @connection = Bunny.new(
          hostname: @config[:hostname] || 'localhost',
          port: @config[:port] || 5672,
          username: @config[:username] || 'guest',
          password: @config[:password] || 'guest',
          vhost: @config[:vhost] || '/',
          heartbeat: @config[:heartbeat] || 60,
          connection_timeout: @config[:connection_timeout] || 5
        )

        @connection.start
        @channel = @connection.create_channel
        @channel.prefetch(@config[:prefetch_count] || 10)

        true
      rescue => e
        logger.error("Failed to initialize RabbitMQ: #{e.message}")
        false
      end

      def healthy?
        @connection&.open? && @channel&.open?
      end

      def create_queue(queue_name)
        @channel.queue(queue_name, durable: true, auto_delete: false)
        true
      end

      def send_message(queue_name, message)
        serialized = serialize_message(message)

        @channel.default_exchange.publish(
          serialized.to_json,
          routing_key: queue_name,
          persistent: true,
          message_id: message[:message_id] || SecureRandom.uuid,
          correlation_id: message[:correlation_id]
        )

        message[:message_id] || SecureRandom.uuid
      end

      def receive_messages(queue_name, max_messages: 10, visibility_timeout: 30)
        queue = @channel.queue(queue_name, durable: true)
        messages = []

        max_messages.times do
          delivery_info, properties, payload = queue.pop(manual_ack: true)
          break unless delivery_info

          receipt_handle = SecureRandom.uuid
          @receipt_handles[receipt_handle] = {
            delivery_tag: delivery_info.delivery_tag,
            channel: @channel
          }

          messages << {
            receipt_handle: receipt_handle,
            message: deserialize_message(payload),
            metadata: {
              enqueued_at: properties.timestamp,
              receive_count: delivery_info.redelivered? ? 2 : 1,
              message_id: properties.message_id,
              correlation_id: properties.correlation_id
            }
          }
        end

        messages
      end

      def delete_message(queue_name, receipt_handle)
        receipt = @receipt_handles[receipt_handle]
        return false unless receipt

        receipt[:channel].ack(receipt[:delivery_tag])
        @receipt_handles.delete(receipt_handle)
        true
      end

      def queue_stats(queue_name)
        queue = @channel.queue(queue_name, durable: true)

        {
          queue_name: queue_name,
          message_count: queue.message_count,
          consumer_count: queue.consumer_count
        }
      end

      def purge_queue(queue_name)
        queue = @channel.queue(queue_name, durable: true)
        count = queue.message_count
        queue.purge
        count
      end

      def delete_queue(queue_name)
        queue = @channel.queue(queue_name, durable: true)
        queue.delete
        true
      end
    end
  end
end
```

## Phase 4: Testing & Migration Strategy (Week 5)

**Integration Points with TAS-34**:
- Validate messaging service works with executor pools under load
- Test resource limit enforcement across messaging + executors
- Verify circuit breaker coordination between services
- Benchmark performance with GlobalBackpressureController active

### 4.2 Provider-Specific Integration Tests

Create comprehensive tests for both providers:

```rust
// tests/services/messaging_service_tests.rs
use tasker_core::services::messaging::{
    MessagingService, PgmqMessageService, RabbitMqMessageService,
    QueueMessage, NamespaceQueueRouter
};

#[tokio::test]
async fn test_provider_compatibility() {
    // Test that both providers implement the same interface
    let providers: Vec<Box<dyn MessagingService<Error = Box<dyn std::error::Error + Send + Sync>>>> = vec![
        Box::new(create_pgmq_service()),
        Box::new(create_rabbitmq_service()),
    ];

    for provider in providers {
        // Test basic operations
        provider.create_queue("test_queue").await.unwrap();

        let message = TestMessage {
            id: "test-123".to_string(),
            data: "test data".to_string(),
        };

        let msg_id = provider.send_message("test_queue", &message).await.unwrap();
        assert!(!msg_id.is_empty());

        let messages = provider.receive_messages::<TestMessage>(
            "test_queue",
            10,
            Duration::from_secs(30)
        ).await.unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message.id, "test-123");

        // Clean up
        provider.delete_message("test_queue", &messages[0].metadata.queue_id).await.unwrap();
        provider.delete_queue("test_queue").await.unwrap();
    }
}

#[tokio::test]
async fn test_message_routing() {
    let router = NamespaceQueueRouter::new("queue");

    assert_eq!(router.route_to_queue("fulfillment", "step"), "fulfillment_queue");
}
```

## Revised Implementation Approach (Post TAS-34 Review)

### AMQP Pattern Analysis from lapin

Based on production lapin examples, key patterns to incorporate:

1. **Connection/Channel Separation**:
   - Single connection per service
   - Multiple channels for concurrent operations
   - Channel recovery on failure

2. **Consumer Models**:
   - Stream-based (async iteration) - preferred for Rust
   - Callback-based (set_delegate) - for event-driven patterns

3. **Message Flow**:
   - Direct queue publishing (simple)
   - Exchange→Routing Key→Queue (AMQP routing)
   - Topic/Fanout patterns for broadcast

4. **Acknowledgment Patterns**:
   - Manual ACK after successful processing
   - NACK with requeue on retriable failures
   - Dead letter on permanent failures

5. **Streaming Consumer Pattern**:
   ```rust
   // lapin stream-based consumer (preferred)
   let mut consumer = channel.basic_consume(queue, tag, options, fields).await?;
   while let Some(delivery) = consumer.next().await {
       let delivery = delivery?;
       // Process message
       delivery.ack(BasicAckOptions::default()).await?;
   }
   ```

   This pattern aligns with our trait's streaming approach:
   - Async iteration over messages
   - Built-in backpressure through stream polling
   - Natural integration with tokio runtime
   - Memory-efficient processing

6. **Double-await Pattern for Confirmations**:
   ```rust
   // First await sends, second await confirms
   channel.basic_publish(exchange, routing_key, options, payload, props)
       .await?  // Send to broker
       .await?; // Wait for broker confirmation
   ```

### Unified Abstraction Strategy

To handle the differences between PGMQ (simple queue) and AMQP (exchange/routing) patterns:

#### 1. **Queue Name Mapping**:
   - **PGMQ**: Direct queue names (e.g., "fulfillment_queue")
   - **AMQP**: Map to routing keys with default exchange
   - **Abstraction**: `destination` parameter maps to queue (PGMQ) or routing key (AMQP)

#### 2. **Consumer Creation**:
   ```rust
   // Unified interface
   let consumer = service.create_consumer("fulfillment_queue").await?;

   // PGMQ: Direct queue polling
   // AMQP: Creates consumer with queue binding
   ```

#### 3. **Message Routing**:
   - **Simple Pattern** (both): Direct queue/routing key
   - **Advanced Pattern** (AMQP only): Use AmqpExtensions trait
   ```rust
   // Simple (works for both)
   service.send_message("notifications_queue", message).await?;

   // Advanced AMQP routing
   if let Some(amqp) = service.as_amqp_extensions() {
       amqp.publish_to_exchange("events", "order.created", message).await?;
   }
   ```

#### 4. **Streaming Consumers**:
   ```rust
   // Both providers implement Stream trait
   let mut messages = service.receive_messages_stream("queue").await?;
   while let Some(msg) = messages.next().await {
       process_message(msg?).await?;
       service.ack(&msg).await?;
   }
   ```

#### 5. **Provider Detection**:
   ```rust
   match config.provider {
       MessageServiceProvider::Pgmq => {
           // Use PgmqMessageService with PgmqExtensions
       },
       MessageServiceProvider::RabbitMq => {
           // Use RabbitMqMessageService with AmqpExtensions
       },
   }
   ```

#### 6. **Graceful Degradation**:
   - Core features work across all providers
   - Advanced features (exchanges, topics) available via extension traits
   - Runtime detection of capabilities
   - Clear error messages for unsupported operations

### Key Integration Points with TAS-34

Based on the TAS-34 code review findings, the messaging service abstraction must be designed with the following considerations:

#### 1. Resource Management Integration
- **Connection Pool Validation**: Must respect SystemResourceLimits from TAS-34
- **Memory Budget Allocation**: Share memory budget with executor pools
- **Dynamic Resource Adjustment**: Support runtime resource reallocation based on system pressure

#### 2. Health Monitoring Alignment
- **State Machine Compliance**: Health states must follow TAS-34's ExecutorHealth state machine
- **Transition Validation**: Enforce valid state transitions (Starting → Healthy → Degraded → Unhealthy)
- **Health Aggregation**: Contribute to global health metrics in OrchestrationLoopCoordinator

#### 3. Circuit Breaker Coordination
- **Unified Configuration**: Use TAS-34's CircuitBreakerConfig structure
- **State Synchronization**: Share circuit breaker state with CircuitBreakerManager
- **Cascade Prevention**: Implement backoff strategies to prevent circuit breaker cascades

#### 4. Backpressure Management
- **Global Coordination**: Integrate with GlobalBackpressureController (when implemented)
- **Hysteresis Application**: Use TAS-34's hysteresis patterns to prevent oscillation
- **Weighted Adjustment**: Apply executor-type-specific backpressure weights

#### 5. Memory Leak Prevention
- **Weak References**: Apply TAS-34's weak reference pattern for long-lived handlers
- **Lifecycle Management**: Proper cleanup of message buffers and connections
- **Background Task Tracking**: Track and cleanup orphaned background tasks

### Implementation Priority Order

1. **Phase 0 (Prerequisite)**: Complete TAS-34 critical fixes
   - Fix memory leak in BaseExecutor::start()
   - Resolve pool scaling race condition
   - Implement health state machine
   - Migrate to async locks

2. **Phase 1**: Core abstractions with resource awareness
   - Design traits with resource validation hooks
   - Include health monitoring interfaces
   - Add circuit breaker integration points

3. **Phase 2**: PGMQ wrapper implementation
   - Wrap existing PgmqClient
   - Add ProtectedPgmqClient integration
   - Implement resource validation

4. **Phase 2.5**: Coordinator integration
   - Wire into OrchestrationLoopCoordinator
   - Implement health aggregation
   - Add backpressure coordination

5. **Phase 3**: Alternative providers (RabbitMQ, SQS)
   - Implement with resource constraints
   - Add provider-specific health checks
   - Test scaling behavior

6. **Phase 4**: Ruby integration
   - Update bindings for new abstraction
   - Maintain backward compatibility
   - Add provider detection

### Migration Path from Current PGMQ Implementation

1. **Phase 1**: Extract current PGMQ code into PgmqMessageService
   - Keep existing functionality intact
   - Wrap with MessagingService trait

2. **Phase 2**: Add RabbitMQ implementation alongside
   - Implement core MessagingService trait
   - Add AmqpExtensions for advanced features

3. **Phase 3**: Update orchestration to use abstraction
   - Replace direct PGMQ calls with service interface
   - Add configuration for provider selection

4. **Phase 4**: Enable runtime switching
   - Configuration-driven provider selection
   - Graceful fallback mechanisms

### Critical Success Factors

1. **Zero Performance Regression**: Abstraction layer must not degrade PGMQ performance
2. **Resource Safety**: Never exceed database connection limits or memory constraints
3. **Health Visibility**: All messaging health states visible in coordinator metrics
4. **Graceful Degradation**: System continues operating when messaging degrades
5. **Provider Transparency**: Switching providers requires only configuration change

### Testing Requirements

1. **Resource Exhaustion Tests**: Validate behavior at resource limits
2. **Health Transition Tests**: Verify valid state machine transitions
3. **Circuit Breaker Tests**: Test circuit breaker coordination
4. **Backpressure Tests**: Validate backpressure doesn't cause oscillation
5. **Memory Leak Tests**: Long-running tests to detect memory leaks
6. **Provider Compatibility**: Each provider passes same test suite

### Risk Mitigation Strategies

1. **Incremental Rollout**: Start with read-only operations before write operations
2. **Shadow Mode**: Run new provider in parallel with PGMQ initially
3. **Fallback Mechanism**: Quick revert to PGMQ if issues detected
4. **Monitoring First**: Deploy comprehensive monitoring before provider switch
5. **Load Testing**: Extensive load testing with TAS-34 executor pools active

### Concrete Usage Examples

#### Example 1: Basic Queue Operations (Works with both PGMQ and RabbitMQ)
```rust
// Initialize service based on config
let service: Box<dyn MessagingService> = match config.provider {
    MessageServiceProvider::Pgmq => {
        Box::new(PgmqMessageService::new(config.pgmq_config))
    },
    MessageServiceProvider::RabbitMq => {
        Box::new(RabbitMqMessageService::new(config.rabbitmq_config))
    },
    _ => panic!("Unsupported provider"),
};

// Initialize and health check
service.initialize().await?;
assert!(service.health_check().await?);

// Create queue (works for both)
service.create_queue("fulfillment_queue").await?;

// Send message (works for both)
let message = StepMessage::new(task_uuid, step_uuid, dependencies);
let message_id = service.send_message("fulfillment_queue", &message).await?;

// Receive and process messages (works for both)
let messages = service.receive_messages("fulfillment_queue", 10, Some(30)).await?;
for msg in messages {
    process_step(msg.message).await?;
    service.delete_message("fulfillment_queue", &msg.metadata.queue_id).await?;
}
```

#### Example 2: Stream-based Consumer (Efficient for both providers)
```rust
// Create streaming consumer
let mut consumer = service.create_consumer_stream("inventory_queue").await?;

// Process messages as they arrive
while let Some(result) = consumer.next().await {
    match result {
        Ok(queued_msg) => {
            // Process the message
            let step_result = handle_inventory_step(&queued_msg.message).await;

            if step_result.is_ok() {
                // Acknowledge successful processing
                service.delete_message("inventory_queue", &queued_msg.metadata.queue_id).await?;
            } else {
                // Change visibility for retry
                service.change_message_visibility(
                    "inventory_queue",
                    &queued_msg.metadata.queue_id,
                    60 // Retry in 60 seconds
                ).await?;
            }
        },
        Err(e) => {
            tracing::error!("Consumer error: {}", e);
            // Implement backoff/reconnection logic
        }
    }
}
```

#### Example 3: Advanced AMQP Routing (RabbitMQ only)
```rust
// Check if provider supports AMQP extensions
if let Some(amqp_service) = service.as_any().downcast_ref::<RabbitMqMessageService>() {
    // Declare topic exchange for event routing
    amqp_service.declare_exchange("workflow_events", "topic", true).await?;

    // Create queues for different event types
    service.create_queue("order_events").await?;
    service.create_queue("inventory_events").await?;

    // Bind queues to exchange with routing patterns
    amqp_service.bind_queue("order_events", "workflow_events", "order.*").await?;
    amqp_service.bind_queue("inventory_events", "workflow_events", "inventory.*").await?;

    // Publish events with routing
    let order_created = OrderCreatedEvent { /* ... */ };
    amqp_service.publish_to_exchange(
        "workflow_events",
        "order.created",
        order_created
    ).await?;

    // This message goes to order_events queue via topic routing
}
```

#### Example 4: Graceful Provider Switching
```rust
/// Factory function that creates appropriate service based on environment
pub async fn create_messaging_service(
    config: &MessagingConfig,
) -> Result<Box<dyn MessagingService>, Box<dyn std::error::Error>> {
    // Try primary provider
    match create_provider(&config.provider, config).await {
        Ok(service) => {
            tracing::info!("Using primary provider: {:?}", config.provider);
            Ok(service)
        },
        Err(e) => {
            tracing::warn!("Primary provider failed: {}", e);

            // Fallback to PGMQ if available
            if let Some(pgmq_config) = &config.fallback_pgmq {
                tracing::info!("Falling back to PGMQ");
                let service = PgmqMessageService::new(pgmq_config.clone());
                service.initialize().await?;
                Ok(Box::new(service))
            } else {
                Err(e)
            }
        }
    }
}

// Usage in orchestration
let messaging = create_messaging_service(&config.messaging).await?;
let orchestrator = OrchestrationLoopCoordinator::new(
    config,
    messaging, // Works with any provider
)?;
```

#### Example 5: Testing with Mock Provider
```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Mock provider for testing without real queues
    struct MockMessageService {
        messages: Arc<Mutex<HashMap<String, Vec<QueuedMessage>>>>,
    }

    #[async_trait]
    impl MessagingService for MockMessageService {
        type Error = MockError;

        async fn send_message<M: QueueMessage>(
            &self,
            queue_name: &str,
            message: M,
        ) -> Result<String, Self::Error> {
            let mut messages = self.messages.lock().await;
            let queue = messages.entry(queue_name.to_string()).or_default();
            let msg_id = uuid::Uuid::new_v4().to_string();
            queue.push(QueuedMessage {
                metadata: QueuedMessageMetadata {
                    queue_id: msg_id.clone(),
                    // ... other fields
                },
                message: Box::new(message),
            });
            Ok(msg_id)
        }

        // ... other trait methods
    }

    #[tokio::test]
    async fn test_workflow_with_mock_messaging() {
        let mock_service = MockMessageService::new();
        let workflow = create_test_workflow(Box::new(mock_service));

        // Test workflow behavior without real queue infrastructure
        workflow.execute().await.unwrap();

        // Verify messages were "sent"
        assert_eq!(mock_service.message_count("fulfillment_queue").await, 3);
    }
}
```

### Conclusion

The integration of lapin patterns shows that our abstraction can successfully bridge PGMQ and AMQP models:
- Core operations (send/receive/ack) map cleanly to both
- Extension traits provide advanced features without complicating the base interface
- Streaming consumers offer efficient, backpressure-aware message processing
- The abstraction maintains the simplicity needed for PGMQ while enabling AMQP's routing capabilities

Key insights from lapin examples:
- Connection/channel separation is crucial for AMQP performance
- Stream-based consumers are the preferred async pattern
- Double-await pattern ensures message delivery confirmation
- Manual acknowledgment provides transaction-like semantics

The messaging service abstraction must be deeply integrated with TAS-34's architecture rather than being a standalone component. This ensures resource safety, health visibility, and coordinated scaling across the entire orchestration system. The implementation should proceed only after TAS-34's critical fixes are complete to avoid building on unstable foundations.

### Infrastructure Checklist

Before implementation begins, ensure:

1. **Local Development Environment**:
   - [ ] Docker Compose file updated with RabbitMQ service
   - [ ] Setup script created for quick initialization
   - [ ] Management UI accessible at http://localhost:15672
   - [ ] Both PGMQ and RabbitMQ can run simultaneously for A/B testing

   **Docker Compose Addition** (`docker-compose.yml`):
   ```yaml
   services:
     # Existing PostgreSQL with PGMQ
     postgres:
       image: postgres:15
       environment:
         POSTGRES_USER: tasker
         POSTGRES_PASSWORD: tasker
         POSTGRES_DB: tasker_rust_test
       ports:
         - "5432:5432"
       volumes:
         - postgres_data:/var/lib/postgresql/data

     # Add RabbitMQ service
     rabbitmq:
       image: rabbitmq:3.12-management-alpine
       container_name: tasker_rabbitmq
       environment:
         RABBITMQ_DEFAULT_USER: tasker
         RABBITMQ_DEFAULT_PASS: tasker_dev
         RABBITMQ_DEFAULT_VHOST: /tasker
       ports:
         - "5672:5672"    # AMQP port
         - "15672:15672"  # Management UI
       volumes:
         - rabbitmq_data:/var/lib/rabbitmq
       healthcheck:
         test: ["CMD", "rabbitmq-diagnostics", "ping"]
         interval: 10s
         timeout: 5s
         retries: 5

   volumes:
     postgres_data:
     rabbitmq_data:
   ```

2. **CI/CD Pipeline**:
   - [ ] GitHub Actions workflow includes RabbitMQ service
   - [ ] Test matrix covers both messaging providers
   - [ ] Health checks ensure services are ready before tests
   - [ ] Cleanup tasks prevent test pollution

**GitHub Actions Service Addition** (.github/workflows/test.yml):

```yaml
jobs:
  test:
    runs-on: ubuntu-latest

    strategy:
      matrix:
        messaging-provider: [pgmq, rabbitmq]

    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_USER: tasker
          POSTGRES_PASSWORD: tasker
          POSTGRES_DB: tasker_rust_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432

      rabbitmq:
        image: rabbitmq:3.12-alpine
        env:
          RABBITMQ_DEFAULT_USER: tasker
          RABBITMQ_DEFAULT_PASS: tasker_ci
          RABBITMQ_DEFAULT_VHOST: /tasker
        options: >-
          --health-cmd "rabbitmq-diagnostics ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5672:5672

    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true

      - name: Install PGMQ extension
        if: matrix.messaging-provider == 'pgmq'
        run: |
          # Install PGMQ extension in PostgreSQL
          cargo install --locked cargo-pgmq
          cargo pgmq install

      - name: Run tests with messaging provider
        env:
          TASKER_MESSAGING_PROVIDER: ${{ matrix.messaging-provider }}
          DATABASE_URL: postgresql://tasker:tasker@localhost/tasker_rust_test
          RABBITMQ_URL: amqp://tasker:tasker_ci@localhost:5672/tasker
        run: |
                  cargo test --all-features -- --test-threads=1
```

## CI Strategy for Dual-Provider Integration Testing

### Overview

The integration test suite in `bindings/ruby/spec/integration/` must validate that both PGMQ and RabbitMQ messaging providers deliver identical workflow execution behavior. This section defines the comprehensive strategy for ensuring provider compatibility without duplicating test code.

### Test Matrix Strategy

#### 1. Environment-Based Provider Selection

The integration tests will use environment variables to determine which messaging provider to use:

```ruby
# spec/support/messaging_provider_helper.rb
module MessagingProviderHelper
  def current_messaging_provider
    ENV.fetch('TASKER_MESSAGING_PROVIDER', 'pgmq').downcase
  end

  def using_pgmq?
    current_messaging_provider == 'pgmq'
  end

  def using_rabbitmq?
    current_messaging_provider == 'rabbitmq'
  end

  def skip_if_provider(provider, reason = nil)
    skip_reason = reason || "Test skipped for #{provider} provider"
    skip skip_reason if current_messaging_provider == provider.to_s
  end

  def provider_specific_config
    case current_messaging_provider
    when 'pgmq'
      {
        connection_string: ENV.fetch('DATABASE_URL'),
        pool_size: 10
      }
    when 'rabbitmq'
      {
        url: ENV.fetch('RABBITMQ_URL', 'amqp://tasker:tasker@localhost:5672/tasker'),
        prefetch_count: 10,
        vhost: '/tasker'
      }
    else
      raise "Unknown messaging provider: #{current_messaging_provider}"
    end
  end
end
```

#### 2. SharedTestLoop Modifications

Update the `SharedTestLoop` helper to be provider-aware:

```ruby
# spec/integration/test_helpers/shared_test_loop.rb
class SharedTestLoop
  include MessagingProviderHelper

  def create_workers(namespace:, num_workers: 2, poll_interval: 0.1)
    # Provider-specific worker creation
    worker_config = {
      namespace: namespace,
      poll_interval: poll_interval,
      provider: current_messaging_provider,
      **provider_specific_config
    }

    num_workers.times do |i|
      @test_workers << TaskerCore::Messaging.create_queue_worker(**worker_config)
    end
  end

  def verify_messaging_setup
    case current_messaging_provider
    when 'pgmq'
      # Verify PGMQ queues exist
      pgmq_client = TaskerCore::Messaging::PgmqClient.new
      expect(pgmq_client.connection).not_to be_nil
    when 'rabbitmq'
      # Verify RabbitMQ connection and exchanges
      service = TaskerCore::Services::MessageServiceFactory.create(
        provider: 'rabbitmq',
        config: provider_specific_config
      )
      expect(service.healthy?).to be true
    end
  end
end
```

#### 3. GitHub Actions Matrix Configuration

```yaml
# .github/workflows/integration-tests.yml
name: Integration Tests

on:
  push:
    branches: [main, 'feature/**']
  pull_request:
    branches: [main]

jobs:
  integration-matrix:
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        messaging-provider: [pgmq, rabbitmq]
        test-suite:
          - linear_workflow
          - diamond_workflow
          - tree_workflow
          - mixed_dag_workflow
          - order_fulfillment

    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_USER: tasker
          POSTGRES_PASSWORD: tasker
          POSTGRES_DB: tasker_rust_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432

      rabbitmq:
        image: rabbitmq:3.12-management-alpine
        env:
          RABBITMQ_DEFAULT_USER: tasker
          RABBITMQ_DEFAULT_PASS: tasker
          RABBITMQ_DEFAULT_VHOST: /tasker
        options: >-
          --health-cmd "rabbitmq-diagnostics ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5672:5672
          - 15672:15672

    steps:
      - uses: actions/checkout@v4

      - name: Setup Ruby
        uses: ruby/setup-ruby@v1
        with:
          ruby-version: '3.2'
          bundler-cache: true

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true

      - name: Install PGMQ Extension
        if: matrix.messaging-provider == 'pgmq'
        run: |
          sudo apt-get update
          sudo apt-get install -y postgresql-client
          PGPASSWORD=tasker psql -h localhost -U tasker -d tasker_rust_test -c "CREATE EXTENSION IF NOT EXISTS pgmq;"

      - name: Setup RabbitMQ Exchanges
        if: matrix.messaging-provider == 'rabbitmq'
        run: |
          # Wait for RabbitMQ to be fully ready
          sleep 10
          # Create required exchanges and queues
          docker exec $(docker ps -q -f ancestor=rabbitmq:3.12-management-alpine) \
            rabbitmqctl eval 'rabbit_exchange:declare({resource, <<"/">>, exchange, <<"tasker.topic">>}, topic, true, false, false, []).'

      - name: Run Migrations
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost/tasker_rust_test
        run: |
          cd bindings/ruby
          bundle exec rake db:migrate

      - name: Compile Ruby Extension
        run: |
          cd bindings/ruby
          bundle exec rake compile

      - name: Run Integration Tests
        env:
          TASKER_MESSAGING_PROVIDER: ${{ matrix.messaging-provider }}
          DATABASE_URL: postgresql://tasker:tasker@localhost/tasker_rust_test
          RABBITMQ_URL: amqp://tasker:tasker@localhost:5672/tasker
          TASKER_ENV: test
        run: |
          cd bindings/ruby
          bundle exec rspec spec/integration/${{ matrix.test-suite }}_integration_spec.rb \
            --format documentation \
            --format RspecJunitFormatter --out results/${{ matrix.messaging-provider }}-${{ matrix.test-suite }}.xml

      - name: Upload Test Results
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: test-results-${{ matrix.messaging-provider }}-${{ matrix.test-suite }}
          path: bindings/ruby/results/*.xml
```

### Test Implementation Patterns

#### 1. Provider-Agnostic Test Structure

All integration tests should remain provider-agnostic:

```ruby
RSpec.describe 'Workflow Integration', type: :integration do
  include MessagingProviderHelper

  before(:suite) do
    # Provider-specific setup happens automatically via environment
    puts "Running tests with #{current_messaging_provider.upcase} provider"
  end

  it 'executes workflow steps in correct order' do
    # Test implementation remains the same regardless of provider
    task_request = TaskerCore::Types::TaskRequest.new(...)
    task = shared_loop.run(task_request: task_request, ...)

    # Assertions work identically for both providers
    expect(task.workflow_steps.count).to eq(4)
  end

  context 'provider-specific behavior' do
    it 'handles high throughput scenarios', :high_throughput do
      skip_if_provider('pgmq', 'PGMQ handles throughput differently')

      # RabbitMQ-specific high throughput test
      # ...
    end
  end
end
```

#### 2. Performance Comparison Metrics

Collect metrics to compare provider performance:

```ruby
# spec/support/performance_collector.rb
class PerformanceCollector
  def self.record_execution(provider:, test_name:, &block)
    start_time = Time.now
    result = yield
    duration = Time.now - start_time

    File.open("tmp/performance_#{provider}.jsonl", "a") do |f|
      f.puts({
        provider: provider,
        test: test_name,
        duration_seconds: duration,
        timestamp: Time.now.iso8601
      }.to_json)
    end

    result
  end
end

# Usage in tests
it 'processes complex workflow' do
  PerformanceCollector.record_execution(
    provider: current_messaging_provider,
    test_name: 'complex_workflow'
  ) do
    task = shared_loop.run(task_request: task_request, ...)
    expect(task).not_to be_nil
  end
end
```

### Local Development Testing

#### Running Tests Against Different Providers Locally

Tests run against PGMQ by default. To test with RabbitMQ, simply set the environment variable:

```bash
# Test with PGMQ (default)
bundle exec rspec spec/integration/

# Test with RabbitMQ
TASKER_MESSAGING_PROVIDER=rabbitmq bundle exec rspec spec/integration/

# Or export for the session
export TASKER_MESSAGING_PROVIDER=rabbitmq
bundle exec rspec spec/integration/
```

The provider configuration is automatically selected based on the `TASKER_MESSAGING_PROVIDER` environment variable:
- `pgmq` (default): Uses PostgreSQL with PGMQ extension
- `rabbitmq`: Uses RabbitMQ with AMQP

Ensure the appropriate service is running:
```bash
# For PGMQ
docker-compose up -d postgres

# For RabbitMQ
docker-compose up -d rabbitmq

# Or both if switching between them
docker-compose up -d postgres rabbitmq
```

### Debugging Provider-Specific Issues

#### 1. Enhanced Logging for Provider Comparison

```ruby
# spec/support/provider_debug_helper.rb
module ProviderDebugHelper
  def log_provider_operation(operation, details = {})
    return unless ENV['DEBUG_PROVIDER']

    log_entry = {
      provider: current_messaging_provider,
      operation: operation,
      timestamp: Time.now.iso8601,
      **details
    }

    Rails.logger.debug "[PROVIDER_DEBUG] #{log_entry.to_json}"
    File.open("tmp/provider_debug.jsonl", "a") { |f| f.puts(log_entry.to_json) }
  end

  def compare_provider_behavior(&block)
    if ENV['COMPARE_PROVIDERS']
      results = {}

      ['pgmq', 'rabbitmq'].each do |provider|
        ENV['TASKER_MESSAGING_PROVIDER'] = provider
        results[provider] = yield
      end

      if results['pgmq'] != results['rabbitmq']
        raise "Provider behavior mismatch: PGMQ=#{results['pgmq']}, RabbitMQ=#{results['rabbitmq']}"
      end
    else
      yield
    end
  end
end
```

#### 2. Provider-Specific Health Checks

```ruby
# spec/support/provider_health_check.rb
class ProviderHealthCheck
  def self.verify_all_providers_healthy
    issues = []

    # Check PGMQ
    begin
      pgmq = TaskerCore::Messaging::PgmqClient.new
      pgmq.create_queue('health_check_queue')
      pgmq.send_message('health_check_queue', {test: true})
      pgmq.delete_queue('health_check_queue')
    rescue => e
      issues << "PGMQ unhealthy: #{e.message}"
    end

    # Check RabbitMQ
    begin
      rabbit = TaskerCore::Services::RabbitMqMessageService.new(
        url: ENV.fetch('RABBITMQ_URL', 'amqp://tasker:tasker@localhost:5672/tasker')
      )
      rabbit.initialize_service
      raise "RabbitMQ not healthy" unless rabbit.healthy?
    rescue => e
      issues << "RabbitMQ unhealthy: #{e.message}"
    end

    unless issues.empty?
      puts "⚠️  Provider Health Issues:"
      issues.each { |issue| puts "  - #{issue}" }
      raise "Not all providers are healthy"
    end

    puts "✅ All messaging providers are healthy"
  end
end
```

### Success Criteria for CI Integration Testing

1. **Provider Parity**: All integration tests must pass for both PGMQ and RabbitMQ providers
2. **Performance Baselines**: Establish acceptable performance differences between providers
3. **Failure Isolation**: Provider-specific failures should be clearly identified in CI output
4. **Parallel Execution**: Tests run in parallel for different providers to reduce CI time
5. **Artifact Collection**: Test results, logs, and performance metrics collected for analysis
6. **Required Pass**: Both providers must pass for CI to succeed - no gradual adoption

### Risk Mitigation

1. **Test Flakiness**: Use retry logic for provider-specific transient failures
2. **Resource Constraints**: Configure appropriate resource limits in CI environment
3. **Provider Coupling**: Ensure tests don't accidentally couple to provider-specific behavior
4. **Maintenance Overhead**: Automated provider comparison to catch divergence early

### Implementation Checklist

1. **Update SharedTestLoop**: Add MessagingProviderHelper module
2. **Configure CI Matrix**: GitHub Actions configuration for both providers
3. **Update Test Helpers**: Make all test helpers provider-aware
4. **Documentation**: Update README with environment variable documentation
5. **Validation**: Ensure all existing integration tests pass with both providers

This comprehensive CI strategy ensures that the messaging abstraction fully supports both PGMQ and RabbitMQ providers from day one, with all tests required to pass for both providers in CI while maintaining simple environment-variable-based switching for local development.

3. **Documentation**:
   - [ ] README updated with RabbitMQ setup instructions
   - [ ] Environment variable documentation complete
   - [ ] Troubleshooting guide for common RabbitMQ issues
   - [ ] Migration guide from PGMQ to RabbitMQ

   **Local Setup Script** (`scripts/setup-messaging.sh`):
   ```bash
   #!/bin/bash
   set -e

   echo "Setting up messaging infrastructure..."

   # Start services
   docker-compose up -d postgres rabbitmq

   # Wait for PostgreSQL
   echo "Waiting for PostgreSQL..."
   until docker-compose exec -T postgres pg_isready -U tasker; do
     sleep 1
   done

   # Install PGMQ extension
   echo "Installing PGMQ extension..."
   docker-compose exec -T postgres psql -U tasker -d tasker_rust_test -c "CREATE EXTENSION IF NOT EXISTS pgmq;"

   # Wait for RabbitMQ
   echo "Waiting for RabbitMQ..."
   until docker-compose exec -T rabbitmq rabbitmq-diagnostics ping; do
     sleep 1
   done

   # Create RabbitMQ vhost and set permissions
   echo "Configuring RabbitMQ..."
   docker-compose exec -T rabbitmq rabbitmqctl add_vhost /tasker || true
   docker-compose exec -T rabbitmq rabbitmqctl set_permissions -p /tasker tasker ".*" ".*" ".*"

   echo "✅ Messaging infrastructure ready!"
   echo "   PostgreSQL: postgresql://tasker:tasker@localhost/tasker_rust_test"
   echo "   RabbitMQ: amqp://tasker:tasker_dev@localhost:5672/tasker"
   echo "   RabbitMQ Management UI: http://localhost:15672 (tasker/tasker_dev)"
   ```

4. **Monitoring & Observability**:
   - [ ] RabbitMQ metrics exposed to monitoring system
   - [ ] Dashboard created for queue depth and consumer lag
   - [ ] Alerts configured for connection failures
   - [ ] Distributed tracing spans for message flow

5. **Testing Strategy**:
   - [ ] Integration tests for both PGMQ and RabbitMQ providers
   - [ ] Performance benchmarks comparing both providers
   - [ ] Failover testing between providers
   - [ ] Load testing with concurrent consumers
   - [ ] Message ordering guarantees verified
   - [ ] Dead letter queue handling tested

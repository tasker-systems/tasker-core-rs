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

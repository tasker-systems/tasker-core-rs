# TAS-35: Message Service Abstraction Implementation Plan

## Executive Summary

This plan details the implementation of a flexible message service abstraction layer for the Tasker workflow orchestration system. The abstraction will enable swapping between different queue providers (starting with PGMQ and RabbitMQ) while preserving the PostgreSQL-backed architecture and simple UUID-based messaging. This approach maintains database integrity while providing flexibility in queue provider selection based on throughput and operational requirements.

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

/// Message routing strategy
#[async_trait]
pub trait MessageRouter: Send + Sync {
    /// Determine which queue a message should be sent to
    fn route_to_queue(&self, namespace: &str, message_type: &str) -> String;
    
    /// Extract namespace from queue name
    fn extract_namespace(&self, queue_name: &str) -> Option<String>;
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
```

### 1.2 Configuration Schema

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
    pub circuit_breaker: Option<CircuitBreakerConfig>,
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

### 2.1 PGMQ Implementation

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
    
    async fn initialize(&mut self) -> Result<(), Self::Error> {
        // PGMQ initializes on creation
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
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Deserialization error: {0}")]
    Deserialization(String),
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
    pub fn new(config: RabbitMqConfig) -> Self {
        Self {
            connection: None,
            channel: None,
            config,
            receipt_handles: Arc::new(RwLock::new(HashMap::new())),
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
    
    async fn initialize(&mut self) -> Result<(), Self::Error> {
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
    
    async fn send_message<T: QueueMessage>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<String, Self::Error> {
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
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Channel error: {0}")]
    Channel(String),
    #[error("Not connected")]
    NotConnected,
    #[error("Failed to create queue: {0}")]
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
}
```

## Phase 3: Ruby Integration Layer (Week 4)

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

### 4.1 Integration Tests

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
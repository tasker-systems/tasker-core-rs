# Service Encapsulation Plan

## Executive Summary

The current Tasker workflow orchestration system has tight coupling to PGMQ message queuing and specific ActiveRecord model access patterns. This proposal outlines a systematic approach to introduce service abstractions that enable alternative queue implementations and flexible model access patterns while preserving the high-performance, UUID-based messaging architecture and PostgreSQL-specific capabilities. **Note: This system will remain PostgreSQL-specific by design to leverage PostgreSQL's advanced features - the abstraction focuses on queue providers and model access patterns, not database providers.**

## Current Architecture Analysis

### Strengths of Current Design
- **Shared Database Backplane**: PostgreSQL as the single source of truth eliminates coordination complexity
- **Simple Message Architecture**: UUID-based messages with database lookups (80% size reduction)
- **Autonomous Workers**: Ruby workers poll queues independently without FFI coupling
- **ActiveRecord Integration**: Handlers work with real ORM objects, not hash conversions

### Identified Coupling Points

#### 1. Queue Management Coupling
**Current State**: Hard-coded PGMQ dependencies in both layers.

**Rust Dependencies**:
- `pgmq-rs` crate used directly in `src/messaging/pgmq_client.rs`
- Queue operations tightly coupled to PostgreSQL message queue extension
- Namespace queue naming pattern hard-coded (`{namespace}_queue`)

**Ruby Dependencies**:
- PGMQ-specific Ruby client in `workers/ruby/lib/tasker_core/messaging/pgmq_client.rb`
- Queue worker polling logic specific to PGMQ visibility timeouts and batching


## Service Encapsulation Strategy

#### 1.1 Messaging Service Abstraction

**Rust Side: Messaging Service Trait**
```rust
// src/services/messaging/mod.rs
pub trait MessagingService: Send + Sync {
    type Message;
    type MessageId;

    async fn create_queue(&self, queue_name: &str) -> Result<()>;
    async fn send_message(&self, queue_name: &str, message: &Self::Message) -> Result<Self::MessageId>;
    async fn receive_messages(&self, queue_name: &str, batch_size: Option<usize>) -> Result<Vec<QueuedMessage<Self::Message>>>;
    async fn delete_message(&self, queue_name: &str, message_id: &Self::MessageId) -> Result<()>;
    async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats>;
}

pub struct QueuedMessage<T> {
    pub id: String,
    pub message: T,
    pub received_count: i32,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

// PGMQ implementation
pub struct PgmqMessageService {
    client: pgmq::PGMQueue,
}

// RabbitMQ implementation
pub struct RabbitMqMessageService {
    client: amqprs::Channel,
}

impl MessagingService for PgmqMessageService {
    type Message = serde_json::Value;
    type MessageId = i64;

    // Implementation details...
}

impl MessagingService for RabbitMqMessageService {
    type Message = serde_json::Value;
    type MessageId = String;

    // Implementation details...
}
```

**Ruby Side: Message Service Interface**
```ruby
# workers/ruby/lib/tasker_core/services/message_service.rb
module TaskerCore
  module Services
    class MessageService
      def create_message(message)
        raise NotImplementedError, "Subclasses must implement create_queue"
      end

      def send_message(queue_name, message)
        raise NotImplementedError, "Subclasses must implement send_message"
      end

      def receive_messages(queue_name, batch_size: 10)
        raise NotImplementedError, "Subclasses must implement receive_messages"
      end

      def delete_message(queue_name, message_id)
        raise NotImplementedError, "Subclasses must implement delete_message"
      end
    end

    # PGMQ implementation
    class PgmqMessageService < MessageService
      def initialize(connection_string = nil)
        @client = TaskerCore::Messaging::PgmqClient.new(connection_string)
      end

      def send_message(queue_name, message)
        @client.send_message(queue_name, message)
      end

      # Other implementations...
    end

    # RabbitMQ implementation

    class RabbitMqMessageService < MessageService
      # implementation details here
    end

    # SQS implementation (future)
    class SqsMessageService < MessageService
      # AWS SQS implementation
    end

    # Kafka implementation (future)
    class KafkaMessageService < MessageService
      # Kafka implementation
    end
  end
end
```

### Phase 2: Implementation Strategy Pattern (Weeks 3-4)

#### 2.1 Configuration-Driven Service Selection

**Rust Configuration Extension**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageServiceConfig {
    pub provider: MessageServiceProvider,
    pub connection_config: MessageServiceConnectionConfig,
    pub namespaces: Vec<String>, // e.g., vec!["{namespace}_queue"]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageServiceProvider {
    PGMQ,
    RabbitMQ,
    SQS,
    Kafka,
    Redis,
}
```

### Phase 3: Message Abstraction Layer (Weeks 5-6)

#### 3.1 Universal Message Interface

**Rust Message Abstraction**:
```rust
// src/services/messaging/mod.rs
pub trait Message: Send + Sync + Clone {
    fn message_type(&self) -> &str;
    fn correlation_id(&self) -> Option<&str>;
    fn to_json(&self) -> Result<serde_json::Value>;
    fn from_json(value: serde_json::Value) -> Result<Self> where Self: Sized;
}

pub trait MessageRouter: Send + Sync {
    fn route_to_queue(&self, message: &dyn Message) -> Result<String>;
    fn extract_namespace(&self, queue_name: &str) -> Result<String>;
}

pub struct NamespaceMessageRouter {
    pattern: String, // e.g., "{namespace}_queue"
}

impl MessageRouter for NamespaceMessageRouter {
    fn route_to_queue(&self, message: &dyn Message) -> Result<String> {
        // Extract namespace from message and apply pattern
        let namespace = self.extract_namespace_from_message(message)?;
        Ok(self.pattern.replace("{namespace}", &namespace))
    }
}
```

**Ruby Message Abstraction**:
```ruby
# workers/ruby/lib/tasker_core/services/message_service.rb
module TaskerCore
  module Services
    class MessageService
      def initialize(queue_service, message_router)
        @queue_service = queue_service
        @message_router = message_router
      end

      def send_message(message)
        queue_name = @message_router.route_to_queue(message)
        @queue_service.send_message(queue_name, message.to_h)
      end

      def receive_messages(namespace, batch_size: 10)
        queue_name = @message_router.queue_for_namespace(namespace)
        raw_messages = @queue_service.receive_messages(queue_name, batch_size: batch_size)

        raw_messages.map do |raw_msg|
          # Convert to appropriate message type based on content
          parse_message(raw_msg)
        end
      end

      private

      def parse_message(raw_message)
        # Strategy pattern for different message types
        case raw_message['type']
        when 'simple_step'
          TaskerCore::Types::SimpleStepMessage.from_hash(raw_message)
        when 'task_request'
          TaskerCore::Types::TaskRequest.from_hash(raw_message)
        else
          raise ArgumentError, "Unknown message type: #{raw_message['type']}"
        end
      end
    end
  end
end
```

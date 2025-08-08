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

#### 1. Database Layer Coupling
**Current State**: Hard-coded PostgreSQL and ActiveRecord dependencies throughout both Rust and Ruby codebases.

**Rust Dependencies**:
- `sqlx::PgPool` used directly in 15+ orchestration components
- PostgreSQL-specific SQL functions in `src/database/sql_functions.rs`
- Hard-coded connection pool management in `src/database/connection.rs`

**Ruby Dependencies**:
- `ActiveRecord::Base` used directly for all model operations
- PostgreSQL-specific connection management in `bindings/ruby/lib/tasker_core/database/connection.rb`
- Direct SQL query execution through ActiveRecord connection

#### 2. Queue Management Coupling
**Current State**: Hard-coded PGMQ dependencies in both layers.

**Rust Dependencies**:
- `pgmq-rs` crate used directly in `src/messaging/pgmq_client.rs`
- Queue operations tightly coupled to PostgreSQL message queue extension
- Namespace queue naming pattern hard-coded (`{namespace}_queue`)

**Ruby Dependencies**:
- PGMQ-specific Ruby client in `bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb`
- Queue worker polling logic specific to PGMQ visibility timeouts and batching

#### 3. Registry System Coupling
**Current State**: Registry operations mixed with database persistence logic.

**Rust Dependencies**:
- `TaskHandlerRegistry` directly uses `sqlx::PgPool` for database operations
- Task template storage hard-coded to PostgreSQL tables

**Ruby Dependencies**:
- `TaskTemplateRegistry` uses ActiveRecord models directly
- Step handler resolution coupled to ActiveRecord query patterns

## Service Encapsulation Strategy

### Phase 1: Core Service Interface Design (Weeks 1-2)

#### 1.1 Model Access Service Abstraction

**Rust Model Access Service Interface** (PostgreSQL-Direct Implementation):
```rust
// src/services/model_access/traits.rs
// Note: We remain PostgreSQL-specific but abstract model access patterns
pub trait ModelAccessService: Send + Sync {
    async fn find_task_by_uuid(&self, uuid: &str) -> Result<Option<TaskModel>>;
    async fn find_step_by_uuid(&self, uuid: &str) -> Result<Option<StepModel>>;
    async fn find_steps_by_uuids(&self, uuids: &[String]) -> Result<Vec<StepModel>>;
    async fn update_step_state(&self, step_uuid: &str, state: StepState) -> Result<()>;
    async fn update_task_state(&self, task_uuid: &str, state: TaskState) -> Result<()>;
    async fn find_ready_steps(&self, task_uuid: &str) -> Result<Vec<StepModel>>;
    async fn health_check(&self) -> Result<bool>;
}

// PostgreSQL-specific implementation (our primary and optimized path)
pub struct PostgreSQLModelAccessService {
    pool: sqlx::Pool<sqlx::Postgres>,
}
```

**Ruby Model Access Service Interface**:
```ruby
# bindings/ruby/lib/tasker_core/services/model_access_service.rb
module TaskerCore
  module Services
    class ModelAccessService
      def find_task_by_uuid(uuid)
        raise NotImplementedError, "Subclasses must implement find_task_by_uuid"
      end
      
      def find_step_by_uuid(uuid)
        raise NotImplementedError, "Subclasses must implement find_step_by_uuid"
      end
      
      def find_steps_by_uuids(uuids)
        raise NotImplementedError, "Subclasses must implement find_steps_by_uuids"
      end
      
      def health_check
        raise NotImplementedError, "Subclasses must implement health_check"
      end
    end
    
    # PostgreSQL ActiveRecord implementation (maintains current functionality)
    class ActiveRecordModelAccessService < ModelAccessService
      def find_task_by_uuid(uuid)
        TaskerCore::Database::Models::Task.find_by(task_uuid: uuid)
      end
      
      def find_step_by_uuid(uuid)
        TaskerCore::Database::Models::WorkflowStep.find_by(step_uuid: uuid)
      end
      
      def find_steps_by_uuids(uuids)
        TaskerCore::Database::Models::WorkflowStep.where(step_uuid: uuids)
      end
      
      def health_check
        ActiveRecord::Base.connection.active?
      end
    end
    
    # HTTP API implementation (for distributed deployments)
    class HttpApiModelAccessService < ModelAccessService
      def initialize(base_url, auth_token)
        @client = TaskerCore::Http::ApiClient.new(base_url, auth_token)
      end
      
      def find_task_by_uuid(uuid)
        response = @client.get("/tasks/#{uuid}")
        TaskStruct.new(response) if response
      end
    end
  end
end
```

#### 1.2 Queue Service Abstraction

**Rust Side: Queue Service Trait**
```rust
// src/services/queue/mod.rs
pub trait QueueService: Send + Sync {
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
pub struct PgmqQueueService {
    client: pgmq::PGMQueue,
}

impl QueueService for PgmqQueueService {
    type Message = serde_json::Value;
    type MessageId = i64;
    
    // Implementation details...
}
```

**Ruby Side: Queue Service Interface**
```ruby
# bindings/ruby/lib/tasker_core/services/queue_service.rb
module TaskerCore
  module Services
    class QueueService
      def create_queue(queue_name)
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
    class PgmqQueueService < QueueService
      def initialize(connection_string = nil)
        @client = TaskerCore::Messaging::PgmqClient.new(connection_string)
      end
      
      def send_message(queue_name, message)
        @client.send_message(queue_name, message)
      end
      
      # Other implementations...
    end
    
    # SQS implementation (future)
    class SqsQueueService < QueueService
      # AWS SQS implementation
    end
    
    # Kafka implementation (future)  
    class KafkaQueueService < QueueService
      # Kafka implementation
    end
  end
end
```

#### 1.3 Registry Service Abstraction

**Rust Side: Registry Service Trait**
```rust
// src/services/registry/mod.rs
pub trait RegistryService: Send + Sync {
    async fn get_task_template(&self, key: &RegistryKey) -> Result<Option<TaskTemplate>>;
    async fn register_task_template(&self, template: &TaskTemplate) -> Result<()>;
    async fn list_templates(&self, namespace: Option<&str>) -> Result<Vec<RegistryKey>>;
    async fn template_exists(&self, key: &RegistryKey) -> Result<bool>;
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RegistryKey {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

// Database-backed implementation
pub struct DatabaseRegistryService {
    database_service: Arc<dyn DatabaseService>,
}

// File-based implementation (future)
pub struct FileRegistryService {
    base_path: PathBuf,
}

// Redis implementation (future)
pub struct RedisRegistryService {
    client: redis::Client,
}
```

### Phase 2: Implementation Strategy Pattern (Weeks 3-4)

#### 2.1 Configuration-Driven Service Selection

**Rust Configuration Extension**:
```rust
// src/config/mod.rs (extend existing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
  pub model_access: ModelAccessServiceConfig,
  pub queue: QueueServiceConfig,
  pub registry: RegistryServiceConfig,
}
  
pub struct ModelAccessServiceConfig {
  pub provider: ModelAccessProvider,
  pub connection_config: ConnectionConfig,
}
  
pub enum ModelAccessProvider {
  PostgreSQLDirect,
  HttpApi,
  GraphQLApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueServiceConfig {
    pub provider: QueueProvider,
    pub connection_config: QueueConnectionConfig,
    pub namespace_pattern: String, // e.g., "{namespace}_queue"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueueProvider {
    PGMQ,
    SQS,
    Kafka,
    Redis,
}
```

**Ruby Configuration Extension**:
```yaml
# config/tasker-config.yaml (extend existing)
# config/tasker-config.yaml
services:
  model_access:
    provider: "postgresql_direct"
    connection_string: "${DATABASE_URL}"
    pool:
      max_connections: 10
      idle_timeout: "30s"
  
  queue:
    provider: "pgmq"
    namespace_pattern: "{namespace}_queue"
    connection:
      database_url: "${DATABASE_URL}"
  
  registry:
    provider: "postgresql"
    cache:
      enabled: true
      ttl_seconds: 300

development:
  services:
    model_access:
      provider: "postgresql_direct"
    queue:
      provider: "pgmq"

production:
  services:
    model_access:
      provider: "http_api"
      connection:
        base_url: "https://api.tasker.internal"
        auth_token: "${API_AUTH_TOKEN}"
    queue:
      provider: "sqs"
      connection:
        region: "us-west-2"
        access_key_id: "${AWS_ACCESS_KEY_ID}"
        secret_access_key: "${AWS_SECRET_ACCESS_KEY}"
```

#### 2.2 Service Factory Pattern

**Rust Service Factory**:
```rust
// src/services/factory.rs
pub struct ServiceFactory {
    config: ServiceConfig,
}

impl ServiceFactory {
    pub fn new(config: ServiceConfig) -> Self {
        Self { config }
    }
    
    pub async fn create_database_service(&self) -> Result<Arc<dyn DatabaseService>> {
        match self.config.database.provider {
            DatabaseProvider::PostgreSQL => {
                let service = PostgreSQLDatabaseService::new(&self.config.database).await?;
                Ok(Arc::new(service))
            }
            DatabaseProvider::MySQL => {
                let service = MySQLDatabaseService::new(&self.config.database).await?;
                Ok(Arc::new(service))
            }
            DatabaseProvider::SQLite => {
                let service = SQLiteDatabaseService::new(&self.config.database).await?;
                Ok(Arc::new(service))
            }
        }
    }
    
    pub async fn create_registry_service(&self) -> Result<Box<dyn RegistryService>> {
        match self.config.queue.provider {
            QueueProvider::PGMQ => {
                let service = PgmqQueueService::new(&self.config.queue).await?;
                Ok(Arc::new(service))
            }
            QueueProvider::SQS => {
                let service = SqsQueueService::new(&self.config.queue).await?;
                Ok(Arc::new(service))
            }
            QueueProvider::Kafka => {
                let service = KafkaQueueService::new(&self.config.queue).await?;
                Ok(Arc::new(service))
            }
            QueueProvider::Redis => {
                let service = RedisQueueService::new(&self.config.queue).await?;
                Ok(Arc::new(service))
            }
        }
    }
    
    pub async fn create_registry_service(&self, database_service: Arc<dyn DatabaseService>) -> Result<Arc<dyn RegistryService>> {
        match self.config.registry.provider {
            RegistryProvider::Database => {
                let service = DatabaseRegistryService::new(database_service);
                Ok(Arc::new(service))
            }
            RegistryProvider::File => {
                let service = FileRegistryService::new(&self.config.registry.base_path);
                Ok(Arc::new(service))
            }
            RegistryProvider::Redis => {
                let service = RedisRegistryService::new(&self.config.registry.connection).await?;
                Ok(Arc::new(service))
            }
        }
    }
}
```

**Ruby Service Factory**:
```ruby
# bindings/ruby/lib/tasker_core/services/factory.rb
module TaskerCore
  module Services
    class Factory
      def self.create_database_service(config = nil)
        config ||= TaskerCore::Config.instance.effective_config.dig('services', 'database')
        
        case config['provider']
        when 'postgresql'
          ActiveRecordDatabaseService.new(config)
        when 'mysql'
          # Future: MySQLDatabaseService.new(config)
          raise NotImplementedError, "MySQL database service not yet implemented"
        when 'sqlite'
          # Future: SQLiteDatabaseService.new(config)
          raise NotImplementedError, "SQLite database service not yet implemented"
        else
          raise ArgumentError, "Unknown database provider: #{config['provider']}"
        end
      end
      
      def self.create_queue_service(config = nil)
        config ||= TaskerCore::Config.instance.effective_config.dig('services', 'queue')
        
        case config['provider']
        when 'pgmq'
          PgmqQueueService.new(config)
        when 'sqs'
          # Future: SqsQueueService.new(config)
          raise NotImplementedError, "SQS queue service not yet implemented"
        when 'kafka'
          # Future: KafkaQueueService.new(config)
          raise NotImplementedError, "Kafka queue service not yet implemented"
        when 'redis'
          # Future: RedisQueueService.new(config)
          raise NotImplementedError, "Redis queue service not yet implemented"
        else
          raise ArgumentError, "Unknown queue provider: #{config['provider']}"
        end
      end
      
      def self.create_registry_service(database_service, config = nil)
        config ||= TaskerCore::Config.instance.effective_config.dig('services', 'registry')
        
        case config['provider']
        when 'database'
          DatabaseRegistryService.new(database_service, config)
        when 'file'
          # Future: FileRegistryService.new(config)
          raise NotImplementedError, "File registry service not yet implemented"
        when 'redis'
          # Future: RedisRegistryService.new(config)
          raise NotImplementedError, "Redis registry service not yet implemented"
        else
          raise ArgumentError, "Unknown registry provider: #{config['provider']}"
        end
      end
    end
  end
end
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
# bindings/ruby/lib/tasker_core/services/message_service.rb
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

### Phase 4: Orchestration Service Refactoring (Weeks 7-10)

#### 4.1 Orchestration Service Interface

**Current Dependencies to Abstract**:
- `OrchestrationSystem` directly uses `PgmqClient` and `PgPool`
- `StepEnqueuer` directly uses `PgmqClient` and database functions
- `TaskClaimer` directly uses SQL queries
- `ViableStepDiscovery` directly uses database functions

**New Orchestration Service**:
```rust
// src/services/orchestration/mod.rs
pub struct OrchestrationService {
  model_access_service: Arc<dyn ModelAccessService>,
  queue_service: Arc<dyn QueueService>,
  registry_service: Arc<dyn RegistryService>,
  message_router: Arc<dyn MessageRouter>,
  config: OrchestrationConfig,
}
  
impl OrchestrationService {
  pub async fn new(
    model_access_service: Arc<dyn ModelAccessService>,
    queue_service: Arc<dyn QueueService>,
    registry_service: Arc<dyn RegistryService>,
    message_router: Arc<dyn MessageRouter>,
    config: OrchestrationConfig,
  ) -> Self {
    Self {
      model_access_service,
      queue_service,
      registry_service,
      message_router,
      config,
    }
  }
    
    pub async fn process_task_request(&self, request: &TaskRequest) -> Result<TaskInitializationResult> {
        // Use registry_service instead of direct database access
        let template = self.registry_service.get_task_template(&request.to_registry_key()).await?;
        
        // Use database_service for task creation
        let task = self.database_service.create_task_from_request(request, &template).await?;
        
        // Use queue_service for enqueueing ready steps
        let ready_steps = self.database_service.find_ready_steps(task.id).await?;
        for step in ready_steps {
            let message = SimpleStepMessage::from_step(&step);
            let queue_name = self.message_router.route_to_queue(&message)?;
            self.queue_service.send_message(&queue_name, &message).await?;
        }
        
        Ok(TaskInitializationResult { task_id: task.id, steps_enqueued: ready_steps.len() })
    }
}
```

#### 4.2 Worker Service Refactoring

**Current Dependencies to Abstract**:
- `QueueWorker` directly uses `PgmqClient` and ActiveRecord models
- Message processing hard-coded to database lookups
- Step handler resolution coupled to `TaskTemplateRegistry`

**New Worker Service**:
```ruby
# bindings/ruby/lib/tasker_core/services/worker_service.rb
module TaskerCore
  module Services
    class WorkerService
      def initialize(namespace, database_service: nil, queue_service: nil, registry_service: nil)
        @namespace = namespace
        @database_service = database_service || Services::Factory.create_database_service
        @queue_service = queue_service || Services::Factory.create_queue_service
        @registry_service = registry_service || Services::Factory.create_registry_service(@database_service)
        @message_service = MessageService.new(@queue_service, NamespaceMessageRouter.new)
        @running = false
        @logger = TaskerCore::Logging::Logger.instance
      end
      
      def start
        @running = true
        @logger.info("🚀 WORKER_SERVICE: Starting worker for namespace: #{@namespace}")
        
        polling_loop
      end
      
      def stop
        @running = false
        @logger.info("🛑 WORKER_SERVICE: Stopping worker for namespace: #{@namespace}")
      end
      
      private
      
      def polling_loop
        while @running
          begin
            messages = @message_service.receive_messages(@namespace, batch_size: 10)
            
            messages.each do |message|
              process_message(message)
            end
            
            sleep(poll_interval) if messages.empty?
          rescue => e
            @logger.error("💥 WORKER_SERVICE: Error in polling loop: #{e.message}")
            sleep(error_backoff_interval)
          end
        end
      end
      
      def process_message(message)
        case message
        when TaskerCore::Types::SimpleStepMessage
          process_simple_step_message(message)
        else
          @logger.warn("⚠️ WORKER_SERVICE: Unknown message type: #{message.class}")
        end
      end
      
      def process_simple_step_message(message)
        # Use model_access_service for fetching models (could be database or API)
        task = @model_access_service.find_task_by_uuid(message.task_uuid)
        step = @model_access_service.find_step_by_uuid(message.step_uuid)
        
        # Use registry_service for handler resolution
        handler = @registry_service.resolve_step_handler(task, step)
        
        # Execute handler with real models (maintaining current benefit)
        result = handler.call(task, build_sequence(message), step)
        
        # Mark message as processed
        @message_service.mark_completed(message)
        
        @logger.info("✅ WORKER_SERVICE: Step completed - step_uuid: #{message.step_uuid}")
      rescue => e
        @logger.error("💥 WORKER_SERVICE: Error processing step #{message.step_uuid}: #{e.message}")
        @message_service.mark_failed(message, e)
      end
    end
  end
end
```

### Phase 5: Service Container and Dependency Injection (Weeks 11-12)

#### 5.1 Service Container Pattern

**Rust Service Container**:
```rust
// src/services/container.rs
pub struct ServiceContainer {
  model_access_service: Arc<dyn ModelAccessService>,
  queue_service: Arc<dyn QueueService>,
  registry_service: Arc<dyn RegistryService>,
  message_router: Arc<dyn MessageRouter>,
}
  
impl ServiceContainer {
  pub async fn from_config(config: ServiceConfig) -> Result<Self> {
    let factory = ServiceFactory::new(config);
      
    let model_access_service = Arc::new(factory.create_model_access_service().await?);
    let queue_service = Arc::new(factory.create_queue_service().await?);
    let registry_service = Arc::new(factory.create_registry_service().await?);
    let message_router = Arc::new(NamespaceMessageRouter::new(
      config.queue.namespace_pattern
    ));
      
    Ok(Self {
      model_access_service,
      queue_service,
      registry_service,
      message_router,
    })
  }
    
    pub fn model_access_service(&self) -> Arc<dyn ModelAccessService> {
        self.model_access_service.clone()
    }
    
    pub fn queue_service(&self) -> Arc<dyn QueueService> {
        self.queue_service.clone()
    }
    
    pub fn registry_service(&self) -> Arc<dyn RegistryService> {
        self.registry_service.clone()
    }
    
    pub fn message_router(&self) -> Arc<dyn MessageRouter> {
        self.message_router.clone()
    }
}
```

**Ruby Service Container**:
```ruby
# bindings/ruby/lib/tasker_core/services/container.rb
module TaskerCore
  module Services
    class Container
      include Singleton
      
      attr_reader :model_access_service, :queue_service, :registry_service, :message_service
      
      def initialize
        @initialized = false
      end
      
      def configure!(config = nil)
        config ||= TaskerCore::Config.instance.effective_config['services']
        
        @model_access_service = Factory.create_model_access_service(config['model_access'])
        @queue_service = Factory.create_queue_service(config['queue'])
        @registry_service = Factory.create_registry_service(config['registry'])
        @message_service = MessageService.new(@queue_service, NamespaceMessageRouter.new(config['queue']['namespace_pattern']))
        
        @initialized = true
        
        TaskerCore::Logging::Logger.instance.info("✅ SERVICE_CONTAINER: Services configured with providers - model_access: #{config['model_access']['provider']}, queue: #{config['queue']['provider']}")
      end
      
      def configured?
        @initialized
      end
      
      def ensure_configured!
        configure! unless configured?
      end
    end
  end
end
```

## Implementation Plan

### Phase 1: Foundation (Weeks 1-2)
**Objective**: Create basic service interface definitions and factory patterns

**Tasks**:
1. **Create Rust service traits**: ModelAccess, Queue, Registry, MessageRouter
2. **Create Ruby service base classes**: ModelAccessService, QueueService, RegistryService
3. **Implement PostgreSQL Direct/PGMQ/ActiveRecord services** as first concrete implementations
4. **Update configuration structures** to support service provider selection
5. **Create service factory patterns** for both Rust and Ruby

**Success Criteria**:
- Service interfaces defined and documented (PostgreSQL-optimized, not abstracted)
- Factory patterns create services based on configuration
- Existing functionality works through new service interfaces
- Model access service supports both database and API patterns
- All tests pass with new service layer

### Phase 2: Orchestration Refactoring (Weeks 3-6)
**Objective**: Refactor orchestration components to use service abstractions

**Tasks**:
1. **Refactor OrchestrationSystem** to use ServiceContainer
2. **Update StepEnqueuer** to use QueueService instead of PgmqClient
3. **Refactor TaskHandlerRegistry** to use RegistryService
4. **Update ViableStepDiscovery** to use ModelAccessService
5. **Refactor task claiming and state management** to use ModelAccessService

**Files to Modify**:
- `src/orchestration/orchestration_system.rs` (inject ServiceContainer)
- `src/orchestration/step_enqueuer.rs` (use QueueService)
- `src/registry/task_handler_registry.rs` (use RegistryService)
- `src/orchestration/viable_step_discovery.rs` (use ModelAccessService)
- `src/orchestration/task_claimer.rs` (use ModelAccessService)

**Success Criteria**:
- Orchestration system works entirely through service abstractions
- No direct PGMQ dependencies in orchestration logic (PostgreSQL direct access preserved)
- Configuration-driven service selection works
- All orchestration tests pass

### Phase 3: Worker Refactoring (Weeks 7-8)
**Objective**: Refactor Ruby workers to use service abstractions

**Tasks**:
1. **Refactor QueueWorker** to use WorkerService
2. **Update message processing** to use MessageService
3. **Refactor step handler resolution** to use RegistryService  
4. **Update model access** to use ModelAccessService (database or API)
5. **Maintain ActiveRecord model benefits** while abstracting data access

**Files to Modify**:
- `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb` (use WorkerService)
- `bindings/ruby/lib/tasker_core/messaging/message_manager.rb` (use ModelAccessService)
- `bindings/ruby/lib/tasker_core/registry/step_handler_resolver.rb` (use RegistryService)
- `bindings/ruby/lib/tasker_core/registry/task_template_registry.rb` (use RegistryService)

**Success Criteria**:
- Workers operate entirely through service abstractions
- ActiveRecord models still available to handlers (maintain current benefit)
- Queue operations abstracted from specific PGMQ implementation
- Model access supports both direct database and API patterns
- All Ruby integration tests pass

### Phase 4: Alternative Implementation Support (Weeks 9-12)
**Objective**: Implement alternative service providers to validate abstraction quality

**Tasks**:
1. **Implement SQS queue service** (both Rust and Ruby)
2. **Implement Redis registry service** (for fast template caching)
3. **Create file-based registry service** (for development/testing)
4. **Add HTTP API model access service** (validate model access abstraction)
5. **Add GraphQL model access service** (alternative API pattern)
5. **Create configuration examples** for different deployment scenarios

**New Components**:
- `src/services/queue/sqs_service.rs`
- `src/services/registry/redis_service.rs`
- `src/services/database/mysql_service.rs`
- `bindings/ruby/lib/tasker_core/services/sqs_queue_service.rb`
- `bindings/ruby/lib/tasker_core/services/redis_registry_service.rb`

**Success Criteria**:
- Multiple queue providers work interchangeably
- Multiple registry providers work interchangeably
- Configuration-driven provider selection works seamlessly
- Performance characteristics comparable to current implementation

### Phase 5: Production Readiness (Weeks 13-16)
**Objective**: Production deployment patterns and monitoring

**Tasks**:
1. **Service health monitoring** for all service types
2. **Service failover and circuit breaker patterns**
3. **Configuration validation** for service combinations
4. **Performance benchmarking** across different service providers
5. **Documentation and deployment guides** for different architectures

## Service Interface Specifications

### Model Access Service Interface

```rust
// Since we are PostgreSQL-specific, we define interfaces for model access
// that can be fulfilled by direct database access or API calls

pub trait ModelAccessService: Send + Sync {
    async fn find_task_by_uuid(&self, uuid: &str) -> Result<Option<TaskModel>>;
    async fn find_step_by_uuid(&self, uuid: &str) -> Result<Option<StepModel>>;
    async fn find_steps_by_uuids(&self, uuids: &[String]) -> Result<Vec<StepModel>>;
    async fn update_step_state(&self, step_uuid: &str, state: StepState) -> Result<()>;
    async fn update_task_state(&self, task_uuid: &str, state: TaskState) -> Result<()>;
    async fn find_ready_steps(&self, task_uuid: &str) -> Result<Vec<StepModel>>;
    async fn health_check(&self) -> Result<bool>;
}

pub struct TaskModel {
    pub uuid: String,
    pub name: String,
    pub state: TaskState,
    pub namespace: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct StepModel {
    pub uuid: String,
    pub name: String,
    pub state: StepState,
    pub task_uuid: String,
    pub namespace: String,
    pub handler_name: String,
    pub dependencies: Vec<String>,
    pub results: Option<serde_json::Value>,
}
```

### Queue Service Interface
```rust
// src/services/queue/traits.rs
pub trait QueueService: Send + Sync {
    type MessageId;
    type QueuedMessage;
    
    // Queue management
    async fn create_queue(&self, queue_name: &str) -> Result<()>;
    async fn delete_queue(&self, queue_name: &str) -> Result<()>;
    async fn queue_exists(&self, queue_name: &str) -> Result<bool>;
    
    // Message operations
    async fn send_message<T>(&self, queue_name: &str, message: &T) -> Result<Self::MessageId>
    where
        T: serde::Serialize + Send + Sync;
    
    async fn receive_messages(&self, queue_name: &str, batch_size: Option<usize>) -> Result<Vec<Self::QueuedMessage>>;
    async fn delete_message(&self, queue_name: &str, message_id: &Self::MessageId) -> Result<()>;
    async fn extend_visibility(&self, queue_name: &str, message_id: &Self::MessageId, timeout: Duration) -> Result<()>;
    
    // Queue monitoring
    async fn queue_stats(&self, queue_name: &str) -> Result<QueueStats>;
    async fn list_queues(&self) -> Result<Vec<String>>;
    
    // Dead letter queue support
    async fn send_to_dlq(&self, queue_name: &str, message: &Self::QueuedMessage, reason: &str) -> Result<()>;
}

pub struct QueuedMessage<T> {
    pub id: String,
    pub message: T,
    pub receive_count: i32,
    pub enqueued_at: DateTime<Utc>,
    pub visibility_timeout: Option<DateTime<Utc>>,
}

pub struct QueueStats {
    pub name: String,
    pub message_count: i64,
    pub in_flight_count: i64,
    pub oldest_message_age: Option<Duration>,
    pub created_at: DateTime<Utc>,
}
```

### Registry Service Interface
```rust
// src/services/registry/traits.rs
pub trait RegistryService: Send + Sync {
    // Template operations
    async fn get_task_template(&self, key: &RegistryKey) -> Result<Option<TaskTemplate>>;
    async fn register_task_template(&self, template: &TaskTemplate) -> Result<()>;
    async fn list_templates(&self, namespace: Option<&str>) -> Result<Vec<RegistryKey>>;
    async fn template_exists(&self, key: &RegistryKey) -> Result<bool>;
    async fn delete_template(&self, key: &RegistryKey) -> Result<()>;
    
    // Handler resolution
    async fn resolve_step_handler(&self, key: &RegistryKey, step_name: &str) -> Result<Option<HandlerMetadata>>;
    async fn list_step_handlers(&self, namespace: &str) -> Result<Vec<StepHandlerInfo>>;
    
    // Cache management
    async fn invalidate_cache(&self, key: &RegistryKey) -> Result<()>;
    async fn warm_cache(&self, namespace: &str) -> Result<()>;
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RegistryKey {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

impl RegistryKey {
    pub fn new(namespace: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
            version: version.into(),
        }
    }
    
    pub fn from_task_request(request: &TaskRequest) -> Self {
        Self::new(&request.namespace, &request.name, &request.version)
    }
}
```

## Detailed Implementation Strategy

### Current Coupling Analysis

#### Rust Orchestration Layer Coupling
**Files with High Coupling**:

1. **`src/orchestration/step_enqueuer.rs`** (Lines 140-180):
   - Direct `PgmqClient` usage for enqueueing
   - Direct `sqlx::PgPool` for database operations
   - Hard-coded queue naming pattern

2. **`src/orchestration/task_claimer.rs`**:
   - Direct SQL queries through `SqlFunctionExecutor`
   - PostgreSQL-specific locking mechanisms
   - Hard-coded database function calls

3. **`src/orchestration/viable_step_discovery.rs`**:
   - Direct database function execution
   - PostgreSQL-specific CTE queries
   - Tight coupling to `SqlFunctionExecutor`

4. **`src/registry/task_handler_registry.rs`** (Lines 97-150):
   - Direct `PgPool` dependency injection
   - PostgreSQL-specific queries for template retrieval
   - Hard-coded table and column names

#### Ruby Worker Layer Coupling
**Files with High Coupling**:

1. **`bindings/ruby/lib/tasker_core/messaging/queue_worker.rb`** (Lines 180-250):
   - Direct `PgmqClient` usage for message polling
   - Hard-coded ActiveRecord model lookups
   - PGMQ-specific visibility timeout handling

2. **`bindings/ruby/lib/tasker_core/messaging/message_manager.rb`**:
   - Direct ActiveRecord queries with UUID lookups
   - Hard-coded model class assumptions
   - Tight coupling to specific database schema

3. **`bindings/ruby/lib/tasker_core/registry/step_handler_resolver.rb`** (Lines 35-75):
   - Direct `TaskTemplateRegistry` singleton access
   - ActiveRecord model dependencies
   - Hard-coded Ruby class resolution patterns

4. **`bindings/ruby/lib/tasker_core/database/connection.rb`**:
   - Direct ActiveRecord connection management
   - PostgreSQL-specific connection pooling
   - Hard-coded database configuration

### Service Migration Strategy

#### Step 1: Create Service Foundation (Week 1)

**New Files to Create**:

```rust
// src/services/mod.rs
pub mod database;
pub mod queue;
pub mod registry;
pub mod container;
pub mod factory;

pub use database::{DatabaseService, TaskRepository, StepRepository};
pub use queue::{QueueService, MessageRouter};
pub use registry::{RegistryService, RegistryKey};
pub use container::ServiceContainer;
pub use factory::ServiceFactory;
```

```ruby
# bindings/ruby/lib/tasker_core/services.rb
require_relative 'services/database_service'
require_relative 'services/queue_service'
require_relative 'services/registry_service'
require_relative 'services/message_service'
require_relative 'services/worker_service'
require_relative 'services/container'
require_relative 'services/factory'

module TaskerCore
  module Services
    # Service module for dependency injection and abstraction
  end
end
```

#### Step 2: Implement PostgreSQL/PGMQ Services (Week 2)

**PostgreSQL Database Service Implementation**:
```rust
// src/services/database/postgresql_service.rs
pub struct PostgreSQLDatabaseService {
    pool: PgPool,
    sql_executor: SqlFunctionExecutor,
}

impl DatabaseService for PostgreSQLDatabaseService {
    type Connection = PgPool;
    type Transaction = sqlx::Transaction<'static, sqlx::Postgres>;
    type QueryResult = Vec<PgRow>;
    
    async fn execute_query(&self, query: &str, params: QueryParams) -> Result<Self::QueryResult> {
        let mut query_builder = sqlx::query(query);
        for param in params.iter() {
            query_builder = query_builder.bind(param);
        }
        
        let rows = query_builder.fetch_all(&self.pool).await?;
        Ok(rows)
    }
    
    async fn execute_function(&self, function_name: &str, params: FunctionParams) -> Result<FunctionResult> {
        self.sql_executor.execute_function(function_name, params).await
    }
}

impl TaskRepository for PostgreSQLDatabaseService {
    async fn find_task_by_uuid(&self, uuid: &str) -> Result<Option<Task>> {
        // Implementation using SQL function or direct query
        self.sql_executor.get_task_by_uuid(uuid).await
    }
    
    async fn create_task(&self, request: &TaskRequest) -> Result<Task> {
        self.sql_executor.create_task_from_request(request).await
    }
}
```

#### Step 3: Refactor Core Components (Weeks 3-4)

**StepEnqueuer Refactoring**:
```rust
// Current: src/orchestration/step_enqueuer.rs
pub struct StepEnqueuer {
    viable_step_discovery: ViableStepDiscovery,
    pgmq_client: PgmqClient,  // REMOVE
    pool: PgPool,             // REMOVE
    config: StepEnqueuerConfig,
    state_manager: StateManager,
}

// New: Use service container
pub struct StepEnqueuer {
    viable_step_discovery: ViableStepDiscovery,
    services: Arc<ServiceContainer>,  // ADD
    config: StepEnqueuerConfig,
    state_manager: StateManager,
}

impl StepEnqueuer {
    pub async fn new(services: Arc<ServiceContainer>) -> Result<Self> {
        let viable_step_discovery = ViableStepDiscovery::new(services.clone());
        let state_manager = StateManager::new(services.clone());
        
        Ok(Self {
            viable_step_discovery,
            services,
            config: StepEnqueuerConfig::default(),
            state_manager,
        })
    }
    
    pub async fn enqueue_ready_steps(&self, claimed_task: &ClaimedTask) -> Result<StepEnqueueResult> {
        // Use services instead of direct clients
        let ready_steps = self.services.database_service()
            .find_ready_steps(claimed_task.task_id).await?;
            
        for step in ready_steps {
            let message = SimpleStepMessage::from_step(&step);
            let queue_name = self.services.message_router()
                .route_to_queue(&message)?;
            
            self.services.queue_service()
                .send_message(&queue_name, &message).await?;
        }
        
        // Build result...
    }
}
```

**QueueWorker Refactoring**:
```ruby
# Current: bindings/ruby/lib/tasker_core/messaging/queue_worker.rb
class QueueWorker
  def initialize(namespace, pgmq_client: nil, logger: nil, ...)
    @pgmq_client = pgmq_client || PgmqClient.new  # REMOVE
    # Other initialization...
  end
end

# New: Use service container
class QueueWorker
  def initialize(namespace, services: nil, logger: nil, ...)
    @namespace = namespace
    @services = services || TaskerCore::Services::Container.instance  # ADD
    @services.ensure_configured!
    @logger = logger || TaskerCore::Logging::Logger.instance
    # Other initialization...
  end
  
  private
  
  def process_simple_step_message(msg_data)
    # Use services instead of direct models
    task = @services.database_service.fetch_by_uuid(
      TaskerCore::Database::Models::Task, 
      msg_data.task_uuid
    )
    
    step = @services.database_service.fetch_by_uuid(
      TaskerCore::Database::Models::WorkflowStep,
      msg_data.step_uuid
    )
    
    # Use registry service for handler resolution
    resolved_handler = @services.registry_service.resolve_step_handler(task, step)
    
    # Execute handler (maintaining ActiveRecord model benefits)
    handler = create_handler_instance(resolved_handler)
    result = handler.call(task, sequence, step)
    
    # Complete message through queue service
    @services.queue_service.delete_message(@queue_name, msg_data.message_id)
  end
end
```

### Configuration Strategy

#### Multi-Provider Configuration Examples

**Production with SQS + PostgreSQL**:
```yaml
# config/production.yaml
services:
  database:
    provider: postgresql
    connection_string: "${DATABASE_URL}"
    pool:
      max_connections: 20
      idle_timeout: 300
      
  queue:
    provider: sqs
    namespace_pattern: "tasker-{namespace}-queue"
    connection:
      region: "${AWS_REGION}"
      access_key_id: "${AWS_ACCESS_KEY_ID}"
      secret_access_key: "${AWS_SECRET_ACCESS_KEY}"
    settings:
      visibility_timeout_seconds: 300
      message_retention_seconds: 1209600  # 14 days
      dead_letter_queue_enabled: true
      max_receive_count: 3
      
  registry:
    provider: database
    cache:
      enabled: true
      provider: redis
      ttl_seconds: 600
      connection_string: "${REDIS_URL}"
```

**Development with PGMQ + PostgreSQL**:
```yaml
# config/development.yaml
services:
  database:
    provider: postgresql
    connection_string: "postgresql://tasker:tasker@localhost/tasker_development"
    
  queue:
    provider: pgmq
    namespace_pattern: "{namespace}_queue"
    
  registry:
    provider: database
    cache:
      enabled: false
```

**Hybrid Architecture Example**:
```yaml
# config/hybrid.yaml - Shows flexibility
services:
  database:
    provider: postgresql
    connection_string: "${DATABASE_URL}"
    
  queue:
    provider: kafka
    namespace_pattern: "tasker.{namespace}.steps"
    connection:
      bootstrap_servers: "${KAFKA_BROKERS}"
      security_protocol: "SASL_SSL"
      sasl_username: "${KAFKA_USERNAME}"
      sasl_password: "${KAFKA_PASSWORD}"
      
  registry:
    provider: redis
    connection_string: "${REDIS_URL}"
    fallback:
      provider: database
      connection_string: "${DATABASE_URL}"
```

### Service Implementation Priorities

#### Critical Path Components (Immediate Refactoring)

1. **OrchestrationSystem Service Injection**:
   - Replace direct `PgmqClient` and `PgPool` dependencies
   - Inject `ServiceContainer` in constructor
   - Route all operations through service abstractions

2. **QueueWorker Service Injection**:
   - Replace direct `PgmqClient` and ActiveRecord dependencies
   - Use `Services::Container.instance` for service access
   - Maintain ActiveRecord model benefits for handlers

3. **Registry Service Abstraction**:
   - Abstract `TaskHandlerRegistry` and `TaskTemplateRegistry`
   - Create unified registry interface for both Rust and Ruby
   - Support caching strategies (Redis, in-memory, etc.)

#### Service Interface Evolution

**Phase 1 Interfaces** (Minimal Viable Abstraction):
- Basic CRUD operations for database service
- Send/receive for queue service  
- Get/register for registry service

**Phase 2 Interfaces** (Production Features):
- Transaction support for database service
- Dead letter queue support for queue service
- Cache management for registry service
- Health monitoring for all services

**Phase 3 Interfaces** (Advanced Features):
- Sharding support for database service
- Message routing strategies for queue service
- Versioning and migration support for registry service
- Circuit breaker patterns for all services

### Database Service Migration Strategy

#### Current Database Dependencies
**Rust Side** (15+ files to migrate):
- `src/orchestration/*.rs` - All components use `PgPool` directly
- `src/database/sql_functions.rs` - PostgreSQL-specific function calls
- `src/registry/task_handler_registry.rs` - Direct database queries

**Ruby Side** (8+ files to migrate):
- `bindings/ruby/lib/tasker_core/database/models/*.rb` - ActiveRecord models
- `bindings/ruby/lib/tasker_core/messaging/message_manager.rb` - Direct AR queries
- `bindings/ruby/lib/tasker_core/registry/*.rb` - Database-coupled registry logic

#### Migration Approach: Adapter Pattern

**Step 1**: Create adapter layer that wraps existing implementations
```rust
// src/services/database/postgresql_adapter.rs
pub struct PostgreSQLAdapter {
    pool: PgPool,
    sql_executor: SqlFunctionExecutor,
}

impl DatabaseService for PostgreSQLAdapter {
    // Delegate to existing implementations
    async fn execute_function(&self, name: &str, params: FunctionParams) -> Result<FunctionResult> {
        self.sql_executor.execute_named_function(name, params).await
    }
}

impl TaskRepository for PostgreSQLAdapter {
    async fn find_task_by_uuid(&self, uuid: &str) -> Result<Option<Task>> {
        // Wrap existing SQL function call
        self.sql_executor.get_task_by_uuid(uuid).await
    }
}
```

**Step 2**: Update consumers to use service interfaces
```rust
// Before
impl StepEnqueuer {
    pub async fn new(pool: PgPool, pgmq_client: PgmqClient) -> Result<Self> {
        // Direct dependencies
    }
}

// After
impl StepEnqueuer {
    pub async fn new(services: Arc<ServiceContainer>) -> Result<Self> {
        // Service abstraction
    }
}
```

**Step 3**: Implement alternative providers
```rust
// Note: We remain PostgreSQL-specific by design
// This example would be for model access abstraction only
pub struct HttpApiModelAccessService {
    client: reqwest::Client,
    base_url: String,
    auth_token: String,
}

impl ModelAccessService for HttpApiModelAccessService {
    async fn find_task_by_uuid(&self, uuid: &str) -> Result<Option<TaskModel>> {
        let response = self.client
            .get(&format!("{}/tasks/{}", self.base_url, uuid))
            .header("Authorization", &format!("Bearer {}", self.auth_token))
            .send()
            .await?;
            
        if response.status() == 404 {
            Ok(None)
        } else {
            let task: TaskModel = response.json().await?;
            Ok(Some(task))
        }
    }
}
```

### Queue Service Migration Strategy

#### Current Queue Dependencies
**Rust Side**:
- `src/messaging/pgmq_client.rs` - Direct PGMQ operations
- `src/orchestration/step_enqueuer.rs` - Queue enqueueing logic
- Queue naming patterns hard-coded throughout

**Ruby Side**:
- `bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb` - PGMQ wrapper
- `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb` - Polling logic
- Message processing coupled to PGMQ message format

#### Queue Service Implementation Patterns

**PGMQ Service Implementation**:
```rust
// src/services/queue/pgmq_service.rs
pub struct PgmqService {
    client: pgmq::PGMQueue,
    router: Arc<dyn MessageRouter>,
}

impl QueueService for PgmqService {
    type MessageId = i64;
    type QueuedMessage = QueuedMessage<serde_json::Value>;
    
    async fn send_message<T>(&self, queue_name: &str, message: &T) -> Result<Self::MessageId>
    where T: serde::Serialize + Send + Sync
    {
        let serialized = serde_json::to_value(message)?;
        let id = self.client.send(queue_name, &serialized).await?;
        Ok(id)
    }
    
    async fn receive_messages(&self, queue_name: &str, batch_size: Option<usize>) -> Result<Vec<Self::QueuedMessage>> {
        let messages = match batch_size {
            Some(size) => self.client.read_batch(queue_name, None, size as i32).await?.unwrap_or_default(),
            None => match self.client.read(queue_name, None).await? {
                Some(msg) => vec![msg],
                None => vec![],
            }
        };
        
        // Convert to unified QueuedMessage format
        Ok(messages.into_iter().map(|msg| QueuedMessage {
            id: msg.msg_id.to_string(),
            message: msg.message,
            receive_count: msg.read_ct.unwrap_or(0),
            enqueued_at: msg.enqueued_at.unwrap_or_else(|| Utc::now()),
            visibility_timeout: msg.vt,
        }).collect())
    }
}
```

**SQS Service Implementation**:
```rust
// src/services/queue/sqs_service.rs
pub struct SqsService {
    client: aws_sdk_sqs::Client,
    queue_urls: HashMap<String, String>,
}

impl QueueService for SqsService {
    type MessageId = String;
    type QueuedMessage = QueuedMessage<serde_json::Value>;
    
    async fn send_message<T>(&self, queue_name: &str, message: &T) -> Result<Self::MessageId>
    where T: serde::Serialize + Send + Sync
    {
        let queue_url = self.get_queue_url(queue_name).await?;
        let body = serde_json::to_string(message)?;
        
        let result = self.client
            .send_message()
            .queue_url(&queue_url)
            .message_body(body)
            .send()
            .await?;
            
        Ok(result.message_id().unwrap_or_default().to_string())
    }
}
```

### Registry Service Migration Strategy

#### Current Registry Dependencies
**Rust Side**:
- `src/registry/task_handler_registry.rs` - Direct database queries
- Template storage tied to PostgreSQL tables
- Hard-coded SQL for template retrieval

**Ruby Side**:
- `bindings/ruby/lib/tasker_core/registry/task_template_registry.rb` - ActiveRecord models
- Step handler resolution using Rails patterns
- Template loading from YAML files

#### Registry Service Implementation

**Database Registry Service**:
```ruby
# bindings/ruby/lib/tasker_core/services/database_registry_service.rb
module TaskerCore
  module Services
    class DatabaseRegistryService < RegistryService
      def initialize(database_service, config = {})
        @database_service = database_service
        @cache_enabled = config.dig('cache', 'enabled') || false
        @cache_ttl = config.dig('cache', 'ttl_seconds') || 300
        @cache = @cache_enabled ? Concurrent::Map.new : nil
      end
      
      def get_task_template(namespace, name, version)
        cache_key = "#{namespace}:#{name}:#{version}"
        
        if @cache_enabled && cached = @cache&.[](cache_key)
          return cached if cache_fresh?(cached)
        end
        
        template = @database_service.execute_function(
          'get_task_template_by_key',
          { namespace: namespace, name: name, version: version }
        )
        
        if @cache_enabled && template
          @cache[cache_key] = { data: template, cached_at: Time.current }
        end
        
        template
      end
      
      def resolve_step_handler(task, step)
        template = get_task_template(task.namespace, task.name, task.version)
        return nil unless template
        
        step_template = find_step_in_template(template, step.name)
        return nil unless step_template
        
        build_resolved_handler(step_template, template)
      end
      
      private
      
      def cache_fresh?(cached_entry)
        return false unless cached_entry[:cached_at]
        (Time.current - cached_entry[:cached_at]) < @cache_ttl
      end
    end
  end
end
```

**Redis Registry Service** (Future Enhancement):
```ruby
# bindings/ruby/lib/tasker_core/services/redis_registry_service.rb
module TaskerCore
  module Services
    class RedisRegistryService < RegistryService
      def initialize(redis_client, fallback_service = nil)
        @redis = redis_client
        @fallback = fallback_service
      end
      
      def get_task_template(namespace, name, version)
        key = "tasker:templates:#{namespace}:#{name}:#{version}"
        
        cached = @redis.get(key)
        if cached
          return JSON.parse(cached)
        end
        
        # Fallback to database if not in cache
        if @fallback
          template = @fallback.get_task_template(namespace, name, version)
          if template
            @redis.setex(key, 600, JSON.generate(template))  # 10 minute TTL
          end
          template
        else
          nil
        end
      end
    end
  end
end
```

## Service Composition Patterns

### Dependency Injection Container

**Rust Service Container**:
```rust
// src/services/container.rs
pub struct ServiceContainer {
    model_access_service: Arc<dyn ModelAccessService>,
    queue_service: Arc<dyn QueueService>,
    registry_service: Arc<dyn RegistryService>,
    message_router: Arc<dyn MessageRouter>,
}

impl ServiceContainer {
    pub async fn from_config(config: &ServiceConfig) -> Result<Self> {
        let factory = ServiceFactory::new(config.clone());
        
        let model_access_service = Arc::new(factory.create_model_access_service().await?);
        let queue_service = Arc::new(factory.create_queue_service().await?);
        let registry_service = Arc::new(factory.create_registry_service().await?);
        let message_router = Arc::new(NamespaceMessageRouter::new(
            config.queue.namespace_pattern.clone()
        ));
        
        Ok(Self {
            model_access_service,
            queue_service,
            registry_service,
            message_router,
        })
    }
    
    pub fn model_access_service(&self) -> Arc<dyn ModelAccessService> { self.model_access_service.clone() }
    pub fn queue_service(&self) -> Arc<dyn QueueService> { self.queue_service.clone() }
    pub fn registry_service(&self) -> Arc<dyn RegistryService> { self.registry_service.clone() }
    pub fn message_router(&self) -> Arc<dyn MessageRouter> { self.message_router.clone() }
}
```

### Backwards Compatibility Strategy

#### Maintaining Current Benefits

**ActiveRecord Model Access**:
```ruby
# Maintain current pattern where handlers get real ActiveRecord models
class WorkerService
  def process_simple_step_message(message)
    # Services abstract data access but still return ActiveRecord models
    task = @services.database_service.fetch_task_model(message.task_uuid)
    step = @services.database_service.fetch_step_model(message.step_uuid)
    
    # Handler still gets real ActiveRecord models with all ORM benefits
    handler.call(task, sequence, step)
  end
end

# Database service returns real ActiveRecord models
class ActiveRecordDatabaseService < DatabaseService
  def fetch_task_model(uuid)
    TaskerCore::Database::Models::Task.find_by!(task_uuid: uuid)
  end
  
  def fetch_step_model(uuid)
    TaskerCore::Database::Models::WorkflowStep.find_by!(step_uuid: uuid)
  end
end
```

**Simple Message Architecture Preservation**:
```rust
// Keep UUID-based simple messages while abstracting queue operations
impl StepEnqueuer {
    pub async fn enqueue_ready_steps(&self, claimed_task: &ClaimedTask) -> Result<StepEnqueueResult> {
        let ready_steps = self.services.step_repository()
            .find_ready_steps(claimed_task.task_id).await?;
            
        for step in ready_steps {
            // Maintain simple message structure
            let message = SimpleStepMessage {
                task_uuid: step.task_uuid,
                step_uuid: step.step_uuid,
                ready_dependency_step_uuids: step.dependency_uuids,
            };
            
            // Abstract queue operations
            let queue_name = self.services.message_router()
                .route_to_queue(&message)?;
            
            self.services.queue_service()
                .send_message(&queue_name, &message).await?;
        }
        
        Ok(result)
    }
}
```

## Testing Strategy for Service Abstractions

### Test Doubles and Mocking

**Rust Test Services**:
```rust
// tests/services/mock_services.rs
pub struct MockDatabaseService {
    pub query_responses: HashMap<String, QueryResult>,
    pub function_responses: HashMap<String, FunctionResult>,
}

impl DatabaseService for MockDatabaseService {
    async fn execute_query(&self, query: &str, _params: QueryParams) -> Result<Self::QueryResult> {
        self.query_responses.get(query)
            .cloned()
            .ok_or_else(|| TaskerError::test_error(&format!("No mock response for query: {}", query)))
    }
}

pub struct MockQueueService {
    pub queues: Mutex<HashMap<String, Vec<QueuedMessage<serde_json::Value>>>>,
}

impl QueueService for MockQueueService {
    async fn send_message<T>(&self, queue_name: &str, message: &T) -> Result<Self::MessageId> {
        let serialized = serde_json::to_value(message)?;
        let mut queues = self.queues.lock().unwrap();
        let queue = queues.entry(queue_name.to_string()).or_insert_with(Vec::new);
        
        let message_id = (queue.len() + 1) as i64;
        queue.push(QueuedMessage {
            id: message_id.to_string(),
            message: serialized,
            receive_count: 0,
            enqueued_at: Utc::now(),
            visibility_timeout: None,
        });
        
        Ok(message_id)
    }
    
    async fn receive_messages(&self, queue_name: &str, batch_size: Option<usize>) -> Result<Vec<Self::QueuedMessage>> {
        let mut queues = self.queues.lock().unwrap();
        let queue = queues.entry(queue_name.to_string()).or_insert_with(Vec::new);
        
        let take_count = batch_size.unwrap_or(1).min(queue.len());
        let messages = queue.drain(0..take_count).collect();
        
        Ok(messages)
    }
}
```

**Ruby Test Services**:
```ruby
# bindings/ruby/spec/support/mock_services.rb
module TaskerCore
  module Services
    class MockDatabaseService < DatabaseService
      def initialize
        @query_responses = {}
        @function_responses = {}
        @models = {}
      end
      
      def stub_query(query, response)
        @query_responses[query] = response
      end
      
      def stub_model(model_class, uuid, model_instance)
        @models["#{model_class.name}:#{uuid}"] = model_instance
      end
      
      def fetch_by_uuid(model_class, uuid)
        key = "#{model_class.name}:#{uuid}"
        @models[key] || raise(ActiveRecord::RecordNotFound, "No mock model for #{key}")
      end
    end
    
    class MockQueueService < QueueService
      def initialize
        @queues = {}
        @message_id_counter = 0
      end
      
      def send_message(queue_name, message)
        @queues[queue_name] ||= []
        @message_id_counter += 1
        @queues[queue_name] << {
          id: @message_id_counter,
          message: message,
          enqueued_at: Time.current
        }
        @message_id_counter
      end
      
      def receive_messages(queue_name, batch_size: 10)
        queue = @queues[queue_name] || []
        messages = queue.shift(batch_size)
        messages || []
      end
    end
  end
end
```

### Integration Testing Strategy

**Service Integration Tests**:
```rust
// tests/services/integration_tests.rs
#[tokio::test]
async fn test_orchestration_with_different_queue_providers() {
    // Test with PGMQ
    let pgmq_config = create_pgmq_config();
    let pgmq_services = ServiceContainer::from_config(&pgmq_config).await?;
    
    let result1 = test_task_orchestration(pgmq_services).await?;
    
    // Test with Mock (for fast testing)
    let mock_config = create_mock_config();
    let mock_services = ServiceContainer::from_config(&mock_config).await?;
    
    let result2 = test_task_orchestration(mock_services).await?;
    
    // Both should produce same logical results
    assert_eq!(result1.task_id, result2.task_id);
    assert_eq!(result1.steps_enqueued, result2.steps_enqueued);
}

async fn test_task_orchestration(services: ServiceContainer) -> Result<TaskResult> {
    let orchestrator = OrchestrationService::new(services).await?;
    
    let request = TaskRequest {
        namespace: "test".to_string(),
        name: "linear_workflow".to_string(),
        version: "1.0.0".to_string(),
        context: json!({"data": "test"}),
    };
    
    orchestrator.process_task_request(&request).await
}
```

## Migration Execution Plan

### Week-by-Week Implementation

#### Week 1: Service Interface Foundation
**Monday-Tuesday**: Create Rust service traits
- Define `DatabaseService`, `QueueService`, `RegistryService` traits
- Create unified error types for service operations
- Define configuration structures for service providers

**Wednesday-Thursday**: Create Ruby service base classes
- Define abstract service classes in Ruby
- Create factory pattern for service creation
- Update configuration loading to support service providers

**Friday**: PostgreSQL/PGMQ adapter implementations
- Wrap existing PostgreSQL/PGMQ implementations in service interfaces
- Ensure all existing functionality still works through adapters
- Add basic integration tests

#### Week 2: Service Container and Factory
**Monday-Tuesday**: Implement service factory patterns
- Create `ServiceFactory` for both Rust and Ruby
- Support configuration-driven service creation
- Add validation for service provider combinations

**Wednesday-Thursday**: Implement service containers
- Create `ServiceContainer` for dependency injection
- Support lazy initialization and service sharing
- Add container lifecycle management

**Friday**: Update configuration and validation
- Extend YAML configuration for service providers
- Add configuration validation for service combinations
- Test different configuration scenarios

#### Week 3-4: Orchestration Layer Migration
**Week 3**: Rust orchestration components
- Refactor `OrchestrationSystem` to use `ServiceContainer`
- Update `StepEnqueuer` to use service interfaces
- Migrate `TaskClaimer` and `ViableStepDiscovery`

**Week 4**: Complete orchestration migration
- Update all remaining orchestration components
- Ensure all tests pass with service abstraction
- Performance testing to verify no regressions

#### Week 5-6: Worker Layer Migration
**Week 5**: Ruby worker components
- Refactor `QueueWorker` to use service interfaces
- Update `MessageManager` and handler resolution
- Maintain ActiveRecord model benefits

**Week 6**: Complete worker migration
- Update all Ruby registry components
- Ensure all integration tests pass
- Verify end-to-end workflows work correctly

#### Week 7-8: Alternative Provider Implementation
**Week 7**: SQS queue service implementation
- Implement `SqsService` for both Rust and Ruby
- Add AWS SDK dependencies and configuration
- Test SQS-based message processing

**Week 8**: Redis registry service implementation
- Implement `RedisRegistryService` with fallback patterns
- Add caching layer for template lookups
- Test registry performance improvements

#### Week 9-10: Production Readiness
**Week 9**: Service monitoring and health checks
- Add health check endpoints for all services
- Implement service-level metrics collection
- Add circuit breaker patterns for fault tolerance

**Week 10**: Documentation and deployment guides
- Complete documentation for all service providers
- Create deployment examples for different architectures
- Add troubleshooting guides for service issues

### Risk Mitigation Strategy

#### Gradual Migration Approach
1. **Adapter Pattern First**: Wrap existing implementations without changing behavior
2. **Component-by-Component**: Migrate one orchestration component at a time
3. **Test Coverage**: Maintain 100% test coverage throughout migration
4. **Performance Monitoring**: Track performance metrics during migration
5. **Rollback Plan**: Keep old implementations available until migration complete

#### Service Quality Assurance
1. **Interface Testing**: Test all service implementations against same interface
2. **Load Testing**: Verify performance under production-like loads
3. **Failure Testing**: Test service failover and error handling
4. **Configuration Testing**: Validate all configuration combinations work

## Expected Benefits

### Architectural Benefits
- **Provider Flexibility**: Switch between PostgreSQL, MySQL, SQS, Kafka, etc.
- **Deployment Options**: Support different infrastructure patterns
- **Testing Efficiency**: Fast tests with mock services
- **Scalability**: Service-specific scaling strategies

### Operational Benefits
- **Configuration-Driven**: Change providers without code changes
- **Infrastructure Independence**: Deploy on different cloud providers
- **Cost Optimization**: Choose most cost-effective services per environment
- **Vendor Independence**: Avoid lock-in to specific technologies

### Development Benefits
- **Clear Boundaries**: Well-defined service responsibilities
- **Parallel Development**: Teams can work on different service implementations
- **Testing Flexibility**: Unit tests with mocks, integration tests with real services
- **Debugging Simplification**: Service-level logging and monitoring

## Configuration Examples

### Multi-Environment Service Configuration

**Development Environment**:
```yaml
# config/development.yaml
services:
  model_access:
    provider: "postgresql_direct"
    connection_string: "${DATABASE_URL}"
    pool:
      max_connections: 10
  queue:
    provider: "pgmq"
    namespace_pattern: "{namespace}_queue"
    settings:
      poll_interval_ms: 100
      visibility_timeout_seconds: 30
  registry:
    provider: "postgresql"
    cache:
      enabled: false
      enabled: false  # No cache in development for fresh reloads
```

**Testing Environment**:
```yaml
# config/test.yaml
services:
  database:
    provider: postgresql
    connection_string: "postgresql://tasker:tasker@localhost/tasker_test"
    pool:
      max_connections: 2
      
  queue:
    provider: mock
    namespace_pattern: "{namespace}_test_queue"
    
  registry:
    provider: mock
    preload_templates: true  # Load test templates at startup
```

**Production Environment**:
```yaml
# config/production.yaml
services:
  database:
    provider: postgresql
    connection_string: "${DATABASE_URL}"
    pool:
      max_connections: 20
      idle_timeout: 300
      connection_timeout: 30
      
  queue:
    provider: sqs
    namespace_pattern: "tasker-prod-{namespace}-queue"
    connection:
      region: "${AWS_REGION}"
      access_key_id: "${AWS_ACCESS_KEY_ID}"
      secret_access_key: "${AWS_SECRET_ACCESS_KEY}"
    settings:
      visibility_timeout_seconds: 300
      message_retention_seconds: 1209600  # 14 days
      dead_letter_queue_enabled: true
      max_receive_count: 3
      
  registry:
    provider: database
    cache:
      enabled: true
      provider: redis
      ttl_seconds: 600
      connection_string: "${REDIS_URL}"
      fallback_to_database: true
```

### Service Composition Examples

**Microservice Architecture**:
```yaml
# config/microservices.yaml
services:
  database:
    provider: postgresql
    connection_string: "${TASKS_DATABASE_URL}"
    
  queue:
    provider: kafka
    namespace_pattern: "tasker.{namespace}.steps"
    connection:
      bootstrap_servers: "${KAFKA_BROKERS}"
      
  registry:
    provider: http
    base_url: "${REGISTRY_SERVICE_URL}"
    auth:
      type: bearer_token
      token: "${REGISTRY_AUTH_TOKEN}"
```

**Hybrid Cloud Architecture**:
```yaml
# config/hybrid.yaml
services:
  model_access:
    provider: "postgresql_direct"
    connection_string: "${ON_PREMISE_DATABASE_URL}"
    
  queue:
    provider: "sqs"
    connection:
      region: "${AWS_REGION}"
      
  registry:
    provider: "postgresql"
    cache:
      enabled: true
      provider: "redis"
      connection_string: "${CLOUD_REDIS_URL}"
```

## Success Metrics

### Technical Metrics
- **Code Coupling Reduction**: 
  - Rust: Remove 15+ direct PGMQ dependencies from orchestration
  - Ruby: Abstract 8+ direct ActiveRecord coupling points through model access service
  - Target: 90% reduction in hard-coded queue and model access dependencies

- **Configuration Flexibility**:
  - Support 3+ queue providers (PGMQ, SQS, Kafka)
  - Support 3+ model access patterns (PostgreSQL Direct, HTTP API, GraphQL API)
  - Support 3+ registry providers (PostgreSQL, Redis, File)
  - Note: PostgreSQL remains the only database backend by design

- **Performance Preservation**:
  - Maintain current message processing throughput
  - Preserve ActiveRecord model benefits for handlers
  - Keep simple message architecture benefits (80% size reduction)
  - Maintain PostgreSQL-specific optimizations and SQL functions

### Operational Metrics
- **Deployment Flexibility**: 
  - Support development (PGMQ), staging (SQS), production (Kafka) configurations
  - Enable distributed deployments with HTTP API model access
  - Support different cloud provider deployments while keeping PostgreSQL backend

- **Testing Efficiency**:
  - 10x faster unit tests with mock model access and queue services
  - Isolated component testing without external dependencies
  - Parallel test execution support

- **Maintenance Simplification**:
  - Clear service boundaries for debugging
  - Service-specific monitoring and alerting
  - Independent queue scaling while maintaining PostgreSQL as central data store

## Risk Assessment and Mitigation

### High-Risk Areas
1. **Performance Regression**: Service abstraction overhead
   - **Mitigation**: Benchmark each service implementation, use zero-cost abstractions, maintain PostgreSQL direct access path
   
2. **ActiveRecord Model Loss**: Losing ORM benefits for handlers
   - **Mitigation**: Ensure model access services return real ActiveRecord models, not just hashes
   
3. **Configuration Complexity**: Too many configuration options
   - **Mitigation**: Provide sensible defaults (PostgreSQL Direct + PGMQ) and configuration validation

### Medium-Risk Areas
1. **Service Interface Evolution**: Changing interfaces breaks implementations
   - **Mitigation**: Version service interfaces, maintain backward compatibility
   
2. **Testing Complexity**: More complex test setup with service mocking
   - **Mitigation**: Provide comprehensive test helpers and factories

### Low-Risk Areas
1. **Provider Implementation Bugs**: Issues in specific service providers
   - **Mitigation**: Comprehensive integration testing per provider
   
2. **Configuration Errors**: Misconfigured service providers
   - **Mitigation**: Runtime validation and clear error messages

## Implementation Checklist

### Phase 1: Foundation (Weeks 1-2)
- [ ] Define Rust service traits (`ModelAccessService`, `QueueService`, `RegistryService`)
- [ ] Create Ruby service base classes for each service type
- [ ] Implement PostgreSQL Direct model access service (wrapping existing patterns)
- [ ] Implement ActiveRecord model access service for Ruby (maintain current functionality)
- [ ] Implement PGMQ adapter wrapping existing `PgmqClient`
- [ ] Create service factory patterns for both languages
- [ ] Update configuration structures for service provider selection
- [ ] Add configuration validation for service combinations
- [ ] Create basic integration tests for new service interfaces

### Phase 2: Orchestration Migration (Weeks 3-6)
- [ ] Refactor `OrchestrationSystem` to use `ServiceContainer`
- [ ] Update `StepEnqueuer` to use `QueueService` interface (eliminate direct PGMQ dependency)
- [ ] Migrate `TaskHandlerRegistry` to use `RegistryService`
- [ ] Update `ViableStepDiscovery` to use `ModelAccessService` (preserve PostgreSQL direct access)
- [ ] Refactor `TaskClaimer` to use model access service abstractions
- [ ] Update all orchestration tests to use service container
- [ ] Performance benchmark to ensure no regression (especially PostgreSQL operations)
- [ ] Add service-level monitoring and logging

### Phase 3: Worker Migration (Weeks 7-8)
- [ ] Refactor `QueueWorker` to use service interfaces
- [ ] Update `MessageManager` to use `ModelAccessService` (support database and API patterns)
- [ ] Migrate step handler resolution to use `RegistryService`
- [ ] Ensure handlers still receive real ActiveRecord models (maintain current benefit)
- [ ] Update all Ruby integration tests to use service abstractions
- [ ] Test end-to-end workflows with both database and API model access
- [ ] Add worker service monitoring and health checks

### Phase 4: Alternative Providers (Weeks 9-12)
- [ ] Implement SQS queue service (Rust and Ruby)
- [ ] Implement Redis registry service with PostgreSQL fallback
- [ ] Add HTTP API model access service (for distributed deployments)
- [ ] Add GraphQL API model access service (alternative API pattern)
- [ ] Create file-based registry for development/testing
- [ ] Test all provider combinations (queue + model access + registry)
- [ ] Add provider-specific configuration examples
- [ ] Performance comparison across queue and model access providers
- [ ] Documentation for provider selection based on deployment needs

### Phase 5: Production Readiness (Weeks 13-16)
- [ ] Add comprehensive service health monitoring
- [ ] Implement circuit breaker patterns
- [ ] Add service failover mechanisms
- [ ] Create deployment guides for different architectures
- [ ] Add troubleshooting documentation
- [ ] Performance optimization for production workloads
- [ ] Security review for all service implementations
- [ ] Load testing with alternative providers

## Conclusion

This service encapsulation plan maintains the architectural strengths of the current system—PostgreSQL-optimized data operations, simple UUID-based messages, and ActiveRecord model benefits—while adding the flexibility to use different queue providers and model access patterns based on deployment requirements.

**PostgreSQL Commitment**: This design intentionally remains PostgreSQL-specific to leverage PostgreSQL's advanced features, stored procedures, and performance optimizations. The abstraction focuses on queue providers and model access patterns, not database providers.

**Model Access Flexibility**: The key innovation is abstracting how models are fetched and hydrated—supporting both direct database access and API-based patterns for distributed deployments—while ensuring Ruby handlers always receive rich model objects.

The strategy pattern approach allows gradual migration with minimal risk, ensuring that existing functionality continues to work while building towards a more flexible and maintainable architecture. The end result will be a system that can adapt to different infrastructure requirements while preserving the simplicity and performance characteristics that make the current architecture effective.

**Key Success Factors**:
1. **Preserve PostgreSQL Benefits**: Maintain advanced SQL functions and performance optimizations
2. **Model Access Abstraction**: Support both database and API patterns while preserving ActiveRecord benefits
3. **Queue Provider Flexibility**: Enable PGMQ, SQS, Kafka without changing core logic
4. **Configuration-Driven**: Enable different providers without code changes  
5. **Gradual Migration**: Adapter pattern ensures continuous functionality
6. **Performance Preservation**: Maintain current throughput and latency characteristics

The implementation will result in a more maintainable, testable, and deployment-flexible system while preserving the architectural innovations that make the current system both simple and powerful, with PostgreSQL as the optimized and committed data foundation.
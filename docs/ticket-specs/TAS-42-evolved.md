# TAS-42-evolved: Ruby Worker Event-Driven FFI Implementation

## Executive Summary

Transform the Ruby worker from a complex infrastructure-heavy implementation to a thin, event-driven FFI layer that leverages the proven tasker-worker foundation. This evolved approach builds on the success of TAS-41 (Rust worker) by implementing a unified event-driven architecture using dry-events as the Ruby pub/sub framework, creating a clean separation between Rust orchestration infrastructure and Ruby business logic.

## Context and Dependencies

### Prerequisites
- **TAS-40 Complete**: tasker-worker foundation with command pattern architecture
- **TAS-41 Complete**: Native Rust worker demonstrates event-driven integration pattern
- **FFI Bridge Components**: Magnus-based FFI infrastructure in place
- **Event System Architecture**: Global WorkerEventSystem singleton pattern established

### Key Architectural Insights from TAS-41

The Rust worker implementation revealed critical architectural patterns that must be replicated for the Ruby FFI layer:

1. **Unified Event System**: All components must use the same `WorkerEventSystem` instance
2. **Event-Driven FFI**: Avoid complex memory management and dynamic method invocation across FFI boundaries
3. **Asynchronous Processing**: Rust publishes events and "frees immediately" - Ruby processes asynchronously
4. **Clean Separation**: Language-specific handler registries with shared event coordination

## Evolved Architecture Overview

### Event-Driven FFI Pattern

The Ruby worker implements a **pure event-driven architecture** that eliminates complex FFI patterns:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────────┐
│   Orchestration │    │  tasker-worker   │    │  Ruby Event-Driven │
│   System        │───▶│  Foundation      │───▶│  Worker             │
└─────────────────┘    └──────────────────┘    └─────────────────────┘
        │                        │                        │
        │                        │                        │
        ▼                        ▼                        ▼
   Task Readiness         WorkerProcessor           Ruby EventHandler
   Event System      →    Event Publisher     →     dry-events Bridge
                                │                        │
                                └────────────────────────┘
                              Global WorkerEventSystem
```

### Complete Event Flow

```
1. 🦀 Rust Worker listens for worker_{namespace}_queue messages
   ↓
2. 🦀 tasker-worker WorkerProcessor claims step via atomic SQL functions
   ↓
3. 🦀 Fires StepExecutionEvent via Global WorkerEventSystem
   ↓
4. 🌉 Ruby FFI Event Handler receives event (Rust → Ruby boundary)
   ↓
5. 🌉 FFI converts Rust event to Ruby hash via magnus/serde_magnus
   ↓
6. 💎 Ruby dry-events system receives converted event
   ↓
7. 💎 dry-events dispatches to registered Ruby step handlers
   ↓
8. 💎 Ruby handler executes business logic in native memory space
   ↓
9. 💎 Ruby handler publishes completion event to dry-events
   ↓
10. 🌉 dry-events subscriber converts Ruby result to FFI format
    ↓
11. 🌉 FFI calls back to Rust with StepExecutionCompletionEvent
    ↓
12. 🦀 Rust publishes completion to Global WorkerEventSystem
```

## Implementation Plan

### Phase 1: FFI Event Bridge Infrastructure

#### 1.1 Enhanced Magnus FFI Integration

**File: `workers/ruby/ext/tasker_core/src/lib.rs`**

```rust
use magnus::{Error, Ruby, Value};
use tasker_shared::events::{StepExecutionEvent, StepExecutionCompletionEvent};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("TaskerCore")?;
    let ffi_module = module.define_module("FFI")?;
    
    // Core FFI methods for event-driven processing
    ffi_module.define_singleton_method("receive_step_execution_event", receive_step_execution_event)?;
    ffi_module.define_singleton_method("send_step_completion_event", send_step_completion_event)?;
    
    // Bootstrap integration
    ffi_module.define_singleton_method("bootstrap_worker", bootstrap_worker_ffi)?;
    
    Ok(())
}

// Called by Rust when StepExecutionEvent is fired
fn receive_step_execution_event(ruby: &Ruby, event_data: Value) -> Result<(), Error> {
    // Convert Rust event to Ruby hash via conversions.rs
    let ruby_event_hash = convert_step_execution_event_to_ruby(event_data)?;
    
    // Get Ruby dry-events publisher and fire event
    let publisher = ruby.eval("TaskerCore::Worker::EventBridge.instance")?;
    publisher.funcall("publish_step_execution", (ruby_event_hash,))?;
    
    Ok(())
}

// Called by Ruby when step processing completes
fn send_step_completion_event(completion_data: Value) -> Result<(), Error> {
    // Convert Ruby completion to Rust event via conversions.rs
    let rust_completion = convert_ruby_completion_to_rust(completion_data)?;
    
    // Send to global event system
    publish_step_completion_to_global_event_system(rust_completion)?;
    
    Ok(())
}
```

#### 1.2 Enhanced FFI Data Conversions

**File: `workers/ruby/ext/tasker_core/src/conversions.rs`**

```rust
use magnus::{Error, RHash, Value};
use serde_magnus::{deserialize, serialize};
use tasker_shared::types::{StepExecutionEvent, StepExecutionCompletionEvent, TaskSequenceStep};

// Convert Rust StepExecutionEvent to Ruby hash
pub fn convert_step_execution_event_to_ruby(event: StepExecutionEvent) -> Result<RHash, Error> {
    let ruby = magnus::Ruby::get()?;
    
    let event_hash = ruby.hash_new();
    event_hash.aset("event_id", event.event_id.to_string())?;
    event_hash.aset("task_uuid", event.payload.task_uuid.to_string())?;
    event_hash.aset("step_uuid", event.payload.step_uuid.to_string())?;
    
    // Convert TaskSequenceStep to Ruby hash
    let task_sequence_step_hash = convert_task_sequence_step_to_ruby(&event.payload.task_sequence_step)?;
    event_hash.aset("task_sequence_step", task_sequence_step_hash)?;
    
    Ok(event_hash)
}

// Convert Ruby completion hash to Rust StepExecutionCompletionEvent
pub fn convert_ruby_completion_to_rust(ruby_hash: Value) -> Result<StepExecutionCompletionEvent, Error> {
    let hash: RHash = ruby_hash.try_convert()?;
    
    let event_id: String = hash.get("event_id").unwrap_or_default();
    let task_uuid: String = hash.get("task_uuid").unwrap_or_default();
    let step_uuid: String = hash.get("step_uuid").unwrap_or_default();
    let success: bool = hash.get("success").unwrap_or(false);
    let result_value: Value = hash.get("result").unwrap_or_default();
    
    // Convert Ruby result to serde_json::Value
    let result = serde_magnus::deserialize(result_value)?;
    
    Ok(StepExecutionCompletionEvent {
        event_id: event_id.parse().map_err(|_| Error::new(magnus::exception::arg_error(), "Invalid event_id UUID"))?,
        task_uuid: task_uuid.parse().map_err(|_| Error::new(magnus::exception::arg_error(), "Invalid task_uuid"))?,
        step_uuid: step_uuid.parse().map_err(|_| Error::new(magnus::exception::arg_error(), "Invalid step_uuid"))?,
        success,
        result,
        metadata: hash.get("metadata").and_then(|v| serde_magnus::deserialize(v).ok()),
        error_message: hash.get("error_message").and_then(|v| v.try_convert().ok()),
    })
}

// Convert TaskSequenceStep to Ruby hash with full fidelity
fn convert_task_sequence_step_to_ruby(step: &TaskSequenceStep) -> Result<RHash, Error> {
    let ruby = magnus::Ruby::get()?;
    let step_hash = ruby.hash_new();
    
    // Task data
    let task_hash = ruby.hash_new();
    task_hash.aset("task_uuid", step.task.task_uuid.to_string())?;
    task_hash.aset("context", serialize(&step.task.context)?)?;
    task_hash.aset("namespace_name", &step.task.namespace_name)?;
    step_hash.aset("task", task_hash)?;
    
    // Workflow step data
    let workflow_step_hash = ruby.hash_new();
    workflow_step_hash.aset("workflow_step_uuid", step.workflow_step.workflow_step_uuid.to_string())?;
    workflow_step_hash.aset("name", &step.workflow_step.name)?;
    step_hash.aset("workflow_step", workflow_step_hash)?;
    
    // Dependency results
    let dependency_results_hash = ruby.hash_new();
    for (key, value) in &step.dependency_results.results {
        dependency_results_hash.aset(key, serialize(value)?)?;
    }
    step_hash.aset("dependency_results", dependency_results_hash)?;
    
    // Step definition
    let step_definition_hash = ruby.hash_new();
    step_definition_hash.aset("name", &step.step_definition.name)?;
    
    let handler_hash = ruby.hash_new();
    handler_hash.aset("callable", &step.step_definition.handler.callable)?;
    handler_hash.aset("initialization", serialize(&step.step_definition.handler.initialization)?)?;
    step_definition_hash.aset("handler", handler_hash)?;
    
    step_hash.aset("step_definition", step_definition_hash)?;
    
    Ok(step_hash)
}
```

### Phase 2: Ruby Event-Driven Worker Infrastructure

#### 2.1 Worker Bootstrap Integration

**File: `workers/ruby/ext/tasker_core/src/bootstrap.rs`**

```rust
use crate::global_event_system::get_global_event_system;
use tasker_worker::{WorkerBootstrap, WorkerBootstrapConfig};
use anyhow::Result;

// Bootstrap worker with Ruby FFI event integration
pub async fn bootstrap_ruby_worker(config: WorkerBootstrapConfig) -> Result<()> {
    // Get the global event system (same pattern as Rust worker)
    let event_system = get_global_event_system();
    
    // Create Ruby event handler bridge
    let ruby_event_handler = RubyEventHandler::new(
        event_system.clone(),
        config.worker_id.clone(),
    );
    
    // Start Ruby event handler (subscribes to WorkerEventSystem)
    ruby_event_handler.start().await?;
    
    // Bootstrap tasker-worker foundation with shared event system
    let _worker_handle = WorkerBootstrap::bootstrap_with_event_system(
        config,
        Some(event_system),
    ).await?;
    
    Ok(())
}
```

#### 2.2 Ruby Event Handler Bridge

**File: `workers/ruby/ext/tasker_core/src/event_handler.rs`**

```rust
use anyhow::Result;
use magnus::{Error, Ruby};
use std::sync::Arc;
use tasker_shared::{
    events::{WorkerEventSubscriber, WorkerEventSystem},
    types::StepExecutionEvent,
};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Ruby Event Handler - bridges WorkerEventSystem to Ruby dry-events
pub struct RubyEventHandler {
    event_subscriber: Arc<WorkerEventSubscriber>,
    worker_id: String,
}

impl RubyEventHandler {
    pub fn new(event_system: Arc<WorkerEventSystem>, worker_id: String) -> Self {
        let event_system_cloned = (*event_system).clone();
        let event_subscriber = Arc::new(WorkerEventSubscriber::new(event_system_cloned));
        
        Self {
            event_subscriber,
            worker_id,
        }
    }
    
    pub async fn start(&self) -> Result<()> {
        info!(
            worker_id = %self.worker_id,
            "Starting Ruby FFI event handler - subscribing to step execution events"
        );
        
        let mut receiver = self.event_subscriber.subscribe_to_step_executions();
        let event_subscriber = self.event_subscriber.clone();
        let worker_id = self.worker_id.clone();
        
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        debug!(
                            worker_id = %worker_id,
                            event_id = %event.event_id,
                            step_name = %event.payload.task_sequence_step.workflow_step.name,
                            "Received step execution event - forwarding to Ruby"
                        );
                        
                        if let Err(e) = Self::forward_event_to_ruby(event).await {
                            error!(
                                worker_id = %worker_id,
                                error = %e,
                                "Failed to forward event to Ruby"
                            );
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(
                            worker_id = %worker_id,
                            lagged_count = count,
                            "Ruby event handler lagged behind"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!(
                            worker_id = %worker_id,
                            "Event channel closed - stopping Ruby event handler"
                        );
                        break;
                    }
                }
            }
        });
        
        Ok(())
    }
    
    async fn forward_event_to_ruby(event: StepExecutionEvent) -> Result<()> {
        // This calls into Ruby FFI layer which will publish to dry-events
        let ruby = unsafe { Ruby::get_unchecked() };
        
        // Convert event to Ruby format and call FFI method
        let ruby_event = crate::conversions::convert_step_execution_event_to_ruby(event)
            .map_err(|e| anyhow::anyhow!("Failed to convert event to Ruby: {}", e))?;
            
        ruby.funcall::<_, _, ()>("TaskerCore::FFI", "receive_step_execution_event", (ruby_event,))
            .map_err(|e| anyhow::anyhow!("Failed to call Ruby FFI method: {}", e))?;
        
        Ok(())
    }
}
```

### Phase 3: Ruby dry-events Integration

#### 3.1 Event Bridge and Publisher

**File: `workers/ruby/lib/tasker_core/worker/event_bridge.rb`**

```ruby
require 'dry-events'
require 'singleton'

module TaskerCore
  module Worker
    # Event bridge between Rust FFI and Ruby dry-events system
    class EventBridge
      include Singleton
      include Dry::Events::Publisher[:worker_events]
      
      attr_reader :logger, :subscriber_count
      
      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        @subscriber_count = 0
        setup_event_schema!
        logger.info "🔗 Ruby EventBridge initialized"
      end
      
      # Called by Rust FFI when StepExecutionEvent is received
      def publish_step_execution(event_data)
        logger.debug("📨 Received step execution event from Rust FFI")
        logger.debug("   Event ID: #{event_data['event_id']}")
        logger.debug("   Step: #{event_data.dig('task_sequence_step', 'workflow_step', 'name')}")
        
        # Publish to dry-events with Ruby-native event structure
        publish('step.execution.requested', {
          event_id: event_data['event_id'],
          task_uuid: event_data['task_uuid'],
          step_uuid: event_data['step_uuid'],
          task_sequence_step: TaskSequenceStepWrapper.new(event_data['task_sequence_step'])
        })
        
        logger.debug("✅ Step execution event published to dry-events")
      rescue StandardError => e
        logger.error("💥 Failed to publish step execution event: #{e.message}")
        logger.error("💥 #{e.backtrace.first(3).join("\n💥 ")}")
        raise
      end
      
      # Subscribe to step execution events
      def subscribe_to_step_execution(subscriber)
        subscribe('step.execution.requested', subscriber)
        @subscriber_count += 1
        logger.info("📋 Subscribed to step execution events (#{@subscriber_count} total subscribers)")
      end
      
      # Called by Ruby handlers when step processing completes
      def publish_step_completion(completion_data)
        logger.debug("📤 Publishing step completion to Rust FFI")
        logger.debug("   Event ID: #{completion_data[:event_id]}")
        logger.debug("   Success: #{completion_data[:success]}")
        
        # Convert Ruby completion to FFI format and call back to Rust
        TaskerCore::FFI.send_step_completion_event({
          event_id: completion_data[:event_id],
          task_uuid: completion_data[:task_uuid],
          step_uuid: completion_data[:step_uuid],
          success: completion_data[:success],
          result: completion_data[:result],
          metadata: completion_data[:metadata],
          error_message: completion_data[:error_message]
        })
        
        logger.debug("✅ Step completion sent to Rust FFI")
      rescue StandardError => e
        logger.error("💥 Failed to publish step completion: #{e.message}")
        logger.error("💥 #{e.backtrace.first(3).join("\n💥 ")}")
        raise
      end
      
      private
      
      def setup_event_schema!
        # Register event types with dry-events
        register_event('step.execution.requested')
        register_event('step.execution.completed')
        register_event('step.execution.failed')
        
        logger.debug("📋 Event schema registered with dry-events")
      end
    end
    
    # Wrapper for TaskSequenceStep data from FFI
    class TaskSequenceStepWrapper
      attr_reader :task, :workflow_step, :dependency_results, :step_definition
      
      def initialize(step_data)
        @task = TaskWrapper.new(step_data['task'])
        @workflow_step = WorkflowStepWrapper.new(step_data['workflow_step'])
        @dependency_results = DependencyResultsWrapper.new(step_data['dependency_results'])
        @step_definition = StepDefinitionWrapper.new(step_data['step_definition'])
      end
      
      # Utility methods for handler access
      def get_task_field(field_name)
        @task.context[field_name.to_s]
      end
      
      def get_dependency_result(step_name)
        @dependency_results.get_result(step_name)
      end
    end
    
    # Task data wrapper
    class TaskWrapper
      attr_reader :task_uuid, :context, :namespace_name
      
      def initialize(task_data)
        @task_uuid = task_data['task_uuid']
        @context = task_data['context'] || {}
        @namespace_name = task_data['namespace_name']
      end
    end
    
    # Workflow step wrapper
    class WorkflowStepWrapper
      attr_reader :workflow_step_uuid, :name
      
      def initialize(step_data)
        @workflow_step_uuid = step_data['workflow_step_uuid']
        @name = step_data['name']
      end
    end
    
    # Dependency results wrapper
    class DependencyResultsWrapper
      def initialize(results_data)
        @results = results_data || {}
      end
      
      def get_result(step_name)
        @results[step_name.to_s]
      end
      
      def [](step_name)
        get_result(step_name)
      end
    end
    
    # Step definition wrapper
    class StepDefinitionWrapper
      attr_reader :name, :handler
      
      def initialize(definition_data)
        @name = definition_data['name']
        @handler = HandlerWrapper.new(definition_data['handler'])
      end
    end
    
    # Handler configuration wrapper
    class HandlerWrapper
      attr_reader :callable, :initialization
      
      def initialize(handler_data)
        @callable = handler_data['callable']
        @initialization = handler_data['initialization'] || {}
      end
    end
  end
end
```

#### 3.2 Step Execution Subscriber

**File: `workers/ruby/lib/tasker_core/worker/step_execution_subscriber.rb`**

```ruby
module TaskerCore
  module Worker
    # Subscribes to step execution events and routes to handlers
    class StepExecutionSubscriber
      attr_reader :logger, :handler_registry, :stats
      
      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        @handler_registry = TaskerCore::Registry::HandlerRegistry.instance
        @stats = { processed: 0, succeeded: 0, failed: 0 }
        
        # Subscribe to step execution events
        TaskerCore::Worker::EventBridge.instance.subscribe_to_step_execution(self)
        logger.info "🎯 Step execution subscriber initialized"
      end
      
      # Called by dry-events when step execution is requested
      def call(event)
        event_data = event.payload
        step_data = event_data[:task_sequence_step]
        
        logger.info("🚀 Processing step execution request")
        logger.info("   Event ID: #{event_data[:event_id]}")
        logger.info("   Step: #{step_data.workflow_step.name}")
        logger.info("   Handler: #{step_data.step_definition.handler.callable}")
        
        @stats[:processed] += 1
        
        begin
          # Resolve step handler from registry
          handler = @handler_registry.resolve_handler(step_data.step_definition.handler.callable)
          
          unless handler
            raise TaskerCore::Error, "No handler found for #{step_data.step_definition.handler.callable}"
          end
          
          # Execute handler with step data
          result = handler.call(
            step_data.task,
            create_sequence_from_dependency_results(step_data.dependency_results),
            step_data.workflow_step
          )
          
          # Publish successful completion
          publish_step_completion(
            event_data: event_data,
            success: true,
            result: result.data,
            metadata: {
              processed_at: Time.now.utc.iso8601,
              processed_by: 'ruby_worker',
              handler_class: step_data.step_definition.handler.callable,
              duration_ms: result.metadata&.dig('duration_ms')
            }
          )
          
          @stats[:succeeded] += 1
          logger.info("✅ Step execution completed successfully")
          
        rescue StandardError => e
          logger.error("💥 Step execution failed: #{e.message}")
          logger.error("💥 #{e.backtrace.first(5).join("\n💥 ")}")
          
          # Publish failure completion
          publish_step_completion(
            event_data: event_data,
            success: false,
            result: nil,
            error_message: e.message,
            metadata: {
              failed_at: Time.now.utc.iso8601,
              failed_by: 'ruby_worker',
              error_class: e.class.name,
              handler_class: step_data.step_definition.handler.callable
            }
          )
          
          @stats[:failed] += 1
        end
      end
      
      private
      
      def publish_step_completion(event_data:, success:, result: nil, error_message: nil, metadata: nil)
        TaskerCore::Worker::EventBridge.instance.publish_step_completion({
          event_id: event_data[:event_id],
          task_uuid: event_data[:task_uuid],
          step_uuid: event_data[:step_uuid],
          success: success,
          result: result,
          metadata: metadata,
          error_message: error_message
        })
      end
      
      def create_sequence_from_dependency_results(dependency_results)
        TaskerCore::Types::Sequence.new(dependency_results.instance_variable_get(:@results) || {})
      end
    end
  end
end
```

### Phase 4: Simplified Handler Framework

#### 4.1 Enhanced Base Step Handler

**File: `workers/ruby/lib/tasker_core/step_handler/base.rb`**

```ruby
module TaskerCore
  module StepHandler
    # Simplified base class for Ruby step handlers
    # 
    # All infrastructure concerns (database, queues, state management) 
    # are handled by the Rust tasker-worker foundation. Ruby handlers
    # focus purely on business logic.
    class Base
      attr_reader :config, :logger
      
      def initialize(config: {})
        @config = config
        @logger = TaskerCore::Logging::Logger.instance
      end
      
      # Main execution method - pure business logic only
      # @param task [TaskWrapper] Task context and metadata
      # @param sequence [TaskerCore::Types::Sequence] Previous step results
      # @param step [WorkflowStepWrapper] Current step information
      # @return [TaskerCore::Types::StepHandlerCallResult] Execution result
      def call(task, sequence, step)
        raise NotImplementedError, "Subclasses must implement #call method"
      end
      
      protected
      
      # Create successful result
      def success(data = {}, metadata = nil)
        TaskerCore::Types::StepHandlerCallResult.success(
          data: data,
          metadata: metadata || default_metadata
        )
      end
      
      # Create retryable error result
      def retryable_error(message, details = {}, metadata = nil)
        TaskerCore::Types::StepHandlerCallResult.retryable_error(
          message: message,
          details: details,
          metadata: metadata || default_metadata
        )
      end
      
      # Create permanent error result
      def permanent_error(message, details = {}, metadata = nil)
        TaskerCore::Types::StepHandlerCallResult.permanent_error(
          message: message,
          details: details,
          metadata: metadata || default_metadata
        )
      end
      
      # Access sequence results by step name
      def sequence_results(sequence, step_name)
        sequence.get_result(step_name)
      end
      
      # Access task context values
      def task_context(task, key)
        task.context[key.to_s]
      end
      
      private
      
      def default_metadata
        {
          executed_at: Time.now.utc.iso8601,
          executed_by: self.class.name,
          ruby_version: RUBY_VERSION
        }
      end
    end
  end
end
```

#### 4.2 Simplified Handler Registry

**File: `workers/ruby/lib/tasker_core/registry/handler_registry.rb`**

```ruby
require 'singleton'

module TaskerCore
  module Registry
    # Simplified handler registry for pure business logic handlers
    class HandlerRegistry
      include Singleton
      
      attr_reader :logger, :handlers
      
      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        @handlers = {}
        bootstrap_handlers!
      end
      
      # Resolve handler by class name
      def resolve_handler(handler_class_name)
        handler_class = @handlers[handler_class_name]
        return nil unless handler_class
        
        # Simple instantiation - no infrastructure setup needed
        handler_class.new
      rescue StandardError => e
        logger.error("💥 Failed to instantiate handler #{handler_class_name}: #{e.message}")
        nil
      end
      
      # Register handler class
      def register_handler(class_name, handler_class)
        @handlers[class_name] = handler_class
        logger.debug("✅ Registered handler: #{class_name}")
      end
      
      # Check if handler is available
      def handler_available?(class_name)
        @handlers.key?(class_name)
      end
      
      # Get all registered handler names
      def registered_handlers
        @handlers.keys.sort
      end
      
      private
      
      def bootstrap_handlers!
        logger.info("🔧 Bootstrapping Ruby handler registry")
        
        registered_count = 0
        
        # Discover and register handlers from task templates
        discover_handler_classes.each do |handler_class_name|
          begin
            handler_class = Object.const_get(handler_class_name)
            register_handler(handler_class_name, handler_class)
            registered_count += 1
          rescue NameError
            logger.debug("⚠️ Handler class not found: #{handler_class_name}")
          rescue StandardError => e
            logger.warn("❌ Failed to register handler #{handler_class_name}: #{e.message}")
          end
        end
        
        logger.info("✅ Handler registry bootstrapped with #{registered_count} handlers")
      end
      
      def discover_handler_classes
        # This would typically scan task template configurations
        # For now, return known handler classes from examples
        [
          'LinearStep1Handler',
          'LinearStep2Handler', 
          'LinearStep3Handler',
          'LinearStep4Handler',
          'DiamondStartHandler',
          'DiamondBranchBHandler',
          'DiamondBranchCHandler',
          'DiamondEndHandler',
          'ValidateOrderHandler',
          'ReserveInventoryHandler',
          'ProcessPaymentHandler',
          'ShipOrderHandler'
        ]
      end
    end
  end
end
```

### Phase 5: Bootstrap Integration

#### 5.1 Ruby Worker Bootstrap

**File: `workers/ruby/lib/tasker_core/worker/bootstrap.rb`**

```ruby
module TaskerCore
  module Worker
    # Ruby worker bootstrap - integrates with Rust tasker-worker foundation
    class Bootstrap
      attr_reader :logger
      
      def initialize
        @logger = TaskerCore::Logging::Logger.instance
      end
      
      # Bootstrap Ruby worker with FFI integration
      def self.start!(worker_id: nil, enable_web_api: false)
        new.start!(worker_id: worker_id, enable_web_api: enable_web_api)
      end
      
      def start!(worker_id: nil, enable_web_api: false)
        worker_id ||= "ruby-worker-#{SecureRandom.hex(4)}"
        
        logger.info("🚀 Starting Ruby Worker with FFI integration")
        logger.info("   Worker ID: #{worker_id}")
        logger.info("   Web API: #{enable_web_api ? 'enabled' : 'disabled'}")
        
        # Step 1: Initialize Ruby event system
        initialize_event_system!
        
        # Step 2: Bootstrap Rust worker foundation via FFI
        bootstrap_rust_foundation!(worker_id, enable_web_api)
        
        # Step 3: Start event processing
        start_event_processing!
        
        logger.info("✅ Ruby Worker started successfully")
        
        # Keep process alive
        trap('INT') { shutdown! }
        trap('TERM') { shutdown! }
        
        # Event loop
        loop { sleep 1 }
      end
      
      private
      
      def initialize_event_system!
        logger.info("🔗 Initializing Ruby event system...")
        
        # Initialize EventBridge singleton
        TaskerCore::Worker::EventBridge.instance
        
        # Initialize step execution subscriber
        TaskerCore::Worker::StepExecutionSubscriber.new
        
        logger.info("✅ Ruby event system initialized")
      end
      
      def bootstrap_rust_foundation!(worker_id, enable_web_api)
        logger.info("🦀 Bootstrapping Rust tasker-worker foundation...")
        
        # Call Rust FFI to bootstrap worker with shared event system
        config = {
          worker_id: worker_id,
          enable_web_api: enable_web_api,
          event_driven_enabled: true,
          deployment_mode_hint: "Hybrid"
        }
        
        TaskerCore::FFI.bootstrap_worker(config)
        
        logger.info("✅ Rust foundation bootstrapped")
      end
      
      def start_event_processing!
        logger.info("📡 Starting event processing...")
        
        # Event processing is handled by the subscriber
        # which is already registered with EventBridge
        
        logger.info("✅ Event processing started")
      end
      
      def shutdown!
        logger.info("🛑 Shutting down Ruby Worker...")
        
        # Graceful shutdown logic here
        
        logger.info("✅ Ruby Worker shutdown complete")
        exit(0)
      end
    end
  end
end
```

### Phase 6: Updated Handler Examples

#### 6.1 Simplified Linear Workflow Handler

**File: `workers/ruby/spec/handlers/examples/linear_workflow/step_handlers/linear_step_1_handler.rb`**

```ruby
require_relative '../../../../../lib/tasker_core/step_handler/base'

# Linear Step 1 Handler - Pure business logic
class LinearStep1Handler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    logger.info("🔢 Executing Linear Step 1")
    
    # Extract input from task context
    even_number = task_context(task, 'even_number') || 6
    
    # Business logic - double the even number
    result = even_number * 2
    
    logger.info("   Input: #{even_number}")
    logger.info("   Output: #{result}")
    
    # Return success with data
    success({
      input_number: even_number,
      doubled_value: result,
      step_1_processed: true,
      processing_timestamp: Time.now.utc.iso8601
    })
  end
end
```

#### 6.2 Order Fulfillment Handler Example

**File: `workers/ruby/spec/handlers/examples/order_fulfillment/step_handlers/validate_order_handler.rb`**

```ruby
require_relative '../../../../../lib/tasker_core/step_handler/base'

# Validate Order Handler - Business validation logic
class ValidateOrderHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    logger.info("📋 Validating order")
    
    # Extract order data from task context
    customer_id = task_context(task, 'customer_id')
    items = task_context(task, 'items') || []
    
    # Validate customer
    unless customer_id && !customer_id.empty?
      return permanent_error("Customer ID is required", {
        field: 'customer_id',
        value: customer_id
      })
    end
    
    # Validate items
    if items.empty?
      return permanent_error("Order must contain at least one item", {
        field: 'items',
        count: items.length
      })
    end
    
    # Business validation logic
    total_amount = items.sum { |item| (item['price'] || 0) * (item['quantity'] || 0) }
    
    if total_amount <= 0
      return permanent_error("Order total must be greater than zero", {
        total_amount: total_amount
      })
    end
    
    # Validation successful
    success({
      customer_id: customer_id,
      validated_items: items,
      total_amount: total_amount,
      validation_timestamp: Time.now.utc.iso8601,
      validation_status: 'passed'
    })
  end
end
```

### Phase 7: Integration Testing

#### 7.1 FFI Integration Test

**File: `workers/ruby/spec/integration/ffi_event_integration_spec.rb`**

```ruby
require 'spec_helper'

RSpec.describe 'FFI Event Integration' do
  let(:event_bridge) { TaskerCore::Worker::EventBridge.instance }
  
  before(:each) do
    # Reset event bridge state
    allow(TaskerCore::FFI).to receive(:send_step_completion_event)
  end
  
  describe 'Step execution event flow' do
    it 'receives events from Rust FFI and processes them' do
      # Simulate step execution event from Rust
      event_data = {
        'event_id' => SecureRandom.uuid,
        'task_uuid' => SecureRandom.uuid,
        'step_uuid' => SecureRandom.uuid,
        'task_sequence_step' => {
          'task' => {
            'task_uuid' => SecureRandom.uuid,
            'context' => { 'even_number' => 10 },
            'namespace_name' => 'linear_workflow'
          },
          'workflow_step' => {
            'workflow_step_uuid' => SecureRandom.uuid,
            'name' => 'linear_step_1'
          },
          'dependency_results' => {},
          'step_definition' => {
            'name' => 'linear_step_1',
            'handler' => {
              'callable' => 'LinearStep1Handler',
              'initialization' => {}
            }
          }
        }
      }
      
      # Publish event (simulating FFI call)
      expect {
        event_bridge.publish_step_execution(event_data)
      }.not_to raise_error
      
      # Verify completion was sent back to FFI
      expect(TaskerCore::FFI).to have_received(:send_step_completion_event)
        .with(hash_including(
          event_id: event_data['event_id'],
          success: true
        ))
    end
    
    it 'handles handler execution errors gracefully' do
      # Create event data that will cause handler error
      event_data = {
        'event_id' => SecureRandom.uuid,
        'task_uuid' => SecureRandom.uuid,
        'step_uuid' => SecureRandom.uuid,
        'task_sequence_step' => {
          'step_definition' => {
            'handler' => {
              'callable' => 'NonExistentHandler'
            }
          }
        }
      }
      
      # Should handle error gracefully
      expect {
        event_bridge.publish_step_execution(event_data)
      }.not_to raise_error
      
      # Should send failure completion
      expect(TaskerCore::FFI).to have_received(:send_step_completion_event)
        .with(hash_including(
          success: false,
          error_message: a_string_including('No handler found')
        ))
    end
  end
end
```

## Success Criteria

### Code Simplification Metrics
- ✅ **80%+ Code Removal**: Infrastructure code eliminated from Ruby worker
- ✅ **Dependency Reduction**: Gemfile reduced to <10 essential gems (dry-events, dry-struct, dry-types)
- ✅ **File Count Reduction**: Ruby infrastructure files reduced by 75%+
- ✅ **Memory Footprint**: 90%+ reduction in Ruby worker memory usage

### Functional Preservation
- ✅ **Handler Compatibility**: All existing step handlers work with minimal changes
- ✅ **Event Flow**: Complete event flow from Rust → Ruby → Rust
- ✅ **Error Handling**: Comprehensive error propagation across FFI boundary
- ✅ **Configuration Compatibility**: Existing YAML task templates work unchanged

### FFI Integration Quality
- ✅ **Event-Driven Processing**: Zero dynamic method calls across FFI boundary
- ✅ **Memory Safety**: No complex memory management between languages
- ✅ **Performance**: <5% overhead compared to native Ruby execution
- ✅ **Thread Safety**: All FFI interactions are thread-safe

### Architecture Benefits
- ✅ **Clean Separation**: Pure business logic in Ruby, infrastructure in Rust
- ✅ **Simplified Deployment**: Ruby workers need no database/queue configuration
- ✅ **Faster Startup**: Ruby worker startup time reduced by 85%+
- ✅ **Easier Debugging**: Event-driven architecture simplifies troubleshooting

## Removed Components

The following Ruby infrastructure components are completely eliminated:

### Database Infrastructure
- `lib/tasker_core/database/` - All database connection and model management
- `lib/tasker_core/models.rb` - ActiveRecord model definitions
- Database migration and schema management

### Queue and Messaging
- `lib/tasker_core/messaging/` - PGMQ client and queue management
- Message processing and claiming logic
- Thread pool management for queue workers

### State Management
- `lib/tasker_core/orchestration/` - Task and step state management
- `lib/tasker_core/state_machine/` - State transition logic
- Finalization and result processing

### Configuration Management
- `lib/tasker_core/config/` - Complex TOML configuration system
- Environment-specific configuration loading
- Resource and connection pool configuration

### System Management
- `lib/tasker_core/internal/` - Orchestration manager
- Health monitoring and metrics collection
- Graceful shutdown coordination

## Risk Mitigation

### Technical Risks
- **FFI Complexity**: Comprehensive test coverage of all FFI boundary interactions
- **Event System Reliability**: Robust error handling and retry mechanisms
- **Data Serialization**: Careful validation of all Ruby ↔ Rust data conversions

### Migration Risks
- **Handler Compatibility**: Maintain compatibility shim during transition
- **Performance Regression**: Benchmark critical paths before/after migration
- **Operational Impact**: Detailed runbooks and rollback procedures

### Operational Risks
- **Memory Management**: Monitor for memory leaks across FFI boundary
- **Error Propagation**: Ensure all error scenarios are properly handled
- **Event Processing**: Monitor event queue depths and processing times

## Conclusion

TAS-42-evolved represents a fundamental architectural improvement that transforms the Ruby worker from a complex infrastructure-heavy implementation to a pure business logic layer. By leveraging the proven tasker-worker foundation through an event-driven FFI architecture, we achieve:

1. **Massive Simplification**: 80%+ code reduction while maintaining full functionality
2. **Performance Improvement**: Native Rust infrastructure with Ruby business logic
3. **Operational Excellence**: Simplified deployment and debugging
4. **Architectural Purity**: Clean separation of concerns across language boundaries

This approach establishes the definitive pattern for all future FFI language integrations (Python, WASM, etc.) while proving that the tasker-worker foundation is truly language-agnostic and production-ready.
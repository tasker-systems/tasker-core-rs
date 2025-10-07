# TAS-42-evolved-v2: Ruby Worker Event-Driven FFI Implementation (Enhanced)

## Executive Summary

This enhanced implementation plan provides a complete FFI bridge between Ruby and Rust for the tasker-worker system, addressing critical gaps in bootstrap patterns, system status management, and event handling. The architecture now properly leverages established patterns from the Rust worker implementation while providing Ruby-specific capabilities through magnus FFI bindings.

## Key Enhancements from Original Plan

1. **Unified Bootstrap Pattern**: Align with `workers/rust/src/bootstrap.rs` and `tasker-worker/src/bootstrap.rs`
2. **Bridge System Management**: Proper handle management for status checking and graceful shutdown
3. **Corrected Event Flow**: Eliminate circular event forwarding and properly bridge Ruby/Rust boundaries
4. **Complete FFI Bindings**: All required methods exposed with proper error handling

## Architecture Overview

### Core Components

```
┌─────────────────────────────────────────────────────────────────┐
│                         Ruby Process                             │
├─────────────────────────────────────────────────────────────────┤
│  Ruby Application Layer                                          │
│  ├── TaskerCore::Worker::Bootstrap (Ruby bootstrap orchestrator) │
│  ├── TaskerCore::Worker::EventBridge (dry-events integration)    │
│  └── Step Handlers (business logic)                             │
├─────────────────────────────────────────────────────────────────┤
│  Magnus FFI Bridge Layer                                         │
│  ├── BridgeHandle (system lifecycle management)                  │
│  ├── Event Conversions (Ruby ↔ Rust)                           │
│  └── Bootstrap Integration                                       │
├─────────────────────────────────────────────────────────────────┤
│  Rust Foundation Layer                                           │
│  ├── WorkerSystemHandle (from tasker-worker)                     │
│  ├── WorkerEventSystem (global singleton)                        │
│  └── WorkerCore (command processing)                             │
└─────────────────────────────────────────────────────────────────┘
```

### Event Flow Architecture

```
1. Step Execution Flow (Rust → Ruby):
   WorkerProcessor → WorkerEventSystem.publish() → RubyEventHandler.forward_to_ruby()
   → Magnus FFI → EventBridge.publish_step_execution() → dry-events → Handler

2. Step Completion Flow (Ruby → Rust):
   Handler → EventBridge.publish_step_completion() → Magnus FFI
   → send_step_completion_event() → WorkerEventSystem.publish_completion()
   → WorkerProcessor.handle_completion()
```

## Implementation Plan

### Phase 1: Enhanced Bridge Infrastructure

#### 1.1 Bridge Handle Management

**File: `workers/ruby/ext/tasker_core/src/bridge.rs`**

```rust
//! # Ruby FFI Bridge with System Handle Management
//!
//! Provides lifecycle management for the worker system with status checking
//! and graceful shutdown capabilities, following patterns from embedded mode.

use magnus::{function, prelude::*, Error, RHash, RModule, Value};
use std::sync::{Arc, Mutex};
use tasker_worker::{WorkerSystemHandle, WorkerSystemStatus};
use tracing::{error, info};

/// Global handle to the worker system for Ruby FFI
static WORKER_SYSTEM: Mutex<Option<RubyBridgeHandle>> = Mutex::new(None);

/// Bridge handle that maintains worker system state
pub struct RubyBridgeHandle {
    /// Handle from tasker-worker bootstrap
    system_handle: WorkerSystemHandle,
    /// Ruby event handler for forwarding events
    event_handler: Arc<RubyEventHandler>,
    /// Keep runtime alive
    _runtime: tokio::runtime::Runtime,
}

impl RubyBridgeHandle {
    pub fn new(
        system_handle: WorkerSystemHandle,
        event_handler: Arc<RubyEventHandler>,
        runtime: tokio::runtime::Runtime,
    ) -> Self {
        Self {
            system_handle,
            event_handler,
            _runtime: runtime,
        }
    }

    pub fn is_running(&self) -> bool {
        self.system_handle.is_running()
    }

    pub async fn status(&self) -> WorkerSystemStatus {
        self.system_handle.status().await
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.system_handle.stop().map_err(|e| e.to_string())
    }

    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        &self.system_handle.runtime_handle
    }

    pub fn event_handler(&self) -> &Arc<RubyEventHandler> {
        &self.event_handler
    }
}

/// Initialize the bridge module with all FFI functions
pub fn init_bridge(module: &RModule) -> Result<(), Error> {
    info!("🔌 Initializing Ruby FFI bridge");

    // Bootstrap and lifecycle
    module.define_singleton_method(
        "bootstrap_worker",
        function!(bootstrap_worker, 1),
    )?;
    module.define_singleton_method(
        "stop_worker",
        function!(stop_worker, 0),
    )?;
    module.define_singleton_method(
        "worker_status",
        function!(get_worker_status, 0),
    )?;
    module.define_singleton_method(
        "transition_to_graceful_shutdown",
        function!(transition_to_graceful_shutdown, 0),
    )?;

    // Event handling
    module.define_singleton_method(
        "send_step_completion_event",
        function!(send_step_completion_event, 1),
    )?;

    info!("✅ Ruby FFI bridge initialized");
    Ok(())
}
```

#### 1.2 Bootstrap Implementation

**File: `workers/ruby/ext/tasker_core/src/bootstrap.rs`**

```rust
//! # Ruby Worker Bootstrap
//!
//! Follows the same patterns as workers/rust/src/bootstrap.rs but adapted
//! for Ruby FFI integration with magnus.

use crate::{
    bridge::{RubyBridgeHandle, WORKER_SYSTEM},
    event_handler::RubyEventHandler,
    global_event_system::get_global_event_system,
};
use anyhow::Result;
use magnus::{Error, RHash, Value};
use std::sync::Arc;
use tasker_worker::{WorkerBootstrap, WorkerBootstrapConfig};
use tracing::{error, info};

/// Bootstrap the worker system for Ruby
///
/// This follows the same pattern as the Rust worker bootstrap but stores
/// the handle in a global static for Ruby access.
///
/// # Arguments
/// * `config` - Ruby hash with bootstrap configuration
pub fn bootstrap_worker(config: Value) -> Result<String, Error> {
    let config_hash: RHash = config.try_convert()?;

    // Extract configuration from Ruby hash
    let worker_id: String = config_hash
        .fetch("worker_id")
        .unwrap_or_else(|_| Value::from(format!("ruby-worker-{}", uuid::Uuid::new_v4())))
        .try_convert()?;

    let enable_web_api: bool = config_hash
        .fetch("enable_web_api")
        .unwrap_or_else(|_| Value::from(false))
        .try_convert()?;

    let event_driven_enabled: bool = config_hash
        .fetch("event_driven_enabled")
        .unwrap_or_else(|_| Value::from(true))
        .try_convert()?;

    let deployment_mode: String = config_hash
        .fetch("deployment_mode")
        .unwrap_or_else(|_| Value::from("Hybrid"))
        .try_convert()?;

    // Check if already running
    let mut handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    if handle_guard.is_some() {
        return Ok("Worker system already running".to_string());
    }

    info!("🚀 Starting Ruby worker bootstrap with configuration:");
    info!("  worker_id: {}", worker_id);
    info!("  enable_web_api: {}", enable_web_api);
    info!("  event_driven: {}", event_driven_enabled);
    info!("  deployment_mode: {}", deployment_mode);

    // Create tokio runtime
    let runtime = tokio::runtime::Runtime::new().map_err(|e| {
        error!("Failed to create tokio runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    // Get global event system (shared singleton)
    let event_system = get_global_event_system();

    // Create Ruby event handler that will forward events to Ruby
    let ruby_event_handler = Arc::new(RubyEventHandler::new(
        event_system.clone(),
        worker_id.clone(),
    ));

    // Bootstrap within runtime context
    let (system_handle, event_handler) = runtime.block_on(async {
        // Start the Ruby event handler (subscribes to events)
        ruby_event_handler.start().await.map_err(|e| {
            error!("Failed to start Ruby event handler: {}", e);
            Error::new(
                magnus::exception::runtime_error(),
                format!("Event handler start failed: {}", e),
            )
        })?;

        info!("✅ Ruby event handler started and subscribed to events");

        // Create bootstrap config
        let bootstrap_config = WorkerBootstrapConfig {
            worker_id: worker_id.clone(),
            enable_web_api,
            event_driven_enabled,
            deployment_mode_hint: Some(deployment_mode),
            ..Default::default()
        };

        // Bootstrap the worker using tasker-worker foundation
        let handle = WorkerBootstrap::bootstrap_with_event_system(
            Some(event_system),
        ).await.map_err(|e| {
            error!("Failed to bootstrap worker system: {}", e);
            Error::new(
                magnus::exception::runtime_error(),
                format!("Worker bootstrap failed: {}", e),
            )
        })?;

        info!("✅ Worker system bootstrapped successfully");

        Ok::<_, Error>((handle, ruby_event_handler))
    })?;

    // Store the bridge handle
    *handle_guard = Some(RubyBridgeHandle::new(
        system_handle,
        event_handler,
        runtime,
    ));

    Ok("Ruby worker system started successfully".to_string())
}

/// Stop the worker system
pub fn stop_worker() -> Result<String, Error> {
    let mut handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    match handle_guard.as_mut() {
        Some(handle) => {
            handle.stop().map_err(|e| {
                error!("Failed to stop worker system: {}", e);
                Error::new(magnus::exception::runtime_error(), e)
            })?;
            *handle_guard = None;
            Ok("Worker system stopped".to_string())
        }
        None => Ok("Worker system not running".to_string()),
    }
}

/// Get worker system status
pub fn get_worker_status() -> Result<Value, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let ruby = magnus::Ruby::get()?;
    let hash = ruby.hash_new();

    match handle_guard.as_ref() {
        Some(handle) => {
            let runtime = handle.runtime_handle();
            let status = runtime.block_on(async {
                handle.status().await
            });

            hash.aset("running", status.running)?;
            hash.aset("environment", status.environment)?;
            hash.aset("worker_core_status", format!("{:?}", status.worker_core_status))?;
            hash.aset("web_api_enabled", status.web_api_enabled)?;
            hash.aset("supported_namespaces", status.supported_namespaces)?;
            hash.aset("database_pool_size", status.database_pool_size)?;
            hash.aset("database_pool_idle", status.database_pool_idle)?;
        }
        None => {
            hash.aset("running", false)?;
            hash.aset("error", "Worker system not initialized")?;
        }
    }

    Ok(hash.as_value())
}

/// Transition to graceful shutdown
pub fn transition_to_graceful_shutdown() -> Result<String, Error> {
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(
            magnus::exception::runtime_error(),
            "Worker system not running",
        )
    })?;

    let runtime = handle.runtime_handle();
    runtime.block_on(async {
        handle.system_handle.worker_core.stop().await.map_err(|e| {
            error!("Failed to transition to graceful shutdown: {}", e);
            Error::new(
                magnus::exception::runtime_error(),
                format!("Graceful shutdown failed: {}", e),
            )
        })
    })?;

    Ok("Worker system transitioned to graceful shutdown".to_string())
}
```

#### 1.3 Corrected Event Handler

**File: `workers/ruby/ext/tasker_core/src/event_handler.rs`**

```rust
//! # Ruby Event Handler
//!
//! Bridges WorkerEventSystem to Ruby dry-events without circular dependencies.
//! Events flow: Rust → Ruby for execution, Ruby → Rust for completion.

use anyhow::Result;
use magnus::{Error as MagnusError, Ruby, Value as RValue};
use std::sync::Arc;
use tasker_shared::{
    events::{WorkerEventSubscriber, WorkerEventSystem},
    types::{StepExecutionCompletionEvent, StepExecutionEvent},
};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Ruby Event Handler - forwards Rust events to Ruby
pub struct RubyEventHandler {
    event_subscriber: Arc<WorkerEventSubscriber>,
    worker_id: String,
}

impl RubyEventHandler {
    pub fn new(event_system: Arc<WorkerEventSystem>, worker_id: String) -> Self {
        let event_subscriber = Arc::new(WorkerEventSubscriber::new((*event_system).clone()));
        Self {
            event_subscriber,
            worker_id,
        }
    }

    pub async fn start(&self) -> Result<()> {
        info!(
            worker_id = %self.worker_id,
            "Starting Ruby event handler - subscribing to step execution events"
        );

        let mut receiver = self.event_subscriber.subscribe_to_step_executions();

        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        debug!(
                            event_id = %event.event_id,
                            step_name = %event.payload.task_sequence_step.workflow_step.name,
                            "Received step execution event - forwarding to Ruby"
                        );

                        if let Err(e) = Self::forward_event_to_ruby(event).await {
                            error!(
                                error = %e,
                                "Failed to forward event to Ruby"
                            );
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(lagged_count = count, "Ruby event handler lagged behind");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Event channel closed - stopping Ruby event handler");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn forward_event_to_ruby(event: StepExecutionEvent) -> Result<()> {
        // Get Ruby runtime context
        let ruby = unsafe { Ruby::get_unchecked() };

        // Convert event to Ruby hash
        let ruby_event = crate::conversions::convert_step_execution_event_to_ruby(event)
            .map_err(|e| anyhow::anyhow!("Failed to convert event to Ruby: {}", e))?;

        // Call directly into Ruby EventBridge to publish event
        // This avoids the circular dependency of calling back through FFI
        let event_bridge: RValue = ruby
            .eval("TaskerCore::Worker::EventBridge.instance")
            .map_err(|e| anyhow::anyhow!("Failed to get EventBridge instance: {}", e))?;

        event_bridge
            .funcall("publish_step_execution", (ruby_event,))
            .map_err(|e| anyhow::anyhow!("Failed to publish event to Ruby: {}", e))?;

        Ok(())
    }

    /// Handle completion event from Ruby and forward to Rust event system
    pub fn handle_completion(&self, completion: StepExecutionCompletionEvent) -> Result<()> {
        // Get the global event system
        let event_system = crate::global_event_system::get_global_event_system();

        // Publish completion to the event system
        event_system.publish_step_completion(completion)?;

        Ok(())
    }
}
```

#### 1.4 Enhanced FFI Library

**File: `workers/ruby/ext/tasker_core/src/lib.rs`**

```rust
use magnus::{function, value::ReprValue, Error, Module, Object, Ruby, Value as RValue};

mod bootstrap;
mod bridge;
mod conversions;
mod event_handler;
mod ffi_logging;
mod global_event_system;

use bootstrap::{
    bootstrap_worker, get_worker_status, stop_worker, transition_to_graceful_shutdown,
};
use conversions::convert_ruby_completion_to_rust;

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // Initialize logging
    ffi_logging::init_ruby_logger()?;

    let module = ruby.define_module("TaskerCore")?;
    let ffi_class = module.define_class("FFI", ruby.class_object())?;

    // Initialize bridge with all lifecycle methods
    bridge::init_bridge(&ffi_class)?;

    Ok(())
}

/// Called by Ruby when step processing completes
/// This properly sends the completion event to the global event system
fn send_step_completion_event(completion_data: RValue) -> Result<(), Error> {
    // Convert Ruby completion to Rust event
    let rust_completion = convert_ruby_completion_to_rust(completion_data)?;

    // Get the bridge handle to access the event handler
    let handle_guard = bridge::WORKER_SYSTEM.lock().map_err(|e| {
        Error::new(
            magnus::exception::runtime_error(),
            format!("Failed to acquire lock: {}", e),
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(
            magnus::exception::runtime_error(),
            "Worker system not initialized",
        )
    })?;

    // Use the event handler to publish the completion
    handle
        .event_handler()
        .handle_completion(rust_completion)
        .map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to publish completion: {}", e),
            )
        })?;

    Ok(())
}
```

### Phase 2: Ruby Integration Layer

#### 2.1 Ruby Bootstrap Orchestrator

**File: `workers/ruby/lib/tasker_core/worker/bootstrap.rb`**

```ruby
module TaskerCore
  module Worker
    # Bootstrap orchestrator for Ruby worker
    # Manages initialization of both Rust foundation and Ruby components
    class Bootstrap
      include Singleton

      attr_reader :logger, :config, :status

      def initialize
        @logger = Logger.new(STDOUT)
        @logger.level = ENV['LOG_LEVEL'] || Logger::INFO
        @status = :initialized
        @shutdown_handlers = []
      end

      # Start the worker system with optional configuration
      def self.start!(config = {})
        instance.start!(config)
      end

      # Main bootstrap method
      def start!(config = {})
        @config = default_config.merge(config)

        logger.info "🚀 Starting Ruby worker bootstrap"
        logger.info "Configuration: #{@config.inspect}"

        # Initialize Ruby components first
        initialize_ruby_components!

        # Bootstrap Rust foundation via FFI
        bootstrap_rust_foundation!

        # Start event processing
        start_event_processing!

        # Register shutdown handlers
        register_shutdown_handlers!

        @status = :running
        logger.info "✅ Ruby worker system started successfully"

        self
      rescue StandardError => e
        logger.error "Failed to start worker: #{e.message}"
        logger.error e.backtrace.join("\n")
        shutdown!
        raise
      end

      # Check if worker is running
      def running?
        @status == :running && rust_worker_running?
      end

      # Get comprehensive status
      def status
        rust_status = TaskerCore::FFI.worker_status
        ruby_status = {
          status: @status,
          event_bridge_active: EventBridge.instance.active?,
          handler_registry_size: Registry::HandlerRegistry.instance.handlers.size,
          subscriber_active: @step_subscriber&.active? || false
        }

        rust_status.merge(ruby_status)
      rescue StandardError => e
        logger.error "Failed to get status: #{e.message}"
        { error: e.message, status: @status }
      end

      # Graceful shutdown
      def shutdown!
        return if @status == :stopped

        logger.info "🛑 Initiating graceful shutdown"
        @status = :shutting_down

        # Transition Rust to graceful shutdown first
        begin
          TaskerCore::FFI.transition_to_graceful_shutdown
        rescue StandardError => e
          logger.error "Failed to transition to graceful shutdown: #{e.message}"
        end

        # Stop Ruby components
        @step_subscriber&.stop!
        EventBridge.instance.stop!

        # Stop Rust worker
        begin
          TaskerCore::FFI.stop_worker
        rescue StandardError => e
          logger.error "Failed to stop Rust worker: #{e.message}"
        end

        # Run custom shutdown handlers
        @shutdown_handlers.each do |handler|
          handler.call rescue nil
        end

        @status = :stopped
        logger.info "✅ Worker shutdown complete"
      end

      # Register custom shutdown handler
      def on_shutdown(&block)
        @shutdown_handlers << block if block_given?
      end

      private

      def default_config
        {
          worker_id: "ruby-worker-#{SecureRandom.uuid}",
          enable_web_api: false,
          event_driven_enabled: true,
          deployment_mode: 'Hybrid',
          namespaces: detect_namespaces
        }
      end

      def detect_namespaces
        # Auto-detect from registered handlers
        Registry::HandlerRegistry.instance.registered_handlers.map do |handler_class|
          handler_class.namespace if handler_class.respond_to?(:namespace)
        end.compact.uniq
      end

      def initialize_ruby_components!
        logger.info "Initializing Ruby components..."

        # Initialize event bridge
        EventBridge.instance

        # Initialize handler registry
        Registry::HandlerRegistry.instance.bootstrap_handlers!

        # Initialize step execution subscriber
        @step_subscriber = StepExecutionSubscriber.new(
          logger: logger,
          handler_registry: Registry::HandlerRegistry.instance
        )

        logger.info "✅ Ruby components initialized"
      end

      def bootstrap_rust_foundation!
        logger.info "Bootstrapping Rust worker foundation..."

        result = TaskerCore::FFI.bootstrap_worker(@config)
        logger.info "Rust bootstrap result: #{result}"

        # Verify it's running
        status = TaskerCore::FFI.worker_status
        unless status[:running]
          raise "Rust worker failed to start: #{status.inspect}"
        end

        logger.info "✅ Rust foundation bootstrapped"
      end

      def start_event_processing!
        logger.info "Starting event processing..."

        # Subscribe to step execution events
        EventBridge.instance.subscribe_to_step_execution do |event|
          @step_subscriber.call(event)
        end

        logger.info "✅ Event processing started"
      end

      def register_shutdown_handlers!
        # Graceful shutdown on signals
        %w[INT TERM].each do |signal|
          Signal.trap(signal) do
            Thread.new { shutdown! }
          end
        end

        # Shutdown on exit
        at_exit { shutdown! if running? }
      end

      def rust_worker_running?
        status = TaskerCore::FFI.worker_status
        status[:running] == true
      rescue
        false
      end
    end
  end
end
```

#### 2.2 Event Bridge with Proper Boundaries

**File: `workers/ruby/lib/tasker_core/worker/event_bridge.rb`**

```ruby
module TaskerCore
  module Worker
    # Event bridge between Rust and Ruby using dry-events
    # Handles StepExecutionEvent (from Rust) and StepExecutionCompletionEvent (to Rust)
    class EventBridge
      include Singleton
      include Dry::Events::Publisher[:tasker_core]

      attr_reader :logger, :active

      def initialize
        @logger = Logger.new(STDOUT)
        @logger.level = ENV['LOG_LEVEL'] || Logger::DEBUG
        @active = true

        setup_event_schema!
      end

      # Check if bridge is active
      def active?
        @active
      end

      # Stop the event bridge
      def stop!
        @active = false
        logger.info "Event bridge stopped"
      end

      # Called by Rust FFI when StepExecutionEvent is received
      # This is the entry point for events from Rust
      def publish_step_execution(event_data)
        return unless active?

        logger.debug "Publishing step execution event: #{event_data[:event_id]}"

        # Wrap the raw data in accessor objects for easier use
        wrapped_event = wrap_step_execution_event(event_data)

        # Publish to dry-events subscribers (Ruby handlers)
        publish('step.execution.received', wrapped_event)

        logger.debug "Step execution event published to #{subscriptions('step.execution.received').count} subscribers"
      rescue StandardError => e
        logger.error "Failed to publish step execution: #{e.message}"
        logger.error e.backtrace.join("\n")
        raise
      end

      # Subscribe to step execution events (used by StepExecutionSubscriber)
      def subscribe_to_step_execution(&block)
        subscribe('step.execution.received', &block)
      end

      # Send completion event back to Rust
      # Called by StepExecutionSubscriber after handler execution
      def publish_step_completion(completion_data)
        return unless active?

        logger.debug "Sending step completion to Rust: #{completion_data[:event_id]}"

        # Validate completion data
        validate_completion!(completion_data)

        # Send to Rust via FFI
        TaskerCore::FFI.send_step_completion_event(completion_data)

        # Also publish locally for monitoring/debugging
        publish('step.completion.sent', completion_data)

        logger.debug "Step completion sent to Rust"
      rescue StandardError => e
        logger.error "Failed to send step completion: #{e.message}"
        logger.error e.backtrace.join("\n")
        raise
      end

      private

      def setup_event_schema!
        # Register event types
        register_event('step.execution.received')
        register_event('step.completion.sent')
        register_event('bridge.error')
      end

      def wrap_step_execution_event(event_data)
        {
          event_id: event_data[:event_id],
          task_uuid: event_data[:task_uuid],
          step_uuid: event_data[:step_uuid],
          task_sequence_step: TaskSequenceStepWrapper.new(event_data[:task_sequence_step])
        }
      end

      def validate_completion!(completion_data)
        required_fields = [:event_id, :task_uuid, :step_uuid, :success]
        missing_fields = required_fields - completion_data.keys

        if missing_fields.any?
          raise ArgumentError, "Missing required fields in completion: #{missing_fields.join(', ')}"
        end

        # Ensure metadata is a hash
        completion_data[:metadata] ||= {}

        # Ensure timestamps
        completion_data[:completed_at] ||= Time.now.utc.iso8601
      end
    end
  end
end
```

### Phase 3: Testing and Validation

#### 3.1 Integration Test Suite

**File: `workers/ruby/spec/integration/ffi_bridge_integration_spec.rb`**

```ruby
require 'spec_helper'

RSpec.describe 'FFI Bridge Integration' do
  let(:bootstrap) { TaskerCore::Worker::Bootstrap.instance }

  describe 'Bootstrap and lifecycle' do
    it 'successfully bootstraps the worker system' do
      # Start the worker
      bootstrap.start!(
        worker_id: 'test-worker-001',
        event_driven_enabled: true,
        deployment_mode: 'Hybrid'
      )

      # Verify it's running
      expect(bootstrap.running?).to be true

      # Check status
      status = bootstrap.status
      expect(status[:running]).to be true
      expect(status[:environment]).to eq('test')
      expect(status[:event_bridge_active]).to be true

      # Graceful shutdown
      bootstrap.shutdown!
      expect(bootstrap.running?).to be false
    end
  end

  describe 'Event flow integration' do
    before do
      bootstrap.start!
    end

    after do
      bootstrap.shutdown!
    end

    it 'processes step execution events from Rust to Ruby' do
      events_received = []

      # Subscribe to events
      TaskerCore::Worker::EventBridge.instance.subscribe_to_step_execution do |event|
        events_received << event
      end

      # Create a test task that will trigger events
      # (This assumes the Rust worker will process and emit events)
      create_test_task

      # Wait for events
      sleep 1

      expect(events_received).not_to be_empty
      event = events_received.first

      expect(event[:event_id]).to be_present
      expect(event[:task_sequence_step]).to be_a(TaskSequenceStepWrapper)
    end

    it 'sends completion events from Ruby to Rust' do
      completion_sent = false

      # Monitor completion events
      TaskerCore::Worker::EventBridge.instance.subscribe('step.completion.sent') do |event|
        completion_sent = true
      end

      # Send a completion
      completion_data = {
        event_id: SecureRandom.uuid,
        task_uuid: SecureRandom.uuid,
        step_uuid: SecureRandom.uuid,
        success: true,
        result: { value: 42 },
        metadata: { handler: 'test' }
      }

      TaskerCore::Worker::EventBridge.instance.publish_step_completion(completion_data)

      expect(completion_sent).to be true
    end
  end

  describe 'Error handling' do
    it 'handles bootstrap failures gracefully' do
      # Attempt to start twice
      bootstrap.start!

      expect {
        TaskerCore::FFI.bootstrap_worker({ worker_id: 'duplicate' })
      }.to raise_error(/already running/)

      bootstrap.shutdown!
    end

    it 'handles event processing errors' do
      bootstrap.start!

      # Send invalid completion
      expect {
        TaskerCore::Worker::EventBridge.instance.publish_step_completion({})
      }.to raise_error(ArgumentError, /Missing required fields/)

      bootstrap.shutdown!
    end
  end
end
```

## Success Criteria

### Functional Requirements
- ✅ Bootstrap aligns with established patterns from Rust worker
- ✅ Bridge provides system status and graceful shutdown capabilities
- ✅ Events flow correctly: Rust→Ruby for execution, Ruby→Rust for completion
- ✅ No circular event forwarding or unnecessary indirection
- ✅ All FFI methods properly exposed and error handled

### Performance Requirements
- Event forwarding latency < 1ms
- Zero memory leaks during long-running operations
- Proper cleanup on shutdown
- Thread-safe operations across FFI boundary

### Architectural Requirements
- Follows established patterns from `tasker-worker/src/bootstrap.rs`
- Maintains separation of concerns between layers
- Provides comprehensive status and monitoring capabilities
- Supports all deployment modes (Polling, EventDriven, Hybrid)

## Implementation Checklist

### Rust FFI Components
- [ ] `bridge.rs` - System handle management
- [ ] `bootstrap.rs` - Worker bootstrap with event system
- [ ] `event_handler.rs` - Corrected event forwarding
- [ ] `lib.rs` - Complete FFI bindings
- [ ] `global_event_system.rs` - Shared event system singleton

### Ruby Components
- [ ] `bootstrap.rb` - Ruby bootstrap orchestrator
- [ ] `event_bridge.rb` - dry-events integration
- [ ] `step_execution_subscriber.rb` - Handler execution
- [ ] `handler_registry.rb` - Handler management
- [ ] Integration tests

### Documentation
- [ ] API documentation for all FFI methods
- [ ] Event flow diagrams
- [ ] Deployment guide
- [ ] Troubleshooting guide

## Conclusion

This enhanced implementation plan provides a complete, production-ready FFI bridge between Ruby and Rust for the tasker-worker system. By following established patterns from the Rust worker implementation and properly managing the system lifecycle through bridge handles, we achieve a robust integration that maintains the benefits of both languages while providing seamless interoperability.

The corrected event flow eliminates unnecessary indirection and ensures that events flow naturally from Rust to Ruby for execution and from Ruby to Rust for completion tracking, all while maintaining proper separation of concerns and error handling throughout the stack.

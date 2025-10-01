# TAS-43 Worker Event System: Unified EventDrivenSystem Architecture

## Problem Statement

The current worker event-driven architecture in `tasker-worker/src/worker/event_driven_processor.rs` is a monolithic implementation that doesn't leverage the unified EventDrivenSystem patterns we established for orchestration. This creates several issues:

1. **Architectural Inconsistency**: Workers use direct pgmq-notify integration while orchestration uses the EventDrivenSystem trait
2. **Code Duplication**: Similar patterns (listener + fallback polling) are implemented differently across worker and orchestration
3. **Limited Deployment Mode Support**: Current implementation doesn't cleanly support PollingOnly/Hybrid/EventDrivenOnly modes
4. **Testing Complexity**: Monolithic structure makes unit testing and mocking difficult
5. **Maintenance Burden**: Different patterns increase complexity for future enhancements

## Solution: Unified EventDrivenSystem Architecture for Workers

Apply the same modular EventDrivenSystem architecture we successfully implemented for orchestration to the worker system, creating consistency across the entire tasker-core ecosystem.

## Current State Analysis

### Existing Worker Components

**event_driven_processor.rs (550 lines):**
- Direct pgmq-notify integration via PgmqNotifyClient and PgmqNotifyListener
- Namespace-aware message processing with TaskTemplateManager integration
- Fallback polling with configurable intervals
- Message event handling through MessageEventHandler
- Integration with WorkerCommand pattern

**command_processor.rs (934 lines):**
- TAS-40 command pattern implementation
- WorkerCommand enum with comprehensive command types
- Integration with WorkerProcessor for step execution
- Event integration support (FFI communication)
- Proper error handling and statistics tracking

**Integration Pattern:**
```rust
// Current flow
pgmq-notify → MessageEventHandler → WorkerCommand::ExecuteStep → WorkerProcessor
```

## Target Architecture: Modular EventDrivenSystem

### File Structure (Following Orchestration Patterns)

```
tasker-worker/src/worker/
├── event_systems/
│   ├── mod.rs
│   └── worker_event_system.rs          # EventDrivenSystem implementation
├── worker_queues/                      # Mirror of orchestration_queues
│   ├── mod.rs
│   ├── events.rs                       # WorkerQueueEvent definitions
│   ├── listener.rs                     # pgmq-notify listener for worker namespace queues
│   └── fallback_poller.rs              # Polling fallback for worker namespace queues
├── command_processor.rs                # Enhanced for event integration
└── event_driven_processor.rs          # Refactored to use new components
```

### Core Components

#### 1. WorkerEventSystem (worker_event_system.rs)

Implements the EventDrivenSystem trait for worker namespace queue processing:

```rust
use async_trait::async_trait;
use tasker_shared::{EventDrivenSystem, DeploymentMode, SystemStatistics};

pub struct WorkerEventSystem {
    /// System identifier
    system_id: String,
    /// Current deployment mode
    deployment_mode: DeploymentMode,
    /// Worker namespace queue listener
    queue_listener: Option<WorkerQueueListener>,
    /// Fallback poller for reliability
    fallback_poller: Option<WorkerFallbackPoller>,
    /// Worker command sender
    command_sender: mpsc::Sender<WorkerCommand>,
    /// System configuration
    config: WorkerEventSystemConfig,
    /// Runtime statistics
    statistics: Arc<WorkerEventStatistics>,
    /// Running state
    is_running: AtomicBool,
}

#[async_trait]
impl EventDrivenSystem for WorkerEventSystem {
    type SystemId = String;
    type Event = WorkerQueueEvent;
    type Config = WorkerEventSystemConfig;
    type Statistics = WorkerEventStatistics;

    async fn start(&mut self) -> Result<(), DeploymentModeError> {
        match self.deployment_mode {
            DeploymentMode::EventDrivenOnly => {
                self.start_event_driven_processing().await?;
            }
            DeploymentMode::Hybrid => {
                self.start_event_driven_processing().await?;
                self.start_fallback_polling().await?;
            }
            DeploymentMode::PollingOnly => {
                self.start_fallback_polling().await?;
            }
        }
        Ok(())
    }

    async fn process_event(&self, event: Self::Event) -> Result<(), DeploymentModeError> {
        match event {
            WorkerQueueEvent::StepMessage(message_event) => {
                self.handle_step_message_event(message_event).await
            }
        }
    }
}
```

#### 2. Worker Queue Components (worker_queues/)

**events.rs - WorkerQueueEvent Definitions:**
```rust
use pgmq::Message as PgmqMessage;
use pgmq_notify::MessageReadyEvent;
use tasker_shared::messaging::message::SimpleStepMessage;

/// Events from worker namespace queues
#[derive(Debug, Clone)]
pub enum WorkerQueueEvent {
    /// Step message ready for processing in namespace queue
    StepMessage(MessageReadyEvent),
}

/// Notification wrapper for worker queue events
#[derive(Debug, Clone)]
pub enum WorkerNotification {
    /// Event-driven notification
    Event(WorkerQueueEvent),
    /// System health notification
    Health(WorkerHealthUpdate),
}
```

**listener.rs - WorkerQueueListener:**
```rust
pub struct WorkerQueueListener {
    /// pgmq-notify listener
    listener: PgmqNotifyListener,
    /// Supported namespaces for this worker
    supported_namespaces: Vec<String>,
    /// Listener configuration
    config: WorkerListenerConfig,
    /// Notification sender
    notification_sender: mpsc::Sender<WorkerNotification>,
}

impl WorkerQueueListener {
    pub async fn new(
        context: Arc<SystemContext>,
        supported_namespaces: Vec<String>,
        config: WorkerListenerConfig,
    ) -> TaskerResult<(Self, mpsc::Receiver<WorkerNotification>)> {
        // Create pgmq-notify listener for worker namespace queues
        let namespace_pattern = format!("(?P<namespace>{})", supported_namespaces.join("|"));
        let notify_config = PgmqNotifyConfig::new()
            .with_queue_naming_pattern(&namespace_pattern)
            .with_default_namespace("default");

        let listener = PgmqNotifyListener::new(
            context.database_pool().clone(),
            notify_config
        ).await?;

        // Implementation follows orchestration_queues/listener.rs patterns
    }
}
```

**fallback_poller.rs - WorkerFallbackPoller:**
```rust
pub struct WorkerFallbackPoller {
    /// System context
    context: Arc<SystemContext>,
    /// Polling configuration
    config: WorkerPollerConfig,
    /// Supported namespaces
    supported_namespaces: Vec<String>,
    /// Command sender for worker operations
    command_sender: mpsc::Sender<WorkerCommand>,
    /// Polling statistics
    statistics: Arc<WorkerPollerStats>,
}

impl WorkerFallbackPoller {
    pub async fn start_polling(&self) -> TaskerResult<()> {
        let mut interval = tokio::time::interval(self.config.polling_interval);

        loop {
            interval.tick().await;

            // Poll all supported namespace queues
            for namespace in &self.supported_namespaces {
                self.poll_namespace_queue(namespace).await;
            }
        }
    }

    async fn poll_namespace_queue(&self, namespace: &str) -> TaskerResult<()> {
        let queue_name = format!("{}_queue", namespace);
        let messages = self.context.message_client()
            .read::<SimpleStepMessage>(
                &queue_name,
                self.config.visibility_timeout.as_secs() as i32,
                self.config.batch_size as i32,
            )
            .await?;

        for message in messages {
            // Send through command pattern (same as event path)
            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
            self.command_sender.send(WorkerCommand::ExecuteStep {
                message,
                queue_name: queue_name.clone(),
                resp: resp_tx,
            }).await?;
        }

        Ok(())
    }
}
```

### Integration with Existing Command Pattern

The new architecture integrates seamlessly with the existing `WorkerProcessor` command pattern:

```rust
// Enhanced command_processor.rs integration
impl WorkerEventSystem {
    async fn handle_step_message_event(&self, event: MessageReadyEvent) -> Result<(), DeploymentModeError> {
        // Convert pgmq-notify event to WorkerCommand
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();

        // Route through existing command pattern
        self.command_sender.send(WorkerCommand::ExecuteStepFromEvent {
            message_event: event,
            resp: resp_tx,
        }).await.map_err(|e| DeploymentModeError::ConfigurationError {
            message: format!("Failed to send worker command: {}", e)
        })?;

        Ok(())
    }
}
```

### Configuration Integration

**Worker Event System Configuration:**
```toml
# config/tasker/base/worker.toml
[event_system]
system_id = "worker-event-system"
deployment_mode = "Hybrid"  # PollingOnly/Hybrid/EventDrivenOnly
health_monitoring_enabled = true
health_check_interval = "30s"
max_concurrent_processors = 10
processing_timeout = "100ms"

[event_system.listener]
enabled = true
batch_processing = true
connection_timeout = "10s"

[event_system.fallback_poller]
enabled = true
polling_interval = "5s"
batch_size = 10
visibility_timeout = "30s"
```

## Implementation Plan

### Phase 1: Create Worker Queue Components (Week 1)
- [ ] Create `worker_queues/` module structure
- [ ] Implement `WorkerQueueEvent` and `WorkerNotification` types
- [ ] Implement `WorkerQueueListener` following orchestration patterns
- [ ] Implement `WorkerFallbackPoller` following orchestration patterns
- [ ] Add comprehensive unit tests for queue components

### Phase 2: Implement WorkerEventSystem (Week 2)
- [ ] Create `worker_event_system.rs` implementing EventDrivenSystem trait
- [ ] Integrate with worker queue components
- [ ] Add deployment mode support (PollingOnly/Hybrid/EventDrivenOnly)
- [ ] Implement health monitoring and statistics
- [ ] Add comprehensive integration tests

### Phase 3: Command Pattern Integration (Week 3)
- [ ] Enhance `WorkerCommand` enum for event-driven operations
- [ ] Update `WorkerProcessor` to handle event-driven commands
- [ ] Integrate WorkerEventSystem with command pattern
- [ ] Add event correlation and tracing support
- [ ] Test end-to-end event flow

### Phase 4: Refactor Event Driven Processor (Week 4)
- [ ] Refactor `event_driven_processor.rs` to use new components
- [ ] Maintain backward compatibility during transition
- [ ] Update TaskTemplateManager integration
- [ ] Enhance error handling and recovery
- [ ] Add performance benchmarks

### Phase 5: Configuration and Deployment (Week 5)
- [ ] Add TOML configuration support for all components
- [ ] Implement deployment mode switching at runtime
- [ ] Add comprehensive metrics and observability
- [ ] Create migration guide for existing deployments
- [ ] Document operational procedures

### Phase 6: Testing and Validation (Week 6)
- [ ] End-to-end integration tests with real workloads
- [ ] Performance comparison: new vs current implementation
- [ ] Stress testing with high message volumes
- [ ] Failure scenario testing (network, database issues)
- [ ] Documentation and examples

## Architectural Benefits

### 1. Consistency
- **Unified Patterns**: Same EventDrivenSystem architecture across orchestration and workers
- **Shared Abstractions**: Leverage tasker-shared event system traits
- **Common Configuration**: Consistent TOML configuration patterns

### 2. Modularity and Testability
- **Component Separation**: Clear boundaries between listener, poller, and event system
- **Dependency Injection**: Easy mocking for unit tests
- **Integration Points**: Well-defined interfaces for testing

### 3. Deployment Mode Flexibility
- **PollingOnly**: Traditional polling for maximum reliability
- **Hybrid**: Event-driven with polling fallback for optimal balance
- **EventDrivenOnly**: Pure event-driven for maximum performance

### 4. Maintainability
- **Code Reuse**: Shared patterns reduce duplication
- **Clear Ownership**: Each component has well-defined responsibilities
- **Future Extensions**: Easy to add new event types or deployment modes

## Event Flow Architecture

### Event-Driven Path
```
Step Message Enqueued → pgmq-notify → WorkerQueueListener → WorkerQueueEvent →
WorkerEventSystem → WorkerCommand::ExecuteStepFromEvent → WorkerProcessor
```

### Fallback Polling Path
```
Polling Timer → WorkerFallbackPoller → Read Messages →
WorkerCommand::ExecuteStep → WorkerProcessor
```

### Dual Mode (Hybrid)
Both paths operate concurrently with deduplication at the WorkerProcessor level.

## Performance Considerations

### Memory Usage
- **Shared Components**: Reuse connection pools and clients
- **Message Buffering**: Configurable buffer sizes for event channels
- **Statistics Aggregation**: Efficient atomic counters for metrics

### Latency Optimization
- **Direct Command Routing**: Minimal overhead from event to command
- **Batch Processing**: Configurable batching for both events and polling
- **Connection Reuse**: Persistent connections for pgmq-notify

### Scalability
- **Namespace Isolation**: Independent processing per namespace
- **Concurrent Processing**: Configurable concurrency limits
- **Backpressure Handling**: Channel-based backpressure management

## Integration with Existing Systems

### TAS-40 Command Pattern
- Reuse existing WorkerCommand enum and patterns
- Enhance with event-driven specific commands
- Maintain all existing functionality

### pgmq-notify Integration
- Leverage existing pgmq-notify crate
- Follow established patterns from orchestration
- Reuse connection pooling and configuration

### TaskTemplateManager
- Preserve existing namespace determination logic
- Enhance with event-driven metadata
- Maintain template caching and updates

## Future Enhancements

### Advanced Event Routing
- Priority-based event processing
- Namespace-specific processing strategies
- Event filtering and transformation

### Multi-Protocol Support
- Support for additional message protocols beyond pgmq
- Abstract event sources (Kafka, RabbitMQ, etc.)
- Protocol-specific optimizations

### Advanced Metrics
- Event processing histograms
- Namespace-level performance tracking
- Predictive scaling indicators

## Conclusion

The TAS-43 Worker Event System unifies the worker architecture with the established EventDrivenSystem patterns, providing:

1. **Architectural Consistency** across orchestration and worker systems
2. **Enhanced Modularity** for better testing and maintenance
3. **Deployment Flexibility** with multiple operational modes
4. **Performance Optimization** through proper abstraction layers
5. **Future Extensibility** for advanced event-driven patterns

This implementation follows the proven patterns from orchestration while respecting the unique requirements of worker systems, creating a cohesive and maintainable event-driven architecture throughout tasker-core.

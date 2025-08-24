# TAS-40 Command Pattern Implementation

## Overview

This ticket implements a fundamental architectural shift from complex polling-based executor pools to a simple, elegant tokio command pattern using mpsc channels. This change aligns with our future event-driven architecture (TAS-43) and message service abstraction (TAS-35) goals.

## Problem Statement

The current system uses `tasker-shared/src/{coordinator,executor}` with complex scaling logic, pool management, and polling-based executors. This was designed to handle spinning up multiple polling workers, but:

1. **Over-Engineering**: Solving problems we won't have with event-driven architecture
2. **Unnecessary Complexity**: Scaling and pool management for polling that becomes irrelevant with pg_notify
3. **Bootstrap Complexity**: Multiple entry points with different assumptions
4. **Maintenance Burden**: Complex coordinator/executor patterns that never fully worked

## Solution: Simple Command Pattern

Replace the complex system with tokio's standard command pattern using mpsc channels and oneshot responders. Keep the dependency injection benefits while eliminating unnecessary complexity.

## Architecture Overview

### Core Components

1. **SystemContext** (renamed from CoordinatorCore): Dependency injection container
2. **Command Processors**: Simple async task handlers using tokio channels
3. **Event-Driven Commands**: Commands sent via pg_notify (future TAS-43 integration)
4. **Message Service Abstraction**: Clean interface for multiple queue backends (TAS-35)

### Command Pattern Structure

```rust
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// Commands for orchestration operations based on existing lifecycle patterns
#[derive(Debug)]
pub enum OrchestrationCommand {
    InitializeTask {
        request: TaskRequestMessage, // Use existing message format
        resp: CommandResponder<TaskInitializeResult>,
    },
    ProcessStepResult {
        result: StepResultMessage, // Use existing message format
        resp: CommandResponder<StepProcessResult>,
    },
    FinalizeTask {
        task_uuid: Uuid,
        resp: CommandResponder<TaskFinalizationResult>,
    },
    GetProcessingStats {
        resp: CommandResponder<OrchestrationProcessingStats>,
    },
    HealthCheck {
        resp: CommandResponder<SystemHealth>,
    },
    Shutdown {
        resp: CommandResponder<()>,
    },
}

/// Result types matching existing orchestration patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInitializeResult {
    pub task_uuid: Uuid,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepProcessResult {
    Success { message: String },
    Failed { error: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskFinalizationResult {
    Success {
        task_uuid: Uuid,
        final_status: String,
        completion_time: Option<chrono::DateTime<chrono::Utc>>,
    },
    NotClaimed {
        reason: String,
        already_claimed_by: Option<String>,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationProcessingStats {
    pub task_requests_processed: u64,
    pub step_results_processed: u64,
    pub tasks_finalized: u64,
    pub processing_errors: u64,
    pub current_queue_sizes: HashMap<String, i64>,
}

/// Commands for worker operations
#[derive(Debug)]
pub enum WorkerCommand {
    ProcessStep {
        step_message: StepMessage,
        resp: CommandResponder<StepExecutionResult>,
    },
    RegisterHandler {
        namespace: String,
        handler_info: HandlerInfo,
        resp: CommandResponder<()>,
    },
    GetWorkerStatus {
        resp: CommandResponder<WorkerStatus>,
    },
    Shutdown {
        resp: CommandResponder<()>,
    },
}

/// Unified response type for all commands
type CommandResponder<T> = oneshot::Sender<TaskerResult<T>>;
```

## Key Insights from Existing Implementation

After reviewing the actual lifecycle management code, several important patterns emerge that the command pattern must preserve:

### 1. Sophisticated Task Initialization (`TaskInitializer`)
- **Transaction Safety**: All task creation wrapped in SQLx transactions for atomicity
- **State Machine Integration**: Proper initialization of task and step state transitions
- **Configuration-Driven**: Supports YAML-based task handler configurations
- **Dependency Management**: Handles complex workflow step dependencies
- **Registry Integration**: Works with TaskHandlerRegistry for validation and template loading

### 2. Advanced Step Result Processing (`StepResultProcessor`)
- **TAS-32 Architecture**: Ruby workers handle step execution, Rust coordinates task-level orchestration
- **Metadata Processing**: Processes worker metadata for backoff and retry decisions
- **Task Finalization Coordination**: Determines when tasks are complete
- **Error Handling**: Comprehensive error handling with message archiving

### 3. Race-Condition-Free Finalization (`FinalizationClaimer`)
- **Atomic Claiming**: Uses SQL functions with `FOR UPDATE SKIP LOCKED` semantics
- **TAS-37 Solution**: Prevents multiple processors from finalizing the same task
- **Timeout Management**: Configurable claim timeouts with automatic expiration
- **Comprehensive Metrics**: Built-in observability for monitoring and alerting
- **RAII Pattern**: ClaimGuard ensures claims are always released

### 4. Command Pattern Integration Strategy

The command pattern should **wrap** these existing sophisticated components rather than replace them:

```rust
// BAD: Reimplementing complex logic in command handlers
async fn handle_initialize_task(context: &Arc<SystemContext>, request: TaskRequest) -> TaskerResult<TaskInitializeResult> {
    // Duplicate complex initialization logic here...
}

// GOOD: Delegating to existing sophisticated components
async fn handle_initialize_task(context: &Arc<SystemContext>, request: TaskRequestMessage) -> TaskerResult<TaskInitializeResult> {
    let task_request_processor = TaskRequestProcessor::new(
        context.pgmq_client.clone(),
        context.task_handler_registry.clone(),
        Arc::new(TaskInitializer::new(context.database_pool.clone())),
        TaskRequestProcessorConfig::default(),
    );
    
    let request_json = serde_json::to_value(&request)?;
    let task_uuid = task_request_processor.process_task_request(&request_json).await?;
    
    Ok(TaskInitializeResult { task_uuid, message: "Task initialized successfully".to_string() })
}
```

This approach:
- **Preserves Complexity**: All sophisticated orchestration logic is maintained
- **Eliminates Polling**: Removes polling loops while keeping the business logic
- **Enables Event-Driven**: Commands can come from pg_notify events instead of queue polling
- **Maintains Atomicity**: Transaction safety and race condition prevention are preserved
- **Keeps Observability**: Metrics and logging continue to work as before

## Implementation Plan

### Phase 1: SystemContext (Dependency Injection Container)

#### 1.1 Rename and Simplify CoordinatorCore

**File**: `tasker-shared/src/system_context.rs` (renamed from `coordinator/core.rs`)

```rust
/// Shared system dependencies and configuration
/// 
/// This serves as a dependency injection container providing access to:
/// - Database connection pool
/// - Configuration manager  
/// - Message queue clients (unified PGMQ/future RabbitMQ)
/// - Task handler registry
/// - Circuit breaker management
/// - Operational state management
pub struct SystemContext {
    /// System instance ID
    pub system_id: Uuid,
    
    /// Configuration manager with environment-aware loading
    pub config_manager: Arc<ConfigManager>,
    
    /// Unified message queue client (PGMQ/RabbitMQ abstraction)
    pub message_client: Arc<UnifiedMessageClient>,
    
    /// Database connection pool
    pub database_pool: PgPool,
    
    /// Task handler registry
    pub task_handler_registry: Arc<TaskHandlerRegistry>,
    
    /// Circuit breaker manager (optional)
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,
    
    /// Operational state manager
    pub operational_state: Arc<OperationalStateManager>,
}
```

#### 1.2 SystemContext Responsibilities

- **Dependency Injection**: Provide shared infrastructure to command processors
- **Configuration Management**: Environment-aware configuration loading
- **Resource Management**: Database pools, message clients, registries
- **Operational State**: System state coordination across components
- **Circuit Breaker Integration**: Resilience patterns when enabled

#### 1.3 File Changes

```bash
# Rename and update
mv tasker-shared/src/coordinator/core.rs tasker-shared/src/system_context.rs

# Update imports throughout codebase
# coordinator::core -> system_context
# CoordinatorCore -> SystemContext
```

### Phase 2: Orchestration Command Processor

#### 2.1 Create Orchestration Command Processor

**File**: `tasker-orchestration/src/command_processor.rs`

```rust
pub struct OrchestrationProcessor {
    /// Shared system dependencies
    context: Arc<SystemContext>,
    
    /// Command receiver channel
    command_rx: mpsc::Receiver<OrchestrationCommand>,
    
    /// Processor task handle
    task_handle: Option<JoinHandle<()>>,
}

impl OrchestrationProcessor {
    pub fn new(context: Arc<SystemContext>, buffer_size: usize) -> (Self, mpsc::Sender<OrchestrationCommand>) {
        let (command_tx, command_rx) = mpsc::channel(buffer_size);
        
        let processor = Self {
            context,
            command_rx,
            task_handle: None,
        };
        
        (processor, command_tx)
    }
    
    pub async fn start(&mut self) -> TaskerResult<()> {
        let context = self.context.clone();
        let mut command_rx = self.command_rx.take()
            .ok_or_else(|| TaskerError::OrchestrationError("Processor already started".to_string()))?;
        
        let handle = tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                Self::process_command(&context, command).await;
            }
        });
        
        self.task_handle = Some(handle);
        Ok(())
    }
    
    async fn process_command(context: &Arc<SystemContext>, command: OrchestrationCommand) {
        match command {
            OrchestrationCommand::InitializeTask { request, resp } => {
                let result = Self::handle_initialize_task(context, request).await;
                let _ = resp.send(result);
            }
            OrchestrationCommand::ProcessStepResult { result, resp } => {
                let result = Self::handle_process_step_result(context, result).await;
                let _ = resp.send(result);
            }
            OrchestrationCommand::ClaimTasks { namespace, limit, resp } => {
                let result = Self::handle_claim_tasks(context, namespace, limit).await;
                let _ = resp.send(result);
            }
            OrchestrationCommand::HealthCheck { resp } => {
                let result = Self::handle_health_check(context).await;
                let _ = resp.send(result);
            }
            OrchestrationCommand::Shutdown { resp } => {
                let _ = resp.send(Ok(()));
                break;
            }
        }
    }
}
```

#### 2.2 Command Handler Implementation

Based on the actual lifecycle management implementation, each command handler implements sophisticated orchestration logic:

```rust
impl OrchestrationProcessor {
    /// Handle task initialization using existing TaskInitializer patterns
    async fn handle_initialize_task(
        context: &Arc<SystemContext>, 
        request: TaskRequestMessage
    ) -> TaskerResult<TaskInitializeResult> {
        // Use the existing TaskRequestProcessor pattern for validation and conversion
        let task_request_processor = TaskRequestProcessor::new(
            context.pgmq_client.clone(),
            context.task_handler_registry.clone(),
            Arc::new(TaskInitializer::new(context.database_pool.clone())),
            TaskRequestProcessorConfig::default(),
        );
        
        // Convert message format to internal format and process
        let request_json = serde_json::to_value(&request).map_err(|e| {
            TaskerError::ValidationError(format!("Failed to serialize task request: {e}"))
        })?;
        
        // Use existing validation and creation logic
        let task_uuid = task_request_processor
            .process_task_request(&request_json)
            .await?;
        
        Ok(TaskInitializeResult { 
            task_uuid,
            message: "Task initialized successfully".to_string(),
        })
    }
    
    /// Handle step result processing using existing patterns
    async fn handle_process_step_result(
        context: &Arc<SystemContext>,
        step_result: StepResultMessage
    ) -> TaskerResult<StepProcessResult> {
        // Use existing StepResultProcessor orchestration logic
        let step_result_processor = StepResultProcessor::new(
            context.database_pool.clone(),
            context.pgmq_client.clone(),
            context.config_manager.tasker_config().clone(),
        ).await?;
        
        // Convert to JSON format expected by existing processor
        let result_json = serde_json::to_value(&step_result).map_err(|e| {
            TaskerError::ValidationError(format!("Failed to serialize step result: {e}"))
        })?;
        
        // Process using existing logic which handles:
        // - Result validation and parsing
        // - Orchestration metadata processing
        // - Task finalization coordination
        // - Error handling and retry logic
        step_result_processor
            .process_single_step_result(&result_json, 0) // msg_id not used in direct processing
            .await?;
        
        Ok(StepProcessResult::Success { 
            message: "Step result processed successfully".to_string(),
        })
    }
    
    /// Handle task finalization using existing FinalizationClaimer patterns
    async fn handle_finalize_task(
        context: &Arc<SystemContext>,
        task_uuid: Uuid
    ) -> TaskerResult<TaskFinalizationResult> {
        // Use atomic finalization claiming to prevent race conditions
        let processor_id = FinalizationClaimer::generate_processor_id("command_processor");
        let claimer = FinalizationClaimer::new(
            context.database_pool.clone(),
            processor_id,
        );
        
        // Attempt to claim the task for finalization
        let claim_result = claimer.claim_task(task_uuid).await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to claim task for finalization: {e}"))
        })?;
        
        if !claim_result.claimed {
            // Another processor is handling finalization or task is not ready
            return Ok(TaskFinalizationResult::NotClaimed { 
                reason: claim_result.message.unwrap_or("Task not available for finalization".to_string()),
                already_claimed_by: claim_result.already_claimed_by,
            });
        }
        
        // We have the claim - perform finalization using existing TaskFinalizer
        let task_finalizer = TaskFinalizer::new(
            context.database_pool.clone(),
            context.config_manager.tasker_config().clone(),
        );
        
        let finalization_result = task_finalizer
            .finalize_task_with_claim(task_uuid)
            .await;
        
        // Always release the claim, regardless of finalization success/failure
        if let Err(release_error) = claimer.release_claim(task_uuid).await {
            tracing::warn!(
                task_uuid = %task_uuid,
                error = %release_error,
                "Failed to release finalization claim after processing"
            );
        }
        
        // Now handle the finalization result
        match finalization_result {
            Ok(result) => Ok(TaskFinalizationResult::Success { 
                task_uuid,
                final_status: result.status,
                completion_time: result.completed_at,
            }),
            Err(e) => Ok(TaskFinalizationResult::Failed {
                error: format!("Task finalization failed: {e}"),
            }),
        }
    }
}
```

### Phase 3: Worker Command Processor

#### 3.1 tasker-worker/src Cleanup Strategy

Based on analysis of the current tasker-worker/src, almost everything is built on old polling-based coordinator/executor system assumptions and should be deleted:

**DELETE - Built on Old Polling System**:
```bash
# Complex coordinator/executor system (mirrors orchestration patterns we're removing)
tasker-worker/src/worker/coordinator.rs         # WorkerLoopCoordinator - complex polling coordinator
tasker-worker/src/worker/executor_pool.rs       # WorkerExecutorPool - complex pool management  
tasker-worker/src/worker/executor.rs            # WorkerExecutor - complex executor with polling loops
tasker-worker/src/worker/core.rs                # WorkerCore - complex bootstrap mirroring OrchestrationCore
tasker-worker/src/worker/health_monitor.rs      # Complex health monitoring system
tasker-worker/src/worker/resource_validator.rs  # Complex resource validation
tasker-worker/src/worker/worker_core_draft.rs   # Draft file
tasker-worker/src/worker/mod.rs                 # Module file for complex system

# Old event system (not event-driven)
tasker-worker/src/event_publisher.rs   # In-process event publisher (not pg_notify)
tasker-worker/src/event_subscriber.rs  # In-process event subscriber (not pg_notify) 
tasker-worker/src/events.rs           # In-process event system (not pg_notify)

# Complex infrastructure
tasker-worker/src/health.rs    # Complex health monitoring
tasker-worker/src/metrics.rs   # Complex metrics system
tasker-worker/src/web.rs       # Worker web interface (probably not needed)
```

**KEEP - Useful Foundation**:
```bash
tasker-worker/src/lib.rs       # Main library file (will be simplified)
tasker-worker/src/config.rs    # Worker configuration (may need simplification)
tasker-worker/src/error.rs     # Error handling (useful for command pattern)
```

**The Problem**: Files like `coordinator.rs:222-263` and `executor_pool.rs:147-192` contain complex polling loops with `tokio::time::interval` and auto-scaling logic designed for the polling system we're eliminating.

#### 3.2 New Simple Worker Architecture

**File**: `tasker-worker/src/command_processor.rs`

```rust
pub struct WorkerProcessor {
    /// Shared system dependencies
    context: Arc<SystemContext>,
    
    /// Command receiver channel
    command_rx: mpsc::Receiver<WorkerCommand>,
    
    /// Processor task handle  
    task_handle: Option<JoinHandle<()>>,
}

impl WorkerProcessor {
    pub fn new(context: Arc<SystemContext>, buffer_size: usize) -> (Self, mpsc::Sender<WorkerCommand>) {
        let (command_tx, command_rx) = mpsc::channel(buffer_size);
        
        let processor = Self {
            context,
            command_rx,
            task_handle: None,
        };
        
        (processor, command_tx)
    }
    
    pub async fn start(&mut self) -> TaskerResult<()> {
        let context = self.context.clone();
        let mut command_rx = self.command_rx.take()
            .ok_or_else(|| TaskerError::WorkerError("Processor already started".to_string()))?;
        
        let handle = tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                Self::process_command(&context, command).await;
            }
        });
        
        self.task_handle = Some(handle);
        Ok(())
    }
    
    async fn process_command(context: &Arc<SystemContext>, command: WorkerCommand) {
        match command {
            WorkerCommand::ProcessStep { step_message, resp } => {
                let result = Self::execute_step_with_context(context, step_message).await;
                let _ = resp.send(result);
            }
            WorkerCommand::RegisterHandler { namespace, handler_info, resp } => {
                let result = Self::register_handler_with_context(context, namespace, handler_info).await;
                let _ = resp.send(result);
            }
            WorkerCommand::GetWorkerStatus { resp } => {
                let status = Self::get_worker_status_with_context(context).await;
                let _ = resp.send(Ok(status));
            }
            WorkerCommand::Shutdown { resp } => {
                tracing::info!("Worker received shutdown command");
                let _ = resp.send(Ok(()));
                break;
            }
        }
    }
}
```

#### 3.3 Command-Driven Step Processing (No Polling)

```rust
impl WorkerProcessor {
    /// Execute a specific step that was sent as a command (pure command pattern - no polling!)
    async fn execute_step_with_context(
        context: &Arc<SystemContext>, 
        step_message: StepMessage
    ) -> TaskerResult<StepExecutionResult> {
        tracing::info!(
            step_id = %step_message.step_id,
            task_id = %step_message.task_id,
            step_name = %step_message.step_name,
            "Processing step command"
        );
        
        // 1. Resolve step handler using existing registry patterns
        let handler = context.task_handler_registry
            .resolve_step_handler(&step_message)
            .map_err(|e| {
                TaskerError::WorkerError(format!("Failed to resolve step handler: {e}"))
            })?;
        
        // 2. Execute step using existing handler patterns
        let execution_result = handler
            .execute_step(step_message.clone())
            .await
            .map_err(|e| {
                TaskerError::WorkerError(format!("Step execution failed: {e}"))
            })?;
        
        // 3. Send result back to orchestration via command (not direct queue access)
        let step_result = StepResult::from_execution_result(
            step_message.step_id,
            execution_result
        );
        
        // Send result as command to orchestration processor
        self.send_step_result_command(step_result).await?;
        
        Ok(StepExecutionResult::Success {
            step_id: step_message.step_id,
            execution_time_ms: execution_result.execution_time_ms.unwrap_or(0),
            message: "Step executed successfully".to_string(),
        })
    }
    
    /// Register a step handler for a namespace
    async fn register_handler_with_context(
        context: &Arc<SystemContext>,
        namespace: String,
        handler_info: HandlerInfo
    ) -> TaskerResult<()> {
        context.task_handler_registry
            .register_handler(namespace, handler_info)
            .await
            .map_err(|e| {
                TaskerError::WorkerError(format!("Failed to register handler: {e}"))
            })
    }
    
    /// Get current worker status
    async fn get_worker_status_with_context(
        context: &Arc<SystemContext>
    ) -> WorkerStatus {
        WorkerStatus {
            status: "active".to_string(),
            steps_processed: 0, // TODO: Add proper metrics tracking
            last_activity: chrono::Utc::now(),
            registered_handlers: context.task_handler_registry.list_handlers().await.unwrap_or_default(),
        }
    }
    
    /// Send step result as command to orchestration processor
    async fn send_step_result_command(&self, step_result: StepResult) -> TaskerResult<()> {
        // This would send a ProcessStepResult command to the orchestration processor
        // The step result flows back through the command pattern, not direct queue access
        
        let (resp_tx, resp_rx) = oneshot::channel();
        let command = OrchestrationCommand::ProcessStepResult {
            result: step_result.into_message(), // Convert to StepResultMessage format
            resp: resp_tx,
        };
        
        // Send to orchestration processor via command channel (maintained by system)
        self.orchestration_tx.send(command).await
            .map_err(|_| TaskerError::WorkerError("Orchestration command channel closed".to_string()))?;
        
        // Wait for acknowledgment
        resp_rx.await
            .map_err(|_| TaskerError::WorkerError("Orchestration response channel closed".to_string()))?
            .map(|_| ())
    }
}
```

#### 3.4 Key Differences from Old System

**Old System Problems** (found in current tasker-worker/src):
1. **WorkerLoopCoordinator**: 376 lines of complex auto-scaling coordinator with polling intervals
2. **WorkerExecutorPool**: 403 lines of complex pool management with executor spawning loops  
3. **WorkerExecutor**: 256 lines of complex step claiming and hydration with polling
4. **WorkerCore**: 334 lines of complex bootstrap mirroring OrchestrationCore patterns
5. **Multiple Event Systems**: In-process event publisher/subscriber instead of pg_notify

**New Command Pattern Benefits**:
1. **Single Responsibility**: WorkerProcessor only processes commands sent to it
2. **No Polling**: Steps are processed when commands arrive (event-driven ready)
3. **No Auto-Scaling**: No complex pool management (unnecessary with event-driven)
4. **Simple Bootstrap**: Uses SystemContext dependency injection
5. **Event-Driven Ready**: Commands can come from pg_notify instead of polling

The new tasker-worker becomes **much simpler**:
```bash
tasker-worker/src/
├── lib.rs                    # Keep (simplified)
├── error.rs                  # Keep  
├── config.rs                 # Keep (simplified)
└── command_processor.rs      # NEW - Simple command pattern (~200 lines vs 1000+)
```

### Phase 4: Event-Driven Integration (TAS-43 Foundation)

#### 4.1 Prepare for pg_notify Integration

**File**: `tasker-shared/src/messaging/event_dispatcher.rs`

```rust
/// Trait for handling pg_notify events - implemented differently in each workspace
#[async_trait]
pub trait EventDispatcher: Send + Sync {
    /// Handle a pg_notify event for this specific processor type
    async fn handle_notification(
        &self,
        channel: &str,
        payload: &str,
    ) -> TaskerResult<()>;
    
    /// Get the list of channels this dispatcher wants to listen to
    fn channels(&self) -> Vec<String>;
    
    /// Start listening for events (default implementation)
    async fn start_listening(&self, pool: PgPool) -> TaskerResult<()> {
        let mut connection = pool.acquire().await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to acquire connection for pg_notify: {e}"))
        })?;
        
        // Listen to all channels this dispatcher cares about
        for channel in self.channels() {
            sqlx::query(&format!("LISTEN {}", channel))
                .execute(&mut *connection)
                .await
                .map_err(|e| {
                    TaskerError::DatabaseError(format!("Failed to LISTEN to channel {}: {e}", channel))
                })?;
        }
        
        // Event processing loop
        while let Some(notification) = connection.recv().await {
            if let Err(e) = self.handle_notification(notification.channel(), notification.payload()).await {
                tracing::error!(
                    channel = notification.channel(),
                    error = %e,
                    "Failed to handle pg_notify event"
                );
            }
        }
        
        Ok(())
    }
}
```

**File**: `tasker-orchestration/src/event_dispatcher.rs`

```rust
/// Orchestration-specific event dispatcher
pub struct OrchestrationEventDispatcher {
    orchestration_tx: mpsc::Sender<OrchestrationCommand>,
}

impl OrchestrationEventDispatcher {
    pub fn new(orchestration_tx: mpsc::Sender<OrchestrationCommand>) -> Self {
        Self { orchestration_tx }
    }
}

#[async_trait]
impl EventDispatcher for OrchestrationEventDispatcher {
    async fn handle_notification(&self, channel: &str, payload: &str) -> TaskerResult<()> {
        match channel {
            "orchestration_task_requests" => {
                // Parse task request and send as command
                let request: TaskRequestMessage = serde_json::from_str(payload).map_err(|e| {
                    TaskerError::MessagingError(format!("Failed to parse task request: {e}"))
                })?;
                
                let (resp_tx, _resp_rx) = oneshot::channel();
                let command = OrchestrationCommand::InitializeTask {
                    request,
                    resp: resp_tx,
                };
                
                self.orchestration_tx.send(command).await.map_err(|_| {
                    TaskerError::MessagingError("Orchestration command channel closed".to_string())
                })?;
            }
            "orchestration_step_results" => {
                // Parse step result and send as command
                let result: StepResultMessage = serde_json::from_str(payload).map_err(|e| {
                    TaskerError::MessagingError(format!("Failed to parse step result: {e}"))
                })?;
                
                let (resp_tx, _resp_rx) = oneshot::channel();
                let command = OrchestrationCommand::ProcessStepResult {
                    result,
                    resp: resp_tx,
                };
                
                self.orchestration_tx.send(command).await.map_err(|_| {
                    TaskerError::MessagingError("Orchestration command channel closed".to_string())
                })?;
            }
            _ => {
                tracing::debug!(channel = channel, "Ignoring unknown channel");
            }
        }
        
        Ok(())
    }
    
    fn channels(&self) -> Vec<String> {
        vec![
            "orchestration_task_requests".to_string(),
            "orchestration_step_results".to_string(),
        ]
    }
}
```

**File**: `tasker-worker/src/event_dispatcher.rs`

```rust
/// Worker-specific event dispatcher  
pub struct WorkerEventDispatcher {
    worker_tx: mpsc::Sender<WorkerCommand>,
}

impl WorkerEventDispatcher {
    pub fn new(worker_tx: mpsc::Sender<WorkerCommand>) -> Self {
        Self { worker_tx }
    }
}

#[async_trait]
impl EventDispatcher for WorkerEventDispatcher {
    async fn handle_notification(&self, channel: &str, payload: &str) -> TaskerResult<()> {
        match channel {
            "worker_step_messages" => {
                // Parse step message and send as command
                let step_message: StepMessage = serde_json::from_str(payload).map_err(|e| {
                    TaskerError::MessagingError(format!("Failed to parse step message: {e}"))
                })?;
                
                let (resp_tx, _resp_rx) = oneshot::channel();
                let command = WorkerCommand::ProcessStep {
                    step_message,
                    resp: resp_tx,
                };
                
                self.worker_tx.send(command).await.map_err(|_| {
                    TaskerError::MessagingError("Worker command channel closed".to_string())
                })?;
            }
            _ => {
                tracing::debug!(channel = channel, "Ignoring unknown channel");
            }
        }
        
        Ok(())
    }
    
    fn channels(&self) -> Vec<String> {
        vec!["worker_step_messages".to_string()]
    }
}
```

### Phase 5: Message Service Abstraction (TAS-35 Foundation)

#### 5.1 Unified Message Client

**File**: `tasker-shared/src/messaging/unified_client.rs`

```rust
/// Abstraction over multiple message queue backends
#[async_trait]
pub trait MessageClient: Send + Sync {
    async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()>;
    async fn claim_step_message(&self, worker_id: usize) -> TaskerResult<Option<StepMessage>>;
    async fn send_step_result(&self, result: StepResult) -> TaskerResult<()>;
    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> TaskerResult<()>;
}

/// Unified client supporting multiple backends
pub enum UnifiedMessageClient {
    Pgmq(PgmqClient),
    RabbitMq(RabbitMqClient), // Future implementation
    InMemory(InMemoryClient), // For testing
}

#[async_trait]
impl MessageClient for UnifiedMessageClient {
    async fn send_step_message(&self, namespace: &str, message: StepMessage) -> TaskerResult<()> {
        match self {
            Self::Pgmq(client) => client.send_step_message(namespace, message).await,
            Self::RabbitMq(client) => client.send_step_message(namespace, message).await,
            Self::InMemory(client) => client.send_step_message(namespace, message).await,
        }
    }
    
    // ... implement other methods
}
```

## Migration Strategy

### Step 1: Rename and Isolate SystemContext

1. **Rename CoordinatorCore**: 
   - `mv tasker-shared/src/coordinator/core.rs tasker-shared/src/system_context.rs`
   - Update all imports: `coordinator::core::CoordinatorCore` → `system_context::SystemContext`

2. **Update Method Names**:
   - Keep dependency injection methods
   - Remove complex orchestration-specific methods
   - Focus on providing shared resources

### Step 2: Remove Complex Systems

**Delete These Files**:
```bash
# Complex coordinator system
rm -rf tasker-shared/src/coordinator/
rm -rf tasker-shared/src/executor/

# Complex orchestration components  
rm -rf tasker-orchestration/src/orchestration/coordinator/
rm -rf tasker-orchestration/src/orchestration/executor/
rm -rf tasker-orchestration/src/orchestration/orchestration_loop.rs
rm -rf tasker-orchestration/src/orchestration/result_processor.rs
rm -rf tasker-orchestration/src/orchestration/step_enqueuer.rs
rm -rf tasker-orchestration/src/orchestration/task_claimer.rs
```

**Keep These Files** (will be refactored):
```bash
# Keep for gradual migration
tasker-orchestration/src/orchestration/core.rs  # Becomes simple bootstrap
tasker-orchestration/src/orchestration/orchestration_system.rs  # Simplified
```

### Step 3: Implement Command Processors

1. **Create orchestration command processor**:
   - `tasker-orchestration/src/command_processor.rs`
   - Simple async command handling
   - Uses SystemContext for dependencies

2. **Create worker command processor**:
   - `tasker-worker/src/command_processor.rs` 
   - Worker pool with tokio task handles
   - Direct step claiming and processing

### Step 4: Update Bootstrap Logic

**File**: `tasker-orchestration/src/orchestration/core.rs`

```rust
pub struct OrchestrationCore {
    context: Arc<SystemContext>,
    orchestration_processor: OrchestrationProcessor,
    orchestration_tx: mpsc::Sender<OrchestrationCommand>,
}

impl OrchestrationCore {
    pub async fn new() -> TaskerResult<Self> {
        // Create shared dependencies
        let context = Arc::new(SystemContext::new().await?);
        
        // Create command processor
        let (orchestration_processor, orchestration_tx) = 
            OrchestrationProcessor::new(context.clone(), 1000);
        
        Ok(Self {
            context,
            orchestration_processor,
            orchestration_tx,
        })
    }
    
    pub async fn start(&mut self) -> TaskerResult<()> {
        // Start the command processor
        self.orchestration_processor.start().await?;
        
        // Initialize system context
        self.context.start().await?;
        
        Ok(())
    }
    
    // Public API methods that send commands
    pub async fn initialize_task(&self, request: TaskRequest) -> TaskerResult<TaskInitializeResult> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let command = OrchestrationCommand::InitializeTask { request, resp: resp_tx };
        
        self.orchestration_tx.send(command).await
            .map_err(|_| TaskerError::OrchestrationError("Command channel closed".to_string()))?;
            
        resp_rx.await
            .map_err(|_| TaskerError::OrchestrationError("Response channel closed".to_string()))?
    }
}
```

### Step 5: Update Tests

1. **Integration tests**: Update to use command pattern
2. **Unit tests**: Focus on individual command handlers
3. **Remove complex orchestration tests**: Replace with simple command processor tests

## Benefits of This Approach

### 1. Simplicity
- **Clear Command Flow**: Request → Command → Handler → Response
- **Single Responsibility**: Each processor handles one concern
- **Easy to Reason About**: Standard tokio patterns

### 2. Event-Driven Ready
- **Channel-Based**: Easy to replace channels with pg_notify events
- **Decoupled**: Commands can come from any source (HTTP, WebSocket, pg_notify)
- **Scalable**: Multiple processors can handle same command types

### 3. Message Service Abstraction Ready
- **Unified Interface**: MessageClient trait abstracts queue backends
- **Backend Flexibility**: Easy to add RabbitMQ, Redis, etc.
- **Testing Support**: In-memory implementation for tests

### 4. Maintains Benefits
- **Dependency Injection**: SystemContext provides shared resources
- **Configuration**: Environment-aware configuration still works
- **Circuit Breakers**: Resilience patterns preserved
- **Observability**: Metrics and logging maintained

### 5. Migration Safety
- **Gradual Migration**: Can implement processors alongside existing system
- **Rollback Capability**: Keep old system until new one is proven
- **Testing**: Can A/B test old vs new implementations

## Implementation Timeline

### Week 1: Foundation
- [ ] Rename CoordinatorCore to SystemContext
- [ ] Create basic command processor structure
- [ ] Remove complex coordinator/executor files

### Week 2: Orchestration Processor  
- [ ] Implement OrchestrationCommand enum
- [ ] Create OrchestrationProcessor with basic handlers
- [ ] Update OrchestrationCore to use command pattern

### Week 3: Worker Processor
- [ ] Implement WorkerCommand enum
- [ ] Create WorkerProcessor with step processing
- [ ] Update worker bootstrap logic

### Week 4: Integration & Testing
- [ ] Integration tests with new command pattern
- [ ] Performance testing vs old system
- [ ] Documentation updates

### Future: Event-Driven Enhancement (TAS-43)
- [ ] Implement EventDispatcher with pg_notify
- [ ] Replace direct command sending with event publishing
- [ ] Performance optimization for event-driven architecture

## Success Metrics

### Performance
- [ ] **Latency**: Command processing latency < 1ms
- [ ] **Throughput**: Handle >1000 commands/second
- [ ] **Memory**: Reduced memory usage vs complex system

### Maintainability
- [ ] **Code Reduction**: 50%+ reduction in orchestration code
- [ ] **Test Simplicity**: Easier to test individual command handlers
- [ ] **Debug Experience**: Clear command flow for debugging

### Future-Proofing
- [ ] **Event-Driven Ready**: Easy migration to pg_notify events
- [ ] **Multi-Backend**: Message client abstraction working
- [ ] **Scalability**: Multiple processor instances support

## Risks and Mitigations

### Risk 1: Performance Regression
- **Mitigation**: Benchmark old vs new system
- **Fallback**: Keep old system until performance validated

### Risk 2: Lost Functionality
- **Mitigation**: Comprehensive testing of all workflows
- **Fallback**: Gradual migration with feature flags

### Risk 3: Complexity in Commands
- **Mitigation**: Keep command handlers simple and focused
- **Review**: Regular architecture review to prevent complexity creep

This command pattern approach provides a clean, maintainable foundation that aligns with our future event-driven and message service abstraction goals while eliminating the complexity of the current polling-based system.
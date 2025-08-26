# TAS-40: Worker Foundations with Workspace Architecture

## Executive Summary

Create a robust Rust worker foundation system within a new multi-workspace architecture that cleanly separates orchestration from worker execution concerns. This foundation will handle all infrastructure responsibilities for step execution across multiple binding languages (Ruby, Python, WASM) while maintaining the proven patterns from our orchestration system.

## Workspace Context

This ticket implements the worker foundation as part of the broader workspace reorganization outlined in TAS-40-preamble.md. The worker foundation will be created as a separate workspace (`tasker-worker/`) that depends on shared components (`tasker-shared/`) and provides infrastructure for binding language implementations.

## Current State Analysis

### Ruby Binding Infrastructure Burden
The current `bindings/ruby/` implementation handles extensive infrastructure concerns:
- Database connection pooling via ActiveRecord
- Concurrent Ruby threading for step processing
- Queue message parsing and claiming logic
- State transition management and result persistence
- Worker thread coordination and resource management

### Orchestration System Success Patterns
Our `tasker-orchestration/` workspace (current `tasker-core-rs/`) provides proven patterns:
- `OrchestrationCore` unified bootstrap system
- `OrchestrationLoopCoordinator` with auto-scaling and health monitoring
- Atomic SQL operations for race condition prevention
- Component-based configuration with environment overrides
- Resource validation and circuit breaker integration

## Implementation Plan

### Phase 1: Workspace Restructuring (Week 1)

#### 1.1 Create Multi-Workspace Structure
```bash
# Create new workspace structure within current repo
mkdir -p tasker-orchestration
mkdir -p tasker-worker/{src,config,tests}
mkdir -p tasker-shared/{src,config}
mkdir -p tasker-worker-rust/{src,config,tests}

# Move current Rust workspace files to orchestration directory
mv src/ tasker-orchestration/
mv tests/ tasker-orchestration/
mv Cargo.toml tasker-orchestration/
mv Cargo.lock tasker-orchestration/
mv sqlx-data.json tasker-orchestration/ 2>/dev/null || true
mv target/ tasker-orchestration/ 2>/dev/null || true

# Keep repo-level files at root: .git/, docs/, bindings/, config/, README.md, etc.
```

#### 1.2 Workspace Cargo.toml Configuration
```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "tasker-orchestration",
    "tasker-worker",
    "tasker-shared"
]
resolver = "2"

[workspace.dependencies]
# Shared dependencies
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-rustls", "chrono", "uuid"] }
pgmq = "0.33.1"
magnus = "0.6"
tracing = "0.1"
thiserror = "1.0"
anyhow = "1.0"
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
```

#### 1.3 Extract Shared Components to `tasker-shared/`
Move these modules from `tasker-orchestration/src/` to `tasker-shared/src/`:

**Core Infrastructure:**
- `config/` - TAS-34 component-based configuration system
- `database/` - Database connection management, migrations, optimized queries
- `error.rs` - Shared error types for FFI boundary and consistent error handling
- `logging.rs` - Unified logging macros used across all components

**Business Logic Foundation:**
- `state_machine/` - Task/step state transitions (core to both orchestration and workers)
- `constants.rs` - System events, states, status groups used throughout ecosystem
- `registry/` - Handler registries (workers) and task template registry (orchestration)
- `validation.rs` - Input validation for web APIs and worker processing

**Data & Query Layer:**
- `scopes/` - Database query scopes used by orchestration analytics and worker lookups
- `sql_functions.rs` - Documentation for shared SQL functions
- `events/` - Event publishing for orchestration-worker coordination
- `execution/` - Message protocols for queue communication

**Models & Messaging:**
- `models/` - All database models and schemas
- `messaging/` - TAS-35 queue traits and client implementations
- `types/` - Common data structures and type definitions
- `utils/` - Shared utilities and helper functions

```rust
// tasker-shared/src/lib.rs

// Core Infrastructure
pub mod config;
pub mod database;
pub mod error;
pub mod logging;

// Business Logic Foundation
pub mod state_machine;
pub mod constants;
pub mod registry;
pub mod validation;

// Data & Query Layer
pub mod scopes;
pub mod sql_functions;
pub mod events;
pub mod execution;

// Models & Messaging
pub mod models;
pub mod messaging;
pub mod types;
pub mod utils;

// Re-export commonly used items
pub use models::*;
pub use config::ConfigManager;
pub use messaging::{PgmqClient, UnifiedPgmqClient};
pub use database::Connection;
pub use types::*;
pub use error::{TaskerError, Result};
pub use constants::{ExecutionStatus, RecommendedAction, HealthStatus};
pub use state_machine::{TaskState, WorkflowStepState};
```

#### 1.4 Update Import Statements
```rust
// tasker-orchestration/src/orchestration/core.rs
use tasker_shared::{
    models::{Task, WorkflowStep},
    config::ConfigManager,
    messaging::UnifiedPgmqClient,
    database::Connection,
    state_machine::{TaskState, WorkflowStepState},
    constants::{ExecutionStatus, RecommendedAction},
    error::{TaskerError, Result},
    logging::{log_orchestrator, log_task},
};

// tasker-worker/src/worker/core.rs
use tasker_shared::{
    models::{Task, WorkflowStep, Sequence},
    config::ConfigManager,
    messaging::UnifiedPgmqClient,
    database::Connection,
    state_machine::{TaskState, WorkflowStepState},
    registry::HandlerRegistry,
    validation::validate_step_inputs,
    error::{TaskerError, Result},
    logging::{log_queue_worker, log_step},
};
```

### Phase 2: Worker Foundation Core (Week 2)

#### 2.1 Worker Configuration System
Uses shared configuration patterns with worker-specific components:

```toml
# config/tasker/base/web.toml (shared with orchestration, worker uses different port)
bind_address = "0.0.0.0:8081"  # Worker-specific port (orchestration uses 8080)
request_timeout_seconds = 30
cors_enabled = true

# config/tasker/base/executor_pools.toml (worker-specific pool configuration)
[step_executor_pool]
initial_pool_size = 10
max_pool_size = 50
scale_up_threshold = 0.8
scale_down_threshold = 0.2
max_concurrent_steps = 100

[result_processor_pool]
initial_pool_size = 5
max_pool_size = 20
result_persistence_timeout_seconds = 30

# config/tasker/base/orchestration_api.toml (worker-specific: API client config)
base_url = "http://localhost:8080"  # Orchestration API endpoint
timeout_seconds = 10
retry_attempts = 3
circuit_breaker_enabled = true

# config/tasker/base/step_processing.toml (worker-specific component)
claim_timeout_seconds = 300
max_retries = 3
retry_backoff_multiplier = 2.0
namespaces = ["fulfillment", "inventory", "notifications"]
heartbeat_interval_seconds = 30

# config/tasker/base/health_monitoring.toml (shared pattern, worker-specific thresholds)
health_check_interval_seconds = 10
metrics_collection_enabled = true
performance_monitoring_enabled = true
step_processing_rate_threshold = 10.0
```

#### 2.2 WorkerCore Bootstrap System
```rust
// tasker-worker/src/worker/core.rs
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use tracing::{info, error};

use tasker_shared::{
    config::ConfigManager,
    messaging::UnifiedPgmqClient,
    database::Connection,
    models::TaskTemplate,
    registry::TaskTemplateRegistry,
    events::InProcessEventSystem,
};

use crate::worker::{
    coordinator::WorkerLoopCoordinator,
    event_publisher::EventPublisher,
    event_subscriber::EventSubscriber,
};

/// Worker foundation core bootstrap system
/// Mirrors OrchestrationCore patterns for consistency
pub struct WorkerCore {
    config_manager: Arc<ConfigManager>,
    database_connection: Arc<Connection>,
    messaging_client: Arc<UnifiedPgmqClient>,
    coordinator: Arc<RwLock<WorkerLoopCoordinator>>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    event_system: Arc<InProcessEventSystem>,
    event_publisher: Arc<EventPublisher>,
    event_subscriber: Arc<EventSubscriber>,
}

impl WorkerCore {
    /// Create new WorkerCore instance
    pub async fn new() -> Result<Self> {
        info!("🚀 Initializing WorkerCore...");

        // Load configuration
        let config_manager = Arc::new(ConfigManager::load_worker_config().await?);
        let worker_config = config_manager.worker_config();

        // Initialize database connection with worker-specific pool
        let database_connection = Arc::new(
            Connection::new_with_worker_pool(&worker_config.database_pools).await?
        );

        // Initialize messaging client
        let messaging_client = Arc::new(
            UnifiedPgmqClient::new_for_worker(&worker_config.messaging).await?
        );

        // Initialize task template registry
        let task_template_registry = Arc::new(
            TaskTemplateRegistry::new_for_worker(
                database_connection.clone(),
                &worker_config.namespaces,
            ).await?
        );

        // Initialize in-process event system
        let event_system = Arc::new(
            InProcessEventSystem::new(&worker_config.event_config).await?
        );

        // Initialize event publisher and subscriber
        let event_publisher = Arc::new(
            EventPublisher::new(event_system.clone()).await?
        );
        let event_subscriber = Arc::new(
            EventSubscriber::new(event_system.clone()).await?
        );

        // Initialize coordinator (similar to OrchestrationLoopCoordinator)
        let coordinator = Arc::new(RwLock::new(
            WorkerLoopCoordinator::new(
                config_manager.clone(),
                database_connection.clone(),
                messaging_client.clone(),
                task_template_registry.clone(),
                event_publisher.clone(),
            ).await?
        ));

        info!("✅ WorkerCore initialized successfully");

        Ok(Self {
            config_manager,
            database_connection,
            messaging_client,
            coordinator,
            task_template_registry,
            event_system,
            event_publisher,
            event_subscriber,
        })
    }

    /// Start worker system
    pub async fn start(&self) -> Result<()> {
        info!("🔄 Starting WorkerCore...");

        // Start coordinator (which manages WorkerExecutor pools)
        let mut coordinator = self.coordinator.write().await;
        coordinator.start().await?;

        // Start health monitoring
        self.start_health_monitoring().await?;

        info!("✅ WorkerCore started successfully");
        Ok(())
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 Shutting down WorkerCore...");

        // Signal graceful shutdown to coordinator
        let mut coordinator = self.coordinator.write().await;
        coordinator.transition_to_graceful_shutdown().await?;

        // Wait for coordinator to finish processing
        coordinator.wait_for_shutdown().await?;

        info!("✅ WorkerCore shutdown completed");
        Ok(())
    }

    /// Get event subscriber for registering step handlers
    pub fn event_subscriber(&self) -> Arc<EventSubscriber> {
        self.event_subscriber.clone()
    }

    async fn start_health_monitoring(&self) -> Result<()> {
        // Implementation similar to OrchestrationCore health monitoring
        // but focused on worker-specific metrics
        Ok(())
    }
}
```

#### 2.3 Worker Module Structure
```rust
// tasker-worker/src/lib.rs
pub mod worker;
pub mod event_publisher;
pub mod event_subscriber;

pub use worker::core::WorkerCore;
pub use event_publisher::EventPublisher;
pub use event_subscriber::EventSubscriber;

// tasker-worker/src/worker/mod.rs
pub mod core;
pub mod coordinator;
pub mod executor_pool;
pub mod health;
pub mod api;

pub use core::WorkerCore;

// tasker-worker/src/event_publisher/mod.rs
use std::sync::Arc;
use anyhow::Result;
use uuid::Uuid;

use tasker_shared::{
    models::{Task, WorkflowStep, Sequence},
    events::InProcessEventSystem,
    types::StepEventPayload,
};

/// Event publisher for step execution events
pub struct EventPublisher {
    event_system: Arc<InProcessEventSystem>,
}

impl EventPublisher {
    pub async fn new(event_system: Arc<InProcessEventSystem>) -> Result<Self> {
        Ok(Self { event_system })
    }

    /// Fire step execution event with (task, sequence, step) payload
    pub async fn fire_step_event(
        &self,
        task: Task,
        sequence: Sequence,
        step: WorkflowStep,
    ) -> Result<()> {
        let payload = StepEventPayload {
            task,
            sequence,
            step,
        };

        self.event_system.publish_step_event(payload).await?;
        Ok(())
    }
}

// tasker-worker/src/event_subscriber/mod.rs
use std::sync::Arc;
use anyhow::Result;

use tasker_shared::{
    events::InProcessEventSystem,
    types::{StepEventPayload, StepExecutionResult},
};

/// Event subscriber for handling step execution events
pub struct EventSubscriber {
    event_system: Arc<InProcessEventSystem>,
}

impl EventSubscriber {
    pub async fn new(event_system: Arc<InProcessEventSystem>) -> Result<Self> {
        Ok(Self { event_system })
    }

    /// Subscribe to step events with a handler function
    pub async fn subscribe_to_step_events<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(StepEventPayload) -> Result<StepExecutionResult> + Send + Sync + 'static,
    {
        self.event_system.subscribe_step_handler(handler).await?;
        Ok(())
    }
}
```

### Phase 3: Worker Loop Coordinator (Week 2-3)

#### 3.1 WorkerLoopCoordinator Implementation
```rust
// tasker-worker/src/worker/coordinator/mod.rs
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use anyhow::Result;
use tracing::{info, warn, error};

use tasker_shared::{
    config::ConfigManager,
    messaging::UnifiedPgmqClient,
    database::Connection,
    types::SystemOperationalState,
    registry::TaskTemplateRegistry,
    models::{Task, WorkflowStep, Sequence},
    state_machine::{WorkflowStepState, TaskState},
};

use crate::worker::{
    executor_pool::WorkerExecutorPool,
    event_publisher::EventPublisher,
};

/// Worker coordinator mirroring OrchestrationLoopCoordinator patterns
/// Manages WorkerExecutor pools that listen on namespaced queues
pub struct WorkerLoopCoordinator {
    config_manager: Arc<ConfigManager>,
    database_connection: Arc<Connection>,
    messaging_client: Arc<UnifiedPgmqClient>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    event_publisher: Arc<EventPublisher>,

    // WorkerExecutor pools per namespace (mirroring OrchestratorExecutor pattern)
    namespace_pools: HashMap<String, WorkerExecutorPool>,

    // Health and scaling management
    health_monitor: Arc<WorkerHealthMonitor>,
    resource_validator: Arc<WorkerResourceValidator>,
    operational_state: Arc<RwLock<SystemOperationalState>>,

    // Background tasks
    coordinator_handle: Option<JoinHandle<()>>,
    health_monitor_handle: Option<JoinHandle<()>>,
}

impl WorkerLoopCoordinator {
    pub async fn new(
        config_manager: Arc<ConfigManager>,
        database_connection: Arc<Connection>,
        messaging_client: Arc<UnifiedPgmqClient>,
        task_template_registry: Arc<TaskTemplateRegistry>,
        event_publisher: Arc<EventPublisher>,
    ) -> Result<Self> {
        let worker_config = config_manager.worker_config();

        // Initialize WorkerExecutor pools for each namespace
        let mut namespace_pools = HashMap::new();
        for namespace in &worker_config.namespaces {
            let pool = WorkerExecutorPool::new(
                namespace.clone(),
                messaging_client.clone(),
                database_connection.clone(),
                task_template_registry.clone(),
                event_publisher.clone(),
                &worker_config.executor_pools,
            ).await?;
            namespace_pools.insert(namespace.clone(), pool);
        }

        // Initialize health monitoring
        let health_monitor = Arc::new(
            WorkerHealthMonitor::new(&worker_config.health_monitoring).await?
        );

        // Initialize resource validation
        let resource_validator = Arc::new(
            WorkerResourceValidator::new(&worker_config.resource_limits).await?
        );

        let operational_state = Arc::new(RwLock::new(SystemOperationalState::Startup));

        Ok(Self {
            config_manager,
            database_connection,
            messaging_client,
            task_template_registry,
            event_publisher,
            namespace_pools,
            health_monitor,
            resource_validator,
            operational_state,
            coordinator_handle: None,
            health_monitor_handle: None,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("🔄 Starting WorkerLoopCoordinator...");

        // Transition to normal operational state
        {
            let mut state = self.operational_state.write().await;
            *state = SystemOperationalState::Normal;
        }

        // Start WorkerExecutor pools (each listens on namespaced queues)
        for (namespace, pool) in self.namespace_pools.iter_mut() {
            info!("🔄 Starting WorkerExecutor pool for namespace: {}", namespace);
            pool.start().await?;
        }

        // Start coordinator loop
        self.coordinator_handle = Some(self.spawn_coordinator_loop().await);

        // Start health monitoring
        self.health_monitor_handle = Some(self.spawn_health_monitor().await);

        info!("✅ WorkerLoopCoordinator started successfully");
        Ok(())
    }

    pub async fn transition_to_graceful_shutdown(&mut self) -> Result<()> {
        info!("🛑 Transitioning WorkerLoopCoordinator to graceful shutdown...");

        // Update operational state
        {
            let mut state = self.operational_state.write().await;
            *state = SystemOperationalState::GracefulShutdown;
        }

        // Signal shutdown to WorkerExecutor pools
        for pool in self.namespace_pools.values_mut() {
            pool.initiate_graceful_shutdown().await?;
        }

        Ok(())
    }

    async fn spawn_coordinator_loop(&self) -> JoinHandle<()> {
        let operational_state = self.operational_state.clone();
        let namespace_pools = self.namespace_pools.clone();
        let health_monitor = self.health_monitor.clone();

        tokio::spawn(async move {
            loop {
                let state = *operational_state.read().await;

                match state {
                    SystemOperationalState::Normal => {
                        // Perform auto-scaling decisions
                        Self::manage_auto_scaling(&namespace_pools, &health_monitor).await;

                        // Resource validation
                        Self::validate_system_resources(&namespace_pools).await;
                    }
                    SystemOperationalState::GracefulShutdown => {
                        // Wait for pools to finish current work
                        if Self::all_pools_idle(&namespace_pools).await {
                            break;
                        }
                    }
                    SystemOperationalState::Stopped => break,
                    _ => {
                        // Handle other states
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        })
    }

    async fn manage_auto_scaling(
        namespace_pools: &HashMap<String, WorkerExecutorPool>,
        health_monitor: &WorkerHealthMonitor,
    ) {
        for (namespace, pool) in namespace_pools {
            let metrics = health_monitor.get_namespace_metrics(namespace).await;

            // Auto-scaling logic based on queue depth and processing rate
            if metrics.queue_depth > 100 && metrics.processing_rate < 10.0 {
                pool.scale_up().await.unwrap_or_else(|e| {
                    error!("Failed to scale up namespace {}: {}", namespace, e);
                });
            } else if metrics.queue_depth < 10 && metrics.processing_rate > 50.0 {
                pool.scale_down().await.unwrap_or_else(|e| {
                    error!("Failed to scale down namespace {}: {}", namespace, e);
                });
            }
        }
    }

    async fn validate_system_resources(namespace_pools: &HashMap<String, WorkerExecutorPool>) {
        // Resource validation logic
        // Similar to OrchestrationLoopCoordinator but focused on worker resources
    }

    async fn all_pools_idle(namespace_pools: &HashMap<String, WorkerExecutorPool>) -> bool {
        for pool in namespace_pools.values() {
            if !pool.is_idle().await {
                return false;
            }
        }
        true
    }
}

// tasker-worker/src/worker/executor_pool/mod.rs
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use anyhow::Result;
use tracing::{info, debug, error};

use tasker_shared::{
    messaging::UnifiedPgmqClient,
    database::Connection,
    registry::TaskTemplateRegistry,
    models::{Task, WorkflowStep, Sequence},
    state_machine::{WorkflowStepState, TaskState},
    types::QueueMessageData,
};

use crate::worker::{
    executor::WorkerExecutor,
    event_publisher::EventPublisher,
};

/// Pool of WorkerExecutors for a specific namespace
/// Mirrors OrchestratorExecutorPool patterns
pub struct WorkerExecutorPool {
    namespace: String,
    messaging_client: Arc<UnifiedPgmqClient>,
    database_connection: Arc<Connection>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    event_publisher: Arc<EventPublisher>,

    // Pool of WorkerExecutors
    executors: Vec<Arc<WorkerExecutor>>,
    executor_handles: Vec<JoinHandle<()>>,

    // Pool configuration
    initial_pool_size: usize,
    max_pool_size: usize,
    current_pool_size: Arc<RwLock<usize>>,

    // Queue configuration
    queue_name: String,
    visibility_timeout: u32,
    batch_size: u32,
}

impl WorkerExecutorPool {
    pub async fn new(
        namespace: String,
        messaging_client: Arc<UnifiedPgmqClient>,
        database_connection: Arc<Connection>,
        task_template_registry: Arc<TaskTemplateRegistry>,
        event_publisher: Arc<EventPublisher>,
        pool_config: &ExecutorPoolConfig,
    ) -> Result<Self> {
        let queue_name = format!("{}_queue", namespace);

        // Create initial WorkerExecutors
        let mut executors = Vec::new();
        for i in 0..pool_config.initial_pool_size {
            let executor = Arc::new(
                WorkerExecutor::new(
                    format!("{}_executor_{}", namespace, i),
                    namespace.clone(),
                    messaging_client.clone(),
                    database_connection.clone(),
                    task_template_registry.clone(),
                    event_publisher.clone(),
                ).await?
            );
            executors.push(executor);
        }

        let current_pool_size = Arc::new(RwLock::new(pool_config.initial_pool_size));

        Ok(Self {
            namespace,
            messaging_client,
            database_connection,
            task_template_registry,
            event_publisher,
            executors,
            executor_handles: Vec::new(),
            initial_pool_size: pool_config.initial_pool_size,
            max_pool_size: pool_config.max_pool_size,
            current_pool_size,
            queue_name,
            visibility_timeout: pool_config.visibility_timeout,
            batch_size: pool_config.batch_size,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("🔄 Starting WorkerExecutorPool for namespace: {}", self.namespace);

        // Start each WorkerExecutor in the pool
        for executor in &self.executors {
            let handle = self.spawn_executor_loop(executor.clone()).await;
            self.executor_handles.push(handle);
        }

        info!("✅ WorkerExecutorPool started for namespace: {}", self.namespace);
        Ok(())
    }

    async fn spawn_executor_loop(&self, executor: Arc<WorkerExecutor>) -> JoinHandle<()> {
        let queue_name = self.queue_name.clone();
        let messaging_client = self.messaging_client.clone();
        let visibility_timeout = self.visibility_timeout;
        let batch_size = self.batch_size;

        tokio::spawn(async move {
            loop {
                // Listen on namespaced queue for step messages
                match messaging_client.read_step_messages(&queue_name, visibility_timeout, batch_size).await {
                    Ok(messages) => {
                        for message_data in messages {
                            if let Err(e) = executor.process_step_message(message_data).await {
                                error!("WorkerExecutor failed to process message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to read messages from queue {}: {}", queue_name, e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }

                // Brief pause to prevent tight polling
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        })
    }

    pub async fn scale_up(&self) -> Result<()> {
        let current_size = *self.current_pool_size.read().await;
        if current_size < self.max_pool_size {
            info!("Scaling up WorkerExecutorPool for namespace: {}", self.namespace);
            // Implementation for adding more WorkerExecutors
        }
        Ok(())
    }

    pub async fn scale_down(&self) -> Result<()> {
        let current_size = *self.current_pool_size.read().await;
        if current_size > self.initial_pool_size {
            info!("Scaling down WorkerExecutorPool for namespace: {}", self.namespace);
            // Implementation for removing WorkerExecutors
        }
        Ok(())
    }

    pub async fn is_idle(&self) -> bool {
        // Check if all WorkerExecutors in the pool are idle
        for executor in &self.executors {
            if !executor.is_idle().await {
                return false;
            }
        }
        true
    }

    pub async fn initiate_graceful_shutdown(&mut self) -> Result<()> {
        info!("🛑 Initiating graceful shutdown for WorkerExecutorPool: {}", self.namespace);

        // Signal shutdown to all WorkerExecutors
        for executor in &self.executors {
            executor.initiate_graceful_shutdown().await?;
        }

        Ok(())
    }
}
```

### Phase 4: WorkerExecutor Implementation (Week 3)

#### 4.1 WorkerExecutor Core Implementation
The WorkerExecutor follows the same patterns as OrchestratorExecutor but focuses on step processing:

```rust
// tasker-worker/src/worker/executor/mod.rs
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::Result;
use tracing::{info, debug, error};

use tasker_shared::{
    messaging::UnifiedPgmqClient,
    database::Connection,
    registry::TaskTemplateRegistry,
    models::{Task, WorkflowStep, Sequence},
    state_machine::{WorkflowStepState, TaskState},
    types::{QueueMessageData, StepEventPayload},
    log_queue_worker,
};

use crate::worker::event_publisher::EventPublisher;

/// Individual WorkerExecutor that processes steps from namespaced queues
/// Mirrors OrchestratorExecutor patterns for consistency
pub struct WorkerExecutor {
    executor_id: String,
    namespace: String,
    messaging_client: Arc<UnifiedPgmqClient>,
    database_connection: Arc<Connection>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    event_publisher: Arc<EventPublisher>,

    // Executor state
    is_running: AtomicBool,
    is_shutting_down: AtomicBool,
    currently_processing: AtomicBool,
}

impl WorkerExecutor {
    pub async fn new(
        executor_id: String,
        namespace: String,
        messaging_client: Arc<UnifiedPgmqClient>,
        database_connection: Arc<Connection>,
        task_template_registry: Arc<TaskTemplateRegistry>,
        event_publisher: Arc<EventPublisher>,
    ) -> Result<Self> {
        log_queue_worker!(info, "Creating WorkerExecutor",
            namespace: namespace,
            executor_id: executor_id
        );

        Ok(Self {
            executor_id,
            namespace,
            messaging_client,
            database_connection,
            task_template_registry,
            event_publisher,
            is_running: AtomicBool::new(false),
            is_shutting_down: AtomicBool::new(false),
            currently_processing: AtomicBool::new(false),
        })
    }

    /// Process a step message from the queue
    pub async fn process_step_message(&self, message_data: QueueMessageData) -> Result<()> {
        if self.is_shutting_down.load(Ordering::Relaxed) {
            return Ok(()); // Skip processing during shutdown
        }

        self.currently_processing.store(true, Ordering::Relaxed);

        let result = self.process_step_message_internal(message_data).await;

        self.currently_processing.store(false, Ordering::Relaxed);

        result
    }

    async fn process_step_message_internal(&self, message_data: QueueMessageData) -> Result<()> {
        let step_message = message_data.step_message;

        log_queue_worker!(debug, "Processing step message",
            namespace: self.namespace,
            executor_id: self.executor_id,
            step_id: step_message.step_id,
            task_id: step_message.task_id
        );

        // 1. Evaluate step against task template registry
        let can_handle = self.can_handle_step(&step_message).await?;
        if !can_handle {
            log_queue_worker!(debug, "Step not supported by this executor",
                namespace: self.namespace,
                executor_id: self.executor_id,
                step_id: step_message.step_id
            );
            return Ok(()); // Skip this step, another executor may handle it
        }

        // 2. Claim step via state machine transition
        let claimed = self.claim_step(&step_message).await?;
        if !claimed {
            log_queue_worker!(debug, "Failed to claim step",
                namespace: self.namespace,
                executor_id: self.executor_id,
                step_id: step_message.step_id
            );
            return Ok(()); // Another executor claimed it
        }

        // 3. Delete message from queue (claimed successfully)
        self.messaging_client.delete_message(&message_data.queue_message).await?;

        // 4. Hydrate full task, sequence, and step data
        let (task, sequence, step) = self.hydrate_step_data(&step_message).await?;

        // 5. Fire in-process event with (task, sequence, step) payload
        self.event_publisher.fire_step_event(task, sequence, step).await?;

        log_queue_worker!(info, "Step processing initiated",
            namespace: self.namespace,
            executor_id: self.executor_id,
            step_id: step_message.step_id,
            task_id: step_message.task_id
        );

        Ok(())
    }

    /// Evaluate step against task template registry to determine if this executor can handle it
    async fn can_handle_step(&self, step_message: &StepMessage) -> Result<bool> {
        let task_template = self.task_template_registry.get_task_template(
            &step_message.namespace,
            &step_message.task_name,
            &step_message.task_version,
        ).await?;

        if let Some(template) = task_template {
            // Check if this task template has the step we're looking for
            let has_step = template.step_templates.iter()
                .any(|step_template| step_template.name == step_message.step_name);

            log_queue_worker!(debug, "Task template evaluation",
                namespace: self.namespace,
                executor_id: self.executor_id,
                task_name: step_message.task_name,
                step_name: step_message.step_name,
                has_step: has_step
            );

            Ok(has_step)
        } else {
            log_queue_worker!(warn, "Task template not found",
                namespace: self.namespace,
                executor_id: self.executor_id,
                task_name: step_message.task_name,
                task_version: step_message.task_version
            );
            Ok(false)
        }
    }

    /// Claim step via state machine transition (atomic operation)
    async fn claim_step(&self, step_message: &StepMessage) -> Result<bool> {
        // Use state machine to atomically transition step to "in_progress"
        let transition_result = WorkflowStepState::transition_to_in_progress(
            &self.database_connection,
            step_message.step_id,
            Some(&self.executor_id),
        ).await?;

        Ok(transition_result.success)
    }

    /// Hydrate full task, sequence, and step data for event payload
    async fn hydrate_step_data(&self, step_message: &StepMessage) -> Result<(Task, Sequence, Step)> {
        // Load full task data
        let task = Task::find_by_uuid(&self.database_connection, step_message.task_id).await?;

        // Load full step data
        let step = WorkflowStep::find_by_uuid(&self.database_connection, step_message.step_id).await?;

        // Build sequence using transitive dependencies approach
        let sequence = Sequence::build_for_step(&self.database_connection, &step).await?;

        Ok((task, sequence, step))
    }

    pub async fn is_idle(&self) -> bool {
        !self.currently_processing.load(Ordering::Relaxed)
    }

    pub async fn initiate_graceful_shutdown(&self) -> Result<()> {
        log_queue_worker!(info, "Initiating graceful shutdown",
            namespace: self.namespace,
            executor_id: self.executor_id
        );

        self.is_shutting_down.store(true, Ordering::Relaxed);
        Ok(())
    }
}
```

#### 4.2 Event-Driven Architecture
The worker foundation uses in-process events to coordinate between queue processing and step handlers:

```rust
// tasker-worker/src/event_system/mod.rs
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use anyhow::Result;

use tasker_shared::{
    types::{StepEventPayload, StepExecutionResult},
};

/// In-process event system for step execution coordination
pub struct InProcessEventSystem {
    step_handlers: Arc<RwLock<HashMap<String, StepHandlerCallback>>>,
    result_handlers: Arc<RwLock<HashMap<String, ResultHandlerCallback>>>,
}

type StepHandlerCallback = Box<dyn Fn(StepEventPayload) -> Result<StepExecutionResult> + Send + Sync>;
type ResultHandlerCallback = Box<dyn Fn(StepExecutionResult) -> Result<()> + Send + Sync>;

impl InProcessEventSystem {
    pub async fn new(config: &EventConfig) -> Result<Self> {
        Ok(Self {
            step_handlers: Arc::new(RwLock::new(HashMap::new())),
            result_handlers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Publish step event to registered handlers
    pub async fn publish_step_event(&self, payload: StepEventPayload) -> Result<()> {
        let handlers = self.step_handlers.read().await;

        // Find appropriate handler based on step name
        let step_name = &payload.step.named_step.name;
        if let Some(handler) = handlers.get(step_name) {
            let result = handler(payload)?;

            // Publish result event
            self.publish_result_event(result).await?;
        }

        Ok(())
    }

    /// Subscribe to step events with a handler function
    pub async fn subscribe_step_handler<F>(&self, step_name: String, handler: F) -> Result<()>
    where
        F: Fn(StepEventPayload) -> Result<StepExecutionResult> + Send + Sync + 'static,
    {
        let mut handlers = self.step_handlers.write().await;
        handlers.insert(step_name, Box::new(handler));
        Ok(())
    }

    /// Publish result event to registered handlers
    pub async fn publish_result_event(&self, result: StepExecutionResult) -> Result<()> {
        let handlers = self.result_handlers.read().await;

        // Notify all result handlers
        for handler in handlers.values() {
            handler(result.clone())?;
        }

        Ok(())
    }
}
```

#### 4.3 Language-Specific Integration Patterns

**Native Rust (tasker-worker-rust/):**
```rust
// Direct integration with in-process event system
event_subscriber.subscribe_to_step_events("generate_even_number", |payload| {
    let handler = LinearWorkflowHandlers::new();
    handler.generate_even_number(payload.task, payload.sequence, payload.step)
}).await?;
```

**Ruby FFI (bindings/ruby/):**
```rust
// FFI bridge subscribes to events and forwards to Ruby
event_subscriber.subscribe_to_step_events("generate_even_number", |payload| {
    ruby_ffi_bridge::fire_dry_event("step_execution", payload)
}).await?;

// Ruby side uses dry-events to handle:
// TaskerCore::EventSubscriber.subscribe("step_execution") do |payload|
//   handler = OrderFulfillment::StepHandlers::GenerateEvenNumber.new
//   handler.call(payload[:task], payload[:sequence], payload[:step])
// end
```

**WASM (js/ts compiled to WASM):**
```rust
// WASM runtime subscribes to events
event_subscriber.subscribe_to_step_events("generate_even_number", |payload| {
    wasm_runtime::call_wasm_handler("generate_even_number", payload)
}).await?;
```

### Phase 5: Worker API and Health Monitoring (Week 3-4)

#### 5.1 Worker Web API
```rust
// tasker-worker/src/worker/api/mod.rs
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::worker::core::WorkerCore;

#[derive(Clone)]
pub struct ApiState {
    worker_core: Arc<WorkerCore>,
}

/// Worker health status
#[derive(Serialize)]
pub struct WorkerHealthStatus {
    pub status: String,
    pub namespaces: Vec<NamespaceHealth>,
    pub system_metrics: SystemMetrics,
    pub uptime_seconds: u64,
}

#[derive(Serialize)]
pub struct NamespaceHealth {
    pub namespace: String,
    pub queue_depth: u64,
    pub processing_rate: f64,
    pub active_executors: u32,
    pub health_status: String,
}

pub fn create_worker_api(worker_core: Arc<WorkerCore>) -> Router {
    let api_state = ApiState { worker_core };

    Router::new()
        .route("/health", get(health_check))
        .route("/health/ready", get(readiness_check))
        .route("/health/live", get(liveness_check))
        .route("/metrics", get(metrics))
        .route("/status", get(worker_status))
        .with_state(api_state)
}

/// Kubernetes readiness probe endpoint
async fn readiness_check(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, StatusCode> {
    // Check if worker can accept new work
    Ok(Json(serde_json::json!({
        "status": "ready",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Kubernetes liveness probe endpoint
async fn liveness_check(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, StatusCode> {
    // Check if worker is alive and functional
    Ok(Json(serde_json::json!({
        "status": "alive",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Comprehensive health check
async fn health_check(State(state): State<ApiState>) -> Result<Json<WorkerHealthStatus>, StatusCode> {
    // Implementation
    todo!()
}

/// Prometheus metrics endpoint
async fn metrics(State(state): State<ApiState>) -> Result<String, StatusCode> {
    // Return Prometheus-formatted metrics
    todo!()
}
```

### Phase 6: Integration and Testing (Week 4)

#### 6.1 Comprehensive Integration Tests
```rust
// tasker-worker/tests/integration/worker_system_test.rs
use tokio::time::{sleep, Duration};
use anyhow::Result;

use tasker_worker_foundation::WorkerCore;
use tasker_shared::{
    models::{Task, WorkflowStep},
    config::ConfigManager,
    types::StepExecutionRequest,
};

#[tokio::test]
async fn test_worker_foundation_end_to_end() -> Result<()> {
    // Setup test environment
    let config_manager = ConfigManager::load_test_config().await?;
    let worker_core = WorkerCore::new_with_config(config_manager).await?;

    // Start worker system
    worker_core.start().await?;

    // Create test task and step
    let task = create_test_task().await?;
    let step = create_test_step(&task).await?;

    // Enqueue step for processing
    enqueue_step_for_worker(&step).await?;

    // Wait for processing
    sleep(Duration::from_secs(2)).await;

    // Verify step was processed correctly
    let processed_step = get_step_by_id(&step.step_id).await?;
    assert_eq!(processed_step.state, "completed");

    // Shutdown worker
    worker_core.shutdown().await?;

    Ok(())
}

#[tokio::test]
async fn test_ruby_ffi_integration() -> Result<()> {
    // Test Ruby FFI bridge integration
    todo!()
}

#[tokio::test]
async fn test_worker_auto_scaling() -> Result<()> {
    // Test auto-scaling functionality
    todo!()
}

#[tokio::test]
async fn test_graceful_shutdown() -> Result<()> {
    // Test graceful shutdown behavior
    todo!()
}
```

## Success Criteria

### Quantitative Metrics
- ✅ **Performance**: Worker system handles 1000+ steps/second from initial release
- ✅ **Resource Usage**: < 10% FFI overhead compared to native execution
- ✅ **Shutdown Time**: Graceful shutdown completes within 5 seconds
- ✅ **Auto-scaling Response**: Scaling decisions respond within 10 seconds
- ✅ **Code Reduction**: Ruby infrastructure code reduced by 70%+

### Qualitative Outcomes
- ✅ **Clean Architecture**: Clear separation between infrastructure and business logic
- ✅ **Language Agnostic**: Foundation supports multiple binding languages identically
- ✅ **Production Ready**: Complete Docker deployment and Kubernetes integration
- ✅ **Zero Race Conditions**: Atomic step claiming and processing guarantees
- ✅ **Comprehensive Monitoring**: Full observability from day one

### Integration Requirements
- ✅ **Existing Workflow Compatibility**: All current Ruby workflow patterns work unchanged
- ✅ **Configuration Consistency**: Same configuration patterns as orchestration system
- ✅ **Database Compatibility**: Uses same database models and schema
- ✅ **Queue Integration**: Works with existing PGMQ and future TAS-35 abstractions

## Risk Mitigation

### Technical Risks
- **FFI Complexity**: Start with Ruby bridge, expand incrementally
- **Performance Overhead**: Benchmark early and optimize critical paths
- **Resource Contention**: Separate database pools and careful resource management

### Operational Risks
- **Migration Complexity**: Maintain backward compatibility during transition
- **Deployment Changes**: Clear documentation and migration guides
- **Monitoring Gaps**: Comprehensive metrics and health checking from day one

This foundation enables TAS-41 (pure Rust worker) and TAS-42 (simplified Ruby bindings) while providing a robust, scalable infrastructure for step execution across multiple binding languages.

---

## APPENDIX: Test Infrastructure Analysis and Migration Plan

### Current Test Analysis (tasker-orchestration/tests - 20 files, 5,716 lines)

Based on compilation failures and architecture changes, tests fall into clear categories:

#### 🚨 REMOVE - Obsolete Architecture Tests (6 files, ~1,500 lines)
**Reason**: Test removed/renamed components that no longer exist

1. **`tests/mod.rs`** - References missing modules (`circuit_breaker`, `coordinator`)
2. **`integration_executor_toml_config.rs`** - Tests removed `executor` module  
3. **`unified_bootstrap_test.rs`** - Tests obsolete `OrchestrationCore` API
4. **`configuration_integration_test.rs`** - Tests removed `WorkflowCoordinatorConfig`
5. **`messaging/task_request_processor_test.rs`** - Tests removed `task_request_processor`
6. **`task_initializer_test.rs`** - Tests removed `StepTemplate`, `HandlerConfiguration`

#### ✅ KEEP - Core Architecture Tests (8 files, ~2,200 lines)
**Reason**: Test current/stable orchestration features

1. **`web/`** directory (8 files) - Web API tests still relevant
   - `test_analytics_endpoints.rs` - Analytics API validation
   - `authenticated_tests.rs` - JWT authentication  
   - `tls_tests.rs` - HTTPS/TLS integration
   - `resource_coordination_tests.rs` - Database pool coordination
   - `test_infrastructure.rs` - Web test utilities
   - `test_openapi_documentation.rs` - API documentation validation

#### 🔄 MIGRATE - Architecture-Specific Tests (3 files, ~1,200 lines)  
**Reason**: Valuable test patterns but need architecture updates

1. **`tas_37_finalization_race_condition_test.rs`** - **MIGRATE TO tasker-worker**
   - Tests finalization claiming (worker responsibility in command pattern)
   - Uses excellent `#[sqlx::test]` patterns
   - Race condition prevention still critical for workers

2. **`complex_workflow_integration_test.rs`** - **MIGRATE TO tasker-worker**
   - Tests workflow execution (worker responsibility in command pattern)
   - Good integration test patterns with TaskInitializer → WorkerTaskInitializer
   - End-to-end workflow validation patterns

3. **`state_manager.rs`** - **EVALUATE FOR tasker-shared**
   - State management patterns might be shared between orchestration and worker
   - Could provide common state transition utilities

#### 🔨 REWRITE - Command Pattern Tests (3 files, ~800 lines)
**Reason**: Good test structure but need complete architectural pattern changes

1. **`config_integration_test.rs`** - Update for component-based configuration  
2. **`messaging/mod.rs`** - Update for pgmq-based messaging patterns
3. Create new **`command_pattern_integration_test.rs`** - Test event-driven architecture

### Migration Strategy Implementation

#### Step 1: Clean House (Remove Obsolete Tests)
```bash
# Remove files that test deleted architecture components
rm tasker-orchestration/tests/integration_executor_toml_config.rs      # executor module removed
rm tasker-orchestration/tests/unified_bootstrap_test.rs                # OrchestrationCore API changed  
rm tasker-orchestration/tests/configuration_integration_test.rs        # WorkflowCoordinatorConfig removed
rm tasker-orchestration/tests/messaging/task_request_processor_test.rs # task_request_processor removed
rm tasker-orchestration/tests/task_initializer_test.rs                 # StepTemplate/HandlerConfiguration removed

# Fix tests/mod.rs to remove missing module references
# Remove: pub mod circuit_breaker; pub mod coordinator;
```

#### Step 2: Migrate Worker-Specific Tests  
```bash
# Move tests that belong in tasker-worker (step processing, finalization, workflow execution)
mv tasker-orchestration/tests/tas_37_finalization_race_condition_test.rs \
   tasker-worker/tests/finalization_race_condition_test.rs

mv tasker-orchestration/tests/complex_workflow_integration_test.rs \
   tasker-worker/tests/workflow_integration_test.rs

mv tasker-orchestration/tests/state_manager.rs \
   tasker-shared/src/testing/state_manager.rs  # If shared patterns identified
```

#### Step 3: Create Command Pattern Tests
```rust
// tasker-orchestration/tests/command_pattern_integration_test.rs
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_task_creation_command_flow(pool: PgPool) {
    // Test: HTTP Request → TaskCreationCommand → Database → TaskCreatedEvent
    // Verify orchestration system handles commands and events properly
}

#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]  
async fn test_orchestration_worker_coordination(pool: PgPool) {
    // Test: Orchestration enqueues step commands → Worker processes → Results flow back
    // Verify end-to-end command pattern coordination
}

// tasker-worker/tests/worker_command_consumption_test.rs
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_worker_consumes_step_commands(pool: PgPool) {
    // Test: Worker polls step queue → Processes step command → Publishes result event
    // Verify worker properly handles commands and publishes results
}
```

#### Step 4: Update Remaining Tests for Architecture Changes
```rust
// Update imports throughout remaining test files:
// OLD: use tasker_orchestration::orchestration::executor::*;
// NEW: use tasker_orchestration::orchestration::command_processors::*;

// OLD: use tasker_orchestration::orchestration::orchestration_loop::*;  
// NEW: use tasker_orchestration::orchestration::event_loop::*;

// Update API calls:
// OLD: OrchestrationCore::new().await?
// NEW: OrchestrationCore::new(context).await?

// OLD: config.executor_pools()
// NEW: config.command_processors()

// OLD: TaskInitializer::for_testing()
// NEW: CommandProcessor::for_testing() // In orchestration tests
//      WorkerTaskInitializer::for_testing() // In worker tests
```

### Test Pattern Standardization

All new and migrated tests must follow these patterns:

```rust
// tasker-worker tests
use tasker_worker::testing::{WorkerTestFactory, WorkerTestDatabase};

#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_worker_functionality(pool: PgPool) {
    let factory = WorkerTestFactory::new(Arc::new(pool.clone()));
    let test_data = WorkerTestData::new("test_step_processing")
        .build_with_factory(&factory)
        .await?;
    
    // Test step processing logic
    assert!(test_data.foundation().is_some());
}

// tasker-orchestration tests  
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_orchestration_functionality(pool: PgPool) {
    // Use simplified database patterns from tasker_shared::test_utils
    let utils = tasker_shared::database::DatabaseConnection::from_pool(pool);
    
    // Test command pattern orchestration
}
```

### Expected Outcomes After Migration

#### Quantitative Results
- **Reduced Test Files**: From 20 to ~14 files (6 removed, 3 migrated)
- **Maintained Coverage**: Core functionality still tested, architecture-aligned
- **Clean Compilation**: Zero architecture mismatch compilation errors
- **Standard Patterns**: 100% of tests use `#[sqlx::test(migrator = "...")]` pattern

#### Qualitative Benefits  
- **Clear Responsibility Separation**: Orchestration tests focus on command pattern, worker tests focus on step processing
- **Architecture Consistency**: Tests reflect actual command pattern architecture
- **Maintainable**: Standard sqlx test patterns instead of complex custom database management
- **Future-Ready**: Foundation supports TAS-41 (pure Rust worker) testing needs

This comprehensive analysis provides the detailed foundation for implementing the test migration phase of TAS-40.

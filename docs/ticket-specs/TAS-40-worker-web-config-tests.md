# TAS-40 Worker Web API, Configuration Cleanup, and Test Modernization Plan

**Status**: Planning Phase - UPDATED with Critical Integration Components  
**Priority**: High  
**Estimated Duration**: 3-4 weeks (Updated from 2-3 weeks)  
**Dependencies**: TAS-40 Command Pattern (Complete), Orchestration Web API Contract

## Executive Summary

Following the completion of TAS-40 Command Pattern implementation, this document outlines the sequential implementation plan for three critical areas:

1. **Worker Web API Implementation**: Create `tasker-worker/src/web` following proven `tasker-orchestration/src/web` patterns **PLUS critical integration components**
2. **Configuration Audit and Cleanup**: Remove obsolete polling-based configuration and consolidate worker/orchestration config **PLUS FFI shared components cleanup**
3. **Test Suite Modernization**: Update test suites to reflect command pattern architecture and remove outdated tests

This plan ensures the worker system has proper observability, the configuration system reflects the new architecture, and the test suite provides comprehensive coverage of the modernized system.

### UPDATED: Critical Integration Components Added

The plan now includes four essential components identified for production-ready worker implementation:

1. **OrchestrationApiClient**: reqwest-based HTTP client for task initialization via POST /v1/tasks
2. **Worker Bootstrap System**: Robust initialization following orchestration patterns but without embedded context  
3. **Task Template Management**: Registry and namespace queue management for distributed worker operation with event-driven step processing
4. **FFI Shared Components Cleanup**: Analysis and cleanup of obsolete tasker-orchestration/src/ffi/shared components

**IMPORTANT ARCHITECTURAL NOTE**: Workers use **non-blocking, event-driven step execution** as described in TAS-40-command-pattern.md. Steps are claimed via state machine transitions, execution events are published to the in-process event system, and FFI handlers execute asynchronously with completion events published back.

---

## Phase 1: Worker Web API Implementation

**Duration**: 1.5-2 weeks (Updated from 1 week)  
**Template Reference**: `docs/ticket-specs/TAS-40-updated.md` lines 1146-1226  
**Pattern Reference**: `tasker-orchestration/src/web/`  
**API Reference**: `tasker-orchestration/src/web/handlers/tasks.rs` (POST /v1/tasks endpoint)  
**Bootstrap Reference**: `bindings/ruby/lib/tasker_core/boot.rb` (Ruby bootstrap patterns)

### 1.1 Core Web Module Structure

Create the following file structure following orchestration patterns:

```
tasker-worker/src/web/
├── mod.rs                 # Main module with create_app function
├── state.rs              # Worker-specific application state
├── handlers/
│   ├── mod.rs
│   ├── health.rs         # Health check endpoints
│   ├── metrics.rs        # Prometheus metrics
│   └── worker_status.rs  # Worker status and info
├── middleware/
│   ├── mod.rs
│   ├── request_id.rs     # Request ID middleware (reuse from orchestration)
│   └── worker_auth.rs    # Simplified worker authentication
├── routes.rs             # Route definitions
└── response_types.rs     # Worker-specific response types
```

### 1.2 Implementation Details

#### 1.2.1 Worker Web State (`tasker-worker/src/web/state.rs`)
```rust
#[derive(Clone)]
pub struct WorkerWebState {
    pub worker_processor: Arc<WorkerProcessor>,
    pub database_pool: Arc<sqlx::PgPool>,
    pub config: WorkerWebConfig,
    pub start_time: std::time::Instant,
}

impl WorkerWebState {
    pub async fn from_worker_core(
        config: WorkerWebConfig,
        worker_processor: Arc<WorkerProcessor>,
        system_context: &SystemContext,
    ) -> TaskerResult<Self> {
        // Implementation following orchestration state pattern
    }
}
```

#### 1.2.2 Health Endpoints (`tasker-worker/src/web/handlers/health.rs`)
Based on TAS-40 template, implement:
- `/health` - Comprehensive worker health including namespace status
- `/health/ready` - Kubernetes readiness probe
- `/health/live` - Kubernetes liveness probe

```rust
#[derive(Serialize)]
pub struct WorkerHealthStatus {
    pub status: String,
    pub namespaces: Vec<NamespaceHealth>,
    pub system_metrics: WorkerSystemMetrics,
    pub uptime_seconds: u64,
    pub worker_id: String,
    pub worker_type: String,
}

#[derive(Serialize)]
pub struct NamespaceHealth {
    pub namespace: String,
    pub queue_depth: u64,
    pub processing_rate: f64,
    pub active_steps: u32,
    pub health_status: String,
    pub last_processed: Option<DateTime<Utc>>,
}
```

#### 1.2.3 Metrics Endpoint (`tasker-worker/src/web/handlers/metrics.rs`)
Prometheus-compatible metrics:
- Step processing rates by namespace
- Error rates and retry counts
- Queue depth and processing latency
- System resource utilization
- Event system metrics (publisher/subscriber stats)

#### 1.2.4 Configuration Integration
Update `config/tasker/base/worker_web.toml` to include:
```toml
# Worker Web API Configuration
enabled = true
bind_address = "0.0.0.0:8081"
request_timeout_ms = 30000

# Authentication (simplified for workers)
authentication_enabled = false
cors_enabled = true

# Metrics collection
metrics_enabled = true
health_check_interval_seconds = 30
```

### 1.3 Integration with Orchestration Patterns

**Reuse from `tasker-orchestration/src/web/`**:
- `middleware/request_id.rs` - Copy directly
- Circuit breaker patterns for database operations
- Response type structures and error handling
- Axum router configuration patterns

**Worker-Specific Adaptations**:
- Simplified authentication (workers typically don't need JWT)
- Worker-specific metrics and health checks
- Event system integration status
- Namespace-focused monitoring

### 1.5 OrchestrationApiClient Implementation

**Purpose**: HTTP client for task initialization via orchestration web API  
**Pattern Reference**: `tasker-orchestration/tests/web/test_infrastructure.rs` (WebTestClient patterns)

#### 1.5.1 Core Client Structure (`tasker-worker/src/orchestration/api_client.rs`)
```rust
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tasker_shared::models::core::task_request::TaskRequest;
use tokio::time::timeout;

#[derive(Debug, Clone)]
pub struct OrchestrationApiClient {
    client: Client,
    base_url: String,
    auth_token: Option<String>,
    timeout: Duration,
    retry_config: RetryConfig,
}

impl OrchestrationApiClient {
    pub fn new(base_url: String, auth_token: Option<String>) -> Result<Self, ClientError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        
        Ok(Self {
            client,
            base_url,
            auth_token,
            timeout: Duration::from_secs(30),
            retry_config: RetryConfig::default(),
        })
    }

    /// Initialize task via POST /v1/tasks endpoint
    pub async fn initialize_task(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskCreationResponse, ClientError> {
        let url = format!("{}/v1/tasks", self.base_url);
        
        let mut request_builder = self.client.post(&url).json(&task_request);
        
        if let Some(token) = &self.auth_token {
            request_builder = request_builder.bearer_auth(token);
        }
        
        // Implement retry logic with exponential backoff
        self.execute_with_retry(request_builder).await
    }
}
```

#### 1.5.2 Response Types (`tasker-worker/src/orchestration/api_types.rs`)
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskCreationResponse {
    pub task_uuid: uuid::Uuid,
    pub status: String,
    pub message: String,
    pub namespace: String,
    pub task_name: String,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_base: f64,
}
```

#### 1.5.3 Integration with Circuit Breaker
```rust
// In OrchestrationApiClient implementation
use tasker_shared::resilience::circuit_breaker::CircuitBreaker;

impl OrchestrationApiClient {
    async fn execute_with_retry(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<TaskCreationResponse, ClientError> {
        // Integrate with circuit breaker pattern from tasker-shared
        // Implement exponential backoff retry logic
        // Handle authentication errors and token refresh
    }
}
```

### 1.6 Worker Bootstrap System

**Purpose**: Robust worker initialization following orchestration patterns but without embedded context  
**Pattern Reference**: `bindings/ruby/lib/tasker_core/boot.rb` and `tasker-orchestration/src/orchestration/core.rs`

#### 1.6.1 Bootstrap Sequence (`tasker-worker/src/bootstrap/mod.rs`)
```rust
use std::sync::Arc;
use tasker_shared::config::{manager::ConfigManager, TaskerConfig};
use sqlx::PgPool;

pub struct WorkerBootstrap {
    config: TaskerConfig,
    database_pool: Arc<PgPool>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    api_client: Arc<OrchestrationApiClient>,
}

impl WorkerBootstrap {
    /// Bootstrap worker system following orchestration patterns
    pub async fn initialize() -> Result<WorkerBootstrap, BootstrapError> {
        // Step 1: Load configuration using ConfigManager (no embedded context)
        let config_manager = ConfigManager::load()?;
        let config = config_manager.config().clone();
        
        // Step 2: Initialize database connection (queue operations only)
        let database_pool = Self::create_database_pool(&config).await?;
        
        // Step 3: Discover and load task templates
        let task_template_registry = Self::initialize_task_templates(
            &database_pool,
            &config
        ).await?;
        
        // Step 4: Initialize OrchestrationApiClient
        let api_client = Self::create_api_client(&config)?;
        
        // Step 5: Initialize namespace queues
        Self::initialize_namespace_queues(
            &database_pool,
            &task_template_registry
        ).await?;
        
        Ok(WorkerBootstrap {
            config,
            database_pool: Arc::new(database_pool),
            task_template_registry: Arc::new(task_template_registry),
            api_client: Arc::new(api_client),
        })
    }

    /// Create database pool for step claiming, result writing, and queue operations
    async fn create_database_pool(config: &TaskerConfig) -> Result<PgPool, BootstrapError> {
        // Database connection for:
        // - Step state machine transitions (claiming steps via step_state_machine.rs)
        // - Writing step results via workflow_step.rs
        // - PGMQ queue operations
        // - SQL function calls for worker operations
    }
}
```

#### 1.6.2 Distinction from OrchestrationCore Bootstrap
- **No Embedded Context**: Worker never operates in embedded mode
- **Extensive Database Access**: Workers query database extensively and use SQL functions
- **Step State Machine**: Workers claim steps via state machine transitions (step_state_machine.rs)
- **Step Results**: Workers write step results through workflow_step.rs
- **API-First**: Always uses OrchestrationApiClient for task initialization  
- **Namespace Focus**: Bootstrap focuses on namespace queue management
- **Distributed Architecture**: Designed for multi-worker coordination

### 1.7 Task Template Management

**Purpose**: Registry and namespace queue management for distributed worker operation  
**Shared Components**: Uses `tasker-shared/src/registry/task_handler_registry.rs` and `tasker-shared/src/models/core/task_template.rs`  
**Event Pattern**: Uses `tasker-worker/src/command_processor.rs` event-driven architecture for non-blocking step execution

#### 1.7.1 Task Template Registry (`tasker-worker/src/templates/registry.rs`)
```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tasker_shared::registry::task_handler_registry::TaskHandlerRegistry;
use tasker_shared::models::core::task_template::TaskTemplate;
use sqlx::PgPool;

/// Local in-memory registry wrapper for tracking worker-supported task templates
/// Uses shared TaskHandlerRegistry for database persistence and TaskTemplate struct
#[derive(Debug, Clone)]
pub struct WorkerTaskTemplateRegistry {
    /// Shared database-backed registry
    shared_registry: TaskHandlerRegistry,
    /// Local tracking of which task templates this worker supports
    supported_templates: Arc<RwLock<HashMap<String, TaskTemplate>>>,
    namespace_queues: Arc<RwLock<HashMap<String, String>>>,
}

impl WorkerTaskTemplateRegistry {
    /// Initialize registry using shared TaskHandlerRegistry
    pub async fn initialize(database_pool: PgPool) -> Result<Self, RegistryError> {
        // Use shared TaskHandlerRegistry for database operations
        let shared_registry = TaskHandlerRegistry::new(database_pool);
        
        // Discover task templates from local config paths during bootstrapping
        // Register them via shared_registry to database
        // Track locally which ones this worker supports
        
        Ok(Self {
            shared_registry,
            supported_templates: Arc::new(RwLock::new(HashMap::new())),
            namespace_queues: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Check if worker can handle a step using task.rs:793 "for orchestration" pattern
    pub async fn can_handle_step(&self, step_message: &SimpleStepMessage) -> Result<bool, RegistryError> {
        // Use tasker_shared::models::core::task.rs:793 "for orchestration" pattern
        // Load task from database using step_message.task_uuid
        // Extract task's namespace/name/version
        // Check if this combination is in local supported_templates
        let task = Task::find_by_uuid(&self.shared_registry.db_pool, step_message.task_uuid).await?;
        
        let template_key = format!("{}/{}/{}", task.namespace, task.name, task.version);
        let supported = self.supported_templates
            .read()
            .unwrap()
            .contains_key(&template_key);
        
        Ok(supported)
    }
    
    /// Register task template from local config path
    pub async fn register_from_config(&self, config_path: &str) -> Result<(), RegistryError> {
        // Load TaskTemplate from config file
        // Persist to database via shared_registry
        // Add to local supported_templates tracking
        todo!("Implement config-based registration")
    }
}
```

#### 1.7.2 Namespace Queue Management
```rust
impl TaskTemplateRegistry {
    /// Initialize namespace queues in pgmq
    async fn initialize_namespace_queues(
        &self,
        pgmq_client: &PgmqClient,
        discovered_namespaces: &[String],
    ) -> Result<(), RegistryError> {
        for namespace in discovered_namespaces {
            let queue_name = format!("{}_queue", namespace);
            
            // Create queue if it doesn't exist
            pgmq_client.create_queue(&queue_name).await?;
            
            // Register mapping
            self.namespace_queues
                .write()
                .unwrap()
                .insert(namespace.clone(), queue_name);
        }
        Ok(())
    }
}
```

#### 1.7.3 Integration with Step Processing (Event-Driven)
```rust
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::models::core::{workflow_step::WorkflowStep, task::Task};
use tasker_shared::events::StepEvent;

/// Worker service combining template registry with event-driven step processing
pub struct WorkerTaskService {
    template_registry: Arc<WorkerTaskTemplateRegistry>,
    api_client: Arc<OrchestrationApiClient>,
}

impl WorkerTaskService {
    /// Evaluate step capability and claim for processing (non-blocking)
    /// This method DOES NOT execute the step - it claims and publishes events
    pub async fn claim_and_publish_step(
        &self,
        step_message: SimpleStepMessage,
    ) -> Result<StepClaimResult, ServiceError> {
        // 1. Check if worker can handle this step using task.rs:793 "for orchestration" pattern
        if !self.template_registry.can_handle_step(&step_message).await? {
            return Ok(StepClaimResult::UnsupportedStep);
        }
        
        // 2. Load WorkflowStep from database
        let workflow_step = WorkflowStep::find_by_uuid(
            &self.database_pool, 
            step_message.step_uuid
        ).await?;
        
        // 3. Create step state machine for claiming
        let mut step_sm = StepStateMachine::new(
            workflow_step,
            self.database_pool.clone(),
            self.event_publisher.clone(),
        );
        
        // 4. Transition to InProgress (claims the step atomically)
        match step_sm.transition(StepEvent::Start).await {
            Ok(_) => {
                // Step claimed successfully - now publish execution event
                self.publish_step_execution_event(step_message).await?;
                Ok(StepClaimResult::ClaimedAndPublished)
            }
            Err(StateMachineError::InvalidTransition { .. }) => {
                // Step already claimed or not ready
                Ok(StepClaimResult::AlreadyClaimed)
            }
            Err(e) => Err(ServiceError::StepClaimError(e.to_string()))
        }
    }
    
    /// Publish step execution event to in-process event system (non-blocking)
    /// Step handlers will receive this event and execute asynchronously
    async fn publish_step_execution_event(
        &self,
        step_message: SimpleStepMessage,
    ) -> Result<(), ServiceError> {
        // Use in-process event publishing as described in TAS-40-command-pattern.md
        // This publishes to the event system where FFI handlers listen
        // Handlers execute asynchronously and publish completion events back
        
        let event = StepExecutionEvent {
            step_uuid: step_message.step_uuid,
            task_uuid: step_message.task_uuid,
            step_payload: self.build_step_payload(step_message).await?,
            execution_context: self.build_execution_context(step_message).await?,
        };
        
        self.event_publisher.publish_step_execution(event).await?;
        
        Ok(())
    }
    
    /// Handle step completion event from FFI handlers (event-driven)
    pub async fn handle_step_completion(
        &self,
        completion_event: StepCompletionEvent,
    ) -> Result<(), ServiceError> {
        // Load WorkflowStep and transition to Complete via state machine
        let workflow_step = WorkflowStep::find_by_uuid(
            &self.database_pool, 
            completion_event.step_uuid
        ).await?;
        
        let mut step_sm = StepStateMachine::new(
            workflow_step,
            self.database_pool.clone(),
            self.event_publisher.clone(),
        );
        
        // Write step results and transition to Complete
        let event = if completion_event.success {
            StepEvent::Complete(completion_event.result)
        } else {
            StepEvent::Fail(completion_event.error)
        };
        
        step_sm.transition(event).await?;
        
        Ok(())
    }
}

    /// Initialize task via orchestration API (separate from step processing)
    pub async fn initialize_task(
        &self,
        namespace: &str,
        task_name: &str,
        context: serde_json::Value,
    ) -> Result<TaskCreationResponse, ServiceError> {
        // Get task template via shared registry
        let template = self.template_registry.shared_registry
            .get_task_template(namespace, task_name, "1.0.0").await?;
        
        // Create TaskRequest using shared types
        let task_request = TaskRequest {
            namespace: namespace.to_string(),
            name: task_name.to_string(),
            version: template.version,
            context,
            // ... other fields
        };
        
        // Send to orchestration via API
        self.api_client.initialize_task(task_request).await
    }
}

/// Result of attempting to claim a step for processing
#[derive(Debug, Clone)]
pub enum StepClaimResult {
    /// Step was successfully claimed and execution event published
    ClaimedAndPublished,
    /// Worker cannot handle this step (wrong namespace/task type)
    UnsupportedStep,
    /// Step already claimed by another worker or not ready
    AlreadyClaimed,
}

/// Step execution event for in-process event system
#[derive(Debug, Clone)]
pub struct StepExecutionEvent {
    pub step_uuid: uuid::Uuid,
    pub task_uuid: uuid::Uuid,
    pub step_payload: serde_json::Value,
    pub execution_context: serde_json::Value,
}

/// Step completion event from FFI handlers
#[derive(Debug, Clone)]
pub struct StepCompletionEvent {
    pub step_uuid: uuid::Uuid,
    pub success: bool,
    pub result: serde_json::Value,
    pub error: Option<String>,
}
```

### 1.7.4 Alignment with Existing WorkerProcessor Implementation

The current `tasker-worker/src/command_processor.rs` already implements the event-driven pattern correctly:

**Key Implementation Details from Existing Code**:
- **Step Claiming**: Uses `StepStateMachine::transition(StepEvent::Start)` for atomic step claiming
- **Event Publishing**: Uses `WorkerEventPublisher::fire_step_execution_event()` for non-blocking execution
- **Database Hydration**: Loads full execution context from database using `Task`, `WorkflowStep`, and `TaskTemplate`
- **Event Integration**: Optional event publisher/subscriber for FFI handler communication
- **Completion Handling**: Separate event loop for processing `StepExecutionResult` from FFI handlers

**Architectural Correctness**:
```rust
// CORRECT: Non-blocking execution from existing WorkerProcessor
if let Some(ref event_publisher) = self.event_publisher {
    event_publisher.fire_step_execution_event(
        task.task_uuid,
        message.step_uuid,
        step_name.clone(),
        handler_class.clone(),
        step_payload,
        dependency_results.clone(),
        task.context.clone(),
    ).await?;
    // Returns immediately - no blocking on step execution
}

// Completion events handled separately in event loop:
tokio::select! {
    completion = completion_receiver.recv() => {
        // Process completion asynchronously
        self.handle_step_completion(completion).await;
    }
}
```

**Task Template Management Integration**:
The `WorkerTaskTemplateRegistry` should wrap this existing functionality rather than replace it:
- Use `can_handle_step()` before calling `WorkerProcessor::ExecuteStep` command
- Let existing `handle_execute_step()` method handle database hydration and event publishing
- Completion events flow back through existing event system architecture

### 1.8 Testing Strategy for Web API (Updated)

Create `tasker-worker/tests/web/` with expanded coverage:
- Health endpoint tests
- Metrics endpoint tests  
- Authentication tests (when enabled)
- Load testing for high-throughput scenarios
- **NEW**: OrchestrationApiClient integration tests
- **NEW**: Task template registry tests with shared components
- **NEW**: Bootstrap system tests
- **NEW**: Step claiming and event publishing tests (non-blocking pattern)
- **NEW**: Event-driven step completion handling tests  
- **NEW**: Integration with existing WorkerProcessor command pattern
- **NEW**: Task template registry integration with step claiming
- **NEW**: State machine integration tests (claiming, result writing via events)
- **NEW**: Database interaction tests (SQL functions, step results via state transitions)
- **NEW**: End-to-end task initialization tests
- **NEW**: Validation of non-blocking step execution pattern
- **NEW**: FFI event system integration tests with existing WorkerProcessor

---

## Phase 2: Configuration Audit and Cleanup

**Duration**: 1.5 weeks (Updated from 1 week)  
**Current State**: Component-based TOML with environment overrides  
**Target State**: Clean configuration aligned with command pattern architecture **PLUS FFI shared components cleanup**

### 2.1 Configuration Files Analysis

#### 2.1.1 Files to Remove/Deprecate

**`config/tasker/base/executor_pools.toml`**
- **Status**: OBSOLETE - Replace with command pattern config
- **Reason**: Polling-based executor pools (task_request_processor, task_claimer, step_enqueuer, step_result_processor, task_finalizer) are replaced by command channels
- **Action**: Create `config/tasker/base/command_processing.toml` with command buffer sizes and timeouts

**Environment overrides referencing executor_pools**:
- `config/tasker/environments/*/executor_pools.toml` - Remove all

#### 2.1.2 Files to Update

**`config/tasker/base/worker.toml`**
- **Current**: Has `step_executor_pool` and `result_processor_pool` sections with polling config
- **Update**: Replace with command pattern configuration:
  ```toml
  [command_processing]
  command_buffer_size = 1000
  command_timeout_ms = 30000
  max_concurrent_commands = 100
  
  [event_integration]
  publisher_buffer_size = 1000
  subscriber_buffer_size = 1000
  correlation_timeout_seconds = 300
  ```

**`config/tasker/base/orchestration.toml`**
- **Current**: May reference executor pools and polling intervals
- **Update**: Focus on command pattern orchestration settings

#### 2.1.3 TaskerConfig Struct Cleanup

**In `tasker-shared/src/config/mod.rs`**:

Remove or mark deprecated:
```rust
// REMOVE: Executor pools are obsolete with command pattern
#[serde(skip_serializing_if = "Option::is_none")]
pub executor_pools: Option<ExecutorPoolsConfig>,

// ADD: Command pattern configuration
pub command_processing: CommandProcessingConfig,
```

**Configuration sections to evaluate**:
- `dependency_graph` - May be obsolete if handled differently in command pattern
- `reenqueue` - Update to work with command pattern
- `events` - Update for new event system architecture
- `backoff` - Ensure compatibility with command pattern retries

### 2.2 New Configuration Components

#### 2.2.1 Command Processing Configuration
Create `tasker-shared/src/config/command_processing.rs`:
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandProcessingConfig {
    pub orchestration_buffer_size: usize,
    pub worker_buffer_size: usize,
    pub command_timeout_ms: u64,
    pub max_concurrent_commands: usize,
    pub retry_config: CommandRetryConfig,
}
```

#### 2.2.2 Event System Configuration
Update `tasker-shared/src/config/events.rs` for new event architecture:
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventsConfig {
    pub enabled: bool,
    pub publisher_buffer_size: usize,
    pub subscriber_buffer_size: usize,
    pub correlation_timeout_seconds: u64,
    pub cross_language_events_enabled: bool,
}
```

### 2.3 Configuration Migration Strategy

1. **Phase 2.1**: Create new command pattern configuration files
2. **Phase 2.2**: Update TaskerConfig struct to use new configuration
3. **Phase 2.3**: Update all code references to use new configuration
4. **Phase 2.4**: Remove obsolete configuration files and sections
5. **Phase 2.5**: Update documentation and examples

### 2.4 FFI Shared Components Evaluation

**Purpose**: Analysis and cleanup of `tasker-orchestration/src/ffi/shared` components  
**Target**: Remove obsolete FFI components, extract useful patterns for pure Rust worker

#### 2.4.1 Components Analysis

**KEEP / EXTRACT PATTERNS FOR WORKER TESTING:**

**`database_cleanup.rs`**
- **Status**: EXTRACT PATTERNS - Useful for worker test infrastructure
- **Value**: Database setup, schema management, migration patterns
- **Action**: Create pure Rust equivalent in `tasker-worker/src/testing/database_utils.rs`
- **Pattern**: Sequential database setup, connection management, test isolation

**`test_database_management.rs`**
- **Status**: EXTRACT PATTERNS - Very useful for worker testing
- **Value**: Comprehensive test database lifecycle management
- **Action**: Create worker-specific version without FFI dependencies
- **Pattern**: Test environment setup, cleanup, and validation

**`testing.rs` (SharedTestingFactory)**
- **Status**: EXTRACT PATTERNS - Useful testing factory patterns
- **Value**: Test data creation patterns, factory design
- **Action**: Create pure Rust factory for worker testing (no OrchestrationCore dependency)
- **Pattern**: Type-safe test data creation, shared factory patterns

**`task_initialization.rs`**
- **Status**: REFERENCE ONLY - Shows TaskRequest handling patterns
- **Value**: TaskRequest creation and validation patterns
- **Action**: Reference for OrchestrationApiClient implementation
- **Pattern**: TaskRequest struct usage, initialization flow

**REMOVE COMPLETELY (FFI-Specific, Not Needed for Pure Rust Worker):**

**`config.rs`**
- **Status**: OBSOLETE - FFI configuration bridging
- **Reason**: Pure Rust worker uses TaskerConfig directly, no FFI bridge needed
- **Action**: Delete entire file

**`errors.rs`**
- **Status**: OBSOLETE - FFI error types
- **Reason**: Pure Rust uses standard Rust error handling (thiserror, anyhow)
- **Action**: Delete entire file

**`event_bridge.rs`**
- **Status**: OBSOLETE - Cross-language event bridging
- **Reason**: Pure Rust worker doesn't need cross-language event bridging
- **Action**: Delete entire file

**`handles.rs`**
- **Status**: OBSOLETE - FFI handle management
- **Reason**: Pure Rust doesn't need FFI handle management patterns
- **Action**: Delete entire file

**`types.rs`**
- **Status**: OBSOLETE - FFI-specific types
- **Reason**: Pure Rust uses native types, no FFI type conversion needed
- **Action**: Delete entire file

**`mod.rs`**
- **Status**: UPDATE - Remove obsolete exports
- **Action**: Update to only export analytics (if still used) or delete if empty

#### 2.4.2 Pure Rust Worker Testing Equivalents

Create worker-specific testing infrastructure inspired by FFI patterns:

**`tasker-worker/src/testing/database_utils.rs`**
```rust
/// Pure Rust database utilities for worker testing
/// Inspired by ffi/shared/database_cleanup.rs but without FFI dependencies
pub struct WorkerDatabaseUtils {
    database_pool: Arc<PgPool>,
}

impl WorkerDatabaseUtils {
    pub async fn setup_test_database(database_url: &str) -> Result<Self, TestError> {
        // Sequential database setup following ffi/shared patterns
        // But using pure Rust error handling and type safety
    }
    
    pub async fn cleanup_test_environment(&self) -> Result<(), TestError> {
        // Test cleanup without FFI dependencies
    }
}
```

**`tasker-worker/src/testing/factory.rs`**
```rust
/// Pure Rust testing factory for worker components
/// Inspired by ffi/shared/testing.rs but without OrchestrationCore dependency
pub struct WorkerTestingFactory {
    config: TaskerConfig,
    database_pool: Arc<PgPool>,
    api_client: Arc<OrchestrationApiClient>,
}

impl WorkerTestingFactory {
    pub async fn create_test_task_request(&self, namespace: &str) -> TaskRequest {
        // Create test TaskRequest without FFI types
    }
    
    pub async fn create_test_namespace(&self, name: &str) -> Result<(), TestError> {
        // Create test namespace and queue
    }
}
```

#### 2.4.3 FFI Cleanup Implementation Plan

1. **Phase 2.4.1**: Create pure Rust worker testing utilities (2 days)
2. **Phase 2.4.2**: Update worker tests to use new testing infrastructure (1 day)  
3. **Phase 2.4.3**: Delete obsolete FFI shared files (1 day)
4. **Phase 2.4.4**: Update imports and module references (1 day)

---

## Phase 3: Test Suite Modernization

**Duration**: 1-2 weeks  
**Current State**: Mixed legacy and modern tests across three crates  
**Target State**: Comprehensive test coverage aligned with command pattern architecture

### 3.1 Test Directory Analysis and Recommendations

#### 3.1.1 `tasker-shared/tests/` - MOSTLY KEEP

**Keep (Still Relevant)**:
- `circuit_breaker/` - Circuit breakers still used in command pattern
- `config.rs` - Configuration loading tests remain relevant
- `database/` - Database operations and SQL functions unchanged
- `models/core/` - Core models (Task, WorkflowStep, etc.) unchanged
- `models/insights/` - Analytics and metrics still needed
- `sql_functions/` - SQL functions still used for database operations
- `state_machine/` - State machines still core to system

**Update/Modernize**:
- `factory_tests.rs` - Update to work with command pattern if needed
- `integration_tests.rs` - Update to use new configuration system
- `test_toml_config.rs` - Update for new configuration structure

**Consider Removing**:
- `tas_37_finalization_race_condition_test_clean_factories` - This appears to be a temporary test file
- Any tests specifically testing polling-based executors

#### 3.1.2 `tasker-orchestration/tests/` - SIGNIFICANT UPDATES NEEDED

**Keep and Update**:
- `config_integration_test.rs` - Update for new configuration system
- `unified_bootstrap_test.rs` - Update for command pattern bootstrap
- `web/` - Keep all web tests, they're comprehensive and valuable
- `messaging/task_request_processor_test.rs` - Update for command pattern

**Remove (Obsolete)**:
- `complex_workflow_integration_test.rs` - Likely tests old polling architecture
- `integration_executor_toml_config.rs` - Tests obsolete executor configuration
- `configuration_integration_test.rs` - May overlap with config_integration_test.rs
- `tas_37_finalization_race_condition_test.rs` - Race conditions solved, test may be obsolete

**Modernize**:
- `task_initializer_test.rs` - Update to work with command pattern
- `state_manager.rs` - Update for new operational state management

#### 3.1.3 `tasker-worker/tests/` - MAJOR EXPANSION NEEDED

**Current State**: Only `integration/worker_system_test.rs`

**Add New Test Suites**:
```
tasker-worker/tests/
├── command_processor/
│   ├── command_handling_tests.rs
│   └── command_integration_tests.rs
├── event_system/
│   ├── publisher_tests.rs
│   ├── subscriber_tests.rs
│   └── correlation_tests.rs
├── web/
│   ├── health_endpoint_tests.rs
│   ├── metrics_endpoint_tests.rs
│   └── worker_status_tests.rs
├── integration/
│   ├── worker_system_test.rs (existing - update)
│   ├── database_integration_tests.rs
│   └── full_workflow_tests.rs
└── performance/
    ├── command_throughput_tests.rs
    └── event_system_benchmarks.rs
```

### 3.2 Root-Level Integration Tests (Future)

**Location**: `tests/` (project root)  
**Purpose**: Full system integration tests combining orchestration and worker  
**Status**: Deferred until event-driven architecture migration and pg_notify implementation complete

**Planned Structure**:
```
tests/
├── full_system_integration/
│   ├── orchestration_worker_integration.rs
│   ├── multi_worker_coordination.rs
│   └── failure_recovery_scenarios.rs
├── performance/
│   ├── system_throughput_tests.rs
│   └── scalability_tests.rs
└── e2e_workflows/
    ├── complex_dag_workflows.rs
    └── multi_namespace_workflows.rs
```

### 3.3 Test Modernization Strategy

#### 3.3.1 Priority 1: Update Existing Tests
- Update configuration-related tests for new TOML structure
- Update integration tests to use command pattern APIs
- Fix any broken tests due to architectural changes

#### 3.3.2 Priority 2: Expand Worker Test Coverage
- Create comprehensive command processor tests
- Create event system integration tests
- Create web API tests
- Add performance and load tests

#### 3.3.3 Priority 3: Clean Up Obsolete Tests
- Remove tests for polling-based executors
- Remove tests for obsolete configuration
- Consolidate duplicate or overlapping tests

### 3.4 Comprehensive Testing Analysis and Implementation Checklist

Based on depth-and-breadth analysis of existing test suites, here's the detailed implementation plan:

#### 3.4.1 tasker-orchestration/tests Analysis

**✅ KEEP (High Value, Worker-Relevant)**
- [ ] `unified_bootstrap_test.rs` - Tests OrchestrationCore initialization with configuration
  - **Pattern Value**: Shows OrchestrationCore usage, circuit breaker testing, queue initialization
  - **Action**: Reference for worker bootstrap patterns
- [ ] `tas_37_finalization_race_condition_test.rs` - Race condition prevention testing
  - **Pattern Value**: Uses SqlxFactory for test data, shows concurrent claiming tests
  - **Action**: Adapt patterns for worker step claiming tests
- [ ] `web/test_infrastructure.rs` + `web/test_analytics_endpoints.rs` + `web/test_openapi_documentation.rs`
  - **Pattern Value**: Web test infrastructure, authentication testing, analytics endpoints
  - **Action**: Adapt for worker web API implementation
- [ ] `messaging/task_request_processor_test.rs`
  - **Pattern Value**: PGMQ integration testing, task request handling
  - **Action**: Update for command pattern messaging

**🗑️ REMOVE (Obsolete Orchestration-Specific)**
- [ ] Remove `complex_workflow_integration_test.rs` - Tests `WorkflowCoordinator`, `FrameworkIntegration`
  - **Reason**: Worker uses command pattern, not complex orchestration
- [ ] Remove `state_manager.rs` - Tests orchestration state management
  - **Reason**: Worker has simpler state management through command pattern
- [ ] Remove `task_initializer_test.rs` - Tests `TaskInitializer` component
  - **Reason**: Task initialization moves to orchestration layer
- [ ] Remove `configuration_integration_test.rs` + `config_integration_test.rs` + `integration_executor_toml_config.rs`
  - **Reason**: These test obsolete executor pool configurations we removed

**🔄 CHANGE (Modernize for Worker Patterns)**
- [ ] Update `messaging/mod.rs` - Focus on worker-specific messaging (step processing, result publishing)
- [ ] Modernize `web/` tests - Update `authenticated_tests.rs`, `unauthenticated_tests.rs` for worker endpoints
- [ ] Update `mod.rs` - Remove references to deleted tests, add new worker-specific modules

#### 3.4.2 FFI Shared Components Pattern Extraction

**🔧 Extract Database Testing Patterns** (from `database_cleanup.rs`)
- [ ] Create `tasker-worker/src/testing/database_utils.rs`
- [ ] Implement sequential database setup with safety checks:
  ```rust
  // Pattern: Sequential database setup with safety checks
  async fn setup_worker_test_database(pool: &PgPool) -> Result<(), TestError> {
      // 1. Terminate existing connections
      // 2. Drop and recreate schema  
      // 3. Run migrations
      // 4. Validate setup
  }
  ```

**🏭 Extract Factory Patterns** (from `testing.rs`)
- [ ] Create `tasker-worker/src/testing/factory.rs`
- [ ] Implement worker-specific test factories:
  ```rust
  pub struct WorkerTestFactory {
      orchestration_core: Arc<OrchestrationCore>,
  }
  
  impl WorkerTestFactory {
      pub async fn create_test_step_message(&self, input: TestStepInput) -> Result<StepMessage, TestError>
      pub async fn create_test_foundation(&self, namespace: &str) -> Result<TestFoundation, TestError>
  }
  ```

**🧪 Extract Test Environment Patterns** (from `test_database_management.rs`)
- [ ] Create `tasker-worker/src/testing/environment.rs`
- [ ] Implement environment safety validation:
  ```rust
  pub struct WorkerTestEnvironment {
      pub fn validate_test_safety(&self) -> Result<(), TestError> {
          // Check environment variables
          // Validate database URL contains 'test'
          // Ensure test-specific configuration
      }
      
      pub async fn cleanup_test_data(&self) -> Result<(), TestError> {
          // Clean tables in proper order
          // Reset sequences
          // Clear queues
      }
  }
  ```

#### 3.4.3 New Worker Testing Infrastructure

**🆕 Worker Command Pattern Testing**
- [ ] Create `tasker-worker/tests/command_processor/`
  - [ ] `command_handling_tests.rs` - Test event-driven step processing
  - [ ] `command_integration_tests.rs` - Test command pattern execution vs old executor pools
  - [ ] `worker_health_monitoring_tests.rs` - Test worker health monitoring and resource limits

**🆕 Worker Web API Testing** (TAS-40 Phase 1)
- [ ] Create `tasker-worker/tests/web/`
  - [ ] `health_endpoint_tests.rs` - Worker-specific health endpoints
  - [ ] `metrics_endpoint_tests.rs` - Worker metrics and analytics
  - [ ] `worker_status_tests.rs` - Worker status and configuration endpoints
  - [ ] `orchestration_api_client_tests.rs` - OrchestrationApiClient integration tests
  - [ ] `bootstrap_system_tests.rs` - Bootstrap system tests
  - [ ] `task_template_registry_tests.rs` - Task template registry with shared components

**🆕 Pure Rust Worker Testing Infrastructure**
- [ ] Create `tasker-worker/tests/infrastructure/`
  - [ ] `database_setup_tests.rs` - Database setup patterns (from `database_cleanup.rs`)
  - [ ] `test_environment_tests.rs` - Test environment safety (from `test_database_management.rs`)
  - [ ] `factory_pattern_tests.rs` - Factory patterns (from `testing.rs`)

**🆕 Configuration Testing**
- [ ] Create `tasker-worker/tests/config/`
  - [ ] `command_pattern_config_tests.rs` - Test command pattern worker.toml configuration
  - [ ] `event_system_config_tests.rs` - Test event system configuration
  - [ ] `resource_limits_config_tests.rs` - Test resource limits and health monitoring config

**🆕 Advanced Worker Testing**
- [ ] Create `tasker-worker/tests/integration/`
  - [ ] `step_claiming_tests.rs` - Step claiming and event publishing tests (non-blocking pattern)
  - [ ] `event_driven_completion_tests.rs` - Event-driven step completion handling tests  
  - [ ] `worker_processor_integration_tests.rs` - Integration with existing WorkerProcessor command pattern
  - [ ] `state_machine_integration_tests.rs` - State machine integration tests (claiming, result writing via events)
  - [ ] `database_interaction_tests.rs` - Database interaction tests (SQL functions, step results via state transitions)
  - [ ] `task_initialization_tests.rs` - End-to-end task initialization tests
  - [ ] `non_blocking_execution_tests.rs` - Validation of non-blocking step execution pattern
  - [ ] `ffi_event_system_tests.rs` - FFI event system integration tests with existing WorkerProcessor

#### 3.4.4 Implementation Priorities

**Phase 3.1: Clean Up Obsolete Tests** (Days 1-2)
- [ ] Remove obsolete orchestration tests from tasker-orchestration/tests
- [ ] Update tasker-orchestration/tests/mod.rs module declarations  
- [ ] Remove obsolete configuration tests
- [ ] Update imports and references

**Phase 3.2: Extract FFI Testing Patterns** (Days 3-4)
- [ ] Create worker testing infrastructure inspired by FFI patterns
- [ ] Implement database utilities, factory patterns, and environment validation
- [ ] Test new infrastructure with basic worker scenarios

**Phase 3.3: Create Worker Testing Suite** (Days 5-7)
- [ ] Implement comprehensive worker command pattern tests
- [ ] Create worker web API test suite
- [ ] Add integration tests for new worker components
- [ ] Implement performance and load tests

**Phase 3.4: Validation and Documentation** (Days 8-9)
- [ ] Run full test suite and ensure coverage
- [ ] Document testing patterns and best practices
- [ ] Create testing guides for future worker development

#### 3.4.5 Success Metrics

**Test Coverage Targets:**
- [ ] Maintain >90% code coverage across all worker components
- [ ] 100% API endpoint coverage for worker web API
- [ ] Comprehensive integration test coverage for all worker scenarios

**Performance Targets:**
- [ ] Command processing tests validate <1ms command handling latency
- [ ] Event system tests validate >10k events/sec throughput
- [ ] Web API tests validate <100ms response times under load

**Quality Targets:**
- [ ] Zero flaky tests (consistent pass/fail behavior)
- [ ] All tests run in <5 minutes total execution time
- [ ] Test isolation verified (tests can run in any order)

---

## Implementation Timeline (Updated)

### Week 1-2: Worker Web API Implementation (1.5-2 weeks)
- **Week 1 Days 1-2**: Core web module structure and state management
- **Week 1 Days 3-4**: Health, metrics, and status endpoints
- **Week 1 Day 5**: Authentication middleware and route configuration
- **Week 2 Days 1-2**: OrchestrationApiClient implementation
- **Week 2 Days 3-4**: Worker Bootstrap System and Task Template Management
- **Week 2 Day 5**: Integration testing and refinement

### Week 3: Configuration Cleanup (1.5 weeks)
- **Days 1-2**: Analysis and planning of configuration changes
- **Days 3-4**: Implementation of new configuration structures
- **Days 5-6**: FFI shared components evaluation and cleanup
- **Day 7**: Migration of existing code to use new configuration

### Week 4-5: Test Suite Modernization (1-2 weeks)
- **Week 4 Days 1-2**: Update existing tests for command pattern
- **Week 4 Days 3-4**: Create new worker test suites with FFI-inspired patterns
- **Week 4 Day 5**: OrchestrationApiClient and bootstrap integration tests
- **Week 5 (if needed)**: Clean up obsolete tests and documentation

## Dependencies and Risks

### Dependencies
- TAS-40 Command Pattern implementation (Complete)
- Current component-based configuration system (Complete)
- Event system implementation (Complete)
- **NEW**: Orchestration Web API contract understanding (POST /v1/tasks endpoint)
- **NEW**: Ruby bootstrap patterns analysis (bindings/ruby/lib/tasker_core/boot.rb)
- **NEW**: TaskRequest/TaskCreationResponse type definitions
- **NEW**: Access to running orchestration instance for API client testing

### Risks and Mitigations

1. **Configuration Migration Risk**: Breaking existing deployments
   - **Mitigation**: Maintain backward compatibility during transition period
   - **Mitigation**: Comprehensive testing of all configuration scenarios

2. **Test Suite Disruption**: Removing too many tests and losing coverage
   - **Mitigation**: Careful analysis of each test before removal
   - **Mitigation**: Maintain test coverage metrics throughout process

3. **Web API Integration**: Compatibility with existing monitoring systems
   - **Mitigation**: Follow orchestration patterns proven in production
   - **Mitigation**: Maintain consistent endpoint structures and response formats

4. **NEW - OrchestrationApiClient Integration**: Network reliability and API compatibility
   - **Mitigation**: Implement comprehensive retry logic with circuit breaker patterns
   - **Mitigation**: Version API contracts and maintain backward compatibility
   - **Mitigation**: Extensive integration testing with real orchestration instances

5. **NEW - FFI Shared Cleanup**: Accidentally removing useful testing patterns
   - **Mitigation**: Careful analysis of each FFI component before removal
   - **Mitigation**: Extract and preserve useful patterns in pure Rust equivalents
   - **Mitigation**: Maintain test coverage during FFI component migration

## Success Criteria

### Phase 1: Worker Web API (Updated)
- [ ] Worker web API fully functional with health, metrics, and status endpoints
- [ ] Kubernetes-compatible readiness and liveness probes
- [ ] Prometheus metrics integration working
- [ ] Load testing shows acceptable performance under high throughput
- [ ] **NEW**: OrchestrationApiClient successfully integrates with POST /v1/tasks endpoint
- [ ] **NEW**: Worker Bootstrap System initializes without embedded context
- [ ] **NEW**: Task Template Management properly maps namespaces to queues
- [ ] **NEW**: End-to-end task initialization via API client working in integration tests

### Phase 2: Configuration Cleanup (Updated)
- [ ] All obsolete configuration removed (executor_pools.toml and related)
- [ ] New command pattern configuration in place and working
- [ ] All components successfully using new configuration structure
- [ ] Configuration documentation updated
- [ ] **NEW**: FFI shared components analysis completed with cleanup plan
- [ ] **NEW**: Obsolete FFI components removed (config.rs, errors.rs, event_bridge.rs, handles.rs, types.rs)
- [ ] **NEW**: Useful FFI testing patterns extracted to pure Rust worker equivalents
- [ ] **NEW**: Worker testing infrastructure functional with FFI-inspired patterns

### Phase 3: Test Modernization
- [ ] Test suite coverage maintained or improved
- [ ] All tests passing with command pattern architecture
- [ ] Obsolete tests removed, reducing maintenance burden
- [ ] New worker test suites providing comprehensive coverage

## Post-Implementation

After completion of this plan:
1. **Root-Level Integration Tests**: Can be implemented once event-driven architecture and pg_notify are complete
2. **Performance Optimization**: Use new test infrastructure to identify and optimize bottlenecks
3. **Production Deployment**: System ready for production deployment with full observability

---

**Document Version**: 2.0 - COMPREHENSIVE UPDATE  
**Last Updated**: 2025-08-25  
**Review Status**: Ready for Implementation with Critical Integration Components  

## Changelog

### Version 2.0 - Critical Integration Components Added
- **Added**: OrchestrationApiClient implementation (Section 1.5)
- **Added**: Worker Bootstrap System (Section 1.6)  
- **Added**: Task Template Management (Section 1.7)
- **Added**: FFI Shared Components Evaluation and Cleanup (Section 2.4)
- **Updated**: Timeline extended from 2-3 weeks to 3-4 weeks
- **Updated**: Dependencies to include orchestration API contract and Ruby bootstrap patterns
- **Updated**: Success criteria to include new integration components
- **Updated**: Risk assessment with new integration and cleanup risks
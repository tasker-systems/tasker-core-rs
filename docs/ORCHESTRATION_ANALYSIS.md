# Rails Tasker Orchestration Analysis & Rust Design Implications

## Executive Summary

The Rails Tasker orchestration system reveals a sophisticated **control flow delegation pattern** that fundamentally changes our Rust architecture. Rather than building a monolithic Rust orchestration engine, we need to build a **high-performance orchestration core** that seamlessly integrates with framework-managed queuing and step execution.

**Key Architectural Insights:**
- **Orchestration Core**: Rust implements WorkflowCoordinator, StepExecutor, and Base Step Handlers
- **Framework-Agnostic Design**: Clean interface that any framework (Rails, Loco, Django, Express) can consume
- **Event-Driven System**: 56+ built-in events with comprehensive observability
- **Dual Finalization Strategy**: Synchronous and asynchronous finalization modes prevent race conditions
- **Four-Phase Step Handler Pattern**: Structured approach to idempotent business logic execution
- **YAML-Driven Configuration**: Complete workflow definition with environment-specific overrides
- **Thread-Safe Registry Systems**: Enterprise-grade component management with validation
- **Production-Ready Error Handling**: Clear classification of permanent vs retryable failures

## Critical Discoveries

### 1. **Control Flow Pattern: Delegation-Based Architecture**

**Current Rails Flow:**
```
Queue → TaskHandler → WorkflowCoordinator → StepExecutor → StepHandler → Business Logic
  ↓         ↓              ↓                ↓             ↓            ↓
Framework  Task          Orchestration    Orchestration Framework   User Code
```

**Corrected Architecture**: WorkflowCoordinator AND StepExecutor are both part of the **orchestration core**. Only the queue management and final business logic execution remain framework-specific.

**Implication for Rust**: We're building the complete **orchestration core** (WorkflowCoordinator + StepExecutor + Base Step Handlers) while providing framework integration points for queue management and business logic execution.

### 2. **Queue Handoff Pattern**

**Key Finding**: The system doesn't manage queues directly. Instead:
- Tasks are dequeued by framework background jobs (`TaskRunnerJob`)
- Control is handed to orchestration system
- Orchestration determines completion vs re-enqueue
- **Rust must signal back to framework** for re-enqueueing decisions

**Critical Interface**: 
```rust
enum TaskResult {
    Complete(TaskCompletionInfo),
    Error(TaskErrorInfo),
    ReenqueueImmediate,
    ReenqueueDelayed(Duration),
}
```

### 3. **Step Handler Architecture Clarification**

**Key Finding**: Step execution coordination happens within the **Rust orchestration core**:
- **StepExecutor** (part of orchestration core) manages step execution lifecycle
- **Base Step Handlers** (part of orchestration core) provide framework for business logic
- **Business Logic Step Handlers** (developer-space) implement `process()` and `process_results()` methods
- Results are managed within the orchestration core state management

**Architecture Layers**:
```rust
// Orchestration Core (Rust)
WorkflowCoordinator → StepExecutor → BaseStepHandler → BusinessLogicStepHandler
                                          ↓                      ↓
                                  handle() method        process() & process_results()
                                  (framework)              (developer business logic)
```

**Framework-Agnostic Interface** (conceptual - frameworks would implement their own integration):
```rust
// This is what any framework would need to provide to use Tasker
trait FrameworkIntegration {
    async fn queue_task(&self, task_id: i64) -> Result<(), QueueError>;
    async fn dequeue_task(&self) -> Result<Option<i64>, QueueError>;
    async fn get_task_context(&self, task_id: i64) -> Result<TaskContext, ContextError>;
}
```

### 4. **Step Handler Architecture Analysis**

**Critical Pattern: Four-Phase Step Handler Execution**

All step handlers in the Rails system follow a proven four-phase pattern that ensures idempotency and proper error handling:

1. **Phase 1: Extract and Validate Inputs**
   - Extract required data from task context and previous step results
   - Validate inputs early with `PermanentError` for missing/invalid data
   - Normalize data formats and fail fast with clear error messages

2. **Phase 2: Execute Business Logic**
   - Perform the core operation (computation, API call, data transformation)
   - Handle service-specific errors with proper classification
   - Return raw results for validation

3. **Phase 3: Validate Business Logic Results**
   - Ensure operation completed successfully
   - Check for business-level failures (e.g., payment declined, rate limiting)
   - Classify errors correctly (`PermanentError` vs `RetryableError`)

4. **Phase 4: Process Results (Optional)**
   - Override `process_results` when needed for custom formatting
   - Store step results safely without affecting business logic retry
   - Handle result processing errors as `PermanentError`

**Key Step Handler Base Classes:**

- `Tasker::StepHandler::Base` - For computational tasks and internal operations
- `Tasker::StepHandler::Api` - For HTTP API calls with built-in retry and circuit breaker support

**Critical Error Classification:**
- `PermanentError` - Authentication failures, business rule violations, invalid data
- `RetryableError` - Network timeouts, service unavailable, rate limiting

### 5. **Dual Finalization Strategy**

**Critical Pattern: Synchronous vs Asynchronous Finalization**

The Rails system uses two distinct finalization modes to prevent race conditions:

**Synchronous Finalization (`synchronous: true`)**
- Used during coordinated workflow execution
- Prevents execution conflicts with active WorkflowCoordinator
- Defers orchestration decisions to the coordinating workflow
- Enables rich event publishing with step context

**Asynchronous Finalization (`synchronous: false`)**
- Used for event-driven task cleanup and autonomous management
- Can reenqueue tasks for further processing
- Makes independent orchestration decisions
- Handles cleanup and continuation logic

**Implication for Rust**: Our `TaskFinalizer` must support both modes and coordinate properly with the delegation pattern.

### 6. **YAML Configuration System**

**Critical Pattern: Configuration-Driven Workflow Definition**

The Rails system uses YAML files to define complete workflows:

```yaml
name: api_task/integration_yaml_example
module_namespace: ApiTask
task_handler_class: IntegrationYamlExample
default_dependent_system: ecommerce_system

named_steps:
  - fetch_cart
  - fetch_products
  - validate_products
  - create_order
  - publish_event

step_templates:
  - name: fetch_cart
    description: Fetch cart details
    handler_class: ApiTask::StepHandler::CartFetchStepHandler
    handler_config:
      type: api
      url: https://api.ecommerce.com/cart
      
  - name: validate_products
    depends_on_steps:
      - fetch_products
      - fetch_cart
    handler_class: ApiTask::StepHandler::ProductsValidateStepHandler

environments:
  development:
    step_templates:
      - name: fetch_cart
        handler_config:
          url: http://localhost:3000/api/cart
          params:
            debug: true
```

**Key Features:**
- **Step Dependencies**: `depends_on_step` and `depends_on_steps` for DAG construction
- **Handler Configuration**: Environment-specific API endpoints and parameters
- **Schema Validation**: JSON schema validation for task context
- **Environment Overrides**: Development, test, and production configurations

**Implication for Rust**: Our system must parse and interpret this YAML structure to build dependency graphs and configure step execution.

### 7. **Event System Integration**

**Critical Pattern: Comprehensive Event-Driven Architecture**

The Rails system publishes 56+ built-in events across four categories:

- **Task Events**: `task.started`, `task.completed`, `task.failed`
- **Step Events**: `step.started`, `step.completed`, `step.failed`
- **Workflow Events**: `workflow.step_dependencies_resolved`, `workflow.viable_steps_discovered`
- **Observability Events**: Performance metrics, health status, system monitoring

**Event Publishing Pattern:**
```ruby
# Automatic event publishing in step handlers
class MyStepHandler < Tasker::StepHandler::Base
  include Tasker::Concerns::EventPublisher
  
  def process(task, sequence, step)
    # Framework automatically publishes step.started
    result = perform_operation(task.context)
    # Framework automatically publishes step.completed
    result
  end
end
```

**Implication for Rust**: Our orchestration system must publish events across the FFI boundary to maintain observability and enable custom integrations.

### 8. **Registry System Patterns**

**Critical Pattern: Thread-Safe Component Management**

The Rails system uses three main registry systems:

1. **HandlerFactory Registry**: Task handler registration with namespace and version support
2. **PluginRegistry**: Format-based plugin discovery for telemetry
3. **SubscriberRegistry**: Event subscriber management

**Key Features:**
- **Thread-Safe Operations**: All use `Concurrent::Hash` storage
- **Interface Validation**: Fail-fast validation with detailed error messages
- **Structured Logging**: Every operation logged with correlation IDs
- **Conflict Resolution**: Graceful handling of registration conflicts

**Implication for Rust**: Our system must maintain compatible registry interfaces while providing high-performance lookup operations.

## Revised Rust Architecture

### Core Principle: **Orchestration Engine, Not Execution Engine**

Our Rust system should be the **high-performance decision-making core** that:
1. **Receives** tasks from framework queue handlers
2. **Analyzes** step readiness using optimized dependency resolution
3. **Delegates** step execution back to framework
4. **Manages** state transitions and completion logic
5. **Signals** re-enqueue decisions back to framework

### Key Components Redefined

#### **1. Orchestration Coordinator**
```rust
pub struct OrchestrationCoordinator {
    step_discovery: ViableStepDiscovery,
    state_manager: StateManager,
    task_finalizer: TaskFinalizer,
    event_publisher: EventPublisher,
    config_manager: ConfigurationManager,
}

impl OrchestrationCoordinator {
    pub async fn orchestrate_task(
        &self, 
        task_id: i64, 
        delegate: &dyn StepExecutionDelegate
    ) -> TaskResult {
        // Publish orchestration started event
        self.event_publisher.publish_orchestration_started(task_id).await?;
        
        loop {
            let viable_steps = self.step_discovery.find_viable_steps(task_id).await?;
            if viable_steps.is_empty() {
                return self.task_finalizer.finalize_task(task_id, synchronous: true).await;
            }
            
            // Publish viable steps discovered event
            self.event_publisher.publish_viable_steps_discovered(task_id, &viable_steps).await?;
            
            // Hand control back to framework for step execution
            let results = delegate.execute_steps(task_id, &viable_steps).await?;
            
            // Process results and update state
            self.state_manager.process_step_results(results).await?;
            
            if self.should_yield_control(task_id).await? {
                return TaskResult::ReenqueueImmediate;
            }
        }
    }
}
```

#### **2. Step Executor (Orchestration Core)**
```rust
pub struct StepExecutor {
    handler_registry: HandlerRegistry,
    event_publisher: EventPublisher,
    retry_manager: RetryStrategyManager,
    sql_executor: SqlFunctionExecutor,
}

impl StepExecutor {
    /// Execute a batch of viable steps within orchestration core
    pub async fn execute_steps(
        &self,
        task_id: i64,
        steps: &[ViableStep],
        task_context: &TaskContext
    ) -> Result<Vec<StepResult>, StepExecutionError> {
        let mut results = Vec::new();
        
        for step in steps {
            // Get step handler from registry
            let handler = self.handler_registry.get_handler(&step.handler_class)?;
            
            // Execute step - circuit breaker logic is handled by SQL functions
            // that determined this step was ready_for_execution
            let result = handler.handle(task_context, step).await;
            
            results.push(result);
        }
        
        Ok(results)
    }
}

pub struct StepResult {
    pub step_id: i64,
    pub status: StepStatus,
    pub output: serde_json::Value,
    pub error_message: Option<String>,
    pub retry_after: Option<Duration>,
    pub error_code: Option<String>,
    pub error_context: Option<HashMap<String, serde_json::Value>>,
    pub execution_duration: Duration,
}

pub struct ViableStep {
    pub step_id: i64,
    pub name: String,
    pub handler_class: String,
    pub handler_config: Option<serde_json::Value>,
    pub dependencies: Vec<i64>,
    pub retry_count: u32,
    pub max_retries: u32,
}
```

#### **3. Task Finalization Engine**
```rust
pub struct TaskFinalizer {
    backoff_calculator: BackoffCalculator,
    completion_analyzer: CompletionAnalyzer,
    event_publisher: EventPublisher,
    reenqueuer: TaskReenqueuer,
}

impl TaskFinalizer {
    /// Finalize task with dual strategy support
    pub async fn finalize_task(&self, task_id: i64, synchronous: bool) -> TaskResult {
        let context = self.analyze_execution_context(task_id).await?;
        
        // Publish finalization started event
        self.event_publisher.publish_finalization_started(task_id, &context).await?;
        
        let result = match context.status {
            ExecutionStatus::AllComplete => {
                self.complete_task(task_id).await?;
                TaskResult::Complete(context.completion_info)
            },
            ExecutionStatus::BlockedByFailures => {
                self.error_task(task_id).await?;
                TaskResult::Error(context.error_info)
            },
            ExecutionStatus::HasReadySteps => {
                if synchronous {
                    // In synchronous mode, return immediately for coordinated execution
                    TaskResult::ReenqueueImmediate
                } else {
                    // In asynchronous mode, handle reenqueuing autonomously
                    self.reenqueuer.reenqueue_task_immediately(task_id).await?;
                    TaskResult::ReenqueueImmediate
                }
            },
            ExecutionStatus::WaitingForDependencies => {
                let delay = self.calculate_optimal_delay(task_id).await?;
                if synchronous {
                    TaskResult::ReenqueueDelayed(delay)
                } else {
                    self.reenqueuer.reenqueue_task_delayed(task_id, delay).await?;
                    TaskResult::ReenqueueDelayed(delay)
                }
            },
        };
        
        // Publish finalization completed event
        self.event_publisher.publish_finalization_completed(task_id, &context, &result).await?;
        
        result
    }
    
    /// Finalize task with step context (coordinated execution)
    pub async fn finalize_task_with_steps(
        &self, 
        task_id: i64, 
        processed_steps: &[ProcessedStep]
    ) -> TaskResult {
        // Always use synchronous mode for coordinated execution
        let context = self.analyze_execution_context_with_steps(task_id, processed_steps).await?;
        
        // Publish rich finalization event with step context
        self.event_publisher.publish_finalization_with_steps(task_id, &context, processed_steps).await?;
        
        self.finalize_task(task_id, true).await
    }
}
```

#### **4. Configuration Manager**
```rust
pub struct ConfigurationManager {
    yaml_parser: YamlParser,
    environment_resolver: EnvironmentResolver,
    schema_validator: SchemaValidator,
    config_cache: Arc<RwLock<HashMap<String, TaskHandlerConfig>>>,
}

impl ConfigurationManager {
    /// Load and parse YAML configuration for task handler
    pub async fn load_task_handler_config(
        &self,
        handler_name: &str
    ) -> Result<TaskHandlerConfig, ConfigurationError> {
        // Check cache first
        if let Some(config) = self.get_cached_config(handler_name).await {
            return Ok(config);
        }
        
        // Load and parse YAML
        let yaml_content = self.load_yaml_file(handler_name).await?;
        let mut config = self.yaml_parser.parse_task_handler(yaml_content)?;
        
        // Apply environment-specific overrides
        self.environment_resolver.apply_environment_overrides(&mut config).await?;
        
        // Validate schema
        self.schema_validator.validate_task_context_schema(&config.schema)?;
        
        // Cache the result
        self.cache_config(handler_name, &config).await;
        
        Ok(config)
    }
    
    /// Build dependency graph from step templates
    pub async fn build_dependency_graph(
        &self,
        config: &TaskHandlerConfig
    ) -> Result<DependencyGraph, ConfigurationError> {
        let mut graph = DependencyGraph::new();
        
        for step_template in &config.step_templates {
            graph.add_node(step_template.name.clone());
            
            // Add dependencies
            for dependency in &step_template.dependencies {
                graph.add_edge(dependency.clone(), step_template.name.clone())?;
            }
        }
        
        // Validate no cycles
        graph.validate_acyclic()?;
        
        Ok(graph)
    }
}
```

#### **5. Event Publisher**
```rust
pub struct EventPublisher {
    event_bus: Arc<EventBus>,
    payload_builder: EventPayloadBuilder,
    ffi_bridge: FfiBridge,
}

impl EventPublisher {
    /// Publish orchestration started event
    pub async fn publish_orchestration_started(&self, task_id: i64) -> Result<(), EventError> {
        let payload = self.payload_builder.build_orchestration_started_payload(task_id).await?;
        self.publish_event("orchestration.started", payload).await
    }
    
    /// Publish viable steps discovered event
    pub async fn publish_viable_steps_discovered(
        &self, 
        task_id: i64, 
        viable_steps: &[ViableStep]
    ) -> Result<(), EventError> {
        let payload = self.payload_builder.build_viable_steps_payload(task_id, viable_steps).await?;
        self.publish_event("workflow.viable_steps_discovered", payload).await
    }
    
    /// Publish finalization event with step context
    pub async fn publish_finalization_with_steps(
        &self,
        task_id: i64,
        context: &ExecutionContext,
        processed_steps: &[ProcessedStep]
    ) -> Result<(), EventError> {
        let payload = self.payload_builder.build_finalization_with_steps_payload(
            task_id, 
            context, 
            processed_steps
        ).await?;
        self.publish_event("task.finalization_completed", payload).await
    }
    
    /// Publish event across FFI boundary
    async fn publish_event(&self, event_name: &str, payload: serde_json::Value) -> Result<(), EventError> {
        // Publish to internal event bus
        self.event_bus.publish(event_name, &payload).await?;
        
        // Bridge to framework event system
        self.ffi_bridge.publish_to_framework(event_name, payload).await?;
        
        Ok(())
    }
}
```

#### **6. Viable Step Discovery**
```rust
pub struct ViableStepDiscovery {
    sql_executor: SqlFunctionExecutor,
    event_publisher: EventPublisher,
}

impl ViableStepDiscovery {
    /// Find steps ready for execution using SQL functions
    pub async fn find_viable_steps(&self, task_id: i64) -> Result<Vec<ViableStep>, DiscoveryError> {
        // Use existing SQL function for step readiness analysis
        let step_readiness = self.sql_executor
            .get_step_readiness_status(task_id, None)
            .await
            .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?;
        
        // Filter for steps ready for execution
        let ready_steps: Vec<_> = step_readiness
            .into_iter()
            .filter(|step| step.ready_for_execution)
            .collect();
        
        // Convert to ViableStep objects
        let mut viable_steps = Vec::new();
        for step in ready_steps {
            viable_steps.push(ViableStep {
                step_id: step.workflow_step_id,
                task_id: step.task_id,
                name: step.name,
                named_step_id: step.named_step_id,
                current_state: step.current_state,
                dependencies_satisfied: step.dependencies_satisfied,
                retry_eligible: step.retry_eligible,
                attempts: step.attempts,
                retry_limit: step.retry_limit,
                last_failure_at: step.last_failure_at,
                next_retry_at: step.next_retry_at,
            });
        }
        
        // Publish discovery event
        self.event_publisher.publish_viable_steps_discovered(task_id, &viable_steps).await?;
        
        Ok(viable_steps)
    }
    
    /// Get dependency levels using SQL function
    pub async fn get_dependency_levels(&self, task_id: i64) -> Result<HashMap<i64, i32>, DiscoveryError> {
        self.sql_executor
            .dependency_levels_hash(task_id)
            .await
            .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))
    }
    
    /// Get task execution context using SQL function
    pub async fn get_execution_context(&self, task_id: i64) -> Result<Option<TaskExecutionContext>, DiscoveryError> {
        self.sql_executor
            .get_task_execution_context(task_id)
            .await
            .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))
    }
}
```

#### **7. State Machine Integration**
```rust
pub struct StateManager {
    task_state_machine: TaskStateMachine,
    step_state_machine: StepStateMachine,
    sql_executor: SqlFunctionExecutor,
    event_publisher: EventPublisher,
}

impl StateManager {
    /// Process step results and update state
    pub async fn process_step_results(
        &self, 
        results: Vec<StepResult>
    ) -> Result<(), StateError> {
        for result in results {
            // Update step state based on result
            match result.status {
                StepStatus::Completed => {
                    self.step_state_machine.transition_to_completed(result.step_id).await?;
                },
                StepStatus::Failed => {
                    self.step_state_machine.transition_to_failed(result.step_id).await?;
                },
                StepStatus::Retrying => {
                    self.step_state_machine.transition_to_retrying(result.step_id).await?;
                },
            }
            
            // Publish state transition event
            self.event_publisher.publish_step_state_transition(
                result.step_id, 
                result.status
            ).await?;
        }
        
        Ok(())
    }
    
    /// Evaluate task state using SQL function
    pub async fn evaluate_task_state(&self, task_id: i64) -> Result<TaskStateTransition, StateError> {
        // Use SQL function to get comprehensive task execution context
        let execution_context = self.sql_executor
            .get_task_execution_context(task_id)
            .await
            .map_err(|e| StateError::DatabaseError(e.to_string()))?;
        
        let context = match execution_context {
            Some(ctx) => ctx,
            None => return Ok(TaskStateTransition::NoChange),
        };
        
        // Determine task state from execution context
        let new_state = if context.is_complete() {
            TaskState::Completed
        } else if context.error_steps > 0 && context.ready_steps == 0 {
            TaskState::Failed
        } else if context.ready_steps > 0 || context.pending_steps > 0 {
            TaskState::InProgress
        } else {
            TaskState::Pending
        };
        
        // Get current state and compare
        let current_state = self.task_state_machine.get_current_state(task_id).await?;
        
        if new_state != current_state {
            self.task_state_machine.transition_to(task_id, new_state).await?;
            return Ok(TaskStateTransition::Changed(new_state));
        }
        
        Ok(TaskStateTransition::NoChange)
    }
    
    /// Get system health using SQL function
    pub async fn get_system_health(&self) -> Result<SystemHealthCounts, StateError> {
        self.sql_executor
            .get_system_health_counts()
            .await
            .map_err(|e| StateError::DatabaseError(e.to_string()))
    }
    
    /// Get analytics metrics using SQL function
    pub async fn get_analytics_metrics(&self, since: Option<DateTime<Utc>>) -> Result<AnalyticsMetrics, StateError> {
        self.sql_executor
            .get_analytics_metrics(since)
            .await
            .map_err(|e| StateError::DatabaseError(e.to_string()))
    }
}
```

#### **8. Base Step Handler Framework (Orchestration Core)**
```rust
/// Base step handler that provides the framework for all step execution
#[async_trait]
pub trait BaseStepHandler: Send + Sync {
    /// Framework method that coordinates step execution (NEVER override in business logic)
    async fn handle(&self, task_context: &TaskContext, step: &ViableStep) -> StepResult {
        // Publish step started event
        self.publish_step_started(step).await;
        
        let start_time = Instant::now();
        
        match self.process(task_context, step).await {
            Ok(output) => {
                let duration = start_time.elapsed();
                
                // Process results using overridable method
                let processed_result = self.process_results(step, &output).await
                    .unwrap_or(output);
                
                // Publish step completed event
                self.publish_step_completed(step, duration).await;
                
                StepResult {
                    step_id: step.step_id,
                    status: StepStatus::Completed,
                    output: processed_result,
                    execution_duration: duration,
                    error_message: None,
                    retry_after: None,
                    error_code: None,
                    error_context: None,
                }
            },
            Err(error) => {
                let duration = start_time.elapsed();
                
                // Publish step failed event
                self.publish_step_failed(step, &error, duration).await;
                
                self.build_error_result(step, error, duration)
            }
        }
    }
    
    /// Developer extension point - implement business logic here
    async fn process(&self, task_context: &TaskContext, step: &ViableStep) -> Result<serde_json::Value, StepExecutionError>;
    
    /// Optional: customize result processing
    async fn process_results(&self, step: &ViableStep, output: &serde_json::Value) -> Result<serde_json::Value, StepExecutionError> {
        Ok(output.clone())
    }
}

/// Base implementation for computational tasks
pub struct BaseStepHandlerImpl {
    event_publisher: EventPublisher,
}

impl BaseStepHandlerImpl {
    pub fn new(event_publisher: EventPublisher) -> Self {
        Self { event_publisher }
    }
}

#[async_trait]
impl BaseStepHandler for BaseStepHandlerImpl {
    async fn process(&self, _task_context: &TaskContext, _step: &ViableStep) -> Result<serde_json::Value, StepExecutionError> {
        Err(StepExecutionError::Permanent {
            message: "Base step handler must be subclassed with process() implementation".to_string(),
            error_code: Some("NOT_IMPLEMENTED".to_string()),
        })
    }
}

/// API step handler for HTTP requests
pub struct ApiStepHandler {
    base: BaseStepHandlerImpl,
    http_client: Arc<reqwest::Client>,
    config: ApiConfig,
}

#[async_trait]
impl BaseStepHandler for ApiStepHandler {
    async fn process(&self, task_context: &TaskContext, step: &ViableStep) -> Result<serde_json::Value, StepExecutionError> {
        // Extract URL and configuration from step config
        let url = step.handler_config
            .as_ref()
            .and_then(|config| config.get("url"))
            .and_then(|url| url.as_str())
            .ok_or_else(|| StepExecutionError::Permanent {
                message: "URL not configured for API step".to_string(),
                error_code: Some("MISSING_URL".to_string()),
            })?;
        
        // Make HTTP request (this is framework-level logic)
        let response = self.http_client
            .post(url)
            .json(task_context)
            .send()
            .await
            .map_err(|e| StepExecutionError::Retryable {
                message: format!("HTTP request failed: {}", e),
                retry_after: None,
                skip_retry: false,
                context: None,
            })?;
        
        if response.status().is_success() {
            let json: serde_json::Value = response.json().await.map_err(|e| {
                StepExecutionError::Permanent {
                    message: format!("Failed to parse response JSON: {}", e),
                    error_code: Some("INVALID_JSON".to_string()),
                }
            })?;
            Ok(json)
        } else {
            Err(StepExecutionError::Retryable {
                message: format!("HTTP request failed with status: {}", response.status()),
                retry_after: None,
                skip_retry: false,
                context: Some(HashMap::from([
                    ("status_code".to_string(), serde_json::Value::Number(response.status().as_u16().into())),
                ])),
            })
        }
    }
}
```

## Rust Client Library for Integration Testing

### **Critical Need: Tasker Rust Client**

To properly test our orchestration system with complex workflows (Linear, Diamond, Tree, Fan-in/Fan-out), we need a **Rust client library** that implements business logic step handlers. This serves multiple purposes:

1. **Integration Testing**: Enable comprehensive workflow testing within our Rust test suite
2. **API Design Foundation**: Prototype the "public surface" of Tasker for framework consumers
3. **Pattern Validation**: Prove out the patterns that other language clients would follow

### **Rust Client Architecture**

#### **1. Business Logic Step Handlers**
```rust
/// Example business logic step handler for integration testing
pub struct PaymentProcessingHandler {
    base: BaseStepHandlerImpl,
}

impl PaymentProcessingHandler {
    pub fn new(event_publisher: EventPublisher) -> Self {
        Self {
            base: BaseStepHandlerImpl::new(event_publisher),
        }
    }
}

#[async_trait]
impl BaseStepHandler for PaymentProcessingHandler {
    async fn process(&self, task_context: &TaskContext, step: &ViableStep) -> Result<serde_json::Value, StepExecutionError> {
        // Phase 1: Extract and validate inputs
        let payment_amount = task_context.get("payment_amount")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StepExecutionError::Permanent {
                message: "payment_amount is required".to_string(),
                error_code: Some("MISSING_PAYMENT_AMOUNT".to_string()),
            })?;
        
        if payment_amount <= 0.0 {
            return Err(StepExecutionError::Permanent {
                message: "payment_amount must be positive".to_string(),
                error_code: Some("INVALID_PAYMENT_AMOUNT".to_string()),
            });
        }
        
        // Phase 2: Execute business logic (simulated)
        tokio::time::sleep(Duration::from_millis(100)).await; // Simulate processing time
        
        let success_rate = 0.9; // 90% success rate for testing
        let random_value: f64 = rand::random();
        
        if random_value < success_rate {
            // Phase 3: Validate successful result
            Ok(serde_json::json!({
                "payment_id": format!("pay_{}", uuid::Uuid::new_v4()),
                "amount": payment_amount,
                "status": "completed",
                "processed_at": chrono::Utc::now().to_rfc3339()
            }))
        } else {
            // Simulate temporary failure
            Err(StepExecutionError::Retryable {
                message: "Payment service temporarily unavailable".to_string(),
                retry_after: Some(5), // Retry after 5 seconds
                skip_retry: false,
                context: Some(HashMap::from([
                    ("service_error".to_string(), serde_json::Value::String("TEMPORARY_UNAVAILABLE".to_string())),
                ])),
            })
        }
    }
    
    async fn process_results(&self, step: &ViableStep, output: &serde_json::Value) -> Result<serde_json::Value, StepExecutionError> {
        // Phase 4: Process results - add metadata
        let mut result = output.clone();
        if let Some(obj) = result.as_object_mut() {
            obj.insert("step_name".to_string(), serde_json::Value::String(step.name.clone()));
            obj.insert("handler_class".to_string(), serde_json::Value::String(step.handler_class.clone()));
        }
        Ok(result)
    }
}

/// Example inventory check handler
pub struct InventoryCheckHandler {
    base: BaseStepHandlerImpl,
}

#[async_trait]
impl BaseStepHandler for InventoryCheckHandler {
    async fn process(&self, task_context: &TaskContext, _step: &ViableStep) -> Result<serde_json::Value, StepExecutionError> {
        let product_id = task_context.get("product_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StepExecutionError::Permanent {
                message: "product_id is required".to_string(),
                error_code: Some("MISSING_PRODUCT_ID".to_string()),
            })?;
        
        let quantity = task_context.get("quantity")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        
        // Simulate inventory check
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Simulate different inventory scenarios for testing
        let available_quantity = match product_id {
            "out_of_stock" => 0,
            "low_stock" => quantity.saturating_sub(1),
            _ => quantity + 10, // Sufficient stock
        };
        
        if available_quantity < quantity {
            Err(StepExecutionError::Permanent {
                message: format!("Insufficient inventory: need {}, have {}", quantity, available_quantity),
                error_code: Some("INSUFFICIENT_INVENTORY".to_string()),
            })
        } else {
            Ok(serde_json::json!({
                "product_id": product_id,
                "requested_quantity": quantity,
                "available_quantity": available_quantity,
                "status": "available"
            }))
        }
    }
}
```

#### **2. Task Handler Registry with Dual-Path Support**
```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Task handler registry with dual-path support for Rust and FFI integration
pub struct TaskHandlerRegistry {
    // Direct handler references for Rust consumers
    handlers: Arc<RwLock<HashMap<String, Arc<dyn TaskHandler>>>>,
    // Stringified references for FFI consumers
    ffi_handlers: Arc<RwLock<HashMap<String, HandlerMetadata>>>,
    event_publisher: EventPublisher,
}

#[derive(Debug, Clone)]
pub struct HandlerMetadata {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub handler_class: String,
    pub config_schema: Option<serde_json::Value>,
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

impl TaskHandlerRegistry {
    pub fn new(event_publisher: EventPublisher) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            ffi_handlers: Arc::new(RwLock::new(HashMap::new())),
            event_publisher,
        }
    }
    
    /// Register a task handler with namespace/name/version (Rust direct reference)
    pub fn register_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        handler: Arc<dyn TaskHandler>
    ) -> Result<(), RegistryError> {
        let key = format!("{}/{}/{}", namespace, name, version);
        
        // Register direct handler reference for Rust
        {
            let mut handlers = self.handlers.write().unwrap();
            handlers.insert(key.clone(), handler);
        }
        
        // Register metadata for introspection
        let metadata = HandlerMetadata {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            handler_class: format!("{}Handler", name),
            config_schema: None,
            registered_at: chrono::Utc::now(),
        };
        
        {
            let mut ffi_handlers = self.ffi_handlers.write().unwrap();
            ffi_handlers.insert(key.clone(), metadata);
        }
        
        // Publish registration event
        self.event_publisher.publish_handler_registered(&key).await?;
        
        Ok(())
    }
    
    /// Register a task handler for FFI (stringified reference)
    pub fn register_ffi_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        handler_class: &str,
        config_schema: Option<serde_json::Value>
    ) -> Result<(), RegistryError> {
        let key = format!("{}/{}/{}", namespace, name, version);
        
        let metadata = HandlerMetadata {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            handler_class: handler_class.to_string(),
            config_schema,
            registered_at: chrono::Utc::now(),
        };
        
        {
            let mut ffi_handlers = self.ffi_handlers.write().unwrap();
            ffi_handlers.insert(key.clone(), metadata);
        }
        
        // Publish registration event
        self.event_publisher.publish_handler_registered(&key).await?;
        
        Ok(())
    }
    
    /// Get task handler by namespace/name/version (Rust direct reference)
    pub fn get_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str
    ) -> Result<Arc<dyn TaskHandler>, RegistryError> {
        let key = format!("{}/{}/{}", namespace, name, version);
        let handlers = self.handlers.read().unwrap();
        
        handlers.get(&key)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(key))
    }
    
    /// Get task handler metadata (for both Rust and FFI)
    pub fn get_handler_metadata(
        &self,
        namespace: &str,
        name: &str,
        version: &str
    ) -> Result<HandlerMetadata, RegistryError> {
        let key = format!("{}/{}/{}", namespace, name, version);
        let ffi_handlers = self.ffi_handlers.read().unwrap();
        
        ffi_handlers.get(&key)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(key))
    }
    
    /// List all handlers in a namespace
    pub fn list_handlers(&self, namespace: Option<&str>) -> Result<Vec<HandlerMetadata>, RegistryError> {
        let ffi_handlers = self.ffi_handlers.read().unwrap();
        
        let filtered: Vec<_> = ffi_handlers.values()
            .filter(|metadata| {
                namespace.map_or(true, |ns| metadata.namespace == ns)
            })
            .cloned()
            .collect();
        
        Ok(filtered)
    }
    
    /// Get registry statistics
    pub fn stats(&self) -> RegistryStats {
        let handlers = self.handlers.read().unwrap();
        let ffi_handlers = self.ffi_handlers.read().unwrap();
        
        let namespaces: std::collections::HashSet<_> = ffi_handlers.values()
            .map(|m| m.namespace.clone())
            .collect();
        
        RegistryStats {
            total_handlers: handlers.len(),
            total_ffi_handlers: ffi_handlers.len(),
            namespaces: namespaces.into_iter().collect(),
            thread_safe: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_handlers: usize,
    pub total_ffi_handlers: usize,
    pub namespaces: Vec<String>,
    pub thread_safe: bool,
}

/// Example usage for integration testing
impl TaskHandlerRegistry {
    pub fn setup_test_handlers(&self) -> Result<(), RegistryError> {
        // Register test task handlers (not step handlers)
        self.register_handler(
            "ecommerce",
            "order_processor",
            "1.0.0",
            Arc::new(OrderProcessorHandler::new())
        )?;
        
        self.register_handler(
            "payments",
            "payment_processor",
            "2.1.0",
            Arc::new(PaymentProcessorHandler::new())
        )?;
        
        self.register_handler(
            "inventory",
            "inventory_manager",
            "1.5.0",
            Arc::new(InventoryManagerHandler::new())
        )?;
        
        Ok(())
    }
}
```

#### **3. Integration Test Workflows**
```rust
#[cfg(test)]
mod integration_workflow_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_linear_workflow() {
        // Test Linear workflow: A → B → C → D
        let coordinator = create_test_coordinator().await;
        let task_id = create_linear_workflow_task().await;
        
        let result = coordinator.orchestrate_task(task_id, &TestFrameworkIntegration::new()).await;
        
        assert!(matches!(result, TaskResult::Complete(_)));
        
        // Verify all steps completed in order
        let steps = get_completed_steps(task_id).await;
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].name, "inventory_check");
        assert_eq!(steps[1].name, "payment_processing");
        assert_eq!(steps[2].name, "order_confirmation");
        assert_eq!(steps[3].name, "email_notification");
    }
    
    #[tokio::test]
    async fn test_diamond_workflow() {
        // Test Diamond workflow: A → (B, C) → D
        let coordinator = create_test_coordinator().await;
        let task_id = create_diamond_workflow_task().await;
        
        let result = coordinator.orchestrate_task(task_id, &TestFrameworkIntegration::new()).await;
        
        assert!(matches!(result, TaskResult::Complete(_)));
        
        // Verify parallel execution and final convergence
        let steps = get_completed_steps(task_id).await;
        assert_eq!(steps.len(), 4);
        
        // First step completes
        assert_eq!(steps[0].name, "order_validation");
        
        // Parallel steps can complete in any order
        let parallel_steps: HashSet<_> = steps[1..3].iter().map(|s| s.name.as_str()).collect();
        assert!(parallel_steps.contains("inventory_check"));
        assert!(parallel_steps.contains("payment_processing"));
        
        // Final step completes last
        assert_eq!(steps[3].name, "order_fulfillment");
    }
    
    #[tokio::test]
    async fn test_tree_workflow() {
        // Test Tree workflow: A → (B → D, C → E)
        let coordinator = create_test_coordinator().await;
        let task_id = create_tree_workflow_task().await;
        
        let result = coordinator.orchestrate_task(task_id, &TestFrameworkIntegration::new()).await;
        
        assert!(matches!(result, TaskResult::Complete(_)));
        
        let steps = get_completed_steps(task_id).await;
        assert_eq!(steps.len(), 5);
    }
    
    #[tokio::test]
    async fn test_fan_out_fan_in_workflow() {
        // Test Fan-out/Fan-in: A → (B, C, D) → E
        let coordinator = create_test_coordinator().await;
        let task_id = create_fan_out_fan_in_workflow_task().await;
        
        let result = coordinator.orchestrate_task(task_id, &TestFrameworkIntegration::new()).await;
        
        assert!(matches!(result, TaskResult::Complete(_)));
        
        let steps = get_completed_steps(task_id).await;
        assert_eq!(steps.len(), 5);
        
        // Verify fan-out then fan-in pattern
        assert_eq!(steps[0].name, "order_received");
        assert_eq!(steps[4].name, "order_complete");
        
        // Middle three can complete in any order
        let middle_steps: HashSet<_> = steps[1..4].iter().map(|s| s.name.as_str()).collect();
        assert!(middle_steps.contains("inventory_reserve"));
        assert!(middle_steps.contains("payment_authorize"));
        assert!(middle_steps.contains("shipping_calculate"));
    }
    
    #[tokio::test]
    async fn test_error_handling_and_retry() {
        // Test retry logic with eventual success
        let coordinator = create_test_coordinator().await;
        let task_id = create_retry_test_workflow_task().await;
        
        let result = coordinator.orchestrate_task(task_id, &TestFrameworkIntegration::new()).await;
        
        assert!(matches!(result, TaskResult::Complete(_)));
        
        // Verify retry attempts were made
        let retry_events = get_retry_events(task_id).await;
        assert!(!retry_events.is_empty());
    }
    
    #[tokio::test]
    async fn test_permanent_error_handling() {
        // Test permanent error stops workflow
        let coordinator = create_test_coordinator().await;
        let task_id = create_permanent_error_workflow_task().await;
        
        let result = coordinator.orchestrate_task(task_id, &TestFrameworkIntegration::new()).await;
        
        assert!(matches!(result, TaskResult::Error(_)));
        
        // Verify workflow stopped at permanent error
        let failed_steps = get_failed_steps(task_id).await;
        assert!(!failed_steps.is_empty());
    }
}
```

### **Example: How Frameworks Would Use Tasker**

#### **Conceptual Example: Any Rust Web Framework**
```rust
// This is an EXAMPLE of how a framework like Loco, Axum, Tide, or Warp
// might integrate with Tasker in the future. This is NOT part of this crate.

// In some future "tasker-loco" or similar crate:
pub struct FrameworkTaskerIntegration {
    orchestration_core: OrchestrationCoordinator,
}

impl FrameworkTaskerIntegration {
    pub fn new() -> Self {
        let orchestration_core = OrchestrationCoordinator::new();
        Self { orchestration_core }
    }
    
    // Framework would handle its own queue management
    async fn run_task(&self, task_id: i64) -> Result<TaskResult, TaskError> {
        // Get task context from framework's database
        let context = self.get_task_context(task_id).await?;
        
        // Use Tasker's orchestration core
        self.orchestration_core.orchestrate_task(task_id, context).await
    }
}

// Framework developers would implement their own step handlers
struct PaymentStepHandler;

impl BaseStepHandler for PaymentStepHandler {
    async fn process(&self, context: &TaskContext, step: &ViableStep) -> Result<Value, Error> {
        // Business logic specific to the application
    }
}
```

## Production-Ready Patterns

### **Circuit Breaker Integration**

**No Custom Implementation Required**: Tasker's circuit breaker functionality is handled by the existing Rails framework through its **SQL-driven, distributed retry architecture**. The Rust orchestration core leverages this system through the existing SQL functions:

- **`get_step_readiness_status()`**: Determines if steps are ready for execution based on circuit breaker state
- **Backoff calculation**: Exponential backoff with jitter handled by SQL functions
- **Retry eligibility**: Distributed coordination through database state

**Key Benefits**:
- **Persistent across restarts**: Circuit state stored in database, not memory
- **Distributed coordination**: Multiple workers respect the same circuit breaker state
- **Framework-managed**: No need to reimplement circuit breaker logic in Rust
- **Observable through SQL**: Easy to query and monitor circuit breaker states

**Integration Pattern**:
```rust
// Rust orchestration core uses existing SQL functions for circuit breaker logic
let step_readiness = sql_executor.get_step_readiness_status(task_id, None).await?;
let ready_steps: Vec<_> = step_readiness
    .into_iter()
    .filter(|step| step.ready_for_execution)  // Circuit breaker logic is in SQL
    .collect();
```

### **Retry Strategy Implementation**
```rust
pub struct RetryStrategyManager {
    backoff_calculator: BackoffCalculator,
    retry_config: RetryConfig,
}

impl RetryStrategyManager {
    /// Determine if error should be retried
    pub fn should_retry(&self, error: &StepExecutionError, attempt: u32) -> bool {
        // Check attempt limits
        if attempt >= self.retry_config.max_attempts {
            return false;
        }
        
        // Check error type
        match error {
            StepExecutionError::Permanent(_) => false,
            StepExecutionError::Retryable(retryable_error) => {
                // Check if error specifically disables retries
                !retryable_error.skip_retry
            },
            StepExecutionError::Timeout => true,
            StepExecutionError::NetworkError => true,
        }
    }
    
    /// Calculate backoff delay for retry
    pub fn calculate_backoff_delay(&self, attempt: u32, error: &StepExecutionError) -> Duration {
        match error {
            StepExecutionError::Retryable(retryable_error) => {
                if let Some(retry_after) = retryable_error.retry_after {
                    return Duration::from_secs(retry_after);
                }
            },
            _ => {}
        }
        
        // Use exponential backoff with jitter
        self.backoff_calculator.calculate_delay(attempt)
    }
}
```

### **Observability Integration**
```rust
pub struct ObservabilityManager {
    metrics_collector: MetricsCollector,
    trace_publisher: TracePublisher,
    health_checker: HealthChecker,
}

impl ObservabilityManager {
    /// Record orchestration metrics
    pub async fn record_orchestration_metrics(
        &self,
        task_id: i64,
        step_count: u32,
        duration: Duration,
        result: &TaskResult
    ) -> Result<(), ObservabilityError> {
        self.metrics_collector.record_counter(
            "orchestration.tasks.completed",
            1,
            &[
                ("task_id", task_id.to_string()),
                ("result", result.to_string()),
            ]
        ).await?;
        
        self.metrics_collector.record_histogram(
            "orchestration.execution.duration",
            duration.as_millis() as f64,
            &[("step_count", step_count.to_string())]
        ).await?;
        
        Ok(())
    }
    
    /// Publish distributed trace
    pub async fn publish_trace(
        &self,
        task_id: i64,
        operation: &str,
        duration: Duration,
        success: bool
    ) -> Result<(), ObservabilityError> {
        let trace_context = TraceContext {
            task_id,
            operation: operation.to_string(),
            start_time: Instant::now() - duration,
            duration,
            success,
            attributes: HashMap::new(),
        };
        
        self.trace_publisher.publish_trace(trace_context).await?;
        Ok(())
    }
    
    /// Check system health
    pub async fn health_check(&self) -> HealthStatus {
        let database_health = self.health_checker.check_database().await;
        let event_system_health = self.health_checker.check_event_system().await;
        let state_machine_health = self.health_checker.check_state_machines().await;
        
        HealthStatus {
            overall: if database_health.is_healthy() && 
                       event_system_health.is_healthy() && 
                       state_machine_health.is_healthy() {
                Health::Healthy
            } else {
                Health::Unhealthy
            },
            database: database_health,
            event_system: event_system_health,
            state_machines: state_machine_health,
        }
    }
}
```

### **Resource Management**
```rust
pub struct ResourceManager {
    connection_pool: DatabaseConnectionPool,
    memory_monitor: MemoryMonitor,
    task_limits: TaskLimits,
}

impl ResourceManager {
    /// Acquire resources for task execution
    pub async fn acquire_resources(&self, task_id: i64) -> Result<ResourceHandle, ResourceError> {
        // Check memory usage
        if self.memory_monitor.current_usage() > self.task_limits.max_memory_mb {
            return Err(ResourceError::MemoryLimitExceeded);
        }
        
        // Check concurrent task limits
        if self.get_active_task_count().await > self.task_limits.max_concurrent_tasks {
            return Err(ResourceError::ConcurrencyLimitExceeded);
        }
        
        // Acquire database connection
        let db_connection = self.connection_pool.acquire().await?;
        
        Ok(ResourceHandle {
            task_id,
            db_connection,
            acquired_at: Instant::now(),
        })
    }
    
    /// Release resources after task completion
    pub async fn release_resources(&self, handle: ResourceHandle) -> Result<(), ResourceError> {
        // Release database connection
        self.connection_pool.release(handle.db_connection).await?;
        
        // Record resource usage metrics
        let usage_duration = handle.acquired_at.elapsed();
        self.record_resource_usage(handle.task_id, usage_duration).await?;
        
        Ok(())
    }
}
```

## Future Integration Patterns

### **FFI Integration (Future Work)**

The orchestration core is designed to support FFI integration in the future, allowing frameworks in any language to leverage Tasker's high-performance orchestration capabilities. The clean separation between orchestration logic and business logic execution makes this straightforward.

Key design decisions that enable future FFI work:
- **Serializable data structures**: All core types use standard serialization formats
- **Async-friendly architecture**: Compatible with various runtime models
- **Clear execution boundaries**: Orchestration vs business logic separation
- **Event-driven communication**: Language-agnostic event publishing

## Performance Optimization Points

### **1. Dependency Resolution (10-100x improvement target)**
- Replace PostgreSQL `calculate_dependency_levels()` function with Rust algorithms
- Implement in-memory DAG traversal with caching
- Use concurrent processing for large dependency graphs

### **2. State Management**
- Atomic state transitions with optimistic locking
- Batch state updates for multiple steps
- Memory-efficient state caching

### **3. Minimal FFI Overhead**
- Batch operations across FFI boundary
- Efficient serialization (consider MessagePack vs JSON)
- Connection pooling and resource reuse

## Migration Strategy

### **Phase 1: Proof of Concept**
1. Build basic orchestration coordinator
2. Implement Ruby FFI with simple delegation
3. Replace one component (viable step discovery) and measure performance

### **Phase 2: Gradual Replacement**
1. Replace state management with Rust implementation
2. Add comprehensive error handling and retry logic
3. Implement full task finalization logic

### **Phase 3: Production Integration**
1. Add comprehensive observability and metrics
2. Optimize FFI performance and resource usage
3. Add support for Python and other language bindings

## Detailed Implementation Requirements

### **Core Traits and Structures**

#### **Task Handler Configuration**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHandlerConfig {
    pub name: String,
    pub module_namespace: String,
    pub task_handler_class: String,
    pub namespace_name: String,
    pub version: String,
    pub default_dependent_system: Option<String>,
    pub named_steps: Vec<String>,
    pub schema: Option<serde_json::Value>,
    pub step_templates: Vec<StepTemplate>,
    pub environments: HashMap<String, EnvironmentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTemplate {
    pub name: String,
    pub description: String,
    pub handler_class: String,
    pub handler_config: Option<serde_json::Value>,
    pub depends_on_steps: Vec<String>,
    pub default_retry_limit: Option<u32>,
    pub default_retryable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub step_templates: Vec<StepTemplateOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTemplateOverride {
    pub name: String,
    pub handler_config: Option<serde_json::Value>,
}
```

#### **Error Types**
```rust
#[derive(Debug, thiserror::Error)]
pub enum OrchestrationError {
    #[error("Configuration error: {0}")]
    Configuration(#[from] ConfigurationError),
    
    #[error("State machine error: {0}")]
    StateMachine(#[from] StateError),
    
    #[error("Step execution error: {0}")]
    StepExecution(#[from] StepExecutionError),
    
    #[error("Event publishing error: {0}")]
    EventPublishing(#[from] EventError),
    
    #[error("FFI communication error: {0}")]
    FfiCommunication(#[from] FfiError),
    
    #[error("Resource management error: {0}")]
    ResourceManagement(#[from] ResourceError),
}

#[derive(Debug, thiserror::Error)]
pub enum StepExecutionError {
    #[error("Permanent error: {message}")]
    Permanent { message: String, error_code: Option<String> },
    
    #[error("Retryable error: {message}")]
    Retryable { 
        message: String, 
        retry_after: Option<u64>, 
        skip_retry: bool,
        context: Option<HashMap<String, serde_json::Value>>,
    },
    
    #[error("Timeout error")]
    Timeout,
    
    #[error("Network error: {0}")]
    NetworkError(String),
}
```

#### **State Definitions**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStateTransition {
    NoChange,
    Changed(TaskState),
}
```

### **Integration Testing Requirements**

#### **Step Handler Integration Tests**
```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_four_phase_step_handler_pattern() {
        // Test that step handlers follow four-phase pattern
        let handler = TestStepHandler::new();
        let task = create_test_task();
        let step = create_test_step();
        
        // Phase 1: Input validation
        assert!(handler.validate_inputs(&task, &step).await.is_ok());
        
        // Phase 2: Business logic execution
        let result = handler.execute_business_logic(&task, &step).await;
        assert!(result.is_ok());
        
        // Phase 3: Result validation
        assert!(handler.validate_results(&result.unwrap()).await.is_ok());
        
        // Phase 4: Result processing
        let processed = handler.process_results(&result.unwrap()).await;
        assert!(processed.is_ok());
    }
    
    #[tokio::test]
    async fn test_error_classification() {
        // Test proper error classification
        let handler = TestStepHandler::new();
        
        // Test permanent errors
        let permanent_error = handler.simulate_permanent_error().await;
        assert!(matches!(permanent_error, StepExecutionError::Permanent { .. }));
        
        // Test retryable errors
        let retryable_error = handler.simulate_retryable_error().await;
        assert!(matches!(retryable_error, StepExecutionError::Retryable { .. }));
    }
}
```

#### **FFI Integration Tests**
```rust
#[cfg(test)]
mod ffi_integration_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_ruby_delegation_roundtrip() {
        let coordinator = OrchestrationCoordinator::new();
        let ruby_delegate = MockRubyStepDelegate::new();
        
        let task_id = 1;
        let result = coordinator.orchestrate_task(task_id, &ruby_delegate).await;
        
        assert!(result.is_ok());
        // Verify delegation occurred
        assert!(ruby_delegate.execute_steps_called());
    }
    
    #[tokio::test]
    async fn test_event_publishing_across_ffi() {
        let event_publisher = EventPublisher::new();
        let mock_ffi_bridge = MockFfiBridge::new();
        
        event_publisher.publish_orchestration_started(1).await.unwrap();
        
        // Verify event was published to framework
        assert!(mock_ffi_bridge.received_event("orchestration.started"));
    }
}
```

## Revised Development Plan Impact

### **Major Changes to Original Plan:**

#### **Phase 1: Foundation Layer** (ORCHESTRATION BRANCH SCOPE)
- **Add**: Complete orchestration core (WorkflowCoordinator + StepExecutor)
- **Add**: Base step handler framework (BaseStepHandler trait + implementations)
- **Add**: YAML configuration parser and environment resolver
- **Add**: Basic event publishing infrastructure
- **Add**: Rust client library with business logic step handlers for testing
- **Focus**: Database models + orchestration core + step handler framework + test infrastructure
- **Success Criteria**: Complete orchestration loop working with Rust business logic handlers

#### **Phase 2: State Management Integration** (ORCHESTRATION BRANCH SCOPE)
- **Add**: Dual finalization strategy implementation
- **Add**: State machine integration with orchestration
- **Add**: Comprehensive state transition validation
- **Focus**: High-performance state transitions with proper coordination
- **Success Criteria**: State transitions work seamlessly with orchestration patterns

#### **Phase 3: Integration Testing & Performance** (ORCHESTRATION BRANCH SCOPE)
- **Add**: Complete integration test suite with complex workflows (Linear, Diamond, Tree, Fan-in/Fan-out)
- **Add**: Performance benchmarking against PostgreSQL functions (10-100x improvement target)
- **Add**: Circuit breaker and retry strategy testing
- **Add**: Error handling validation (permanent vs retryable classification)
- **Focus**: Comprehensive testing and performance validation
- **Success Criteria**: All workflow patterns working with measurable performance improvements

#### **Phase 4: Production Readiness** (ORCHESTRATION BRANCH SCOPE)
- **Add**: Comprehensive observability and metrics
- **Add**: Resource management and health checking
- **Add**: Documentation and API design validation
- **Focus**: Production resilience patterns
- **Success Criteria**: Orchestration core ready for framework consumption

### **Future Work (NOT in this branch):**

#### **FFI Integration Phase**
- Ruby FFI bindings for Rails integration
- Python FFI bindings using PyO3
- Node.js FFI bindings using N-API
- Cross-language event publishing

#### **Framework-Specific Adapters Phase**
- Tasker-Rails gem enhancement
- Tasker-Loco crate
- Tasker-Django package
- Other framework-specific integrations

## Success Metrics (Revised)

### **Performance Targets:**
- **Dependency Resolution**: 10-100x faster than PostgreSQL functions ✓
- **Step Coordination**: <1ms overhead per step handoff
- **FFI Overhead**: <10% performance penalty vs native Ruby
- **Memory Usage**: Minimal footprint between delegations
- **Event Publishing**: <100μs latency for event publication across FFI boundary
- **Configuration Parsing**: <10ms for YAML configuration parsing and validation
- **State Transitions**: <50μs for state machine transitions
- **Viable Step Discovery**: <5ms for dependency graph analysis on 1000+ step workflows

### **Integration Targets:**
- **Clean API Design**: Well-defined public interface for framework consumers
- **Pattern Validation**: Four-phase step handler pattern working correctly
- **Event System**: Comprehensive event publishing within orchestration core
- **Configuration System**: Full YAML configuration parsing and validation
- **Error Handling**: Proper classification of permanent vs retryable errors
- **Circuit Breaker**: <1% false positive rate for circuit breaker activation
- **Test Coverage**: All workflow patterns validated through Rust client library

### **Production Readiness Targets:**
- **Availability**: 99.9% uptime for orchestration components
- **Observability**: Full metrics, tracing, and health checking
- **Resource Management**: Automatic resource cleanup and leak prevention
- **Error Recovery**: Graceful degradation under failure conditions
- **API Design**: Clean, intuitive public interface validated through client library
- **Test Coverage**: >90% code coverage with comprehensive integration tests
- **Documentation**: Complete API documentation for orchestration core usage

### **Validation Criteria:**
- **Four-Phase Pattern**: All step handlers follow the proven four-phase pattern
- **Idempotency**: All operations are safely retryable without side effects
- **Dual Finalization**: Synchronous and asynchronous finalization modes work correctly
- **YAML Configuration**: Environment-specific overrides and schema validation
- **Registry Integration**: Thread-safe registry operations with proper validation
- **Event System**: Complete event catalog with custom event support
- **State Machine Integration**: Seamless integration with existing state machines
- **Rust Client Library**: Complete business logic step handlers for comprehensive testing
- **Workflow Patterns**: Linear, Diamond, Tree, and Fan-in/Fan-out patterns all working correctly
- **Public API Design**: Clean interface that any framework can consume

### **Integration Testing Requirements:**
- **Complex Workflow Testing**: All workflow patterns tested with Rust business logic handlers
- **Error Scenario Testing**: Permanent vs retryable error classification validated
- **Retry Logic Testing**: Exponential backoff and circuit breaker patterns working
- **Performance Benchmarking**: Measurable improvements over PostgreSQL-based dependency resolution
- **API Validation**: Public interface tested through comprehensive client library usage
- **Event System Testing**: All events published correctly within orchestration boundaries

## Performance and Concurrency Test Plan

### **Domain-Reasonable Boundaries**

Based on production analysis, most Tasker workflows have well-defined characteristics:

**Workflow Characteristics**:
- **Typical Step Count**: 3-12 steps per workflow (95th percentile)
- **Maximum Step Count**: ≤20 steps per workflow (enterprise upper bound)
- **Typical Fanout**: 2-5 parallel branches maximum
- **Maximum Depth**: ≤10 levels of dependency nesting
- **Typical DAG Complexity**: Linear (40%), Diamond (30%), Tree (20%), Mixed (10%)

**Concurrency Characteristics**:
- **Typical Concurrent Workflows**: 10-100 simultaneous workflows
- **Peak Concurrent Workflows**: 500-1000 simultaneous workflows (enterprise)
- **Typical Concurrent Steps**: 50-500 steps executing simultaneously
- **Peak Concurrent Steps**: 2000-5000 steps executing simultaneously
- **Batch Processing**: 10-50 workflows processed in batch operations

### **Performance Test Categories**

#### **1. Orchestration Core Performance Tests**

**Target**: 10-100x performance improvement over PostgreSQL dependency resolution

```rust
#[cfg(test)]
mod orchestration_performance_tests {
    use super::*;
    use std::time::Instant;
    
    #[tokio::test]
    async fn test_dependency_resolution_performance() {
        // Test dependency resolution speed vs PostgreSQL
        let coordinator = create_test_coordinator().await;
        
        // Create workflows with different complexity levels
        let test_cases = vec![
            (5, "Simple Linear"),      // 5 steps
            (10, "Medium Complexity"), // 10 steps with 2-3 parallel branches
            (20, "Maximum Complexity") // 20 steps with complex DAG
        ];
        
        for (step_count, description) in test_cases {
            let task_id = create_complex_workflow_task(step_count).await;
            
            // Test Rust dependency resolution
            let start = Instant::now();
            let viable_steps = coordinator
                .step_discovery
                .find_viable_steps(task_id)
                .await
                .unwrap();
            let rust_duration = start.elapsed();
            
            // Test PostgreSQL dependency resolution (baseline)
            let start = Instant::now();
            let pg_viable_steps = get_step_readiness_status_sql(task_id).await;
            let pg_duration = start.elapsed();
            
            // Performance assertions
            let performance_ratio = pg_duration.as_nanos() as f64 / rust_duration.as_nanos() as f64;
            
            println!("{}: Rust {}μs, PostgreSQL {}μs, Ratio: {:.1}x", 
                description, 
                rust_duration.as_micros(), 
                pg_duration.as_micros(),
                performance_ratio
            );
            
            // Target: At least 10x improvement
            assert!(performance_ratio >= 10.0, 
                "Performance improvement insufficient: {:.1}x (expected ≥10x)", performance_ratio);
            
            // Target: Under 5ms for maximum complexity
            if step_count == 20 {
                assert!(rust_duration < Duration::from_millis(5),
                    "Maximum complexity resolution too slow: {}ms", rust_duration.as_millis());
            }
            
            // Verify same results
            assert_eq!(viable_steps.len(), pg_viable_steps.len(),
                "Different result counts between Rust and PostgreSQL");
        }
    }
    
    #[tokio::test]
    async fn test_step_coordination_overhead() {
        // Test step execution handoff overhead
        let coordinator = create_test_coordinator().await;
        let task_id = create_linear_workflow_task(10).await;
        
        let mut handoff_times = Vec::new();
        
        for _ in 0..100 {
            let start = Instant::now();
            let viable_steps = coordinator
                .step_discovery
                .find_viable_steps(task_id)
                .await
                .unwrap();
            
            if !viable_steps.is_empty() {
                let step_result = coordinator
                    .step_executor
                    .execute_steps(task_id, &viable_steps[0..1], &create_test_context())
                    .await
                    .unwrap();
            }
            
            let handoff_time = start.elapsed();
            handoff_times.push(handoff_time);
        }
        
        let avg_handoff = handoff_times.iter().sum::<Duration>() / handoff_times.len() as u32;
        let p95_handoff = percentile(&handoff_times, 95);
        
        // Target: <1ms average handoff time
        assert!(avg_handoff < Duration::from_millis(1),
            "Average handoff time too slow: {}μs", avg_handoff.as_micros());
            
        // Target: <2ms 95th percentile
        assert!(p95_handoff < Duration::from_millis(2),
            "P95 handoff time too slow: {}μs", p95_handoff.as_micros());
    }
}
```

#### **2. Concurrency and Load Tests**

**Target**: Handle realistic concurrent loads without performance degradation

```rust
#[tokio::test]
async fn test_concurrent_workflow_execution() {
    // Test multiple workflows executing simultaneously
    let coordinator = Arc::new(create_test_coordinator().await);
    let concurrent_workflows = 100; // Typical production load
    
    let start = Instant::now();
    let mut tasks = Vec::new();
    
    // Launch concurrent workflows
    for i in 0..concurrent_workflows {
        let coordinator_clone = Arc::clone(&coordinator);
        let task = tokio::spawn(async move {
            let task_id = create_random_workflow_task().await;
            coordinator_clone
                .orchestrate_task(task_id, &TestFrameworkIntegration::new())
                .await
        });
        tasks.push(task);
    }
    
    // Wait for all workflows to complete
    let results: Vec<_> = futures::future::try_join_all(tasks).await.unwrap();
    let total_duration = start.elapsed();
    
    // Performance assertions
    let avg_time_per_workflow = total_duration / concurrent_workflows;
    let success_count = results.iter()
        .filter(|r| matches!(r, Ok(TaskResult::Complete(_))))
        .count();
    
    // Target: All workflows succeed
    assert_eq!(success_count, concurrent_workflows as usize,
        "Not all concurrent workflows succeeded");
        
    // Target: Average workflow time under 100ms at 100 concurrent
    assert!(avg_time_per_workflow < Duration::from_millis(100),
        "Average workflow time too slow under load: {}ms", 
        avg_time_per_workflow.as_millis());
    
    // Target: Total throughput > 1000 workflows/second
    let throughput = concurrent_workflows as f64 / total_duration.as_secs_f64();
    assert!(throughput > 1000.0,
        "Throughput too low: {:.1} workflows/second", throughput);
}

#[tokio::test]
async fn test_peak_concurrent_step_execution() {
    // Test maximum concurrent step execution
    let coordinator = create_test_coordinator().await;
    let concurrent_steps = 2000; // Peak enterprise load
    
    let task_ids: Vec<_> = (0..concurrent_steps)
        .map(|_| create_single_step_workflow_task())
        .collect::<FuturesUnordered<_>>()
        .try_collect()
        .await
        .unwrap();
    
    let start = Instant::now();
    
    // Execute all steps simultaneously
    let mut step_tasks = Vec::new();
    for task_id in task_ids {
        let coordinator_clone = coordinator.clone();
        let task = tokio::spawn(async move {
            coordinator_clone
                .orchestrate_task(task_id, &TestFrameworkIntegration::new())
                .await
        });
        step_tasks.push(task);
    }
    
    let results = futures::future::try_join_all(step_tasks).await.unwrap();
    let total_duration = start.elapsed();
    
    // Performance assertions
    let success_count = results.iter()
        .filter(|r| matches!(r, Ok(TaskResult::Complete(_))))
        .count();
    
    assert_eq!(success_count, concurrent_steps as usize);
    
    // Target: Handle 2000 concurrent steps in under 5 seconds
    assert!(total_duration < Duration::from_secs(5),
        "Peak concurrent step execution too slow: {}s", total_duration.as_secs());
    
    // Target: Step throughput > 500 steps/second
    let step_throughput = concurrent_steps as f64 / total_duration.as_secs_f64();
    assert!(step_throughput > 500.0,
        "Step throughput too low: {:.1} steps/second", step_throughput);
}
```

#### **3. Memory and Resource Efficiency Tests**

**Target**: Minimal memory footprint and efficient resource usage

```rust
#[tokio::test]
async fn test_memory_efficiency_under_load() {
    // Test memory usage doesn't grow unbounded
    let coordinator = create_test_coordinator().await;
    let initial_memory = get_memory_usage();
    
    // Process 1000 workflows in batches
    for batch in 0..10 {
        let mut batch_tasks = Vec::new();
        
        for _ in 0..100 {
            let task_id = create_mixed_complexity_workflow_task().await;
            let coordinator_clone = coordinator.clone();
            let task = tokio::spawn(async move {
                coordinator_clone
                    .orchestrate_task(task_id, &TestFrameworkIntegration::new())
                    .await
            });
            batch_tasks.push(task);
        }
        
        // Wait for batch to complete
        let _results = futures::future::try_join_all(batch_tasks).await.unwrap();
        
        // Force garbage collection
        tokio::task::yield_now().await;
        
        let current_memory = get_memory_usage();
        let memory_growth = current_memory - initial_memory;
        
        // Target: Memory growth <100MB after 1000 workflows
        assert!(memory_growth < 100_000_000, // 100MB
            "Excessive memory growth: {}MB after {} batches", 
            memory_growth / 1_000_000, batch + 1);
    }
}

#[tokio::test]
async fn test_resource_cleanup() {
    // Test that resources are properly cleaned up
    let coordinator = create_test_coordinator().await;
    let resource_monitor = create_resource_monitor();
    
    // Process workflows with deliberate failures
    for _ in 0..100 {
        let task_id = create_failing_workflow_task().await;
        
        let result = coordinator
            .orchestrate_task(task_id, &TestFrameworkIntegration::new())
            .await;
        
        // Should fail, but resources should be cleaned up
        assert!(matches!(result, Ok(TaskResult::Error(_))));
    }
    
    // Check for resource leaks
    let leaked_connections = resource_monitor.count_leaked_connections();
    let leaked_memory = resource_monitor.count_leaked_memory_blocks();
    let leaked_file_handles = resource_monitor.count_leaked_file_handles();
    
    assert_eq!(leaked_connections, 0, "Database connection leaks detected");
    assert_eq!(leaked_memory, 0, "Memory leaks detected");
    assert_eq!(leaked_file_handles, 0, "File handle leaks detected");
}
```

#### **4. Error Handling and Circuit Breaker Tests**

**Target**: Robust error handling with proper circuit breaker behavior

```rust
#[tokio::test]
async fn test_circuit_breaker_under_sustained_failures() {
    // Test circuit breaker activates correctly under failure conditions
    let coordinator = create_test_coordinator().await;
    
    // Create workflow with step that will fail repeatedly
    let task_id = create_failing_step_workflow_task().await;
    
    let mut failure_count = 0;
    let mut circuit_opened = false;
    
    // Keep trying until circuit breaker opens
    for attempt in 0..20 {
        let start = Instant::now();
        let viable_steps = coordinator
            .step_discovery
            .find_viable_steps(task_id)
            .await
            .unwrap();
        
        if viable_steps.is_empty() {
            // Circuit breaker has opened
            circuit_opened = true;
            break;
        }
        
        // Execute the failing step
        let result = coordinator
            .step_executor
            .execute_steps(task_id, &viable_steps[0..1], &create_test_context())
            .await;
        
        if result.is_err() || matches!(result, Ok(ref results) if results[0].status == StepStatus::Failed) {
            failure_count += 1;
        }
        
        // Wait for backoff period
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Circuit breaker should have opened after multiple failures
    assert!(circuit_opened, "Circuit breaker did not open after {} failures", failure_count);
    assert!(failure_count >= 3, "Not enough failure attempts before circuit opened");
    assert!(failure_count <= 10, "Too many attempts before circuit opened");
}

#[tokio::test]
async fn test_graceful_degradation_under_overload() {
    // Test system behavior under extreme load
    let coordinator = create_test_coordinator().await;
    let overload_factor = 10; // 10x normal load
    
    let concurrent_workflows = 1000 * overload_factor;
    let start = Instant::now();
    
    let mut tasks = Vec::new();
    for _ in 0..concurrent_workflows {
        let coordinator_clone = coordinator.clone();
        let task = tokio::spawn(async move {
            let task_id = create_simple_workflow_task().await;
            coordinator_clone
                .orchestrate_task(task_id, &TestFrameworkIntegration::new())
                .await
        });
        tasks.push(task);
    }
    
    // Don't wait for all to complete - test graceful degradation
    tokio::time::timeout(Duration::from_secs(30), async {
        let results = futures::future::join_all(tasks).await;
        
        let completed_count = results.iter()
            .filter(|r| matches!(r, Ok(Ok(TaskResult::Complete(_)))))
            .count();
        
        let error_count = results.iter()
            .filter(|r| matches!(r, Ok(Ok(TaskResult::Error(_)))))
            .count();
        
        let system_error_count = results.iter()
            .filter(|r| r.is_err())
            .count();
        
        // Under overload, some workflows should complete
        assert!(completed_count > 0, "No workflows completed under overload");
        
        // System should gracefully handle overload (not crash)
        let total_processed = completed_count + error_count + system_error_count;
        assert!(total_processed > concurrent_workflows / 10,
            "System processed too few requests under overload");
        
        // Most failures should be graceful workflow errors, not system crashes
        assert!(system_error_count < total_processed / 5,
            "Too many system errors under overload: {}/{}", 
            system_error_count, total_processed);
    })
    .await
    .expect("Graceful degradation test timed out");
}
```

#### **5. Event System Performance Tests**

**Target**: Low-latency event publishing with high throughput

```rust
#[tokio::test]
async fn test_event_publishing_performance() {
    // Test event publishing latency and throughput
    let event_publisher = create_test_event_publisher().await;
    let event_count = 10_000;
    
    let mut publish_times = Vec::new();
    let start = Instant::now();
    
    for i in 0..event_count {
        let event_start = Instant::now();
        
        event_publisher
            .publish_viable_steps_discovered(i, &create_test_viable_steps())
            .await
            .unwrap();
        
        let event_time = event_start.elapsed();
        publish_times.push(event_time);
    }
    
    let total_time = start.elapsed();
    let avg_publish_time = publish_times.iter().sum::<Duration>() / publish_times.len() as u32;
    let p95_publish_time = percentile(&publish_times, 95);
    let throughput = event_count as f64 / total_time.as_secs_f64();
    
    // Target: Average event publishing <100μs
    assert!(avg_publish_time < Duration::from_micros(100),
        "Average event publishing too slow: {}μs", avg_publish_time.as_micros());
    
    // Target: P95 event publishing <1ms
    assert!(p95_publish_time < Duration::from_millis(1),
        "P95 event publishing too slow: {}μs", p95_publish_time.as_micros());
    
    // Target: Event throughput >100,000 events/second
    assert!(throughput > 100_000.0,
        "Event throughput too low: {:.0} events/second", throughput);
}
```

### **Performance Monitoring and Metrics**

```rust
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub dependency_resolution_time: Duration,
    pub step_coordination_overhead: Duration,
    pub event_publishing_latency: Duration,
    pub memory_usage_bytes: u64,
    pub concurrent_workflows: u32,
    pub concurrent_steps: u32,
    pub success_rate: f64,
    pub error_rate: f64,
    pub circuit_breaker_activations: u32,
}

impl PerformanceMetrics {
    pub fn assert_production_readiness(&self) {
        // Dependency resolution performance
        assert!(self.dependency_resolution_time < Duration::from_millis(5),
            "Dependency resolution too slow: {}ms", 
            self.dependency_resolution_time.as_millis());
        
        // Step coordination efficiency
        assert!(self.step_coordination_overhead < Duration::from_millis(1),
            "Step coordination overhead too high: {}μs",
            self.step_coordination_overhead.as_micros());
        
        // Event system performance  
        assert!(self.event_publishing_latency < Duration::from_micros(100),
            "Event publishing too slow: {}μs",
            self.event_publishing_latency.as_micros());
        
        // Memory efficiency
        assert!(self.memory_usage_bytes < 100_000_000, // 100MB max
            "Memory usage too high: {}MB",
            self.memory_usage_bytes / 1_000_000);
        
        // Success rate
        assert!(self.success_rate > 0.99,
            "Success rate too low: {:.2}%", self.success_rate * 100.0);
        
        // Error rate
        assert!(self.error_rate < 0.01,
            "Error rate too high: {:.2}%", self.error_rate * 100.0);
    }
}
```

### **Load Test Scenarios**

#### **Realistic Production Simulation**
- **Daily Load**: 10,000 workflows, 3-8 steps each, executed over 8 hours
- **Peak Load**: 500 simultaneous workflows for 30 minutes
- **Burst Load**: 1,000 workflows submitted in 60 seconds
- **Error Scenarios**: 5% step failure rate with proper retry behavior
- **Mixed Complexity**: 70% simple (≤5 steps), 25% medium (6-12 steps), 5% complex (13-20 steps)

#### **Stress Test Scenarios** 
- **Maximum Concurrency**: 1,000 simultaneous workflows with 5,000 concurrent steps
- **Complex DAG Stress**: 100 workflows with maximum 20-step complexity
- **Error Storm**: 50% failure rate to test circuit breaker behavior
- **Memory Pressure**: Sustained load to test garbage collection and memory management
- **Database Pressure**: High concurrent database access patterns

### **Success Criteria Summary**

**Core Performance Requirements**:
- **Dependency Resolution**: ≥10x faster than PostgreSQL (target: ≥50x)
- **Step Coordination**: <1ms average overhead per handoff
- **Event Publishing**: <100μs average latency
- **Memory Usage**: <100MB steady state, <200MB under peak load
- **Throughput**: >1,000 workflows/second, >5,000 steps/second
- **Success Rate**: >99% under normal load, >95% under stress

**Scalability Requirements**:
- **Concurrent Workflows**: Handle 1,000 simultaneous workflows
- **Concurrent Steps**: Handle 5,000 simultaneous step executions  
- **Complex Workflows**: Process 20-step DAGs in <10ms
- **Error Handling**: <1% false positive circuit breaker activations
- **Resource Efficiency**: No memory leaks over 24-hour runs

**Production Readiness**:
- **Availability**: 99.9% uptime under realistic load
- **Error Recovery**: Graceful degradation under 10x overload
- **Observability**: Complete metrics for all performance characteristics
- **Deterministic Behavior**: Consistent performance across test runs

This performance and concurrency test plan provides comprehensive validation that the Rust orchestration core can handle real-world production workloads while delivering the targeted 10-100x performance improvements over the existing PostgreSQL-based dependency resolution system.

This analysis fundamentally reshapes our approach from building a replacement system to building a **complete high-performance orchestration core** that includes WorkflowCoordinator, StepExecutor, and base step handler framework. The clean API design and Rust client library serve as the foundation and pattern for how any framework - whether Rails, Django, Loco, or others - would integrate with Tasker in the future.
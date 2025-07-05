# System Patterns: Tasker Core Rust

## Architecture Overview

### Step Handler Foundation Pattern

The core architectural pattern is **step handler foundation** where the Rust core implements the complete step handler base class that frameworks extend through subclassing with overridable hooks.

```rust
// Core step handler foundation in Rust
pub struct StepHandler {
    // Foundation handles all lifecycle logic
    backoff_calculator: BackoffCalculator,
    retry_analyzer: RetryAnalyzer,
    output_processor: OutputProcessor,
    task_finalizer: TaskFinalizer,
}

impl StepHandler {
    // Foundation method - handles complete step lifecycle
    pub async fn handle(&self, step: &WorkflowStep) -> StepResult {
        // 1. Pre-processing and validation
        let context = self.prepare_context(step).await?;

        // 2. Call framework-overridable process method
        let output = self.process(context).await?;

        // 3. Process results and handle output
        let processed_output = self.process_results(output).await?;

        // 4. Handle backoff, retry analysis, finalization
        self.finalize_step(step, processed_output).await
    }

    // Framework-overridable hooks
    async fn process(&self, context: StepContext) -> Result<StepOutput>;
    async fn process_results(&self, output: StepOutput) -> Result<ProcessedOutput>;
}
```

### Component Architecture

```
src/
├── models/          # Database entities with FFI-compatible serialization
├── orchestration/   # Core engine (Coordinator, ViableStepDiscovery, TaskFinalizer)
├── step_handler/    # Step handler foundation (base class, lifecycle management)
├── state_machine/   # State management (TaskStateMachine, StepStateMachine)
├── events/          # Event system (Publisher, Subscriber, LifecycleEvents)
├── registry/        # Registry systems (HandlerFactory, PluginRegistry)
├── database/        # Database layer (Connection, Migrations, Repositories)
├── ffi/            # Foreign Function Interface (Ruby, Python, C API)
├── queue/          # Queue abstraction (dependency injection for different backends)
├── query_builder/   # Type-safe query building with scopes
├── error.rs        # Comprehensive error handling
└── config.rs       # Configuration management
```

## Key Design Patterns

### 1. Step Handler Foundation Pattern

Complete step lifecycle management with framework extension points:

```rust
// Rust foundation - handles all lifecycle logic
pub struct StepHandlerFoundation {
    pub step_id: i64,
    pub task_id: i64,
    pub backoff_calculator: Arc<BackoffCalculator>,
    pub retry_analyzer: Arc<RetryAnalyzer>,
    pub output_processor: Arc<OutputProcessor>,
    pub queue_injector: Arc<dyn QueueInjector>,
}

impl StepHandlerFoundation {
    // Complete handle logic in Rust
    pub async fn handle(&self) -> StepResult {
        let step = self.load_step().await?;

        // Pre-processing
        let context = self.prepare_step_context(&step).await?;

        // Framework hook - user business logic
        let output = self.process(context).await?;

        // Framework hook - post-processing
        let processed = self.process_results(output).await?;

        // Foundation handles finalization
        self.finalize_step(&step, processed).await
    }

    // Framework-overridable methods (FFI hooks)
    async fn process(&self, context: StepContext) -> Result<StepOutput> {
        // Default implementation or framework override
    }

    async fn process_results(&self, output: StepOutput) -> Result<ProcessedOutput> {
        // Default implementation or framework override
    }
}
```

### 2. Queue Abstraction Pattern

Dependency injection for different queue backends:

```rust
// Queue abstraction for dependency injection
#[async_trait]
pub trait QueueInjector: Send + Sync {
    async fn enqueue_task(&self, task_id: i64, delay: Option<Duration>) -> Result<()>;
    async fn enqueue_step(&self, step_id: i64, delay: Option<Duration>) -> Result<()>;
    async fn get_queue_info(&self) -> QueueInfo;
}

// Rails Sidekiq implementation
pub struct SidekiqQueueInjector {
    // Rails-specific queue implementation
}

impl QueueInjector for SidekiqQueueInjector {
    async fn enqueue_task(&self, task_id: i64, delay: Option<Duration>) -> Result<()> {
        // Delegate to Rails Sidekiq via FFI
        self.call_rails_enqueue(task_id, delay).await
    }
}

// Python Celery implementation
pub struct CeleryQueueInjector {
    // Python-specific queue implementation
}
```

### 3. Framework Subclass Pattern

Multi-language step handler subclassing:

```rust
// Ruby FFI - Rails step handler subclass
#[magnus::wrap(class = "Tasker::StepHandler")]
pub struct RubyStepHandler {
    foundation: StepHandlerFoundation,
    ruby_process_method: magnus::Value,
    ruby_process_results_method: magnus::Value,
}

impl RubyStepHandler {
    // Override process method to call Ruby
    async fn process(&self, context: StepContext) -> Result<StepOutput> {
        let ruby_context = context.to_ruby_value()?;
        let ruby_output = self.ruby_process_method.call((ruby_context,))?;
        StepOutput::from_ruby_value(ruby_output)
    }
}

// Python FFI - FastAPI step handler subclass
#[pyclass]
pub struct PyStepHandler {
    foundation: StepHandlerFoundation,
    python_process_method: PyObject,
    python_process_results_method: PyObject,
}
```

### 4. Universal Foundation Pattern

Same core across all frameworks:

```rust
// Universal step handler that works across frameworks
pub struct UniversalStepHandler {
    foundation: StepHandlerFoundation,
    language_binding: Box<dyn LanguageBinding>,
    queue_injector: Arc<dyn QueueInjector>,
}

#[async_trait]
pub trait LanguageBinding: Send + Sync {
    async fn call_process(&self, context: StepContext) -> Result<StepOutput>;
    async fn call_process_results(&self, output: StepOutput) -> Result<ProcessedOutput>;
}
```

### 5. Orchestration Coordination Pattern

Orchestration works with step handler foundation:

```rust
pub struct OrchestrationCoordinator {
    step_discovery: Arc<ViableStepDiscovery>,
    step_handler_factory: Arc<StepHandlerFactory>,
    task_finalizer: Arc<TaskFinalizer>,
}

impl OrchestrationCoordinator {
    pub async fn orchestrate_task(&self, task_id: i64) -> TaskResult {
        loop {
            let viable_steps = self.step_discovery.find_viable_steps(task_id).await?;
            if viable_steps.is_empty() {
                return self.task_finalizer.finalize_task(task_id).await;
            }

            // Create step handlers for each viable step
            for step in viable_steps {
                let handler = self.step_handler_factory.create_handler(step).await?;

                // Foundation handles complete step lifecycle
                let result = handler.handle().await?;

                // Process step result
                self.process_step_result(result).await?;
            }

            // Check if task should continue or re-enqueue
            if self.should_re_enqueue_task(task_id).await? {
                return TaskResult::ReenqueueTask;
            }
        }
    }
}
```

## Database Patterns

### 1. Schema-First Modeling

All Rust models match PostgreSQL schema exactly:

```rust
// Matches tasker_workflow_steps table exactly
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStep {
    pub workflow_step_id: i64,
    pub task_id: i64,
    pub named_step_id: i32,
    pub retryable: bool,
    pub retry_limit: Option<i32>,
    pub in_process: bool,
    pub processed: bool,
    pub processed_at: Option<NaiveDateTime>,
    pub attempts: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
    pub backoff_request_seconds: Option<i32>,
    pub inputs: Option<serde_json::Value>,
    pub results: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub skippable: bool,
}
```

### 2. Step Handler Data Access

Optimized queries for step handler operations:

```rust
impl WorkflowStep {
    // Step handler foundation methods
    pub async fn load_for_processing(pool: &PgPool, step_id: i64) -> Result<Self> {
        sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT * FROM tasker_workflow_steps
            WHERE workflow_step_id = $1 AND in_process = false AND processed = false
            FOR UPDATE SKIP LOCKED
            "#,
            step_id
        )
        .fetch_one(pool)
        .await
        .map_err(Into::into)
    }

    pub async fn mark_in_process(&mut self, pool: &PgPool) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps
            SET in_process = true, last_attempted_at = NOW(), attempts = COALESCE(attempts, 0) + 1
            WHERE workflow_step_id = $1
            "#,
            self.workflow_step_id
        )
        .execute(pool)
        .await?;

        self.in_process = true;
        Ok(())
    }
}
```

## Concurrency Patterns

### 1. Step Handler Concurrency

Thread-safe step handler execution:

```rust
pub struct ConcurrentStepExecutor {
    handlers: Arc<DashMap<i64, Arc<StepHandlerFoundation>>>,
    semaphore: Arc<Semaphore>,
}

impl ConcurrentStepExecutor {
    pub async fn execute_step(&self, step_id: i64) -> Result<StepResult> {
        let _permit = self.semaphore.acquire().await?;

        let handler = self.handlers.get(&step_id)
            .ok_or_else(|| TaskerError::StepHandlerNotFound(step_id))?;

        handler.handle().await
    }
}
```

### 2. Queue Injection Safety

Thread-safe queue operations:

```rust
pub struct ThreadSafeQueueInjector {
    inner: Arc<RwLock<Box<dyn QueueInjector>>>,
}

impl QueueInjector for ThreadSafeQueueInjector {
    async fn enqueue_task(&self, task_id: i64, delay: Option<Duration>) -> Result<()> {
        let injector = self.inner.read().await;
        injector.enqueue_task(task_id, delay).await
    }
}
```

## Error Handling Patterns

### 1. Step Handler Error Management

Comprehensive error handling for step execution:

```rust
#[derive(Debug, thiserror::Error)]
pub enum StepHandlerError {
    #[error("Step processing failed: {0}")]
    ProcessingError(String),

    #[error("Step results processing failed: {0}")]
    ResultsProcessingError(String),

    #[error("Framework hook failed: {method} - {error}")]
    FrameworkHookError { method: String, error: String },

    #[error("Queue injection failed: {0}")]
    QueueInjectionError(String),

    #[error("Step handler not found: {step_id}")]
    StepHandlerNotFound(i64),
}
```

### 2. Framework Error Propagation

Error handling across language boundaries:

```rust
impl StepHandlerFoundation {
    async fn handle_with_error_propagation(&self) -> StepResult {
        match self.handle().await {
            Ok(result) => result,
            Err(e) => {
                // Log error with full context
                tracing::error!(
                    step_id = self.step_id,
                    task_id = self.task_id,
                    error = %e,
                    "Step handler execution failed"
                );

                // Determine if step should retry or fail
                self.handle_step_error(e).await
            }
        }
    }
}
```

## Performance Patterns

### 1. Step Handler Optimization

High-performance step execution:

```rust
pub struct OptimizedStepHandler {
    // Pre-compiled queries
    load_step_query: sqlx::query::Query<'static, sqlx::Postgres, sqlx::postgres::PgArguments>,

    // Cached data
    step_cache: Arc<DashMap<i64, CachedStepData>>,

    // Connection pool
    pool: Arc<PgPool>,
}
```

### 2. Queue Abstraction Performance

Efficient queue operations:

```rust
pub struct BatchQueueInjector {
    batch_size: usize,
    pending_enqueues: Arc<Mutex<Vec<QueueItem>>>,
    flush_interval: Duration,
}

impl BatchQueueInjector {
    pub async fn enqueue_batch(&self) -> Result<()> {
        let items = {
            let mut pending = self.pending_enqueues.lock().await;
            std::mem::take(&mut *pending)
        };

        if !items.is_empty() {
            self.flush_to_queue(items).await?;
        }

        Ok(())
    }
}
```

### 3. Memory-Efficient FFI

Optimized cross-language data transfer:

```rust
// Zero-copy serialization for FFI
pub struct FFIStepContext {
    step_id: i64,
    inputs_ptr: *const u8,
    inputs_len: usize,
}

impl FFIStepContext {
    pub fn from_step_context(context: &StepContext) -> Result<Self> {
        // Serialize once, share across language boundaries
        let serialized = rmp_serde::to_vec(context)?;
        Ok(Self {
            step_id: context.step_id,
            inputs_ptr: serialized.as_ptr(),
            inputs_len: serialized.len(),
        })
    }
}
```

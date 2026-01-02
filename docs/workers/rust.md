# Rust Worker

**Last Updated**: 2026-01-01
**Audience**: Rust Developers
**Status**: Active
**Package**: `workers-rust`
**Related Docs**: [Patterns and Practices](patterns-and-practices.md) | [Worker Event Systems](../worker-event-systems.md) | [API Convergence Matrix](api-convergence-matrix.md)
**Related Tickets**: TAS-112 (Handler Capability Traits)

<- Back to [Worker Crates Overview](README.md)

---

The Rust worker is the native, high-performance implementation for workflow step execution. It demonstrates the full capability of the tasker-worker foundation with zero FFI overhead.

## Quick Start

### Running the Server

```bash
cd workers/rust
cargo run
```

### With Custom Configuration

```bash
TASKER_CONFIG_PATH=/path/to/config.toml cargo run
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `TASKER_CONFIG_PATH` | Path to TOML configuration | Auto-detected |
| `RUST_LOG` | Log level | `info` |

---

## Architecture

### Entry Point

**Location**: `workers/rust/src/main.rs`

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging
    tasker_shared::logging::init_tracing();

    // Bootstrap worker system
    let mut bootstrap_result = bootstrap().await?;

    // Start event handler (legacy path)
    tokio::spawn(async move {
        bootstrap_result.event_handler.start().await
    });

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => { /* shutdown */ }
        _ = wait_for_sigterm() => { /* shutdown */ }
    }

    bootstrap_result.worker_handle.stop()?;
    Ok(())
}
```

### Bootstrap Process

**Location**: `workers/rust/src/bootstrap.rs`

The bootstrap process:

1. Creates step handler registry with all handlers
2. Sets up global event system
3. Bootstraps `tasker-worker` foundation
4. Creates domain event publisher registry
5. Spawns `HandlerDispatchService` for non-blocking dispatch
6. Creates event handler for legacy path

```rust
pub async fn bootstrap() -> Result<RustWorkerBootstrapResult> {
    // Create registry with all handlers
    let registry = Arc::new(RustStepHandlerRegistry::new());

    // Bootstrap worker foundation
    let worker_handle = WorkerBootstrap::bootstrap_with_event_system(...).await?;

    // Set up dispatch service (TAS-67 non-blocking path)
    let dispatch_service = HandlerDispatchService::with_callback(...);

    Ok(RustWorkerBootstrapResult {
        worker_handle,
        event_handler,
        dispatch_service_handle,
    })
}
```

### Handler Dispatch

The Rust worker uses the `HandlerDispatchService` for non-blocking handler execution:

```
┌────────────────────────────────────────────────────────────────┐
│                    RUST HANDLER DISPATCH                        │
└────────────────────────────────────────────────────────────────┘

   PGMQ Queue
        │
        ▼
  ┌─────────────┐
  │  Dispatch   │
  │  Channel    │
  └─────────────┘
        │
        ▼
  ┌─────────────────────────────────────────┐
  │       HandlerDispatchService            │
  │  ┌────────────────────────────────────┐ │
  │  │  Semaphore (10 permits)            │ │
  │  │       │                            │ │
  │  │       ▼                            │ │
  │  │  handler.call(&step_data).await    │ │
  │  │       │                            │ │
  │  │       ▼                            │ │
  │  │  DomainEventCallback               │ │
  │  └────────────────────────────────────┘ │
  └─────────────────────────────────────────┘
        │
        ▼
  ┌─────────────┐
  │ Completion  │
  │  Channel    │
  └─────────────┘
        │
        ▼
   Orchestration
```

---

## Handler Development

### Capability Traits (TAS-112)

Rust uses traits for handler composition, matching the mixin pattern in Ruby/Python/TypeScript.

**Location**: `tasker-worker/src/handler_capabilities.rs`

#### APICapable Trait

For HTTP API integration:

```rust
use tasker_worker::handler_capabilities::APICapable;

impl APICapable for MyHandler {
    // Use the helper methods:
    // - api_success(step_uuid, data, status, headers, execution_time_ms)
    // - api_failure(step_uuid, message, status, error_type, execution_time_ms)
    // - classify_status_code(status) -> ErrorClassification
}
```

#### DecisionCapable Trait

For dynamic workflow routing:

```rust
use tasker_worker::handler_capabilities::DecisionCapable;

impl DecisionCapable for MyHandler {
    // Use the helper methods:
    // - decision_success(step_uuid, step_names, routing_context, execution_time_ms)
    // - skip_branches(step_uuid, reason, routing_context, execution_time_ms)
    // - decision_failure(step_uuid, message, error_type, execution_time_ms)
}
```

#### BatchableCapable Trait

For batch processing:

```rust
use tasker_worker::handler_capabilities::BatchableCapable;

impl BatchableCapable for MyHandler {
    // Use the helper methods:
    // - create_cursor_configs(total_items, worker_count) -> Vec<CursorConfig>
    // - create_cursor_ranges(total_items, batch_size, max_batches) -> Vec<CursorConfig>
    // - batch_analyzer_success(step_uuid, worker_template, configs, total_items, ...)
    // - batch_worker_success(step_uuid, processed, succeeded, failed, skipped, ...)
    // - no_batches_outcome(step_uuid, reason, execution_time_ms)
    // - batch_failure(step_uuid, message, error_type, retryable, ...)
}
```

#### Composing Multiple Traits

```rust
// Implement multiple capability traits for a single handler
pub struct CompositeHandler {
    config: StepHandlerConfig,
}

impl APICapable for CompositeHandler {}
impl DecisionCapable for CompositeHandler {}

#[async_trait]
impl RustStepHandler for CompositeHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Use API capability to fetch data
        let response = self.call_api("/users/123").await?;

        // Use Decision capability to route based on response
        if response.status == 200 {
            self.decision_success(step_uuid, vec!["process_user"], None, 50)
        } else {
            self.api_failure(step_uuid, "API failed", response.status, "api_error", 50)
        }
    }
}
```

### Handler Trait

**Location**: `workers/rust/src/step_handlers/mod.rs`

All Rust handlers implement the `RustStepHandler` trait:

```rust
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;

#[async_trait]
pub trait RustStepHandler: Send + Sync {
    /// Handler name for registration
    fn name(&self) -> &str;

    /// Execute the handler
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult>;

    /// Create a new instance with configuration from YAML
    fn new(config: StepHandlerConfig) -> Self where Self: Sized;
}
```

### Creating a Handler

```rust
use async_trait::async_trait;
use anyhow::Result;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use crate::step_handlers::{RustStepHandler, StepHandlerConfig, success_result};
use serde_json::json;

pub struct ProcessOrderHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessOrderHandler {
    fn name(&self) -> &str {
        "process_order"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract input from task context
        let order_id = step_data.task.context
            .get("order_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing order_id"))?;

        // Process the order
        let result = process_order(order_id).await?;

        // Return success using helper function
        Ok(success_result(
            step_uuid,
            json!({
                "order_id": order_id,
                "status": "processed",
                "total": result.total
            }),
            start_time.elapsed().as_millis() as i64,
            None,
        ))
    }
}
```

### Handler Registration

**Location**: `workers/rust/src/step_handlers/registry.rs`

Handlers are registered in the `RustStepHandlerRegistry`:

```rust
pub struct RustStepHandlerRegistry {
    handlers: HashMap<String, Arc<dyn RustStepHandler>>,
}

impl RustStepHandlerRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
        };

        registry.register_all_handlers();
        registry
    }

    fn register_all_handlers(&mut self) {
        let empty_config = StepHandlerConfig::empty();

        // Linear workflow handlers
        self.register_handler(Arc::new(LinearStep1Handler::new(empty_config.clone())));
        self.register_handler(Arc::new(LinearStep2Handler::new(empty_config.clone())));

        // Order fulfillment handlers
        self.register_handler(Arc::new(ValidateOrderHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ProcessPaymentHandler::new(empty_config.clone())));

        // ... more handlers
    }

    fn register_handler(&mut self, handler: Arc<dyn RustStepHandler>) {
        let name = handler.name().to_string();
        self.handlers.insert(name, handler);
    }

    pub fn get_handler(&self, name: &str) -> Result<Arc<dyn RustStepHandler>, RustStepHandlerError> {
        self.handlers
            .get(name)
            .cloned()
            .ok_or_else(|| RustStepHandlerError::SystemError {
                message: format!("Handler '{}' not found in registry", name),
            })
    }
}
```

---

## Example Handlers

### Linear Workflow

**Location**: `workers/rust/src/step_handlers/linear_workflow.rs`

Simple sequential workflow with 4 steps:

```rust
pub struct LinearStep1Handler;

#[async_trait]
impl RustStepHandler for LinearStep1Handler {
    fn name(&self) -> &str {
        "linear_step_1"
    }

    async fn call(&self, step_data: &StepExecutionData) -> Result<StepHandlerResult> {
        info!("LinearStep1Handler: Processing step");

        let input = step_data.input_data.clone();
        let mut result = serde_json::Map::new();
        result.insert("step1_processed".to_string(), json!(true));
        result.insert("input_received".to_string(), input);

        Ok(StepHandlerResult::success(json!(result)))
    }
}
```

### Diamond Workflow

**Location**: `workers/rust/src/step_handlers/diamond_workflow.rs`

Parallel branching with convergence:

```
    ┌─────┐
    │Start│
    └──┬──┘
       │
  ┌────┴────┐
  ▼         ▼
┌───┐     ┌───┐
│ B │     │ C │
└─┬─┘     └─┬─┘
  │         │
  └────┬────┘
       ▼
    ┌─────┐
    │ End │
    └─────┘
```

### Batch Processing

**Location**: `workers/rust/src/step_handlers/batch_processing_products_csv.rs`

Three-phase batch processing:

1. **Analyzer**: Counts total records
2. **Batch Processor**: Processes chunks
3. **Aggregator**: Combines results

```rust
pub struct CsvBatchProcessorHandler;

#[async_trait]
impl RustStepHandler for CsvBatchProcessorHandler {
    fn name(&self) -> &str {
        "csv_batch_processor"
    }

    async fn call(&self, step_data: &StepExecutionData) -> Result<StepHandlerResult> {
        let batch_size = step_data.step_inputs
            .get("batch_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as usize;

        let start_cursor = step_data.step_inputs
            .get("start_cursor")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        // Process records in batch
        let processed = process_batch(start_cursor, batch_size).await?;

        Ok(StepHandlerResult::success(json!({
            "processed_count": processed,
            "batch_complete": true
        })))
    }
}
```

### Error Injection (Testing)

**Location**: `workers/rust/src/step_handlers/error_injection/`

Handlers for testing retry behavior:

```rust
pub struct FailNTimesHandler;

impl FailNTimesHandler {
    /// Create handler that fails N times before succeeding
    pub fn new(fail_count: u32) -> Self {
        Self { fail_count, attempts: AtomicU32::new(0) }
    }
}

#[async_trait]
impl RustStepHandler for FailNTimesHandler {
    async fn call(&self, _step_data: &StepExecutionData) -> Result<StepHandlerResult> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);

        if attempt < self.fail_count {
            Ok(StepHandlerResult::failure(
                "Intentional failure for testing",
                "test_error",
                true, // retryable
            ))
        } else {
            Ok(StepHandlerResult::success(json!({"attempts": attempt + 1})))
        }
    }
}
```

---

## Domain Events

### TAS-65: Post-Execution Publishing

Handlers can publish domain events after step execution using the `StepEventPublisher` trait:

```rust
use async_trait::async_trait;
use std::sync::Arc;
use tasker_shared::events::domain_events::DomainEventPublisher;
use tasker_worker::worker::step_event_publisher::{
    StepEventPublisher, StepEventContext, PublishResult
};

#[derive(Debug)]
pub struct PaymentEventPublisher {
    domain_publisher: Arc<DomainEventPublisher>,
}

impl PaymentEventPublisher {
    pub fn new(domain_publisher: Arc<DomainEventPublisher>) -> Self {
        Self { domain_publisher }
    }
}

#[async_trait]
impl StepEventPublisher for PaymentEventPublisher {
    fn name(&self) -> &str {
        "PaymentEventPublisher"
    }

    fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
        &self.domain_publisher
    }

    async fn publish(&self, ctx: &StepEventContext) -> PublishResult {
        let mut result = PublishResult::default();

        if ctx.step_succeeded() {
            let payload = json!({
                "order_id": ctx.execution_result.result["order_id"],
                "amount": ctx.execution_result.result["amount"],
            });

            // Uses default impl from trait
            match self.publish_event(ctx, "payment.completed", payload).await {
                Ok(event_id) => result.published.push(event_id),
                Err(e) => result.errors.push(e.to_string()),
            }
        }

        result
    }
}
```

### Dual-Path Delivery

Events can route to different delivery paths:

| Path | Description | Use Case |
|------|-------------|----------|
| `durable` | Published to PGMQ | External consumers, audit |
| `fast` | In-process bus | Metrics, telemetry |

---

## Configuration

### Bootstrap Configuration

```rust
pub struct WorkerBootstrapConfig {
    pub worker_id: String,
    pub enable_web_api: bool,
    pub event_driven_enabled: bool,
    pub deployment_mode_hint: Option<String>,
}

// Default configuration
let config = WorkerBootstrapConfig {
    worker_id: "rust-worker-001".to_string(),
    enable_web_api: true,
    event_driven_enabled: true,
    deployment_mode_hint: Some("Hybrid".to_string()),
    ..Default::default()
};
```

### Dispatch Configuration

```rust
let config = HandlerDispatchConfig {
    max_concurrent_handlers: 10,
    handler_timeout: Duration::from_secs(30),
    service_id: "rust-handler-dispatch".to_string(),
    load_shedding: LoadSheddingConfig::default(),
};
```

---

## Signal Handling

The Rust worker handles graceful shutdown:

```rust
// Wait for shutdown signal
tokio::select! {
    _ = tokio::signal::ctrl_c() => {
        info!("Received Ctrl+C, initiating graceful shutdown...");
    }
    result = wait_for_sigterm() => {
        info!("Received SIGTERM, initiating graceful shutdown...");
    }
}

// Graceful shutdown sequence
bootstrap_result.worker_handle.stop()?;
```

---

## Performance

### Characteristics

- **Zero FFI Overhead**: Native Rust handlers
- **Async/Await**: Non-blocking I/O with Tokio
- **Bounded Concurrency**: Semaphore-limited parallelism
- **Memory Safety**: Rust's ownership model

### Benchmarking

```bash
# Run with release optimizations
cargo run --release

# With performance profiling
RUST_LOG=trace cargo run --release
```

---

## File Structure

```
workers/rust/
├── src/
│   ├── main.rs                  # Entry point
│   ├── bootstrap.rs             # Worker initialization
│   ├── lib.rs                   # Library exports
│   ├── event_handler.rs         # Event bridging (legacy)
│   ├── global_event_system.rs   # Global event coordination
│   ├── step_handlers/
│   │   ├── mod.rs               # Handler traits and types
│   │   ├── registry.rs          # Handler registry
│   │   ├── linear_workflow.rs   # Linear workflow handlers
│   │   ├── diamond_workflow.rs  # Diamond workflow handlers
│   │   ├── tree_workflow.rs     # Tree workflow handlers
│   │   ├── mixed_dag_workflow.rs
│   │   ├── order_fulfillment.rs
│   │   ├── batch_processing_*.rs
│   │   ├── error_injection/     # Test handlers
│   │   └── domain_event_*.rs    # Event publishing
│   └── event_subscribers/
│       ├── mod.rs
│       ├── logging_subscriber.rs
│       └── metrics_subscriber.rs
├── Cargo.toml
└── tests/
```

---

## Testing

### Unit Tests

```bash
cargo test -p workers-rust
```

### Integration Tests

```bash
# With database
DATABASE_URL=postgresql://... cargo test -p workers-rust --test integration
```

### E2E Tests

```bash
# From project root
DATABASE_URL=postgresql://... cargo nextest run --package workers-rust
```

---

## See Also

- [Worker Crates Overview](README.md) - High-level introduction
- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Worker Event Systems](../worker-event-systems.md) - Architecture details
- [Worker Actors](../worker-actors.md) - Actor pattern documentation

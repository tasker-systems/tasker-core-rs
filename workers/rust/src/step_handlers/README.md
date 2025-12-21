# Rust Step Handler Patterns

This document describes recommended patterns for implementing step handlers in Rust workers.
These patterns align with the cross-language API standards established in TAS-92.

## Table of Contents

- [Handler Signature](#handler-signature)
- [Result Construction](#result-construction)
- [Error Handling](#error-handling)
- [API Handler Pattern](#api-handler-pattern)
- [Decision Handler Pattern](#decision-handler-pattern)
- [Batch Processing Pattern](#batch-processing-pattern)
- [Registry Usage](#registry-usage)

## Handler Signature

All Rust handlers implement the `RustStepHandler` trait with an async `call` method:

```rust
use async_trait::async_trait;
use anyhow::Result;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;

#[async_trait]
impl RustStepHandler for MyHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        // Handler implementation
    }

    fn name(&self) -> &str {
        "my_handler"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
```

The `TaskSequenceStep` provides access to:
- `step_data.task` - Task context and metadata
- `step_data.workflow_step` - Step-specific configuration
- `step_data.step_definition` - Handler initialization params
- `step_data.dependency_results` - Results from upstream steps

## Result Construction

### Success Results

```rust
use tasker_shared::messaging::StepExecutionResult;
use serde_json::json;

// Simple success
let result = StepExecutionResult::success(
    step_uuid,
    json!({
        "processed": true,
        "count": 42
    }),
    Some(HashMap::from([
        ("handler".to_string(), json!("my_handler"))
    ])),
);

// Success with timing
let result = success_result(
    step_uuid,
    json!({"output": "value"}),
    elapsed_ms,
    Some(metadata),
);
```

### Failure Results

```rust
use tasker_shared::types::error_types;

// Standard failure with error type constant
let result = StepExecutionResult::failure(
    step_uuid,
    "Payment gateway timeout",
    error_types::TIMEOUT,
    true, // retryable
)
.with_error_code("GATEWAY_TIMEOUT");

// With full metadata
let result = error_result(
    step_uuid,
    "Validation failed".to_string(),
    Some("VALIDATION_001".to_string()),
    Some(error_types::VALIDATION_ERROR.to_string()),
    false, // not retryable
    elapsed_ms,
    Some(metadata),
);
```

## Error Handling

### Standard Error Types

Use the constants from `tasker_shared::types::error_types` for consistency:

```rust
use tasker_shared::types::error_types;

// Available constants:
error_types::PERMANENT_ERROR    // Non-recoverable failures
error_types::RETRYABLE_ERROR    // Transient failures
error_types::VALIDATION_ERROR   // Input validation failures
error_types::TIMEOUT            // Timeout errors
error_types::HANDLER_ERROR      // Handler-internal errors

// Check if a type is standard
if error_types::is_standard(error_type) {
    // Known error type
}

// Get default retry behavior
if error_types::is_typically_retryable(error_type) {
    // Should retry
}
```

### Error Code Helpers

The `StepExecutionResult` provides convenience methods for error codes:

```rust
// Builder pattern for setting error code
let result = StepExecutionResult::failure(step_uuid, "Error", "permanent_error", false)
    .with_error_code("ERR_001")
    .with_error_type(error_types::PERMANENT_ERROR);

// Accessor for reading error code
if let Some(code) = result.error_code() {
    tracing::error!(error_code = code, "Step failed");
}
```

## API Handler Pattern

For handlers that make HTTP requests:

```rust
use reqwest::Client;
use tasker_shared::types::error_types;

pub struct ApiHandler {
    client: Client,
    base_url: String,
}

impl ApiHandler {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn get(&self, path: &str) -> Result<Response, reqwest::Error> {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
    }

    pub async fn post<T: Serialize>(&self, path: &str, body: &T) -> Result<Response, reqwest::Error> {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
    }

    /// Classify HTTP status codes to standard error types
    fn classify_error(status: StatusCode) -> &'static str {
        match status.as_u16() {
            400..=499 => error_types::PERMANENT_ERROR,  // Client errors
            500..=599 => error_types::RETRYABLE_ERROR,  // Server errors
            _ => error_types::HANDLER_ERROR,
        }
    }

    /// Determine if error is retryable based on status
    fn is_retryable(status: StatusCode) -> bool {
        matches!(status.as_u16(), 408 | 429 | 500..=599)
    }
}
```

## Decision Handler Pattern

For handlers that route to different steps based on conditions:

```rust
pub struct DecisionHandler;

#[async_trait]
impl RustStepHandler for DecisionHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract decision criteria from inputs or context
        let order_total = step_data
            .workflow_step
            .inputs
            .get("order_total")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Decision logic
        let (steps_to_activate, routing_reason) = if order_total > 1000.0 {
            (
                vec!["high_value_review", "priority_fulfillment"],
                "high_value_order"
            )
        } else if order_total > 100.0 {
            (vec!["standard_fulfillment"], "standard_order")
        } else {
            (vec!["low_value_batch"], "low_value_order")
        };

        // Return success with routing information
        StepExecutionResult::success(
            step_uuid,
            json!({
                "activated_steps": steps_to_activate,
                "routing_context": {
                    "reason": routing_reason,
                    "order_total": order_total
                }
            }),
            Some(HashMap::from([
                ("routing_reason".to_string(), json!(routing_reason)),
            ])),
        )
    }

    fn name(&self) -> &str {
        "order_router"
    }
}
```

## Batch Processing Pattern

For handlers that process data in batches with cursor-based resumability:

### Batchable Step (Creates Batches)

```rust
use tasker_shared::messaging::{BatchProcessingOutcome, CursorConfig};

// Analyze dataset and create batch configurations
let cursor_configs: Vec<CursorConfig> = (0..worker_count)
    .map(|i| CursorConfig {
        batch_id: format!("{:03}", i + 1),
        start_cursor: json!(i * batch_size),
        end_cursor: json!((i + 1) * batch_size),
        batch_size: batch_size as u32,
    })
    .collect();

let outcome = BatchProcessingOutcome::create_batches(
    "process_batch".to_string(),
    worker_count as u32,
    cursor_configs,
    total_items,
);

StepExecutionResult::success(
    step_uuid,
    json!({
        "batch_processing_outcome": outcome.to_value(),
        "total_items": total_items,
    }),
    None,
)
```

### Batch Worker Step (Processes One Batch)

```rust
use tasker_worker::batch_processing::BatchWorkerContext;

// Extract batch context from step data
let context = BatchWorkerContext::from_step_data(step_data)?;

// Check for no-op scenario
if context.is_no_op() {
    return Ok(success_result(
        step_uuid,
        json!({ "no_op": true }),
        0,
        None,
    ));
}

// Process items in the batch
let start = context.start_position();
let end = context.end_position();

for position in start..end {
    // Process item at position
}

StepExecutionResult::success(
    step_uuid,
    json!({
        "processed_count": end - start,
        "batch_id": context.batch_id(),
    }),
    None,
)
```

### Aggregator Step (Combines Results)

```rust
use tasker_worker::BatchAggregationScenario;

let scenario = BatchAggregationScenario::detect(
    &step_data.dependency_results,
    "batchable_step",    // Name of the batchable step
    "batch_worker_",     // Prefix for batch worker steps
)?;

match scenario {
    BatchAggregationScenario::NoBatches { batchable_result } => {
        // Dataset was too small - get info from batchable step
        let total = batchable_result.result.get("total_items")...
    }
    BatchAggregationScenario::WithBatches { batch_results, worker_count } => {
        // Aggregate results from all batch workers
        for (step_name, result) in batch_results {
            let count = result.result.get("processed_count")...
        }
    }
}
```

## Registry Usage

### Registering Handlers

```rust
use crate::step_handlers::{RustStepHandlerRegistry, StepHandlerConfig};

let mut registry = RustStepHandlerRegistry::new();

// Register with factory function
registry.register_handler("my_handler", |config| {
    Box::new(MyHandler::new(config))
});
```

### Cross-Language Standard API

The registry provides methods that align with Ruby and Python registries:

```rust
// Check registration (cross-language standard)
if registry.is_registered("my_handler") {
    // Handler exists
}

// List all handlers (cross-language standard)
let handlers: Vec<String> = registry.list_handlers();

// Get handler (alias: resolve)
if let Some(handler) = registry.get_handler("my_handler") {
    let result = handler.call(&step_data).await?;
}
```

## See Also

- [batch_processing_example.rs](./batch_processing_example.rs) - Complete batch processing example
- [batch_processing_products_csv.rs](./batch_processing_products_csv.rs) - CSV batch processing with real file I/O
- [error_types module](../../../tasker-shared/src/types/error_types.rs) - Standard error type constants

# StepContext API Reference

StepContext is the primary data access object for step handlers across all languages in the Tasker worker ecosystem. It provides a consistent interface for accessing task inputs, dependency results, configuration, and checkpoint data.

## Overview

Every step handler receives a StepContext (or `TaskSequenceStep` in Rust) that contains:

- **Task context** - Input data for the workflow (JSONB from task.context)
- **Dependency results** - Results from upstream DAG steps
- **Step configuration** - Handler-specific settings from the template
- **Checkpoint data** - Batch processing state for resumability (TAS-125)
- **Retry information** - Current attempt count and max retries

## Cross-Language API Reference

### Core Data Access

| Operation | Rust | Ruby | Python | TypeScript |
|-----------|------|------|--------|------------|
| Get task input | `get_input::<T>("key")?` | `get_input("key")` | `get_input("key")` | `getInput("key")` |
| Get input with default | `get_input_or("key", default)` | `get_input_or("key", default)` | `get_input_or("key", default)` | `getInputOr("key", default)` |
| Get config value | `get_config::<T>("key")?` | `get_config("key")` | `get_config("key")` | `getConfig("key")` |
| Get dependency result | `get_dependency_result_column_value::<T>("step")?` | `get_dependency_result("step")` | `get_dependency_result("step")` | `getDependencyResult("step")` |
| Get nested dependency field | `get_dependency_field::<T>("step", &["path"])?` | `get_dependency_field("step", *path)` | `get_dependency_field("step", *path)` | `getDependencyField("step", ...path)` |

### Retry Helpers

| Operation | Rust | Ruby | Python | TypeScript |
|-----------|------|------|--------|------------|
| Check if retry | `is_retry()` | `is_retry?` | `is_retry()` | `isRetry()` |
| Check if last retry | `is_last_retry()` | `is_last_retry?` | `is_last_retry()` | `isLastRetry()` |
| Get retry count | `retry_count()` | `retry_count` | `retry_count` | `retryCount` |
| Get max retries | `max_retries()` | `max_retries` | `max_retries` | `maxRetries` |

### Checkpoint Access (TAS-125)

| Operation | Rust | Ruby | Python | TypeScript |
|-----------|------|------|--------|------------|
| Get raw checkpoint | `checkpoint()` | `checkpoint` | `checkpoint` | `checkpoint` |
| Get cursor | `checkpoint_cursor::<T>()` | `checkpoint_cursor` | `checkpoint_cursor` | `checkpointCursor` |
| Get items processed | `checkpoint_items_processed()` | `checkpoint_items_processed` | `checkpoint_items_processed` | `checkpointItemsProcessed` |
| Get accumulated results | `accumulated_results::<T>()` | `accumulated_results` | `accumulated_results` | `accumulatedResults` |
| Check has checkpoint | `has_checkpoint()` | `has_checkpoint?` | `has_checkpoint()` | `hasCheckpoint()` |

### Standard Fields

| Field | Rust | Ruby | Python | TypeScript |
|-------|------|------|--------|------------|
| Task UUID | `task.task.task_uuid` | `task_uuid` | `task_uuid` | `taskUuid` |
| Step UUID | `workflow_step.workflow_step_uuid` | `step_uuid` | `step_uuid` | `stepUuid` |
| Correlation ID | `task.task.correlation_id` | `task.correlation_id` | `correlation_id` | `correlationId` |
| Input data (raw) | `task.task.context` | `input_data` / `context` | `input_data` | `inputData` |
| Step config (raw) | `step_definition.handler.initialization` | `step_config` | `step_config` | `stepConfig` |

## Usage Examples

### Rust

```rust
use tasker_shared::types::base::TaskSequenceStep;

async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    // Get task input
    let order_id: String = step_data.get_input("order_id")?;
    let batch_size: i32 = step_data.get_input_or("batch_size", 100);

    // Get config
    let api_url: String = step_data.get_config("api_url")?;

    // Get dependency result
    let validation_result: ValidationResult = step_data.get_dependency_result_column_value("validate")?;

    // Extract nested field from dependency
    let item_count: i32 = step_data.get_dependency_field("process", &["stats", "count"])?;

    // Check retry status
    if step_data.is_retry() {
        println!("Retry attempt {}", step_data.retry_count());
    }

    // Resume from checkpoint
    let cursor: Option<i64> = step_data.checkpoint_cursor();
    let start_from = cursor.unwrap_or(0);

    // ... handler logic ...
}
```

### Ruby

```ruby
def call(context)
  # Get task input
  order_id = context.get_input('order_id')
  batch_size = context.get_input_or('batch_size', 100)

  # Get config
  api_url = context.get_config('api_url')

  # Get dependency result
  validation_result = context.get_dependency_result('validate')

  # Extract nested field from dependency
  item_count = context.get_dependency_field('process', 'stats', 'count')

  # Check retry status
  if context.is_retry?
    logger.info("Retry attempt #{context.retry_count}")
  end

  # Resume from checkpoint
  start_from = context.checkpoint_cursor || 0

  # ... handler logic ...
end
```

### Python

```python
def call(self, context: StepContext) -> StepHandlerResult:
    # Get task input
    order_id = context.get_input("order_id")
    batch_size = context.get_input_or("batch_size", 100)

    # Get config
    api_url = context.get_config("api_url")

    # Get dependency result
    validation_result = context.get_dependency_result("validate")

    # Extract nested field from dependency
    item_count = context.get_dependency_field("process", "stats", "count")

    # Check retry status
    if context.is_retry():
        print(f"Retry attempt {context.retry_count}")

    # Resume from checkpoint
    start_from = context.checkpoint_cursor or 0

    # ... handler logic ...
```

### TypeScript

```typescript
async call(context: StepContext): Promise<StepHandlerResult> {
  // Get task input
  const orderId = context.getInput<string>('order_id');
  const batchSize = context.getInputOr('batch_size', 100);

  // Get config
  const apiUrl = context.getConfig<string>('api_url');

  // Get dependency result
  const validationResult = context.getDependencyResult('validate');

  // Extract nested field from dependency
  const itemCount = context.getDependencyField('process', 'stats', 'count');

  // Check retry status
  if (context.isRetry()) {
    console.log(`Retry attempt ${context.retryCount}`);
  }

  // Resume from checkpoint
  const startFrom = context.checkpointCursor ?? 0;

  // ... handler logic ...
}
```

## Checkpoint Usage Guide

Checkpoints enable resumable batch processing (TAS-125). When a handler processes large datasets, it can save progress via checkpoints and resume from where it left off on retry.

### Checkpoint Fields

- **cursor** - Position marker (can be int, string, or object)
- **items_processed** - Count of items completed
- **accumulated_results** - Running totals or aggregated state

### Reading Checkpoints

```python
# Python example
def call(self, context: StepContext) -> StepHandlerResult:
    # Check if resuming from checkpoint
    if context.has_checkpoint():
        cursor = context.checkpoint_cursor
        items_done = context.checkpoint_items_processed
        totals = context.accumulated_results or {}
        print(f"Resuming from cursor {cursor}, {items_done} items done")
    else:
        cursor = 0
        items_done = 0
        totals = {}

    # Process from cursor position...
```

### Writing Checkpoints

Checkpoints are written by including checkpoint data in the handler result metadata. See the batch processing documentation for details on the checkpoint yield pattern.

## Notes

- All accessor methods handle missing data gracefully (return None/null or use defaults)
- Dependency results are automatically unwrapped from the `{"result": value}` envelope
- Type conversion is handled automatically where supported (Rust, TypeScript generics)
- Checkpoint data is persisted atomically by the CheckpointService

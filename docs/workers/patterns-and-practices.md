# Worker Crates: Common Patterns and Practices

**Last Updated**: 2026-01-06
**Audience**: Developers, Architects
**Status**: Active (TAS-125 Complete)
**Related Docs**: [Worker Event Systems](../worker-event-systems.md) | [Worker Actors](../worker-actors.md)

<- Back to [Worker Crates Overview](README.md)

---

This document describes the common patterns and practices shared across all three worker implementations (Rust, Ruby, Python). Understanding these patterns helps developers write consistent handlers regardless of the language.

## Table of Contents

- [Architectural Patterns](#architectural-patterns)
- [Handler Lifecycle](#handler-lifecycle)
- [Error Handling](#error-handling)
- [Polling Architecture](#polling-architecture)
- [Event Bridge Pattern](#event-bridge-pattern)
- [Singleton Pattern](#singleton-pattern)
- [Observability](#observability)
- [Checkpoint Yielding](#checkpoint-yielding)

---

## Architectural Patterns

### Dual-Channel Architecture

All workers implement a dual-channel architecture for non-blocking step execution:

```
┌─────────────────────────────────────────────────────────────────┐
│                    DUAL-CHANNEL PATTERN                         │
└─────────────────────────────────────────────────────────────────┘

    PostgreSQL PGMQ
          │
          ▼
  ┌───────────────────┐
  │  Dispatch Channel │  ──→  Step events flow TO handlers
  └───────────────────┘
          │
          ▼
  ┌───────────────────┐
  │  Handler Execution │  ──→  Business logic runs here
  └───────────────────┘
          │
          ▼
  ┌───────────────────┐
  │ Completion Channel │  ──→  Results flow BACK to orchestration
  └───────────────────┘
          │
          ▼
    Orchestration
```

**Benefits**:
- Fire-and-forget dispatch (non-blocking)
- Bounded concurrency via semaphores
- Results processed independently from dispatch
- Consistent pattern across all languages

### Language-Specific Implementations

| Component | Rust | Ruby | Python |
|-----------|------|------|--------|
| Dispatch Channel | `mpsc::channel` | `poll_step_events` FFI | `poll_step_events` FFI |
| Completion Channel | `mpsc::channel` | `complete_step_event` FFI | `complete_step_event` FFI |
| Concurrency Model | Tokio async tasks | Ruby threads + FFI polling | Python threads + FFI polling |
| GIL Handling | N/A | Pull-based polling | Pull-based polling |

---

## Handler Lifecycle

### Handler Registration

All implementations follow the same registration pattern:

```
1. Define handler class/struct
2. Set handler_name identifier
3. Register with HandlerRegistry
4. Handler ready for resolution
```

**Ruby Example**:
```ruby
class ProcessOrderHandler < TaskerCore::StepHandler::Base
  def call(context)
    # Access data via cross-language standard methods
    order_id = context.get_task_field('order_id')

    # Business logic here...

    # Return result using base class helper (keyword args required)
    success(result: { order_id: order_id, status: 'processed' })
  end
end

# Registration
registry = TaskerCore::Registry::HandlerRegistry.instance
registry.register_handler('ProcessOrderHandler', ProcessOrderHandler)
```

**Python Example**:
```python
from tasker_core import StepHandler, StepHandlerResult, HandlerRegistry

class ProcessOrderHandler(StepHandler):
    handler_name = "process_order"

    def call(self, context):
        order_id = context.input_data.get("order_id")
        return StepHandlerResult.success_handler_result(
            {"order_id": order_id, "status": "processed"}
        )

# Registration
registry = HandlerRegistry.instance()
registry.register("process_order", ProcessOrderHandler)
```

### Handler Resolution Flow

```
1. Step event received with handler name
2. Registry.resolve(handler_name) called
3. Handler class instantiated
4. handler.call(context) invoked
5. Result returned to completion channel
```

### Handler Context

All handlers receive a context object containing:

| Field | Description |
|-------|-------------|
| `task_uuid` | Unique identifier for the task |
| `step_uuid` | Unique identifier for the step |
| `input_data` | Task context data passed to the step |
| `dependency_results` | Results from parent/dependency steps |
| `step_config` | Configuration from step definition |
| `step_inputs` | Runtime inputs from workflow_step.inputs |
| `retry_count` | Current retry attempt number |
| `max_retries` | Maximum retry attempts allowed |

### Handler Results

All handlers return a structured result indicating success or failure. However, **the APIs differ between Ruby and Python** - this is a known design inconsistency that may be addressed in a future ticket.

**Ruby** - Uses keyword arguments and separate Success/Error types:
```ruby
# Via base handler shortcuts
success(result: { key: "value" }, metadata: { duration_ms: 150 })

failure(
  message: "Something went wrong",
  error_type: "PermanentError",
  error_code: "VALIDATION_ERROR",  # Ruby has error_code field
  retryable: false,
  metadata: { field: "email" }
)

# Or via type factory methods
TaskerCore::Types::StepHandlerCallResult.success(result: { key: "value" })
TaskerCore::Types::StepHandlerCallResult.error(
  error_type: "PermanentError",
  message: "Error message",
  error_code: "ERR_001"
)
```

**Python** - Uses positional/keyword arguments and a single result type:
```python
# Via base handler shortcuts
self.success(result={"key": "value"}, metadata={"duration_ms": 150})

self.failure(
    message="Something went wrong",
    error_type="ValidationError",  # Python has error_type only (no error_code)
    retryable=False,
    metadata={"field": "email"}
)

# Or via class factory methods
StepHandlerResult.success_handler_result(
    {"key": "value"},             # First positional arg is result
    {"duration_ms": 150}          # Second positional arg is metadata
)
StepHandlerResult.failure_handler_result(
    message="Something went wrong",
    error_type="ValidationError",
    retryable=False,
    metadata={"field": "email"}
)
```

**Key Differences**:
| Aspect | Ruby | Python |
|--------|------|--------|
| Factory method names | `.success()`, `.error()` | `.success_handler_result()`, `.failure_handler_result()` |
| Result type | `Success` / `Error` structs | Single `StepHandlerResult` class |
| Error code field | `error_code` (freeform) | Not present |
| Argument style | Keyword required (`result:`) | Positional allowed |

---

## Error Handling

### Error Classification

All workers classify errors into two categories:

| Type | Description | Behavior |
|------|-------------|----------|
| **Retryable** | Transient errors that may succeed on retry | Step re-enqueued up to max_retries |
| **Permanent** | Unrecoverable errors | Step marked as failed immediately |

### HTTP Status Code Classification (ApiHandler)

```
400, 401, 403, 404, 422  →  Permanent Error (client errors)
429                       →  Retryable Error (rate limiting)
500-599                   →  Retryable Error (server errors)
```

### Exception Hierarchy

**Ruby**:
```ruby
TaskerCore::Error                  # Base class
├── TaskerCore::RetryableError     # Transient failures
├── TaskerCore::PermanentError     # Unrecoverable failures
├── TaskerCore::FFIError           # FFI bridge errors
└── TaskerCore::ConfigurationError # Configuration issues
```

**Python**:
```python
TaskerError                        # Base class
├── WorkerNotInitializedError      # Worker not bootstrapped
├── WorkerBootstrapError           # Bootstrap failed
├── WorkerAlreadyRunningError      # Double initialization
├── FFIError                       # FFI bridge errors
├── ConversionError                # Type conversion errors
└── StepExecutionError             # Handler execution errors
```

### Error Context Propagation

All errors should include context for debugging:

```python
StepHandlerResult.failure_handler_result(
    message="Payment gateway timeout",
    error_type="gateway_timeout",
    retryable=True,
    metadata={
        "gateway": "stripe",
        "request_id": "req_xyz",
        "response_time_ms": 30000
    }
)
```

---

## Polling Architecture

### Why Polling?

Ruby and Python workers use a pull-based polling model due to language runtime constraints:

**Ruby**: The Global VM Lock (GVL) prevents Rust from safely calling Ruby methods from Rust threads. Polling allows Ruby to control thread context.

**Python**: The Global Interpreter Lock (GIL) has the same limitation. Python must initiate all cross-language calls.

### Polling Characteristics

| Parameter | Default Value | Description |
|-----------|---------------|-------------|
| Poll Interval | 10ms | Time between polls when no events |
| Max Latency | ~10ms | Time from event generation to processing start |
| Starvation Check | Every 100 polls (1 second) | Detect processing bottlenecks |
| Cleanup Interval | Every 1000 polls (10 seconds) | Clean up timed-out events |

### Poll Loop Structure

```python
while running:
    # 1. Poll for event
    event = poll_step_events()

    if event:
        # 2. Process event through handler
        process_event(event)
    else:
        # 3. Sleep when no events
        time.sleep(0.01)  # 10ms

    # 4. Periodic maintenance
    if poll_count % 100 == 0:
        check_starvation_warnings()

    if poll_count % 1000 == 0:
        cleanup_timeouts()
```

### FFI Contract

Ruby and Python share the same FFI contract:

| Function | Description |
|----------|-------------|
| `poll_step_events()` | Get next pending event (returns None if empty) |
| `complete_step_event(event_id, result)` | Submit handler result |
| `get_ffi_dispatch_metrics()` | Get dispatch channel metrics |
| `check_starvation_warnings()` | Trigger starvation logging |
| `cleanup_timeouts()` | Clean up timed-out events |

---

## Event Bridge Pattern

### Overview

All workers implement an EventBridge (pub/sub) pattern for internal coordination:

```
┌─────────────────────────────────────────────────────────────────┐
│                      EVENT BRIDGE PATTERN                        │
└─────────────────────────────────────────────────────────────────┘

  Publishers                    EventBridge                 Subscribers
  ─────────                    ───────────                 ───────────
  HandlerRegistry  ──publish──→            ──notify──→  StepExecutionSubscriber
  EventPoller      ──publish──→  [Events]  ──notify──→  MetricsCollector
  Worker           ──publish──→            ──notify──→  Custom Subscribers
```

### Standard Event Names

| Event | Description | Payload |
|-------|-------------|---------|
| `handler_registered` | Handler added to registry | `(name, handler_class)` |
| `step_execution_received` | Step event received | `FfiStepEvent` |
| `step_execution_completed` | Handler finished | `StepHandlerResult` |
| `worker_started` | Worker bootstrap complete | `worker_id` |
| `worker_stopped` | Worker shutdown | `worker_id` |

### Implementation Libraries

| Language | Library | Pattern |
|----------|---------|---------|
| Ruby | `dry-events` | Publisher/Subscriber |
| Python | `pyee` | EventEmitter |
| Rust | Native channels | mpsc |

### Usage Example (Python)

```python
from tasker_core import EventBridge, EventNames

bridge = EventBridge.instance()

# Subscribe to events
def on_step_received(event):
    print(f"Processing step {event.step_uuid}")

bridge.subscribe(EventNames.STEP_EXECUTION_RECEIVED, on_step_received)

# Publish events
bridge.publish(EventNames.HANDLER_REGISTERED, "my_handler", MyHandler)
```

---

## Singleton Pattern

### Worker State Management

All workers store global state in a thread-safe singleton:

```
┌─────────────────────────────────────────────────────────────────┐
│                    SINGLETON WORKER STATE                        │
└─────────────────────────────────────────────────────────────────┘

    Thread-Safe Global
           │
           ▼
    ┌──────────────────┐
    │   WorkerSystem   │
    │  ┌────────────┐  │
    │  │ Mutex/Lock │  │
    │  │  Inner     │  │
    │  │  State     │  │
    │  └────────────┘  │
    └──────────────────┘
           │
           ├──→ HandlerRegistry
           ├──→ EventBridge
           ├──→ EventPoller
           └──→ Configuration
```

### Singleton Classes

| Language | Singleton Implementation |
|----------|------------------------|
| Rust | `OnceLock<Mutex<WorkerSystem>>` |
| Ruby | `Singleton` module |
| Python | Class-level `_instance` with `instance()` classmethod |

### Reset for Testing

All singletons provide reset methods for test isolation:

```python
# Python
HandlerRegistry.reset_instance()
EventBridge.reset_instance()
```

```ruby
# Ruby
TaskerCore::Registry::HandlerRegistry.reset_instance!
```

---

## Observability

### Health Checks

All workers expose health information via FFI:

```python
from tasker_core import get_health_check

health = get_health_check()
# Returns: HealthCheck with component statuses
```

### Metrics

Standard metrics available from all workers:

| Metric | Description |
|--------|-------------|
| `pending_count` | Events awaiting processing |
| `in_flight_count` | Events currently being processed |
| `completed_count` | Successfully completed events |
| `failed_count` | Failed events |
| `starvation_detected` | Whether events are timing out |
| `starving_event_count` | Events exceeding timeout threshold |

### Structured Logging

All workers use structured logging with consistent fields:

```python
from tasker_core import log_info, LogContext

context = LogContext(
    correlation_id="abc-123",
    task_uuid="task-456",
    operation="process_order"
)
log_info("Processing order", context)
```

---

## Specialized Handlers

### Handler Type Hierarchy

**Ruby** (all are subclasses):
```
TaskerCore::StepHandler::Base
├── TaskerCore::StepHandler::Api        # HTTP/REST API integration
├── TaskerCore::StepHandler::Decision   # Dynamic workflow decisions (TAS-53)
└── TaskerCore::StepHandler::Batchable  # Batch processing support (TAS-88)
```

**Python** (Batchable is a mixin):
```
StepHandler (ABC)
├── ApiHandler         # HTTP/REST API integration (subclass)
├── DecisionHandler    # Dynamic workflow decisions (subclass)
└── + Batchable        # Batch processing (mixin via multiple inheritance)
```

### ApiHandler

For HTTP API integration with automatic error classification:

```python
class FetchUserHandler(ApiHandler):
    handler_name = "fetch_user"

    def call(self, context):
        response = self.get(f"/users/{context.input_data['user_id']}")
        return self.success(result=response.json())
```

### DecisionHandler

For dynamic workflow routing:

```ruby
class RoutingDecisionHandler < TaskerCore::StepHandler::Decision
  def call(context)
    amount = context.get_task_field('amount')

    if amount < 1000
      decision_success(steps: ['auto_approve'], result_data: { route: 'auto' })
    else
      decision_success(steps: ['manager_approval', 'finance_review'])
    end
  end
end
```

### Batchable

For processing large datasets in chunks. **Note**: Ruby uses subclass inheritance, Python uses mixin.

**Ruby** (subclass):
```ruby
class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
  def call(context)
    batch_ctx = get_batch_context(context)
    no_op_result = handle_no_op_worker(batch_ctx)
    return no_op_result if no_op_result

    # Process records using batch_ctx.start_cursor, batch_ctx.end_cursor
    batch_worker_complete(processed_count: batch_ctx.batch_size)
  end
end
```

**Python** (mixin):
```python
class CsvProcessorHandler(StepHandler, Batchable):
    handler_name = "csv_processor"

    def call(self, context: StepContext) -> StepHandlerResult:
        batch_ctx = self.get_batch_context(context)
        # Process records using batch_ctx.start_cursor, batch_ctx.end_cursor
        return self.batch_worker_success(processed_count=batch_ctx.batch_size)
```

---

## Checkpoint Yielding

Checkpoint yielding (TAS-125) enables batch workers to persist progress and yield control back to the orchestrator for re-dispatch. This is essential for long-running batch operations.

### When to Use

- Processing takes longer than visibility timeout
- You need resumable processing after failures
- Long-running operations need progress visibility

### Cross-Language API

All Batchable handlers provide `checkpoint_yield()` (or `checkpointYield()` in TypeScript):

**Ruby**:
```ruby
class MyBatchWorker < TaskerCore::StepHandler::Batchable
  def call(context)
    batch_ctx = get_batch_context(context)

    # Resume from checkpoint if present
    start = batch_ctx.has_checkpoint? ? batch_ctx.checkpoint_cursor : 0

    items.each_with_index do |item, idx|
      process_item(item)

      # Checkpoint every 1000 items
      if (idx + 1) % 1000 == 0
        checkpoint_yield(
          cursor: start + idx + 1,
          items_processed: idx + 1,
          accumulated_results: { partial: "data" }
        )
      end
    end

    batch_worker_complete(processed_count: items.size)
  end
end
```

**Python**:
```python
class MyBatchWorker(StepHandler, Batchable):
    def call(self, context):
        batch_ctx = self.get_batch_context(context)

        # Resume from checkpoint if present
        start = batch_ctx.checkpoint_cursor if batch_ctx.has_checkpoint() else 0

        for idx, item in enumerate(items):
            self.process_item(item)

            # Checkpoint every 1000 items
            if (idx + 1) % 1000 == 0:
                self.checkpoint_yield(
                    cursor=start + idx + 1,
                    items_processed=idx + 1,
                    accumulated_results={"partial": "data"}
                )

        return self.batch_worker_success(processed_count=len(items))
```

**TypeScript**:
```typescript
class MyBatchWorker extends BatchableHandler {
  async call(context: StepContext): Promise<StepHandlerResult> {
    const batchCtx = this.getBatchContext(context);

    // Resume from checkpoint if present
    const start = batchCtx.hasCheckpoint() ? batchCtx.checkpointCursor : 0;

    for (let idx = 0; idx < items.length; idx++) {
      await this.processItem(items[idx]);

      // Checkpoint every 1000 items
      if ((idx + 1) % 1000 === 0) {
        await this.checkpointYield({
          cursor: start + idx + 1,
          itemsProcessed: idx + 1,
          accumulatedResults: { partial: "data" }
        });
      }
    }

    return this.batchWorkerSuccess({ processedCount: items.length });
  }
}
```

### BatchWorkerContext Checkpoint Accessors

All languages provide consistent accessors for checkpoint data:

| Accessor | Ruby | Python | TypeScript |
|----------|------|--------|------------|
| Cursor position | `checkpoint_cursor` | `checkpoint_cursor` | `checkpointCursor` |
| Accumulated data | `accumulated_results` | `accumulated_results` | `accumulatedResults` |
| Has checkpoint? | `has_checkpoint?` | `has_checkpoint()` | `hasCheckpoint()` |
| Items processed | `checkpoint_items_processed` | `checkpoint_items_processed` | `checkpointItemsProcessed` |

### FFI Contract

| Function | Description |
|----------|-------------|
| `checkpoint_yield_step_event(event_id, data)` | Persist checkpoint and re-dispatch step |

### Key Invariants

1. **Checkpoint-Persist-Then-Redispatch**: Progress saved before re-dispatch
2. **Step Stays InProgress**: No state machine transitions during yield
3. **Handler-Driven**: Handlers decide when to checkpoint

See [Batch Processing Guide - Checkpoint Yielding](../guides/batch-processing.md#checkpoint-yielding-tas-125) for comprehensive documentation.

---

## Best Practices

### 1. Keep Handlers Focused

Each handler should do one thing well:
- Validate input
- Perform single operation
- Return clear result

### 2. Use Error Classification

Always specify whether errors are retryable:
```python
# Good - clear error classification
return self.failure("API rate limit", retryable=True)

# Bad - ambiguous error handling
raise Exception("API error")
```

### 3. Include Context in Errors

```python
return StepHandlerResult.failure_handler_result(
    message="Database connection failed",
    error_type="database_error",
    retryable=True,
    metadata={
        "host": "db.example.com",
        "port": 5432,
        "connection_timeout_ms": 5000
    }
)
```

### 4. Use Structured Logging

```python
log_info("Order processed", {
    "order_id": order_id,
    "total": total,
    "items_count": len(items)
})
```

### 5. Test Handler Isolation

Reset singletons between tests:
```python
def setup_method(self):
    HandlerRegistry.reset_instance()
    EventBridge.reset_instance()
```

---

## See Also

- [Worker Crates Overview](README.md) - High-level introduction
- [Rust Worker](rust.md) - Native Rust implementation
- [Ruby Worker](ruby.md) - Ruby gem documentation
- [Python Worker](python.md) - Python package documentation
- [Worker Event Systems](../worker-event-systems.md) - Detailed architecture
- [Worker Actors](../worker-actors.md) - Actor pattern documentation

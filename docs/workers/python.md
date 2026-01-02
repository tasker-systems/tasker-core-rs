# Python Worker

**Last Updated**: 2026-01-01
**Audience**: Python Developers
**Status**: Active
**Package**: `tasker_core`
**Related Docs**: [Patterns and Practices](patterns-and-practices.md) | [Worker Event Systems](../worker-event-systems.md) | [API Convergence Matrix](api-convergence-matrix.md)
**Related Tickets**: TAS-112 (Lifecycle Hooks, Mixin Pattern)

<- Back to [Worker Crates Overview](README.md)

---

The Python worker provides a package-based interface for integrating tasker-core workflow execution into Python applications. It supports both standalone server deployment and headless embedding in existing codebases.

## Quick Start

### Installation

```bash
cd workers/python
uv sync                    # Install dependencies
uv run maturin develop     # Build FFI extension
```

### Running the Server

```bash
python bin/server.py
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `TASKER_ENV` | Environment (test/development/production) | development |
| `TASKER_CONFIG_PATH` | Path to TOML configuration | Auto-detected |
| `TASKER_TEMPLATE_PATH` | Path to task templates | Auto-detected |
| `PYTHON_HANDLER_PATH` | Path for handler auto-discovery | Not set |
| `RUST_LOG` | Log level (trace/debug/info/warn/error) | info |

---

## Architecture

### Server Mode

**Location**: `workers/python/bin/server.py`

The server bootstraps the Rust foundation and manages Python handler execution:

```python
from tasker_core import (
    bootstrap_worker,
    EventBridge,
    EventPoller,
    HandlerRegistry,
    StepExecutionSubscriber,
)

# Bootstrap Rust worker foundation
result = bootstrap_worker(config)

# Start event dispatch system
event_bridge = EventBridge.instance()
event_bridge.start()

# Create step execution subscriber
handler_registry = HandlerRegistry.instance()
step_subscriber = StepExecutionSubscriber(
    event_bridge=event_bridge,
    handler_registry=handler_registry,
    worker_id="python-worker-001"
)
step_subscriber.start()

# Start event poller (10ms polling)
event_poller = EventPoller(polling_interval_ms=10)
event_poller.on_step_event(lambda e: event_bridge.publish("step_execution_received", e))
event_poller.start()

# Wait for shutdown signal
shutdown_event.wait()

# Graceful shutdown
event_poller.stop()
step_subscriber.stop()
event_bridge.stop()
stop_worker()
```

### Headless/Embedded Mode

For embedding in existing Python applications:

```python
from tasker_core import (
    bootstrap_worker,
    HandlerRegistry,
    EventBridge,
    EventPoller,
    StepExecutionSubscriber,
)
from tasker_core.types import BootstrapConfig

# Bootstrap worker (headless mode controlled via TOML: web.enabled = false)
config = BootstrapConfig(namespace="my-app")
bootstrap_worker(config)

# Register handlers
registry = HandlerRegistry.instance()
registry.register("process_data", ProcessDataHandler)

# Start event dispatch (required for embedded usage)
bridge = EventBridge.instance()
bridge.start()

subscriber = StepExecutionSubscriber(bridge, registry, "embedded-worker")
subscriber.start()

poller = EventPoller()
poller.on_step_event(lambda e: bridge.publish("step_execution_received", e))
poller.start()
```

### FFI Bridge

Python communicates with the Rust foundation via FFI polling:

```
┌────────────────────────────────────────────────────────────────┐
│                    PYTHON FFI BRIDGE                            │
└────────────────────────────────────────────────────────────────┘

   Rust Worker System
          │
          │ FFI (poll_step_events)
          ▼
   ┌─────────────────────┐
   │    EventPoller      │
   │  (daemon thread)    │──→ poll every 10ms
   └─────────────────────┘
          │
          │ publish to EventBridge
          ▼
   ┌─────────────────────┐
   │ StepExecution       │
   │ Subscriber          │──→ route to handler
   └─────────────────────┘
          │
          │ handler.call(context)
          ▼
   ┌─────────────────────┐
   │  Handler Execution  │
   └─────────────────────┘
          │
          │ FFI (complete_step_event)
          ▼
   Rust Completion Channel
```

---

## Handler Development

### Base Handler (ABC)

**Location**: `python/tasker_core/step_handler/base.py`

All handlers inherit from `StepHandler`:

```python
from tasker_core import StepHandler, StepContext, StepHandlerResult

class ProcessOrderHandler(StepHandler):
    handler_name = "process_order"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        # Access input data
        order_id = context.input_data.get("order_id")
        amount = context.input_data.get("amount")

        # Business logic
        result = self.process_order(order_id, amount)

        # Return success
        return self.success(result={
            "order_id": order_id,
            "status": "processed",
            "total": result["total"]
        })
```

### Handler Signature

```python
def call(self, context: StepContext) -> StepHandlerResult:
    # context.task_uuid       - Task identifier
    # context.step_uuid       - Step identifier
    # context.input_data      - Task context data
    # context.dependency_results - Results from parent steps
    # context.step_config     - Handler configuration
    # context.step_inputs     - Runtime inputs
    # context.retry_count     - Current retry attempt
    # context.max_retries     - Maximum retry attempts
```

### Result Methods

```python
# Success result (from base class)
return self.success(
    result={"key": "value"},
    metadata={"duration_ms": 100}
)

# Failure result (from base class)
return self.failure(
    message="Payment declined",
    error_type="payment_error",
    retryable=True,
    metadata={"card_last_four": "1234"}
)

# Or using factory methods
from tasker_core import StepHandlerResult

return StepHandlerResult.success_handler_result(
    {"key": "value"},
    {"duration_ms": 100}
)

return StepHandlerResult.failure_handler_result(
    message="Error",
    error_type="validation_error",
    retryable=False
)
```

### Accessing Dependencies

```python
def call(self, context: StepContext) -> StepHandlerResult:
    # Get result from a dependency step
    validation = context.dependency_results.get("validate_order", {})

    if validation.get("valid"):
        # Process with validated data
        return self.success(result={"processed": True})

    return self.failure("Validation failed", retryable=False)
```

---

## Composition Pattern (TAS-112)

Python handlers use composition via mixins (multiple inheritance) rather than single inheritance.

### Using Mixins (Recommended for New Code)

```python
from tasker_core.step_handler import StepHandler
from tasker_core.step_handler.mixins import APIMixin, DecisionMixin

class MyHandler(StepHandler, APIMixin, DecisionMixin):
    handler_name = "my_handler"

    def call(self, context: StepContext) -> StepHandlerResult:
        # Has both API methods (get, post, put, delete)
        # And Decision methods (decision_success, skip_branches)
        response = self.get("/api/endpoint")
        return self.decision_success(["next_step"], response)
```

### Available Mixins

| Mixin | Location | Methods Provided |
|-------|----------|------------------|
| `APIMixin` | `mixins/api.py` | `get`, `post`, `put`, `delete`, `http_client` |
| `DecisionMixin` | `mixins/decision.py` | `decision_success`, `skip_branches`, `decision_failure` |
| `BatchableMixin` | (base class) | `get_batch_context`, `handle_no_op_worker`, `create_cursor_configs` |

### Using Wrapper Classes (Backward Compatible)

The wrapper classes delegate to mixins internally:

```python
# These are equivalent:
class MyHandler(ApiHandler):
    # Inherits API methods via APIMixin internally
    pass

class MyHandler(StepHandler, APIMixin):
    # Explicit mixin inclusion
    pass
```

---

## Specialized Handlers

### API Handler

**Location**: `python/tasker_core/step_handler/api.py`

For HTTP API integration with automatic error classification:

```python
from tasker_core.step_handler import ApiHandler

class FetchUserHandler(ApiHandler):
    handler_name = "fetch_user"
    base_url = "https://api.example.com"

    def call(self, context: StepContext) -> StepHandlerResult:
        user_id = context.input_data["user_id"]

        # Automatic error classification
        response = self.get(f"/users/{user_id}")

        return self.api_success(response)
```

**HTTP Methods**:

```python
# GET request
response = self.get("/path", params={"key": "value"}, headers={})

# POST request
response = self.post("/path", data={"key": "value"}, headers={})

# PUT request
response = self.put("/path", data={"key": "value"}, headers={})

# DELETE request
response = self.delete("/path", params={}, headers={})
```

**ApiResponse Properties**:

```python
response.status_code     # HTTP status code
response.headers         # Response headers
response.body            # Parsed body (dict or str)
response.ok              # True if 2xx status
response.is_client_error # True if 4xx status
response.is_server_error # True if 5xx status
response.is_retryable    # True if should retry (408, 429, 500-504)
response.retry_after     # Retry-After header value in seconds
```

**Error Classification**:

| Status | Classification | Behavior |
|--------|---------------|----------|
| 400, 401, 403, 404, 422 | Non-retryable | Permanent failure |
| 408, 429, 500-504 | Retryable | Standard retry |

### Decision Handler

**Location**: `python/tasker_core/step_handler/decision.py`

For dynamic workflow routing:

```python
from tasker_core.step_handler import DecisionHandler
from tasker_core import DecisionPointOutcome

class RoutingDecisionHandler(DecisionHandler):
    handler_name = "routing_decision"

    def call(self, context: StepContext) -> StepHandlerResult:
        amount = context.input_data.get("amount", 0)

        if amount < 1000:
            # Auto-approve small amounts
            outcome = DecisionPointOutcome.create_steps(
                ["auto_approve"],
                routing_context={"route_type": "auto"}
            )
            return self.decision_success(outcome)

        elif amount < 5000:
            # Manager approval for medium amounts
            outcome = DecisionPointOutcome.create_steps(
                ["manager_approval"],
                routing_context={"route_type": "manager"}
            )
            return self.decision_success(outcome)

        else:
            # Dual approval for large amounts
            outcome = DecisionPointOutcome.create_steps(
                ["manager_approval", "finance_review"],
                routing_context={"route_type": "dual"}
            )
            return self.decision_success(outcome)
```

**Decision Methods**:

```python
# Create steps
outcome = DecisionPointOutcome.create_steps(
    step_names=["step1", "step2"],
    routing_context={"key": "value"}
)
return self.decision_success(outcome)

# No branches needed
outcome = DecisionPointOutcome.no_branches(reason="condition not met")
return self.decision_no_branches(outcome)
```

### Batchable Mixin

**Location**: `python/tasker_core/batch_processing/`

For processing large datasets in chunks. Both analyzer and worker handlers implement the standard `call()` method:

**Analyzer Handler** (creates batch configurations):
```python
from tasker_core import StepHandler, StepHandlerResult
from tasker_core.batch_processing import Batchable

class CsvAnalyzerHandler(StepHandler, Batchable):
    handler_name = "csv_analyzer"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Analyze CSV and create batch worker configurations."""
        csv_path = context.input_data["csv_path"]
        row_count = count_csv_rows(csv_path)

        if row_count == 0:
            # No data to process
            return self.batch_analyzer_success(
                cursor_configs=[],
                total_items=0,
                batch_metadata={"csv_path": csv_path}
            )

        # Create cursor ranges for batch workers
        cursor_configs = self.create_cursor_ranges(
            total_items=row_count,
            batch_size=100,
            max_batches=5
        )

        return self.batch_analyzer_success(
            cursor_configs=cursor_configs,
            total_items=row_count,
            worker_template_name="process_csv_batch",
            batch_metadata={"csv_path": csv_path}
        )
```

**Worker Handler** (processes a batch):
```python
class CsvBatchProcessorHandler(StepHandler, Batchable):
    handler_name = "csv_batch_processor"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process a batch of CSV rows."""
        # Get cursor config from step_inputs
        step_inputs = context.step_inputs or {}

        # Check for no-op placeholder batch
        if step_inputs.get("is_no_op"):
            return self.batch_worker_success(
                items_processed=0,
                items_succeeded=0,
                metadata={"no_op": True}
            )

        cursor = step_inputs.get("cursor", {})
        start_cursor = cursor.get("start_cursor", 0)
        end_cursor = cursor.get("end_cursor", 0)

        # Get CSV path from analyzer result
        analyzer_result = context.get_dependency_result("analyze_csv")
        csv_path = analyzer_result["batch_metadata"]["csv_path"]

        # Process the batch
        results = process_csv_batch(csv_path, start_cursor, end_cursor)

        return self.batch_worker_success(
            items_processed=results["count"],
            items_succeeded=results["success"],
            items_failed=results["failed"],
            results=results["data"],
            last_cursor=end_cursor
        )
```

**Batchable Helper Methods**:

```python
# Analyzer helpers
self.create_cursor_ranges(total_items, batch_size, max_batches)
self.batch_analyzer_success(cursor_configs, total_items, worker_template_name, batch_metadata)

# Worker helpers
self.batch_worker_success(items_processed, items_succeeded, items_failed, results, last_cursor, metadata)
self.get_batch_context(context)  # Returns BatchWorkerContext or None

# Aggregator helpers
self.aggregate_worker_results(worker_results)  # Returns aggregated counts
```

---

## Handler Registry

### Registration

**Location**: `python/tasker_core/handler.py`

```python
from tasker_core import HandlerRegistry

registry = HandlerRegistry.instance()

# Manual registration
registry.register("process_order", ProcessOrderHandler)

# Check if registered
registry.is_registered("process_order")  # True

# Resolve and instantiate
handler = registry.resolve("process_order")
result = handler.call(context)

# List all handlers
registry.list_handlers()  # ["process_order", ...]

# Handler count
registry.handler_count()  # 1
```

### Auto-Discovery

```python
# Discover handlers from a package
count = registry.discover_handlers("myapp.handlers")
print(f"Discovered {count} handlers")
```

Handlers are discovered by:
1. Scanning the package for classes inheriting from `StepHandler`
2. Using the `handler_name` class attribute for registration

---

## Type System

### Pydantic Models

Python types use Pydantic for validation:

```python
from tasker_core import StepContext, StepHandlerResult, FfiStepEvent

# StepContext - validated from FFI event
context = StepContext.from_ffi_event(event, "handler_name")
context.task_uuid      # UUID
context.step_uuid      # UUID
context.input_data     # dict
context.retry_count    # int

# StepHandlerResult - structured result
result = StepHandlerResult.success_handler_result({"key": "value"})
result.success         # True
result.result          # {"key": "value"}
result.error_message   # None
```

### Configuration Types

```python
from tasker_core.types import BootstrapConfig, CursorConfig

# Bootstrap configuration
# Note: Headless mode is controlled via TOML config (web.enabled = false)
config = BootstrapConfig(
    namespace="my-app",
    log_level="info"
)

# Cursor configuration for batch processing
cursor = CursorConfig(
    batch_size=100,
    start_cursor=0,
    end_cursor=1000
)
```

---

## Event System

### EventBridge

**Location**: `python/tasker_core/event_bridge.py`

```python
from tasker_core import EventBridge, EventNames

bridge = EventBridge.instance()

# Start the event system
bridge.start()

# Subscribe to events
def on_step_received(event):
    print(f"Processing step: {event.step_uuid}")

bridge.subscribe(EventNames.STEP_EXECUTION_RECEIVED, on_step_received)

# Publish events
bridge.publish(EventNames.HANDLER_REGISTERED, "my_handler", MyHandler)

# Stop when done
bridge.stop()
```

### Event Names

```python
from tasker_core import EventNames

EventNames.STEP_EXECUTION_RECEIVED  # Step event received from FFI
EventNames.STEP_COMPLETION_SENT     # Handler result sent to FFI
EventNames.HANDLER_REGISTERED       # Handler registered
EventNames.HANDLER_ERROR            # Handler execution error
EventNames.POLLER_METRICS           # FFI dispatch metrics update
EventNames.POLLER_ERROR             # Poller encountered an error
```

### EventPoller

**Location**: `python/tasker_core/event_poller.py`

```python
from tasker_core import EventPoller

poller = EventPoller(
    polling_interval_ms=10,       # Poll every 10ms
    starvation_check_interval=100, # Check every 1 second
    cleanup_interval=1000          # Cleanup every 10 seconds
)

# Register callbacks
poller.on_step_event(handle_step)
poller.on_metrics(handle_metrics)
poller.on_error(handle_error)

# Start polling (daemon thread)
poller.start()

# Get metrics
metrics = poller.get_metrics()
print(f"Pending: {metrics.pending_count}")

# Stop polling
poller.stop(timeout=5.0)
```

---

## Domain Events (TAS-112)

Python has full domain event support with lifecycle hooks matching Ruby and TypeScript capabilities.

**Location**: `python/tasker_core/domain_events.py`

### BasePublisher

Publishers transform step execution context into domain-specific events:

```python
from tasker_core.domain_events import BasePublisher, StepEventContext, DomainEvent

class PaymentEventPublisher(BasePublisher):
    publisher_name = "payment_events"

    def publishes_for(self) -> list[str]:
        """Which steps trigger this publisher."""
        return ["process_payment", "refund_payment"]

    async def transform_payload(self, ctx: StepEventContext) -> dict:
        """Transform step context into domain event payload."""
        return {
            "payment_id": ctx.result.get("payment_id"),
            "amount": ctx.result.get("amount"),
            "currency": ctx.result.get("currency"),
            "status": ctx.result.get("status")
        }

    # Lifecycle hooks (optional)
    async def before_publish(self, ctx: StepEventContext) -> None:
        """Called before publishing."""
        print(f"Publishing payment event for step: {ctx.step_name}")

    async def after_publish(self, ctx: StepEventContext, event: DomainEvent) -> None:
        """Called after successful publish."""
        print(f"Published event: {event.event_name}")

    async def on_publish_error(self, ctx: StepEventContext, error: Exception) -> None:
        """Called on publish failure."""
        print(f"Failed to publish: {error}")

    async def additional_metadata(self, ctx: StepEventContext) -> dict:
        """Inject custom metadata."""
        return {"payment_processor": "stripe"}
```

### BaseSubscriber

Subscribers react to domain events matching specific patterns:

```python
from tasker_core.domain_events import BaseSubscriber, InProcessDomainEvent, SubscriberResult

class AuditLoggingSubscriber(BaseSubscriber):
    subscriber_name = "audit_logger"

    def subscribes_to(self) -> list[str]:
        """Which events to handle (glob patterns supported)."""
        return ["payment.*", "order.completed"]

    async def handle(self, event: InProcessDomainEvent) -> SubscriberResult:
        """Handle matching events."""
        await self.log_to_audit_trail(event)
        return SubscriberResult(success=True)

    # Lifecycle hooks (optional)
    async def before_handle(self, event: InProcessDomainEvent) -> None:
        """Called before handling."""
        print(f"Handling: {event.event_name}")

    async def after_handle(self, event: InProcessDomainEvent, result: SubscriberResult) -> None:
        """Called after handling."""
        print(f"Handled successfully: {result.success}")

    async def on_handle_error(self, event: InProcessDomainEvent, error: Exception) -> None:
        """Called on handler failure."""
        print(f"Handler error: {error}")
```

### Registries

Manage publishers and subscribers with singleton registries:

```python
from tasker_core.domain_events import PublisherRegistry, SubscriberRegistry

# Publisher Registry
pub_registry = PublisherRegistry.instance()
pub_registry.register(PaymentEventPublisher)
pub_registry.register(OrderEventPublisher)

# Get publisher for a step
publisher = pub_registry.get_for_step("process_payment")

# Subscriber Registry
sub_registry = SubscriberRegistry.instance()
sub_registry.register(AuditLoggingSubscriber)
sub_registry.register(MetricsSubscriber)

# Start all subscribers
sub_registry.start_all()

# Stop all subscribers
sub_registry.stop_all()
```

---

## Signal Handling

The Python worker handles signals for graceful shutdown:

| Signal | Behavior |
|--------|----------|
| `SIGTERM` | Graceful shutdown |
| `SIGINT` | Graceful shutdown (Ctrl+C) |
| `SIGUSR1` | Report worker status |

```python
import signal

def handle_shutdown(signum, frame):
    print("Shutting down...")
    shutdown_event.set()

signal.signal(signal.SIGTERM, handle_shutdown)
signal.signal(signal.SIGINT, handle_shutdown)
```

---

## Error Handling

### Exception Classes

```python
from tasker_core import (
    TaskerError,              # Base class
    WorkerNotInitializedError,
    WorkerBootstrapError,
    WorkerAlreadyRunningError,
    FFIError,
    ConversionError,
    StepExecutionError,
)
```

### Using StepExecutionError

```python
from tasker_core import StepExecutionError

def call(self, context):
    # Retryable error
    raise StepExecutionError(
        "Database connection timeout",
        error_type="database_error",
        retryable=True
    )

    # Non-retryable error
    raise StepExecutionError(
        "Invalid input format",
        error_type="validation_error",
        retryable=False
    )
```

---

## Logging

### Structured Logging

```python
from tasker_core import log_info, log_error, log_warn, log_debug, LogContext

# Simple logging
log_info("Processing started")
log_error("Failed to connect")

# With context dict
log_info("Order processed", {
    "order_id": "123",
    "amount": "100.00"
})

# With LogContext model
context = LogContext(
    correlation_id="abc-123",
    task_uuid="task-456",
    operation="process_order"
)
log_info("Processing", context)
```

---

## File Structure

```
workers/python/
├── bin/
│   └── server.py              # Production server
├── python/
│   └── tasker_core/
│       ├── __init__.py        # Package exports
│       ├── handler.py         # Handler registry
│       ├── event_bridge.py    # Event pub/sub
│       ├── event_poller.py    # FFI polling
│       ├── logging.py         # Structured logging
│       ├── types.py           # Pydantic models
│       ├── step_handler/
│       │   ├── __init__.py
│       │   ├── base.py        # Base handler ABC
│       │   ├── api.py         # API handler
│       │   └── decision.py    # Decision handler
│       ├── batch_processing/
│       │   └── __init__.py    # Batchable mixin
│       └── step_execution_subscriber.py
├── src/                       # Rust/PyO3 extension
├── tests/
│   ├── test_step_handler.py
│   ├── test_module_exports.py
│   └── handlers/examples/
├── pyproject.toml
└── uv.lock
```

---

## Testing

### Unit Tests

```bash
cd workers/python
uv run pytest tests/
```

### With Coverage

```bash
uv run pytest tests/ --cov=tasker_core
```

### Type Checking

```bash
uv run mypy python/tasker_core/
```

### Linting

```bash
uv run ruff check python/
```

---

## Example Handlers

### Linear Workflow

```python
class LinearStep1Handler(StepHandler):
    handler_name = "linear_step_1"

    def call(self, context: StepContext) -> StepHandlerResult:
        return self.success(result={
            "step1_processed": True,
            "input_received": context.input_data,
            "processed_at": datetime.now().isoformat()
        })
```

### Data Processing

```python
class TransformDataHandler(StepHandler):
    handler_name = "transform_data"

    def call(self, context: StepContext) -> StepHandlerResult:
        # Get raw data from dependency
        raw_data = context.dependency_results.get("fetch_data", {})

        # Transform
        transformed = [
            {"id": item["id"], "value": item["raw_value"] * 2}
            for item in raw_data.get("items", [])
        ]

        return self.success(result={
            "items": transformed,
            "count": len(transformed)
        })
```

### Conditional Approval

```python
class ApprovalRouterHandler(DecisionHandler):
    handler_name = "approval_router"

    THRESHOLDS = {
        "auto": 1000,
        "manager": 5000
    }

    def call(self, context: StepContext) -> StepHandlerResult:
        amount = context.input_data.get("amount", 0)

        if amount < self.THRESHOLDS["auto"]:
            outcome = DecisionPointOutcome.create_steps(["auto_approve"])
        elif amount < self.THRESHOLDS["manager"]:
            outcome = DecisionPointOutcome.create_steps(["manager_approval"])
        else:
            outcome = DecisionPointOutcome.create_steps(
                ["manager_approval", "finance_review"]
            )

        return self.decision_success(outcome)
```

---

## See Also

- [Worker Crates Overview](README.md) - High-level introduction
- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Ruby Worker](ruby.md) - Ruby implementation
- [Worker Event Systems](../worker-event-systems.md) - Architecture details

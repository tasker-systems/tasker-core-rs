# Worker Crates Overview

**Last Updated**: 2025-12-17
**Audience**: Developers, Architects, Operators
**Status**: Active
**Related Docs**: [Worker Event Systems](../worker-event-systems.md) | [Worker Actors](../worker-actors.md)

<- Back to [Documentation Hub](../README.md)

---

The tasker-core workspace provides three worker implementations for executing workflow step handlers. Each implementation targets different deployment scenarios and developer ecosystems while sharing the same core Rust foundation.

## Quick Navigation

| Document | Description |
|----------|-------------|
| [Patterns and Practices](patterns-and-practices.md) | Common patterns across all workers |
| [Rust Worker](rust.md) | Native Rust implementation |
| [Ruby Worker](ruby.md) | Ruby gem for Rails integration |
| [Python Worker](python.md) | Python package for data pipelines |

---

## Overview

### Three Workers, One Foundation

All workers share the same Rust core (`tasker-worker` crate) for orchestration, queueing, and state management. The language-specific workers add handler execution in their respective runtimes.

```
┌─────────────────────────────────────────────────────────────────┐
│                     WORKER ARCHITECTURE                          │
└─────────────────────────────────────────────────────────────────┘

                    PostgreSQL + PGMQ
                          │
                          ▼
            ┌─────────────────────────────┐
            │   Rust Core (tasker-worker) │
            │   ─────────────────────────│
            │   • Queue Management        │
            │   • State Machines          │
            │   • Orchestration           │
            │   • Actor System            │
            └─────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          │               │               │
          ▼               ▼               ▼
    ┌───────────┐   ┌───────────┐   ┌───────────┐
    │   Rust    │   │   Ruby    │   │  Python   │
    │  Worker   │   │  Worker   │   │  Worker   │
    │───────────│   │───────────│   │───────────│
    │ Native    │   │ FFI Bridge│   │ FFI Bridge│
    │ Handlers  │   │ + Gem     │   │ + Package │
    └───────────┘   └───────────┘   └───────────┘
```

### Comparison Table

| Feature | Rust | Ruby | Python |
|---------|------|------|--------|
| **Performance** | Native | GVL-limited | GIL-limited |
| **Integration** | Standalone | Rails/Rack apps | Data pipelines |
| **Handler Style** | Async traits | Class-based | ABC-based |
| **Concurrency** | Tokio async | Thread + FFI poll | Thread + FFI poll |
| **Deployment** | Binary | Gem + Server | Package + Server |
| **Headless Mode** | N/A | Library embed | Library embed |

### When to Use Each

**Rust Worker** - Best for:
- Maximum throughput requirements
- Resource-constrained environments
- Standalone microservices
- Performance-critical handlers

**Ruby Worker** - Best for:
- Rails/Ruby applications
- ActiveRecord/ORM integration
- Existing Ruby codebases
- Quick prototyping with Ruby ecosystem

**Python Worker** - Best for:
- Data processing pipelines
- ML/AI integration
- Scientific computing workflows
- Python-native team preferences

---

## Deployment Modes

### Server Mode

All workers can run as standalone servers:

**Rust**:
```bash
cargo run -p workers-rust
```

**Ruby**:
```bash
cd workers/ruby
./bin/server.rb
```

**Python**:
```bash
cd workers/python
python bin/server.py
```

### Headless/Embedded Mode (Ruby & Python)

Ruby and Python workers can be embedded into existing applications without running the HTTP server. Headless mode is controlled via TOML configuration, not bootstrap parameters.

**TOML Configuration** (e.g., `config/tasker/base/worker.toml`):
```toml
[web]
enabled = false  # Disables HTTP server for headless/embedded mode
```

**Ruby (in Rails)**:
```ruby
# config/initializers/tasker.rb
require 'tasker_core'

# Bootstrap worker (web server disabled via TOML config)
TaskerCore::Worker::Bootstrap.start!

# Register handlers
TaskerCore::Registry::HandlerRegistry.instance.register_handler(
  'MyHandler',
  MyHandler
)
```

**Python (in application)**:
```python
from tasker_core import bootstrap_worker, HandlerRegistry
from tasker_core.types import BootstrapConfig

# Bootstrap worker (web server disabled via TOML config)
config = BootstrapConfig(namespace="my-app")
bootstrap_worker(config)

# Register handlers
registry = HandlerRegistry.instance()
registry.register("my_handler", MyHandler)
```

---

## Core Concepts

### 1. Handler Registration

All workers use a registry pattern for handler discovery:

```
                    ┌─────────────────────┐
                    │  HandlerRegistry    │
                    │  (Singleton)        │
                    └─────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
              ▼               ▼               ▼
         ┌─────────┐    ┌─────────┐    ┌─────────┐
         │Handler A│    │Handler B│    │Handler C│
         └─────────┘    └─────────┘    └─────────┘
```

### 2. Event Flow

Step events flow through a consistent pipeline:

```
1. PGMQ Queue → Event received
2. Worker claims step (atomic)
3. Handler resolved by name
4. Handler.call(context) executed
5. Result sent to completion channel
6. Orchestration receives result
```

### 3. Error Classification

All workers distinguish between:
- **Retryable Errors**: Transient failures → Re-enqueue step
- **Permanent Errors**: Unrecoverable → Mark step failed

### 4. Graceful Shutdown

All workers handle shutdown signals (SIGTERM, SIGINT):

```
1. Signal received
2. Stop accepting new work
3. Complete in-flight handlers
4. Flush completion channel
5. Shutdown Rust foundation
6. Exit cleanly
```

---

## Configuration

### Environment Variables

Common across all workers:

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `TASKER_ENV` | Environment (test/development/production) | development |
| `TASKER_CONFIG_PATH` | Path to TOML configuration | Auto-detected |
| `TASKER_TEMPLATE_PATH` | Path to task templates | Auto-detected |
| `TASKER_NAMESPACE` | Worker namespace for queue isolation | default |
| `RUST_LOG` | Log level (trace/debug/info/warn/error) | info |

### Language-Specific

**Ruby**:
| Variable | Description |
|----------|-------------|
| `RUBY_GC_HEAP_GROWTH_FACTOR` | GC tuning for production |

**Python**:
| Variable | Description |
|----------|-------------|
| `PYTHON_HANDLER_PATH` | Path for handler auto-discovery |

---

## Handler Types

All workers support specialized handler types:

### StepHandler (Base)

Basic step execution:
```python
class MyHandler(StepHandler):
    handler_name = "my_handler"

    def call(self, context):
        return self.success({"result": "done"})
```

### ApiHandler

HTTP/REST API integration with automatic error classification:
```ruby
class FetchDataHandler < TaskerCore::StepHandler::Api
  def call(context)
    user_id = context.get_task_field('user_id')
    response = connection.get("/users/#{user_id}")
    process_response(response)
    success(result: response.body)
  end
end
```

### DecisionHandler

Dynamic workflow routing:
```python
class RouteHandler(DecisionHandler):
    handler_name = "route_handler"

    def call(self, context):
        if context.input_data["amount"] < 1000:
            return self.route_to_steps(["auto_approve"])
        return self.route_to_steps(["manager_approval"])
```

### Batchable

Large dataset processing. Note: Ruby uses subclass inheritance, Python uses mixin:

**Ruby** (subclass of Base):
```ruby
class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
  def call(context)
    batch_ctx = get_batch_context(context)
    no_op_result = handle_no_op_worker(batch_ctx)
    return no_op_result if no_op_result

    # Process batch using batch_ctx.start_cursor, batch_ctx.end_cursor
    batch_worker_complete(processed_count: batch_ctx.batch_size)
  end
end
```

**Python** (mixin):
```python
class CsvBatchProcessor(StepHandler, Batchable):
    handler_name = "csv_batch_processor"

    def call(self, context: StepContext) -> StepHandlerResult:
        batch_ctx = self.get_batch_context(context)
        if batch_ctx is None:
            return self.failure(message="No batch context", error_type="missing_context")
        # Process batch using batch_ctx.start_cursor, batch_ctx.end_cursor
        batch_size = batch_ctx.cursor_config.end_cursor - batch_ctx.cursor_config.start_cursor
        return self.batch_worker_success(items_processed=batch_size)
```

---

## Quick Start

### Rust

```bash
# Build and run
cd workers/rust
cargo run

# With custom configuration
TASKER_CONFIG_PATH=/path/to/config.toml cargo run
```

### Ruby

```bash
# Install dependencies
cd workers/ruby
bundle install
bundle exec rake compile

# Run server
./bin/server.rb
```

### Python

```bash
# Install dependencies
cd workers/python
uv sync
uv run maturin develop

# Run server
python bin/server.py
```

---

## Monitoring

### Health Checks

All workers expose health status:

```python
# Python
from tasker_core import get_health_check
health = get_health_check()
```

```ruby
# Ruby
health = TaskerCore::FFI.health_check
```

### Metrics

Common metrics available:

| Metric | Description |
|--------|-------------|
| `pending_count` | Events awaiting processing |
| `in_flight_count` | Events being processed |
| `completed_count` | Successfully completed |
| `failed_count` | Failed events |
| `starvation_detected` | Processing bottleneck |

### Logging

All workers use structured logging:

```
2025-01-15T10:30:00Z [INFO] python-worker: Processing step step_uuid=abc-123 handler=process_order
2025-01-15T10:30:01Z [INFO] python-worker: Step completed step_uuid=abc-123 success=true duration_ms=150
```

---

## Architecture Deep Dive

For detailed architectural documentation:

- **[Worker Event Systems](../worker-event-systems.md)** - Dual-channel architecture, event-driven processing
- **[Worker Actors](../worker-actors.md)** - Actor-based coordination, message handling
- **[Events and Commands](../events-and-commands.md)** - Event definitions, command routing

---

## See Also

- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Rust Worker](rust.md) - Native implementation details
- [Ruby Worker](ruby.md) - Ruby gem documentation
- [Python Worker](python.md) - Python package documentation

# Ruby Worker

**Last Updated**: 2026-01-01
**Audience**: Ruby Developers
**Status**: Active
**Package**: `tasker_core` (gem)
**Related Docs**: [Patterns and Practices](patterns-and-practices.md) | [Worker Event Systems](../worker-event-systems.md) | [API Convergence Matrix](api-convergence-matrix.md)
**Related Tickets**: TAS-112 (Mixin Pattern, 0-indexed Cursors)

<- Back to [Worker Crates Overview](README.md)

---

The Ruby worker provides a gem-based interface for integrating tasker-core workflow execution into Ruby applications. It supports both standalone server deployment and headless embedding in Rails applications.

## Quick Start

### Installation

```bash
cd workers/ruby
bundle install
bundle exec rake compile  # Compile FFI extension
```

### Running the Server

```bash
./bin/server.rb
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `TASKER_ENV` | Environment (test/development/production) | development |
| `TASKER_CONFIG_PATH` | Path to TOML configuration | Auto-detected |
| `TASKER_TEMPLATE_PATH` | Path to task templates | Auto-detected |
| `RUBY_GC_HEAP_GROWTH_FACTOR` | GC tuning for production | Ruby default |

---

## Architecture

### Server Mode

**Location**: `workers/ruby/bin/server.rb`

The server bootstraps the Rust foundation and manages Ruby handler execution:

```ruby
# Bootstrap the worker system
bootstrap = TaskerCore::Worker::Bootstrap.start!

# Signal handlers for graceful shutdown
Signal.trap('TERM') { shutdown_event.set }
Signal.trap('INT') { shutdown_event.set }

# Main loop with health checks
loop do
  break if shutdown_event.set?
  sleep(1)
end

# Graceful shutdown
bootstrap.shutdown!
```

### Headless/Embedded Mode

For embedding in Rails applications without an HTTP server:

```ruby
# config/initializers/tasker.rb
require 'tasker_core'

# Bootstrap worker (headless mode controlled via TOML: web.enabled = false)
TaskerCore::Worker::Bootstrap.start!

# Register application handlers
TaskerCore::Registry::HandlerRegistry.instance.register_handler(
  'ProcessOrderHandler',
  ProcessOrderHandler
)
```

### FFI Bridge

Ruby communicates with the Rust foundation via FFI polling:

```
┌────────────────────────────────────────────────────────────────┐
│                    RUBY FFI BRIDGE                              │
└────────────────────────────────────────────────────────────────┘

   Rust Worker System
          │
          │ FFI (poll_step_events)
          ▼
   ┌─────────────┐
   │   Ruby      │
   │   Thread    │──→ poll every 10ms
   └─────────────┘
          │
          ▼
   ┌─────────────┐
   │  Handler    │
   │  Execution  │──→ handler.call(context)
   └─────────────┘
          │
          │ FFI (complete_step_event)
          ▼
   Rust Completion Channel
```

---

## Handler Development

### Base Handler

**Location**: `lib/tasker_core/step_handler/base.rb`

All handlers inherit from `TaskerCore::StepHandler::Base`:

```ruby
class ProcessOrderHandler < TaskerCore::StepHandler::Base
  def call(context)
    # Access task context via cross-language standard methods
    order_id = context.get_task_field('order_id')
    amount = context.get_task_field('amount')

    # Business logic
    result = process_order(order_id, amount)

    # Return success result
    success(result: {
      order_id: order_id,
      status: 'processed',
      total: result[:total]
    })
  end
end
```

### Handler Signature

```ruby
def call(context)
  # context - StepContext with cross-language standard fields:
  #   context.task_uuid       - Task UUID
  #   context.step_uuid       - Step UUID
  #   context.input_data      - Step inputs from workflow_step.inputs
  #   context.step_config     - Handler config from step_definition
  #   context.retry_count     - Current retry attempt
  #   context.max_retries     - Maximum retry attempts
  #   context.get_task_field('field')       - Get field from task context
  #   context.get_dependency_result('step') - Get result from parent step
end
```

### Result Methods

```ruby
# Success result (keyword arguments required)
success(
  result: { key: 'value' },
  metadata: { duration_ms: 100 }
)

# Failure result
# error_type must be one of: 'PermanentError', 'RetryableError',
# 'ValidationError', 'UnexpectedError', 'StepCompletionError'
failure(
  message: 'Payment declined',
  error_type: 'PermanentError',   # Use enum value, not freeform string
  error_code: 'PAYMENT_DECLINED', # Optional freeform error code
  retryable: false,
  metadata: { card_last_four: '1234' }
)
```

### Accessing Dependencies

```ruby
def call(context)
  # Get result from a dependency step
  validation_result = context.get_dependency_result('validate_order')

  if validation_result && validation_result['valid']
    # Process with validated data
  end
end
```

---

## Composition Pattern (TAS-112)

Ruby handlers use composition via mixins rather than inheritance. You can use either:
1. **Wrapper classes** (Api, Decision, Batchable) - simpler, backward compatible
2. **Mixin modules** (Mixins::API, Mixins::Decision, Mixins::Batchable) - explicit composition

### Using Mixins (Recommended for New Code)

```ruby
class MyHandler < TaskerCore::StepHandler::Base
  include TaskerCore::StepHandler::Mixins::API
  include TaskerCore::StepHandler::Mixins::Decision

  def call(context)
    # Has both API methods (get, post, put, delete)
    # And Decision methods (decision_success, decision_no_branches)
    response = get('/api/endpoint')
    decision_success(steps: ['next_step'], result_data: response)
  end
end
```

### Available Mixins

| Mixin | Location | Methods Provided |
|-------|----------|------------------|
| `Mixins::API` | `mixins/api.rb` | `get`, `post`, `put`, `delete`, `connection` |
| `Mixins::Decision` | `mixins/decision.rb` | `decision_success`, `decision_no_branches`, `skip_branches` |
| `Mixins::Batchable` | `mixins/batchable.rb` | `get_batch_context`, `handle_no_op_worker`, `create_cursor_configs` |

### Using Wrapper Classes (Backward Compatible)

The wrapper classes delegate to mixins internally:

```ruby
# These are equivalent:
class MyHandler < TaskerCore::StepHandler::Api
  # Inherits API methods via Mixins::API
end

class MyHandler < TaskerCore::StepHandler::Base
  include TaskerCore::StepHandler::Mixins::API
  # Explicit mixin inclusion
end
```

---

## Specialized Handlers

### API Handler

**Location**: `lib/tasker_core/step_handler/api.rb`

For HTTP API integration with automatic error classification:

```ruby
class FetchUserHandler < TaskerCore::StepHandler::Api
  def call(context)
    user_id = context.get_task_field('user_id')

    # Automatic error classification (429 → retryable, 404 → permanent)
    response = connection.get("/users/#{user_id}")
    process_response(response)  # Raises on errors, returns response on success

    # Return success result with response data
    success(result: response.body)
  end

  # Optional: Custom connection configuration
  def configure_connection
    Faraday.new(base_url) do |conn|
      conn.request :json
      conn.response :json
      conn.options.timeout = 30
    end
  end
end
```

**HTTP Methods Available**:
- `get(path, params: {}, headers: {})`
- `post(path, data: {}, headers: {})`
- `put(path, data: {}, headers: {})`
- `delete(path, params: {}, headers: {})`

**Error Classification**:

| Status | Classification | Behavior |
|--------|---------------|----------|
| 400, 401, 403, 404, 422 | Permanent | No retry |
| 429 | Retryable | Respect Retry-After |
| 500-599 | Retryable | Standard backoff |

### Decision Handler

**Location**: `lib/tasker_core/step_handler/decision.rb`

For dynamic workflow routing (TAS-53):

```ruby
class RoutingDecisionHandler < TaskerCore::StepHandler::Decision
  def call(context)
    amount = context.get_task_field('amount')

    if amount < 1000
      # Auto-approve small amounts
      decision_success(
        steps: ['auto_approve'],
        result_data: { route_type: 'auto', amount: amount }
      )
    elsif amount < 5000
      # Manager approval for medium amounts
      decision_success(
        steps: ['manager_approval'],
        result_data: { route_type: 'manager', amount: amount }
      )
    else
      # Dual approval for large amounts
      decision_success(
        steps: ['manager_approval', 'finance_review'],
        result_data: { route_type: 'dual', amount: amount }
      )
    end
  end
end
```

**Decision Methods**:
- `decision_success(steps:, result_data: {})` - Create steps dynamically
- `decision_no_branches(result_data: {})` - Skip conditional steps

### Batchable Handler

**Location**: `lib/tasker_core/step_handler/batchable.rb`

For processing large datasets in chunks (TAS-59, TAS-112):

**⚠️ TAS-112 Breaking Change**: Cursors are now **0-indexed** (previously 1-indexed) to match Python, TypeScript, and Rust.

```ruby
class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
  def call(context)
    # Extract batch context from step inputs
    batch_ctx = get_batch_context(context)

    # Handle no-op placeholder batches
    no_op_result = handle_no_op_worker(batch_ctx)
    return no_op_result if no_op_result

    # Process this batch
    csv_file = context.get_dependency_result('analyze_csv')&.dig('csv_file_path')
    records = read_csv_batch(csv_file, batch_ctx.start_cursor, batch_ctx.batch_size)

    processed = records.map { |record| transform_record(record) }

    # Return batch completion
    batch_worker_complete(
      processed_count: processed.size,
      result_data: { records: processed }
    )
  end
end
```

**Batch Helper Methods**:
- `get_batch_context(context)` - Get batch boundaries from StepContext
- `handle_no_op_worker(batch_ctx)` - Handle placeholder batches
- `batch_worker_complete(processed_count:, result_data:)` - Complete batch
- `create_cursor_configs(total_items, worker_count)` - Create 0-indexed cursor ranges

**Cursor Indexing (TAS-112)**:

```ruby
# Creates 0-indexed cursor ranges
configs = create_cursor_configs(1000, 5)
# => [
#   { batch_id: '1', start_cursor: 0, end_cursor: 200 },
#   { batch_id: '2', start_cursor: 200, end_cursor: 400 },
#   { batch_id: '3', start_cursor: 400, end_cursor: 600 },
#   { batch_id: '4', start_cursor: 600, end_cursor: 800 },
#   { batch_id: '5', start_cursor: 800, end_cursor: 1000 }
# ]
```

---

## Handler Registry

### Registration

**Location**: `lib/tasker_core/registry/handler_registry.rb`

```ruby
registry = TaskerCore::Registry::HandlerRegistry.instance

# Manual registration
registry.register_handler('ProcessOrderHandler', ProcessOrderHandler)

# Check availability
registry.handler_available?('ProcessOrderHandler')  # => true

# List all handlers
registry.registered_handlers  # => ["ProcessOrderHandler", ...]
```

### Discovery Modes

1. **Preloaded Handlers** (Test environment)
   - ObjectSpace scanning for loaded handler classes

2. **Template-Driven Discovery**
   - YAML templates define handler references
   - Handlers loaded from configured paths

### Handler Search Paths

```
app/handlers/
lib/handlers/
handlers/
app/tasker/handlers/
lib/tasker/handlers/
spec/handlers/examples/  (test environment)
```

---

## Configuration

### Bootstrap Configuration

Bootstrap configuration is controlled via TOML files, not Ruby parameters:

```toml
# config/tasker/base/worker.toml
[web]
enabled = true              # Set to false for headless/embedded mode
bind_address = "0.0.0.0"
port = 8080
```

```ruby
# Ruby bootstrap is simple - config comes from TOML
TaskerCore::Worker::Bootstrap.start!
```

### Handler Configuration

```ruby
class MyHandler < TaskerCore::StepHandler::Base
  def initialize(config: {})
    super
    @timeout = config[:timeout] || 30
    @max_retries = config[:retries] || 3
  end

  def config_schema
    {
      type: 'object',
      properties: {
        timeout: { type: 'integer', minimum: 1, default: 30 },
        retries: { type: 'integer', minimum: 0, default: 3 }
      }
    }
  end
end
```

---

## Signal Handling

The Ruby worker handles multiple signals:

| Signal | Behavior |
|--------|----------|
| `SIGTERM` | Graceful shutdown |
| `SIGINT` | Graceful shutdown (Ctrl+C) |
| `SIGUSR1` | Report worker status |
| `SIGUSR2` | Reload configuration |

```ruby
# Status reporting
Signal.trap('USR1') do
  logger.info "Worker Status: #{bootstrap.status.inspect}"
end

# Configuration reload
Signal.trap('USR2') do
  bootstrap.reload_config
end
```

---

## Error Handling

### Exception Classes

```ruby
TaskerCore::Errors::Error                  # Base class
├── TaskerCore::Errors::ConfigurationError # Configuration issues
├── TaskerCore::Errors::FFIError           # FFI bridge errors
├── TaskerCore::Errors::ProceduralError    # Base for workflow errors
│   ├── TaskerCore::Errors::RetryableError # Transient failures
│   ├── TaskerCore::Errors::PermanentError # Unrecoverable failures
│   │   ├── TaskerCore::Errors::ValidationError # Validation failures
│   │   └── TaskerCore::Errors::NotFoundError   # Resource not found
│   ├── TaskerCore::Errors::TimeoutError   # Timeout failures
│   └── TaskerCore::Errors::NetworkError   # Network failures
└── TaskerCore::Errors::ServerError        # Embedded server errors
```

### Raising Errors

```ruby
def call(context)
  # Retryable error (will be retried)
  raise TaskerCore::Errors::RetryableError.new(
    'Database connection timeout',
    retry_after: 5,
    context: { service: 'database' }
  )

  # Permanent error (no retry)
  raise TaskerCore::Errors::PermanentError.new(
    'Invalid order format',
    error_code: 'INVALID_ORDER',
    context: { field: 'order_id' }
  )

  # Validation error (permanent, with field info)
  raise TaskerCore::Errors::ValidationError.new(
    'Email format is invalid',
    field: 'email',
    error_code: 'INVALID_EMAIL'
  )
end
```

---

## Logging

### Structured Logging (Recommended)

New code should use `TaskerCore::Tracing` for unified structured logging via FFI:

```ruby
# Recommended: Use Tracing directly
TaskerCore::Tracing.info('Processing order', {
  order_id: order.id,
  amount: order.total,
  customer_id: order.customer_id
})

TaskerCore::Tracing.error('Payment failed', {
  error_code: 'DECLINED',
  card_last_four: '1234'
})
```

### Legacy Logger (Deprecated)

**Note**: `TaskerCore::Logger` is maintained for backward compatibility but delegates to `TaskerCore::Tracing`. New code should use Tracing directly.

```ruby
# Legacy (still works, but deprecated)
logger = TaskerCore::Logger.instance
logger.info('Processing order', {
  order_id: order.id,
  amount: order.total
})
```

### Log Levels

Controlled via `RUST_LOG` environment variable:
- `trace` - Very detailed debugging
- `debug` - Debugging information
- `info` - Normal operation
- `warn` - Warning conditions
- `error` - Error conditions

---

## File Structure

```
workers/ruby/
├── bin/
│   ├── server.rb            # Production server
│   └── health_check.rb      # Health check script
├── ext/
│   └── tasker_core/
│       └── extconf.rb       # FFI extension config
├── lib/
│   └── tasker_core/
│       ├── errors.rb        # Exception classes
│       ├── handlers.rb      # Handler namespace
│       ├── internal.rb      # Internal modules
│       ├── logger.rb        # Logging
│       ├── models.rb        # Type models
│       ├── registry/
│       │   ├── handler_registry.rb
│       │   └── step_handler_resolver.rb
│       ├── step_handler/
│       │   ├── base.rb      # Base handler
│       │   ├── api.rb       # API handler
│       │   ├── decision.rb  # Decision handler
│       │   └── batchable.rb # Batch handler
│       ├── task_handler/
│       │   └── base.rb      # Task orchestration
│       ├── types/           # Type definitions
│       └── version.rb
├── spec/
│   ├── handlers/examples/   # Example handlers
│   └── integration/         # Integration tests
├── Gemfile
└── tasker_core.gemspec
```

---

## Testing

### Unit Tests

```bash
cd workers/ruby
bundle exec rspec spec/
```

### Integration Tests

```bash
DATABASE_URL=postgresql://... bundle exec rspec spec/integration/
```

### E2E Tests (from project root)

```bash
DATABASE_URL=postgresql://... \
TASKER_ENV=test \
bundle exec rspec spec/handlers/
```

---

## Example Handlers

### Linear Workflow

```ruby
# spec/handlers/examples/linear_workflow/step_handlers/linear_step_1_handler.rb
module LinearWorkflow
  module StepHandlers
    class LinearStep1Handler < TaskerCore::StepHandler::Base
      def call(context)
        input = context.context  # Full task context
        success(result: {
          step1_processed: true,
          input_received: input,
          processed_at: Time.now.iso8601
        })
      end
    end
  end
end
```

### Order Fulfillment

```ruby
class ValidateOrderHandler < TaskerCore::StepHandler::Base
  def call(context)
    order = context.context  # Full task context

    unless order['items']&.any?
      return failure(
        message: 'Order must have at least one item',
        error_type: 'ValidationError',
        error_code: 'EMPTY_ORDER',
        retryable: false
      )
    end

    success(result: {
      valid: true,
      item_count: order['items'].size,
      total: calculate_total(order['items'])
    })
  end
end
```

### Conditional Approval

```ruby
class RoutingDecisionHandler < TaskerCore::StepHandler::Decision
  THRESHOLDS = {
    auto_approve: 1000,
    manager_only: 5000
  }.freeze

  def call(context)
    amount = context.get_task_field('amount').to_f

    if amount < THRESHOLDS[:auto_approve]
      decision_success(steps: ['auto_approve'])
    elsif amount < THRESHOLDS[:manager_only]
      decision_success(steps: ['manager_approval'])
    else
      decision_success(steps: ['manager_approval', 'finance_review'])
    end
  end
end
```

---

## See Also

- [Worker Crates Overview](README.md) - High-level introduction
- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Python Worker](python.md) - Python implementation
- [Worker Event Systems](../worker-event-systems.md) - Architecture details

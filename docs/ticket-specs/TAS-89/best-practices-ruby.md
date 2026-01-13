# Ruby Best Practices for Tasker Core

**Purpose**: Codify Ruby-specific coding standards for the tasker-core workers/ruby project.

---

## Code Style

### Formatting
- Use RuboCop (enforced via `cargo make check-ruby`)
- 2-space indentation
- Maximum line length: 120 characters
- Use double quotes for strings with interpolation, single quotes otherwise

### Naming Conventions
```ruby
# Classes/Modules: PascalCase
class StepHandler
  module Mixins
    module API
    end
  end
end

# Methods/variables: snake_case
def process_step(context)
  step_result = execute_handler(context)
end

# Constants: SCREAMING_SNAKE_CASE
DEFAULT_TIMEOUT = 30
MAX_RETRIES = 3

# Predicates: end with ?
def ready?
  dependencies_met?
end

# Dangerous methods: end with !
def reset!
  @state = :pending
end
```

### Module Organization
```ruby
# lib/tasker_core/step_handler/base.rb
module TaskerCore
  module StepHandler
    class Base
      # 1. Includes/extends
      include Mixins::ResultFactory

      # 2. Constants
      DEFAULT_TIMEOUT = 30

      # 3. Class methods
      def self.handler_name
        # ...
      end

      # 4. Instance methods (public)
      def call(context)
        # ...
      end

      # 5. Protected methods
      protected

      def validate_context(context)
        # ...
      end

      # 6. Private methods
      private

      def internal_process
        # ...
      end
    end
  end
end
```

---

## Handler Patterns

### Base Handler Structure
```ruby
module TaskerCore
  module StepHandler
    class MyHandler < Base
      # Include capabilities via mixins (TAS-112 composition pattern)
      include Mixins::API
      include Mixins::Decision

      def call(context)
        # Access context data
        input = context.input_data
        config = context.step_config

        # Execute handler logic
        result = perform_work(input)

        # Return result using factory methods
        success(result: result, metadata: { processed_at: Time.now.iso8601 })
      rescue StandardError => e
        failure(
          message: e.message,
          error_type: 'UnexpectedError',
          retryable: true
        )
      end

      private

      def perform_work(input)
        # Implementation
      end
    end
  end
end
```

### Result Factory Methods
```ruby
# Success result
success(result: { id: 123 }, metadata: { duration_ms: 50 })

# Failure result
failure(
  message: 'API request failed',
  error_type: 'RetryableError',
  error_code: 'API_TIMEOUT',
  retryable: true,
  metadata: { attempted_at: Time.now.iso8601 }
)

# Decision result (for decision handlers)
decision_success(
  steps: ['process_order', 'send_notification'],
  routing_context: { decision_type: 'conditional' }
)

# Skip branches
skip_branches(
  reason: 'No items to process',
  routing_context: { skip_type: 'empty_input' }
)
```

---

## Mixin Pattern (TAS-112)

### Using Mixins
```ruby
class OrderHandler < TaskerCore::StepHandler::Base
  # Composition over inheritance
  include TaskerCore::StepHandler::Mixins::API
  include TaskerCore::StepHandler::Mixins::Decision

  def call(context)
    # API mixin provides: get, post, put, delete
    response = get('/api/orders', params: { id: context.input_data['order_id'] })

    # Process response
    if response.success?
      success(result: response.body)
    else
      failure(message: response.error, error_type: 'APIError')
    end
  end
end
```

### Available Mixins
| Mixin | Purpose | Methods Provided |
|-------|---------|------------------|
| `Mixins::API` | HTTP requests | `get`, `post`, `put`, `delete` |
| `Mixins::Decision` | Decision points | `decision_success`, `skip_branches`, `decision_failure` |
| `Mixins::Batchable` | Batch processing | `get_batch_context`, `batch_worker_complete`, `handle_no_op_worker` |

---

## Error Handling

### Use Specific Exception Classes
```ruby
module TaskerCore
  module Errors
    class HandlerError < StandardError; end
    class ValidationError < HandlerError; end
    class ConfigurationError < HandlerError; end
    class TimeoutError < HandlerError; end
  end
end

# Usage
def validate_input(data)
  raise TaskerCore::Errors::ValidationError, 'Missing required field: id' unless data['id']
end
```

### Rescue Specific Exceptions
```ruby
# BAD: Catching all exceptions
def call(context)
  process(context)
rescue Exception => e  # Too broad!
  failure(message: e.message)
end

# GOOD: Catch specific exceptions
def call(context)
  process(context)
rescue TaskerCore::Errors::ValidationError => e
  failure(message: e.message, error_type: 'ValidationError', retryable: false)
rescue Net::OpenTimeout, Net::ReadTimeout => e
  failure(message: e.message, error_type: 'TimeoutError', retryable: true)
rescue StandardError => e
  failure(message: e.message, error_type: 'UnexpectedError', retryable: true)
end
```

---

## Documentation

### YARD Documentation
```ruby
# Handler class documentation
#
# Processes order fulfillment by coordinating inventory
# reservation and shipping label generation.
#
# @example Basic usage
#   handler = OrderFulfillmentHandler.new
#   result = handler.call(context)
#
# @see TaskerCore::StepHandler::Base
class OrderFulfillmentHandler < TaskerCore::StepHandler::Base
  # Process the order fulfillment step.
  #
  # @param context [TaskerCore::StepContext] The execution context
  # @return [TaskerCore::StepHandlerResult] Success or failure result
  # @raise [TaskerCore::Errors::ValidationError] if order_id is missing
  def call(context)
    # ...
  end

  private

  # Reserves inventory for the order items.
  #
  # @param items [Array<Hash>] List of items with :sku and :quantity
  # @return [Hash] Reservation confirmation with :reservation_id
  def reserve_inventory(items)
    # ...
  end
end
```

---

## Testing

### RSpec Patterns
```ruby
# spec/step_handlers/order_handler_spec.rb
RSpec.describe TaskerCore::StepHandler::OrderHandler do
  subject(:handler) { described_class.new }

  let(:context) { build_step_context(input_data: input_data) }
  let(:input_data) { { 'order_id' => '12345' } }

  describe '#call' do
    context 'when order exists' do
      before do
        stub_api_request(:get, '/api/orders/12345')
          .to_return(status: 200, body: { id: '12345', status: 'pending' }.to_json)
      end

      it 'returns success with order data' do
        result = handler.call(context)

        expect(result).to be_success
        expect(result.result['id']).to eq('12345')
      end
    end

    context 'when order not found' do
      before do
        stub_api_request(:get, '/api/orders/12345')
          .to_return(status: 404)
      end

      it 'returns failure with appropriate error' do
        result = handler.call(context)

        expect(result).to be_failure
        expect(result.error_type).to eq('NotFoundError')
        expect(result.retryable).to be false
      end
    end
  end
end
```

### Test Helpers
```ruby
# spec/support/step_context_helper.rb
module StepContextHelper
  def build_step_context(input_data: {}, step_config: {}, dependency_results: {})
    TaskerCore::StepContext.new(
      task_uuid: SecureRandom.uuid,
      step_uuid: SecureRandom.uuid,
      input_data: input_data,
      step_config: step_config,
      dependency_results: dependency_results
    )
  end
end

RSpec.configure do |config|
  config.include StepContextHelper
end
```

---

## FFI Considerations

### Working with Rust Extensions
```ruby
# The native extension is loaded automatically
require 'tasker_core/native'

# FFI types are converted automatically
# Ruby Hash <-> Rust HashMap
# Ruby Array <-> Rust Vec
# Ruby String <-> Rust String

# Be aware of type boundaries
context.input_data  # Already converted from Rust
```

### Memory Safety
```ruby
# Don't hold references to FFI objects longer than needed
def process(context)
  # Extract needed data immediately
  order_id = context.input_data['order_id']

  # Don't store context for later use
  # @context = context  # BAD: May cause memory issues

  process_order(order_id)
end
```

---

## Project-Specific Patterns

### Registry Usage
```ruby
# Register handlers
TaskerCore::Registry.instance.register('order_handler', OrderHandler)

# Check availability
TaskerCore::Registry.instance.is_registered('order_handler')

# Resolve handler
handler = TaskerCore::Registry.instance.resolve('order_handler')
```

### Domain Events
```ruby
class OrderCompletedPublisher < TaskerCore::DomainEvents::BasePublisher
  def subscribes_to
    'order.completed'
  end

  def publish(ctx)
    # ctx contains task_uuid, step_uuid, result, metadata
    notify_downstream_systems(ctx)
  end

  # Lifecycle hooks (TAS-112)
  def before_publish(ctx)
    Rails.logger.info("Publishing order completed event")
  end

  def after_publish(ctx, result)
    metrics.increment('order.completed.published')
  end
end
```

---

## References

- [Ruby Style Guide](https://rubystyle.guide/)
- [API Convergence Matrix](../../workers/api-convergence-matrix.md)
- [Ruby Worker Documentation](../../workers/ruby.md)
- [Composition Over Inheritance](../../principles/composition-over-inheritance.md)

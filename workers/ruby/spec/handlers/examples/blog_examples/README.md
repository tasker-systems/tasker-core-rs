# Blog Example Handlers

This directory contains Ruby handler implementations for blog examples, demonstrating real-world workflow patterns migrated from the Rails engine.

## Structure

```
workers/ruby/spec/handlers/examples/blog_examples/
├── post_01_ecommerce/
│   ├── handlers/
│   │   └── order_processing_handler.rb      # Task handler
│   ├── step_handlers/
│   │   ├── validate_cart_handler.rb         # Step 1: Cart validation
│   │   ├── process_payment_handler.rb       # Step 2: Payment processing
│   │   ├── update_inventory_handler.rb      # Step 3: Inventory management
│   │   ├── create_order_handler.rb          # Step 4: Order creation
│   │   └── send_confirmation_handler.rb     # Step 5: Email confirmation
│   └── config/
│       └── order_processing_handler.yaml    # Workflow configuration
│
├── post_02_data_pipeline/
│   ├── handlers/
│   │   └── customer_analytics_handler.rb
│   ├── step_handlers/
│   │   ├── extract_orders_handler.rb
│   │   ├── extract_users_handler.rb
│   │   ├── extract_products_handler.rb
│   │   ├── transform_customer_metrics_handler.rb
│   │   ├── transform_product_metrics_handler.rb
│   │   ├── generate_insights_handler.rb
│   │   ├── update_dashboard_handler.rb
│   │   └── send_notifications_handler.rb
│   └── config/
│       └── customer_analytics_handler.yaml
│
├── post_03_microservices/
├── post_04_team_scaling/
└── post_05_observability/
```

## Handler Patterns

### Task Handler Pattern

Task handlers coordinate workflow execution and can apply custom configuration:

```ruby
module Ecommerce
  class OrderProcessingHandler < TaskerCore::TaskHandler::Base
    # Override initialize_task to apply custom configuration
    def initialize_task(task_request)
      setup_task_specific_configuration(task_request[:context])
      super
    end

    def setup_task_specific_configuration(context)
      # Apply premium customer optimizations
      if context['customer_info']['tier'] == 'premium'
        context['processing_priority'] = 'high'
        context['expedited_shipping'] = true
      end
    end

    # Provide metadata for monitoring
    def metadata
      super.merge(
        workflow_type: 'order_processing',
        supports_premium_tier: true
      )
    end
  end
end
```

### Step Handler Pattern

Step handlers implement business logic for individual workflow steps:

```ruby
module Ecommerce
  module StepHandlers
    class ProcessPaymentHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Extract inputs from prior steps
        inputs = extract_and_validate_inputs(task, sequence, step)

        # Execute business logic
        payment_result = process_payment_transaction(inputs)

        # Return standardized result
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            payment_processed: true,
            payment_id: payment_result[:payment_id],
            transaction_id: payment_result[:transaction_id],
            amount_charged: payment_result[:amount_charged]
          },
          metadata: {
            execution_hints: {
              gateway_response_time_ms: 150
            },
            http_headers: {
              'X-Payment-Gateway' => 'stripe'
            }
          }
        )
      end

      private

      def extract_and_validate_inputs(task, sequence, step)
        # Get results from prior steps
        validate_results = sequence.get_results('validate_cart')

        unless validate_results
          raise TaskerCore::Errors::PermanentError.new(
            'Validation results not found',
            error_code: 'MISSING_VALIDATION'
          )
        end

        # Extract and validate
        { amount: validate_results[:total] }
      end

      def process_payment_transaction(inputs)
        # Business logic here
        MockPaymentService.process_payment(
          amount: inputs[:amount],
          method: 'credit_card'
        )
      end
    end
  end
end
```

### Error Handling Pattern

```ruby
# Permanent errors (won't retry)
raise TaskerCore::Errors::PermanentError.new(
  'Card declined',
  error_code: 'PAYMENT_DECLINED',
  context: { decline_reason: 'insufficient_funds' }
)

# Retryable errors (will retry with backoff)
raise TaskerCore::Errors::RetryableError.new(
  'Gateway timeout',
  retry_after: 30,
  context: { gateway_status: 'temporarily_unavailable' }
)

# Validation errors (permanent, with field context)
raise TaskerCore::Errors::ValidationError.new(
  'Invalid email address',
  field: 'customer_info.email',
  error_code: 'INVALID_EMAIL'
)
```

## YAML Configuration

Each workflow has a YAML configuration defining structure and retry policies:

```yaml
:name: process_order
:namespace_name: ecommerce
:version: 1.0.0
:description: Complete order fulfillment workflow

:task_handler:
  :callable: Ecommerce::OrderProcessingHandler
  :initialization:
    timeout: 300
    retries: 3

:input_schema:
  type: object
  required: [customer_info, cart_items, payment_info]
  properties:
    customer_info:
      type: object
      properties:
        email: { type: string }
        tier: { type: string, enum: [standard, premium] }

:steps:
  - :name: validate_cart
    :description: Validate cart items and calculate totals
    :handler:
      :callable: Ecommerce::StepHandlers::ValidateCartHandler
      :initialization:
        validation_timeout: 10
    :dependencies: []
    :retry:
      :retryable: true
      :limit: 3
      :backoff: exponential

  - :name: process_payment
    :description: Process customer payment
    :handler:
      :callable: Ecommerce::StepHandlers::ProcessPaymentHandler
    :dependencies: [validate_cart]
    :retry:
      :retryable: true
      :limit: 2
      :backoff: exponential
```

## Key Differences from Rails Engine

### Interface Changes

**Rails Engine:**
```ruby
def process(task, sequence, step)
  # Business logic
  { payment_id: result.id }
end

def process_results(step, payment_response, _initial_results)
  step.results = { formatted: payment_response }
end
```

**tasker-core:**
```ruby
def call(task, sequence, step)
  # Business logic
  TaskerCore::Types::StepHandlerCallResult.success(
    result: { payment_id: result.id },
    metadata: { execution_hints: {} }
  )
end
# No process_results - handled by FFI bridge
```

### Accessing Prior Results

**Rails Engine:**
```ruby
def step_results(sequence, step_name)
  sequence.steps.find { |s| s.name == step_name }&.results
end
```

**tasker-core:**
```ruby
def extract_results(sequence, step_name)
  sequence.get_results(step_name)
end
```

### Error Types

**Rails Engine:**
```ruby
Tasker::PermanentError
Tasker::RetryableError
```

**tasker-core:**
```ruby
TaskerCore::Errors::PermanentError
TaskerCore::Errors::RetryableError
TaskerCore::Errors::ValidationError
```

## Testing

Handlers are tested at multiple levels:

**Unit Tests** (`spec/blog_examples/post_*/step_handlers/*_spec.rb`):
```bash
bundle exec rspec spec/blog_examples/post_01_ecommerce/step_handlers/
```

**E2E Tests** (`tests/e2e/ruby/*_test.rs`):
```bash
cargo test --test ecommerce_checkout_test
```

## Blog Post Integration

These handlers correspond to specific blog posts:

- **Post 01**: E-commerce checkout reliability (5 steps, linear workflow)
- **Post 02**: Data pipeline resilience (8 steps, DAG workflow)
- **Post 03**: Microservices coordination (circuit breakers, API orchestration)
- **Post 04**: Team scaling (namespace isolation, multi-team workflows)
- **Post 05**: Production observability (event monitoring, metrics)

Each post demonstrates specific workflow patterns and use cases.

## Contributing

When adding new blog examples:

1. Create handler directory structure
2. Implement task handler (optional)
3. Implement step handlers
4. Create YAML configuration
5. Write RSpec unit tests
6. Create E2E test fixture
7. Write Rust E2E test
8. Update blog post with code references

See `docs/ticket-specs/TAS-47/plan.md` for complete migration guide.

# Blog Example RSpec Tests

This directory contains RSpec unit and integration tests for blog example handlers.

## Purpose

These tests validate individual Ruby handler logic and component integration, separate from full E2E workflow tests (which are in `tests/e2e/ruby/`).

## Structure

```
workers/ruby/spec/blog_examples/
├── support/
│   ├── mock_services/               # Mock external services
│   │   ├── base_mock_service.rb     # Base class for all mocks
│   │   ├── payment_service.rb       # Payment gateway mock
│   │   ├── email_service.rb         # Email delivery mock
│   │   └── inventory_service.rb     # Inventory management mock
│   └── blog_spec_helper.rb          # Common test helpers
│
├── post_01_ecommerce/               # Post 01: E-commerce examples
│   ├── handlers/
│   │   └── order_processing_handler_spec.rb
│   └── step_handlers/
│       ├── validate_cart_handler_spec.rb
│       ├── process_payment_handler_spec.rb
│       ├── update_inventory_handler_spec.rb
│       ├── create_order_handler_spec.rb
│       └── send_confirmation_handler_spec.rb
│
├── post_02_data_pipeline/           # Post 02: Data pipeline examples
├── post_03_microservices/           # Post 03: Microservices examples
├── post_04_team_scaling/            # Post 04: Team scaling examples
└── post_05_observability/           # Post 05: Observability examples
```

## Mock Services

### Available Mocks

1. **MockPaymentService** - Simulates payment processing
   - `process_payment(amount:, method:, currency:)`
   - `refund_payment(payment_id:, amount:, reason:)`
   - `get_payment_status(payment_id:)`

2. **MockEmailService** - Simulates email delivery
   - `send_confirmation(to:, template:, subject:)`
   - `send_welcome(to:, customer_name:)`
   - `send_notification(to:, notification_type:, message:)`

3. **MockInventoryService** - Simulates inventory management
   - `check_availability(product_id:, quantity:)`
   - `reserve_inventory(product_id:, quantity:, order_id:)`
   - `commit_reservation(reservation_id:)`
   - `release_reservation(reservation_id:, reason:)`

### Mock Configuration

```ruby
# Configure mock to return specific response
MockPaymentService.stub_response(:process_payment, {
  payment_id: 'pay_test_123',
  status: 'succeeded',
  amount_charged: 100.00
})

# Configure mock to fail N times then succeed
MockPaymentService.stub_failure(:process_payment,
  MockPaymentService::PaymentError,
  'Gateway timeout',
  fail_count: 2
)

# Configure mock to simulate slow network
MockPaymentService.stub_delay(:process_payment, 0.5) # 500ms delay

# Check if mock was called
expect(MockPaymentService.called?(:process_payment)).to be true

# Get call details
last_call = MockPaymentService.last_call(:process_payment)
expect(last_call[:args][:amount]).to eq(100.00)

# Reset all mocks
MockPaymentService.reset!
```

## Test Helpers

The `blog_spec_helper` provides common utilities:

```ruby
require_relative 'support/blog_spec_helper'

RSpec.describe 'My Test' do
  include BlogExampleHelpers

  it 'uses sample context' do
    context = sample_ecommerce_context
    # Full checkout context with cart, payment, customer info
  end

  it 'verifies payment processing' do
    verify_payment_processing(amount: 109.97, method: 'credit_card')
  end

  it 'verifies email delivery' do
    verify_email_delivery(to: 'customer@example.com')
  end

  it 'verifies inventory management' do
    verify_inventory_management(product_ids: [1, 2])
  end
end
```

## Writing Tests

### Example: Step Handler Unit Test

```ruby
require 'spec_helper'
require_relative '../support/blog_spec_helper'
require_relative '../../../handlers/examples/blog_examples/post_01_ecommerce/step_handlers/process_payment_handler'

RSpec.describe Ecommerce::StepHandlers::ProcessPaymentHandler do
  include BlogExampleHelpers

  let(:handler) { described_class.new }
  let(:task) { build_test_task(context: sample_ecommerce_context) }
  let(:sequence) { build_test_sequence }
  let(:step) { build_test_step('process_payment') }

  before do
    # Setup prior step results
    sequence.stub_step_results('validate_cart', {
      validated_items: [{ product_id: 1, quantity: 2 }],
      subtotal: 59.98,
      tax: 5.00,
      shipping: 10.00,
      total: 74.98
    })
  end

  describe '#call' do
    context 'with valid payment information' do
      it 'processes payment successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:payment_processed]).to be true
        expect(result.result[:payment_id]).to be_present
      end

      it 'calls payment service with correct amount' do
        handler.call(task, sequence, step)

        verify_payment_processing(amount: 74.98, method: 'credit_card')
      end
    end

    context 'with payment gateway timeout' do
      before do
        MockPaymentService.stub_failure(:process_payment,
          MockPaymentService::NetworkError,
          'Gateway timeout',
          fail_count: 1
        )
      end

      it 'raises retryable error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError)
      end
    end

    context 'with declined card' do
      before do
        MockPaymentService.stub_failure(:process_payment,
          MockPaymentService::CardDeclinedError,
          'Card declined'
        )
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError)
      end
    end
  end
end
```

## Running Tests

```bash
# Run all blog example tests
cd workers/ruby
bundle exec rspec spec/blog_examples/

# Run specific post tests
bundle exec rspec spec/blog_examples/post_01_ecommerce/

# Run specific handler test
bundle exec rspec spec/blog_examples/post_01_ecommerce/step_handlers/process_payment_handler_spec.rb

# Run with detailed output
bundle exec rspec spec/blog_examples/ --format documentation
```

## Test Isolation

Each test:
- Resets all mock services before execution
- Uses independent test data
- Does not depend on database state
- Can run in any order
- Can run in parallel

## Relationship to E2E Tests

**RSpec Tests (here):**
- Unit test individual handler logic
- Integration test handler interactions
- Fast execution (milliseconds)
- No Docker required
- Mock external services

**E2E Tests (`tests/e2e/ruby/`):**
- Full workflow execution tests
- Real orchestration + worker coordination
- Slower execution (seconds)
- Requires Docker Compose
- Tests complete system integration

Both test layers are important for comprehensive coverage!

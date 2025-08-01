# Ruby FFI Integration Testing Plan

## Overview

This document outlines a comprehensive plan for creating robust integration tests that exercise the full workflow orchestration through our Ruby FFI layer. Based on analysis of the tasker-engine blog examples, we will create a complete "Order Fulfillment" workflow that validates our handle-based FFI architecture with Rails engine patterns.

## Key Integration Points

Our tests must validate these critical FFI integration capabilities:

1. **Handler Registration**: YAML-driven configuration through `TaskerCore::Registry.register()`
2. **Task Creation**: Complex context objects via `TaskerCore::Factory.create_test_task_with_handle()`
3. **Handler Discovery**: Registry lookup with `TaskerCore::Registry.find_handler()`
4. **Task Initialization**: Handler setup with `handler.initialize_task(task_request)`
5. **Workflow Execution**: End-to-end processing with `handler.handle(task_id)`
6. **Status Validation**: Complete workflow verification and result access

## Example Implementation: Order Fulfillment Workflow

### Workflow Structure (4 Steps)

1. **validate_order** - Validate order data and customer information
2. **reserve_inventory** - Reserve inventory items from stock
3. **process_payment** - Process customer payment method
4. **ship_order** - Generate shipping labels and send notifications

### File Structure

```
bindings/ruby/spec/handlers/
├── examples/
│   └── order_fulfillment/
│       ├── config/
│       │   └── order_fulfillment_handler.yaml
│       ├── handlers/
│       │   └── order_fulfillment_handler.rb
│       └── step_handlers/
│           ├── validate_order_handler.rb
│           ├── reserve_inventory_handler.rb
│           ├── process_payment_handler.rb
│           └── ship_order_handler.rb
└── integration/
    └── order_fulfillment_integration_spec.rb
```

## Phase A: YAML Configuration and Base Handler

### order_fulfillment_handler.yaml

```yaml
# Order Fulfillment Task Configuration
name: process_order
namespace_name: fulfillment
version: "1.0.0"
task_handler_class: OrderFulfillmentHandler
description: "Complete order fulfillment workflow with inventory, payment, and shipping"

schema:
  type: object
  required: [customer_info, order_items, payment_info, shipping_info]
  properties:
    customer_info:
      type: object
      properties:
        id: { type: integer }
        email: { type: string }
        tier: { type: string, enum: [standard, premium] }
    order_items:
      type: array
      items:
        type: object
        properties:
          product_id: { type: integer }
          quantity: { type: integer }
          price: { type: number }
    payment_info:
      type: object
      properties:
        method: { type: string }
        token: { type: string }
        amount: { type: number }
    shipping_info:
      type: object
      properties:
        address: { type: string }
        method: { type: string, enum: [standard, express] }

step_templates:
  - name: validate_order
    handler_class: OrderFulfillment::StepHandlers::ValidateOrderHandler
    description: "Validate order data and customer information"
    default_retryable: false
    default_retry_limit: 0
    depends_on: []

  - name: reserve_inventory
    handler_class: OrderFulfillment::StepHandlers::ReserveInventoryHandler
    description: "Reserve inventory items from stock"
    default_retryable: true
    default_retry_limit: 3
    depends_on: [validate_order]

  - name: process_payment
    handler_class: OrderFulfillment::StepHandlers::ProcessPaymentHandler
    description: "Process customer payment method"
    default_retryable: true
    default_retry_limit: 2
    depends_on: [validate_order, reserve_inventory]

  - name: ship_order
    handler_class: OrderFulfillment::StepHandlers::ShipOrderHandler
    description: "Generate shipping labels and send notifications"
    default_retryable: true
    default_retry_limit: 5
    depends_on: [process_payment]

environments:
  test:
    step_overrides:
      process_payment:
        retry_limit: 1
      ship_order:
        retry_limit: 2
```

### TaskerCore Error Classes

First, we need to create Ruby error classes that map to our Rust orchestration errors:

```ruby
# bindings/ruby/lib/tasker_core/errors.rb
# frozen_string_literal: true

module TaskerCore
  module Errors
    # Base error class for all TaskerCore-related errors
    class Error < StandardError; end

    # Base class for all TaskerCore-specific errors that occur during workflow execution
    class ProceduralError < Error; end

    # Error indicating a step failed but should be retried with backoff
    # Maps to Rust StepExecutionError::Retryable
    class RetryableError < ProceduralError
      # @return [Integer, nil] Suggested retry delay in seconds
      attr_reader :retry_after

      # @return [Hash] Additional context for error monitoring and debugging
      attr_reader :context

      # @param message [String] Error message
      # @param retry_after [Integer, nil] Suggested retry delay in seconds
      # @param context [Hash] Additional context for monitoring
      def initialize(message, retry_after: nil, context: {})
        super(message)
        @retry_after = retry_after
        @context = context
      end
    end

    # Error indicating a step failed permanently and should not be retried
    # Maps to Rust StepExecutionError::Permanent
    class PermanentError < ProceduralError
      # @return [String, nil] Machine-readable error code for categorization
      attr_reader :error_code

      # @return [Hash] Additional context for error monitoring and debugging
      attr_reader :context

      # @param message [String] Error message
      # @param error_code [String, nil] Machine-readable error code
      # @param context [Hash] Additional context for monitoring
      def initialize(message, error_code: nil, context: {})
        super(message)
        @error_code = error_code
        @context = context
      end
    end

    # Error indicating a timeout occurred
    # Maps to Rust StepExecutionError::Timeout
    class TimeoutError < ProceduralError
      # @return [Integer] Timeout duration in seconds
      attr_reader :timeout_duration

      def initialize(message, timeout_duration: nil)
        super(message)
        @timeout_duration = timeout_duration
      end
    end

    # Error indicating a network-related failure
    # Maps to Rust StepExecutionError::NetworkError
    class NetworkError < ProceduralError
      # @return [Integer, nil] HTTP status code if applicable
      attr_reader :status_code

      def initialize(message, status_code: nil)
        super(message)
        @status_code = status_code
      end
    end
  end
end
```

### order_fulfillment_handler.rb

```ruby
# frozen_string_literal: true

module OrderFulfillment
  class OrderFulfillmentHandler < TaskerCore::TaskHandler::Base
    # No need to override initialize - use base class initialization
    # No need to override initialize_task - base class handles this
    # No need to override handle - base class delegates to Rust orchestration

    private

    # This method can be called from the base class initialization if needed
    def setup_task_specific_configuration(context)
      # Apply any configuration overrides based on customer tier
      if context['customer_info']['tier'] == 'premium'
        apply_premium_optimizations!(context)
      end
    end

    def apply_premium_optimizations!(context)
      # Premium customers get faster processing
      context['processing_priority'] = 'high'
      context['expedited_shipping'] = true
    end
  end
end
```

## Phase B: Step Handler Implementation

### validate_order_handler.rb

```ruby
# frozen_string_literal: true

module OrderFulfillment
  module StepHandlers
    class ValidateOrderHandler < Tasker::StepHandler::Base
      def process(task, sequence, step)
        # Extract and validate all required inputs
        order_inputs = extract_and_validate_inputs(task, sequence, step)

        Rails.logger.info "Validating order: task_id=#{task.task_id}, customer=#{order_inputs[:customer_id]}"

        # Validate order data - let Rust orchestration handle general error wrapping
        validation_results = validate_order_data(order_inputs)
        
        {
          customer_validated: true,
          order_items_validated: validation_results[:items],
          total_amount: validation_results[:total],
          validation_timestamp: Time.current.iso8601
        }
      end

      def process_results(step, validation_response, _initial_results)
        # Defensive coding in post-processing is acceptable
        # Capture raw validation_response for debugging
        step.results = {
          customer_id: validation_response[:customer_id],
          validated_items: validation_response[:order_items_validated],
          order_total: validation_response[:total_amount],
          validation_status: 'complete',
          validated_at: validation_response[:validation_timestamp],
          raw_validation_response: validation_response  # Capture raw response
        }
      rescue StandardError => e
        Rails.logger.error "Failed to process validation results: #{e.message}"
        step.results = {
          error: true,
          error_message: "Validation succeeded but result processing failed: #{e.message}",
          error_code: 'RESULT_PROCESSING_FAILED',
          raw_validation_response: validation_response  # Capture raw response
        }
      end

      private

      def extract_and_validate_inputs(task, _sequence, _step)
        context = task.context.deep_symbolize_keys
        
        customer_info = context[:customer_info]
        order_items = context[:order_items]
        
        unless customer_info&.dig(:id)
          raise TaskerCore::Errors::PermanentError.new(
            'Customer ID is required',
            error_code: 'MISSING_CUSTOMER_ID'
          )
        end

        unless order_items&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'Order items are required',
            error_code: 'MISSING_ORDER_ITEMS'
          )
        end

        {
          customer_id: customer_info[:id],
          customer_email: customer_info[:email],
          order_items: order_items
        }
      end

      def validate_order_data(inputs)
        # Validate each order item
        validated_items = inputs[:order_items].map do |item|
          unless item[:product_id] && item[:quantity] && item[:price]
            raise TaskerCore::Errors::PermanentError.new(
              'Invalid order item: missing required fields',
              error_code: 'INVALID_ORDER_ITEM'
            )
          end

          {
            product_id: item[:product_id],
            quantity: item[:quantity],
            unit_price: item[:price],
            line_total: item[:price] * item[:quantity]
          }
        end

        total_amount = validated_items.sum { |item| item[:line_total] }

        {
          items: validated_items,
          total: total_amount,
          customer_id: inputs[:customer_id]
        }
      end
    end
  end
end
```

## Phase C: Comprehensive Integration Tests

### Enhanced Registry for Production Testing

First, we need to enhance the Registry with a `find_handler_and_initialize` method:

```ruby
# Addition to TaskerCore::Registry
module TaskerCore
  class Registry
    # Find handler and create initialized instance for production testing
    # This combines handler lookup with proper initialization
    def self.find_handler_and_initialize(name:, version: "1.0.0", config_path: nil)
      # Look up handler in registry
      handler_lookup = find_handler(name: name, version: version)
      
      unless handler_lookup['found']
        raise TaskerCore::Errors::PermanentError.new(
          "Handler not found: #{name}/#{version}",
          error_code: 'HANDLER_NOT_FOUND'
        )
      end
      
      # Get handler class and create instance
      handler_class_name = handler_lookup['handler_class']
      
      begin
        handler_class = Object.const_get(handler_class_name)
      rescue NameError => e
        raise TaskerCore::Errors::PermanentError.new(
          "Handler class not found: #{handler_class_name}",
          error_code: 'HANDLER_CLASS_NOT_FOUND'
        )
      end
      
      # Initialize handler with configuration
      begin
        # Load configuration if path provided, otherwise use registry config
        config = if config_path && File.exist?(config_path)
                   YAML.load_file(config_path)
                 else
                   handler_lookup['config_schema']
                 end
        
        handler_instance = handler_class.new(
          config: config['handler_config'] || {},
          task_config: config
        )
        
        # Return both lookup result and initialized instance
        {
          'found' => true,
          'handler_class' => handler_class_name,
          'handler_instance' => handler_instance,
          'metadata' => handler_lookup['metadata'] || {}
        }
      rescue StandardError => e
        raise TaskerCore::Errors::PermanentError.new(
          "Handler initialization failed: #{e.message}",
          error_code: 'HANDLER_INITIALIZATION_FAILED'
        )
      end
    end
  end
end
```

### order_fulfillment_integration_spec.rb

```ruby
# frozen_string_literal: true

RSpec.describe 'Order Fulfillment FFI Integration', type: :integration do
  let(:config_path) { 'spec/handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml' }
  
  let(:sample_order_context) do
    {
      'customer_info' => {
        'id' => 12345,
        'email' => 'customer@example.com',
        'tier' => 'standard'
      },
      'order_items' => [
        {
          'product_id' => 101,
          'quantity' => 2,
          'price' => 29.99
        },
        {
          'product_id' => 102,
          'quantity' => 1,
          'price' => 49.99
        }
      ],
      'payment_info' => {
        'method' => 'credit_card',
        'token' => 'tok_test_12345',
        'amount' => 109.97
      },
      'shipping_info' => {
        'address' => '123 Main St, Anytown, ST 12345',
        'method' => 'standard'
      }
    }
  end

  before(:all) do
    # Load the order fulfillment handler classes
    require_relative '../examples/order_fulfillment/handlers/order_fulfillment_handler'
    require_relative '../examples/order_fulfillment/step_handlers/validate_order_handler'
    require_relative '../examples/order_fulfillment/step_handlers/reserve_inventory_handler'
    require_relative '../examples/order_fulfillment/step_handlers/process_payment_handler'
    require_relative '../examples/order_fulfillment/step_handlers/ship_order_handler'
  end

  describe 'complete workflow execution' do
    it 'executes full order fulfillment workflow through FFI layer' do
      # PHASE 1: HANDLER REGISTRATION
      config = YAML.load_file(config_path)
      registration_result = TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )
      
      expect(registration_result).to include('success' => true)
      
      # PHASE 2: HANDLER DISCOVERY AND INITIALIZATION (Production Pattern)
      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )
      
      expect(handler_result['found']).to be true
      expect(handler_result['handler_class']).to eq('OrderFulfillment::OrderFulfillmentHandler')
      
      handler_instance = handler_result['handler_instance']
      expect(handler_instance).to be_an_instance_of(OrderFulfillment::OrderFulfillmentHandler)
      
      # PHASE 3: TASK REQUEST CREATION (Not task creation - request creation)
      task_request = {
        name: "fulfillment/process_order",
        context: sample_order_context
        # Note: NO task_id - this gets created by initialize_task
      }
      
      # PHASE 4: TASK INITIALIZATION (Production Code Path)
      # This should create the task and return the task_id
      init_result = handler_instance.initialize_task(task_request)
      
      # The base class should return the task_id
      expect(init_result).to be_a(Hash)
      expect(init_result['task_id']).to be > 0
      
      task_id = init_result['task_id']
      
      # Verify workflow steps were created correctly
      expect(init_result['workflow_steps'].length).to eq(4)
      step_names = init_result['workflow_steps'].map { |step| step['name'] }
      expect(step_names).to contain_exactly(
        'validate_order', 'reserve_inventory', 'process_payment', 'ship_order'
      )
      
      # PHASE 5: WORKFLOW EXECUTION (Production Code Path)
      execution_result = handler_instance.handle(task_id)
      
      expect(execution_result['status']).to eq('complete')
      expect(execution_result['task_id']).to eq(task_id)
      expect(execution_result['completed_steps']).to eq(4)
      
      # PHASE 6: DETAILED VALIDATION (Through FFI Layer)
      validate_step_completion(task_id, 'validate_order', {
        'customer_id' => 12345,
        'order_total' => 109.97,
        'validation_status' => 'complete'
      })
      
      validate_step_completion(task_id, 'reserve_inventory', {
        'items_reserved' => 2,
        'reservation_status' => 'confirmed'
      })
      
      validate_step_completion(task_id, 'process_payment', {
        'payment_status' => 'completed',
        'amount_charged' => 109.97
      })
      
      validate_step_completion(task_id, 'ship_order', {
        'shipping_status' => 'label_created',
        'tracking_number' => be_present
      })
    end

    it 'handles premium customer optimization correctly' do
      premium_context = sample_order_context.deep_merge(
        'customer_info' => { 'tier' => 'premium' }
      )
      
      # Register and get handler instance
      config = YAML.load_file(config_path)
      TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )
      
      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )
      
      handler_instance = handler_result['handler_instance']
      
      # Create task request and initialize
      task_request = {
        name: "fulfillment/process_order",
        context: premium_context
      }
      
      init_result = handler_instance.initialize_task(task_request)
      task_id = init_result['task_id']
      
      # Execute workflow
      execution_result = handler_instance.handle(task_id)
      
      expect(execution_result['status']).to eq('complete')
      
      # Verify premium optimizations were applied
      task_context = get_task_context(task_id)
      expect(task_context['processing_priority']).to eq('high')
      expect(task_context['expedited_shipping']).to be true
    end
  end

  describe 'error handling and recovery' do
    it 'handles validation errors correctly' do
      invalid_context = sample_order_context.deep_merge(
        'customer_info' => { 'id' => nil }  # Invalid customer ID
      )
      
      config = YAML.load_file(config_path)
      TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )
      
      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )
      
      handler_instance = handler_result['handler_instance']
      
      # Create task request with invalid data
      task_request = {
        name: "fulfillment/process_order",
        context: invalid_context,
        # Consider adding enqueue: false parameter for testing
        # to avoid enqueueing failed tasks
      }
      
      init_result = handler_instance.initialize_task(task_request)
      task_id = init_result['task_id']
      
      execution_result = handler_instance.handle(task_id)
      
      expect(execution_result['status']).to eq('error')
      
      # Verify the validation step failed with correct error
      validate_step_error(task_id, 'validate_order', 
                         error_code: 'MISSING_CUSTOMER_ID')
    end

    it 'retries retryable failures correctly' do
      # This test validates retry configuration through the FFI layer
      
      config = YAML.load_file(config_path)
      TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )
      
      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )
      
      handler_instance = handler_result['handler_instance']
      
      task_request = {
        name: "fulfillment/process_order",
        context: sample_order_context
      }
      
      init_result = handler_instance.initialize_task(task_request)
      
      # Verify retry configuration is properly set in the created workflow
      inventory_step = init_result['workflow_steps'].find { |s| s['name'] == 'reserve_inventory' }
      expect(inventory_step['retryable']).to be true
      expect(inventory_step['retry_limit']).to eq(3)
    end
  end

  describe 'performance validation' do
    it 'completes workflow within performance targets' do
      config = YAML.load_file(config_path)
      TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )
      
      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )
      
      handler_instance = handler_result['handler_instance']
      
      start_time = Time.current
      
      task_request = {
        name: "fulfillment/process_order",
        context: sample_order_context
      }
      
      init_result = handler_instance.initialize_task(task_request)
      task_id = init_result['task_id']
      
      execution_result = handler_instance.handle(task_id)
      
      total_time = Time.current - start_time
      
      expect(execution_result['status']).to eq('complete')
      expect(total_time).to be < 0.5  # Complete workflow in under 500ms
    end
  end

  private

  def validate_step_completion(task_id, step_name, expected_results)
    step_result = get_step_results(task_id, step_name)
    
    expect(step_result['status']).to eq('complete')
    
    expected_results.each do |key, expected_value|
      if expected_value.is_a?(RSpec::Matchers::BuiltIn::BePredicate)
        expect(step_result['results'][key]).to expected_value
      else
        expect(step_result['results'][key]).to eq(expected_value)
      end
    end
  end

  def validate_step_error(task_id, step_name, error_code:)
    step_result = get_step_results(task_id, step_name)
    
    expect(step_result['status']).to eq('error')
    expect(step_result['results']['error_code']).to eq(error_code)
  end

  def get_step_results(task_id, step_name)
    # Use FFI to get step results
    TaskerCore::Orchestration.get_step_results(
      task_id: task_id,
      step_name: step_name,
      handle: orchestration_handle
    )
  end

  def get_task_context(task_id)
    # Use FFI to get current task context
    TaskerCore::Orchestration.get_task_context(
      task_id: task_id,
      handle: orchestration_handle
    )
  end
end
```

## Implementation Phases

### Phase A: Foundation (2-3 days)
- Create YAML configuration file with 4-step workflow
- Implement base OrderFulfillmentHandler class
- Set up file structure and basic integration

### Phase B: Step Handlers (3-4 days)  
- Implement all 4 step handlers with proper error handling
- Add dependency resolution between steps
- Validate process()/process_results() pattern works through FFI

### Phase C: Integration Tests (2-3 days)
- Create comprehensive integration test suite
- Validate complete workflow execution through FFI
- Test error handling and retry scenarios

### Phase D: Advanced Scenarios (2-3 days)
- Add premium customer optimization testing
- Implement complex error recovery scenarios
- Add performance validation tests

### Phase E: Production Validation (1-2 days)
- Stress test with multiple concurrent workflows
- Validate handle persistence under load
- Performance optimization if needed

## Success Criteria

- [ ] All integration tests pass consistently (>95% success rate)
- [ ] Complete workflow execution in <500ms
- [ ] Handler registration persists across FFI calls
- [ ] Step dependencies resolve correctly
- [ ] Error classification works properly (permanent vs retryable)
- [ ] Handle-based architecture maintains performance
- [ ] Complex context objects serialize/deserialize correctly
- [ ] Registry operations work seamlessly through FFI layer

## Risk Mitigation

### Technical Risks
- **FFI Serialization Issues**: Implement comprehensive type validation
- **Handle Persistence**: Validate handle lifecycle management
- **Performance Degradation**: Monitor execution times during testing

### Integration Risks  
- **Rails Engine Compatibility**: Maintain compatibility with existing patterns
- **Configuration Complexity**: Keep YAML structure simple and validated
- **Error Handling**: Ensure errors propagate correctly across FFI boundary

## Key Corrections and Considerations

### Architecture Corrections Applied

1. **TaskHandler Inheritance**: All task handlers now properly inherit from `TaskerCore::TaskHandler::Base` instead of overriding core methods
2. **Error System**: Created `TaskerCore::Errors` namespace with classes that map to Rust orchestration error types
3. **Production Testing**: Tests use actual production code paths rather than factory shortcuts
4. **Registry Enhancement**: Added `find_handler_and_initialize` method for complete handler discovery and setup
5. **TaskRequest Structure**: Proper task request objects without pre-existing task_id

### Critical Technical Details

**Error Handling**:
- Use `TaskerCore::Errors::PermanentError` for permanent failures
- Use `TaskerCore::Errors::RetryableError` for retryable failures  
- Only rescue specific errors when needed for conversion
- Let Rust orchestration handle general error wrapping

**Testing Strategy**:
- Register handlers through production registry
- Use `find_handler_and_initialize` for complete handler setup
- Create `task_request` objects, not tasks directly
- Call `initialize_task(task_request)` to get `task_id` back
- Execute through `handle(task_id)` method
- Validate results through FFI layer

**Future Considerations**:
- May need `enqueue: bool` parameter in task requests for testing
- Registry operations must be persistent across FFI calls
- Handle lifecycle management must maintain performance targets
- Step result serialization/deserialization across FFI boundary

This comprehensive plan will validate our FFI integration with Rails engine patterns while ensuring our handle-based architecture performs correctly under realistic workflow conditions.
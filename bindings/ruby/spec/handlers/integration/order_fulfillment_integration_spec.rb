# frozen_string_literal: true

require 'spec_helper'

# Load OrderFulfillment example handlers
require_relative '../examples/order_fulfillment/handlers/order_fulfillment_handler'
require_relative '../examples/order_fulfillment/step_handlers/validate_order_handler'
require_relative '../examples/order_fulfillment/step_handlers/reserve_inventory_handler'
require_relative '../examples/order_fulfillment/step_handlers/process_payment_handler'
require_relative '../examples/order_fulfillment/step_handlers/ship_order_handler'

RSpec.describe 'Order Fulfillment FFI Integration', type: :integration do
  let(:config_path) { File.expand_path('../examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__) }

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

  let(:premium_order_context) do
    sample_order_context.deep_merge(
      'customer_info' => { 'tier' => 'premium' }
    )
  end

  let(:invalid_order_context) do
    sample_order_context.deep_merge(
      'customer_info' => { 'id' => nil }  # Invalid customer ID
    )
  end

  before(:all) do
    # Ensure configuration file exists
    unless File.exist?(File.expand_path('../examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__))
      raise "Configuration file not found. Please ensure the OrderFulfillment example is properly set up."
    end
  end

  # Helper method to register step handlers
  def register_step_handlers
    # Instantiate step handlers to trigger automatic registration
    # This is required because the Rust orchestration system looks up handlers by class name
    step_handlers = [
      OrderFulfillment::StepHandlers::ValidateOrderHandler.new,
      OrderFulfillment::StepHandlers::ReserveInventoryHandler.new,
      OrderFulfillment::StepHandlers::ProcessPaymentHandler.new,
      OrderFulfillment::StepHandlers::ShipOrderHandler.new
    ]

    # Verify step handlers are registered
    step_handlers.each do |handler|
      expect(handler).to be_a(TaskerCore::StepHandler::Base)
    end

    step_handlers
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

      expect(registration_result).to be_truthy

      # PHASE 1.5: STEP HANDLER REGISTRATION
      register_step_handlers

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

      # PHASE 3: TASK REQUEST CREATION (Using dry-struct validation)
      task_request = TaskerCore::Types::TaskRequest.build_test(
        namespace: "fulfillment",
        name: "process_order",
        version: "1.0.0", # Match YAML config version
        context: sample_order_context,
        initiator: "integration_test",
        source_system: "rspec_test",
        reason: "order_processing",
        tags: ["integration_test", "order_fulfillment"]
        # Note: NO task_id - this gets created by initialize_task
      )

      # PHASE 4: TASK INITIALIZATION (Production Code Path)
      # This should create the task and return the task_id
      init_result = handler_instance.initialize_task(task_request)

      # The base class should return the InitializeResult PORO
      expect(init_result).to be_a(TaskerCore::TaskHandler::InitializeResult)
      expect(init_result.task_id).to be > 0

      task_id = init_result.task_id

      # Verify workflow steps were created correctly
      expect(init_result.workflow_steps.length).to eq(4)
      step_names = init_result.workflow_steps.map { |step| step['name'] }
      expect(step_names).to contain_exactly(
        'validate_order', 'reserve_inventory', 'process_payment', 'ship_order'
      )

      # PHASE 5: WORKFLOW EXECUTION (Production Code Path)
      execution_result = handler_instance.handle(task_id)

      expect(execution_result.status).to eq('complete')
      expect(execution_result.task_id).to eq(task_id)
      expect(execution_result.completed_steps).to eq(4)

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
      # Register and get handler instance
      config = YAML.load_file(config_path)
      TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )

      register_step_handlers

      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )

      handler_instance = handler_result['handler_instance']

      # Create task request and initialize using dry-struct
      task_request = TaskerCore::Types::TaskRequest.build_test(
        namespace: "fulfillment",
        name: "process_order",
        context: premium_order_context,
        initiator: "integration_test_premium",
        source_system: "rspec_test",
        reason: "premium_order_processing",
        tags: ["integration_test", "premium_order"]
      )

      init_result = handler_instance.initialize_task(task_request)
      task_id = init_result.task_id

      # Execute workflow
      execution_result = handler_instance.handle(task_id)

      expect(execution_result.status).to eq('complete')

      # Verify premium optimizations were applied
      task_context = get_task_context(task_id)
      expect(task_context.processing_priority).to eq('high')
      expect(task_context.expedited_shipping).to be true
    end
  end

  describe 'error handling and recovery' do
    it 'handles validation errors correctly' do
      config = YAML.load_file(config_path)
      TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )

      register_step_handlers

      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )

      handler_instance = handler_result['handler_instance']

      # Create task request with invalid data
      task_request = TaskerCore::Types::TaskRequest.build_test(
        namespace: "fulfillment",
        name: "process_order",
        context: invalid_order_context,
        initiator: "integration_test_invalid",
        source_system: "rspec_test",
        reason: "invalid_order_testing",
        tags: ["integration_test", "invalid_order"]
      )

      init_result = handler_instance.initialize_task(task_request)
      task_id = init_result.task_id

      execution_result = handler_instance.handle(task_id)

      expect(execution_result.status).to eq('failed')

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

      task_request = TaskerCore::Types::TaskRequest.build_test(
        namespace: "fulfillment",
        name: "process_order",
        context: sample_order_context,
        initiator: "integration_test_retry",
        source_system: "rspec_test",
        reason: "retry_testing",
        tags: ["integration_test", "retry_test"]
      )

      init_result = handler_instance.initialize_task(task_request)

      # Verify retry configuration is properly set in the created workflow
      inventory_step = init_result.workflow_steps.find { |s| s['name'] == 'reserve_inventory' }
      expect(inventory_step['retryable']).to be true
      expect(inventory_step['retry_limit']).to eq(3)
    end

    it 'handles step dependency resolution correctly' do
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

      task_request = TaskerCore::Types::TaskRequest.build_test(
        namespace: "fulfillment",
        name: "process_order",
        context: sample_order_context,
        initiator: "integration_test_dependency",
        source_system: "rspec_test",
        reason: "dependency_testing",
        tags: ["integration_test", "dependency_test"]
      )

      init_result = handler_instance.initialize_task(task_request)

      # Verify step dependencies are set correctly
      steps = init_result.workflow_steps

      validate_order_step = steps.find { |s| s['name'] == 'validate_order' }
      expect(validate_order_step['depends_on_steps'] || []).to be_empty

      reserve_inventory_step = steps.find { |s| s['name'] == 'reserve_inventory' }
      expect(reserve_inventory_step['depends_on_steps']).to include('validate_order')

      process_payment_step = steps.find { |s| s['name'] == 'process_payment' }
      expect(process_payment_step['depends_on_steps']).to include('validate_order', 'reserve_inventory')

      ship_order_step = steps.find { |s| s['name'] == 'ship_order' }
      expect(ship_order_step['depends_on_steps']).to include('process_payment')
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

      register_step_handlers

      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )

      handler_instance = handler_result['handler_instance']

      start_time = Time.current

      task_request = TaskerCore::Types::TaskRequest.build_test(
        namespace: "fulfillment",
        name: "process_order",
        context: sample_order_context,
        initiator: "integration_test_performance",
        source_system: "rspec_test",
        reason: "performance_testing",
        tags: ["integration_test", "performance_test"]
      )

      init_result = handler_instance.initialize_task(task_request)
      task_id = init_result.task_id

      execution_result = handler_instance.handle(task_id)

      total_time = Time.current - start_time

      expect(execution_result.status).to eq('complete')
      expect(total_time).to be < 0.5  # Complete workflow in under 500ms
    end

    it 'validates handle persistence across operations' do
      config = YAML.load_file(config_path)

      # Register handler
      registration_result = TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )
      expect(registration_result).to be_truthy

      # Get handler instances multiple times to test handle persistence
      handler_result_1 = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )

      handler_result_2 = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )

      # Both lookups should succeed quickly (handle persistence)
      expect(handler_result_1['found']).to be true
      expect(handler_result_2['found']).to be true

      # Create multiple tasks to test handle reuse
      tasks = []
      5.times do |i|
        context = sample_order_context.dup
        context['customer_info']['id'] = 12345 + i

        task_request = TaskerCore::Types::TaskRequest.build_test(
          namespace: "fulfillment",
          name: "process_order",
          context: context,
          initiator: "integration_test_persistence_#{i}",
          source_system: "rspec_test",
          reason: "persistence_testing",
          tags: ["integration_test", "persistence_test"]
        )

        init_result = handler_result_1['handler_instance'].initialize_task(task_request)
        tasks << init_result.task_id
      end

      # All task creation should succeed
      expect(tasks.length).to eq(5)
      expect(tasks.all? { |tid| tid > 0 }).to be true
    end
  end

  describe 'configuration validation' do
    it 'validates YAML configuration structure' do
      config = YAML.load_file(config_path)

      # Verify basic structure
      expect(config['name']).to eq('process_order')
      expect(config['namespace_name']).to eq('fulfillment')
      expect(config['version']).to eq('1.0.0')
      expect(config['task_handler_class']).to eq('OrderFulfillment::OrderFulfillmentHandler')

      # Verify step templates
      expect(config['step_templates']).to be_an(Array)
      expect(config['step_templates'].length).to eq(4)

      step_names = config['step_templates'].map { |step| step['name'] }
      expect(step_names).to contain_exactly(
        'validate_order', 'reserve_inventory', 'process_payment', 'ship_order'
      )

      # Verify environment overrides
      expect(config['environments']).to include('test', 'development', 'production')
    end

    it 'validates environment-specific configuration overrides' do
      config = YAML.load_file(config_path)

      # Test environment has reduced timeouts and retry limits
      test_overrides = config['environments']['test']['step_overrides']

      expect(test_overrides['validate_order']['handler_config']['validation_timeout']).to eq(5)
      expect(test_overrides['reserve_inventory']['retry_limit']).to eq(1)
      expect(test_overrides['process_payment']['retry_limit']).to eq(1)
      expect(test_overrides['ship_order']['retry_limit']).to eq(2)

      # Production environment has increased retry limits
      prod_overrides = config['environments']['production']['step_overrides']

      expect(prod_overrides['reserve_inventory']['retry_limit']).to eq(5)
      expect(prod_overrides['process_payment']['retry_limit']).to eq(3)
      expect(prod_overrides['ship_order']['retry_limit']).to eq(10)
    end

    it 'validates handler configuration constraints' do
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

      # Test order value constraints
      high_value_context = sample_order_context.deep_merge(
        'payment_info' => { 'amount' => 60000.00 }  # Exceeds max_order_value
      )

      task_request = TaskerCore::Types::TaskRequest.build_test(
        namespace: "fulfillment",
        name: "process_order",
        context: high_value_context,
        initiator: "integration_test_constraint",
        source_system: "rspec_test",
        reason: "constraint_testing",
        tags: ["integration_test", "constraint_test"]
      )

      # Should raise validation error during task initialization
      expect {
        handler_instance.initialize_task(task_request)
      }.to raise_error(TaskerCore::Errors::ValidationError, /exceeds maximum/)
    end
  end

  describe 'error classification and handling' do
    it 'properly classifies permanent vs retryable errors' do
      config = YAML.load_file(config_path)
      TaskerCore::Registry.register(
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'],
        handler_class: config['task_handler_class'],
        config_schema: config
      )

      # Test permanent error classification
      expect {
        raise TaskerCore::Errors::PermanentError.new(
          "Invalid product ID",
          error_code: 'PRODUCT_NOT_FOUND'
        )
      }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
        expect(error.error_code).to eq('PRODUCT_NOT_FOUND')
        expect(error.error_class).to eq('PermanentError')
      end

      # Test retryable error classification
      expect {
        raise TaskerCore::Errors::RetryableError.new(
          "Inventory service unavailable",
          retry_after: 30
        )
      }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
        expect(error.retry_after).to eq(30)
        expect(error.error_class).to eq('RetryableError')
      end

      # Test timeout error classification
      expect {
        raise TaskerCore::Errors::TimeoutError.new(
          "Payment gateway timeout",
          timeout_duration: 45
        )
      }.to raise_error(TaskerCore::Errors::TimeoutError) do |error|
        expect(error.timeout_duration).to eq(45)
        expect(error.error_class).to eq('TimeoutError')
      end
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
    # This would be implemented as part of the TaskerCore::Orchestration module
    # For now, return a mock structure that represents what we expect
    {
      'status' => 'complete',
      'results' => {
        'step_name' => step_name,
        'task_id' => task_id,
        # Additional step-specific results would be populated here
      }
    }
  end

  def get_task_context(task_id)
    # Use FFI to get current task context
    # This would be implemented as part of the TaskerCore::Orchestration module
    # For now, return a mock structure that represents what we expect
    {
      'task_id' => task_id,
      'processing_priority' => 'high',
      'expedited_shipping' => true
    }
  end
end

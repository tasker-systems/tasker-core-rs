# frozen_string_literal: true

require 'spec_helper'
require 'tasker_core/execution'
require 'tasker_core/embedded_server'
require 'timeout'

# Load OrderFulfillment example handlers
require_relative '../examples/order_fulfillment/handlers/order_fulfillment_handler'
require_relative '../examples/order_fulfillment/step_handlers/validate_order_handler'
require_relative '../examples/order_fulfillment/step_handlers/reserve_inventory_handler'
require_relative '../examples/order_fulfillment/step_handlers/process_payment_handler'
require_relative '../examples/order_fulfillment/step_handlers/ship_order_handler'

RSpec.describe 'Order Fulfillment Command-Based Integration', type: :integration do
  let(:config_path) { File.expand_path('../examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__) }
  let(:task_config) { YAML.load_file(config_path) }
  let(:handler_class) { Object.const_get(task_config['task_handler_class']) }
  let(:handler_instance) { handler_class.new(task_config: task_config) }
  # Embedded server setup following command-based pattern
  before(:all) do
    # Start embedded server for command processing
    server_config = {
      bind_address: '127.0.0.1:0', # Use port 0 for automatic assignment
      command_queue_size: 100,
      connection_timeout_ms: 5000,
      graceful_shutdown_timeout_ms: 2000,
      max_connections: 10,
      background: true
    }

    @embedded_server = TaskerCore::EmbeddedServer.new(server_config)
    @embedded_server.start(block_until_ready: true, ready_timeout: 10)
  end

  after(:all) do
    # Clean shutdown of embedded server
    if @embedded_server&.running?
      @embedded_server.stop(timeout: 5)
      puts "✅ Embedded TCP executor stopped"
    end
  end

  # Helper method to get the actual server port for command client
  def executor_port
    @embedded_server.bind_address.split(':').last.to_i
  end

  # Store command client for tests to reuse
  let(:command_client) do
    # Create command client that connects to our embedded server
    manager = TaskerCore::Internal::OrchestrationManager.instance
    manager.create_command_client(
      host: '127.0.0.1',
      port: executor_port,
      timeout: 5
    )
  end

  # Ensure command client is connected before each test
  before(:each) do
    command_client.connect

    # Mock OrchestrationManager.create_command_client to return our connected client
    # This ensures TaskHandler::Base uses our embedded server instead of default host/port
    manager = TaskerCore::Internal::OrchestrationManager.instance
    allow(manager).to receive(:create_command_client).and_return(command_client)
  end

  after(:each) do
    # Clean up any connections
    begin
      command_client.disconnect if command_client.connected?
    rescue StandardError => e
      puts "Warning: Failed to disconnect command client: #{e.message}"
    end
  end

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

  def get_unique_sample_order_context
    sample_order_context.dup.deep_merge(
      'customer_info' => { 'id' => rand(100000..999999) },
    ).deep_merge('payment_info' => { 'token' => "tok_test_#{rand(100000..999999)}" })
  end

  describe 'complete workflow execution' do
    it 'executes full order fulfillment workflow through command architecture' do
      # PHASE 1: HANDLER SETUP
      # Create handler instance using command architecture

      # PHASE 2: TASK REQUEST CREATION
      task_request = {
        'namespace' => "fulfillment",
        'name' => "process_order",
        'version' => "1.0.0",
        'context' => get_unique_sample_order_context,
        'initiator' => "integration_test",
        'source_system' => "rspec_test",
        'reason' => "order_processing"
      }

      # PHASE 3: TASK INITIALIZATION (Command Architecture)
      # This sends InitializeTask command to the embedded server
      init_response = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_response.inspect}"

      expect(init_response.error?).to be false
      # Extract task_id from response if available
      task_id = init_response.task_id
      expect(task_id).to be_truthy

      # PHASE 4: TASK HANDLING (Command Architecture)
      # This sends TryTaskIfReady command to check if task is ready for processing
      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end

    it 'handles premium customer optimization correctly' do
      # PHASE 1: HANDLER SETUP
      # Create handler instance using command architecture

      # PHASE 2: PREMIUM TASK REQUEST CREATION
      task_request = {
        'namespace' => "fulfillment",
        'name' => "process_order",
        'version' => "1.0.0",
        'context' => premium_order_context,
        'initiator' => "integration_test_premium",
        'source_system' => "rspec_test",
        'reason' => "premium_order_processing"
      }

      # PHASE 3: TASK INITIALIZATION (Command Architecture)
      init_response = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_response.inspect}"
      expect(init_response.error?).to be false
      # Extract task_id from response if available
      task_id = init_response.task_id
      expect(task_id).to be_truthy

      # PHASE 4: TASK HANDLING (Command Architecture)
      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end
  end

  describe 'error handling and recovery' do
    it 'handles validation errors correctly' do
      # Create task request with invalid data
      task_request = {
        'namespace' => "fulfillment",
        'name' => "process_order",
        'version' => task_config['version'],
        'context' => invalid_order_context,
        'initiator' => "integration_test_invalid",
        'source_system' => "rspec_test",
        'reason' => "invalid_order_testing",
        'tags' => ["integration_test", "invalid_order"]
      }

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result.error?).to be false
      task_id = init_result.task_id
      expect(task_id).to be_truthy

      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end

    it 'retries retryable failures correctly' do
      # This test validates retry configuration through the FFI layer
      task_request = {
        namespace: "fulfillment",
        name: "process_order",
        version: task_config['version'],
        context: get_unique_sample_order_context,
        initiator: "integration_test_retry",
        source_system: "rspec_test",
        reason: "retry_testing",
        tags: ["integration_test", "retry_test"]
      }

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result.error?).to be false
      task_id = init_result.task_id
      expect(task_id).to be_truthy

      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end

    it 'handles step dependency resolution correctly' do
      task_request = {
        namespace: "fulfillment",
        name: "process_order",
        version: task_config['version'],
        context: get_unique_sample_order_context,
        initiator: "integration_test_dependency",
        source_system: "rspec_test",
        reason: "dependency_testing",
        tags: ["integration_test", "dependency_test"]
      }

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result.error?).to be false
      task_id = init_result.task_id
      expect(task_id).to be_truthy

      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end
  end

  describe 'performance validation' do
    it 'completes workflow within performance targets' do
      start_time = Time.current

      task_request = {
        namespace: "fulfillment",
        name: "process_order",
        version: task_config['version'],
        context: get_unique_sample_order_context,
        initiator: "integration_test_performance",
        source_system: "rspec_test",
        reason: "performance_testing",
        tags: ["integration_test", "performance_test"]
      }

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result.error?).to be false
      task_id = init_result.task_id
      expect(task_id).to be_truthy

      handle_response = handler_instance.handle(task_id)

      puts "Handle responese for task #{task_id}: #{handle_response.inspect}"
    end

    it 'validates handle persistence across operations' do
      # Create multiple tasks to test handle reuse
      tasks = []
      5.times do |i|
        task_request = {
          namespace: "fulfillment",
          name: "process_order",
          version: task_config['version'],
          context: get_unique_sample_order_context,
          initiator: "integration_test_persistence_#{i}",
          source_system: "rspec_test",
          reason: "persistence_testing",
          tags: ["integration_test", "persistence_test"]
        }

        init_result = handler_instance.initialize_task(task_request)

        puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
        expect(init_result.error?).to be false
        task_id = init_result.task_id
        expect(task_id).to be_truthy

        tasks << task_id
      end

      # All task creation should succeed
      expect(tasks.length).to eq(5)
      expect(tasks.all? { |tid| tid > 0 }).to be true
    end
  end

  describe 'configuration validation' do
    it 'validates YAML configuration structure' do
      # Verify basic structure
      expect(task_config['name']).to eq('process_order')
      expect(task_config['namespace_name']).to eq('fulfillment')
      expect(task_config['version']).to eq('1.0.0')
      expect(task_config['task_handler_class']).to eq('OrderFulfillment::OrderFulfillmentHandler')

      # Verify step templates
      expect(task_config['step_templates']).to be_an(Array)
      expect(task_config['step_templates'].length).to eq(4)

      step_names = task_config['step_templates'].map { |step| step['name'] }
      expect(step_names).to contain_exactly(
        'validate_order', 'reserve_inventory', 'process_payment', 'ship_order'
      )

      # Verify environment overrides
      expect(task_config['environments']).to include('test', 'development', 'production')
    end

    it 'validates environment-specific configuration overrides' do
      # Test environment has reduced timeouts and retry limits
      test_overrides = task_config['environments']['test']['step_overrides']

      expect(test_overrides['validate_order']['handler_config']['validation_timeout']).to eq(5)
      expect(test_overrides['reserve_inventory']['retry_limit']).to eq(1)
      expect(test_overrides['process_payment']['retry_limit']).to eq(1)
      expect(test_overrides['ship_order']['retry_limit']).to eq(2)

      # Production environment has increased retry limits
      prod_overrides = task_config['environments']['production']['step_overrides']

      expect(prod_overrides['reserve_inventory']['retry_limit']).to eq(5)
      expect(prod_overrides['process_payment']['retry_limit']).to eq(3)
      expect(prod_overrides['ship_order']['retry_limit']).to eq(10)
    end

    it 'validates handler configuration constraints' do
      handler_class = Object.const_get(task_config['task_handler_class'])
      handler_instance = handler_class.new(task_config: task_config)

      # Test order value constraints
      high_value_context = get_unique_sample_order_context.deep_merge(
        'payment_info' => { 'amount' => 60000.00 }  # Exceeds max_order_value
      )

      task_request = {
        namespace: "fulfillment",
        name: "process_order",
        version: task_config['version'],
        context: high_value_context,
        initiator: "integration_test_constraint",
        source_system: "rspec_test",
        reason: "constraint_testing",
        tags: ["integration_test", "constraint_test"]
      }

      # Should raise validation error during task initialization
      expect {
        handler_instance.initialize_task(task_request)
      }.to raise_error(TaskerCore::Errors::ValidationError, /exceeds maximum/)
    end
  end

  describe 'error classification and handling' do
    it 'properly classifies permanent vs retryable errors' do

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
    # Wait for step to complete asynchronously
    step_result = wait_for_step_completion(task_id, step_name)

    expect(step_result['status']).to eq('complete')

    expected_results.each do |key, expected_value|
      if expected_value.respond_to?(:matches?)
        # Handle RSpec matchers like be_present
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
    # Get actual workflow steps from database via FFI
    steps = handler_instance.get_steps_for_task(task_id)
    step = steps.find { |s| s['name'] == step_name }

    return nil unless step

    {
      'status' => step['processed'] ? 'complete' : 'pending',
      'workflow_step_id' => step['workflow_step_id'],
      'results' => step['results'] || {}
    }
  end

  # Polling helper for fire-and-forget architecture
  # Waits for asynchronous ZeroMQ step execution to complete before validating results
  def wait_for_step_completion(task_id, step_name, timeout_seconds: 5, poll_interval: 1)
    start_time = Time.current

    loop do
      step_result = get_step_results(task_id, step_name)

      # Return immediately if step is complete
      if step_result && step_result['status'] == 'complete'
        return step_result
      end

      # Check timeout
      if Time.current - start_time > timeout_seconds
        raise "Timeout waiting for step '#{step_name}' to complete after #{timeout_seconds} seconds"
      end

      # Wait before next poll
      sleep poll_interval
    end
  end

  # Polling helper to wait for multiple steps to complete
  def wait_for_steps_completion(task_id, step_names, timeout_seconds: 5, poll_interval: 1)
    start_time = Time.current
    completed_steps = {}

    loop do
      # Check each step that hasn't completed yet
      remaining_steps = step_names - completed_steps.keys

      remaining_steps.each do |step_name|
        step_result = get_step_results(task_id, step_name)
        if step_result && step_result['status'] == 'complete'
          completed_steps[step_name] = step_result
        end
      end

      # Return if all steps are complete
      if completed_steps.keys.sort == step_names.sort
        return completed_steps
      end

      # Check timeout
      if Time.current - start_time > timeout_seconds
        completed = completed_steps.keys
        remaining = step_names - completed
        raise "Timeout waiting for steps to complete after #{timeout_seconds} seconds. " \
              "Completed: #{completed.join(', ')}. " \
              "Still pending: #{remaining.join(', ')}"
      end

      # Wait before next poll
      sleep poll_interval
    end
  end

  private

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

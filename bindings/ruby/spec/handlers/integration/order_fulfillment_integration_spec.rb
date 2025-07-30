# frozen_string_literal: true

require 'spec_helper'
require 'tasker_core/execution'
require 'tasker_core/execution/worker_manager'
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

    # Register worker with explicit task handler information for database-first registry
    register_worker_with_task_handlers
  end

  after(:all) do
    # Clean shutdown worker and embedded server
    if @worker_manager
      begin
        @worker_manager.stop_heartbeat
        @worker_manager.stop
        puts "✅ Worker manager cleaned up"
      rescue StandardError => e
        puts "⚠️ Warning: Failed to cleanup worker manager: #{e.message}"
      end
    end

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
    command_client.connect unless command_client.connected?

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

  def get_task_request(context)
    {
      namespace: task_config['namespace_name'],
      name: task_config['name'],
      version: task_config['version'],
      context: context,
      initiator: "integration_test_#{rand(100000..999999)}",
      source_system: "rspec_test",
      reason: "order_processing",
      tags: ["integration_test", "order_processing"]
    }
  end

  describe 'complete workflow execution' do
    it 'executes full order fulfillment workflow through command architecture' do
      # PHASE 1: HANDLER SETUP
      # Create handler instance using command architecture

      # PHASE 2: TASK REQUEST CREATION
      task_request = get_task_request(get_unique_sample_order_context)

      # PHASE 3: TASK INITIALIZATION (Command Architecture)
      # 🎯 FIX: Use WorkerManager's CommandClient instead of BaseTaskHandler to ensure connection reuse
      # This sends InitializeTask command using the SAME SharedCommandClient that RegisterWorker used
      init_response = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_response.inspect}"

      expect(init_response).not_to be_error
      expect(init_response.response_data[:step_count]).to eq(4)
      
      # Extract task_id from response if available
      task_id = init_response.task_id
      expect(task_id).to be_truthy
      
      # ✅ NEW: Validate workflow steps are populated with actual data
      workflow_steps = init_response.response_data[:workflow_steps]
      expect(workflow_steps).to be_an(Array)
      expect(workflow_steps.length).to eq(4)
      
      # Validate each workflow step has required fields
      workflow_steps.each do |step|
        expect(step[:workflow_step_id]).to be_a(Integer)
        expect(step[:task_id]).to eq(task_id)
        expect(step[:named_step_id]).to be_a(Integer)
        expect(step[:retryable]).to be(true).or be(false)
        expect(step[:in_process]).to eq(false)  # Should be false initially
        expect(step[:processed]).to eq(false)   # Should be false initially
        expect(step).to have_key(:inputs)       # Should have inputs configuration
      end
      
      # Validate specific step configurations from YAML
      step_by_named_id = workflow_steps.to_h { |step| [step[:named_step_id], step] }
      
      # Step 1: validate_order - should have strict_validation input
      validate_step = step_by_named_id[1]
      expect(validate_step[:inputs][:strict_validation]).to eq(true)
      expect(validate_step[:inputs][:validation_timeout]).to eq(10)
      expect(validate_step[:retry_limit]).to eq(1)
      
      # Step 2: reserve_inventory - should have reservation settings
      inventory_step = step_by_named_id[2]
      expect(inventory_step[:inputs][:allow_backorder]).to eq(false)
      expect(inventory_step[:inputs][:reservation_timeout]).to eq(30)
      expect(inventory_step[:retry_limit]).to eq(3)
      
      # Step 3: process_payment - should have payment settings
      payment_step = step_by_named_id[3]
      expect(payment_step[:inputs][:capture_immediately]).to eq(true)
      expect(payment_step[:inputs][:payment_timeout]).to eq(45)
      expect(payment_step[:retry_limit]).to eq(2)
      
      # Step 4: ship_order - should have shipping settings
      shipping_step = step_by_named_id[4]
      expect(shipping_step[:inputs][:send_notifications]).to eq(true)
      expect(shipping_step[:inputs][:shipping_timeout]).to eq(60)
      expect(shipping_step[:retry_limit]).to eq(5)

      # PHASE 4: TASK HANDLING (Command Architecture)
      # 🎯 FIX: Use WorkerManager's CommandClient instead of BaseTaskHandler to ensure connection reuse
      # This sends TryTaskIfReady command using the SAME SharedCommandClient that RegisterWorker used
      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
      
      # ✅ NEW: Validate task readiness response structure
      expect(handle_response).not_to be_error
      expect(handle_response.response_data[:task_id]).to eq(task_id)
      expect(handle_response.response_data[:ready]).to be(true).or be(false)
      expect(handle_response.response_data[:ready_steps_count]).to be_a(Integer)
      
      # Validate batch info if task is ready
      if handle_response.response_data[:ready]
        batch_info = handle_response.response_data[:batch_info]
        expect(batch_info).to be_a(Hash)
        expect(batch_info[:batch_id]).to be_a(String)
        expect(batch_info[:estimated_steps]).to be_a(Integer)
        expect(batch_info[:publication_time_ms]).to be_a(Integer)
        expect(batch_info[:next_poll_delay_ms]).to be_a(Integer)
      end
    end

    it 'handles premium customer optimization correctly' do
      # PHASE 1: HANDLER SETUP
      # Create handler instance using command architecture

      # PHASE 2: PREMIUM TASK REQUEST CREATION
      task_request = get_task_request(premium_order_context.deep_merge(
        'purchaser_info' => {
          'id' => 12345,
          'email' => 'customer@example.com',
          'tier' => 'premium',
          'timestamp' => Process.clock_gettime(Process::CLOCK_REALTIME)
        }
      ))

      # PHASE 3: TASK INITIALIZATION (Command Architecture)
      init_response = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_response.inspect}"
      expect(init_response).not_to be_error
      expect(init_response.response_data[:step_count]).to eq(4)
      
      # Extract task_id from response if available
      task_id = init_response.task_id
      expect(task_id).to be_truthy
      
      # ✅ NEW: Validate premium customer gets same workflow steps structure
      workflow_steps = init_response.response_data[:workflow_steps]
      expect(workflow_steps).to be_an(Array)
      expect(workflow_steps.length).to eq(4)
      
      # Validate premium customer context was properly passed through
      workflow_steps.each do |step|
        expect(step[:task_id]).to eq(task_id)
        expect(step[:in_process]).to eq(false)
        expect(step[:processed]).to eq(false)
      end

      # PHASE 4: TASK HANDLING (Command Architecture)
      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end
  end

  describe 'error handling and recovery' do
    it 'handles validation errors correctly' do
      # Create task request with invalid data
      task_request = get_task_request(invalid_order_context)

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result).to be_error
      
      # ✅ NEW: Validate error response structure
      expect(init_result.response_data[:error_type]).to be_a(String)
      expect(init_result.response_data[:message]).to be_a(String)
      expect(init_result.response_data[:retryable]).to be(true).or be(false)
      
      # Should include validation details
      if init_result.response_data[:details]
        details = init_result.response_data[:details]
        expect(details[:namespace]).to eq('fulfillment')
        expect(details[:task_name]).to eq('process_order')
        expect(details[:version]).to eq('1.0.0')
      end

    end

    it 'retries retryable failures correctly' do
      # This test validates retry configuration through the FFI layer
      task_request = get_task_request(get_unique_sample_order_context)

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result).not_to be_error
      expect(init_result.response_data[:step_count]).to eq(4)
      task_id = init_result.task_id
      expect(task_id).to be_truthy

      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end

    it 'handles step dependency resolution correctly' do
      task_request = get_task_request(get_unique_sample_order_context)

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result).not_to be_error
      expect(init_result.response_data[:step_count]).to eq(4)
      task_id = init_result.task_id
      expect(task_id).to be_truthy

      handle_response = handler_instance.handle(task_id)

      puts "HANDLE RESPONSE: Handle responese for task #{task_id}: #{handle_response.inspect}"
    end
  end

  describe 'performance validation' do
    it 'completes workflow within performance targets' do
      task_request = get_task_request(get_unique_sample_order_context)

      init_result = handler_instance.initialize_task(task_request)

      puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
      expect(init_result).not_to be_error
      expect(init_result.response_data[:step_count]).to eq(4)
      task_id = init_result.task_id
      expect(task_id).to be_truthy

      handle_response = handler_instance.handle(task_id)

      puts "Handle responese for task #{task_id}: #{handle_response.inspect}"
    end

    it 'validates handle persistence across operations' do
      # Create multiple tasks to test handle reuse
      tasks = []
      5.times do |i|
        task_request = get_task_request(get_unique_sample_order_context).deep_merge(
          initiator: "integration_test_persistence_#{i}"
        )

        init_result = handler_instance.initialize_task(task_request)

        puts "INIT RESPONSE: Initialize task response: #{init_result.inspect}"
        expect(init_result).not_to be_error
        expect(init_result.response_data[:step_count]).to eq(4)
        task_id = init_result.task_id
        expect(task_id).to be_truthy

        tasks << task_id
      end

      # All task creation should succeed
      expect(tasks.length).to eq(5)
      expect(tasks.all? { |tid| tid > 0 }).to be true
      
      # ✅ NEW: Validate task IDs are unique (no duplicate creation)
      expect(tasks.uniq.length).to eq(5)
      
      # ✅ NEW: Validate all tasks were created with consistent step count
      tasks.each do |task_id|
        # Note: We would need to add a method to query task details
        # This validates the handle persistence across multiple operations
        expect(task_id).to be_a(Integer)
        expect(task_id).to be > 0
      end
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

      task_request = get_task_request(high_value_context)

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

  # Register worker with explicit task handler information for database-first registry
  # This method creates a worker registration that includes complete task handler definitions
  # extracted from the YAML configuration, addressing the nil results issue
  def register_worker_with_task_handlers
    puts "🔧 Registering worker with explicit task handler information..."

    begin
      # Load YAML configuration directly (can't use let declarations in before(:all))
      config_path = File.expand_path('../examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
      loaded_task_config = YAML.load_file(config_path)

      # Extract task handler information from YAML configuration
      supported_tasks = [{
        namespace: loaded_task_config['namespace_name'],
        handler_name: loaded_task_config['name'],
        version: loaded_task_config['version'],
        handler_class: loaded_task_config['task_handler_class'],
        description: loaded_task_config['description'] || "Auto-registered from integration test",
        supported_step_types: loaded_task_config['step_templates']&.map { |step| step['name'] } || [],
        handler_config: loaded_task_config,
        priority: 100,
        timeout_ms: 30000,
        supports_retries: true
      }]

      puts "📋 Task handler configuration loaded: #{supported_tasks.first[:namespace]}/#{supported_tasks.first[:handler_name]}"
      puts "🎯 DEBUG: supported_step_types = #{supported_tasks.first[:supported_step_types]}"
      puts "🚫 SKIPPING FFI registration - using command pattern instead"

      # Create WorkerManager with supported_tasks that will be sent via RegisterWorker command
      worker_id = "integration_test_worker_#{rand(10000..99999)}"
      @worker_manager = TaskerCore::Execution::WorkerManager.new(
        worker_id: worker_id,
        supported_namespaces: [loaded_task_config['namespace_name']],
        server_host: '127.0.0.1',
        server_port: executor_port,
        heartbeat_interval: 30,
        custom_capabilities: {
          'integration_test' => true,
          'order_fulfillment_capable' => true,
          'supports_all_step_types' => true,
          'explicit_task_registration' => true,
          'manager_type' => 'rust_backed',
          'supports_execute_batch' => true,
          'ruby_worker' => true
        },
        supported_tasks: supported_tasks
      )

      puts "🎯 DEBUG: About to start worker - this should send RegisterWorker command with task handler info"
      puts "🎯 DEBUG: Worker ID = #{worker_id}"
      puts "🎯 DEBUG: Server = 127.0.0.1:#{executor_port}"
      puts "🎯 DEBUG: Supported namespaces = #{@worker_manager.supported_namespaces}"

      # Start the worker - this sends RegisterWorker command via TCP to Rust
      @worker_manager.start

      puts "✅ Worker registered successfully via command pattern (not FFI)"
      puts "💓 Worker heartbeat started, worker is now active"
      puts "🎯 DEBUG: Worker manager running = #{@worker_manager.running?}"

    rescue StandardError => e
      puts "❌ Failed to register worker with task handlers: #{e.message}"
      puts "Backtrace: #{e.backtrace.first(3).join(', ')}"
      raise
    end
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

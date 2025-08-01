# frozen_string_literal: true

require 'spec_helper'
require 'tasker_core/execution'
require 'tasker_core/execution/worker_manager'
require 'tasker_core/execution/command_listener'
require 'tasker_core/execution/batch_execution_handler'
require 'tasker_core/embedded_server'
require 'timeout'

# Load OrderFulfillment example handlers
require_relative '../examples/order_fulfillment/handlers/order_fulfillment_handler'
require_relative '../examples/order_fulfillment/step_handlers/validate_order_handler'
require_relative '../examples/order_fulfillment/step_handlers/reserve_inventory_handler'
require_relative '../examples/order_fulfillment/step_handlers/process_payment_handler'
require_relative '../examples/order_fulfillment/step_handlers/ship_order_handler'

require_relative '../../orchestration_helpers'
include TaskerCore::OrchestrationHelpers


RSpec.describe 'Order Fulfillment Command-Based Integration', type: :integration do
  let(:config_path) { File.expand_path('../examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__) }
  let(:task_config) { YAML.load_file(config_path) }
  let(:handler_class) { Object.const_get(task_config['task_handler_class']) }
  let(:handler_instance) { handler_class.new(task_config: task_config) }

  # Embedded server setup following command-based pattern
  before(:all) do
    start_embedded_server
    sleep 1 # wait for server to start
    raise "Server not running" unless @embedded_server.running?
    start_worker_manager
  end

  after(:all) do
    stop_worker_manager
    stop_embedded_server
  end

  let(:command_client) do
    @worker_manager.command_client
  end

  let(:command_listener) do
    @worker_manager.command_listener
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

  describe 'complete end-to-end command architecture workflow' do
    it 'validates full order fulfillment workflow: task initialization, batch processing, and result aggregation' do
      # PHASE 1: TASK REQUEST CREATION
      # Create a unique order context to avoid task ID conflicts
      task_request = get_task_request(get_unique_sample_order_context)
      # PHASE 2: TASK INITIALIZATION (Command Architecture)
      # Send InitializeTask command using SharedCommandClient that registered worker used
      puts "🕐 [#{Time.now.strftime('%H:%M:%S.%3N')}] Starting InitializeTask command..."
      start_time = Time.now
      init_response = handler_instance.initialize_task(task_request)
      elapsed_time = ((Time.now - start_time) * 1000).round(2)
      puts "✅ [#{Time.now.strftime('%H:%M:%S.%3N')}] InitializeTask completed in #{elapsed_time}ms"

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

      # PHASE 3: TASK READINESS CHECK (Command Architecture)
      # Send TryTaskIfReady command to check if task has viable steps ready for execution
      puts "🕐 [#{Time.now.strftime('%H:%M:%S.%3N')}] Starting TryTaskIfReady command..."
      start_time = Time.now
      handle_response = handler_instance.handle(task_id)
      elapsed_time = ((Time.now - start_time) * 1000).round(2)
      puts "✅ [#{Time.now.strftime('%H:%M:%S.%3N')}] TryTaskIfReady completed in #{elapsed_time}ms"

      # ✅ NEW: Validate task readiness response structure
      expect(handle_response).not_to be_error
      expect(handle_response.response_data[:task_id]).to eq(task_id)
      expect(handle_response.response_data[:ready]).to be(true).or be(false)
      expect(handle_response.response_data[:ready_steps_count]).to be_a(Integer)

      # Task should be ready for processing - this is required for integration success
      expect(handle_response.response_data[:ready]).to be(true), "Task should be ready for batch processing"

      # Validate batch info structure when task is ready
      batch_info = handle_response.response_data[:batch_info]
      expect(batch_info).to be_a(Hash)
      expect(batch_info[:batch_id]).to be_a(String)
      expect(batch_info[:estimated_steps]).to be_a(Integer)
      expect(batch_info[:publication_time_ms]).to be_a(Integer)
      expect(batch_info[:next_poll_delay_ms]).to be_a(Integer)

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

      expect(handle_response).not_to be_error
      expect(handle_response.response_data[:task_id]).to eq(task_id)
      expect(handle_response.response_data[:ready]).to be(true)
      expect(handle_response.response_data[:ready_steps_count]).to be_a(Integer)
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
end

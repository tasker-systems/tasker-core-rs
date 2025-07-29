# frozen_string_literal: true

require 'spec_helper'
require 'json'
require 'socket'

RSpec.describe 'ExecuteBatch Integration' do
  let(:worker_id) { 'test_execute_batch_worker' }
  let(:supported_namespaces) { ['api_tests', 'orders'] }
  let(:logger) do
    TaskerCore::Logging::Logger.instance
  end

  # Shared instances to avoid port conflicts
  before(:all) do
    @shared_worker_manager = TaskerCore::Execution::WorkerManager.new(
      worker_id: 'test_execute_batch_worker_shared',
      supported_namespaces: ['api_tests', 'orders'],
      max_concurrent_steps: 5
    )
    @shared_command_listener = @shared_worker_manager.command_listener
    @shared_batch_handler = @shared_worker_manager.batch_handler
  end

  after(:all) do
    @shared_worker_manager&.stop if @shared_worker_manager&.running?
    @shared_command_listener&.stop if @shared_command_listener&.running?
  end

  # Use shared instances for most tests, create fresh ones only when needed
  let(:worker_manager) { @shared_worker_manager }
  let(:command_listener) { @shared_command_listener }
  let(:batch_handler) { @shared_batch_handler }

  # Sample ExecuteBatch command data
  let(:execute_batch_command_data) do
    {
      command_type: 'ExecuteBatch',
      command_id: 'test_batch_cmd_123',
      correlation_id: nil,
      metadata: {
        timestamp: Time.now.utc.iso8601,
        source: {
          type: 'RustOrchestrator',
          data: { id: 'test_orchestrator' }
        }
      },
      payload: {
        type: 'ExecuteBatch',
        data: {
          batch_id: 'test_batch_456',
          steps: [
            {
              step_id: 1001,
              task_id: 2001,
              step_name: 'validate_order',
              handler_class: 'TestValidateOrderHandler',
              handler_config: { 'timeout' => 30 },
              task_context: { 'order_id' => 12345, 'customer_id' => 67890 },
              previous_results: {},
              metadata: {
                attempt: 1,
                retry_limit: 3,
                timeout_ms: 30000,
                priority: 'normal'
              }
            },
            {
              step_id: 1002,
              task_id: 2001,
              step_name: 'process_payment',
              handler_class: 'TestProcessPaymentHandler',
              handler_config: { 'payment_processor' => 'stripe' },
              task_context: { 'order_id' => 12345, 'customer_id' => 67890 },
              previous_results: { 'validate_order' => { 'valid' => true } },
              metadata: {
                attempt: 1,
                retry_limit: 3,
                timeout_ms: 30000,
                priority: 'normal'
              }
            }
          ],
          task_template: {
            name: 'order_processing',
            module_namespace: nil,
            task_handler_class: 'OrderProcessingHandler',
            namespace_name: 'orders',
            version: '1.0.0',
            description: 'Process customer orders',
            default_dependent_system: nil,
            named_steps: ['validate_order', 'process_payment'],
            schema: nil,
            step_templates: [
              {
                name: 'validate_order',
                description: 'Validate order details',
                handler_class: 'TestValidateOrderHandler',
                handler_config: { 'timeout' => 30 },
                depends_on: [],
                retryable: true,
                retry_limit: 3,
                timeout_seconds: 30
              },
              {
                name: 'process_payment',
                description: 'Process payment for order',
                handler_class: 'TestProcessPaymentHandler',
                handler_config: { 'payment_processor' => 'stripe' },
                depends_on: ['validate_order'],
                retryable: true,
                retry_limit: 3,
                timeout_seconds: 30
              }
            ],
            environments: nil,
            custom_events: nil
          }
        }
      }
    }
  end

  class OrderProcessingHandler < TaskerCore::TaskHandler::Base; end

  # Test step handlers
  class TestValidateOrderHandler < TaskerCore::StepHandler::Base
    def handle(task, sequence, step)
      logger.info("Validating order #{task.order_id} for customer #{task.customer_id}")

      # Simulate validation
      if task.order_id && task.customer_id
        { 'valid' => true, 'order_total' => 99.99, 'validated_at' => Time.now.iso8601 }
      else
        raise TaskerCore::Errors::PermanentError, 'Invalid order or customer ID'
      end
    end
  end

  class TestProcessPaymentHandler < TaskerCore::StepHandler::Base
    def handle(task, sequence, step)
      logger.info("Processing payment for order #{task.order_id}")

      # Check if validation passed
      validate_result = step.previous_results['validate_order']
      unless validate_result && validate_result['valid']
        raise TaskerCore::Errors::PermanentError, 'Cannot process payment: order validation failed'
      end

      # Simulate payment processing
      {
        'payment_status' => 'completed',
        'transaction_id' => 'txn_12345',
        'amount_charged' => validate_result['order_total'],
        'processed_at' => Time.now.iso8601
      }
    end
  end

  describe 'BatchExecutionHandler' do
    let(:execute_batch_command) do
      TaskerCore::Types::ResponseFactory.create_response(execute_batch_command_data)
    end

    it 'processes ExecuteBatch command successfully' do
      expect(execute_batch_command).to be_a(TaskerCore::Types::ExecutionTypes::ExecuteBatchResponse)
      expect(execute_batch_command.execute_batch?).to be true
      expect(execute_batch_command.batch_id).to eq('test_batch_456')
      expect(execute_batch_command.steps.size).to eq(2)
      expect(execute_batch_command.step_count).to eq(2)

      result = batch_handler.handle(execute_batch_command)

      expect(result).to be_a(Hash)
      expect(result[:command_type]).to eq('BatchExecuted')
      expect(result[:payload][:type]).to eq('BatchExecuted')

      batch_data = result[:payload][:data]
      expect(batch_data[:batch_id]).to eq('test_batch_456')
      expect(batch_data[:steps_processed]).to eq(2)
      expect(batch_data[:steps_succeeded]).to eq(2)
      expect(batch_data[:steps_failed]).to eq(0)
      expect(batch_data[:step_summaries]).to be_an(Array)
      expect(batch_data[:step_summaries].size).to eq(2)

      # Check individual step results
      validate_step = batch_data[:step_summaries].find { |s| s[:step_name] == 'validate_order' }
      expect(validate_step[:status]).to eq('completed')
      expect(validate_step[:result]['valid']).to be true

      payment_step = batch_data[:step_summaries].find { |s| s[:step_name] == 'process_payment' }
      expect(payment_step[:status]).to eq('completed')
      expect(payment_step[:result]['payment_status']).to eq('completed')
    end

    it 'handles step failures gracefully' do
      # Create command with invalid step handler
      invalid_command_data = execute_batch_command_data.dup
      invalid_command_data[:payload][:data][:steps][0][:handler_class] = 'NonExistentHandler'

      invalid_command = TaskerCore::Types::ResponseFactory.create_response(invalid_command_data)
      result = batch_handler.handle(invalid_command)

      expect(result[:command_type]).to eq('BatchExecuted')

      batch_data = result[:payload][:data]
      expect(batch_data[:steps_processed]).to eq(2)
      expect(batch_data[:steps_succeeded]).to eq(1)  # Only process_payment should succeed
      expect(batch_data[:steps_failed]).to eq(1)     # validate_order should fail

      # Check that the failed step has error information
      failed_step = batch_data[:step_summaries].find { |s| s[:step_name] == 'validate_order' }
      expect(failed_step[:status]).to eq('failed')
      expect(failed_step[:error]).to be_present
    end
  end
end

# frozen_string_literal: true

require 'spec_helper'
require 'tasker_core/messaging/batch_queue_worker'
require 'tasker_core/messaging/pgmq_client'
require 'tasker_core/types/batch_message'
require 'tasker_core/types/batch_result_message'
require 'tasker_core/step_handler/example_handlers'

RSpec.describe TaskerCore::Messaging::BatchQueueWorker do
  let(:namespace) { 'test_namespace' }
  let(:config) do
    {
      polling_interval_seconds: 0.1, # Fast polling for tests
      batch_size: 3,
      visibility_timeout_seconds: 30,
      max_concurrent_steps: 2,
      step_timeout_seconds: 5,
      error_retry_delay_seconds: 0.1,
      results_queue_name: 'test_orchestration_batch_results'
    }
  end
  let(:worker) { described_class.new(namespace: namespace, config: config) }
  let(:mock_pgmq_client) { instance_double(TaskerCore::Messaging::PgmqClient) }

  before do
    allow(TaskerCore::Messaging::PgmqClient).to receive(:new).and_return(mock_pgmq_client)
    allow(mock_pgmq_client).to receive(:create_queue)
    allow(mock_pgmq_client).to receive(:read_messages).and_return([])
    allow(mock_pgmq_client).to receive(:delete_message)
    allow(mock_pgmq_client).to receive(:archive_message)
    allow(mock_pgmq_client).to receive(:send_message)
  end

  describe '#initialize' do
    it 'initializes with namespace and config' do
      expect(worker.namespace).to eq(namespace)
      expect(worker.config[:batch_size]).to eq(3)
      expect(worker.config[:max_concurrent_steps]).to eq(2)
    end

    it 'uses default config values when not provided' do
      default_worker = described_class.new(namespace: namespace)
      expect(default_worker.config[:polling_interval_seconds]).to eq(1)
      expect(default_worker.config[:batch_size]).to eq(5)
      expect(default_worker.config[:max_concurrent_steps]).to eq(4)
    end

    it 'validates required configuration' do
      expect {
        described_class.new(namespace: nil)
      }.to raise_error(ArgumentError, /namespace cannot be blank/)

      expect {
        described_class.new(namespace: namespace, config: { batch_size: 0 })
      }.to raise_error(ArgumentError, /batch_size must be positive/)
    end
  end

  describe '#statistics' do
    it 'returns worker statistics' do
      stats = worker.statistics
      
      expect(stats).to include(
        namespace: namespace,
        running: false,
        shutdown_requested: false,
        batch_queue_name: "#{namespace}_batch_queue",
        results_queue_name: config[:results_queue_name],
        worker_id: be_a(String)
      )
    end
  end

  describe 'message processing' do
    let(:batch_message) do
      TaskerCore::Types::BatchMessage.new(
        batch_id: 12345,
        task_id: 67890,
        namespace: namespace,
        task_name: 'test_task',
        task_version: '1.0.0',
        steps: [
          TaskerCore::Types::BatchStep.new(
            step_id: 101,
            sequence: 1,
            step_name: 'validate_order',
            step_payload: { 'order_id' => 'ORD-001', 'customer_id' => 'CUST-001' },
            step_metadata: {}
          ),
          TaskerCore::Types::BatchStep.new(
            step_id: 102,
            sequence: 2,
            step_name: 'reserve_inventory',
            step_payload: { 'items' => [{ 'sku' => 'ITEM-A', 'quantity' => 2 }] },
            step_metadata: {}
          )
        ],
        metadata: TaskerCore::Types::BatchMetadata.new(
          batch_created_at: Time.now.utc.iso8601,
          timeout_seconds: 300,
          retry_policy: 'exponential_backoff',
          max_retries: 3,
          retry_count: 0
        )
      )
    end

    let(:message_data) do
      {
        msg_id: 999,
        message: batch_message.to_hash
      }
    end

    describe 'successful batch processing' do
      before do
        allow(mock_pgmq_client).to receive(:read_messages)
          .with("#{namespace}_batch_queue", anything)
          .and_return([message_data])
      end

      it 'processes batch messages and publishes results' do
        # Expect message deletion after successful processing
        expect(mock_pgmq_client).to receive(:delete_message)
          .with("#{namespace}_batch_queue", 999)

        # Expect result publication
        expect(mock_pgmq_client).to receive(:send_message) do |queue_name, result_message|
          expect(queue_name).to eq(config[:results_queue_name])
          expect(result_message).to include(
            batch_id: 12345,
            task_id: 67890,
            namespace: namespace,
            batch_status: 'Success'
          )
          expect(result_message[:step_results]).to have(2).items
          expect(result_message[:metadata][:successful_steps]).to eq(2)
          expect(result_message[:metadata][:failed_steps]).to eq(0)
        end

        # Process one batch and then stop
        allow(mock_pgmq_client).to receive(:read_messages).and_return([message_data], [])
        
        # Start worker in a thread and stop it quickly
        worker_thread = Thread.new do
          worker.start
        end

        sleep(0.2) # Let it process one batch
        worker.stop
        worker_thread.join(1.0)
      end
    end

    describe 'failed step processing' do
      let(:failing_batch_message) do
        TaskerCore::Types::BatchMessage.new(
          batch_id: 12346,
          task_id: 67891,
          namespace: namespace,
          task_name: 'test_task',
          task_version: '1.0.0',
          steps: [
            TaskerCore::Types::BatchStep.new(
              step_id: 201,
              sequence: 1,
              step_name: 'nonexistent_step', # This will fail
              step_payload: {},
              step_metadata: {}
            )
          ],
          metadata: TaskerCore::Types::BatchMetadata.new(
            batch_created_at: Time.now.utc.iso8601,
            timeout_seconds: 300,
            retry_policy: 'none',
            max_retries: 0,
            retry_count: 0
          )
        )
      end

      let(:failing_message_data) do
        {
          msg_id: 998,
          message: failing_batch_message.to_hash
        }
      end

      before do
        allow(mock_pgmq_client).to receive(:read_messages)
          .with("#{namespace}_batch_queue", anything)
          .and_return([failing_message_data])
      end

      it 'handles step failures and reports them' do
        # Expect message deletion after processing (even with failures)
        expect(mock_pgmq_client).to receive(:delete_message)
          .with("#{namespace}_batch_queue", 998)

        # Expect result publication with failure status
        expect(mock_pgmq_client).to receive(:send_message) do |queue_name, result_message|
          expect(queue_name).to eq(config[:results_queue_name])
          expect(result_message).to include(
            batch_id: 12346,
            task_id: 67891,
            namespace: namespace,
            batch_status: 'Failed'
          )
          expect(result_message[:step_results]).to have(1).item
          expect(result_message[:step_results][0][:status]).to eq('Failed')
          expect(result_message[:step_results][0][:error]).to be_present
          expect(result_message[:metadata][:successful_steps]).to eq(0)
          expect(result_message[:metadata][:failed_steps]).to eq(1)
        end

        # Process one batch and then stop
        allow(mock_pgmq_client).to receive(:read_messages).and_return([failing_message_data], [])
        
        # Start worker in a thread and stop it quickly
        worker_thread = Thread.new do
          worker.start
        end

        sleep(0.2) # Let it process one batch
        worker.stop
        worker_thread.join(1.0)
      end
    end

    describe 'malformed message handling' do
      let(:malformed_message_data) do
        {
          msg_id: 997,
          message: { invalid: 'message format' }
        }
      end

      before do
        allow(mock_pgmq_client).to receive(:read_messages)
          .with("#{namespace}_batch_queue", anything)
          .and_return([malformed_message_data])
      end

      it 'archives malformed messages' do
        # Expect message archiving for malformed messages
        expect(mock_pgmq_client).to receive(:archive_message)
          .with("#{namespace}_batch_queue", 997)

        # Should not publish results for malformed messages
        expect(mock_pgmq_client).not_to receive(:send_message)

        # Process one batch and then stop
        allow(mock_pgmq_client).to receive(:read_messages).and_return([malformed_message_data], [])
        
        # Start worker in a thread and stop it quickly
        worker_thread = Thread.new do
          worker.start
        end

        sleep(0.2) # Let it process one batch
        worker.stop
        worker_thread.join(1.0)
      end
    end
  end

  describe 'step handler integration' do
    it 'finds and executes registered step handlers' do
      # Test ValidateOrderHandler
      handler = worker.send(:find_step_handler, 'validate_order')
      expect(handler).to eq(TaskerCore::StepHandler::ValidateOrderHandler)

      # Test ReserveInventoryHandler
      handler = worker.send(:find_step_handler, 'reserve_inventory')
      expect(handler).to eq(TaskerCore::StepHandler::ReserveInventoryHandler)

      # Test nonexistent handler
      handler = worker.send(:find_step_handler, 'nonexistent_step')
      expect(handler).to be_nil
    end

    it 'executes step handlers with proper context' do
      batch_message = TaskerCore::Types::BatchMessage.new(
        batch_id: 12345,
        task_id: 67890,
        namespace: namespace,
        task_name: 'test_task',
        task_version: '1.0.0',
        steps: [],
        metadata: TaskerCore::Types::BatchMetadata.new(
          batch_created_at: Time.now.utc.iso8601,
          timeout_seconds: 300,
          retry_policy: 'none',
          max_retries: 0,
          retry_count: 0
        )
      )

      step = TaskerCore::Types::BatchStep.new(
        step_id: 101,
        sequence: 1,
        step_name: 'validate_order',
        step_payload: { 'order_id' => 'ORD-001', 'customer_id' => 'CUST-001' },
        step_metadata: {}
      )

      result = worker.send(:process_single_step, batch_message, step)

      expect(result).to include(
        step_id: 101,
        status: 'Success',
        output: be_a(Hash),
        error: nil,
        execution_duration_ms: be_a(Integer)
      )

      expect(result[:output]).to include(
        order_id: 'ORD-001',
        status: 'validated'
      )
    end
  end

  describe 'batch status determination' do
    it 'determines Success when all steps succeed' do
      step_results = [
        { step_id: 1, status: 'Success' },
        { step_id: 2, status: 'Success' }
      ]
      
      status = worker.send(:determine_batch_status, step_results)
      expect(status).to eq('Success')
    end

    it 'determines Failed when all steps fail' do
      step_results = [
        { step_id: 1, status: 'Failed' },
        { step_id: 2, status: 'Failed' }
      ]
      
      status = worker.send(:determine_batch_status, step_results)
      expect(status).to eq('Failed')
    end

    it 'determines PartialSuccess when some steps fail' do
      step_results = [
        { step_id: 1, status: 'Success' },
        { step_id: 2, status: 'Failed' }
      ]
      
      status = worker.send(:determine_batch_status, step_results)
      expect(status).to eq('PartialSuccess')
    end
  end

  describe 'error classification' do
    it 'classifies different error types' do
      expect(worker.send(:classify_error, ArgumentError.new)).to eq('VALIDATION_ERROR')
      expect(worker.send(:classify_error, NoMethodError.new)).to eq('VALIDATION_ERROR')
      expect(worker.send(:classify_error, Timeout::Error.new)).to eq('TIMEOUT_ERROR')
      expect(worker.send(:classify_error, StandardError.new)).to eq('EXECUTION_ERROR')
      expect(worker.send(:classify_error, Exception.new)).to eq('UNKNOWN_ERROR')
    end
  end

  describe 'concurrent step processing' do
    let(:multi_step_batch) do
      TaskerCore::Types::BatchMessage.new(
        batch_id: 12347,
        task_id: 67892,
        namespace: namespace,
        task_name: 'concurrent_test',
        task_version: '1.0.0',
        steps: [
          TaskerCore::Types::BatchStep.new(
            step_id: 301,
            sequence: 1,
            step_name: 'generic_test',
            step_payload: { 'delay_ms' => 100 },
            step_metadata: {}
          ),
          TaskerCore::Types::BatchStep.new(
            step_id: 302,
            sequence: 2,
            step_name: 'generic_test',
            step_payload: { 'delay_ms' => 100 },
            step_metadata: {}
          ),
          TaskerCore::Types::BatchStep.new(
            step_id: 303,
            sequence: 3,
            step_name: 'generic_test',
            step_payload: { 'delay_ms' => 100 },
            step_metadata: {}
          )
        ],
        metadata: TaskerCore::Types::BatchMetadata.new(
          batch_created_at: Time.now.utc.iso8601,
          timeout_seconds: 300,
          retry_policy: 'none',
          max_retries: 0,
          retry_count: 0
        )
      )
    end

    it 'processes steps concurrently when configured' do
      start_time = Time.now
      
      results = worker.send(:process_batch_steps_concurrently, multi_step_batch)
      
      end_time = Time.now
      total_time = (end_time - start_time)
      
      # With 3 steps of 100ms each and max_concurrent_steps=2,
      # concurrent execution should be faster than sequential (300ms)
      expect(total_time).to be < 0.3 # Should be around 200ms with concurrency
      expect(results).to have(3).items
      expect(results.all? { |r| r[:status] == 'Success' }).to be true
    end

    it 'falls back to sequential processing on concurrent errors' do
      # Mock Concurrent::FixedThreadPool to raise an error
      allow(Concurrent::FixedThreadPool).to receive(:new).and_raise(StandardError.new('Pool error'))
      
      results = worker.send(:process_batch_steps_concurrently, multi_step_batch)
      
      expect(results).to have(3).items
      expect(results.all? { |r| r[:status] == 'Success' }).to be true
    end
  end
end
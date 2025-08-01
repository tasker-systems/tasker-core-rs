# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'PGMQ Architecture Integration', type: :integration do
  let(:namespace) { 'fulfillment' }
  let(:queue_name) { "#{namespace}_queue" }
  let(:test_task_id) { 123 }
  let(:test_step_id) { 456 }

  let(:pgmq_client) { TaskerCore::Messaging.create_pgmq_client }
  let(:sql_functions) { TaskerCore::Database.create_sql_functions }
  let(:queue_worker) { TaskerCore::Messaging.create_queue_worker(namespace, poll_interval: 0.1) }

  before(:each) do
    # Ensure queue exists and is clean
    pgmq_client.create_queue(queue_name)
    pgmq_client.purge_queue(queue_name)
  end

  after(:each) do
    # Clean up
    queue_worker.stop if queue_worker.running?
    pgmq_client.close
    sql_functions.close
  end

  describe 'Phase 1: Basic Queue Operations' do
    it 'can create and manage queues' do
      # Test queue creation
      expect { pgmq_client.create_queue(queue_name) }.not_to raise_error
      
      # Test queue metrics
      metrics = pgmq_client.queue_stats(queue_name)
      expect(metrics).to include(:total_messages, :queue_length, :collected_at)
      expect(metrics[:total_messages]).to eq(0)
    end

    it 'can send and receive step messages' do
      # Create a test step message
      step_message = TaskerCore::Types::StepMessage.new(
        step_id: test_step_id,
        task_id: test_task_id,
        namespace: namespace,
        task_name: 'process_order',
        task_version: '1.0.0',
        step_name: 'validate_order',
        step_payload: { order_id: 'TEST_123', customer_id: 456 },
        metadata: TaskerCore::Types::StepMessageMetadata.new(
          enqueued_at: Time.now.utc.iso8601,
          retry_count: 0,
          max_retries: 3
        )
      )

      # Send the message
      msg_id = pgmq_client.send_step_message(namespace, step_message)
      expect(msg_id).to be > 0

      # Verify queue has the message
      metrics = pgmq_client.queue_stats(queue_name)
      expect(metrics[:total_messages]).to eq(1)
      expect(metrics[:queue_length]).to eq(1)

      # Read the message back
      messages = pgmq_client.read_step_messages(namespace, visibility_timeout: 30, qty: 1)
      expect(messages).to have(1).message

      message_data = messages.first
      received_step_message = message_data[:step_message]
      queue_message = message_data[:queue_message]

      # Verify message content
      expect(received_step_message.step_id).to eq(test_step_id)
      expect(received_step_message.task_id).to eq(test_task_id)
      expect(received_step_message.namespace).to eq(namespace)
      expect(received_step_message.step_name).to eq('validate_order')
      expect(received_step_message.step_payload).to eq({ 'order_id' => 'TEST_123', 'customer_id' => 456 })

      # Verify queue message structure
      expect(queue_message[:msg_id]).to eq(msg_id)
      expect(queue_message[:read_ct]).to eq(1)

      # Clean up - delete the message
      result = pgmq_client.delete_message(queue_name, msg_id)
      expect(result).to be true
    end

    it 'can handle batch message operations' do
      # Send multiple messages
      messages = 3.times.map do |i|
        TaskerCore::Types::StepMessage.new(
          step_id: test_step_id + i,
          task_id: test_task_id,
          namespace: namespace,
          task_name: 'process_order',
          task_version: '1.0.0',
          step_name: "step_#{i}",
          step_payload: { step_index: i },
          metadata: TaskerCore::Types::StepMessageMetadata.new(
            enqueued_at: Time.now.utc.iso8601,
            retry_count: 0
          )
        )
      end

      # Send all messages
      msg_ids = messages.map { |msg| pgmq_client.send_step_message(namespace, msg) }
      expect(msg_ids).to have(3).items
      expect(msg_ids).to all(be > 0)

      # Read all messages in batch
      received_messages = pgmq_client.read_step_messages(namespace, visibility_timeout: 30, qty: 3)
      expect(received_messages).to have(3).messages

      # Verify each message
      received_messages.each_with_index do |msg_data, i|
        step_message = msg_data[:step_message]
        expect(step_message.step_name).to eq("step_#{i}")
        expect(step_message.step_payload['step_index']).to eq(i)
      end

      # Clean up - delete all messages
      msg_ids.each { |msg_id| pgmq_client.delete_message(queue_name, msg_id) }
    end
  end

  describe 'Phase 1: SQL Functions Integration' do
    it 'can access system health metrics' do
      health = sql_functions.system_health_counts
      expect(health).to be_a(Hash)
      expect(health).to include(:total_tasks, :active_tasks, :completed_tasks)
    end

    it 'can access analytics metrics' do
      analytics = sql_functions.analytics_metrics
      expect(analytics).to be_a(Hash)
      # Should include various performance metrics
    end

    it 'can check task completion status' do
      # This will return false for non-existent tasks
      complete = sql_functions.task_complete?(test_task_id)
      expect([true, false]).to include(complete)
    end

    it 'can get task progress information' do
      progress = sql_functions.task_progress(test_task_id)
      expect(progress).to be_a(Hash)
      # Should include progress information even for non-existent tasks
    end
  end

  describe 'Phase 1: Queue Worker Framework' do
    # Create a simple test handler for testing
    before(:all) do
      # Define test step handler class
      unless defined?(Fulfillment::StepHandlers::ValidateOrderHandler)
        module Fulfillment
          module StepHandlers
            class ValidateOrderHandler
              def process(payload)
                if payload['order_id']&.start_with?('VALID_')
                  { success: true, data: { validated: true, order_id: payload['order_id'] } }
                else
                  { success: false, message: 'Invalid order ID format', retryable: true }
                end
              end
            end
          end
        end
      end
    end

    it 'can start and stop workers' do
      expect(queue_worker.running?).to be false

      # Start worker
      result = queue_worker.start
      expect(result).to be true
      expect(queue_worker.running?).to be true

      # Stop worker
      result = queue_worker.stop
      expect(result).to be true
      expect(queue_worker.running?).to be false
    end

    it 'can check if it can handle specific steps' do
      step_message = TaskerCore::Types::StepMessage.new(
        step_id: test_step_id,
        task_id: test_task_id,
        namespace: namespace,
        task_name: 'process_order',
        task_version: '1.0.0',
        step_name: 'validate_order',
        step_payload: {},
        metadata: TaskerCore::Types::StepMessageMetadata.new
      )

      # Should be able to handle this step (we defined the handler)
      expect(queue_worker.can_handle_step?(step_message)).to be true

      # Should not handle steps from different namespace
      wrong_namespace_message = step_message.new(namespace: 'inventory')
      expect(queue_worker.can_handle_step?(wrong_namespace_message)).to be false

      # Should not handle steps without handlers
      no_handler_message = step_message.new(step_name: 'unknown_step')
      expect(queue_worker.can_handle_step?(no_handler_message)).to be false
    end

    it 'can process step messages manually' do
      step_message = TaskerCore::Types::StepMessage.new(
        step_id: test_step_id,
        task_id: test_task_id,
        namespace: namespace,
        task_name: 'process_order',
        task_version: '1.0.0',
        step_name: 'validate_order',
        step_payload: { 'order_id' => 'VALID_123' },
        metadata: TaskerCore::Types::StepMessageMetadata.new
      )

      # Process the message
      result = queue_worker.process_step_message(step_message)

      # Verify successful result
      expect(result).to be_a(TaskerCore::Types::StepResult)
      expect(result.success?).to be true
      expect(result.step_id).to eq(test_step_id)
      expect(result.task_id).to eq(test_task_id)
      expect(result.result_data).to include('validated' => true, 'order_id' => 'VALID_123')
    end

    it 'handles step processing failures gracefully' do
      step_message = TaskerCore::Types::StepMessage.new(
        step_id: test_step_id,
        task_id: test_task_id,
        namespace: namespace,
        task_name: 'process_order',
        task_version: '1.0.0',
        step_name: 'validate_order',
        step_payload: { 'order_id' => 'INVALID_123' },  # Will fail validation
        metadata: TaskerCore::Types::StepMessageMetadata.new
      )

      # Process the message
      result = queue_worker.process_step_message(step_message)

      # Verify failed result
      expect(result).to be_a(TaskerCore::Types::StepResult)
      expect(result.success?).to be false
      expect(result.step_id).to eq(test_step_id)
      expect(result.task_id).to eq(test_task_id)
      expect(result.error).to be_a(TaskerCore::Types::StepExecutionError)
      expect(result.error.retryable).to be true
    end

    it 'maintains accurate statistics' do
      # Get initial stats
      initial_stats = queue_worker.stats
      expect(initial_stats[:messages_processed]).to eq(0)
      expect(initial_stats[:messages_succeeded]).to eq(0)
      expect(initial_stats[:messages_failed]).to eq(0)

      # Process a successful message
      success_message = TaskerCore::Types::StepMessage.new(
        step_id: test_step_id,
        task_id: test_task_id,
        namespace: namespace,
        task_name: 'process_order',
        task_version: '1.0.0',
        step_name: 'validate_order',
        step_payload: { 'order_id' => 'VALID_123' },
        metadata: TaskerCore::Types::StepMessageMetadata.new
      )

      queue_worker.process_step_message(success_message)

      # Process a failed message
      failure_message = success_message.new(step_payload: { 'order_id' => 'INVALID_123' })
      queue_worker.process_step_message(failure_message)

      # Check updated stats
      final_stats = queue_worker.stats
      expect(final_stats[:messages_processed]).to eq(2)
      expect(final_stats[:messages_succeeded]).to eq(1)
      expect(final_stats[:messages_failed]).to eq(1)
      expect(final_stats[:total_execution_time_ms]).to be > 0
    end
  end

  describe 'Phase 1: End-to-End Flow' do
    it 'can handle complete message lifecycle with worker' do
      # Start the worker
      queue_worker.start
      expect(queue_worker.running?).to be true

      # Send a message to the queue
      step_message = TaskerCore::Types::StepMessage.new(
        step_id: test_step_id,
        task_id: test_task_id,
        namespace: namespace,
        task_name: 'process_order',
        task_version: '1.0.0',
        step_name: 'validate_order',
        step_payload: { 'order_id' => 'VALID_E2E_123' },
        metadata: TaskerCore::Types::StepMessageMetadata.new(
          enqueued_at: Time.now.utc.iso8601,
          retry_count: 0,
          max_retries: 3
        )
      )

      msg_id = pgmq_client.send_step_message(namespace, step_message)
      expect(msg_id).to be > 0

      # Wait for worker to process the message
      # Note: This is a simple polling check - in real scenarios we'd use better synchronization
      processed = false
      timeout = 5.0  # 5 second timeout
      start_time = Time.now
      
      while Time.now - start_time < timeout && !processed
        sleep 0.1
        
        # Check if message was processed (removed from queue)
        metrics = pgmq_client.queue_stats(queue_name)
        processed = metrics[:queue_length] == 0
      end

      # Verify the message was processed
      expect(processed).to be true, "Message was not processed within #{timeout} seconds"

      # Check worker stats
      stats = queue_worker.stats
      expect(stats[:messages_processed]).to be >= 1
      expect(stats[:messages_succeeded]).to be >= 1

      # Stop the worker
      queue_worker.stop
      expect(queue_worker.running?).to be false
    end
  end

  describe 'Phase 1: Module Integration' do
    it 'has all modules properly loaded' do
      # Test Messaging module
      expect(TaskerCore::Messaging).to respond_to(:create_pgmq_client)
      expect(TaskerCore::Messaging).to respond_to(:create_queue_worker)
      expect(TaskerCore::Messaging).to respond_to(:ensure_namespace_queues)
      expect(TaskerCore::Messaging).to respond_to(:all_queue_metrics)

      # Test Database module
      expect(TaskerCore::Database).to respond_to(:create_sql_functions)
      expect(TaskerCore::Database).to respond_to(:system_health)
      expect(TaskerCore::Database).to respond_to(:analytics)
      expect(TaskerCore::Database).to respond_to(:task_complete?)
      expect(TaskerCore::Database).to respond_to(:task_progress)

      # Test Types module
      expect(TaskerCore::Types::StepMessage).to be_a(Class)
      expect(TaskerCore::Types::StepResult).to be_a(Class)
      expect(TaskerCore::Types::StepMessageMetadata).to be_a(Class)
      expect(TaskerCore::Types::StepExecutionError).to be_a(Class)
    end

    it 'can create clients through module interfaces' do
      # Test Messaging module factory methods
      client = TaskerCore::Messaging.create_pgmq_client
      expect(client).to be_a(TaskerCore::Messaging::PgmqClient)
      client.close

      worker = TaskerCore::Messaging.create_queue_worker('test_namespace')
      expect(worker).to be_a(TaskerCore::Messaging::QueueWorker)

      # Test Database module factory methods
      sql_client = TaskerCore::Database.create_sql_functions
      expect(sql_client).to be_a(TaskerCore::Database::SqlFunctions)
      sql_client.close
    end
  end
end
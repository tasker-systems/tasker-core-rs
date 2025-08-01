# frozen_string_literal: true

require 'concurrent-ruby'
require 'json'
require_relative 'pgmq_client'
require_relative '../types/batch_message'
require_relative '../types/batch_result_message'
require_relative '../step_handler/base_step_handler'

module TaskerCore
  module Messaging
    ##
    # BatchQueueWorker - Autonomous Ruby worker for processing step execution batches
    #
    # This worker:
    # 1. Polls namespace-specific batch queues (e.g. fulfillment_batch_queue)
    # 2. Processes batches concurrently using concurrent-ruby
    # 3. Executes step handlers with existing business logic
    # 4. Publishes results to orchestration_batch_results queue
    # 5. Operates completely autonomously without registration
    #
    # Architecture:
    # - Queue-driven processing (no TCP/coordination complexity)
    # - Concurrent step execution within batches
    # - Fire-and-forget result publishing
    # - Automatic error handling and retry logic
    # - Compatible with existing Rails Tasker step handlers
    class BatchQueueWorker
      include TaskerCore::Utils::Logging

      attr_reader :namespace, :config, :pgmq_client, :step_handler_registry

      def initialize(namespace:, config: {})
        @namespace = namespace
        @config = default_config.merge(config)
        @pgmq_client = PgmqClient.new
        @step_handler_registry = build_step_handler_registry
        @running = false
        @shutdown_requested = false

        validate_configuration!
        log_info "BatchQueueWorker initialized for namespace: #{namespace}"
      end

      ##
      # Start the worker processing loop
      #
      # This is the main entry point that:
      # 1. Initializes the batch queue for this namespace
      # 2. Starts the polling loop
      # 3. Processes batches as they arrive
      # 4. Handles graceful shutdown
      def start
        return if @running

        @running = true
        @shutdown_requested = false

        log_info "Starting BatchQueueWorker for namespace: #{namespace}"
        log_info "Configuration: #{config.inspect}"

        # Ensure the batch queue exists
        initialize_batch_queue

        # Start the main processing loop
        process_loop
      rescue => e
        log_error "BatchQueueWorker crashed for namespace #{namespace}: #{e.message}"
        log_error e.backtrace.join("\n")
        raise
      ensure
        @running = false
      end

      ##
      # Request graceful shutdown
      def stop
        log_info "Shutdown requested for BatchQueueWorker namespace: #{namespace}"
        @shutdown_requested = true
      end

      ##
      # Check if worker is running
      def running?
        @running
      end

      ##
      # Get worker statistics
      def statistics
        {
          namespace: namespace,
          running: running?,
          shutdown_requested: @shutdown_requested,
          batch_queue_name: batch_queue_name,
          results_queue_name: config[:results_queue_name],
          worker_id: worker_id,
          started_at: @started_at,
          processed_batches: @processed_batches || 0,
          processed_steps: @processed_steps || 0,
          failed_batches: @failed_batches || 0,
          failed_steps: @failed_steps || 0
        }
      end

      private

      ##
      # Main processing loop
      def process_loop
        @started_at = Time.now
        @processed_batches = 0
        @processed_steps = 0
        @failed_batches = 0
        @failed_steps = 0

        log_info "Starting processing loop for namespace: #{namespace}"

        while !@shutdown_requested
          begin
            processed_count = process_batch_messages

            if processed_count == 0
              # No messages processed, wait before polling again
              sleep(config[:polling_interval_seconds])
            end
            # If we processed messages, continue immediately for better throughput
          rescue => e
            log_error "Error in processing loop for namespace #{namespace}: #{e.message}"
            log_error e.backtrace.join("\n")
            
            # Wait before retrying on error
            sleep(config[:error_retry_delay_seconds])
          end
        end

        log_info "Processing loop stopped for namespace: #{namespace}"
      end

      ##
      # Process a batch of messages from the queue
      def process_batch_messages
        # Read messages from the batch queue
        messages = pgmq_client.read_messages(
          batch_queue_name,
          visibility_timeout: config[:visibility_timeout_seconds],
          limit: config[:batch_size]
        )

        return 0 if messages.empty?

        log_debug "Processing #{messages.size} batch messages for namespace: #{namespace}"

        processed_count = 0

        messages.each do |message|
          begin
            process_single_batch(message[:message], message[:msg_id])
            
            # Delete successfully processed message
            pgmq_client.delete_message(batch_queue_name, message[:msg_id])
            processed_count += 1
            @processed_batches += 1

          rescue => e
            log_error "Failed to process batch message #{message[:msg_id]} for namespace #{namespace}: #{e.message}"
            log_error e.backtrace.join("\n")
            @failed_batches += 1
            
            # Archive failed messages for debugging
            begin
              pgmq_client.archive_message(batch_queue_name, message[:msg_id])
            rescue => archive_error
              log_error "Failed to archive message #{message[:msg_id]}: #{archive_error.message}"
            end
          end
        end

        if processed_count > 0
          log_info "Processed #{processed_count}/#{messages.size} batch messages for namespace: #{namespace}"
        end

        processed_count
      end

      ##
      # Process a single batch message
      def process_single_batch(message_payload, msg_id)
        # Parse the batch message
        batch_message = TaskerCore::Types::BatchMessage.from_json(message_payload)
        
        log_info "Processing batch #{batch_message.batch_id} for task #{batch_message.task_id} (namespace: #{namespace})"
        log_debug "Batch contains #{batch_message.steps.size} steps"

        # Record batch start time
        batch_start_time = Time.now

        # Process all steps in the batch concurrently
        step_results = process_batch_steps_concurrently(batch_message)

        # Calculate batch execution time
        batch_execution_time_ms = ((Time.now - batch_start_time) * 1000).round

        # Determine overall batch status
        batch_status = determine_batch_status(step_results)

        # Create and publish batch result message
        result_message = create_batch_result_message(
          batch_message,
          batch_status,
          step_results,
          batch_execution_time_ms
        )

        publish_batch_result(result_message)

        # Update statistics
        @processed_steps += step_results.size
        @failed_steps += step_results.count { |result| result[:status] == 'Failed' }

        log_info "Completed batch #{batch_message.batch_id} with status #{batch_status} (#{batch_execution_time_ms}ms)"
      end

      ##
      # Process all steps in a batch concurrently using concurrent-ruby
      def process_batch_steps_concurrently(batch_message)
        # Create a thread pool for concurrent execution
        pool = Concurrent::FixedThreadPool.new(config[:max_concurrent_steps])
        
        # Create promises for each step execution
        step_promises = batch_message.steps.map do |step|
          Concurrent::Promise.execute(executor: pool) do
            process_single_step(batch_message, step)
          end
        end

        # Wait for all step executions to complete
        step_results = step_promises.map(&:value!)

        # Shutdown the thread pool
        pool.shutdown
        pool.wait_for_termination(config[:step_timeout_seconds])

        step_results
      rescue => e
        log_error "Error in concurrent step processing for batch #{batch_message.batch_id}: #{e.message}"
        
        # If concurrent processing fails, fall back to sequential processing
        log_warn "Falling back to sequential step processing for batch #{batch_message.batch_id}"
        batch_message.steps.map { |step| process_single_step(batch_message, step) }
      ensure
        # Always ensure the pool is shutdown
        pool&.shutdown
      end

      ##
      # Process a single step within a batch
      def process_single_step(batch_message, step)
        step_start_time = Time.now
        
        log_debug "Executing step #{step.step_id} (#{step.step_name}) for task #{batch_message.task_id}"

        begin
          # Find the appropriate step handler
          handler_class = find_step_handler(step.step_name)
          
          if handler_class.nil?
            raise "No handler found for step: #{step.step_name}"
          end

          # Create handler instance with step context
          handler = handler_class.new(
            step_id: step.step_id,
            task_id: batch_message.task_id,
            namespace: batch_message.namespace,
            step_payload: step.step_payload,
            step_metadata: step.step_metadata
          )

          # Execute the step handler
          result = handler.execute

          # Calculate execution time
          execution_time_ms = ((Time.now - step_start_time) * 1000).round

          # Return successful step result
          {
            step_id: step.step_id,
            status: 'Success',
            output: result,
            error: nil,
            error_code: nil,
            execution_duration_ms: execution_time_ms,
            executed_at: Time.now.utc.iso8601
          }

        rescue => e
          execution_time_ms = ((Time.now - step_start_time) * 1000).round
          
          log_error "Step #{step.step_id} (#{step.step_name}) failed: #{e.message}"
          log_debug e.backtrace.join("\n")

          # Return failed step result
          {
            step_id: step.step_id,
            status: 'Failed',
            output: nil,
            error: e.message,
            error_code: classify_error(e),
            execution_duration_ms: execution_time_ms,
            executed_at: Time.now.utc.iso8601
          }
        end
      end

      ##
      # Find the appropriate step handler class
      def find_step_handler(step_name)
        # First try the registry
        handler_class = step_handler_registry[step_name]
        return handler_class if handler_class

        # Fall back to convention-based lookup
        handler_class_name = "#{step_name.camelize}Handler"
        
        # Try to find in TaskerCore::StepHandler namespace
        begin
          return "TaskerCore::StepHandler::#{handler_class_name}".constantize
        rescue NameError
          # Try global namespace
          begin
            return handler_class_name.constantize
          rescue NameError
            log_warn "No handler found for step: #{step_name}"
            nil
          end
        end
      end

      ##
      # Determine overall batch status from step results
      def determine_batch_status(step_results)
        failed_count = step_results.count { |result| result[:status] == 'Failed' }
        
        if failed_count == 0
          'Success'
        elsif failed_count == step_results.size
          'Failed'
        else
          'PartialSuccess'
        end
      end

      ##
      # Create a batch result message
      def create_batch_result_message(batch_message, batch_status, step_results, execution_time_ms)
        successful_steps = step_results.count { |result| result[:status] == 'Success' }
        failed_steps = step_results.count { |result| result[:status] == 'Failed' }

        TaskerCore::Types::BatchResultMessage.new(
          batch_id: batch_message.batch_id,
          task_id: batch_message.task_id,
          namespace: batch_message.namespace,
          batch_status: batch_status,
          step_results: step_results.map { |result| TaskerCore::Types::StepResult.new(result) },
          metadata: TaskerCore::Types::BatchResultMetadata.new(
            completed_at: Time.now.utc.iso8601,
            worker_id: worker_id,
            successful_steps: successful_steps,
            failed_steps: failed_steps,
            total_execution_time_ms: execution_time_ms,
            worker_hostname: Socket.gethostname,
            worker_pid: Process.pid,
            custom: {
              ruby_version: RUBY_VERSION,
              concurrent_processing: config[:max_concurrent_steps] > 1,
              namespace: namespace
            }
          )
        )
      end

      ##
      # Publish batch result to the results queue
      def publish_batch_result(result_message)
        log_debug "Publishing batch result for batch #{result_message.batch_id} to #{config[:results_queue_name]}"
        
        pgmq_client.send_message(
          config[:results_queue_name],
          result_message.to_hash
        )
        
        log_debug "Successfully published batch result for batch #{result_message.batch_id}"
      end

      ##
      # Initialize the batch queue for this namespace
      def initialize_batch_queue
        log_debug "Initializing batch queue: #{batch_queue_name}"
        
        begin
          pgmq_client.create_queue(batch_queue_name)
          log_debug "Batch queue #{batch_queue_name} initialized"
        rescue => e
          # Queue likely already exists, which is fine
          log_debug "Batch queue #{batch_queue_name} may already exist: #{e.message}"
        end
      end

      ##
      # Get the batch queue name for this namespace
      def batch_queue_name
        "#{namespace}_batch_queue"
      end

      ##
      # Generate unique worker ID
      def worker_id
        @worker_id ||= "#{namespace}-worker-#{Socket.gethostname}-#{Process.pid}-#{SecureRandom.hex(4)}"
      end

      ##
      # Default configuration
      def default_config
        {
          # Queue settings
          polling_interval_seconds: 1,
          batch_size: 5,
          visibility_timeout_seconds: 300, # 5 minutes
          results_queue_name: 'orchestration_batch_results',
          
          # Processing settings
          max_concurrent_steps: 4,
          step_timeout_seconds: 60,
          error_retry_delay_seconds: 5,
          
          # Monitoring settings
          log_level: :info,
          metrics_enabled: true
        }
      end

      ##
      # Build step handler registry
      def build_step_handler_registry
        registry = {}
        
        # Register default handlers
        registry['validate_order'] = TaskerCore::StepHandler::ValidateOrderHandler if defined?(TaskerCore::StepHandler::ValidateOrderHandler)
        registry['reserve_inventory'] = TaskerCore::StepHandler::ReserveInventoryHandler if defined?(TaskerCore::StepHandler::ReserveInventoryHandler)
        registry['charge_payment'] = TaskerCore::StepHandler::ChargePaymentHandler if defined?(TaskerCore::StepHandler::ChargePaymentHandler)
        registry['send_notification'] = TaskerCore::StepHandler::SendNotificationHandler if defined?(TaskerCore::StepHandler::SendNotificationHandler)
        
        # Allow custom registration via configuration
        if config[:custom_handlers]
          registry.merge!(config[:custom_handlers])
        end
        
        registry
      end

      ##
      # Classify error for error codes
      def classify_error(error)
        case error
        when ArgumentError, NoMethodError
          'VALIDATION_ERROR'
        when Timeout::Error
          'TIMEOUT_ERROR'
        when StandardError
          'EXECUTION_ERROR'
        else
          'UNKNOWN_ERROR'
        end
      end

      ##
      # Validate configuration
      def validate_configuration!
        required_keys = [:polling_interval_seconds, :batch_size, :visibility_timeout_seconds, :results_queue_name]
        
        required_keys.each do |key|
          raise ArgumentError, "Missing required config key: #{key}" unless config.key?(key)
        end

        raise ArgumentError, "namespace cannot be blank" if namespace.nil? || namespace.strip.empty?
        raise ArgumentError, "batch_size must be positive" unless config[:batch_size] > 0
        raise ArgumentError, "max_concurrent_steps must be positive" unless config[:max_concurrent_steps] > 0
      end
    end
  end
end
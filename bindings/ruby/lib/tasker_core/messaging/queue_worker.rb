# frozen_string_literal: true

require 'concurrent'

module TaskerCore
  module Messaging
    # Autonomous queue worker for processing step messages
    #
    # This is the core of the new pgmq architecture - workers that poll queues
    # independently, execute step handlers, and update the database directly.
    # No FFI coupling, no central coordination needed.
    #
    # Examples:
    #   worker = QueueWorker.new("fulfillment")
    #   worker.start  # Begins polling loop
    #   worker.stop   # Graceful shutdown
    class QueueWorker
      # Default configuration
      DEFAULT_POLL_INTERVAL = 1.0  # seconds
      DEFAULT_VISIBILITY_TIMEOUT = 30  # seconds
      DEFAULT_BATCH_SIZE = 5
      DEFAULT_MAX_RETRIES = 3
      DEFAULT_SHUTDOWN_TIMEOUT = 30  # seconds

      attr_reader :namespace, :queue_name, :pgmq_client, :sql_functions, :logger,
                  :poll_interval, :visibility_timeout, :batch_size, :max_retries,
                  :running, :shutdown_timeout

      def initialize(namespace,
                     pgmq_client: nil,
                     sql_functions: nil,
                     logger: nil,
                     poll_interval: DEFAULT_POLL_INTERVAL,
                     visibility_timeout: DEFAULT_VISIBILITY_TIMEOUT,
                     batch_size: DEFAULT_BATCH_SIZE,
                     max_retries: DEFAULT_MAX_RETRIES,
                     shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT)
        @namespace = namespace
        @queue_name = "#{namespace}_queue"
        @pgmq_client = pgmq_client || PgmqClient.new
        @sql_functions = sql_functions || TaskerCore::Database::SqlFunctions.new
        @logger = logger || TaskerCore::Logging::Logger.instance

        # Configuration
        @poll_interval = poll_interval
        @visibility_timeout = visibility_timeout
        @batch_size = batch_size
        @max_retries = max_retries
        @shutdown_timeout = shutdown_timeout

        # State management
        @running = false
        @worker_thread = nil
        @shutdown_signal = Concurrent::Event.new
        @stats = initialize_stats

        # Ensure the queue exists
        ensure_queue_exists
      end

      # Start the worker (begins polling loop)
      #
      # @return [Boolean] true if started successfully
      def start
        return false if running?

        logger.info("🚀 QUEUE_WORKER: Starting worker for namespace: #{namespace}")

        @running = true
        @shutdown_signal.reset

        @worker_thread = Thread.new do
          Thread.current.name = "QueueWorker-#{namespace}"
          polling_loop
        end

        logger.info("✅ QUEUE_WORKER: Worker started for namespace: #{namespace}")
        true
      end

      # Stop the worker (graceful shutdown)
      #
      # @return [Boolean] true if stopped successfully
      def stop
        return true unless running?

        logger.info("🛑 QUEUE_WORKER: Stopping worker for namespace: #{namespace}")

        @running = false
        @shutdown_signal.set

        # Wait for worker thread to finish
        if @worker_thread && @worker_thread.alive?
          if @worker_thread.join(shutdown_timeout)
            logger.info("✅ QUEUE_WORKER: Worker stopped gracefully for namespace: #{namespace}")
          else
            logger.warn("⚠️ QUEUE_WORKER: Worker shutdown timeout, forcing stop for namespace: #{namespace}")
            @worker_thread.kill
          end
        end

        @worker_thread = nil
        true
      end

      # Check if worker is running
      #
      # @return [Boolean] true if worker is running
      def running?
        @running && @worker_thread&.alive?
      end

      # Get worker statistics
      #
      # @return [Hash] Worker performance statistics
      def stats
        @stats.dup.tap do |s|
          s[:running] = running?
          s[:queue_name] = queue_name
          s[:namespace] = namespace
          s[:uptime_seconds] = running? ? (Time.now - s[:started_at]).to_i : 0
        end
      end

      # Process a single step message manually (for testing)
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message to process
      # @return [TaskerCore::Types::StepResult] Execution result
      def process_step_message(step_message)
        start_time = Time.now

        logger.info("🔄 QUEUE_WORKER: Processing step - step_id: #{step_message.step_id}, task_id: #{step_message.task_id}, step_name: #{step_message.step_name}")

        begin
          # Find and execute the appropriate step handler
          result = execute_step_handler(step_message)

          # Update statistics
          execution_time_ms = ((Time.now - start_time) * 1000).to_i
          @stats[:messages_processed] += 1
          @stats[:total_execution_time_ms] += execution_time_ms

          if result.success?
            @stats[:messages_succeeded] += 1
            logger.info("✅ QUEUE_WORKER: Step completed successfully - step_id: #{step_message.step_id}, execution_time: #{execution_time_ms}ms")
          else
            @stats[:messages_failed] += 1
            logger.error("❌ QUEUE_WORKER: Step failed - step_id: #{step_message.step_id}, error: #{result.error&.message}")
          end

          result
        rescue => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i
          @stats[:messages_failed] += 1
          @stats[:messages_errored] += 1

          logger.error("💥 QUEUE_WORKER: Unexpected error processing step #{step_message.step_id}: #{e.message}")
          logger.error("💥 QUEUE_WORKER: #{e.backtrace.first(5).join("\n")}")

          # Create failure result
          error = TaskerCore::Types::StepExecutionError.new(
            error_type: 'UnexpectedError',
            message: e.message,
            retryable: true,
            stack_trace: e.backtrace.join("\n")
          )

          TaskerCore::Types::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: execution_time_ms
          )
        end
      end

      # Check if worker can handle a specific step
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message to check
      # @return [Boolean] true if this worker can handle the step
      def can_handle_step?(step_message)
        # Basic namespace matching
        return false unless step_message.namespace == namespace

        # Check if we have a handler for this step
        handler_class = find_step_handler_class(step_message.step_name)

        if handler_class
          logger.debug("✅ QUEUE_WORKER: Can handle step: #{step_message.step_name} (#{handler_class})")
          true
        else
          logger.debug("❌ QUEUE_WORKER: No handler found for step: #{step_message.step_name}")
          false
        end
      end

      # NEW Phase 5.2: Check if we can extract (task, sequence, step) from execution context
      # This enables the immediate delete pattern by ensuring all necessary data is in the message
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message to validate
      # @return [Boolean] true if execution context is complete
      def can_extract_execution_context?(step_message)
        return false unless step_message.namespace == namespace
        return false unless find_step_handler_class(step_message.step_name)

        # Validate execution context has required data
        execution_context = step_message.execution_context
        return false unless execution_context
        return false unless execution_context.task && !execution_context.task.empty?
        return false unless execution_context.step && !execution_context.step.empty?

        logger.debug("✅ QUEUE_WORKER: Can extract execution context for step: #{step_message.step_name}")
        true
      rescue => e
        logger.warn("⚠️ QUEUE_WORKER: Failed to validate execution context: #{e.message}")
        false
      end

      private

      # Main polling loop
      def polling_loop
        logger.debug("🔄 QUEUE_WORKER: Starting polling loop for namespace: #{namespace}")

        while running? && !@shutdown_signal.set?
          begin
            poll_and_process_batch
          rescue => e
            logger.error("💥 QUEUE_WORKER: Polling loop error for #{namespace}: #{e.message}")
            logger.error("💥 QUEUE_WORKER: #{e.backtrace.first(3).join("\n")}")
            @stats[:polling_errors] += 1
          end

          # Wait for next poll cycle (unless shutting down)
          unless @shutdown_signal.wait(poll_interval)
            # Continue polling
          end
        end

        logger.debug("🏁 QUEUE_WORKER: Polling loop ended for namespace: #{namespace}")
      end

      # Poll queue and process a batch of messages
      def poll_and_process_batch
        # Read messages from queue
        queue_messages = pgmq_client.read_step_messages(
          namespace,
          visibility_timeout: visibility_timeout,
          qty: batch_size
        )

        return if queue_messages.empty?

        logger.debug("📥 QUEUE_WORKER: Received #{queue_messages.length} messages from #{queue_name}")
        @stats[:batches_processed] += 1

        # Process messages concurrently
        futures = queue_messages.map do |msg_data|
          Concurrent::Promises.future do
            process_queue_message(msg_data)
          end
        end

        # Wait for all messages to complete
        results = futures.map(&:value!)

        logger.debug("✅ QUEUE_WORKER: Batch processing completed - #{results.count(&:success?)} succeeded, #{results.count { |r| !r.success? }} failed")
      end

      # Process a single message from the queue
      # NEW Phase 5.2: Immediate delete pattern - "Worker Executes, Orchestration Coordinates"
      def process_queue_message(msg_data)
        queue_message = msg_data[:queue_message]
        step_message = msg_data[:step_message]

        begin
          # 1. Validate we can extract (task, sequence, step) from execution context
          unless can_extract_execution_context?(step_message)
            logger.debug("⏭️ QUEUE_WORKER: Skipping step - cannot extract execution context: #{step_message.step_name}")
            return create_skip_result(step_message)
          end

          # 2. IMMEDIATELY delete message from queue (no retry logic here!)
          # The orchestration system handles all retry decisions
          pgmq_client.delete_message(queue_name, queue_message[:msg_id])
          logger.debug("🗑️ QUEUE_WORKER: Message deleted immediately - msg_id: #{queue_message[:msg_id]}")

          # 3. Execute handler and collect rich metadata
          result = execute_step_handler_with_metadata(step_message)

          # 4. Send result (with metadata) to orchestration result queue
          send_result_to_orchestration(result)

          result
        rescue => e
          logger.error("💥 QUEUE_WORKER: Error processing queue message #{queue_message[:msg_id]}: #{e.message}")
          @stats[:processing_errors] += 1

          # Create error result and send to orchestration for retry decisions
          error_result = create_error_result(step_message, e)
          send_result_to_orchestration(error_result)

          error_result
        end
      end

      # Execute the appropriate step handler for the message
      def execute_step_handler(step_message)
        handler_class = find_step_handler_class(step_message.step_name)

        unless handler_class
          error = TaskerCore::Types::StepExecutionError.new(
            error_type: 'HandlerNotFound',
            message: "No handler found for step: #{step_message.step_name}",
            retryable: false
          )

          return TaskerCore::Types::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: 0
          )
        end

        # Create handler instance and execute
        handler = handler_class.new
        start_time = Time.now

        begin
          # Extract (task, sequence, step) from execution context
          execution_context = step_message.execution_context
          task = execution_context.task
          sequence = execution_context.dependencies  # Use the convenient wrapper
          step = execution_context.step
          
          # Execute with (task, sequence, step) interface
          handler_result = handler.call(task, sequence, step)
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          # Handler returned data - always treat as success since handlers just return JSON-serializable data
          # Any exceptions would be caught in the rescue block below
          TaskerCore::Types::StepResult.success(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            result_data: handler_result,
            execution_time_ms: execution_time_ms
          )
        rescue => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          error = TaskerCore::Types::StepExecutionError.new(
            error_type: 'HandlerException',
            message: e.message,
            retryable: true,
            stack_trace: e.backtrace.join("\n")
          )

          TaskerCore::Types::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: execution_time_ms
          )
        end
      end

      # NEW Phase 5.2: Execute step handler with rich metadata collection
      # This replaces execute_step_handler for Phase 5.2 metadata flow
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message with execution context
      # @return [TaskerCore::Types::StepResult] Enhanced result with orchestration metadata
      def execute_step_handler_with_metadata(step_message)
        handler_class = find_step_handler_class(step_message.step_name)

        unless handler_class
          error = TaskerCore::Types::StepExecutionError.new(
            error_type: 'HandlerNotFound',
            message: "No handler found for step: #{step_message.step_name}",
            retryable: false
          )

          return TaskerCore::Types::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: 0
          )
        end

        # Extract (task, sequence, step) from execution context
        execution_context = step_message.execution_context
        task = execution_context.task
        sequence = execution_context.dependencies  # Use the convenient wrapper
        step = execution_context.step

        # Create handler instance and execute with new interface
        handler = handler_class.new
        start_time = Time.now

        begin
          # Execute with enhanced (task, sequence, step) interface
          handler_result = handler.call(task, sequence, step)
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          # Extract orchestration metadata from handler result
          orchestration_metadata = extract_orchestration_metadata(handler_result)

          # Handler returned data - always treat as success since handlers just return JSON-serializable data
          # Any exceptions would be caught in the rescue block below
          result = TaskerCore::Types::StepResult.success(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            result_data: handler_result,
            execution_time_ms: execution_time_ms
          )
          
          # Add orchestration metadata if present
          result.orchestration_metadata = orchestration_metadata if orchestration_metadata
          result
        rescue => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          error = TaskerCore::Types::StepExecutionError.new(
            error_type: 'HandlerException',
            message: e.message,
            retryable: true,
            stack_trace: e.backtrace.join("\n")
          )

          TaskerCore::Types::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: execution_time_ms
          )
        end
      end

      # Extract orchestration metadata from handler result
      # @param handler_result [Hash] Result from handler execution
      # @return [Hash, nil] Orchestration metadata or nil
      def extract_orchestration_metadata(handler_result)
        return nil unless handler_result.is_a?(Hash)
        
        # Look for _orchestration_metadata key (preferred) or metadata key (legacy)
        metadata = handler_result[:_orchestration_metadata] || handler_result[:metadata]
        return nil unless metadata

        # Map to expected orchestration metadata structure
        {
          http_headers: metadata[:http_headers] || metadata[:headers] || {},
          execution_hints: metadata[:execution_hints] || {},
          backoff_hints: metadata[:backoff_hints] || {},
          error_context: metadata[:error_context],
          custom: metadata[:custom] || {}
        }
      end

      # Send result to orchestration system for coordination decisions
      # @param result [TaskerCore::Types::StepResult] Step execution result
      def send_result_to_orchestration(result)
        # TODO: Implement orchestration result queue publishing
        # This will send results to orchestration_step_results queue
        # For now, just log the action
        logger.debug("📤 QUEUE_WORKER: Sending result to orchestration - step_id: #{result.step_id}, status: #{result.status}")

        # In Phase 5.2, this will publish to pgmq orchestration result queue
        # pgmq_client.send_json_message('orchestration_step_results', result.to_hash)
      end

      # Find the step handler class for a step name
      def find_step_handler_class(step_name)
        # Convert step_name to class name (e.g., "validate_order" -> "ValidateOrderHandler")
        class_name = step_name.split('_').map(&:capitalize).join + 'Handler'

        # Try to find the handler class in various namespaces
        [
          "#{namespace.capitalize}::StepHandlers::#{class_name}",
          "StepHandlers::#{class_name}",
          class_name
        ].each do |full_class_name|
          begin
            return Object.const_get(full_class_name)
          rescue NameError
            # Continue searching
          end
        end

        nil
      end

      # Ensure the queue exists for this namespace
      def ensure_queue_exists
        begin
          pgmq_client.create_queue(queue_name)
        rescue TaskerCore::Error => e
          logger.warn("⚠️ QUEUE_WORKER: Queue creation warning for #{queue_name}: #{e.message}")
        end
      end

      # Initialize worker statistics
      def initialize_stats
        {
          started_at: Time.now,
          messages_processed: 0,
          messages_succeeded: 0,
          messages_failed: 0,
          messages_errored: 0,
          batches_processed: 0,
          polling_errors: 0,
          processing_errors: 0,
          total_execution_time_ms: 0
        }
      end

      # Create a skip result for steps we can't handle
      def create_skip_result(step_message)
        TaskerCore::Types::StepResult.new(
          step_id: step_message.step_id,
          task_id: step_message.task_id,
          status: TaskerCore::Types::StepExecutionStatus.new(status: 'cancelled'),
          execution_time_ms: 0
        )
      end

      # Create a max retries exceeded result
      def create_max_retries_result(step_message)
        error = TaskerCore::Types::StepExecutionError.new(
          error_type: 'MaxRetriesExceeded',
          message: "Step exceeded maximum retry limit: #{step_message.metadata.max_retries}",
          retryable: false
        )

        TaskerCore::Types::StepResult.failure(
          step_id: step_message.step_id,
          task_id: step_message.task_id,
          error: error,
          execution_time_ms: 0
        )
      end

      # Create an error result for unexpected errors
      def create_error_result(step_message, exception)
        error = TaskerCore::Types::StepExecutionError.new(
          error_type: 'ProcessingError',
          message: exception.message,
          retryable: true,
          stack_trace: exception.backtrace.join("\n")
        )

        TaskerCore::Types::StepResult.failure(
          step_id: step_message.step_id,
          task_id: step_message.task_id,
          error: error,
          execution_time_ms: 0
        )
      end
    end
  end
end

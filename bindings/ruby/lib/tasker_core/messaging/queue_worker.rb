# frozen_string_literal: true

require 'concurrent'
require_relative '../types/step_types'

module TaskerCore
  module Messaging
    # Autonomous queue worker for processing step messages
    #
    # This is the core of the new pgmq architecture - workers that poll queues
    # independently, execute step handlers, and update the database directly.
    # No FFI coupling, no central coordination needed.
    #
    # Configuration is loaded from YAML with environment-specific optimization:
    # - Test: 100ms (10x/sec) for fast CI/CD
    # - Development: 500ms (2x/sec) for balanced debugging
    # - Production: 200ms (5x/sec) for high responsiveness  
    # - Base: 250ms (4x/sec) default
    #
    # Examples:
    #   worker = QueueWorker.new("fulfillment")
    #   worker.start  # Begins polling loop
    #   worker.stop   # Graceful shutdown
    class QueueWorker
      # Fallback defaults if configuration is unavailable
      FALLBACK_POLL_INTERVAL = 0.25  # 250ms in seconds as fallback
      FALLBACK_VISIBILITY_TIMEOUT = 30  # seconds
      FALLBACK_BATCH_SIZE = 5
      FALLBACK_MAX_RETRIES = 3
      FALLBACK_SHUTDOWN_TIMEOUT = 30  # seconds

      attr_reader :namespace, :queue_name, :pgmq_client, :sql_functions, :step_handler_registry, :logger,
                  :poll_interval, :visibility_timeout, :batch_size, :max_retries, :shutdown_timeout

      def initialize(namespace,
                     pgmq_client: nil,
                     sql_functions: nil,
                     step_handler_registry: nil,
                     logger: nil,
                     poll_interval: nil,
                     visibility_timeout: nil,
                     batch_size: nil,
                     max_retries: nil,
                     shutdown_timeout: nil)
        @namespace = namespace
        @queue_name = "#{namespace}_queue"
        @pgmq_client = pgmq_client || PgmqClient.new
        @sql_functions = sql_functions || TaskerCore::Database::SqlFunctions.new
        @step_handler_registry = step_handler_registry || TaskerCore::Registry.step_handler_registry
        @logger = logger || TaskerCore::Logging::Logger.instance

        # Configuration - read from YAML with environment-specific optimization
        config = load_configuration
        @poll_interval = poll_interval || config[:poll_interval] || FALLBACK_POLL_INTERVAL
        @visibility_timeout = visibility_timeout || config[:visibility_timeout] || FALLBACK_VISIBILITY_TIMEOUT
        @batch_size = batch_size || config[:batch_size] || FALLBACK_BATCH_SIZE
        @max_retries = max_retries || config[:max_retries] || FALLBACK_MAX_RETRIES
        @shutdown_timeout = shutdown_timeout || config[:shutdown_timeout] || FALLBACK_SHUTDOWN_TIMEOUT

        # Log configuration for debugging
        logger&.debug("🔧 QUEUE_WORKER: Configuration loaded for namespace: #{namespace} - poll_interval: #{@poll_interval}s, batch_size: #{@batch_size}")

        # State management
        @running = false
        @worker_thread = nil
        @shutdown_signal = Concurrent::Event.new

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

      # Process a single step message manually (for testing)
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message to process
      # @return [TaskerCore::Types::StepTypes::StepResult] Execution result
      def process_step_message(step_message)
        start_time = Time.now

        logger.info("🔄 QUEUE_WORKER: Processing step - step_id: #{step_message.step_id}, task_id: #{step_message.task_id}, step_name: #{step_message.step_name}")

        begin
          # Find and execute the appropriate step handler
          result = execute_step_handler(step_message)

          # Log completion
          execution_time_ms = ((Time.now - start_time) * 1000).to_i
          if result.success?
            logger.info("✅ QUEUE_WORKER: Step completed successfully - step_id: #{step_message.step_id}, execution_time: #{execution_time_ms}ms")
          else
            logger.error("❌ QUEUE_WORKER: Step failed - step_id: #{step_message.step_id}, error: #{result.error&.message}")
          end

          result
        rescue => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          logger.error("💥 QUEUE_WORKER: Unexpected error processing step #{step_message.step_id}: #{e.message}")
          logger.error("💥 QUEUE_WORKER: #{e.backtrace.first(5).join("\n")}")

          # Create failure result
          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'UnexpectedError',
            message: e.message,
            retryable: true,
            stack_trace: e.backtrace.join("\n")
          )

          TaskerCore::Types::StepTypes::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: execution_time_ms
          )
        end
      end

      # Check if worker can handle a specific step using database-backed configuration
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message to check
      # @return [Boolean] true if this worker can handle the step
      def can_handle_step?(step_message)
        # Basic namespace matching
        return false unless step_message.namespace == namespace

        # Use registry to check if handler is available (database-backed)
        can_handle = step_handler_registry.can_handle_step?(step_message)

        if can_handle
          logger.debug("✅ QUEUE_WORKER: Can handle step: #{step_message.step_name} (database-backed)")
          true
        else
          logger.debug("❌ QUEUE_WORKER: No handler found for step: #{step_message.step_name} (database-backed)")
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
        return false unless step_handler_registry.can_handle_step?(step_message)

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

      # Load configuration from YAML with environment-specific timing optimization
      def load_configuration
        begin
          config_instance = TaskerCore::Config.instance
          effective_config = config_instance.effective_config
          pgmq_config = effective_config['pgmq'] || {}

          {
            # Convert milliseconds to seconds for Ruby's sleep() method
            poll_interval: pgmq_config['poll_interval_ms'] ? pgmq_config['poll_interval_ms'] / 1000.0 : nil,
            visibility_timeout: pgmq_config['visibility_timeout_seconds'],
            batch_size: pgmq_config['batch_size'],
            max_retries: pgmq_config['max_retries'],
            shutdown_timeout: pgmq_config['shutdown_timeout_seconds']
          }
        rescue => e
          logger&.warn("⚠️ QUEUE_WORKER: Failed to load configuration: #{e.message}, using fallback values")
          {}
        end
      end

      # Get orchestration results queue name from configuration
      def get_orchestration_results_queue_name
        begin
          config_instance = TaskerCore::Config.instance
          effective_config = config_instance.effective_config
          
          # Try to get from orchestration.queues.step_results first
          queue_name = effective_config.dig('orchestration', 'queues', 'step_results')
          
          # Fallback to default if not configured
          queue_name || 'orchestration_step_results'
        rescue => e
          logger&.warn("⚠️ QUEUE_WORKER: Failed to load orchestration queue config: #{e.message}, using default")
          'orchestration_step_results'
        end
      end

      # Main polling loop
      def polling_loop
        logger.debug("🔄 QUEUE_WORKER: Starting polling loop for namespace: #{namespace}")

        while running? && !@shutdown_signal.set?
          begin
            poll_and_process_batch
          rescue => e
            logger.error("💥 QUEUE_WORKER: Polling loop error for #{namespace}: #{e.message}")
            logger.error("💥 QUEUE_WORKER: #{e.backtrace.first(3).join("\n")}")
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

          # Create error result and send to orchestration for retry decisions
          error_result = create_error_result(step_message, e)
          send_result_to_orchestration(error_result)

          error_result
        end
      end

      # Execute the appropriate step handler using database-backed configuration
      def execute_step_handler(step_message)
        # Use registry to resolve handler (database-backed)
        resolved_handler = step_handler_registry.resolve_step_handler(step_message)

        unless resolved_handler
          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'HandlerNotFound',
            message: "No handler found for step: #{step_message.step_name}",
            retryable: false
          )

          return TaskerCore::Types::StepTypes::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: 0
          )
        end

        start_time = Time.now

        begin
          # Create handler instance with proper configuration from database
          handler = step_handler_registry.create_handler_instance(resolved_handler)

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
          TaskerCore::Types::StepTypes::StepResult.success(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            result_data: handler_result,
            execution_time_ms: execution_time_ms
          )
        rescue => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'HandlerException',
            message: e.message,
            retryable: true,
            stack_trace: e.backtrace.join("\n")
          )

          TaskerCore::Types::StepTypes::StepResult.failure(
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
      # @return [TaskerCore::Types::StepTypes::StepResult] Enhanced result with orchestration metadata
      def execute_step_handler_with_metadata(step_message)
        # Use registry to resolve handler (database-backed)
        resolved_handler = step_handler_registry.resolve_step_handler(step_message)

        unless resolved_handler
          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'HandlerNotFound',
            message: "No handler found for step: #{step_message.step_name}",
            retryable: false
          )

          return TaskerCore::Types::StepTypes::StepResult.failure(
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

        start_time = Time.now

        begin
          # Create handler instance with proper configuration from database
          handler = step_handler_registry.create_handler_instance(resolved_handler)

          # Execute with enhanced (task, sequence, step) interface
          handler_output = handler.call(task, sequence, step)
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          # Process handler output using new standardized result structure
          call_result = process_handler_output(handler_output, execution_time_ms)
          
          # Convert to StepResult based on call_result type
          if call_result.success
            result = TaskerCore::Types::StepTypes::StepResult.success(
              step_id: step_message.step_id,
              task_id: step_message.task_id,
              result_data: call_result.result,
              execution_time_ms: execution_time_ms
            )
            
            # Add orchestration metadata from call_result
            if call_result.metadata && !call_result.metadata.empty?
              result.orchestration_metadata = extract_orchestration_metadata_from_call_result(call_result)
            end
            
            result
          else
            # Handler returned an error result
            error = TaskerCore::Types::StepTypes::StepExecutionError.new(
              error_type: call_result.error_type,
              message: call_result.message,
              retryable: call_result.retryable,
              error_code: call_result.error_code
            )

            TaskerCore::Types::StepTypes::StepResult.failure(
              step_id: step_message.step_id,
              task_id: step_message.task_id,
              error: error,
              execution_time_ms: execution_time_ms
            )
          end
        rescue TaskerCore::Errors::PermanentError, TaskerCore::Errors::RetryableError, TaskerCore::Errors::ValidationError => e
          # Our structured exceptions - convert to StepHandlerCallResult
          execution_time_ms = ((Time.now - start_time) * 1000).to_i
          call_result = TaskerCore::Types::StepHandlerCallResult.from_exception(e)
          
          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: call_result.error_type,
            message: call_result.message,
            retryable: call_result.retryable,
            error_code: call_result.error_code
          )

          TaskerCore::Types::StepTypes::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: execution_time_ms
          )
        rescue => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'HandlerException',
            message: e.message,
            retryable: true,
            stack_trace: e.backtrace.join("\n")
          )

          TaskerCore::Types::StepTypes::StepResult.failure(
            step_id: step_message.step_id,
            task_id: step_message.task_id,
            error: error,
            execution_time_ms: execution_time_ms
          )
        end
      end

      # Process handler output into standardized StepHandlerCallResult
      # @param handler_output [Object] Raw output from handler.call
      # @param execution_time_ms [Integer] Time taken to execute
      # @return [StepHandlerCallResult::Success, StepHandlerCallResult::Error]
      def process_handler_output(handler_output, execution_time_ms)
        # Convert to standardized result structure
        call_result = TaskerCore::Types::StepHandlerCallResult.from_handler_output(handler_output)
        
        # Add execution timing to metadata if not already present
        if call_result.is_a?(TaskerCore::Types::StepHandlerCallResult::Success)
          metadata = call_result.metadata.dup
          metadata[:processing_time_ms] ||= execution_time_ms
          
          # Return updated success result
          TaskerCore::Types::StepHandlerCallResult.success(
            result: call_result.result,
            metadata: metadata
          )
        else
          # Error result - add timing to metadata
          metadata = call_result.metadata.dup
          metadata[:processing_time_ms] ||= execution_time_ms
          
          TaskerCore::Types::StepHandlerCallResult.error(
            error_type: call_result.error_type,
            message: call_result.message,
            error_code: call_result.error_code,
            retryable: call_result.retryable,
            metadata: metadata
          )
        end
      end

      # Extract orchestration metadata from StepHandlerCallResult
      # @param call_result [StepHandlerCallResult::Success] Success result with metadata
      # @return [Hash, nil] Orchestration metadata or nil
      def extract_orchestration_metadata_from_call_result(call_result)
        metadata = call_result.metadata
        return nil if metadata.empty?

        # Map to expected orchestration metadata structure
        {
          http_headers: metadata[:http_headers] || metadata[:headers] || {},
          execution_hints: metadata[:execution_hints] || {},
          backoff_hints: metadata[:backoff_hints] || {},
          error_context: metadata[:error_context],
          processing_time_ms: metadata[:processing_time_ms],
          operation: metadata[:operation],
          input_refs: metadata[:input_refs] || {},
          custom: metadata.reject { |k, _| [:http_headers, :headers, :execution_hints, :backoff_hints, :error_context, :processing_time_ms, :operation, :input_refs].include?(k) }
        }
      end

      # Legacy method for backward compatibility
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
      # Phase 5.2: Send results to orchestration result queue for processing
      # @param result [TaskerCore::Types::StepTypes::StepResult] Step execution result
      def send_result_to_orchestration(result)
        logger.debug("📤 QUEUE_WORKER: Sending result to orchestration - step_id: #{result.step_id}, status: #{result.status.status}")

        begin
          # Get orchestration results queue name from configuration
          orchestration_queue = get_orchestration_results_queue_name
          pgmq_client.create_queue(orchestration_queue)

          # Use StepResult.to_h and add worker-specific metadata
          result_message = result.to_h.merge(
            namespace: namespace,
            processed_at: Time.now.iso8601,
            worker_id: "#{namespace}_worker_#{Process.pid}"
          )

          # Send to orchestration queue
          msg_id = pgmq_client.send_message(orchestration_queue, result_message)
          
          logger.debug("✅ QUEUE_WORKER: Result sent to orchestration - msg_id: #{msg_id}")
          
          msg_id
        rescue => e
          logger.error("❌ QUEUE_WORKER: Failed to send result to orchestration: #{e.message}")
          logger.error("❌ QUEUE_WORKER: #{e.backtrace.first(3).join("\n")}")
          
          # Don't re-raise - this is a secondary operation, main step processing succeeded
          # The orchestration system will detect missing results via other mechanisms
          nil
        end
      end


      # Ensure the queue exists for this namespace
      def ensure_queue_exists
        begin
          pgmq_client.create_queue(queue_name)
        rescue TaskerCore::Error => e
          logger.warn("⚠️ QUEUE_WORKER: Queue creation warning for #{queue_name}: #{e.message}")
        end
      end


      # Create a skip result for steps we can't handle
      def create_skip_result(step_message)
        TaskerCore::Types::StepTypes::StepResult.new(
          step_id: step_message.step_id,
          task_id: step_message.task_id,
          status: TaskerCore::Types::StepExecutionStatus.new(status: 'cancelled'),
          execution_time_ms: 0
        )
      end

      # Create a max retries exceeded result
      def create_max_retries_result(step_message)
        error = TaskerCore::Types::StepTypes::StepExecutionError.new(
          error_type: 'MaxRetriesExceeded',
          message: "Step exceeded maximum retry limit: #{step_message.metadata.max_retries}",
          retryable: false
        )

        TaskerCore::Types::StepTypes::StepResult.failure(
          step_id: step_message.step_id,
          task_id: step_message.task_id,
          error: error,
          execution_time_ms: 0
        )
      end

      # Create an error result for unexpected errors
      def create_error_result(step_message, exception)
        error = TaskerCore::Types::StepTypes::StepExecutionError.new(
          error_type: 'ProcessingError',
          message: exception.message,
          retryable: true,
          stack_trace: exception.backtrace.join("\n")
        )

        TaskerCore::Types::StepTypes::StepResult.failure(
          step_id: step_message.step_id,
          task_id: step_message.task_id,
          error: error,
          execution_time_ms: 0
        )
      end
    end
  end
end

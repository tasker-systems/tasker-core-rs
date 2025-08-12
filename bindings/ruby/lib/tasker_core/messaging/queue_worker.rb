# frozen_string_literal: true

require 'concurrent'
require 'json'
require_relative '../types/step_types'
require_relative '../types/simple_message'
require_relative '../execution/step_sequence'
require_relative 'message_manager'

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
      attr_reader :namespace, :queue_name, :pgmq_client, :step_handler_resolver, :logger,
                  :poll_interval, :visibility_timeout, :batch_size, :max_retries, :shutdown_timeout

      def initialize(namespace,
                     pgmq_client: nil,
                     logger: nil,
                     poll_interval: nil,
                     visibility_timeout: nil,
                     batch_size: nil,
                     max_retries: nil,
                     shutdown_timeout: nil)
        @namespace = namespace
        @queue_name = "#{namespace}_queue"
        @pgmq_client = pgmq_client || PgmqClient.new
        @logger = TaskerCore::Logging::Logger.instance

        # Configuration - read from YAML with environment-specific optimization
        config_defaults = TaskerCore::Config.instance.queue_worker_defaults
        @poll_interval = poll_interval || config_defaults[:poll_interval]
        @visibility_timeout = visibility_timeout || config_defaults[:visibility_timeout]
        @batch_size = batch_size || config_defaults[:batch_size]
        @max_retries = max_retries || config_defaults[:max_retries]
        @shutdown_timeout = shutdown_timeout || config_defaults[:shutdown_timeout]

        # Fail fast if configuration is invalid
        if @poll_interval <= 0
          raise TaskerCore::Errors::ConfigurationError,
                "Invalid poll_interval: #{@poll_interval}. Must be positive number."
        end

        if @visibility_timeout <= 0
          raise TaskerCore::Errors::ConfigurationError,
                "Invalid visibility_timeout: #{@visibility_timeout}. Must be positive number."
        end

        if @batch_size <= 0
          raise TaskerCore::Errors::ConfigurationError,
                "Invalid batch_size: #{@batch_size}. Must be positive number."
        end

        # Log configuration for debugging
        logger&.debug("🔧 QUEUE_WORKER: Configuration loaded for namespace: #{namespace} - poll_interval: #{@poll_interval}s, batch_size: #{@batch_size}")

        # State management
        @running = false
        @worker_thread = nil
        @shutdown_signal = Concurrent::Event.new

        @step_handler_resolver = TaskerCore::Registry::StepHandlerResolver.instance

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
        if @worker_thread&.alive?
          if @worker_thread.join(shutdown_timeout)
            logger.info("✅ QUEUE_WORKER: Worker stopped gracefully for namespace: #{namespace}")
          else
            logger.warn("⚠️ QUEUE_WORKER: Worker shutdown timeout after #{shutdown_timeout}s, using force kill for namespace: #{namespace}")
            force_kill
          end
        end

        @worker_thread = nil
        true
      rescue StandardError => e
        logger.error("❌ QUEUE_WORKER: Error during worker shutdown: #{e.message}")
        # Force cleanup even if there's an error
        force_kill
        true
      end

      # Force kill the worker - aggressively terminates blocking operations
      #
      # @return [Boolean] true if force killed successfully
      def force_kill
        return true unless @worker_thread&.alive?

        logger.warn("🗞️ QUEUE_WORKER: Force killing worker for namespace: #{namespace}")

        begin
          # 1. Set running to false immediately to stop new polling
          @running = false
          @shutdown_signal.set

          # 2. Kill the thread immediately
          @worker_thread.kill
          logger.debug("☠️ QUEUE_WORKER: Killed worker thread for namespace: #{namespace}")

          # 3. Clean up state immediately
          @worker_thread = nil
          logger.debug("✅ QUEUE_WORKER: Force kill completed for namespace: #{namespace}")

          true
        rescue StandardError => e
          logger.error("❌ QUEUE_WORKER: Error during force kill: #{e.message}")
          # Clean up state even if force kill failed
          @running = false
          @worker_thread = nil
          true
        end
      end

      # Check if worker is running
      #
      # @return [Boolean] true if worker is running
      def running?
        @running && @worker_thread&.alive?
      end

      # Handle no handler found result
      #
      # @param task [TaskerCore::Types::TaskTypes::Task] Task
      # @param step [TaskerCore::Types::WorkflowTypes::WorkflowStep] Workflow step
      # @param start_time [Time] Start time
      # @return [TaskerCore::Types::StepTypes::StepResult] Execution result
      def build_no_handler_found_result(task:, step:, start_time:)
        execution_time_ms = ((Time.now - start_time) * 1000).to_i

        error = TaskerCore::Types::StepTypes::StepExecutionError.new(
          error_type: 'HandlerNotFound',
          message: "No handler found for step: #{step.name}",
          retryable: false
        )

        TaskerCore::Types::StepTypes::StepResult.failure(
          step_uuid: step.workflow_step_uuid,
          task_uuid: task.task_uuid,
          error: error,
          execution_time_ms: execution_time_ms
        )
      end

      # Process a successful step message using ActiveRecord lookups (simplified)
      #
      # @param step [TaskerCore::Types::WorkflowStep] Workflow step
      # @param task [TaskerCore::Types::Task] Task
      # @param result [TaskerCore::Types::StepTypes::StepResult] Execution result
      # @param start_time [Time] Start time
      # @return [TaskerCore::Types::StepTypes::StepResult] Execution result
      def build_success_result(step:, task:, result_data:, start_time:)
        execution_time_ms = ((Time.now - start_time) * 1000).to_i

        TaskerCore::Types::StepTypes::StepResult.success(
          step_uuid: step.workflow_step_uuid,
          task_uuid: task.task_uuid,
          execution_time_ms: execution_time_ms,
          result_data: JSON.generate(result_data.to_h)
        )
      end

      # Process a simple step message using ActiveRecord lookups (simplified)
      #
      # @param msg_data [TaskerCore::Types::SimpleQueueMessageData] Simple queue message data
      # @return [TaskerCore::Types::StepTypes::StepResult] Execution result
      def process_simple_step_message(msg_data)
        start_time = Time.now

        logger.info("🔄 QUEUE_WORKER: Processing simple step - step_uuid: #{msg_data.step_uuid}, task_uuid: #{msg_data.task_uuid}")

        begin
          # Fetch ActiveRecord models directly using UUIDs
          task, sequence, step = MessageManager.get_records_from_message(msg_data)

          resolved_handler = step_handler_resolver.resolve_step_handler(task, step)

          unless resolved_handler
            return build_no_handler_found_result(
              step: step,
              task: task,
              start_time: start_time
            )
          end

          # Create handler instance and execute
          handler = step_handler_resolver.create_handler_instance(resolved_handler)

          unless handler.respond_to?(:call)
            return build_no_handler_found_result(
              step: step,
              task: task,
              start_time: start_time
            )
          end

          handler_result = handler.call(task, sequence, step)

          logger.info("✅ QUEUE_WORKER: Simple step completed successfully - step_uuid: #{msg_data.step_uuid}")
          build_success_result(
            task: task,
            step: step,
            start_time: start_time,
            result_data: handler_result
          )
        rescue ActiveRecord::RecordNotFound => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          logger.error("❌ QUEUE_WORKER: Record not found for simple step #{msg_data.step_uuid}: #{e.message}")

          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'RecordNotFound',
            message: "Database record not found: #{e.message}",
            retryable: false
          )

          TaskerCore::Types::StepTypes::StepResult.failure(
            task_uuid: msg_data.task_uuid,
            step_uuid: msg_data.step_uuid,
            error: error,
            execution_time_ms: execution_time_ms
          )
        rescue StandardError => e
          execution_time_ms = ((Time.now - start_time) * 1000).to_i

          logger.error("💥 QUEUE_WORKER: Unexpected error processing simple step #{msg_data.step_uuid}: #{e.message}")
          logger.error("💥 QUEUE_WORKER: #{e.backtrace.first(5).join("\n")}")

          error = TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'UnexpectedError',
            message: e.message,
            retryable: true,
            stack_trace: e.backtrace.join("\n")
          )

          TaskerCore::Types::StepTypes::StepResult.failure(
            task_uuid: msg_data.task_uuid,
            step_uuid: msg_data.step_uuid,
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
        can_handle = step_handler_resolver.can_handle_step?(step_message)

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
        logger.debug("🔍 QUEUE_WORKER: Validating execution context - step_id: #{step_message.step_uuid}, step_name: #{step_message.step_name}")

        unless step_message.namespace == namespace
          logger.debug("❌ QUEUE_WORKER: Namespace mismatch - expected: #{namespace}, got: #{step_message.namespace}")
          return false
        end

        unless step_handler_resolver.can_handle_step?(step_message)
          logger.debug("❌ QUEUE_WORKER: No handler available for step: #{step_message.step_name}")
          return false
        end

        # Validate execution context has required data
        execution_context = step_message.execution_context
        unless execution_context
          logger.debug('❌ QUEUE_WORKER: Missing execution_context')
          return false
        end

        unless execution_context.task && !execution_context.task.empty?
          logger.debug("❌ QUEUE_WORKER: Invalid task context - task: #{execution_context.task.inspect}")
          return false
        end

        unless execution_context.step && !execution_context.step.empty?
          logger.debug("❌ QUEUE_WORKER: Invalid step context - step: #{execution_context.step.inspect}")
          return false
        end

        logger.debug("✅ QUEUE_WORKER: Can extract execution context for step: #{step_message.step_name}")
        true
      rescue StandardError => e
        logger.warn("⚠️ QUEUE_WORKER: Failed to validate execution context: #{e.message}")
        logger.warn("⚠️ QUEUE_WORKER: #{e.backtrace.first(3).join("\n")}")
        false
      end

      private

      # Get orchestration results queue name from configuration
      def get_orchestration_results_queue_name
        config_instance = TaskerCore::Config.instance
        effective_config = config_instance.effective_config

        # Try to get from orchestration.queues.step_results first
        queue_name = effective_config.dig('orchestration', 'queues', 'step_results')

        # Fallback to default if not configured
        queue_name || 'orchestration_step_results'
      rescue StandardError => e
        logger.warn("⚠️ QUEUE_WORKER: Failed to load orchestration queue config: #{e.message}, using default")
        'orchestration_step_results'
      end

      # Main polling loop
      def polling_loop
        logger.debug("🔄 QUEUE_WORKER: Starting polling loop for namespace: #{namespace}")

        while running? && !@shutdown_signal.set?
          begin
            poll_and_process_batch
          rescue Interrupt => e
            logger.info("🛑 QUEUE_WORKER: Received shutdown interrupt for #{namespace}: #{e.message}")
            @running = false
            break
          rescue StandardError => e
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
        logger.debug("🔍 QUEUE_WORKER: Polling queue '#{queue_name}' for messages (batch_size: #{batch_size})")

        # Read messages from queue with timeout for responsive shutdown
        begin
          queue_messages = pgmq_client.read_step_messages(
            namespace,
            visibility_timeout: visibility_timeout,
            qty: batch_size,
            poll_timeout: poll_interval * 2 # Use 2x poll interval as max wait time
          )
        rescue TaskerCore::Errors::DatabaseError => e
          # Handle connection issues gracefully during shutdown/teardown
          if e.message.include?('no connection to the server') ||
             e.message.include?('message contents do not agree with length') ||
             e.message.include?('connection not established') ||
             e.message.include?('Bad file descriptor') ||
             e.message.include?('invalid socket')
            logger.warn("⚠️ QUEUE_WORKER: Database connection issue during polling (likely shutdown): #{e.message}")
            logger.warn("⚠️ QUEUE_WORKER: Terminating polling loop for namespace: #{namespace}")
            @running = false
            return
          else
            # Re-raise other database errors as they might be real issues
            raise
          end
        rescue PG::Error => e
          # Handle direct PostgreSQL errors that might not be wrapped
          if e.message.include?('Bad file descriptor') ||
             e.message.include?('invalid socket') ||
             e.message.include?('no connection to the server')
            logger.warn("⚠️ QUEUE_WORKER: PostgreSQL connection issue during polling (likely shutdown): #{e.message}")
            logger.warn("⚠️ QUEUE_WORKER: Terminating polling loop for namespace: #{namespace}")
            @running = false
            return
          else
            # Re-raise other PostgreSQL errors as they might be real issues
            raise TaskerCore::Errors::DatabaseError, "PostgreSQL error: #{e.message}"
          end
        end

        if queue_messages.empty?
          logger.debug("⏸️ QUEUE_WORKER: No messages found in queue '#{queue_name}'")
          return
        end

        logger.info("📥 QUEUE_WORKER: Received #{queue_messages.length} messages from queue '#{queue_name}' (namespace: #{namespace})")

        # Log details about each message - simple UUID messages only
        queue_messages.each_with_index do |msg_data, index|
          logger.debug("📄 QUEUE_WORKER: Message #{index + 1} - task_uuid: #{msg_data.task_uuid}, step_uuid: #{msg_data.step_uuid}, msg_id: #{msg_data.msg_id}")
        end

        # Simple processing: All messages are processable UUID messages
        processable_messages = queue_messages
        skipped_count = 0

        # 2. MAIN THREAD: Delete messages we're committing to process (handle race conditions gracefully)
        committed_messages = []
        processable_messages.each do |msg_data|
          deleted = pgmq_client.force_delete_message(queue_name, msg_data.msg_id)

          if deleted
            logger.debug("🗑️ QUEUE_WORKER: Force deleted message #{msg_data.msg_id} from queue")
            committed_messages << msg_data
          else
            # Message not found - likely processed by another worker (race condition)
            logger.info("🤝 QUEUE_WORKER: Message #{msg_data.msg_id} already processed by another worker")
            skipped_count += 1
          end
        end

        # 3. CONCURRENT: Execute business logic in parallel (no queue operations)
        # Only process messages we successfully committed to (deleted from queue)
        futures = committed_messages.map do |msg_data|
          Concurrent::Promises.future do
            # Simple UUID-based processing with ActiveRecord models
            process_simple_step_message(msg_data)
          end
        end

        # 4. MAIN THREAD: Wait for all business logic to complete
        execution_results = futures.map(&:value!)

        # 5. MAIN THREAD: Send ONLY committed message results to orchestration
        # These are the only messages orchestration should know about (they won't reappear on queue)
        execution_results.each do |result|
          send_result_to_orchestration(result)
        rescue StandardError => e
          logger.error("❌ QUEUE_WORKER: Failed to send result to orchestration: #{e.message}")
          # Continue processing other results even if one fails
        end

        # Log summary
        successful_executions = execution_results.count(&:success?)
        failed_executions = execution_results.count { |r| !r.success? }
        committed_count = committed_messages.length

        logger.info("✅ QUEUE_WORKER: Batch processing completed - #{committed_count} committed, #{successful_executions} succeeded, #{failed_executions} failed, #{skipped_count} skipped/deferred")
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
        logger.debug("📤 QUEUE_WORKER: Sending result to orchestration - step_id: #{result.step_uuid}, status: #{result.status.status}")

        begin
          # Get orchestration results queue name from configuration
          orchestration_queue = get_orchestration_results_queue_name
          pgmq_client.create_queue(orchestration_queue)

          # Use StepResult.to_h (which now includes all required metadata) and add namespace
          result_message = result.to_h.merge(
            namespace: namespace
          )

          # Send to orchestration queue
          msg_id = pgmq_client.send_message(orchestration_queue, result_message)

          logger.debug("✅ QUEUE_WORKER: Result sent to orchestration - msg_id: #{msg_id}")

          msg_id
        rescue StandardError => e
          logger.error("❌ QUEUE_WORKER: Failed to send result to orchestration: #{e.message}")
          logger.error("❌ QUEUE_WORKER: #{e.backtrace.first(3).join("\n")}")

          # Don't re-raise - this is a secondary operation, main step processing succeeded
          # The orchestration system will detect missing results via other mechanisms
          nil
        end
      end

      # Ensure the queue exists for this namespace
      def ensure_queue_exists
        pgmq_client.create_queue(queue_name)
      rescue TaskerCore::Error => e
        logger.warn("⚠️ QUEUE_WORKER: Queue creation warning for #{queue_name}: #{e.message}")
      end

      # Create a skip result for steps we can't handle
      def create_skip_result(step_message)
        TaskerCore::Types::StepTypes::StepResult.new(
          step_uuid: step_message.step_uuid,
          task_uuid: step_message.task_uuid,
          status: TaskerCore::Types::StepExecutionStatus.new(status: 'cancelled'),
          execution_time_ms: 0,
          completed_at: Time.now
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
          step_uuid: step_message.step_uuid,
          task_uuid: step_message.task_uuid,
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
          step_uuid: step_message.step_uuid,
          task_uuid: step_message.task_uuid,
          error: error,
          execution_time_ms: 0
        )
      end

      # Check if a message is a simple UUID-based message format (Phase 1)
      #
      # @param message [Hash] Raw message from pgmq
      # @return [Boolean] true if this is a simple message with UUIDs
      def is_simple_message?(message)
        return false unless message.is_a?(Hash)

        # Simple messages have exactly 3 fields: task_uuid, step_uuid, ready_dependency_step_uuids
        required_keys = %w[task_uuid step_uuid ready_dependency_step_uuids]
        message.keys.map(&:to_s).sort

        # Check if this looks like a simple message (has the UUID fields)
        has_uuid_fields = required_keys.all? { |key| message.key?(key) || message.key?(key.to_sym) }

        # Additional check: no complex nested structures that indicate legacy format
        no_complex_structures = !message.key?('execution_context') && !message.key?(:execution_context) &&
                                !message.key?('metadata') && !message.key?(:metadata)

        has_uuid_fields && no_complex_structures
      rescue StandardError => e
        logger.debug("⚠️ QUEUE_WORKER: Error checking message format: #{e.message}")
        false
      end

      # Parse a simple UUID-based message into a SimpleStepMessage
      #
      # @param message [Hash] Raw message hash from pgmq
      # @return [TaskerCore::Types::SimpleStepMessage] Parsed simple message
      def parse_simple_message(message)
        TaskerCore::Types::SimpleStepMessage.from_hash(message)
      rescue Dry::Struct::Error => e
        logger.error("❌ QUEUE_WORKER: Failed to parse simple message: #{e.message}")
        logger.error("❌ QUEUE_WORKER: Message content: #{message.inspect}")
        raise TaskerCore::Errors::ValidationError, "Invalid simple message format: #{e.message}"
      end

      # Create a temporary step message for registry lookup (transitional approach)
      # This allows us to use the existing registry while we have ActiveRecord models
      #
      # @param task [TaskerCore::Database::Models::Task] Task ActiveRecord model
      # @param step [TaskerCore::Database::Models::WorkflowStep] Step ActiveRecord model
      # @return [Object] Minimal step message for registry lookup
      def create_temp_step_message_for_registry(task, step)
        # Create minimal execution context for registry lookup
        execution_context_hash = {
          task: {
            task_uuid: task.task_uuid,
            context: task.context
          },
          step: {
            workflow_step_uuid: step.workflow_step_uuid,
            step_name: step.name,
            task_uuid: task.task_uuid
          },
          sequence: []
        }

        # Create minimal step message with just what the registry needs
        OpenStruct.new(
          step_name: step.name,
          step_uuid: step.workflow_step_uuid,
          task_uuid: task.task_uuid,
          namespace: step.task.namespace_name,
          execution_context: OpenStruct.new(execution_context_hash)
        )
      end
    end
  end
end

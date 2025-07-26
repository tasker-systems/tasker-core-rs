# frozen_string_literal: true

require 'json'
require 'logger'
require 'concurrent-ruby'
require 'timeout'

module TaskerCore
  module Orchestration
    # Production-grade batch step execution orchestrator with concurrent workers
    #
    # This class implements the revolutionary ZeroMQ-based concurrent execution architecture
    # that receives batch messages from Rust and executes steps using concurrent-ruby workers.
    # Each worker self-reports partial results via the dual result pattern.
    #
    # @example Basic Usage
    #   orchestrator = BatchStepExecutionOrchestrator.new(
    #     step_sub_endpoint: 'inproc://steps',
    #     result_pub_endpoint: 'inproc://results',
    #     max_workers: 10
    #   )
    #   orchestrator.start
    #   # ... orchestrator runs in background processing batches
    #   orchestrator.stop
    #
    # @example With Custom Handler Registry
    #   registry = TaskerCore::Orchestration::EnhancedHandlerRegistry.new
    #   registry.register_proc('OrderProcessor') do |task, sequence, step|
    #     { status: 'completed', output: process_order(task.context) }
    #   end
    #
    #   orchestrator = BatchStepExecutionOrchestrator.new(
    #     handler_registry: registry,
    #     max_workers: 20
    #   )
    class BatchStepExecutionOrchestrator
      # Use TaskerCore singleton logger for thread-safe logging
      def logger
        TaskerCore::Logging::Logger.instance
      end

      # Note: Type structures have been moved to TaskerCore::Types::OrchestrationTypes
      # for better organization and maintainability

      # @param config [TaskerCore::Config::ZeroMQConfig] Configuration object
      # @param handler_registry [Object] Registry for resolving step handlers/callables
      # @param zmq_orchestrator [ZeromqOrchestrator] Optional pre-configured ZMQ orchestrator
      def initialize(orchestration_manager: nil)
        @config = TaskerCore::Config.instance.zeromq

        # Compose ZeroMQ orchestrator for socket management
        @zmq_orchestrator = ZeromqOrchestrator.new(config: @config)

        # Concurrent worker pool for true parallelism
        @worker_pool = Concurrent::ThreadPoolExecutor.new(
          min_threads: [2, @config.max_workers / 2].min,
          max_threads: @config.max_workers,
          max_queue: @config.max_workers * 2,
          fallback_policy: :caller_runs,
          name: 'batch-step-executor'
        )

        # Orchestration manager for orchestration system
        @orchestration_manager = orchestration_manager

        # Handler registry for resolving callables
        @handler_registry = @orchestration_manager.handler_registry

        # State management
        @running = false
        @listener_thread = nil
        @batch_futures = Concurrent::Map.new
        @worker_counter = Concurrent::AtomicFixnum.new(0)

        logger.info "BatchStepExecutionOrchestrator initialized with #{@config.max_workers} max workers"
      end

      # Start the orchestrator - begins listening for batch messages
      # @return [void]
      def start
        return if @running

        logger.info "Starting BatchStepExecutionOrchestrator with #{@config.max_workers} workers"

        @zmq_orchestrator.start
        @running = true

        # Run batch listener with our message handler
        @zmq_orchestrator.run_listener do |batch_request|
          process_batch_with_workers(batch_request)
        end

        logger.info "BatchStepExecutionOrchestrator started successfully"
      end

      # Stop the orchestrator gracefully
      # @param timeout [Integer] Maximum time to wait for graceful shutdown in seconds
      # @return [void]
      def stop(timeout: 10)
        return unless @running

        logger.info "Stopping BatchStepExecutionOrchestrator"
        @running = false

        # Note: ZeroMQ orchestrator handles its own listener thread

        # Shutdown worker pool gracefully
        @worker_pool.shutdown
        unless @worker_pool.wait_for_termination(timeout)
          logger.warn "Worker pool did not terminate gracefully, forcing shutdown"
          @worker_pool.kill
        end

        # Clean up any remaining batch futures
        @batch_futures.each_value do |futures|
          futures.each(&:cancel) if futures.respond_to?(:each)
        end
        @batch_futures.clear

        # Stop ZeroMQ orchestrator
        @zmq_orchestrator.stop(timeout: timeout)
        logger.info "BatchStepExecutionOrchestrator stopped"
      end

      # Get current orchestrator statistics
      # @return [Hash] Statistics about the orchestrator state
      def stats
        {
          running: @running,
          max_workers: @config.max_workers,
          active_threads: @worker_pool.length,
          queue_length: @worker_pool.queue_length,
          completed_task_count: @worker_pool.completed_task_count,
          active_batches: @batch_futures.size,
          worker_counter: @worker_counter.value,
          zeromq: @zmq_orchestrator.stats
        }
      end

      private

      # Note: ZeroMQ socket setup and batch listening are now handled by ZeromqOrchestrator

      # 🚀 Core Innovation: Concurrent Worker Orchestration
      # Creates concurrent futures for each step in the batch
      def process_batch_with_workers(batch_request)
        batch_id = batch_request[:batch_id]
        steps = batch_request[:steps]

        logger.info "Processing batch #{batch_id} with #{steps.size} steps using concurrent workers"

        # Create concurrent futures for each step
        step_futures = steps.map do |step_data|
          create_step_worker_future(batch_id, step_data)
        end

        # Store futures for potential cancellation/monitoring
        @batch_futures[batch_id] = step_futures

        # Coordinate batch completion in separate future (non-blocking)
        Concurrent::Future.new(executor: @worker_pool) do
          coordinate_batch_completion(batch_id, step_futures)
        end
      end

      # 🔥 Revolutionary Callable Interface: .call(task, sequence, step)
      # Creates a worker future that executes a single step
      def create_step_worker_future(batch_id, step_data)
        Concurrent::Future.new(executor: @worker_pool) do
          worker_id = "worker_#{@worker_counter.increment}"

          begin
            logger.debug "#{worker_id} starting step #{step_data[:step_id]} in batch #{batch_id}"

            # Create validated structs using factory methods
            task, sequence, step = create_execution_structs(step_data)

            # Resolve callable object
            callable = resolve_step_callable(step_data)

            # Execute step with timeout management
            start_time = Time.now
            result = execute_step_with_timeout(callable, task, sequence, step)
            execution_time = ((Time.now - start_time) * 1000).to_i

            # Self-report partial result via ZeroMQ
            @zmq_orchestrator.publish_partial_result(
              batch_id, step_data[:step_id], 'completed',
              result, execution_time, worker_id
            )

            logger.debug "#{worker_id} completed step #{step_data[:step_id]} in #{execution_time}ms"

            {
              step_id: step_data[:step_id],
              status: 'completed',
              result: result,
              execution_time: execution_time,
              worker_id: worker_id
            }

          rescue Timeout::Error => e
            logger.warn "#{worker_id} step #{step_data[:step_id]} timed out in batch #{batch_id}"
            @zmq_orchestrator.publish_partial_result(
              batch_id, step_data[:step_id], 'failed',
              nil, nil, worker_id, e
            )
            { step_id: step_data[:step_id], status: 'failed', error: e, worker_id: worker_id }

          rescue StandardError => e
            logger.error "#{worker_id} step #{step_data[:step_id]} failed in batch #{batch_id}: #{e.message}"

            # Send execution metadata for Rust to determine retryability
            execution_metadata = build_execution_metadata(e, step_data)

            @zmq_orchestrator.publish_partial_result(
              batch_id, step_data[:step_id], 'failed',
              nil, nil, worker_id, e, execution_metadata[:retryable]
            )

            { step_id: step_data[:step_id], status: 'failed', error: e, metadata: execution_metadata, worker_id: worker_id }
          end
        end
      end

      # 🎯 Flexible Callable Resolution: Beyond Class Constraints
      # Resolves any callable object that responds to .call(task, sequence, step)
      def resolve_step_callable(step_data)
        handler_class = step_data[:handler_class]

        # Priority 1: Check for registered callable objects (Procs, Lambdas, etc.)
        if @handler_registry.respond_to?(:get_callable_for_class)
          callable = @handler_registry.get_callable_for_class(handler_class)
          return callable if callable
        end

        # Priority 2: Check if handler_class itself is callable
        return handler_class if handler_class.respond_to?(:call)

        # Priority 3: Traditional handler instance resolution
        handler_instance = get_handler_instance(handler_class)

        # Check if instance has .call method
        return handler_instance if handler_instance.respond_to?(:call)

        # No legacy support - handlers must implement .call

        raise "No callable found for handler class: #{handler_class}"
      end

      # Get handler instance using the registry
      def get_handler_instance(handler_class)
        if @handler_registry.respond_to?(:get_handler_instance)
          @handler_registry.get_handler_instance(handler_class)
        else
          # Fallback: basic class resolution
          handler_class.constantize.new
        end
      rescue StandardError => e
        raise "Could not resolve handler instance for #{handler_class}: #{e.message}"
      end

      # Execute step callable with timeout protection
      def execute_step_with_timeout(callable, task, sequence, step)
        timeout_seconds = step.timeout_seconds

        Timeout.timeout(timeout_seconds) do
          callable.call(task, sequence, step)
        end
      end

      # Create validated execution structs from step data
      # @param step_data [Hash] Raw step data from batch message
      # @return [Array<TaskStruct, SequenceStruct, StepStruct>] Validated structs
      def create_execution_structs(step_data)
        task = Types::StructFactory.create_task(step_data)
        sequence = Types::StructFactory.create_sequence(step_data)
        step = Types::StructFactory.create_step(step_data)

        [task, sequence, step]
      rescue ArgumentError => e
        # If required data is missing, we should fail the step
        raise PermanentError.new(
          "Invalid step data: #{e.message}",
          error_code: 'INVALID_STEP_DATA',
          error_category: 'validation'
        )
      end

      # Build execution metadata for Rust to make retryability decisions
      # @param error [Exception] The error that occurred
      # @param step_data [Hash] Step configuration data
      # @return [Hash] Metadata for Rust TaskFinalizer
      def build_execution_metadata(error, step_data)
        {
          error_type: error.class.name,
          error_message: error.message,
          error_category: categorize_error(error),
          handler_config: step_data[:handler_config] || {},
          execution_context: {
            step_name: step_data[:step_name],
            handler_class: step_data[:handler_class],
            timeout_ms: step_data.dig(:metadata, :timeout_ms)
          },
          retryable: is_error_retryable?(error) # Initial hint, Rust makes final decision
        }
      end

      # Categorize error for Rust TaskFinalizer
      # @param error [Exception] Error to categorize
      # @return [String] Error category
      def categorize_error(error)
        case error
        when Timeout::Error, Net::TimeoutError
          'timeout'
        when PermanentError
          error.error_category || 'permanent'
        when RetryableError
          error.error_category || 'retryable'
        when ValidationError
          'validation'
        when StandardError
          if error.message.match?(/network|connection|unavailable|service/i)
            'infrastructure'
          else
            'business_logic'
          end
        else
          'unknown'
        end
      end

      # Initial retryability hint (Rust makes final decision)
      # @param error [Exception] Error to check
      # @return [Boolean] Whether error might be retryable
      def is_error_retryable?(error)
        case error
        when PermanentError
          false
        when RetryableError
          true
        when Timeout::Error, Net::TimeoutError
          true
        when StandardError
          # Infrastructure errors are typically retryable
          error.message.match?(/network|connection|timeout|unavailable|service/i)
        else
          false
        end
      end

      # 🎯 Future Joining: Batch Completion Coordination
      # Waits for all step futures and publishes batch completion message
      def coordinate_batch_completion(batch_id, step_futures)
        logger.info "Coordinating completion for batch #{batch_id} with #{step_futures.size} workers"

        begin
          # Wait for all step futures to complete (with error handling)
          step_results = step_futures.map do |future|
            future.value! # This will raise if the future failed
          rescue StandardError => e
            logger.warn "Step future failed: #{e.message}"
            { step_id: 0, status: 'failed', error: e, worker_id: 'unknown' }
          end

          # Publish batch completion via ZeroMQ orchestrator
          @zmq_orchestrator.publish_batch_completion(batch_id, step_results)

        rescue StandardError => e
          logger.error "Error coordinating batch #{batch_id} completion: #{e.message}"
          @zmq_orchestrator.publish_batch_error(batch_id, e)
        ensure
          # Clean up futures tracking
          @batch_futures.delete(batch_id)
        end
      end

      # Note: ZeroMQ message handling methods have been moved to ZeromqOrchestrator
    end
  end
end

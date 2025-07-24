# frozen_string_literal: true

require 'ffi-rzmq'
require 'json'
require 'logger'
require 'concurrent-ruby'
require 'dry-struct'
require 'dry-types'
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

      # Type-safe data structures using dry-struct for validation
      module Types
        include Dry.Types()
      end
      
      # Represents a task with its context and metadata
      class TaskStruct < Dry::Struct
        attribute :task_id, Types::Integer
        attribute :context, Types::Hash
        attribute :metadata, Types::Hash.optional.default({})
      end

      # Represents step sequence information for coordination
      class SequenceStruct < Dry::Struct  
        attribute :sequence_number, Types::Integer
        attribute :total_steps, Types::Integer
        attribute :previous_results, Types::Hash
      end

      # Represents an individual step to be executed
      class StepStruct < Dry::Struct
        attribute :step_id, Types::Integer
        attribute :step_name, Types::String
        attribute :handler_config, Types::Hash.default({})
        attribute :timeout_ms, Types::Integer.optional
        attribute :retry_limit, Types::Integer.optional
      end

      # @param step_sub_endpoint [String] ZeroMQ endpoint for receiving step batches from Rust
      # @param result_pub_endpoint [String] ZeroMQ endpoint for publishing results back to Rust
      # @param max_workers [Integer] Maximum number of concurrent workers
      # @param handler_registry [Object] Registry for resolving step handlers/callables
      # @param zmq_context [ZMQ::Context] Optional shared ZMQ context (TCP doesn't require sharing)
      def initialize(
        step_sub_endpoint: 'tcp://127.0.0.1:5555',
        result_pub_endpoint: 'tcp://127.0.0.1:5556', 
        max_workers: 10,
        handler_registry: nil,
        zmq_context: nil
      )
        @step_sub_endpoint = step_sub_endpoint
        @result_pub_endpoint = result_pub_endpoint
        @max_workers = max_workers
        
        # Initialize ZeroMQ context and sockets
        @context = zmq_context
        @step_socket = nil
        @result_socket = nil
        
        # Concurrent worker pool for true parallelism
        @worker_pool = Concurrent::ThreadPoolExecutor.new(
          min_threads: [2, max_workers / 2].min,
          max_threads: max_workers,
          max_queue: max_workers * 2,
          fallback_policy: :caller_runs,
          name: 'batch-step-executor'
        )
        
        # Handler registry for resolving callables
        @handler_registry = handler_registry || OrchestrationManager.instance
        
        # State management
        @running = false
        @listener_thread = nil
        @batch_futures = Concurrent::Map.new
        @worker_counter = Concurrent::AtomicFixnum.new(0)
        
        logger.info "BatchStepExecutionOrchestrator initialized with #{@max_workers} max workers"
      end

      # Start the orchestrator - begins listening for batch messages
      # @return [void]
      def start
        return if @running
        
        logger.info "Starting BatchStepExecutionOrchestrator with #{@max_workers} workers"
        
        setup_zeromq_sockets
        @running = true
        @listener_thread = Thread.new { run_batch_listener }
        
        logger.info "BatchStepExecutionOrchestrator started successfully"
      end

      # Stop the orchestrator gracefully
      # @param timeout [Integer] Maximum time to wait for graceful shutdown in seconds
      # @return [void]
      def stop(timeout: 10)
        return unless @running
        
        logger.info "Stopping BatchStepExecutionOrchestrator"
        @running = false
        
        # Wait for listener thread to finish
        @listener_thread&.join(timeout)
        
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
        
        cleanup_zeromq
        logger.info "BatchStepExecutionOrchestrator stopped"
      end

      # Get current orchestrator statistics
      # @return [Hash] Statistics about the orchestrator state
      def stats
        {
          running: @running,
          max_workers: @max_workers,
          active_threads: @worker_pool.length,
          queue_length: @worker_pool.queue_length,
          completed_task_count: @worker_pool.completed_task_count,
          active_batches: @batch_futures.size,
          worker_counter: @worker_counter.value
        }
      end

      private

      # Set up ZeroMQ sockets for dual result pattern communication
      def setup_zeromq_sockets
        @context ||= ZMQ::Context.new
        
        # Subscribe to step batches from Rust orchestration layer
        @step_socket = @context.socket(ZMQ::SUB)
        @step_socket.connect(@step_sub_endpoint)
        @step_socket.setsockopt(ZMQ::SUBSCRIBE, 'steps')
        
        # Publish partial results and batch completions back to Rust
        @result_socket = @context.socket(ZMQ::PUB)  
        @result_socket.bind(@result_pub_endpoint)
        
        # Allow sockets to establish connections
        sleep(0.1)
        
        logger.debug "ZeroMQ sockets configured: sub=#{@step_sub_endpoint}, pub=#{@result_pub_endpoint}"
      end

      # Main batch listening loop - runs in background thread
      def run_batch_listener
        logger.info "Batch listener started on #{@step_sub_endpoint}"
        
        while @running
          begin
            message = receive_batch_message
            
            if message
              logger.info "Received ZeroMQ message: #{message[0..100]}..."
            end
            
            next unless message

            batch_request = parse_batch_message(message)
            
            if batch_request
              logger.info "Parsed batch request: batch_id=#{batch_request[:batch_id]}"
            else
              logger.warn "Failed to parse batch message"
            end
            
            next unless batch_request

            process_batch_with_workers(batch_request)
            
          rescue => e
            logger.error "Batch listener error: #{e.message}"
            publish_batch_error(batch_request&.dig(:batch_id), e) if batch_request
          end
          
          # Prevent busy-waiting while allowing responsive shutdown
          sleep(0.001)
        end
        
        logger.debug "Batch listener stopped"
      end

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
            
            # Build type-safe data structures
            task = TaskStruct.new(
              task_id: step_data[:task_id],
              context: step_data[:task_context] || {},
              metadata: step_data.dig(:metadata) || {}
            )
            
            sequence = SequenceStruct.new(
              sequence_number: step_data.dig(:metadata, :sequence) || 1,
              total_steps: step_data.dig(:metadata, :total_steps) || 1,
              previous_results: step_data[:previous_results] || {}
            )
            
            step = StepStruct.new(
              step_id: step_data[:step_id],
              step_name: step_data[:step_name], 
              handler_config: step_data[:handler_config] || {},
              timeout_ms: step_data.dig(:metadata, :timeout_ms),
              retry_limit: step_data.dig(:metadata, :retry_limit)
            )
            
            # 🎯 BREAKING CHANGE: Flexible Callable Interface
            # Resolve callable object (Proc, Lambda, class, etc.)
            callable = resolve_step_callable(step_data)
            
            # Execute step with timeout management
            start_time = Time.now
            result = execute_step_with_timeout(callable, task, sequence, step)
            execution_time = ((Time.now - start_time) * 1000).to_i
            
            # Self-report partial result via ZeroMQ dual pattern
            publish_partial_result(batch_id, step_data[:step_id], 'completed', result, execution_time, worker_id)
            
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
            publish_partial_result(batch_id, step_data[:step_id], 'failed', nil, nil, worker_id, e)
            { step_id: step_data[:step_id], status: 'failed', error: e, worker_id: worker_id }
            
          rescue => e
            logger.error "#{worker_id} step #{step_data[:step_id]} failed in batch #{batch_id}: #{e.message}"
            retryable = determine_retryability(e, step_data[:handler_config])
            publish_partial_result(batch_id, step_data[:step_id], 'failed', nil, nil, worker_id, e, retryable)
            { step_id: step_data[:step_id], status: 'failed', error: e, retryable: retryable, worker_id: worker_id }
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
        
        # Legacy fallback: Wrap existing .process method in callable
        if handler_instance.respond_to?(:process)
          return ->(task, sequence, step) { handler_instance.process(task, sequence, step) }
        end
        
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
      rescue => e
        raise "Could not resolve handler instance for #{handler_class}: #{e.message}"
      end

      # Execute step callable with timeout protection
      def execute_step_with_timeout(callable, task, sequence, step)
        timeout_seconds = (step.timeout_ms || 30_000) / 1000.0
        
        Timeout.timeout(timeout_seconds) do
          callable.call(task, sequence, step)
        end
      end

      # 🚀 Dual Result Pattern: Partial Results via ZeroMQ
      # Self-reporting mechanism for workers to publish step completion
      def publish_partial_result(batch_id, step_id, status, result, execution_time, worker_id, error = nil, retryable = true)
        partial_result = {
          message_type: 'partial_result',
          batch_id: batch_id,
          step_id: step_id,
          status: status,
          output: result,
          execution_time_ms: execution_time,
          worker_id: worker_id,
          sequence: 1, # Could be enhanced for multi-sequence steps
          timestamp: Time.now.utc.iso8601,
          error: error ? serialize_error(error) : nil,
          retryable: retryable
        }
        
        publish_to_results_socket('partial_result', partial_result)
        logger.debug "Published partial result for step #{step_id} in batch #{batch_id}"
      end

      # 🎯 Future Joining: Batch Completion Coordination
      # Waits for all step futures and publishes batch completion message
      def coordinate_batch_completion(batch_id, step_futures)
        logger.info "Coordinating completion for batch #{batch_id} with #{step_futures.size} workers"
        
        begin
          # Wait for all step futures to complete (with error handling)
          step_results = step_futures.map do |future|
            future.value! # This will raise if the future failed
          rescue => e
            logger.warn "Step future failed: #{e.message}"
            { step_id: 0, status: 'failed', error: e, worker_id: 'unknown' }
          end
          
          # Aggregate results for batch completion message
          completed_steps = step_results.count { |r| r[:status] == 'completed' }
          failed_steps = step_results.count { |r| r[:status] == 'failed' }
          total_execution_time = step_results.sum { |r| r[:execution_time] || 0 }
          
          # Create step summaries for Rust reconciliation
          step_summaries = step_results.map do |result|
            {
              step_id: result[:step_id],
              final_status: result[:status],
              execution_time_ms: result[:execution_time],
              worker_id: result[:worker_id] || "unknown"
            }
          end
          
          # 🎯 Batch Completion Message via ZeroMQ Dual Pattern
          batch_completion = {
            message_type: 'batch_completion',
            batch_id: batch_id,
            protocol_version: '2.0',
            total_steps: step_futures.size,
            completed_steps: completed_steps,
            failed_steps: failed_steps,
            in_progress_steps: 0,
            step_summaries: step_summaries,
            completed_at: Time.now.utc.iso8601,
            total_execution_time_ms: total_execution_time
          }
          
          publish_to_results_socket('batch_completion', batch_completion)
          
          logger.info "Batch #{batch_id} completed: #{completed_steps} succeeded, #{failed_steps} failed"
          
        rescue => e
          logger.error "Error coordinating batch #{batch_id} completion: #{e.message}"
          publish_batch_error(batch_id, e)
        ensure
          # Clean up futures tracking
          @batch_futures.delete(batch_id)
        end
      end

      # Publish message to results socket with proper formatting
      def publish_to_results_socket(topic, message)
        full_message = "#{topic} #{message.to_json}"
        
        if @result_socket
          @result_socket.send_string(full_message)
        else
          logger.error "Cannot publish #{topic}: result socket not available"
        end
      end

      # Determine if an error should trigger a retry
      def determine_retryability(error, handler_config)
        # Check handler-specific configuration first
        return handler_config[:retryable] if handler_config.key?(:retryable)
        return handler_config['retryable'] if handler_config.key?('retryable')
        
        # Smart retryability based on error type
        case error
        when Timeout::Error, Net::TimeoutError
          true
        when StandardError
          # Network/infrastructure errors are typically retryable
          # Business logic errors are typically not retryable
          error.message.match?(/network|connection|timeout|unavailable|service/i)
        else
          false
        end
      end

      # Serialize error for JSON transmission
      def serialize_error(error)
        {
          message: error.message,
          type: error.class.name,
          backtrace: error.backtrace&.first(5) || []
        }
      end

      # Receive batch message from ZeroMQ (non-blocking)
      def receive_batch_message
        return nil unless @step_socket
        
        message = String.new
        rc = @step_socket.recv_string(message, ZMQ::DONTWAIT)
        rc == 0 ? message : nil
      end

      # Parse received batch message
      def parse_batch_message(message)
        topic, json_data = message.split(' ', 2)
        return nil unless topic == 'steps'
        
        JSON.parse(json_data, symbolize_names: true)
      rescue JSON::ParserError => e
        logger.error "Failed to parse batch message: #{e.message}"
        nil
      end

      # Publish error message for batch processing failure
      def publish_batch_error(batch_id, error)
        error_message = {
          message_type: 'batch_error',
          batch_id: batch_id,
          error: serialize_error(error),
          timestamp: Time.now.utc.iso8601
        }
        
        publish_to_results_socket('batch_error', error_message)
      end

      # Clean up ZeroMQ resources
      def cleanup_zeromq
        @step_socket&.close
        @result_socket&.close
        @context&.terminate
        
        @step_socket = nil
        @result_socket = nil
        @context = nil
        
        logger.debug "ZeroMQ resources cleaned up"
      end
    end
  end
end
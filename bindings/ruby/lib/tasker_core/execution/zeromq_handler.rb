# frozen_string_literal: true

require 'ffi-rzmq'
require 'json'
require 'logger'

module TaskerCore
  module Execution
    # ZeroMQ pub-sub handler for language-agnostic step execution
    #
    # This handler implements fire-and-forget step execution using ZeroMQ pub-sub pattern:
    # - Subscribes to "steps" topic for receiving step batches from Rust
    # - Publishes to "results" topic for sending execution results back to Rust
    # - Processes steps using existing Ruby step handler infrastructure
    # - Eliminates timeout/idempotency issues of FFI approach
    #
    # @example Basic Usage
    #   handler = TaskerCore::Execution::ZeroMQHandler.new(
    #     step_sub_endpoint: 'inproc://steps',
    #     result_pub_endpoint: 'inproc://results'
    #   )
    #   handler.start
    #   # Handler runs in background thread processing steps
    #   handler.stop
    #
    class ZeroMQHandler
      # Default endpoints for ZeroMQ communication
      DEFAULT_STEP_SUB_ENDPOINT = 'inproc://steps'
      DEFAULT_RESULT_PUB_ENDPOINT = 'inproc://results'
      DEFAULT_POLL_INTERVAL_MS = 1

      # @param step_sub_endpoint [String] ZeroMQ endpoint for subscribing to step batches
      # @param result_pub_endpoint [String] ZeroMQ endpoint for publishing results
      # @param handler_registry [Object] Registry for looking up task and step handlers
      # @param logger [Logger] Logger instance for debugging and monitoring
      def initialize(
        step_sub_endpoint: DEFAULT_STEP_SUB_ENDPOINT,
        result_pub_endpoint: DEFAULT_RESULT_PUB_ENDPOINT,
        handler_registry: nil,
        logger: nil
      )
        @step_sub_endpoint = step_sub_endpoint
        @result_pub_endpoint = result_pub_endpoint
        @handler_registry = handler_registry || TaskerCore::Internal::OrchestrationManager.instance
        @logger = logger || Logger.new($stdout, level: Logger::INFO)
        @running = false
        @thread = nil

        setup_zeromq_sockets
      end

      # Start the ZeroMQ handler in a background thread
      #
      # @return [Thread] The background thread running the handler
      def start
        return @thread if @running

        @running = true
        @thread = Thread.new { run_handler_loop }
        @logger.info "ZeroMQ Handler started on #{@step_sub_endpoint} -> #{@result_pub_endpoint}"
        @thread
      end

      # Stop the ZeroMQ handler gracefully
      #
      # @param timeout [Numeric] Maximum time to wait for graceful shutdown (seconds)
      # @return [Boolean] True if stopped gracefully, false if forced
      def stop(timeout: 5)
        return true unless @running

        @running = false
        @logger.info 'ZeroMQ Handler stopping...'

        if @thread&.join(timeout)
          @logger.info 'ZeroMQ Handler stopped gracefully'
          cleanup_sockets
          true
        else
          @logger.warn 'ZeroMQ Handler forced stop after timeout'
          @thread&.kill
          cleanup_sockets
          false
        end
      end

      # Check if the handler is currently running
      #
      # @return [Boolean] True if running, false otherwise
      def running?
        @running && @thread&.alive?
      end

      private

      # Set up ZeroMQ context and sockets
      def setup_zeromq_sockets
        @context = ZMQ::Context.new

        # Subscriber for receiving step batches
        @step_socket = @context.socket(ZMQ::SUB)
        raise "Failed to create SUB socket" unless @step_socket
        
        rc = @step_socket.connect(@step_sub_endpoint)
        raise "Failed to connect to #{@step_sub_endpoint}: #{ZMQ::Util.error_string}" unless ZMQ::Util.resultcode_ok?(rc)
        
        rc = @step_socket.setsockopt(ZMQ::SUBSCRIBE, 'steps')
        raise "Failed to subscribe to 'steps' topic: #{ZMQ::Util.error_string}" unless ZMQ::Util.resultcode_ok?(rc)

        # Publisher for sending results
        @result_socket = @context.socket(ZMQ::PUB)
        raise "Failed to create PUB socket" unless @result_socket
        
        rc = @result_socket.bind(@result_pub_endpoint)
        raise "Failed to bind to #{@result_pub_endpoint}: #{ZMQ::Util.error_string}" unless ZMQ::Util.resultcode_ok?(rc)

        @logger.debug "ZeroMQ sockets configured: SUB(#{@step_sub_endpoint}) PUB(#{@result_pub_endpoint})"
      end

      # Clean up ZeroMQ sockets and context
      def cleanup_sockets
        @step_socket&.close
        @result_socket&.close
        @context&.terminate
        @logger.debug 'ZeroMQ sockets cleaned up'
      end

      # Main handler loop - runs in background thread
      def run_handler_loop
        @logger.info 'ZeroMQ Handler loop started'

        while @running
          message = receive_step_message
          
          if message
            process_message_async(message)
          else
            # No message available, brief sleep to prevent busy-waiting
            sleep(DEFAULT_POLL_INTERVAL_MS / 1000.0)
          end
        end

        @logger.info 'ZeroMQ Handler loop ended'
      rescue => e
        @logger.error "ZeroMQ Handler loop error: #{e.message}"
        @logger.error e.backtrace.join("\n")
      end

      # Receive step message from ZeroMQ (non-blocking)
      #
      # @return [String, nil] Message string or nil if no message available
      def receive_step_message
        message = String.new
        rc = @step_socket.recv_string(message, ZMQ::DONTWAIT)
        
        if ZMQ::Util.resultcode_ok?(rc)
          @logger.debug "Received message: #{message[0..100]}#{'...' if message.length > 100}"
          message
        elsif rc == -1 && ZMQ::Util.errno == ZMQ::EAGAIN
          # No message available - this is normal for non-blocking receive
          nil
        else
          @logger.error "ZMQ receive error: #{ZMQ::Util.error_string}"
          nil
        end
      end

      # Process message asynchronously to avoid blocking the main loop
      #
      # @param message [String] Raw ZeroMQ message
      def process_message_async(message)
        Thread.new do
          begin
            process_step_message(message)
          rescue => e
            @logger.error "Error processing message: #{e.message}"
            @logger.error e.backtrace.join("\n")
          end
        end
      end

      # Process a step message from ZeroMQ
      #
      # @param message [String] Raw ZeroMQ message with topic and JSON data
      def process_step_message(message)
        # Parse topic and JSON data
        parts = message.split(' ', 2)
        unless parts.length == 2 && parts[0] == 'steps'
          @logger.warn "Invalid message format or topic: #{message[0..50]}"
          return
        end

        topic, json_data = parts
        request = JSON.parse(json_data, symbolize_names: true)
        
        @logger.info "Processing step batch #{request[:batch_id]} with #{request[:steps]&.length || 0} steps"

        # Process the batch
        response = process_step_batch(request)
        
        # Publish results
        publish_results(response)
        
        @logger.debug "Completed step batch #{request[:batch_id]}"
      rescue JSON::ParserError => e
        @logger.error "JSON parse error: #{e.message}"
        publish_error_response(nil, e)
      rescue => e
        @logger.error "Step processing error: #{e.message}"
        publish_error_response(request&.dig(:batch_id), e)
      end

      # Process a batch of steps
      #
      # @param request [Hash] Step batch request with batch_id and steps array
      # @return [Hash] Step batch response with results
      def process_step_batch(request)
        batch_id = request[:batch_id]
        steps = request[:steps] || []
        
        results = steps.map do |step|
          process_single_step(step)
        end

        {
          batch_id: batch_id,
          protocol_version: request[:protocol_version] || '1.0',
          results: results
        }
      end

      # Process a single step within a batch
      #
      # @param step [Hash] Step execution request
      # @return [Hash] Step execution result
      def process_single_step(step)
        start_time = Time.now
        step_id = step[:step_id]
        
        @logger.debug "Processing step #{step_id}: #{step[:step_name]}"

        # Get handler for this step
        handler = get_handler_for_step(step)
        
        # Build execution objects from step data
        task = build_task_object(step)
        sequence = build_sequence_object(step) 
        step_obj = build_step_object(step)

        # Execute using existing handler interface
        result = handler.process(task, sequence, step_obj)
        execution_time_ms = ((Time.now - start_time) * 1000).round

        # Build successful response
        handler_version = begin
          handler.class::VERSION
        rescue
          '1.0.0'
        end

        {
          step_id: step_id,
          status: 'completed',
          output: result,
          error: nil,
          metadata: {
            execution_time_ms: execution_time_ms,
            handler_version: handler_version,
            retryable: false, # Success doesn't need retry
            completed_at: Time.now.utc.iso8601
          }
        }
      rescue => e
        execution_time_ms = ((Time.now - start_time) * 1000).round
        @logger.error "Step #{step_id} failed: #{e.message}"
        
        {
          step_id: step_id,
          status: 'failed',
          output: nil,
          error: {
            message: e.message,
            error_type: e.class.name,
            backtrace: e.backtrace&.first(5),
            retryable: determine_retryability(e)
          },
          metadata: {
            execution_time_ms: execution_time_ms,
            retryable: determine_retryability(e),
            completed_at: Time.now.utc.iso8601
          }
        }
      end

      # Get the appropriate handler for a step
      #
      # @param step [Hash] Step execution request
      # @return [Object] Step handler instance
      def get_handler_for_step(step)
        task_id = step[:task_id]
        step_name = step[:step_name]
        
        # Use the Ruby TaskHandler registry to find the appropriate handler
        task_handler = @handler_registry.get_task_handler_for_task(task_id)
        raise "No task handler found for task #{task_id}" unless task_handler

        step_handler = task_handler.get_step_handler_from_name(step_name)
        raise "No step handler found for step '#{step_name}' in task #{task_id}" unless step_handler

        step_handler
      end

      # Build task object from step data
      #
      # @param step [Hash] Step execution request
      # @return [TaskerCore::Models::Task] Task object
      def build_task_object(step)
        TaskerCore::Models::Task.new(
          id: step[:task_id],
          context: step[:task_context] || {},
          metadata: step[:metadata] || {}
        )
      end

      # Build step sequence object from step data
      #
      # @param step [Hash] Step execution request  
      # @return [TaskerCore::Models::StepSequence] Step sequence object
      def build_sequence_object(step)
        TaskerCore::Models::StepSequence.new(
          task_id: step[:task_id],
          previous_results: step[:previous_results] || {},
          metadata: step[:metadata] || {}
        )
      end

      # Build step object from step data
      #
      # @param step [Hash] Step execution request
      # @return [TaskerCore::Models::Step] Step object  
      def build_step_object(step)
        TaskerCore::Models::Step.new(
          id: step[:step_id],
          name: step[:step_name],
          config: step[:handler_config] || {},
          metadata: step[:metadata] || {}
        )
      end

      # Publish results to ZeroMQ
      #
      # @param response [Hash] Step batch response
      def publish_results(response)
        result_message = "results #{response.to_json}"
        rc = @result_socket.send_string(result_message)
        
        unless ZMQ::Util.resultcode_ok?(rc)
          @logger.error "Failed to publish results: #{ZMQ::Util.error_string}"
        else
          @logger.debug "Published results for batch #{response[:batch_id]}"
        end
      end

      # Publish error response for batch processing failures
      #
      # @param batch_id [String, nil] Batch ID if available
      # @param error [Exception] The error that occurred
      def publish_error_response(batch_id, error)
        error_response = {
          batch_id: batch_id || 'unknown',
          protocol_version: '1.0',
          results: [],
          error: {
            message: error.message,
            error_type: error.class.name,
            backtrace: error.backtrace&.first(5)
          }
        }

        publish_results(error_response)
      end

      # Determine if an error should trigger a retry
      #
      # @param error [Exception] The error to evaluate
      # @return [Boolean] True if retryable, false otherwise
      def determine_retryability(error)
        case error
        when StandardError
          # Most standard errors are retryable
          true
        when SystemExit, NoMemoryError, SystemStackError
          # System-level errors are not retryable
          false
        else
          # Default to retryable for unknown error types
          true
        end
      end
    end
  end
end
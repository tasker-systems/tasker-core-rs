# frozen_string_literal: true

require 'ffi-rzmq'
require 'json'
require 'concurrent-ruby'

module TaskerCore
  module Orchestration
    # ZeroMQ orchestration class responsible for socket management, 
    # message handling, and cross-language communication with Rust.
    #
    # This class encapsulates all ZeroMQ-specific functionality, including:
    # - Socket setup and cleanup
    # - Message receiving and publishing
    # - Batch message parsing and validation
    # - Result publishing with dual result pattern
    #
    # @example Basic usage
    #   orchestrator = ZeromqOrchestrator.new
    #   orchestrator.start
    #   orchestrator.publish_partial_result(batch_id, step_id, 'completed', result)
    #   orchestrator.stop
    #
    class ZeromqOrchestrator
      # Use TaskerCore singleton logger for thread-safe logging
      def logger
        TaskerCore::Logging::Logger.instance
      end

      # @param config [TaskerCore::Config::ZeroMQConfig] ZeroMQ configuration
      # @param zmq_context [ZMQ::Context] Optional shared ZMQ context
      def initialize(config: nil, zmq_context: nil)
        @config = config || TaskerCore::Config.instance.zeromq
        @zmq_context = zmq_context
        
        # Socket management
        @context = nil
        @step_socket = nil
        @result_socket = nil
        
        # State management
        @running = false
        @listener_thread = nil
        
        logger.info "ZeromqOrchestrator initialized with config: #{@config.to_debug_info}"
      end

      # Start ZeroMQ orchestration - sets up sockets and begins listening
      # @return [void]
      def start
        return if @running
        
        logger.info "Starting ZeromqOrchestrator"
        setup_sockets
        @running = true
        
        logger.info "ZeromqOrchestrator started successfully"
      end

      # Stop ZeroMQ orchestration gracefully
      # @param timeout [Integer] Maximum time to wait for graceful shutdown
      # @return [void]
      def stop(timeout: 10)
        return unless @running
        
        logger.info "Stopping ZeromqOrchestrator"
        @running = false
        
        # Wait for listener thread if running independently
        @listener_thread&.join(timeout) if @listener_thread
        
        cleanup_sockets
        logger.info "ZeromqOrchestrator stopped"
      end

      # Check if orchestrator is running
      # @return [Boolean]
      def running?
        @running
      end

      # Get orchestrator statistics
      # @return [Hash] Current status and configuration
      def stats
        {
          running: @running,
          config: @config.to_h,
          sockets: {
            step_socket_connected: !@step_socket.nil?,
            result_socket_connected: !@result_socket.nil?
          }
        }
      end

      # Receive a batch message from the step socket (non-blocking)
      # @return [Hash, nil] Parsed batch request or nil if no message
      def receive_batch_message
        return nil unless @step_socket && @running
        
        message = String.new
        rc = @step_socket.recv_string(message, ZMQ::DONTWAIT)
        
        return nil unless rc == 0
        
        logger.debug "Received ZeroMQ message: #{message[0..100]}..."
        parse_batch_message(message)
      end

      # Publish partial result message via dual result pattern
      # @param batch_id [String] Batch identifier
      # @param step_id [Integer] Step identifier
      # @param status [String] Step status ('completed', 'failed')
      # @param result [Hash] Step execution result
      # @param execution_time [Integer] Execution time in milliseconds
      # @param worker_id [String] Worker identifier
      # @param error [Exception, nil] Error if step failed
      # @param retryable [Boolean] Whether the error is retryable
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

      # Publish batch completion message
      # @param batch_id [String] Batch identifier  
      # @param step_results [Array<Hash>] Results from all steps
      def publish_batch_completion(batch_id, step_results)
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
        
        # Batch completion message via ZeroMQ dual pattern
        batch_completion = {
          message_type: 'batch_completion',
          batch_id: batch_id,
          protocol_version: '2.0',
          total_steps: step_results.size,
          completed_steps: completed_steps,
          failed_steps: failed_steps,
          in_progress_steps: 0,
          step_summaries: step_summaries,
          completed_at: Time.now.utc.iso8601,
          total_execution_time_ms: total_execution_time
        }
        
        publish_to_results_socket('batch_completion', batch_completion)
        logger.info "Batch #{batch_id} completed: #{completed_steps} succeeded, #{failed_steps} failed"
      end

      # Publish error message for batch processing failure
      # @param batch_id [String] Batch identifier
      # @param error [Exception] Error that occurred
      def publish_batch_error(batch_id, error)
        error_message = {
          message_type: 'batch_error',
          batch_id: batch_id,
          error: serialize_error(error),
          timestamp: Time.now.utc.iso8601
        }
        
        publish_to_results_socket('batch_error', error_message)
        logger.error "Published batch error for #{batch_id}: #{error.message}"
      end

      # Run the batch listener loop (for independent operation)
      # @param message_handler [Proc] Block to handle received batch messages
      def run_listener(&message_handler)
        raise ArgumentError, "Message handler block required" unless block_given?
        
        @listener_thread = Thread.new do
          logger.info "Batch listener started on #{@config.step_sub_endpoint}"
          
          while @running
            begin
              batch_request = receive_batch_message
              
              if batch_request
                logger.info "Parsed batch request: batch_id=#{batch_request[:batch_id]}"
                message_handler.call(batch_request)
              end
              
            rescue StandardError => e
              logger.error "Batch listener error: #{e.message}"
              publish_batch_error(batch_request&.dig(:batch_id), e) if batch_request
            end
            
            # Prevent busy-waiting while allowing responsive shutdown
            sleep(@config.poll_interval_ms / 1000.0)
          end
          
          logger.debug "Batch listener stopped"
        end
      end

      private

      # Set up ZeroMQ sockets using configuration
      def setup_sockets
        @context = @zmq_context || ZMQ::Context.new
        
        # Subscribe to step batches from Rust orchestration layer
        @step_socket = @context.socket(ZMQ::SUB)
        @step_socket.connect(@config.step_sub_endpoint)
        @step_socket.setsockopt(ZMQ::SUBSCRIBE, 'steps')
        @step_socket.setsockopt(ZMQ::RCVHWM, @config.step_queue_hwm)
        
        # Publish partial results and batch completions back to Rust
        @result_socket = @context.socket(ZMQ::PUB)
        @result_socket.bind(@config.result_pub_endpoint)
        @result_socket.setsockopt(ZMQ::SNDHWM, @config.result_queue_hwm)
        
        # Allow sockets to establish connections
        sleep(0.1)
        
        logger.debug "ZeroMQ sockets configured: sub=#{@config.step_sub_endpoint}, pub=#{@config.result_pub_endpoint}"
      end

      # Parse received batch message
      # @param message [String] Raw ZeroMQ message
      # @return [Hash, nil] Parsed batch request or nil if invalid
      def parse_batch_message(message)
        topic, json_data = message.split(' ', 2)
        return nil unless topic == 'steps'
        
        JSON.parse(json_data, symbolize_names: true)
      rescue JSON::ParserError => e
        logger.error "Failed to parse batch message: #{e.message}"
        nil
      end

      # Publish message to results socket with proper formatting
      # @param topic [String] Message topic
      # @param message [Hash] Message data
      def publish_to_results_socket(topic, message)
        return unless @result_socket

        full_message = "#{topic} #{message.to_json}"
        
        begin
          @result_socket.send_string(full_message)
        rescue StandardError => e
          logger.error "Failed to publish #{topic}: #{e.message}"
        end
      end

      # Serialize error for JSON transmission
      # @param error [Exception] Error to serialize
      # @return [Hash] Serialized error data
      def serialize_error(error)
        {
          message: error.message,
          type: error.class.name,
          backtrace: error.backtrace&.first(5) || []
        }
      end

      # Clean up ZeroMQ resources
      def cleanup_sockets
        @step_socket&.close
        @result_socket&.close
        
        # Only terminate context if we created it
        @context&.terminate unless @zmq_context
        
        @step_socket = nil
        @result_socket = nil
        @context = nil
        
        logger.debug "ZeroMQ resources cleaned up"
      end
    end
  end
end
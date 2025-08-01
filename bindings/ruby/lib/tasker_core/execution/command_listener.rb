# frozen_string_literal: true

require 'logger'
require 'socket'
require 'json'
require 'securerandom'
require 'concurrent'
require_relative '../types/execution_types'

module TaskerCore
  module Execution
    class CommandListener
      # Default configuration
      DEFAULT_TIMEOUT = 30
      DEFAULT_MAX_CONNECTIONS = 10


      def config
        TaskerCore::Config.instance
      end

      attr_reader :host, :port, :timeout, :logger, :rust_listener, :running,
                  :max_connections

      def initialize(host: nil, port: nil, timeout: nil, max_connections: DEFAULT_MAX_CONNECTIONS)
        @logger = TaskerCore::Logging::Logger.instance
        @connected = false

        # Use provided config or fall back to defaults from config
        @host = host || config.command_listener_host
        @port = port || config.command_listener_port
        @timeout = timeout || config.command_listener_timeout
        @max_connections = max_connections || DEFAULT_MAX_CONNECTIONS

        @rust_listener = TaskerCore::CommandListener.new_with_config({
          bind_address: @host,
          port: @port,
          max_connections: @max_connections,
          connection_timeout_ms: @timeout
        })

        # Initialize default handlers (ExecuteBatch)
        register_default_handlers
      end


      # Start the command listener server (delegates to Rust FFI)
      #
      # @return [Boolean] true if started successfully
      # @raise [ListenerError] if startup fails
      def start
        return true if running?

        begin
          @rust_listener.start
          logger.info("✅ RUST_LISTENER: CommandListener started via Rust FFI delegation on #{@host}:#{@port}")
          
          # Brief pause to ensure server is accepting connections
          sleep(0.1)
          
          # Verify the server is actually running
          unless running?
            raise ListenerError, "Server failed to start - not in running state"
          end
          
          logger.info("✅ RUST_LISTENER: CommandListener verified as running and ready to accept connections")
          true
        rescue StandardError => e
          logger.error("❌ RUST_LISTENER: Failed to start TCP server: #{e.message}")
          raise ListenerError, "Failed to start TCP server: #{e.message}"
        end
      end

      # Stop the command listener server (delegates to Rust FFI)
      #
      # @return [Boolean] true if stopped successfully
      def stop
        return true unless running?

        begin
          @rust_listener.stop
          logger.info("✅ RUST_LISTENER: CommandListener stopped via Rust FFI delegation")
          true
        rescue StandardError => e
          logger.error("❌ RUST_LISTENER: Error stopping TCP server: #{e.message}")
          false
        end
      end

      # Check if server is running (delegates to Rust FFI)
      #
      # @return [Boolean] true if running
      def running?
        @rust_listener.running?
      end

      # Get current server statistics (delegates to Rust FFI)
      #
      # @return [Hash] Server stats
      def stats
        rust_stats = @rust_listener.get_statistics
        {
          running: running?,
          host: host,
          port: port,
          max_connections: max_connections,
          server_type: "rust_ffi_delegated",
          rust_stats: rust_stats
        }
      end

      private

      # Register ExecuteBatch handler for Rust FFI delegation
      def register_default_handlers
        logger.info("🚀 RUST_LISTENER: Setting up ExecuteBatch handler for Rust FFI delegation")

        # Register ExecuteBatch handler with the Rust command listener
        # This tells the Rust side that we can handle ExecuteBatch commands
        @rust_listener.register_command_handler("ExecuteBatch", "ruby_batch_execution_handler")

        # Register the Ruby callback with the Rust FFI bridge
        # The RubyCommandBridge will handle acknowledgment responses, so we just focus on processing
        @rust_listener.register_ruby_callback("ExecuteBatch", create_execute_batch_handler)

        logger.info("✅ RUST_LISTENER: ExecuteBatch handler registered with Rust FFI")
      end

      # Create the ExecuteBatch handler proc
      # RubyCommandBridge handles immediate acknowledgment, so this focuses on actual processing
      def create_execute_batch_handler
        proc do |command|
          logger.info("🚀 RUBY_HANDLER: Received ExecuteBatch command for processing: #{command['command_id']}")

          # Start async processing using concurrent-ruby future
          # RubyCommandBridge already sent acknowledgment, so we focus on the actual work
          Concurrent::Promises.future do
            begin
              logger.info("🔄 ASYNC_PROCESSING: Starting background batch processing for #{command['command_id']}")
              
              # Delegate to BatchExecutionHandler (this will send ReportPartialResult and ReportBatchCompletion)
              handler = TaskerCore::Execution::BatchExecutionHandler.new
              handler.handle(command)
              
              logger.info("✅ ASYNC_PROCESSING: Background batch processing completed for #{command['command_id']}")
            rescue StandardError => e
              logger.error("❌ ASYNC_PROCESSING: Background batch processing failed for #{command['command_id']}: #{e.message}")
              logger.error("❌ ASYNC_PROCESSING: #{e.backtrace.first(5).join("\n")}")
            end
          end
          
          # Return a simple success indication since RubyCommandBridge handles the actual response
          { 'status' => 'processing_started', 'command_id' => command['command_id'] }
        end
      end

    end

    # Custom error classes
    class CommandListenerError < StandardError; end
    class ListenerError < CommandListenerError; end
    class HandlerError < CommandListenerError; end
  end
end

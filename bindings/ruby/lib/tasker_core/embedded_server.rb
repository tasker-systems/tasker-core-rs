# frozen_string_literal: true

require 'concurrent'
require_relative 'errors'

module TaskerCore
  # Ruby wrapper for embedded TCP executor - enables single-process deployments
  #
  # This class provides a Ruby-friendly interface to the embedded TCP executor,
  # allowing developers to run the Rust TCP server directly within their Ruby process.
  # Perfect for:
  # - Integration testing
  # - Development environments
  # - Docker containers with single service management
  # - Simplified deployment scenarios
  #
  # @example Basic usage
  #   server = TaskerCore::EmbeddedServer.new
  #   server.start
  #   puts "Server running on #{server.bind_address}"
  #   server.stop
  #
  # @example Custom configuration
  #   server = TaskerCore::EmbeddedServer.new(
  #     bind_address: '127.0.0.1:8080',
  #     max_connections: 50,
  #     connection_timeout_ms: 30000
  #   )
  #   server.start
  #
  class EmbeddedServer
    # Default configuration for embedded server
    DEFAULT_CONFIG = {
      bind_address: '127.0.0.1:8080',
      command_queue_size: 1000,
      connection_timeout_ms: 30000,
      graceful_shutdown_timeout_ms: 5000,
      max_connections: 100
    }.freeze

    # @return [Hash] Current server configuration
    attr_reader :config

    # @return [Concurrent::Future, nil] Background future running the server (if any)
    attr_reader :server_future

    # @return [Boolean] Whether the server should run in background thread
    attr_reader :background_mode

    # Initialize embedded server with configuration
    #
    # @param config [Hash] Server configuration options
    # @option config [String] :bind_address ('127.0.0.1:8080') Address to bind server to
    # @option config [Integer] :command_queue_size (1000) Size of command processing queue
    # @option config [Integer] :connection_timeout_ms (30000) Connection timeout in milliseconds
    # @option config [Integer] :graceful_shutdown_timeout_ms (5000) Graceful shutdown timeout
    # @option config [Integer] :max_connections (100) Maximum concurrent connections
    # @option config [Boolean] :background (true) Whether to run server in background thread
    def initialize(config = {})
      @config = DEFAULT_CONFIG.merge(config)
      @background_mode = @config.fetch(:background, true)
      @server_future = nil
      @shutdown_requested = Concurrent::AtomicBoolean.new(false)
      @mutex = Mutex.new
    end

    # Start the embedded TCP executor server
    #
    # @param block_until_ready [Boolean] Whether to block until server is ready
    # @param ready_timeout [Integer] Maximum seconds to wait for server ready
    # @return [Boolean] True if server started successfully
    # @raise [TaskerCore::Errors::ServerError] If server fails to start
    def start(block_until_ready: true, ready_timeout: 10)
      @mutex.synchronize do
        raise TaskerCore::Errors::ServerError, 'Server is already running' if running?

        @shutdown_requested.make_false()

        if @background_mode
          start_background_server(block_until_ready, ready_timeout)
        else
          start_foreground_server
        end
      end
    end

    # Stop the embedded TCP executor server
    #
    # @param force [Boolean] Whether to force immediate shutdown
    # @param timeout [Integer] Maximum seconds to wait for graceful shutdown
    # @return [Boolean] True if server stopped successfully
    def stop(force: false, timeout: 10)
      @mutex.synchronize do
        stop_internal(force: force, timeout: timeout)
      end
    end

    private

    # Internal stop implementation without mutex (to avoid deadlocks)
    #
    # @param force [Boolean] Whether to force immediate shutdown
    # @param timeout [Integer] Maximum seconds to wait for graceful shutdown
    # @return [Boolean] True if server stopped successfully
    def stop_internal(force: false, timeout: 10)
      return false unless running?

      @shutdown_requested.make_true()

      begin
        # Stop the embedded executor
        result = TaskerCore.stop_embedded_executor

        # If running in background future, wait for it to finish or clean it up
        if @server_future
          if !@server_future.complete?
            if force
              @server_future.cancel
            else
              begin
                @server_future.value(timeout)
              rescue Concurrent::TimeoutError
                @server_future.cancel
              end
            end
          end
          @server_future = nil
        end

        result
      rescue => e
        raise TaskerCore::Errors::ServerError, "Failed to stop server: #{e.message}"
      end
    end

    public

    # Check if the embedded server is currently running
    #
    # @return [Boolean] True if server is running
    def running?
      begin
        TaskerCore.embedded_executor_running?
      rescue
        false
      end
    end

    # Get current server status and metrics
    #
    # @return [Hash] Server status information
    def status
      return default_status unless running?

      begin
        TaskerCore.embedded_executor_status
      rescue => e
        default_status.merge(error: e.message)
      end
    end

    # Get the current bind address of the server
    #
    # @return [String] Server bind address
    def bind_address
      return @config[:bind_address] unless running?

      begin
        TaskerCore.embedded_executor_bind_address
      rescue
        @config[:bind_address]
      end
    end

    # Wait for the server to be ready to accept connections
    #
    # @param timeout [Integer] Maximum seconds to wait
    # @return [Boolean] True if server became ready within timeout
    def wait_for_ready(timeout = 10)
      start_time = Time.now

      # Poll until server is running or timeout
      while (Time.now - start_time) < timeout
        return true if running?
        sleep 0.1
      end

      false
    end

    # Restart the server with current configuration
    #
    # @param ready_timeout [Integer] Maximum seconds to wait for server ready
    # @return [Boolean] True if restart was successful
    def restart(ready_timeout: 10)
      stop(timeout: 5) if running?
      sleep 0.1 # Brief pause between stop and start
      start(ready_timeout: ready_timeout)
    end

    # Get server uptime in seconds
    #
    # @return [Integer] Server uptime in seconds
    def uptime
      status[:uptime_seconds] || 0
    end

    # Get total number of commands processed
    #
    # @return [Integer] Total commands processed
    def commands_processed
      status[:commands_processed] || 0
    end

    # Get current number of active connections
    #
    # @return [Integer] Active connections count
    def active_connections
      status[:active_connections] || 0
    end

    # Check if shutdown has been requested
    #
    # @return [Boolean] True if shutdown was requested
    def shutdown_requested?
      @shutdown_requested
    end

    # Create server with custom configuration and start it
    #
    # @param config [Hash] Server configuration
    # @return [EmbeddedServer] Started server instance
    def self.start_server(config = {})
      server = new(config)
      server.start
      server
    end

    # Utility method to run a block with a temporary embedded server
    #
    # @param config [Hash] Server configuration
    # @yield [server] Block to execute with running server
    # @yieldparam server [EmbeddedServer] Running server instance
    # @return [Object] Return value of the block
    def self.with_server(config = {})
      server = start_server(config)
      yield(server)
    ensure
      server&.stop
    end

    private

    # Start server in background future
    def start_background_server(block_until_ready, ready_timeout)
      # Create the embedded executor with configuration
      # Convert symbol keys to string keys for FFI compatibility
      string_key_config = @config.transform_keys(&:to_s)
      create_result = TaskerCore.create_embedded_executor_with_config(string_key_config)
      unless create_result
        raise TaskerCore::Errors::ServerError, 'Failed to create embedded executor'
      end

      # Start the server in a background future
      @server_future = Concurrent::Future.execute do
        begin
          start_result = TaskerCore.start_embedded_executor
          unless start_result
            raise TaskerCore::Errors::ServerError, 'Failed to start embedded executor'
          end

          # Keep future alive while server is running
          while !@shutdown_requested.value && running?
            sleep 0.1
          end
        rescue => e
          warn "Embedded server future error: #{e.message}"
          raise
        end
      end

      # Wait for server to be ready if requested
      if block_until_ready
        ready = wait_for_ready(ready_timeout)
        unless ready
          # Call stop_internal to avoid mutex deadlock
          stop_internal(force: true)
          raise TaskerCore::Errors::ServerError,
                "Server failed to become ready within #{ready_timeout} seconds"
        end
      end

      true
    end

    # Start server in foreground (blocking)
    def start_foreground_server
      # Create and start the embedded executor
      # Convert symbol keys to string keys for FFI compatibility
      string_key_config = @config.transform_keys(&:to_s)
      create_result = TaskerCore.create_embedded_executor_with_config(string_key_config)
      unless create_result
        raise TaskerCore::Errors::ServerError, 'Failed to create embedded executor'
      end

      start_result = TaskerCore.start_embedded_executor
      unless start_result
        raise TaskerCore::Errors::ServerError, 'Failed to start embedded executor'
      end

      true
    end

    # Default status when server is not running
    def default_status
      {
        running: false,
        bind_address: @config[:bind_address],
        total_connections: 0,
        active_connections: 0,
        uptime_seconds: 0,
        commands_processed: 0
      }
    end
  end
end

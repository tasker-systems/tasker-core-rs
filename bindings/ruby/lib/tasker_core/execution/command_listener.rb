# frozen_string_literal: true

require 'socket'
require 'json'
require 'logger'
require 'time'
require_relative '../types/execution_types'

module TaskerCore
  module Execution
    # Ruby TCP listener for receiving commands from Rust Command Executor
    #
    # This listener handles incoming commands from the Rust orchestrator,
    # primarily ExecuteBatch commands that need to be processed by Ruby
    # TaskHandlers and StepHandlers.
    #
    # @example Basic usage
    #   listener = CommandListener.new(port: 8081)
    #   listener.register_handler(:execute_batch) do |command|
    #     # Process ExecuteBatch command
    #     BatchExecutionHandler.new.handle(command)
    #   end
    #
    #   listener.start
    #   # Listener is now accepting commands from Rust
    #
    #   listener.stop
    #
    class CommandListener
      # Default configuration
      DEFAULT_HOST = '0.0.0.0'  # Listen on all interfaces
      DEFAULT_PORT = 8081       # Different from command client port
      DEFAULT_TIMEOUT = 30
      DEFAULT_MAX_CONNECTIONS = 10

      attr_reader :host, :port, :timeout, :logger, :server_socket, :running,
                  :max_connections, :active_connections, :command_handlers

      def initialize(host: nil, port: nil, timeout: nil, max_connections: DEFAULT_MAX_CONNECTIONS)
        @config = TaskerCore::Config.instance.effective_config
        @host = host || @config["command_backplane"]["client"]["host"] || DEFAULT_HOST
        base_port = port || @config["command_backplane"]["client"]["port"] || DEFAULT_PORT
        @timeout = timeout || @config["command_backplane"]["client"]["timeout"] || DEFAULT_TIMEOUT
        @max_connections = max_connections
        @logger = TaskerCore::Logging::Logger.instance
        @server_socket = nil
        @running = false
        @active_connections = []
        @connection_mutex = Mutex.new
        @command_handlers = {}
        @server_thread = nil
        
        # Don't set @port yet - we'll find an available port when starting
        @base_port = base_port
        @port = nil
      end

      # Register a command handler
      #
      # @param command_type [Symbol] The command type to handle (:execute_batch, etc.)
      # @param block [Proc] The handler block that takes a command and returns a response
      def register_handler(command_type, &block)
        raise ArgumentError, "Handler block required" unless block_given?

        @command_handlers[command_type.to_sym] = block
        logger.info("Registered handler for command type: #{command_type}")
      end

      # Start the command listener server
      #
      # @return [Boolean] true if started successfully
      # @raise [ListenerError] if startup fails
      def start
        return true if running?

        begin
          # Find an available port if not already set
          @port = find_available_port(@base_port) unless @port
          
          logger.info("Starting command listener on #{@host}:#{@port}")

          # Create server socket
          @server_socket = TCPServer.new(@host, @port)
          @server_socket.setsockopt(Socket::SOL_SOCKET, Socket::SO_REUSEADDR, 1)

          @running = true

          # Start server thread
          start_server_thread

          logger.info("Command listener started successfully on #{@host}:#{@port}")
          true
        rescue StandardError => e
          logger.error("Failed to start command listener: #{e.message}")
          cleanup
          raise ListenerError, "Failed to start listener: #{e.message}"
        end
      end

      # Stop the command listener server
      #
      # @return [Boolean] true if stopped successfully
      def stop
        return true unless running?

        begin
          logger.info("Stopping command listener")

          @running = false

          # Close all active connections
          close_active_connections

          # Close server socket
          if @server_socket
            @server_socket.close
            @server_socket = nil
          end

          # Wait for server thread to finish
          if @server_thread && @server_thread.alive?
            @server_thread.join(5) # Wait up to 5 seconds
            @server_thread.kill if @server_thread.alive?
          end

          logger.info("Command listener stopped successfully")
          true
        rescue StandardError => e
          logger.error("Error stopping command listener: #{e.message}")
          false
        end
      end

      # Check if listener is running
      #
      # @return [Boolean] true if running
      def running?
        @running && @server_socket && !@server_socket.closed?
      end

      # Get current listener statistics
      #
      # @return [Hash] Listener stats
      def stats
        @connection_mutex.synchronize do
          {
            running: running?,
            host: @host,
            port: @port,
            active_connections: @active_connections.size,
            max_connections: @max_connections,
            registered_handlers: @command_handlers.keys,
            uptime_seconds: running? ? (Time.now - start_time).to_i : 0
          }
        end
      end

      private

      # Find an available port starting from a base port
      #
      # @param base_port [Integer] Starting port to check
      # @return [Integer] Available port number
      def find_available_port(base_port)
        port = base_port

        10.times do # Try up to 10 ports
          begin
            server = TCPServer.new('localhost', port)
            server.close
            return port
          rescue Errno::EADDRINUSE
            port += 1
          end
        end

        # If we can't find a port, use a random high port
        @logger.warn("Could not find available port starting from #{base_port}, using random port")
        base_port + rand(1000)
      end

      # Start the server thread that accepts connections
      def start_server_thread
        @server_thread = Thread.new do
          Thread.current.name = "command-listener-#{@port}"

          while running?
            begin
              # Accept incoming connection with timeout
              ready = IO.select([@server_socket], nil, nil, 1.0)
              next unless ready

              client_socket = @server_socket.accept

              # Check connection limit
              if connection_count >= @max_connections
                logger.warn("Connection limit reached (#{@max_connections}), rejecting connection")
                client_socket.close
                next
              end

              # Handle connection in separate thread
              handle_connection_async(client_socket)

            rescue IOError, SystemCallError => e
              # Socket closed or system error - break if we're shutting down
              break unless running?
              logger.error("Server socket error: #{e.message}")

            rescue StandardError => e
              logger.error("Error accepting connection: #{e.message}")
              # Continue accepting other connections
            end
          end
        end
      end

      # Handle incoming connection asynchronously
      #
      # @param client_socket [TCPSocket] The client connection
      def handle_connection_async(client_socket)
        Thread.new do
          Thread.current.name = "client-#{client_socket.object_id}"

          begin
            add_active_connection(client_socket)
            handle_client_connection(client_socket)
          ensure
            remove_active_connection(client_socket)
            client_socket.close rescue nil
          end
        end
      end

      # Handle a client connection
      #
      # @param client_socket [TCPSocket] The client connection
      def handle_client_connection(client_socket)
        client_socket.setsockopt(Socket::IPPROTO_TCP, Socket::TCP_NODELAY, 1)

        logger.debug("Accepted connection from #{client_socket.peeraddr[3]}")

        while running? && !client_socket.closed?
          begin
            # Read command with timeout
            ready = IO.select([client_socket], nil, nil, @timeout)
            unless ready
              logger.debug("Client connection timed out")
              break
            end

            line = client_socket.gets
            break unless line

            # Parse and handle command
            command_data = JSON.parse(line.strip, symbolize_names: true)
            response = handle_command(command_data)

            # Send response
            response_json = JSON.generate(response)
            client_socket.puts(response_json)

          rescue JSON::ParserError => e
            logger.error("Invalid JSON received: #{e.message}")
            error_response = create_error_response(nil, "Invalid JSON: #{e.message}")
            client_socket.puts(JSON.generate(error_response))

          rescue EOFError
            logger.debug("Client disconnected")
            break

          rescue StandardError => e
            logger.error("Error handling client connection: #{e.message}")
            error_response = create_error_response(nil, "Server error: #{e.message}")
            begin
              client_socket.puts(JSON.generate(error_response))
            rescue
              # Client may have disconnected
            end
            break
          end
        end
      end

      # Handle a parsed command
      #
      # @param command_data [Hash] The parsed command data
      # @return [Hash] Response data
      def handle_command(command_data)
        command_type = command_data[:command_type]
        command_id = command_data[:command_id]

        logger.debug("Handling command: #{command_type} (ID: #{command_id})")

        # Convert to typed command object
        command = TaskerCore::Types::ResponseFactory.create_response(command_data)

        # Find appropriate handler
        handler_key = command_type.to_s.downcase.gsub(/([a-z])([A-Z])/, '\1_\2').to_sym
        handler = @command_handlers[handler_key]

        unless handler
          logger.warn("No handler registered for command type: #{command_type}")
          return create_error_response(command_id, "No handler for command type: #{command_type}")
        end

        # Execute handler
        begin
          result = handler.call(command)

          # Ensure result is a hash with proper structure
          if result.is_a?(Hash)
            result[:correlation_id] = command_id if command_id
            result
          else
            create_success_response(command_id, result)
          end

        rescue StandardError => e
          logger.error("Command handler failed: #{e.message}")
          logger.error(e.backtrace.join("\n")) if logger.debug?
          create_error_response(command_id, "Handler error: #{e.message}")
        end
      end

      # Create success response
      #
      # @param command_id [String] Original command ID
      # @param data [Object] Response data
      # @return [Hash] Success response
      def create_success_response(command_id, data)
        {
          command_type: 'Success',
          command_id: generate_response_id,
          correlation_id: command_id,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: { id: 'command_listener' }
            }
          },
          payload: {
            type: 'Success',
            data: data
          }
        }
      end

      # Create error response
      #
      # @param command_id [String] Original command ID
      # @param message [String] Error message
      # @return [Hash] Error response
      def create_error_response(command_id, message)
        {
          command_type: 'Error',
          command_id: generate_response_id,
          correlation_id: command_id,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: { id: 'command_listener' }
            }
          },
          payload: {
            type: 'Error',
            data: {
              error_type: 'HandlerError',
              message: message,
              retryable: false
            }
          }
        }
      end

      # Generate unique response ID
      #
      # @return [String] Unique response identifier
      def generate_response_id
        "ruby_response_#{Process.pid}_#{Time.now.to_f}_#{rand(1000)}"
      end

      # Add connection to active list
      #
      # @param socket [TCPSocket] Client socket
      def add_active_connection(socket)
        @connection_mutex.synchronize do
          @active_connections << socket
        end
      end

      # Remove connection from active list
      #
      # @param socket [TCPSocket] Client socket
      def remove_active_connection(socket)
        @connection_mutex.synchronize do
          @active_connections.delete(socket)
        end
      end

      # Get current connection count
      #
      # @return [Integer] Number of active connections
      def connection_count
        @connection_mutex.synchronize do
          @active_connections.size
        end
      end

      # Close all active connections
      def close_active_connections
        @connection_mutex.synchronize do
          @active_connections.each do |socket|
            begin
              socket.close unless socket.closed?
            rescue StandardError => e
              logger.warn("Error closing connection: #{e.message}")
            end
          end
          @active_connections.clear
        end
      end

      # Get listener start time
      #
      # @return [Time] Start time
      def start_time
        @start_time ||= Time.now
      end

      # Cleanup resources
      def cleanup
        close_active_connections
        @server_socket&.close rescue nil
        @server_socket = nil
        @running = false
      end

      # Create default logger
      #
      # @return [Logger] Default logger instance
      def default_logger
        logger = Logger.new(STDOUT)
        logger.level = Logger::INFO
        logger.formatter = proc do |severity, datetime, progname, msg|
          "[#{datetime}] CommandListener #{severity}: #{msg}\n"
        end
        logger
      end
    end

    # Custom error classes
    class CommandListenerError < StandardError; end
    class ListenerError < CommandListenerError; end
    class HandlerError < CommandListenerError; end
  end
end

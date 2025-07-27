# frozen_string_literal: true

require 'socket'
require 'json'
require 'timeout'
require 'logger'
require 'time'

module TaskerCore
  module Execution
    # Ruby TCP client for communicating with Rust Command Executor
    #
    # Provides high-level Ruby interface for sending commands to the Rust
    # TokioTcpExecutor, handling connection management, serialization,
    # and response correlation.
    #
    # @example Basic usage
    #   client = CommandClient.new(host: 'localhost', port: 8080)
    #   client.connect
    #   
    #   response = client.register_worker(
    #     worker_id: 'ruby_worker_1',
    #     max_concurrent_steps: 10,
    #     supported_namespaces: ['orders', 'payments'],
    #     step_timeout_ms: 30000,
    #     supports_retries: true,
    #     language_runtime: 'ruby',
    #     version: RUBY_VERSION
    #   )
    #   
    #   client.disconnect
    #
    class CommandClient
      # Default configuration
      DEFAULT_HOST = 'localhost'
      DEFAULT_PORT = 8080
      DEFAULT_TIMEOUT = 30
      DEFAULT_CONNECT_TIMEOUT = 5
      DEFAULT_READ_TIMEOUT = 10

      attr_reader :host, :port, :timeout, :logger, :socket, :connected

      def initialize(host: DEFAULT_HOST, port: DEFAULT_PORT, timeout: DEFAULT_TIMEOUT, logger: nil)
        @host = host
        @port = port
        @timeout = timeout
        @logger = logger || default_logger
        @socket = nil
        @connected = false
        @command_counter = 0
        @pending_commands = {}
        @response_mutex = Mutex.new
      end

      # Establish connection to Rust TCP executor
      #
      # @return [Boolean] true if connection successful
      # @raise [ConnectionError] if connection fails
      def connect
        return true if connected?

        begin
          @socket = TCPSocket.new(@host, @port)
          @socket.setsockopt(Socket::IPPROTO_TCP, Socket::TCP_NODELAY, 1)
          @connected = true
          
          logger.info("Connected to Rust TCP executor at #{@host}:#{@port}")
          true
        rescue StandardError => e
          logger.error("Failed to connect to #{@host}:#{@port}: #{e.message}")
          @connected = false
          raise ConnectionError, "Failed to connect: #{e.message}"
        end
      end

      # Close connection to TCP executor
      def disconnect
        return unless @socket

        begin
          @socket.close
        rescue StandardError => e
          logger.warn("Error closing socket: #{e.message}")
        ensure
          @socket = nil
          @connected = false
          logger.info("Disconnected from Rust TCP executor")
        end
      end

      # Check if client is connected
      #
      # @return [Boolean] true if connected
      def connected?
        @connected && @socket && !@socket.closed?
      end

      # Register a Ruby worker with the Rust orchestration system
      #
      # @param worker_id [String] Unique identifier for this worker
      # @param max_concurrent_steps [Integer] Maximum number of concurrent steps this worker can handle
      # @param supported_namespaces [Array<String>] List of namespaces this worker supports
      # @param step_timeout_ms [Integer] Timeout for individual steps in milliseconds
      # @param supports_retries [Boolean] Whether this worker supports step retries
      # @param language_runtime [String] Runtime identifier (usually 'ruby')
      # @param version [String] Runtime version
      # @param custom_capabilities [Hash] Additional worker capabilities
      # @return [Hash] Response from Rust orchestrator
      # @raise [NotConnectedError] if not connected
      # @raise [CommandError] if command fails
      def register_worker(worker_id:, max_concurrent_steps:, supported_namespaces:, 
                         step_timeout_ms:, supports_retries:, language_runtime:, 
                         version:, custom_capabilities: {})
        ensure_connected!

        command = {
          command_type: 'RegisterWorker',
          command_id: generate_command_id,
          correlation_id: nil,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: {
                id: worker_id
              }
            },
            target: nil,
            timeout_ms: step_timeout_ms,
            retry_policy: nil,
            namespace: nil,
            priority: nil
          },
          payload: {
            type: 'RegisterWorker',
            data: {
              worker_capabilities: {
                worker_id: worker_id,
                max_concurrent_steps: max_concurrent_steps,
                supported_namespaces: supported_namespaces,
                step_timeout_ms: step_timeout_ms,
                supports_retries: supports_retries,
                language_runtime: language_runtime,
                version: version,
                custom_capabilities: custom_capabilities
              }
            }
          }
        }

        send_command(command)
      end

      # Send heartbeat to maintain worker connection
      #
      # @param worker_id [String] Worker identifier
      # @param current_load [Integer] Current number of steps being processed
      # @param system_stats [Hash, nil] Optional system statistics
      # @return [Hash] Response from Rust orchestrator
      def send_heartbeat(worker_id:, current_load:, system_stats: nil)
        ensure_connected!

        command = {
          command_type: 'WorkerHeartbeat',
          command_id: generate_command_id,
          correlation_id: nil,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: {
                id: worker_id
              }
            },
            target: nil,
            timeout_ms: (@timeout * 1000).to_i,
            retry_policy: nil,
            namespace: nil,
            priority: nil
          },
          payload: {
            type: 'WorkerHeartbeat',
            data: {
              worker_id: worker_id,
              current_load: current_load,
              system_stats: system_stats
            }
          }
        }

        send_command(command)
      end

      # Unregister worker from orchestration system
      #
      # @param worker_id [String] Worker identifier
      # @param reason [String] Reason for unregistration
      # @return [Hash] Response from Rust orchestrator
      def unregister_worker(worker_id:, reason: 'Client shutdown')
        ensure_connected!

        command = {
          command_type: 'UnregisterWorker',
          command_id: generate_command_id,
          correlation_id: nil,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: {
                id: worker_id
              }
            },
            target: nil,
            timeout_ms: (@timeout * 1000).to_i,
            retry_policy: nil,
            namespace: nil,
            priority: nil
          },
          payload: {
            type: 'UnregisterWorker',
            data: {
              worker_id: worker_id,
              reason: reason
            }
          }
        }

        send_command(command)
      end

      # Send health check command
      #
      # @param diagnostic_level [String] Level of diagnostic information ('Basic', 'Detailed', 'Full')
      # @return [Hash] Health check response
      def health_check(diagnostic_level: 'Basic')
        ensure_connected!

        command = {
          command_type: 'HealthCheck',
          command_id: generate_command_id,
          correlation_id: nil,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: {
                id: 'health_check_client'
              }
            },
            target: nil,
            timeout_ms: (@timeout * 1000).to_i,
            retry_policy: nil,
            namespace: nil,
            priority: nil
          },
          payload: {
            type: 'HealthCheck',
            data: {
              diagnostic_level: diagnostic_level
            }
          }
        }

        send_command(command)
      end

      private

      # Send command to Rust executor and wait for response
      #
      # @param command [Hash] Command to send
      # @return [Hash] Response from executor
      # @raise [CommandError] if command fails
      def send_command(command)
        json_command = JSON.generate(command)
        command_id = command[:command_id]

        logger.debug("Sending command: #{command[:command_type]} (ID: #{command_id})")

        begin
          # Send command
          @socket.puts(json_command)
          
          # Wait for response with timeout
          raw_response = nil
          Timeout.timeout(@timeout) do
            response_line = @socket.gets
            raise CommandError, "No response received" unless response_line
            
            raw_response = JSON.parse(response_line.strip, symbolize_names: true)
          end

          logger.debug("Received response for command #{command_id}: #{raw_response[:command_type]}")
          
          # Validate response correlation
          if raw_response[:correlation_id] && raw_response[:correlation_id] != command_id
            logger.warn("Response correlation mismatch: expected #{command_id}, got #{raw_response[:correlation_id]}")
          end

          # Convert raw response to typed response using ResponseFactory
          TaskerCore::Types::ResponseFactory.create_response(raw_response)
        rescue Timeout::Error
          logger.error("Command #{command_id} timed out after #{@timeout} seconds")
          raise CommandError, "Command timed out"
        rescue JSON::ParserError => e
          logger.error("Failed to parse response for command #{command_id}: #{e.message}")
          raise CommandError, "Invalid response format: #{e.message}"
        rescue StandardError => e
          logger.error("Command #{command_id} failed: #{e.message}")
          raise CommandError, "Command failed: #{e.message}"
        end
      end

      # Generate unique command ID
      #
      # @return [String] Unique command identifier
      def generate_command_id
        @command_counter += 1
        "ruby_cmd_#{Process.pid}_#{@command_counter}_#{Time.now.to_f}"
      end

      # Ensure client is connected
      #
      # @raise [NotConnectedError] if not connected
      def ensure_connected!
        raise NotConnectedError, "Client not connected" unless connected?
      end

      # Create default logger
      #
      # @return [Logger] Default logger instance
      def default_logger
        logger = Logger.new(STDOUT)
        logger.level = Logger::INFO
        logger.formatter = proc do |severity, datetime, progname, msg|
          "[#{datetime}] #{severity}: #{msg}\n"
        end
        logger
      end
    end

    # Custom error classes
    class CommandClientError < StandardError; end
    class ConnectionError < CommandClientError; end
    class NotConnectedError < CommandClientError; end
    class CommandError < CommandClientError; end
  end
end
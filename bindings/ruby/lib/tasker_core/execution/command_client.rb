# frozen_string_literal: true

require 'logger'
require_relative '../types/execution_types'

module TaskerCore
  module Execution
    # Ruby wrapper for Rust-backed CommandClient
    #
    # This class provides a Ruby-friendly interface to the Rust-backed command client,
    # eliminating Ruby socket dependencies while maintaining API compatibility.
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
      DEFAULT_NAMESPACE = 'default'

      attr_reader :host, :port, :timeout, :logger, :rust_client, :connected

      def initialize(host: nil, port: nil, timeout: nil)
        @config = TaskerCore::Config.instance.effective_config
        @host = host || @config.dig("command_backplane", "server", "host") || DEFAULT_HOST
        @port = port || @config.dig("command_backplane", "server", "port") || DEFAULT_PORT
        @timeout = timeout || @config.dig("command_backplane", "server", "timeout") || DEFAULT_TIMEOUT
        @logger = TaskerCore::Logging::Logger.instance
        @connected = false

        # Create Rust-backed command client
        client_config = {
          host: @host,
          port: @port,
          timeout_seconds: @timeout,
          connect_timeout_seconds: DEFAULT_CONNECT_TIMEOUT,
          namespace: DEFAULT_NAMESPACE
        }

        @rust_client = TaskerCore::CommandClient.new_with_config(client_config)
      end

      # Establish connection to Rust TCP executor
      #
      # @return [Boolean] true if connection successful
      # @raise [ConnectionError] if connection fails
      def connect
        return true if connected?

        begin
          success = @rust_client.connect
          if success
            @connected = true
            logger.info("Connected to Rust TCP executor at #{@host}:#{@port} via Rust client")
            true
          else
            raise ConnectionError, "Rust client connection failed"
          end
        rescue StandardError => e
          logger.error("Failed to connect to #{@host}:#{@port}: #{e.message}")
          @connected = false
          raise ConnectionError, "Failed to connect: #{e.message}"
        end
      end

      # Close connection to TCP executor
      def disconnect
        return unless @rust_client

        begin
          @rust_client.disconnect
          @connected = false
          logger.info("Disconnected from Rust TCP executor")
        rescue StandardError => e
          logger.warn("Error disconnecting Rust client: #{e.message}")
        end
      end

      # Check if client is connected
      #
      # @return [Boolean] true if connected
      def connected?
        @connected && @rust_client&.connected?
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
      # @param supported_tasks [Array<Hash>, nil] Task handler configurations with step templates
      # @return [TaskerCore::Types::ExecutionTypes::WorkerRegistrationResponse] Typed response from Rust orchestrator
      # @raise [NotConnectedError] if not connected
      # @raise [CommandError] if command fails
      def register_worker(worker_id:, max_concurrent_steps:, supported_namespaces: [DEFAULT_NAMESPACE],
                         step_timeout_ms:, supports_retries:, language_runtime:,
                         version:, custom_capabilities: {}, supported_tasks: nil)
        ensure_connected!

        options = {
          worker_id: worker_id,
          max_concurrent_steps: max_concurrent_steps,
          supported_namespaces: supported_namespaces,
          step_timeout_ms: step_timeout_ms,
          supports_retries: supports_retries,
          language_runtime: language_runtime,
          version: version,
          custom_capabilities: custom_capabilities
        }
        
        # Add supported_tasks if provided
        if supported_tasks && !supported_tasks.empty?
          options[:supported_tasks] = supported_tasks
          logger.debug("Including #{supported_tasks.size} supported tasks in worker registration")
        end

        begin
          raw_response = @rust_client.register_worker(options)
          logger.debug("Worker registration successful: #{worker_id}")
          
          # Convert raw response to typed WorkerRegistrationResponse
          TaskerCore::Types::ExecutionTypes::ResponseFactory.create_response(raw_response)
        rescue StandardError => e
          logger.error("Worker registration failed: #{e.message}")
          raise CommandError, "Worker registration failed: #{e.message}"
        end
      end

      # Send heartbeat to maintain worker connection
      #
      # @param worker_id [String] Worker identifier
      # @param current_load [Integer] Current number of steps being processed
      # @param system_stats [Hash, nil] Optional system statistics
      # @return [TaskerCore::Types::ExecutionTypes::HeartbeatResponse] Typed response from Rust orchestrator
      def send_heartbeat(worker_id:, current_load:, system_stats: nil)
        ensure_connected!

        options = {
          worker_id: worker_id,
          current_load: current_load,
          system_stats: system_stats
        }

        begin
          raw_response = @rust_client.send_heartbeat(options)
          logger.debug("Heartbeat successful for worker: #{worker_id}")
          
          # Convert raw response to typed HeartbeatResponse
          TaskerCore::Types::ExecutionTypes::ResponseFactory.create_response(raw_response)
        rescue StandardError => e
          logger.error("Heartbeat failed: #{e.message}")
          raise CommandError, "Heartbeat failed: #{e.message}"
        end
      end

      # Unregister worker from orchestration system
      #
      # @param worker_id [String] Worker identifier
      # @param reason [String] Reason for unregistration
      # @return [TaskerCore::Types::ExecutionTypes::WorkerUnregistrationResponse] Typed response from Rust orchestrator
      def unregister_worker(worker_id:, reason: 'Client shutdown')
        ensure_connected!

        options = {
          worker_id: worker_id,
          reason: reason
        }

        begin
          raw_response = @rust_client.unregister_worker(options)
          logger.debug("Worker unregistration successful: #{worker_id}")
          
          # Convert raw response to typed WorkerUnregistrationResponse
          TaskerCore::Types::ExecutionTypes::ResponseFactory.create_response(raw_response)
        rescue StandardError => e
          logger.error("Worker unregistration failed: #{e.message}")
          raise CommandError, "Worker unregistration failed: #{e.message}"
        end
      end

      # Send health check command
      #
      # @param diagnostic_level [String] Level of diagnostic information ('Basic', 'Detailed', 'Full')
      # @return [TaskerCore::Types::ExecutionTypes::HealthCheckResponse] Typed health check response
      def health_check(diagnostic_level: 'Basic')
        ensure_connected!

        begin
          raw_response = @rust_client.health_check(diagnostic_level)
          logger.debug("Health check successful")
          
          # Convert raw response to typed HealthCheckResponse
          TaskerCore::Types::ExecutionTypes::ResponseFactory.create_response(raw_response)
        rescue StandardError => e
          logger.error("Health check failed: #{e.message}")
          raise CommandError, "Health check failed: #{e.message}"
        end
      end

      # Send TryTaskIfReady command to check if task has ready steps and request batch creation
      #
      # @param task_id [Integer] Task ID to check for readiness
      # @return [TaskerCore::Types::ExecutionTypes::TaskReadinessResponse] Typed task readiness response
      def try_task_if_ready(task_id)
        ensure_connected!
        raise ArgumentError, "task_id must be an integer" unless task_id.is_a?(Integer)

        begin
          raw_response = @rust_client.try_task_if_ready(task_id)
          logger.debug("TryTaskIfReady command sent for task #{task_id}")
          
          # Convert raw response to typed TaskReadinessResponse
          TaskerCore::Types::ExecutionTypes::ResponseFactory.create_response(raw_response)
        rescue StandardError => e
          logger.error("TryTaskIfReady command failed for task #{task_id}: #{e.message}")
          raise CommandError, "TryTaskIfReady failed: #{e.message}"
        end
      end

      # Send InitializeTask command to create a new task
      #
      # @param task_request [Hash] Task request data
      # @return [TaskerCore::Types::ExecutionTypes::InitializeTaskResponse] Typed initialize task response
      def initialize_task(task_request)
        ensure_connected!
        raise ArgumentError, "task_request is required" unless task_request

        begin
          raw_response = @rust_client.initialize_task(task_request)
          logger.debug("InitializeTask command sent")
          
          # Convert raw response to typed response
          TaskerCore::Types::ExecutionTypes::ResponseFactory.create_response(raw_response)
        rescue StandardError => e
          logger.error("InitializeTask command failed: #{e.message}")
          raise CommandError, "InitializeTask failed: #{e.message}"
        end
      end

      # Get connection information
      #
      # @return [Hash] Connection information
      def connection_info
        return { connected: false } unless @rust_client

        begin
          info = @rust_client.connection_info
          # Return as-is since this is connection metadata, not a command response
          info.is_a?(Hash) ? info.transform_keys(&:to_sym) : info
        rescue StandardError => e
          logger.warn("Failed to get connection info: #{e.message}")
          { connected: false, error: e.message }
        end
      end

      private

      # Ensure client is connected
      #
      # @raise [NotConnectedError] if not connected
      def ensure_connected!
        return if connected?
        
        # Auto-reconnect for singleton pattern
        @logger.info "🔄 SINGLETON: Auto-reconnecting CommandClient for command execution"
        begin
          connect
          @logger.info "✅ SINGLETON: Auto-reconnection successful"
        rescue StandardError => e
          error_msg = "SINGLETON: Auto-reconnection failed: #{e.message}"
          @logger.error error_msg
          raise NotConnectedError, error_msg
        end
      end
    end

    # Custom error classes
    class CommandClientError < StandardError; end
    class ConnectionError < CommandClientError; end
    class NotConnectedError < CommandClientError; end
    class CommandError < CommandClientError; end
  end
end
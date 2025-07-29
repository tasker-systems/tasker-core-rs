# frozen_string_literal: true

require_relative 'execution/command_client'
require_relative 'execution/command_listener'
require_relative 'execution/batch_execution_handler'
require_relative 'execution/worker_manager'

module TaskerCore
  # Execution module for Ruby-Rust Command Integration
  #
  # Provides Ruby interfaces for communicating with the Rust Command Executor,
  # replacing ZeroMQ with native TCP communication and command-based protocols.
  #
  # This module includes:
  # - CommandClient: Low-level TCP client for sending commands to Rust
  # - CommandListener: TCP server for receiving commands from Rust
  # - BatchExecutionHandler: Processes ExecuteBatch commands from Rust orchestrator
  # - WorkerManager: High-level worker lifecycle management with ExecuteBatch support
  #
  # @example Quick worker setup
  #   TaskerCore::Execution.start_worker(
  #     worker_id: 'my_worker',
  #     supported_namespaces: ['orders', 'payments']
  #   )
  #
  # @example Custom executor connection
  #   TaskerCore::Execution.configure do |config|
  #     config.executor_host = 'rust-executor.local'
  #     config.executor_port = 9090
  #     config.default_heartbeat_interval = 15
  #   end
  #
  module Execution
    class << self
      attr_accessor :default_executor_host, :default_executor_port,
                    :default_heartbeat_interval, :default_logger

      # Configure default execution settings
      #
      # @yield [config] Configuration block
      def configure
        yield(self) if block_given?
      end

      # Quick start a worker with default configuration
      #
      # @param worker_id [String] Unique worker identifier
      # @param supported_namespaces [Array<String>] Namespaces this worker supports (nil = auto-discover)
      # @param options [Hash] Additional worker options
      # @return [WorkerManager] Configured and started worker manager
      def start_worker(worker_id:, supported_namespaces: nil, **options)
        manager = create_worker_manager(
          worker_id: worker_id,
          supported_namespaces: supported_namespaces,
          **options
        )
        
        manager.start
        manager
      end

      # Create a worker manager with default configuration
      #
      # @param worker_id [String] Unique worker identifier
      # @param supported_namespaces [Array<String>] Namespaces this worker supports (nil = auto-discover)
      # @param options [Hash] Additional worker options
      # @return [WorkerManager] Configured worker manager (not started)
      def create_worker_manager(worker_id:, supported_namespaces: nil, **options)
        defaults = {
          executor_host: default_executor_host || CommandClient::DEFAULT_HOST,
          executor_port: default_executor_port || CommandClient::DEFAULT_PORT,
          heartbeat_interval: default_heartbeat_interval || WorkerManager::DEFAULT_HEARTBEAT_INTERVAL,
          logger: default_logger
        }

        WorkerManager.new(
          worker_id: worker_id,
          supported_namespaces: supported_namespaces,
          **defaults.merge(options)
        )
      end

      # Create a command client with default configuration
      #
      # @param options [Hash] Client options
      # @return [CommandClient] Configured command client
      def create_command_client(**options)
        defaults = {
          host: default_executor_host || CommandClient::DEFAULT_HOST,
          port: default_executor_port || CommandClient::DEFAULT_PORT,
          logger: default_logger
        }

        CommandClient.new(**defaults.merge(options))
      end

      # Check health of Rust executor
      #
      # @param host [String] Executor host
      # @param port [Integer] Executor port
      # @param timeout [Integer] Connection timeout
      # @return [Hash] Health check result
      def check_executor_health(host: nil, port: nil, timeout: 5)
        client = create_command_client(
          host: host || default_executor_host || CommandClient::DEFAULT_HOST,
          port: port || default_executor_port || CommandClient::DEFAULT_PORT,
          timeout: timeout
        )

        begin
          client.connect
          response = client.health_check
          client.disconnect
          
          {
            healthy: true,
            response: response,
            message: 'Executor is healthy'
          }
        rescue StandardError => e
          {
            healthy: false,
            response: nil,
            message: "Health check failed: #{e.message}"
          }
        ensure
          client&.disconnect
        end
      end

      # Get version information
      #
      # @return [Hash] Version information
      def version_info
        {
          ruby_version: RUBY_VERSION,
          tasker_core_version: TaskerCore::VERSION,
          command_protocol_version: '1.0.0'
        }
      end
    end

    # Set sensible defaults
    self.default_executor_host = 'localhost'
    self.default_executor_port = 8080
    self.default_heartbeat_interval = 30
    self.default_logger = nil # Will use component default loggers
  end
end
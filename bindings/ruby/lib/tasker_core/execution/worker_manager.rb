# frozen_string_literal: true

require 'logger'
require_relative 'command_client'
require_relative 'command_listener'
require_relative 'command_backplane'
require_relative 'batch_execution_handler'
require_relative '../types/execution_types'

module TaskerCore
  module Execution
    # Ruby wrapper for Rust-backed WorkerManager
    #
    # This class provides a Ruby-friendly interface to the Rust-backed worker manager,
    # providing unified worker lifecycle management while eliminating Ruby dependencies.
    #
    # @example Simple worker registration
    #   manager = WorkerManager.new(
    #     worker_id: 'payment_processor_1',
    #     supported_namespaces: ['payments', 'billing']
    #   )
    #
    #   manager.start
    #   # Worker is now registered and sending heartbeats
    #
    #   manager.stop
    #
    # @example Custom configuration
    #   manager = WorkerManager.new(
    #     worker_id: 'inventory_worker',
    #     max_concurrent_steps: 20,
    #     supported_namespaces: ['inventory', 'shipping'],
    #     server_host: 'rust-orchestrator.local',
    #     server_port: 9090,
    #     heartbeat_interval: 15
    #   )
    #
    class WorkerManager
      # Default configuration
      DEFAULT_MAX_CONCURRENT_STEPS = 10
      DEFAULT_STEP_TIMEOUT_MS = 30000
      DEFAULT_HEARTBEAT_INTERVAL = 30 # seconds
      DEFAULT_SUPPORTS_RETRIES = true
      DEFAULT_NAMESPACE = 'default'

      attr_reader :worker_id, :max_concurrent_steps, :supported_namespaces,
                  :step_timeout_ms, :supports_retries, :heartbeat_interval,
                  :running, :current_load, :custom_capabilities,
                  :supported_tasks, :server_host, :server_port

      def initialize(worker_id:, supported_namespaces: nil,
                     max_concurrent_steps: DEFAULT_MAX_CONCURRENT_STEPS,
                     step_timeout_ms: DEFAULT_STEP_TIMEOUT_MS,
                     supports_retries: DEFAULT_SUPPORTS_RETRIES,
                     heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
                     custom_capabilities: {},
                     supported_tasks: nil)
        @worker_id = worker_id
        # Auto-discover namespaces if not explicitly provided
        @supported_namespaces = supported_namespaces || discover_registered_namespaces
        @max_concurrent_steps = max_concurrent_steps
        @step_timeout_ms = step_timeout_ms
        @supports_retries = supports_retries
        @heartbeat_interval = heartbeat_interval
        @custom_capabilities = custom_capabilities
        @supported_tasks = supported_tasks
        @current_load = 0
        @running = false
        @load_mutex = Mutex.new
        @logger = TaskerCore::Logging::Logger.instance

        # 🎯 SINGLETON: Use OrchestrationManager's singleton CommandClient
        # This ensures all operations use the same process-wide CommandClient instance
        @orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance

        @logger.info "🔄 WORKER_MANAGER: Using singleton CommandClient from OrchestrationManager"
      end

      # Start worker - initialize as worker and register with Rust orchestrator
      #
      # @return [Boolean] true if started successfully
      # @raise [WorkerError] if startup fails
      def start
        return true if running?

        begin
          @logger.info("Starting worker #{@worker_id}")

          command_listener.start
          command_client.connect
          register_worker

          @running = true
          @logger.info("Worker #{@worker_id} started successfully with shared CommandClient")
          true
        rescue StandardError => e
          @logger.error("Failed to start worker #{@worker_id}: #{e.message}")
          raise WorkerError, "Failed to start worker: #{e.message}"
        end
      end

      # Stop worker - unregister and cleanup
      #
      # @param reason [String] Reason for stopping
      # @return [Boolean] true if stopped successfully
      def stop(reason: 'Graceful shutdown')
        return true unless running?

        begin
          @logger.info("Stopping worker #{@worker_id}: #{reason}")

          unregister_worker
          command_listener.stop
          command_client.disconnect

          @running = false
          @logger.info("Worker #{@worker_id} stopped successfully")
          true
        rescue StandardError => e
          @logger.error("Error stopping worker #{@worker_id}: #{e.message}")
          false
        end
      end

      def command_client
        TaskerCore::Execution::CommandBackplane.instance.command_client
      end

      def command_listener
        TaskerCore::Execution::CommandBackplane.instance.command_listener
      end

      # Check if worker is running
      #
      # @return [Boolean] true if running
      def running?
        @running
      end

      # Get current worker statistics
      #
      # @return [Hash] Current worker stats
      def stats
        begin
          connection_info = command_client.connection_info

          # Ruby worker statistics
          {
            worker_id: @worker_id,
            running: running?,
            current_load: @current_load,
            max_concurrent_steps: @max_concurrent_steps,
            available_capacity: [@max_concurrent_steps - @current_load, 0].max,
            load_percentage: (@current_load.to_f / @max_concurrent_steps * 100).round(1),
            supported_namespaces: @supported_namespaces,
            supports_retries: @supports_retries,
            manager_type: 'singleton_command_client',
            connection_info: connection_info,
            command_client_connected: command_client.connected?
          }
        rescue StandardError => e
          @logger.warn("Failed to get worker statistics: #{e.message}")
          # Return basic Ruby statistics as fallback
          {
            worker_id: @worker_id,
            running: running?,
            current_load: @current_load,
            max_concurrent_steps: @max_concurrent_steps,
            error: "Failed to get stats: #{e.message}"
          }
        end
      end

      # Send immediate heartbeat (useful for testing or manual updates)
      #
      # @return [TaskerCore::Types::ExecutionTypes::HeartbeatResponse] Typed heartbeat response
      def send_heartbeat
        ensure_running!

        current_load = @load_mutex.synchronize { @current_load }

        begin
          # Send heartbeat using shared CommandClient
          heartbeat_options = {
            worker_id: @worker_id,
            current_load: current_load,
            system_stats: {}  # TODO: Add real system stats
          }

          command_client.send_heartbeat(
            worker_id: heartbeat_options[:worker_id],
            current_load: heartbeat_options[:current_load],
            system_stats: heartbeat_options[:system_stats]
          )
        rescue StandardError => e
          @logger.error("Manual heartbeat failed: #{e.message}")
          raise WorkerError, "Heartbeat failed: #{e.message}"
        end
      end

      def stop_heartbeat
        # TODO: Implement heartbeat stopping when automatic heartbeat is implemented
        @logger.info("stop_heartbeat called for worker #{@worker_id} - heartbeat management to be implemented")
      end

      # Check connection health
      #
      # @return [Boolean] true if healthy
      def healthy?
        running? && command_client_healthy?
      end

      # Get detailed health information
      #
      # @return [Hash] Health details
      def health_details
        begin
          connection_info = command_client.connection_info

          {
            healthy: healthy?,
            running: running?,
            command_client_connected: command_client.connected?,
            connection_info: connection_info,
            current_load: @current_load,
            max_capacity: @max_concurrent_steps
          }
        rescue StandardError => e
          {
            healthy: false,
            error: e.message,
            running: @running,
            current_load: @current_load
          }
        end
      end

      private

      # Auto-discover namespaces from registered TaskHandlers
      #
      # @return [Array<String>] List of namespaces from registered handlers
      def discover_registered_namespaces
        # TODO: Implement namespace discovery from crawling config directory
        [DEFAULT_NAMESPACE]
      end

      # Register worker using shared CommandClient
      #
      # @param enhanced_capabilities [Hash] Worker capabilities
      # @return [TaskerCore::Types::ExecutionTypes::WorkerRegistrationResponse] Typed registration response
      def register_worker_with_shared_client(enhanced_capabilities = {})
        @custom_capabilities ||= {
          'ruby_worker' => true,
          'listener_host' => TaskerCore::Execution::CommandBackplane.instance.command_listener_host,
          'listener_port' => TaskerCore::Execution::CommandBackplane.instance.command_listener_port,
        }.merge(enhanced_capabilities)

        # if these get overridden, we need to update the custom_capabilities
        # to ensure the correct values are used
        @custom_capabilities['listener_host'] = TaskerCore::Execution::CommandBackplane.instance.command_listener_host
        @custom_capabilities['listener_port'] = TaskerCore::Execution::CommandBackplane.instance.command_listener_port

        options = {
          worker_id: @worker_id,
          max_concurrent_steps: @max_concurrent_steps,
          supported_namespaces: @supported_namespaces,
          language_runtime: 'ruby',
          version: RUBY_VERSION,
          custom_capabilities: @custom_capabilities
        }

        # Add connection info for proper worker registration
        # This provides the Rust side with details about how to communicate with this worker
        # CRITICAL: Use CommandListener's address, not CommandClient's!
        options[:connection_info] = {
          host: TaskerCore::Execution::CommandBackplane.instance.command_listener_host,
          port: TaskerCore::Execution::CommandBackplane.instance.command_listener_port,
          listener_port: TaskerCore::Execution::CommandBackplane.instance.command_listener_port,
          transport_type: 'tcp',
          protocol_version: '1.0'
        }

        # Add supported tasks if provided (for database-backed task handler registration)
        options[:supported_tasks] = @supported_tasks unless @supported_tasks&.empty?

        command_client.register_worker(
          worker_id: options[:worker_id],
          max_concurrent_steps: options[:max_concurrent_steps],
          supported_namespaces: options[:supported_namespaces],
          step_timeout_ms: options[:step_timeout_ms],
          supports_retries: options[:supports_retries],
          language_runtime: options[:language_runtime],
          version: options[:version],
          custom_capabilities: options[:custom_capabilities],
          supported_tasks: options[:supported_tasks]
        )
      end


      # Check if singleton CommandClient is healthy
      #
      # @return [Boolean] true if healthy
      def command_client_healthy?
        begin
          command_client.connected?
        rescue StandardError
          false
        end
      end

      # Ensure worker is running
      #
      # @raise [WorkerError] if not running
      def ensure_running!
        raise WorkerError, "Worker not running" unless running?
      end

      def register_worker
        @logger.info("Worker initialized, registering with orchestrator")

        # Register worker with enhanced capabilities using shared CommandClient
        enhanced_capabilities = @custom_capabilities.merge(
          'ruby_worker' => true,
          'supports_execute_batch' => true,
          'manager_type' => 'shared_command_client'
        )

        response = register_worker_with_shared_client(enhanced_capabilities)
        @logger.info("Worker registered successfully: #{response}")
      end

      def unregister_worker
        # Unregister worker using shared CommandClient
        unregister_options = {
          worker_id: @worker_id,
          reason: "Unregistering worker"
        }

        response = command_client.unregister_worker(
          worker_id: unregister_options[:worker_id],
          reason: unregister_options[:reason]
        )
        @logger.info("Worker unregistered: #{response}")
      end
    end
    # Custom error for worker operations
    class WorkerError < StandardError; end
  end
end

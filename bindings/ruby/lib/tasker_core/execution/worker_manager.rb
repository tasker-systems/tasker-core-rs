# frozen_string_literal: true

require 'logger'
require_relative 'command_client'
require_relative 'command_listener'
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
                     server_host: 'localhost',
                     server_port: 8080,
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
        @server_host = server_host
        @server_port = server_port
        @current_load = 0
        @running = false
        @load_mutex = Mutex.new

        @logger = begin
          TaskerCore::Logging::Logger.instance
        rescue StandardError => e
          # Fallback logger if TaskerCore logger fails
          puts "WorkerManager: TaskerCore logger failed: #{e.message}, using fallback"
          Logger.new($stdout).tap do |log|
            log.level = Logger::INFO
            log.formatter = proc { |severity, datetime, progname, msg|
              "[#{datetime}] WorkerManager #{severity}: #{msg}\n"
            }
          end
        end

        # Debug: Verify logger is not nil
        if @logger.nil?
          puts "CRITICAL: @logger is still nil after initialization!"
          @logger = Logger.new($stdout)
        end

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
          @logger.info("Starting worker #{@worker_id} with shared CommandClient")

          # Get singleton CommandClient (auto-connects as needed)
          command_client = @orchestration_manager.create_command_client(
            host: @server_host,
            port: @server_port,
            timeout: @heartbeat_interval + 10
          )

          @logger.info("Worker initialized, registering with orchestrator")

          # Register worker with enhanced capabilities using shared CommandClient
          enhanced_capabilities = @custom_capabilities.merge(
            'ruby_worker' => true,
            'supports_execute_batch' => true,
            'manager_type' => 'shared_command_client'
          )

          response = register_worker_with_shared_client(enhanced_capabilities)
          @logger.info("Worker registered successfully: #{response}")

          # TODO: Implement proper heartbeat with shared CommandClient
          @logger.info("Heartbeat background task would be started here for worker #{@worker_id}")

          @running = true
          @logger.info("Worker #{@worker_id} started successfully with shared CommandClient")
          true
        rescue StandardError => e
          @logger.error("Failed to start worker #{@worker_id}: #{e.message}")
          cleanup
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

          # TODO: Stop automatic heartbeat when implemented
          @logger.info("Heartbeat stopping would happen here for worker #{@worker_id}")

          # Unregister worker using shared CommandClient
          unregister_options = {
            worker_id: @worker_id,
            reason: reason
          }
          # Get singleton CommandClient
          command_client = @orchestration_manager.create_command_client(
            host: @server_host,
            port: @server_port,
            timeout: @heartbeat_interval + 10
          )
          
          response = command_client.unregister_worker(
            worker_id: unregister_options[:worker_id],
            reason: unregister_options[:reason]
          )
          @logger.info("Worker unregistered: #{response}")

          @running = false
          @logger.info("Worker #{@worker_id} stopped successfully")
          true
        rescue StandardError => e
          @logger.error("Error stopping worker #{@worker_id}: #{e.message}")
          false
        end
      end

      # Check if worker is running
      #
      # @return [Boolean] true if running
      def running?
        @running
      end

      # Report step execution start (increases load)
      #
      # @param step_count [Integer] Number of steps starting
      def report_step_start(step_count = 1)
        @load_mutex.synchronize do
          @current_load += step_count
          @logger.debug("Worker load increased to #{@current_load}")
        end
      end

      # Report step execution completion (decreases load)
      #
      # @param step_count [Integer] Number of steps completed
      def report_step_completion(step_count = 1)
        @load_mutex.synchronize do
          @current_load = [@current_load - step_count, 0].max
          @logger.debug("Worker load decreased to #{@current_load}")
        end
      end

      # Get current worker statistics
      #
      # @return [Hash] Current worker stats
      def stats
        begin
          # Get connection info from singleton CommandClient
          command_client = @orchestration_manager.create_command_client(
            host: @server_host,
            port: @server_port,
            timeout: @heartbeat_interval + 10
          )
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
          
          # Get singleton CommandClient
          command_client = @orchestration_manager.create_command_client(
            host: @server_host,
            port: @server_port,
            timeout: @heartbeat_interval + 10
          )
          
          raw_response = command_client.send_heartbeat(
            worker_id: heartbeat_options[:worker_id],
            current_load: heartbeat_options[:current_load],
            system_stats: heartbeat_options[:system_stats]
          )
          @logger.debug("Manual heartbeat sent for worker #{@worker_id}")

          # Response is already typed by CommandClient, no conversion needed
          raw_response
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
        running? && singleton_command_client_healthy?
      end

      # Get detailed health information
      #
      # @return [Hash] Health details
      def health_details
        begin
          # Get health info from singleton CommandClient
          command_client = @orchestration_manager.create_command_client(
            host: @server_host,
            port: @server_port,
            timeout: @heartbeat_interval + 10
          )
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
        begin
          # Get all registered handlers from the orchestration system
          handlers_result = TaskerCore::TaskHandler::Base.list_registered_handlers

          if handlers_result && handlers_result['handlers']
            namespaces = []
            handlers_result['handlers'].each do |handler|
              # Handle both string keys and symbol keys
              namespace = handler['namespace'] || handler[:namespace] || 'default'
              namespaces << namespace unless namespaces.include?(namespace)
            end

            # Always include 'default' as fallback
            namespaces << 'default' unless namespaces.include?('default')

            @logger.info("Auto-discovered namespaces for worker #{@worker_id}: #{namespaces}")
            return namespaces
          end
        rescue StandardError => e
          @logger.warn("Failed to auto-discover namespaces: #{e.message}")
          @logger.debug("Error details: #{e.class.name}: #{e.message}")
          @logger.debug("Backtrace: #{e.backtrace.first(3).join(', ')}")
        end

        # Fallback to default namespace
        @logger.info("Using default namespace for worker #{@worker_id}")
        [DEFAULT_NAMESPACE]
      end

      # Register worker using shared CommandClient
      #
      # @param enhanced_capabilities [Hash] Worker capabilities
      # @return [TaskerCore::Types::ExecutionTypes::WorkerRegistrationResponse] Typed registration response
      def register_worker_with_shared_client(enhanced_capabilities)
        options = {
          worker_id: @worker_id,
          max_concurrent_steps: @max_concurrent_steps,
          supported_namespaces: @supported_namespaces,
          language_runtime: 'ruby',
          version: RUBY_VERSION,
          custom_capabilities: enhanced_capabilities
        }

        # Add connection info for proper worker registration
        # This provides the Rust side with details about how to communicate with this worker
        options[:connection_info] = {
          host: @server_host,
          port: @server_port,
          listener_port: nil, # Ruby workers don't typically listen on their own port
          transport_type: 'tcp',
          protocol_version: '1.0'
        }

        # Add supported tasks if provided (for database-backed task handler registration)
        if @supported_tasks && !@supported_tasks.empty?
          @logger.info("🎯 WORKER_MANAGER: Including #{@supported_tasks.size} supported tasks in RegisterWorker command")
          options[:supported_tasks] = @supported_tasks
          @logger.debug("🎯 WORKER_MANAGER: Task info: #{@supported_tasks.map { |t| "#{t[:namespace]}/#{t[:handler_name]}" }.join(', ')}")
        else
          @logger.info("🎯 WORKER_MANAGER: No supported_tasks provided - using namespace-based registration only")
        end

        @logger.info("🎯 WORKER_MANAGER: Including connection info: #{options[:connection_info]}")

        # Use singleton CommandClient - pass as keyword arguments
        command_client = @orchestration_manager.create_command_client(
          host: @server_host,
          port: @server_port,
          timeout: @heartbeat_interval + 10
        )
        
        raw_response = command_client.register_worker(
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
        
        # Response is already typed by CommandClient, no conversion needed
        raw_response
      end

      # Check if singleton CommandClient is healthy
      #
      # @return [Boolean] true if healthy
      def singleton_command_client_healthy?
        begin
          command_client = @orchestration_manager.create_command_client(
            host: @server_host,
            port: @server_port,
            timeout: @heartbeat_interval + 10
          )
          command_client.connected?
        rescue StandardError
          false
        end
      end

      # Cleanup resources
      def cleanup
        begin
          # TODO: Cleanup heartbeat when implemented
          @logger.info("cleanup called for worker #{@worker_id}")
        rescue StandardError => e
          @logger.warn("Error during cleanup: #{e.message}")
        end
      end

      # Ensure worker is running
      #
      # @raise [WorkerError] if not running
      def ensure_running!
        raise WorkerError, "Worker not running" unless running?
      end
    end

    # Custom error for worker operations
    class WorkerError < StandardError; end
  end
end

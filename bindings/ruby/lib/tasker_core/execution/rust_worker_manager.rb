# frozen_string_literal: true

require 'logger'

module TaskerCore
  module Execution
    # Ruby wrapper for Rust-backed WorkerManager
    #
    # This class provides a Ruby-friendly interface to the Rust-backed worker manager,
    # providing unified worker lifecycle management while eliminating Ruby dependencies.
    #
    # @example Simple worker registration
    #   manager = RustWorkerManager.new(
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
    #   manager = RustWorkerManager.new(
    #     worker_id: 'inventory_worker',
    #     max_concurrent_steps: 20,
    #     supported_namespaces: ['inventory', 'shipping'],
    #     server_host: 'rust-orchestrator.local',
    #     server_port: 9090,
    #     heartbeat_interval: 15
    #   )
    #
    class RustWorkerManager
      # Default configuration
      DEFAULT_MAX_CONCURRENT_STEPS = 10
      DEFAULT_STEP_TIMEOUT_MS = 30000
      DEFAULT_HEARTBEAT_INTERVAL = 30 # seconds
      DEFAULT_SUPPORTS_RETRIES = true
      DEFAULT_NAMESPACE = 'default'

      attr_reader :worker_id, :max_concurrent_steps, :supported_namespaces,
                  :step_timeout_ms, :supports_retries, :heartbeat_interval,
                  :running, :current_load, :custom_capabilities, :rust_manager

      def initialize(worker_id:, supported_namespaces: nil,
                     max_concurrent_steps: DEFAULT_MAX_CONCURRENT_STEPS,
                     step_timeout_ms: DEFAULT_STEP_TIMEOUT_MS,
                     supports_retries: DEFAULT_SUPPORTS_RETRIES,
                     heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
                     custom_capabilities: {},
                     server_host: 'localhost',
                     server_port: 8080)
        @worker_id = worker_id
        # Auto-discover namespaces if not explicitly provided
        @supported_namespaces = supported_namespaces || discover_registered_namespaces
        @max_concurrent_steps = max_concurrent_steps
        @step_timeout_ms = step_timeout_ms
        @supports_retries = supports_retries
        @heartbeat_interval = heartbeat_interval
        @custom_capabilities = custom_capabilities
        @current_load = 0
        @running = false
        @load_mutex = Mutex.new

        @logger = begin
          TaskerCore::Logging::Logger.instance
        rescue StandardError => e
          # Fallback logger if TaskerCore logger fails
          puts "RustWorkerManager: TaskerCore logger failed: #{e.message}, using fallback"
          Logger.new($stdout).tap do |log|
            log.level = Logger::INFO
            log.formatter = proc { |severity, datetime, progname, msg|
              "[#{datetime}] RustWorkerManager #{severity}: #{msg}\n"
            }
          end
        end
        
        # Debug: Verify logger is not nil
        if @logger.nil?
          puts "CRITICAL: @logger is still nil after initialization!"
          @logger = Logger.new($stdout)
        end

        # Create Rust-backed worker manager
        manager_config = {
          server_host: server_host,
          server_port: server_port,
          heartbeat_interval_seconds: heartbeat_interval,
          worker_namespace: @supported_namespaces.first || DEFAULT_NAMESPACE
        }

        @rust_manager = TaskerCore::WorkerManager.new_with_config(manager_config)
      end

      # Start worker - initialize as worker and register with Rust orchestrator
      #
      # @return [Boolean] true if started successfully
      # @raise [WorkerError] if startup fails
      def start
        return true if running?

        begin
          @logger.info("Starting Rust-backed worker #{@worker_id}")

          # Initialize as worker (connects to server)
          success = @rust_manager.initialize_as_worker
          unless success
            raise WorkerError, "Failed to initialize worker connection"
          end

          @logger.info("Worker initialized, registering with orchestrator")

          # Register worker with enhanced capabilities
          enhanced_capabilities = @custom_capabilities.merge(
            'ruby_worker' => true,
            'supports_execute_batch' => true,
            'manager_type' => 'rust_backed'
          )

          response = register_worker_with_rust(enhanced_capabilities)
          @logger.info("Worker registered successfully: #{response}")

          # Start automatic heartbeat
          heartbeat_success = @rust_manager.start_heartbeat(@worker_id, @heartbeat_interval)
          unless heartbeat_success
            @logger.warn("Failed to start automatic heartbeat, will send manual heartbeats")
          end

          @running = true
          @logger.info("Rust-backed worker #{@worker_id} started successfully")
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
          @logger.info("Stopping Rust-backed worker #{@worker_id}: #{reason}")

          # Stop automatic heartbeat
          @rust_manager.stop_heartbeat

          # Unregister worker
          response = @rust_manager.unregister_worker(@worker_id, reason)
          @logger.info("Worker unregistered: #{response}")

          @running = false
          @logger.info("Rust-backed worker #{@worker_id} stopped successfully")
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
          # Get stats from Rust manager
          rust_stats = @rust_manager.get_statistics
          manager_status = @rust_manager.get_status

          # Combine Ruby and Rust statistics
          base_stats = {
            worker_id: @worker_id,
            running: running?,
            current_load: @current_load,
            max_concurrent_steps: @max_concurrent_steps,
            available_capacity: [@max_concurrent_steps - @current_load, 0].max,
            load_percentage: (@current_load.to_f / @max_concurrent_steps * 100).round(1),
            supported_namespaces: @supported_namespaces,
            supports_retries: @supports_retries,
            manager_type: 'rust_backed'
          }

          # Merge with Rust statistics
          base_stats.merge(
            rust_statistics: rust_stats,
            rust_status: manager_status
          )
        rescue StandardError => e
          @logger.warn("Failed to get Rust statistics: #{e.message}")
          # Return basic Ruby statistics as fallback
          {
            worker_id: @worker_id,
            running: running?,
            current_load: @current_load,
            max_concurrent_steps: @max_concurrent_steps,
            error: "Failed to get Rust stats: #{e.message}"
          }
        end
      end

      # Send immediate heartbeat (useful for testing or manual updates)
      #
      # @return [Hash] Heartbeat response
      def send_heartbeat
        ensure_running!

        current_load = @load_mutex.synchronize { @current_load }

        begin
          response = @rust_manager.send_heartbeat(@worker_id, current_load)
          @logger.debug("Manual heartbeat sent for worker #{@worker_id}")
          symbolize_response(response)
        rescue StandardError => e
          @logger.error("Manual heartbeat failed: #{e.message}")
          raise WorkerError, "Heartbeat failed: #{e.message}"
        end
      end

      # Check connection health
      #
      # @return [Boolean] true if healthy
      def healthy?
        running? && rust_manager_healthy?
      end

      # Get detailed health information
      #
      # @return [Hash] Health details
      def health_details
        begin
          status = @rust_manager.get_status
          stats = @rust_manager.get_statistics
          
          {
            healthy: healthy?,
            running: running?,
            rust_status: status,
            rust_statistics: stats,
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

      # Register worker using Rust manager
      #
      # @param enhanced_capabilities [Hash] Worker capabilities
      # @return [Hash] Registration response
      def register_worker_with_rust(enhanced_capabilities)
        options = {
          worker_id: @worker_id,
          max_concurrent_steps: @max_concurrent_steps,
          supported_namespaces: @supported_namespaces,
          language_runtime: 'ruby',
          version: RUBY_VERSION,
          custom_capabilities: enhanced_capabilities
        }

        response = @rust_manager.register_worker(options)
        symbolize_response(response)
      end

      # Check if Rust manager is healthy
      #
      # @return [Boolean] true if healthy
      def rust_manager_healthy?
        begin
          status = @rust_manager.get_status
          status && status['status'] != 'Error'
        rescue StandardError
          false
        end
      end

      # Cleanup resources
      def cleanup
        begin
          @rust_manager.stop_heartbeat if @rust_manager
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

      # Convert response to Ruby hash with symbolized keys for compatibility
      #
      # @param response [Hash] Response from Rust manager
      # @return [Hash] Response with symbolized keys
      def symbolize_response(response)
        case response
        when Hash
          response.transform_keys do |key|
            key.is_a?(String) ? key.to_sym : key
          end.transform_values { |value| symbolize_response(value) }
        when Array
          response.map { |item| symbolize_response(item) }
        else
          response
        end
      end
    end

    # Custom error for Rust worker operations
    class RustWorkerError < WorkerError; end
  end
end
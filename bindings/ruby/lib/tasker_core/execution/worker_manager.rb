# frozen_string_literal: true

require_relative 'command_client'

module TaskerCore
  module Execution
    # High-level Ruby worker management for Rust Command Executor integration
    #
    # Provides an easy-to-use interface for Ruby workers to register with
    # the Rust orchestration system, maintain heartbeats, and handle
    # lifecycle management.
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
    #     executor_host: 'rust-orchestrator.local',
    #     executor_port: 9090,
    #     heartbeat_interval: 15
    #   )
    #
    class WorkerManager
      # Default configuration
      DEFAULT_MAX_CONCURRENT_STEPS = 10
      DEFAULT_STEP_TIMEOUT_MS = 30000
      DEFAULT_HEARTBEAT_INTERVAL = 30 # seconds
      DEFAULT_SUPPORTS_RETRIES = true

      attr_reader :worker_id, :max_concurrent_steps, :supported_namespaces,
                  :step_timeout_ms, :supports_retries, :heartbeat_interval,
                  :command_client, :running, :heartbeat_thread, :current_load,
                  :custom_capabilities

      def initialize(worker_id:, supported_namespaces:, 
                     max_concurrent_steps: DEFAULT_MAX_CONCURRENT_STEPS,
                     step_timeout_ms: DEFAULT_STEP_TIMEOUT_MS,
                     supports_retries: DEFAULT_SUPPORTS_RETRIES,
                     heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
                     executor_host: CommandClient::DEFAULT_HOST,
                     executor_port: CommandClient::DEFAULT_PORT,
                     timeout: CommandClient::DEFAULT_TIMEOUT,
                     custom_capabilities: {},
                     logger: nil)
        @worker_id = worker_id
        @supported_namespaces = Array(supported_namespaces)
        @max_concurrent_steps = max_concurrent_steps
        @step_timeout_ms = step_timeout_ms
        @supports_retries = supports_retries
        @heartbeat_interval = heartbeat_interval
        @custom_capabilities = custom_capabilities
        @current_load = 0
        @running = false
        @heartbeat_thread = nil
        @load_mutex = Mutex.new

        @command_client = CommandClient.new(
          host: executor_host,
          port: executor_port,
          timeout: timeout,
          logger: logger
        )

        @logger = logger || default_logger
      end

      # Start worker - register with Rust orchestrator and begin heartbeats
      #
      # @return [Boolean] true if started successfully
      # @raise [WorkerError] if startup fails
      def start
        return true if running?

        begin
          @logger.info("Starting worker #{@worker_id}")
          
          # Connect to Rust executor
          @command_client.connect
          
          # Register worker
          response = register_worker
          @logger.info("Worker registered successfully: #{response}")
          
          # Start heartbeat thread
          start_heartbeat_thread
          
          @running = true
          @logger.info("Worker #{@worker_id} started successfully")
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
          
          # Stop heartbeat thread
          stop_heartbeat_thread
          
          # Unregister worker
          if @command_client.connected?
            response = @command_client.unregister_worker(
              worker_id: @worker_id,
              reason: reason
            )
            @logger.info("Worker unregistered: #{response}")
          end
          
          # Cleanup
          cleanup
          
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
        {
          worker_id: @worker_id,
          running: running?,
          current_load: @current_load,
          max_concurrent_steps: @max_concurrent_steps,
          available_capacity: [@max_concurrent_steps - @current_load, 0].max,
          load_percentage: (@current_load.to_f / @max_concurrent_steps * 100).round(1),
          supported_namespaces: @supported_namespaces,
          supports_retries: @supports_retries
        }
      end

      # Send immediate heartbeat (useful for testing or manual updates)
      #
      # @return [Hash] Heartbeat response
      def send_heartbeat
        ensure_running!
        
        current_load = @load_mutex.synchronize { @current_load }
        system_stats = collect_system_stats
        
        @command_client.send_heartbeat(
          worker_id: @worker_id,
          current_load: current_load,
          system_stats: system_stats
        )
      end

      # Check connection health
      #
      # @return [Boolean] true if healthy
      def healthy?
        running? && @command_client.connected?
      end

      private

      # Register worker with Rust orchestrator
      #
      # @return [Hash] Registration response
      def register_worker
        @command_client.register_worker(
          worker_id: @worker_id,
          max_concurrent_steps: @max_concurrent_steps,
          supported_namespaces: @supported_namespaces,
          step_timeout_ms: @step_timeout_ms,
          supports_retries: @supports_retries,
          language_runtime: 'ruby',
          version: RUBY_VERSION,
          custom_capabilities: @custom_capabilities
        )
      end

      # Start heartbeat thread
      def start_heartbeat_thread
        return if @heartbeat_thread&.alive?

        @heartbeat_thread = Thread.new do
          Thread.current.name = "heartbeat-#{@worker_id}"
          
          loop do
            break unless running?
            
            begin
              send_heartbeat
              @logger.debug("Heartbeat sent for worker #{@worker_id}")
            rescue StandardError => e
              @logger.error("Heartbeat failed for worker #{@worker_id}: #{e.message}")
              # Continue heartbeat attempts - don't fail worker for single heartbeat failure
            end
            
            sleep(@heartbeat_interval)
          end
        end
      end

      # Stop heartbeat thread
      def stop_heartbeat_thread
        return unless @heartbeat_thread

        @heartbeat_thread.kill if @heartbeat_thread.alive?
        @heartbeat_thread.join(5) # Wait up to 5 seconds for graceful shutdown
        @heartbeat_thread = nil
      end

      # Collect system statistics
      #
      # @return [Hash, nil] System stats or nil if unavailable
      def collect_system_stats
        begin
          # Basic system stats - could be enhanced with more detailed metrics
          {
            cpu_usage_percent: 0.0, # Placeholder - could use system calls
            memory_usage_mb: (Process.memory_usage[:rss] / 1024 / 1024).round,
            active_connections: 1, # Connection to Rust executor
            uptime_seconds: (Time.now - start_time).to_i
          }
        rescue StandardError => e
          @logger.warn("Failed to collect system stats: #{e.message}")
          nil
        end
      end

      # Get worker start time
      #
      # @return [Time] Start time
      def start_time
        @start_time ||= Time.now
      end

      # Cleanup resources
      def cleanup
        stop_heartbeat_thread
        @command_client.disconnect
      end

      # Ensure worker is running
      #
      # @raise [WorkerError] if not running
      def ensure_running!
        raise WorkerError, "Worker not running" unless running?
      end

      # Create default logger
      #
      # @return [Logger] Default logger
      def default_logger
        logger = Logger.new(STDOUT)
        logger.level = Logger::INFO
        logger.formatter = proc do |severity, datetime, progname, msg|
          "[#{datetime}] Worker[#{@worker_id}] #{severity}: #{msg}\n"
        end
        logger
      end
    end

    # Custom error for worker operations
    class WorkerError < StandardError; end
  end
end
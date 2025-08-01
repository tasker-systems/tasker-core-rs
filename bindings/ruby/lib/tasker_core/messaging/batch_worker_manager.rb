# frozen_string_literal: true

require 'concurrent-ruby'
require_relative 'batch_queue_worker'

module TaskerCore
  module Messaging
    ##
    # BatchWorkerManager - Manages multiple BatchQueueWorkers for different namespaces
    #
    # This manager:
    # 1. Starts workers for multiple namespaces concurrently
    # 2. Handles graceful shutdown of all workers
    # 3. Provides centralized monitoring and statistics
    # 4. Manages worker lifecycle and error recovery
    # 5. Supports dynamic worker scaling per namespace
    class BatchWorkerManager
      include TaskerCore::Utils::Logging

      attr_reader :config, :workers, :running

      def initialize(config = {})
        @config = default_config.merge(config)
        @workers = {}
        @worker_threads = {}
        @running = false
        @shutdown_requested = false

        validate_configuration!
        log_info "BatchWorkerManager initialized"
      end

      ##
      # Start workers for all configured namespaces
      def start
        return if @running

        @running = true
        @shutdown_requested = false
        @started_at = Time.now

        log_info "Starting BatchWorkerManager"
        log_info "Configured namespaces: #{config[:namespaces].join(', ')}"
        log_info "Workers per namespace: #{config[:workers_per_namespace]}"

        config[:namespaces].each do |namespace|
          start_workers_for_namespace(namespace)
        end

        log_info "All workers started successfully"

        # Set up signal handlers for graceful shutdown
        setup_signal_handlers if config[:handle_signals]

        # Start monitoring thread if enabled
        start_monitoring_thread if config[:monitoring_enabled]

        # Keep the main thread alive
        keep_alive_loop if config[:block_main_thread]

      rescue => e
        log_error "Failed to start BatchWorkerManager: #{e.message}"
        log_error e.backtrace.join("\n")
        stop
        raise
      end

      ##
      # Stop all workers gracefully
      def stop
        return unless @running

        log_info "Stopping BatchWorkerManager"
        @shutdown_requested = true

        # Request shutdown for all workers
        workers.each_value do |worker_list|
          worker_list.each(&:stop)
        end

        # Wait for worker threads to finish
        @worker_threads.each_value do |thread_list|
          thread_list.each do |thread|
            begin
              thread.join(config[:shutdown_timeout_seconds])
            rescue => e
              log_error "Error waiting for worker thread to finish: #{e.message}"
            end
          end
        end

        @running = false
        log_info "BatchWorkerManager stopped"
      end

      ##
      # Get comprehensive statistics for all workers
      def statistics
        worker_stats = {}
        total_stats = {
          processed_batches: 0,
          processed_steps: 0,
          failed_batches: 0,
          failed_steps: 0,
          active_workers: 0
        }

        workers.each do |namespace, worker_list|
          namespace_stats = {
            workers: [],
            totals: {
              processed_batches: 0,
              processed_steps: 0,
              failed_batches: 0,
              failed_steps: 0,
              active_workers: 0
            }
          }

          worker_list.each do |worker|
            stats = worker.statistics
            namespace_stats[:workers] << stats
            
            # Aggregate namespace totals
            namespace_stats[:totals][:processed_batches] += stats[:processed_batches]
            namespace_stats[:totals][:processed_steps] += stats[:processed_steps]
            namespace_stats[:totals][:failed_batches] += stats[:failed_batches]
            namespace_stats[:totals][:failed_steps] += stats[:failed_steps]
            namespace_stats[:totals][:active_workers] += 1 if stats[:running]
          end

          worker_stats[namespace] = namespace_stats

          # Aggregate global totals
          total_stats[:processed_batches] += namespace_stats[:totals][:processed_batches]
          total_stats[:processed_steps] += namespace_stats[:totals][:processed_steps]
          total_stats[:failed_batches] += namespace_stats[:totals][:failed_batches]
          total_stats[:failed_steps] += namespace_stats[:totals][:failed_steps]
          total_stats[:active_workers] += namespace_stats[:totals][:active_workers]
        end

        {
          manager: {
            running: @running,
            shutdown_requested: @shutdown_requested,
            started_at: @started_at,
            uptime_seconds: @started_at ? (Time.now - @started_at).round : 0,
            configured_namespaces: config[:namespaces],
            workers_per_namespace: config[:workers_per_namespace]
          },
          totals: total_stats,
          namespaces: worker_stats
        }
      end

      ##
      # Scale workers for a specific namespace
      def scale_namespace(namespace, new_worker_count)
        return unless config[:namespaces].include?(namespace)
        return if new_worker_count < 1

        current_workers = workers[namespace] || []
        current_count = current_workers.size

        if new_worker_count > current_count
          # Add more workers
          (new_worker_count - current_count).times do |i|
            add_worker_for_namespace(namespace, current_count + i + 1)
          end
          log_info "Scaled #{namespace} workers from #{current_count} to #{new_worker_count}"
        elsif new_worker_count < current_count
          # Remove workers
          workers_to_remove = current_workers.last(current_count - new_worker_count)
          threads_to_remove = @worker_threads[namespace].last(current_count - new_worker_count)
          
          workers_to_remove.each(&:stop)
          threads_to_remove.each { |thread| thread.join(config[:shutdown_timeout_seconds]) }
          
          workers[namespace] = current_workers.first(new_worker_count)
          @worker_threads[namespace] = @worker_threads[namespace].first(new_worker_count)
          
          log_info "Scaled #{namespace} workers from #{current_count} to #{new_worker_count}"
        end
      end

      ##
      # Check if manager is running
      def running?
        @running
      end

      ##
      # Get health status
      def health_check
        return { status: 'stopped', message: 'Manager not running' } unless @running

        unhealthy_workers = []
        total_workers = 0

        workers.each do |namespace, worker_list|
          worker_list.each_with_index do |worker, index|
            total_workers += 1
            unless worker.running?
              unhealthy_workers << "#{namespace}-#{index + 1}"
            end
          end
        end

        if unhealthy_workers.empty?
          {
            status: 'healthy',
            message: "All #{total_workers} workers running",
            total_workers: total_workers,
            healthy_workers: total_workers
          }
        else
          {
            status: 'degraded',
            message: "#{unhealthy_workers.size}/#{total_workers} workers not running",
            total_workers: total_workers,
            healthy_workers: total_workers - unhealthy_workers.size,
            unhealthy_workers: unhealthy_workers
          }
        end
      end

      private

      ##
      # Start workers for a specific namespace
      def start_workers_for_namespace(namespace)
        log_info "Starting #{config[:workers_per_namespace]} workers for namespace: #{namespace}"
        
        workers[namespace] = []
        @worker_threads[namespace] = []

        config[:workers_per_namespace].times do |i|
          add_worker_for_namespace(namespace, i + 1)
        end
      end

      ##
      # Add a single worker for a namespace
      def add_worker_for_namespace(namespace, worker_number)
        worker_config = config[:worker_config].merge(
          log_level: config[:log_level]
        )

        worker = BatchQueueWorker.new(
          namespace: namespace,
          config: worker_config
        )

        workers[namespace] ||= []
        workers[namespace] << worker

        # Start worker in its own thread
        thread = Thread.new do
          Thread.current.name = "#{namespace}-worker-#{worker_number}"
          
          begin
            worker.start
          rescue => e
            log_error "Worker #{namespace}-#{worker_number} crashed: #{e.message}"
            log_error e.backtrace.join("\n")
          end
        end

        @worker_threads[namespace] ||= []
        @worker_threads[namespace] << thread

        log_debug "Started worker #{namespace}-#{worker_number}"
      end

      ##
      # Setup signal handlers for graceful shutdown
      def setup_signal_handlers
        %w[INT TERM].each do |signal|
          Signal.trap(signal) do
            log_info "Received #{signal} signal, initiating graceful shutdown"
            stop
            exit(0)
          end
        end
      end

      ##
      # Start monitoring thread
      def start_monitoring_thread
        @monitoring_thread = Thread.new do
          Thread.current.name = 'worker-monitor'
          
          while @running && !@shutdown_requested
            begin
              monitor_workers
              sleep(config[:monitoring_interval_seconds])
            rescue => e
              log_error "Error in monitoring thread: #{e.message}"
            end
          end
        end
      end

      ##
      # Monitor worker health and restart failed workers
      def monitor_workers
        workers.each do |namespace, worker_list|
          worker_list.each_with_index do |worker, index|
            unless worker.running?
              log_warn "Worker #{namespace}-#{index + 1} is not running, restarting..."
              
              # Stop the old worker cleanly
              worker.stop
              
              # Create a new worker
              new_worker = BatchQueueWorker.new(
                namespace: namespace,
                config: config[:worker_config]
              )
              
              # Replace in the list
              workers[namespace][index] = new_worker
              
              # Start in a new thread
              @worker_threads[namespace][index] = Thread.new do
                Thread.current.name = "#{namespace}-worker-#{index + 1}-restarted"
                
                begin
                  new_worker.start
                rescue => e
                  log_error "Restarted worker #{namespace}-#{index + 1} crashed: #{e.message}"
                end
              end
              
              log_info "Restarted worker #{namespace}-#{index + 1}"
            end
          end
        end
      end

      ##
      # Keep main thread alive
      def keep_alive_loop
        while @running && !@shutdown_requested
          sleep(1)
        end
      end

      ##
      # Default configuration
      def default_config
        {
          # Namespaces to process
          namespaces: %w[fulfillment inventory notifications payments analytics],
          
          # Worker scaling
          workers_per_namespace: 2,
          
          # Worker configuration passed to each BatchQueueWorker
          worker_config: {
            polling_interval_seconds: 1,
            batch_size: 5,
            visibility_timeout_seconds: 300,
            max_concurrent_steps: 4,
            step_timeout_seconds: 60,
            error_retry_delay_seconds: 5,
            results_queue_name: 'orchestration_batch_results'
          },
          
          # Manager settings
          shutdown_timeout_seconds: 30,
          handle_signals: true,
          block_main_thread: true,
          
          # Monitoring
          monitoring_enabled: true,
          monitoring_interval_seconds: 30,
          
          # Logging
          log_level: :info
        }
      end

      ##
      # Validate configuration
      def validate_configuration!
        required_keys = [:namespaces, :workers_per_namespace, :worker_config]
        
        required_keys.each do |key|
          raise ArgumentError, "Missing required config key: #{key}" unless config.key?(key)
        end

        raise ArgumentError, "namespaces must be an array" unless config[:namespaces].is_a?(Array)
        raise ArgumentError, "namespaces cannot be empty" if config[:namespaces].empty?
        raise ArgumentError, "workers_per_namespace must be positive" unless config[:workers_per_namespace] > 0
        raise ArgumentError, "worker_config must be a hash" unless config[:worker_config].is_a?(Hash)
      end
    end
  end
end
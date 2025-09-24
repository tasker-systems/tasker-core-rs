# frozen_string_literal: true

module TaskerCore
  module Worker
    # Bootstrap orchestrator for Ruby worker
    # Manages initialization of both Rust foundation and Ruby components
    class Bootstrap
      include Singleton

      attr_reader :logger, :config, :rust_handle

      def initialize
        @logger = TaskerCore::Logger.instance
        @status = :initialized
        @shutdown_handlers = []
        @rust_handle = nil
      end

      # Start the worker system with optional configuration
      def self.start!(config = {})
        instance.start!(config)
      end

      # Main bootstrap method
      def start!(config = {})
        @config = default_config.merge(config)

        logger.info 'Starting Ruby worker bootstrap'
        logger.info "Configuration: #{@config.inspect}"

        # Initialize Ruby components first
        initialize_ruby_components!

        # Bootstrap Rust foundation via FFI
        bootstrap_rust_foundation!

        # Start event processing
        start_event_processing!

        # Register shutdown handlers
        register_shutdown_handlers!

        @status = :running
        logger.info 'Ruby worker system started successfully'

        self
      rescue StandardError => e
        logger.error "Failed to start worker: #{e.message}"
        logger.error e.backtrace.join("\n")
        shutdown!
        raise
      end

      # Check if worker is running
      def running?
        @status == :running && rust_worker_running?
      end

      # Check if Rust handle is valid and running
      def rust_handle_running?
        return false unless @rust_handle.is_a?(Hash) &&
                            (@rust_handle[:handle_id] || @rust_handle['handle_id'])

        rust_worker_running?
      end

      # Get comprehensive status
      def status
        rust_status = TaskerCore::FFI.worker_status
        {
          rust: rust_status,
          ruby: {
            status: @status,
            handle_stored: !@rust_handle.nil?,
            handle_id: @rust_handle&.dig('handle_id') || @rust_handle&.dig(:handle_id),
            worker_id: @rust_handle&.dig('worker_id') || @rust_handle&.dig(:worker_id),
            event_bridge_active: EventBridge.instance.active?,
            handler_registry_size: Registry::HandlerRegistry.instance.handlers.size,
            subscriber_active: @step_subscriber&.active? || false
          }
        }
      rescue StandardError => e
        logger.error "Failed to get status: #{e.message}"
        { error: e.message, status: @status }
      end

      # Execute a block with the Rust handle, bootstrapping if necessary
      def with_rust_handle(&block)
        bootstrap_rust_foundation! unless rust_handle_running?
        block.call
      end

      # Graceful shutdown
      def shutdown!
        return if @status == :stopped

        logger.info 'Initiating graceful shutdown'
        @status = :shutting_down

        # Transition Rust to graceful shutdown first
        if @rust_handle
          begin
            TaskerCore::FFI.transition_to_graceful_shutdown
          rescue StandardError => e
            logger.error "Failed to transition to graceful shutdown: #{e.message}"
          end
        end

        # Stop Ruby components
        @step_subscriber&.stop!
        EventBridge.instance.stop!

        # Stop Rust worker and clear handle
        if @rust_handle
          begin
            TaskerCore::FFI.stop_worker
          rescue StandardError => e
            logger.error "Failed to stop Rust worker: #{e.message}"
          ensure
            @rust_handle = nil
          end
        end

        # Run custom shutdown handlers
        @shutdown_handlers.each do |handler|
          handler.call
        rescue StandardError
          nil
        end

        @status = :stopped
        logger.info 'Worker shutdown complete'
      end

      # Register custom shutdown handler
      def on_shutdown(&block)
        @shutdown_handlers << block if block_given?
      end

      # Perform health check on both Ruby and Rust components
      def health_check
        return { healthy: false, status: @status, error: 'not_running' } unless running?

        begin
          # Get Rust worker status
          rust_status = TaskerCore::FFI.worker_status
          rust_running = rust_status['running'] || rust_status[:running]

          # Check Ruby components
          ruby_healthy = @status == :running &&
                         EventBridge.instance.active? &&
                         Registry::HandlerRegistry.instance.handlers.any?

          overall_healthy = rust_running && ruby_healthy

          {
            healthy: overall_healthy,
            status: @status,
            rust: {
              running: rust_running,
              worker_core_status: rust_status['worker_core_status'] || rust_status[:worker_core_status]
            },
            ruby: {
              status: @status,
              event_bridge_active: EventBridge.instance.active?,
              handlers_registered: Registry::HandlerRegistry.instance.handlers.size,
              subscriber_active: @step_subscriber&.active? || false
            }
          }
        rescue StandardError => e
          logger.error "Health check failed: #{e.message}"
          {
            healthy: false,
            status: @status,
            error: e.message
          }
        end
      end

      private

      def default_config
        {
          worker_id: "ruby-worker-#{SecureRandom.uuid}",
          enable_web_api: false,
          event_driven_enabled: true,
          deployment_mode: 'Hybrid',
          namespaces: detect_namespaces
        }
      end

      def detect_namespaces
        # Auto-detect from registered handlers
        Registry::HandlerRegistry.instance.registered_handlers.map do |handler_class|
          handler_class.namespace if handler_class.respond_to?(:namespace)
        end.compact.uniq
      end

      def initialize_ruby_components!
        logger.info 'Initializing Ruby components...'

        # Initialize event bridge
        EventBridge.instance

        # Initialize handler registry (bootstrap happens automatically)
        Registry::HandlerRegistry.instance

        # Initialize step execution subscriber
        @step_subscriber = StepExecutionSubscriber.new

        logger.info 'Ruby components initialized'
      end

      def bootstrap_rust_foundation!
        # Check if we already have a running handle
        if rust_handle_running?
          logger.debug 'Rust worker foundation already running, reusing handle'
          return @rust_handle
        end

        logger.info 'Bootstrapping Rust worker foundation...'

        # Bootstrap the worker and store the handle result
        result = TaskerCore::FFI.bootstrap_worker
        logger.info "Rust bootstrap result: #{result.inspect}"

        # Check if it was already running or newly started
        # Handle both string and symbol keys from Rust FFI
        status = result['status'] || result[:status]
        worker_id = result['worker_id'] || result[:worker_id]

        if status == 'already_running'
          logger.info 'Worker system was already running, reusing existing handle'
        elsif status == 'started'
          logger.info "New worker system started with ID: #{worker_id}"
        else
          raise "Unexpected bootstrap status: #{status}"
        end

        # Store the handle information
        @rust_handle = result

        # Verify it's running
        status = TaskerCore::FFI.worker_status
        # Handle both string and symbol keys from Rust FFI
        running = status['running'] || status[:running]
        unless running
          @rust_handle = nil
          raise "Rust worker failed to start: #{status.inspect}"
        end

        handle_id = @rust_handle['handle_id'] || @rust_handle[:handle_id]
        logger.info "Rust foundation bootstrapped with handle: #{handle_id}"
        @rust_handle
      end

      def start_event_processing!
        logger.info 'Starting event processing...'

        # Subscribe to step execution events
        EventBridge.instance.subscribe_to_step_execution do |event|
          @step_subscriber.call(event)
        end

        logger.info 'Event processing started'
      end

      def register_shutdown_handlers!
        # Graceful shutdown on signals
        %w[INT TERM].each do |signal|
          Signal.trap(signal) do
            Thread.new { shutdown! }
          end
        end

        # Shutdown on exit
        at_exit { shutdown! if running? }
      end

      def rust_worker_running?
        status = TaskerCore::FFI.worker_status
        # Handle both string and symbol keys from Rust FFI
        running = status['running'] || status[:running]
        running == true
      rescue StandardError
        false
      end
    end
  end
end

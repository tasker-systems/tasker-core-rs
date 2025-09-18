# frozen_string_literal: true

module TaskerCore
  module Worker
    # Bootstrap orchestrator for Ruby worker
    # Manages initialization of both Rust foundation and Ruby components
    class Bootstrap
      include Singleton

      attr_reader :logger, :config

      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        @status = :initialized
        @shutdown_handlers = []
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
        logger.info "✅ Ruby worker system started successfully"

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

      # Get comprehensive status
      def status
        rust_status = TaskerCore::FFI.worker_status
        {
          rust: rust_status,
          ruby: {
            status: @status,
            event_bridge_active: EventBridge.instance.active?,
            handler_registry_size: Registry::HandlerRegistry.instance.handlers.size,
            subscriber_active: @step_subscriber&.active? || false
          }
        }
      rescue StandardError => e
        logger.error "Failed to get status: #{e.message}"
        { error: e.message, status: @status }
      end

      # Graceful shutdown
      def shutdown!
        return if @status == :stopped

        logger.info "🛑 Initiating graceful shutdown"
        @status = :shutting_down

        # Transition Rust to graceful shutdown first
        begin
          TaskerCore::FFI.transition_to_graceful_shutdown
        rescue StandardError => e
          logger.error "Failed to transition to graceful shutdown: #{e.message}"
        end

        # Stop Ruby components
        @step_subscriber&.stop!
        EventBridge.instance.stop!

        # Stop Rust worker
        begin
          TaskerCore::FFI.stop_worker
        rescue StandardError => e
          logger.error "Failed to stop Rust worker: #{e.message}"
        end

        # Run custom shutdown handlers
        @shutdown_handlers.each do |handler|
          handler.call rescue nil
        end

        @status = :stopped
        logger.info "✅ Worker shutdown complete"
      end

      # Register custom shutdown handler
      def on_shutdown(&block)
        @shutdown_handlers << block if block_given?
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
        logger.info "Initializing Ruby components..."

        # Initialize event bridge
        EventBridge.instance

        # Initialize handler registry
        Registry::HandlerRegistry.instance.bootstrap_handlers!

        # Initialize step execution subscriber
        @step_subscriber = StepExecutionSubscriber.new(
          logger: logger,
          handler_registry: Registry::HandlerRegistry.instance
        )

        logger.info "✅ Ruby components initialized"
      end

      def bootstrap_rust_foundation!
        logger.info "Bootstrapping Rust worker foundation..."

        result = TaskerCore::FFI.bootstrap_worker(@config)
        logger.info "Rust bootstrap result: #{result}"

        # Verify it's running
        status = TaskerCore::FFI.worker_status
        unless status[:running]
          raise "Rust worker failed to start: #{status.inspect}"
        end

        logger.info "✅ Rust foundation bootstrapped"
      end

      def start_event_processing!
        logger.info "Starting event processing..."

        # Subscribe to step execution events
        EventBridge.instance.subscribe_to_step_execution do |event|
          @step_subscriber.call(event)
        end

        logger.info "✅ Event processing started"
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
        status[:running] == true
      rescue
        false
      end
    end
  end
end

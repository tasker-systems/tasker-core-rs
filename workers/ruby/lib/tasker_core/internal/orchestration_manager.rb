# frozen_string_literal: true

require 'singleton'
require_relative '../logging/logger'

module TaskerCore
  module Internal
    # Simplified orchestration manager for pgmq-based architecture
    #
    # This provides a singleton pattern at the Ruby level to manage
    # orchestration system initialization and queue bootstrapping
    class OrchestrationManager
      include Singleton

      attr_reader :initialized_at, :status, :logger

      def initialize
        @initialized = false
        @status = 'not_initialized'
        @initialized_at = nil
        @orchestration_mode = nil
        @base_task_handler = nil
        @pgmq_client = nil
        @embedded_orchestrator = nil
        @logger = TaskerCore::Logging::Logger.instance
        # Registry is accessed as singleton, no need to store instance
      end

      # Get the orchestration system (pgmq-based initialization)
      def orchestration_system
        bootstrap_orchestration_system unless @initialized

        {
          'architecture' => 'pgmq',
          'mode' => orchestration_mode,
          'pgmq_available' => pgmq_available?,
          'embedded_orchestrator_available' => embedded_orchestrator_available?
        }
      end

      # Check if the orchestration system is initialized
      def initialized?
        @initialized
      end

      # Get orchestration system status information
      def info
        base_info = {
          initialized: @initialized,
          status: @status,
          initialized_at: @initialized_at,
          architecture: 'pgmq',
          mode: orchestration_mode,
          pgmq_available: pgmq_available?,
          embedded_orchestrator_available: embedded_orchestrator_available?
        }

        # Add handler registry information for distributed mode
        base_info[:handler_registry] = if orchestration_mode == 'distributed'
                                         {
                                           available: true,
                                           stats: TaskerCore::Registry::StepHandlerResolver.instance.stats
                                         }
                                       else
                                         { available: false }
                                       end

        base_info
      end

      # Reset the orchestration system (for testing)
      def reset!
        logger.info '🧹 Starting orchestration system reset...'

        @initialized = false
        @status = 'reset'
        @initialized_at = nil
        @orchestration_mode = nil
        @base_task_handler = nil

        # Stop embedded orchestrator if running
        if @embedded_orchestrator
          begin
            logger.debug '🔌 Stopping embedded orchestrator...'
            TaskerCore.stop_embedded_orchestration! if orchestration_mode == 'embedded'
            logger.debug '✅ Embedded orchestrator stopped'
          rescue StandardError => e
            logger.warn "⚠️ Failed to stop embedded orchestrator during reset: #{e.message}"
          end
          @embedded_orchestrator = nil
        end

        # Close pgmq client connections
        if @pgmq_client
          begin
            logger.debug '🔌 Closing pgmq client connections...'
            @pgmq_client.close if @pgmq_client.respond_to?(:close)
            logger.debug '✅ pgmq client connections closed'
          rescue StandardError => e
            logger.warn "⚠️ Failed to close pgmq client during reset: #{e.message}"
          end
          @pgmq_client = nil
        end

        # Reset registry
        if @registry
          begin
            logger.debug '🔄 Resetting handler registry...'
            # NOTE: StepHandlerResolver is a singleton, so we don't nil it
            # but we do clear any cached state if needed
            TaskerCore::Registry::StepHandlerResolver.instance
            logger.debug '✅ Handler registry reset'
          rescue StandardError => e
            logger.warn "⚠️ Failed to reset handler registry: #{e.message}"
          end
        end

        # Force garbage collection to clean up any lingering objects
        begin
          GC.start
        rescue StandardError => e
          logger.warn "⚠️ Garbage collection failed during reset: #{e.message}"
        end

        logger.info '🔄 Orchestration system reset completed'
      end

      # Bootstrap the orchestration system based on configuration mode
      def bootstrap_orchestration_system
        mode = orchestration_mode
        logger.info "🚀 Bootstrapping #{mode} mode orchestration system with pgmq architecture"

        case mode
        when 'embedded'
          bootstrap_embedded_mode
        when 'distributed'
          bootstrap_distributed_mode
        else
          raise TaskerCore::Errors::OrchestrationError,
                "Unknown orchestration mode: #{mode}. Expected 'embedded' or 'distributed'"
        end

        # Ensure core queues are created
        bootstrap_core_queues

        # Bootstrap handler registry for distributed mode
        bootstrap_distributed_handlers if mode == 'distributed'

        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now

        logger.info "✅ #{mode} mode orchestration system initialized with pgmq architecture"
      end

      # Get the base task handler (memoized)
      def base_task_handler
        @base_task_handler ||= TaskerCore::TaskHandler::Base.new
      end

      # Get pgmq client (lazy initialization)
      def pgmq_client
        @pgmq_client ||= TaskerCore::Messaging::PgmqClient.new
      end

      # Get orchestration mode from configuration
      def orchestration_mode
        return @orchestration_mode if @orchestration_mode

        config = TaskerCore::Config.instance
        @orchestration_mode = config.orchestration_config.mode

        # Default to embedded mode if not specified or in test environment
        if @orchestration_mode.nil?
          @orchestration_mode = if config.test_environment?
                                  'embedded'
                                else
                                  'distributed'
                                end
        end

        @orchestration_mode
      end

      # Get the step handler resolver (backward compatibility alias)
      # Returns nil in embedded mode since distributed handlers aren't used
      def distributed_handler_registry
        return nil if orchestration_mode == 'embedded'

        TaskerCore::Registry::StepHandlerResolver.instance
      end

      # Get the step handler resolver
      def registry
        TaskerCore::Registry::StepHandlerResolver.instance
      end

      # Check if pgmq is available
      def pgmq_available?
        pgmq_client&.respond_to?(:send_message)
      end

      # Check if embedded orchestrator is available
      def embedded_orchestrator_available?
        return false unless orchestration_mode == 'embedded'

        TaskerCore.respond_to?(:embedded_orchestrator) &&
          TaskerCore.embedded_orchestrator.respond_to?(:running?)
      end

      private

      # Bootstrap embedded mode orchestration
      def bootstrap_embedded_mode
        logger.debug '🔌 Initializing embedded orchestrator'

        # Load task templates into database before starting Rust orchestrator
        # This ensures the Rust TaskHandlerRegistry can find configurations
        bootstrap_embedded_task_templates

        # Discover all viable namespaces from the database
        viable_namespaces = discover_viable_namespaces
        logger.info "🔍 Discovered viable namespaces for orchestration: #{viable_namespaces.join(', ')}"

        # In embedded mode, start orchestration with all discovered namespaces
        TaskerCore.start_embedded_orchestration!(viable_namespaces)
        @embedded_orchestrator = TaskerCore.embedded_orchestrator
        logger.debug '✅ Embedded orchestrator started'
      end

      # Bootstrap distributed mode orchestration
      def bootstrap_distributed_mode
        logger.debug '🌐 Initializing distributed orchestration'

        # In distributed mode, pgmq must be available
        unless pgmq_available?
          raise TaskerCore::Errors::OrchestrationError,
                'PGMQ client not available for distributed orchestration. ' \
                'Ensure DATABASE_URL is set and PostgreSQL is running with pgmq extension'
        end

        logger.debug '✅ Distributed orchestration ready with pgmq client'
      end

      # Bootstrap core orchestration queues based on configuration
      def bootstrap_core_queues
        logger.debug '🗂️ Bootstrapping core orchestration queues'

        config = TaskerCore::Config.instance
        queue_config = config.orchestration_config.queues

        unless queue_config
          raise TaskerCore::Errors::ConfigurationError,
                'No orchestration.queues configuration found. Cannot bootstrap queues.'
        end

        core_queues = [
          queue_config.task_requests,
          queue_config.task_processing,
          queue_config.batch_results
        ].compact

        worker_queues = queue_config.worker_queues&.values || []
        all_queues = core_queues + worker_queues

        logger.debug "📋 Core queues to bootstrap: #{all_queues.join(', ')}"

        # Create queues using pgmq client - must be available
        unless pgmq_available?
          raise TaskerCore::Errors::OrchestrationError,
                'PGMQ client not available for queue bootstrap'
        end

        created_count = 0
        failed_queues = []

        all_queues.each do |queue_name|
          pgmq_client.create_queue(queue_name)
          logger.debug "✅ Queue created: #{queue_name}"
          created_count += 1
        rescue StandardError => e
          failed_queues << "#{queue_name}: #{e.message}"
        end

        unless failed_queues.empty?
          raise TaskerCore::Errors::OrchestrationError,
                "Failed to create required queues: #{failed_queues.join(', ')}"
        end

        logger.info "🗂️ Queue bootstrap complete: #{created_count} created, 0 failed"
      end

      # Bootstrap distributed handler registry
      def bootstrap_distributed_handlers
        logger.debug '🔧 Bootstrapping distributed handler registry'

        # Initialize distributed handler registry and bootstrap handlers
        result = TaskerCore::Registry::StepHandlerResolver.instance.bootstrap_handlers

        unless result['status'] == 'success'
          raise TaskerCore::Errors::OrchestrationError,
                "Handler registry bootstrap failed: #{result['error']}"
        end

        logger.info "✅ Handler registry bootstrapped: #{result['registered_handlers']} handlers registered"
      end

      # Bootstrap task templates for embedded mode
      # This ensures task configurations are available in the database
      # before the Rust orchestrator tries to use them
      def bootstrap_embedded_task_templates
        logger.debug '🔧 Loading task templates for embedded mode'

        # Use TaskTemplateRegistry to load existing templates from database
        # In embedded mode, templates should already be registered
        templates = TaskerCore::Registry::TaskTemplateRegistry.instance.load_task_templates_from_database

        logger.info "✅ embedded mode task template bootstrap complete: #{templates.size} templates loaded"
      end

      # Discover all viable namespaces from the database
      # Queries the tasker_task_namespaces table populated by task template loading
      # FAIL-FAST: No fallbacks - if we can't query the database, we have bigger problems
      def discover_viable_namespaces
        unless pgmq_available?
          raise TaskerCore::Errors::OrchestrationError,
                'Cannot discover namespaces: PGMQ not available. Check DATABASE_URL and PostgreSQL connection.'
        end

        begin
          namespaces = TaskerCore::Database::Models::TaskNamespace.all.pluck(:name)

          # FAIL-FAST: Empty namespaces means task templates weren't loaded properly
          if namespaces.empty?
            raise TaskerCore::Errors::OrchestrationError,
                  'No namespaces found in tasker_task_namespaces table. ' \
                  'Task templates must be loaded before starting orchestration. ' \
                  'Check bootstrap_embedded_task_templates or distributed handler registry.'
          end

          namespaces
        rescue PG::Error => e
          raise TaskerCore::Errors::OrchestrationError,
                "Failed to query tasker_task_namespaces table: #{e.message}. " \
                'Check database schema and migrations.'
        rescue StandardError => e
          raise TaskerCore::Errors::OrchestrationError,
                "Unexpected error discovering namespaces: #{e.message}"
        end
      end
    end
  end
end

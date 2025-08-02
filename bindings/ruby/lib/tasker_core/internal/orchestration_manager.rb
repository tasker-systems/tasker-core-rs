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

      attr_reader :initialized_at, :status, :logger, :base_task_handler, :registry

      def initialize
        @initialized = false
        @status = 'not_initialized'
        @initialized_at = nil
        @orchestration_mode = nil
        @base_task_handler = nil
        @pgmq_client = nil
        @embedded_orchestrator = nil
        @logger = TaskerCore::Logging::Logger.instance
        @registry = TaskerCore::Orchestration::DistributedHandlerRegistry.instance
      end

      # Get the orchestration system (pgmq-based initialization)
      def orchestration_system
        unless @initialized
          bootstrap_orchestration_system
        end

        {
          'architecture' => 'pgmq',
          'mode' => orchestration_mode,
          'queues_initialized' => queues_initialized?,
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
          embedded_orchestrator_available: embedded_orchestrator_available?,
          queues_initialized: queues_initialized?
        }

        # Add handler registry information for distributed mode
        if orchestration_mode == 'distributed' && @registry
          base_info[:handler_registry] = {
            available: true,
            stats: @registry.stats
          }
        else
          base_info[:handler_registry] = { available: false }
        end

        base_info
      end

      # Reset the orchestration system (for testing)
      def reset!
        @initialized = false
        @status = 'reset'
        @initialized_at = nil
        @orchestration_mode = nil
        @base_task_handler = nil
        @pgmq_client = nil
        @registry = nil

        # Stop embedded orchestrator if running
        if @embedded_orchestrator
          begin
            TaskerCore.stop_embedded_orchestration! if orchestration_mode == 'embedded'
          rescue StandardError => e
            @logger.warn "Failed to stop embedded orchestrator during reset: #{e.message}"
          end
          @embedded_orchestrator = nil
        end

        logger.info "🔄 Orchestration system reset"
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
        if mode == 'distributed'
          bootstrap_distributed_handlers
        end

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

        config = TaskerCore::Config.instance.effective_config
        @orchestration_mode = config.dig('orchestration', 'mode')

        # Default to embedded mode if not specified or in test environment
        if @orchestration_mode.nil?
          if config.dig('execution', 'environment') == 'test'
            @orchestration_mode = 'embedded'
          else
            @orchestration_mode = 'distributed'
          end
        end

        @orchestration_mode
      rescue StandardError => e
        logger.warn "⚠️ Failed to determine orchestration mode: #{e.message}, defaulting to distributed"
        @orchestration_mode = 'distributed'
      end

      # Check if pgmq is available
      def pgmq_available?
        pgmq_client&.respond_to?(:send_message)
      rescue StandardError => e
        logger.debug "PGMQ availability check failed: #{e.message}"
        false
      end

      # Check if embedded orchestrator is available
      def embedded_orchestrator_available?
        return false unless orchestration_mode == 'embedded'

        TaskerCore.respond_to?(:embedded_orchestrator) &&
          TaskerCore.embedded_orchestrator&.respond_to?(:running?)
      rescue StandardError => e
        logger.debug "Embedded orchestrator availability check failed: #{e.message}"
        false
      end

      # Check if core queues are initialized
      def queues_initialized?
        # TODO: Implement queue existence checking in Phase 4.5
        # For now, assume queues exist if pgmq is available
        pgmq_available?
      end

      private

      # Bootstrap embedded mode orchestration
      def bootstrap_embedded_mode
        logger.debug "🔌 Initializing embedded orchestrator"

        config = TaskerCore::Config.instance.effective_config
        auto_start = config.dig('orchestration', 'embedded_orchestrator', 'auto_start')

        if auto_start
          begin
            TaskerCore.start_embedded_orchestration!
            @embedded_orchestrator = TaskerCore.embedded_orchestrator
            logger.debug "✅ Embedded orchestrator started automatically"
          rescue StandardError => e
            logger.warn "⚠️ Failed to auto-start embedded orchestrator: #{e.message}"
            logger.warn "Call TaskerCore.start_embedded_orchestration! manually if needed"
          end
        else
          logger.debug "🔧 Embedded orchestrator available but auto_start disabled"
        end
      end

      # Bootstrap distributed mode orchestration
      def bootstrap_distributed_mode
        logger.debug "🌐 Initializing distributed orchestration"

        # In distributed mode, we check pgmq availability but don't fail if unavailable
        # Workers will register themselves and poll queues autonomously when database is available
        if pgmq_available?
          logger.debug "✅ Distributed orchestration ready with pgmq client"
        else
          logger.warn "⚠️ PGMQ client not available - distributed orchestration will be limited"
          logger.warn "Ensure DATABASE_URL is set and PostgreSQL is running with pgmq extension"
        end
      end

      # Bootstrap core orchestration queues based on configuration
      def bootstrap_core_queues
        logger.debug "🗂️ Bootstrapping core orchestration queues"

        config = TaskerCore::Config.instance.effective_config
        queue_config = config.dig('orchestration', 'queues')

        unless queue_config
          logger.warn "⚠️ No orchestration.queues configuration found, skipping queue bootstrap"
          return
        end

        core_queues = [
          queue_config['task_requests'],
          queue_config['task_processing'],
          queue_config['batch_results']
        ].compact

        worker_queues = queue_config.dig('worker_queues')&.values || []
        all_queues = core_queues + worker_queues

        logger.debug "📋 Core queues to bootstrap: #{all_queues.join(', ')}"

        # Create queues using pgmq client if available
        if pgmq_available?
          created_count = 0
          failed_count = 0

          all_queues.each do |queue_name|
            begin
              pgmq_client.create_queue(queue_name)
              logger.debug "✅ Queue created: #{queue_name}"
              created_count += 1
            rescue StandardError => e
              logger.warn "⚠️ Failed to create queue #{queue_name}: #{e.message}"
              failed_count += 1
            end
          end

          logger.info "🗂️ Queue bootstrap complete: #{created_count} created, #{failed_count} failed"
        else
          logger.warn "⚠️ PGMQ client not available, queues will be created on-demand"
          all_queues.each do |queue_name|
            logger.debug "📋 Queue planned: #{queue_name}"
          end
        end
      end

      # Bootstrap distributed handler registry
      def bootstrap_distributed_handlers
        logger.debug "🔧 Bootstrapping distributed handler registry"

        begin
          # Initialize distributed handler registry and bootstrap handlers
          result = registry.bootstrap_handlers

          if result['status'] == 'success'
            logger.info "✅ Handler registry bootstrapped: #{result['registered_handlers']} handlers registered"
          else
            logger.warn "⚠️ Handler registry bootstrap had issues: #{result['error']}"
          end

        rescue StandardError => e
          logger.warn "⚠️ Failed to bootstrap distributed handler registry: #{e.message}"
        end
      end
    end
  end
end

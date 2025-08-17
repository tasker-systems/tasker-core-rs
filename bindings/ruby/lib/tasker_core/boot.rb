# frozen_string_literal: true

# TaskerCore Boot Sequence
#
# This file handles the proper initialization order for the TaskerCore system.
# It must be called early in the load process to ensure all components are
# initialized in the correct sequence before any orchestration begins.

require 'dotenv'

module TaskerCore
  module Boot
    class << self
      attr_reader :booted, :boot_time

      # Boot the TaskerCore system with proper initialization order
      #
      # @param force_reload [Boolean] Force re-initialization even if already booted
      # @return [Hash] Boot result with status and timing information
      def boot!(force_reload: false)
        Dotenv.load
        return boot_status unless force_reload || !@booted

        start_time = Time.now
        logger.info '🚀 Starting TaskerCore boot sequence...'

        begin
          # Step 1: Environment and configuration
          load_environment_config!

          # Step 2: Run all initializers
          run_all_initializers!

          # Step 3: Database connection
          establish_database_connection!

          # Step 4: Load TaskTemplates to database (before orchestrator starts)
          load_task_templates_to_database!

          # Step 5: Initialize registries (database-backed only)
          initialize_registries!

          # Step 6: Start embedded orchestrator if in embedded mode
          start_embedded_orchestrator_if_configured!

          @booted = true
          @boot_time = Time.now - start_time

          logger.info "✅ TaskerCore boot sequence completed in #{@boot_time.round(3)}s"

          {
            success: true,
            boot_time: @boot_time,
            environment: TaskerCore::Config.instance.environment,
            embedded_mode: embedded_mode?,
            registry_results: @registry_results,
            registries_initialized: @registries_initialized || false,
            orchestrator_started: @orchestrator_started || false
          }
        rescue StandardError => e
          logger.error "💥 TaskerCore boot sequence failed: #{e.message}"
          logger.error "💥 #{e.backtrace.first(5).join("\n💥 ")}"

          {
            success: false,
            error: e.message,
            boot_time: Time.now - start_time,
            environment: TaskerCore::Config.instance&.environment
          }
        end
      end

      # Get current boot status
      #
      # @return [Hash] Current boot status
      def boot_status
        {
          booted: @booted || false,
          boot_time: @boot_time,
          environment: TaskerCore::Config.instance&.environment,
          embedded_mode: embedded_mode?,
          database_connected: database_connected?,
          registry_results: @registry_results,
          registries_initialized: @registries_initialized || false,
          orchestrator_started: @orchestrator_started || false
        }
      end

      # Check if system is booted
      #
      # @return [Boolean] true if boot sequence completed successfully
      def booted?
        @booted || false
      end

      # Ensure system is booted (boot if not already)
      #
      # @return [Hash] Boot result
      def ensure_booted!
        return boot_status if booted?

        boot!
      end

      # Step 1: Load environment configuration
      def load_environment_config!
        Dotenv.load
        logger.debug '📋 Loading environment configuration...'

        config = TaskerCore::Config.instance
        logger.info "📋 Environment: #{config.environment}"
        # Database URL is now managed internally by the Rust configuration

        true
      end

      # Step 2: Establish database connection
      def establish_database_connection!
        logger.debug '🔌 Establishing database connection...'

        # Ensure ActiveRecord connection is established
        TaskerCore::Database::Connection.instance

        raise TaskerCore::Errors::DatabaseError, 'Failed to establish database connection' unless database_connected?

        logger.info '✅ Database connection established'
        true
      end

      # Step 3: Load TaskTemplates to database (CRITICAL: before orchestrator starts)
      def load_task_templates_to_database!
        logger.debug '📚 Loading TaskTemplates to database...'

        TaskerCore::Utils::TemplateLoader.load_templates!

        true
      end

      # Step 4: Initialize registries (database-backed only)
      def initialize_registries!
        logger.debug '🔧 Initializing registries...'
        TaskerCore::Registry::TaskTemplateRegistry.instance
        TaskerCore::Registry::StepHandlerResolver.instance

        @registries_initialized = true
        logger.info '✅ Registries initialized'
        true
      end

      # Step 5: Start embedded orchestrator if configured
      def start_embedded_orchestrator_if_configured!
        unless embedded_mode?
          logger.info '📡 Embedded mode disabled, skipping orchestrator startup'
          @orchestrator_started = false
          return true
        end

        logger.debug '🎯 Starting embedded orchestrator...'

        # Get viable namespaces from database (not filesystem)
        viable_namespaces = get_viable_namespaces_from_database

        if viable_namespaces.empty?
          logger.warn '⚠️ No viable namespaces found in database, falling back to configured namespaces'
          viable_namespaces = get_fallback_namespaces_from_config

          if viable_namespaces.empty?
            logger.warn '⚠️ No fallback namespaces configured, skipping orchestrator startup'
            @orchestrator_started = false
            return true
          end
        end

        logger.info "🎯 Starting embedded orchestrator with namespaces: #{viable_namespaces.join(', ')}"

        # Start embedded orchestrator with viable namespaces
        result = TaskerCore.start_embedded_orchestration!(viable_namespaces)

        @orchestrator_started = result.is_a?(Hash) ? result['success'] : true
        logger.info '✅ Embedded orchestrator started'
        true
      end

      # load from config/initializers/*.rb within the gem installation
      def run_all_initializers!
        # Load initializers from the gem's lib directory (installed location)
        # Not from the application's config directory
        Dir[File.join(TaskerCore.gem_lib_root, 'config', 'initializers', '*.rb')].each do |file|
          require file
        end
      end

      # Check if we're in embedded mode
      def embedded_mode?
        env = TaskerCore::Config.instance.environment
        # Always embedded in test, check ENV variable for other environments
        env == 'test' || ENV['TASKER_EMBEDDED_MODE'] == 'true'
      rescue StandardError => e
        logger.warn "⚠️ Could not determine embedded mode: #{e.message}"
        # Default to true for test environments
        TaskerCore::Config.instance&.environment == 'test'
      end

      # Check if database is connected
      def database_connected?
        ActiveRecord::Base.connected? && ActiveRecord::Base.connection.active?
      rescue StandardError
        false
      end

      # Get viable namespaces from database TaskNamespace records
      def get_viable_namespaces_from_database
        TaskerCore::Database::Models::TaskNamespace.all.pluck(:name)
      rescue StandardError => e
        logger.error "💥 Could not get namespaces from database: #{e.message}"
        []
      end

      # Get fallback namespaces from configuration
      def get_fallback_namespaces_from_config
        config = TaskerCore::Config.instance

        # Try embedded_orchestrator namespaces first
        embedded_config = config.orchestration_config.embedded_orchestrator
        if embedded_config && embedded_config[:namespaces]
          logger.info "📋 Using embedded_orchestrator namespaces from config: #{embedded_config[:namespaces]}"
          return embedded_config[:namespaces]
        end

        # Fall back to pgmq default_namespaces
        pgmq_config = config.pgmq_config
        if pgmq_config&.default_namespaces
          logger.info "📋 Using pgmq default_namespaces from config: #{pgmq_config.default_namespaces}"
          return pgmq_config.default_namespaces
        end

        # Ultimate fallback
        logger.warn '⚠️ No configured namespaces found, using ultimate fallback'
        ['default']
      rescue StandardError => e
        logger.error "💥 Could not get fallback namespaces from config: #{e.message}"
        ['default']
      end

      # Get logger instance
      def logger
        TaskerCore::Logging::Logger.instance
      end
    end
  end
end

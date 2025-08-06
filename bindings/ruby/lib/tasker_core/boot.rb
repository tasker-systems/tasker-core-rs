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
        return boot_status unless force_reload || !@booted

        start_time = Time.now
        logger.info '🚀 Starting TaskerCore boot sequence...'

        begin
          # Step 1: Environment and configuration
          load_environment_config!

          # Step 2: Database connection
          establish_database_connection!

          # Step 3: Load TaskTemplates to database (before orchestrator starts)
          load_task_templates_to_database!

          # Step 4: Initialize registries (database-backed only)
          initialize_registries!

          # Step 5: Start embedded orchestrator if in embedded mode
          start_embedded_orchestrator_if_configured!

          @booted = true
          @boot_time = Time.now - start_time

          logger.info "✅ TaskerCore boot sequence completed in #{@boot_time.round(3)}s"

          {
            success: true,
            boot_time: @boot_time,
            environment: TaskerCore::Config.instance.environment,
            embedded_mode: embedded_mode?,
            task_templates_loaded: @task_templates_loaded || 0,
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
          task_templates_loaded: @task_templates_loaded || 0,
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
        logger.info "📋 Database URL: #{config.database_url[0..50]}..." if config.database_url

        true
      end

      # Step 2: Establish database connection
      def establish_database_connection!
        logger.debug '🔌 Establishing database connection...'

        # Ensure ActiveRecord connection is established
        TaskerCore::Database::Connection.instance

        unless database_connected?
          raise TaskerCore::Errors::DatabaseError, 'Failed to establish database connection'
        end

        logger.info '✅ Database connection established'
        true
      end

      # Step 3: Load TaskTemplates to database (CRITICAL: before orchestrator starts)
      def load_task_templates_to_database!
        logger.debug '📚 Loading TaskTemplates to database...'

        # Get search paths from configuration (environment-appropriate)
        search_patterns = TaskerCore::Config.instance.task_template_search_paths
        logger.debug "📁 TaskTemplate search patterns from config: #{search_patterns}"

        yaml_files = []
        search_patterns.each do |pattern|
          # Expand the pattern to handle glob matching
          found_files = Dir.glob(pattern)
          yaml_files.concat(found_files)
          logger.debug "📁 Pattern #{pattern}: found #{found_files.length} files"
        end

        if yaml_files.empty?
          logger.warn '⚠️ No TaskTemplate YAML files found'
          @task_templates_loaded = 0
          return true
        end

        # Load templates using database-backed registry
        registry = TaskerCore::Orchestration::HandlerRegistry.instance
        loaded_count = 0
        failed_count = 0

        yaml_files.each do |file_path|
          begin
            result = registry.register_task_template_from_yaml(file_path)
            if result[:success]
              loaded_count += 1
            else
              failed_count += 1
              logger.warn "⚠️ Failed to load #{file_path}: #{result[:error]}"
            end
          rescue StandardError => e
            failed_count += 1
            logger.error "💥 Error loading #{file_path}: #{e.message}"
          end
        end

        @task_templates_loaded = loaded_count
        logger.info "📚 TaskTemplates loaded: #{loaded_count} successful, #{failed_count} failed"

        # Fail fast if no templates loaded
        if loaded_count == 0 && yaml_files.length > 0
          raise TaskerCore::Errors::ConfigurationError, 'No TaskTemplates could be loaded to database'
        end

        true
      end

      # Step 4: Initialize registries (database-backed only)
      def initialize_registries!
        logger.debug '🔧 Initializing registries...'

        # Initialize step handler registry (database-backed)
        TaskerCore::Registry.step_handler_registry

        # Initialize distributed handler registry (database-backed)
        TaskerCore::Orchestration::HandlerRegistry.instance

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
        config = TaskerCore::Config.instance.effective_config

        # Try embedded_orchestrator namespaces first
        embedded_config = config.dig('orchestration', 'embedded_orchestrator')
        if embedded_config && embedded_config['namespaces']
          logger.info "📋 Using embedded_orchestrator namespaces from config: #{embedded_config['namespaces']}"
          return embedded_config['namespaces']
        end

        # Fall back to pgmq default_namespaces
        pgmq_config = config['pgmq']
        if pgmq_config && pgmq_config['default_namespaces']
          logger.info "📋 Using pgmq default_namespaces from config: #{pgmq_config['default_namespaces']}"
          return pgmq_config['default_namespaces']
        end

        # Ultimate fallback
        logger.warn "⚠️ No configured namespaces found, using ultimate fallback"
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

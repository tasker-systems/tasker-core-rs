# frozen_string_literal: true

require 'yaml'
require 'json'
require 'logger'
require 'singleton'
require 'digest'
require_relative 'internal/orchestration_manager'

module TaskerCore
  class TaskHandler
    # Ruby wrapper that extends the Rust BaseTaskHandler with Ruby-specific functionality.
    # This inherits from the actual Rust handler foundation where orchestration flows,
    # data access, and transaction management are handled at the Rust layer.
    #
    # Architecture:
    # - Rust BaseTaskHandler: Core orchestration, state management, transactions
    # - Ruby wrapper: Business logic hooks, Ruby-specific tooling, validation
    #
    # Developer Interface:
    # - handle(task) - Main task processing (Ruby implementation)
    # - handle_one_step(task, sequence, step) - Step processing delegation
    #
    # Registry Integration:
    # - Task handlers register themselves with TaskHandlerRegistry
    # - Step handlers are discovered through task configuration
    # Ruby wrapper that delegates to Rust BaseTaskHandler for orchestration integration
    # Note: We use composition instead of inheritance due to Magnus FFI limitations
    class Base

      attr_reader :config, :logger, :rust_integration, :task_config

      def initialize(config: {}, rust_integration: nil, task_config_path: nil, task_config: nil)
        @config = config || {}
        @logger = TaskerCore::Logging::Logger.instance
        @rust_integration = rust_integration || default_rust_integration
        @task_config_path = task_config_path
        @task_config = task_config || (task_config_path ? load_task_config_from_path(task_config_path) : {})

        # 🎯 CRITICAL FIX: Get BaseTaskHandler from OrchestrationManager singleton
        # This prevents the "37 calls to BaseTaskHandler.new" problem by using memoized handler
        if @task_config && !@task_config.empty?
          begin
            # Convert to JSON for BaseTaskHandler
            task_config_json = @task_config.to_json
            task_config_hash = JSON.parse(task_config_json)

            # Get BaseTaskHandler from OrchestrationManager (memoized)
            orchestration_manager = TaskerCore::OrchestrationManager.instance
            @rust_handler = orchestration_manager.get_base_task_handler(task_config_hash)

            if @rust_handler
              logger.info '♻️  Got stateless BaseTaskHandler from OrchestrationManager (config passed per-call)'
            else
              logger.error 'Failed to get BaseTaskHandler from OrchestrationManager'
            end
          rescue StandardError => e
            logger.error "Failed to get BaseTaskHandler: #{e.message}"
            @rust_handler = nil
          end
        else
          logger.warn 'No task config provided - Rust integration disabled'
          @rust_handler = nil
        end

        # Auto-register this handler with the orchestration system
        register_with_orchestration_system if @task_config && !@task_config.empty?
      end

      # ========================================================================
      # ORCHESTRATION SYSTEM INTEGRATION
      # ========================================================================

      # Register this task handler with the Rust orchestration system
      # This allows the TaskHandlerRegistry to discover and call this handler
      def register_with_orchestration_system
        return unless @task_config

        handler_data = {
          'namespace' => @task_config['namespace_name'] || 'default',
          'name' => @task_config['name'],
          'version' => @task_config['version'] || '1.0.0',
          'handler_class' => self.class.name,
          'config_schema' => @task_config
        }

        result = TaskerCore::OrchestrationManager.instance.register_handler(handler_data)

        if result['status'] == 'registered'
          logger.info "Registered task handler: #{self.class.name} for task: #{handler_data['name']}"
        else
          logger.error "Failed to register task handler: #{result['error']}"
        end

        result
      rescue StandardError => e
        logger.error "Error registering task handler: #{e.message}"
        { 'status' => 'error', 'error' => e.message }
      end

      # Check if this handler is registered with the orchestration system
      def registered?
        return false unless @task_config

        handler_key = {
          'namespace' => @task_config['namespace_name'] || 'default',
          'name' => @task_config['name'],
          'version' => @task_config['version'] || '1.0.0'
        }

        # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct FFI
        result = TaskerCore::OrchestrationManager.instance.handler_exists?(handler_key)
        result['exists'] == true
      rescue StandardError => e
        logger.error "Error checking handler registration: #{e.message}"
        false
      end

      # Get all registered task handlers
      def self.list_registered_handlers
        # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct FFI
        TaskerCore::OrchestrationManager.instance.list_handlers(nil)
      rescue StandardError => e
        { 'handlers' => [], 'count' => 0, 'error' => e.message }
      end

      # ========================================================================
      # MAIN TASK EXECUTION INTERFACE (implemented by subclasses)
      # ========================================================================

      # Main task processing method - Rails engine signature: handle(task)
      # We delegate to the Rust layer for orchestration via composition
      # @param task [Tasker::Task] Task model instance with context data
      # @return [Object] Task results (Hash, Array, String, etc.)
      def handle(task)
        unless @rust_handler
          raise NotImplementedError, 'handle method must be implemented by subclass (Rust handler not available)'
        end

        @rust_handler.handle(task.task_id)
      end

      # Delegate other BaseTaskHandler methods to maintain interface compatibility

      def initialize_task(task_request)
        raise NotImplementedError, 'Rust handler not available' unless @rust_handler

        @rust_handler.initialize_task(task_request)
      end

      def capabilities
        # Combine Rust handler capabilities with Ruby-specific capabilities
        rust_capabilities = if @rust_handler
                              @rust_handler.capabilities
                            else
                              ['handle'] # Basic capabilities when Rust handler is not available
                            end

        # Add Ruby-specific capabilities that are implemented in this wrapper
        ruby_capabilities = %w[handle_one_step get_step_handler find_step_template]

        (rust_capabilities + ruby_capabilities).uniq
      end

      def supports_capability?(capability)
        # Check both Rust capabilities and Ruby-specific capabilities
        capabilities.include?(capability)
      end

      # Step processing delegation - Rails engine signature: handle_one_step(task, sequence, step)
      # This delegates to the appropriate step handler based on the task configuration
      # @param task [Tasker::Task] Task model instance with context data
      # @param sequence [Tasker::Types::StepSequence] Step sequence for navigation
      # @param step [Tasker::WorkflowStep] Current step being processed
      # @return [Object] Step results (Hash, Array, String, etc.)
      def handle_one_step(task, sequence, step)
        step_handler = get_step_handler(step)
        return { error: 'Step handler not found' } unless step_handler

        step_handler.process(task, sequence, step)
      end

      # Get step handler for a given step
      def get_step_handler(step)
        step_name = step.respond_to?(:name) ? step.name : step['name']
        step_template = find_step_template(step_name)
        return nil unless step_template

        handler_class_name = step_template['handler_class']
        handler_class = Object.const_get(handler_class_name)
        handler_config = step_template['handler_config'] || {}

        handler_class.new(config: handler_config, logger: @logger)
      rescue StandardError => e
        logger.error "Error creating step handler: #{e.message}"
        nil
      end

      # Find step template in task configuration
      def find_step_template(step_name)
        return nil unless @task_config && @task_config['step_templates']

        @task_config['step_templates'].find { |template| template['name'] == step_name }
      end

      # ========================================================================
      # CONFIGURATION AND METADATA
      # ========================================================================

      # Validate task handler configuration
      # @param config_hash [Hash] Configuration to validate
      # @return [Boolean] Whether configuration is valid
      def validate_config(config_hash)
        # Basic validation - subclasses can override for specific requirements
        config_hash.is_a?(Hash)
      end

      # Get handler name for registration and logging
      # @return [String] Handler name from task configuration
      def handler_name
        @task_config&.dig('name')
      end

      # Get handler metadata for monitoring and introspection
      # @return [Hash] Handler metadata
      def metadata
        {
          handler_name: handler_name,
          handler_class: self.class.name,
          task_config: @task_config,
          version: @task_config&.dig('version') || '1.0.0',
          namespace: @task_config&.dig('namespace_name') || 'default',
          capabilities: capabilities,
          step_templates: @task_config&.dig('step_templates') || [],
          ruby_version: RUBY_VERSION,
          created_at: Time.now.iso8601
        }
      end

      # Get configuration schema for validation
      # @return [Hash] JSON schema describing expected configuration
      def config_schema
        # Return handler configuration schema, not task schema
        {
          type: 'object',
          properties: {
            timeout: { type: 'integer', minimum: 1, default: 300 },
            retries: { type: 'integer', minimum: 0, default: 3 },
            log_level: { type: 'string', enum: %w[debug info warn error], default: 'info' }
          },
          additionalProperties: true
        }
      end

      # ========================================================================
      # INTERNAL PROCESSING METHODS
      # ========================================================================

      private

      def load_task_config_from_path(path)
        return {} unless path && File.exist?(path)

        YAML.load_file(path)
      rescue StandardError => e
        puts "Error loading task configuration: #{e.message}"
        {}
      end

      # Default Rust integration
      def default_rust_integration
        TaskerCore::RailsIntegration.new
      end
    end
  end
end

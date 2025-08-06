# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Registry
    # Database-backed step handler registry that uses TaskTemplate configurations
    # to properly resolve step handlers instead of guessing class names.
    #
    # This implements the proper pattern:
    # WorkflowStep.named_step_id → Task.named_task_id → NamedTask.configuration (TaskTemplate) → StepTemplate → handler_class
    #
    # Key features:
    # - Database-backed configuration lookup
    # - In-memory caching with TTL for performance
    # - Proper TaskTemplate/StepTemplate deserialization
    # - Handler class availability checking
    # - Thread-safe concurrent access
    class StepHandlerRegistry
      # Cache TTL for named_task configurations (5 minutes default)
      DEFAULT_CACHE_TTL = 300

      # Cache structure for task template configurations
      class CacheEntry < Dry::Struct
        attribute :task_template, TaskerCore::Types::TaskTemplate
        attribute :cached_at, TaskerCore::Types::Types::Strict::Time
        attribute :ttl, TaskerCore::Types::Types::Strict::Integer
      end

      # Resolved step handler information
      class ResolvedStepHandler < Dry::Struct
        attribute :handler_class_name, TaskerCore::Types::Types::Strict::String
        attribute :handler_class, TaskerCore::Types::Types::Strict::Class
        attribute :handler_config, TaskerCore::Types::Types::Strict::Hash
        attribute :step_template, TaskerCore::Types::StepTemplate
        attribute :task_template, TaskerCore::Types::TaskTemplate
      end

      attr_reader :cache_ttl

      def initialize(cache_ttl: DEFAULT_CACHE_TTL)
        @cache_ttl = cache_ttl
        @logger = TaskerCore::Logging::Logger.instance
        @config_cache = Concurrent::Map.new
        # Don't use singleton connection - create a fresh connection for thread safety
        @db_connection = nil
      end

      # Resolve step handler from step_message using database-backed configuration
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message with execution context
      # @return [ResolvedStepHandler, nil] Resolved handler info or nil if not found
      def resolve_step_handler(step_message)
        # Extract step context from execution context
        execution_context = step_message.execution_context
        return nil unless execution_context&.step&.dig(:workflow_step_id)

        workflow_step_id = execution_context.step[:workflow_step_id]
        step_name = step_message.step_name

        @logger.debug("🔍 STEP_REGISTRY: Resolving handler - workflow_step_id: #{workflow_step_id}, step_name: #{step_name}")

        # Get workflow step to find named_step_id and task_id
        workflow_step = find_workflow_step(workflow_step_id)
        return nil unless workflow_step

        # Get task template configuration (with caching)
        task_template = get_task_template_for_task(workflow_step.task_id)
        return nil unless task_template

        # Find step template by name
        step_template = find_step_template(task_template, step_name)
        return nil unless step_template

        # Check if handler class exists and is available
        handler_class = resolve_handler_class(step_template.handler_class)
        return nil unless handler_class

        @logger.info("✅ STEP_REGISTRY: Handler resolved - step: #{step_name}, class: #{step_template.handler_class}")

        ResolvedStepHandler.new(
          handler_class_name: step_template.handler_class,
          handler_class: handler_class,
          handler_config: step_template.handler_config || {},
          step_template: step_template,
          task_template: task_template
        )
      rescue StandardError => e
        @logger.error("💥 STEP_REGISTRY: Error resolving handler for step #{step_message.step_name}: #{e.message}")
        @logger.error("💥 STEP_REGISTRY: #{e.backtrace.first(3).join("\n")}")
        nil
      end

      # Check if we can handle a step (fast check using database configuration)
      #
      # @param step_message [TaskerCore::Types::StepMessage] Step message to check
      # @return [Boolean] true if handler is available
      def can_handle_step?(step_message)
        resolved = resolve_step_handler(step_message)
        !resolved.nil?
      end

      # Create handler instance with proper configuration
      #
      # @param resolved_handler [ResolvedStepHandler] Resolved handler info
      # @return [Object] Handler instance ready for execution
      def create_handler_instance(resolved_handler)
        # Initialize handler with configuration if the constructor accepts it
        handler_class = resolved_handler.handler_class
        handler_config = resolved_handler.handler_config

        if handler_class.instance_method(:initialize).arity.positive?
          handler_class.new(handler_config)
        else
          handler_class.new
        end
      rescue StandardError => e
        @logger.error("💥 STEP_REGISTRY: Error creating handler instance for #{resolved_handler.to_h}: #{e.message}, #{e.backtrace.join('\n')}")
        raise TaskerCore::Error, "Handler instantiation failed: #{e.message}"
      end

      # Clear cache (useful for testing or configuration reloads)
      def clear_cache!
        @config_cache.clear
        @logger.info('🧹 STEP_REGISTRY: Configuration cache cleared')
      end

      # Get cache statistics for monitoring
      #
      # @return [Hash] Cache statistics
      def cache_stats
        total_entries = @config_cache.size
        expired_entries = @config_cache.values.count { |entry| cache_expired?(entry) }

        {
          total_entries: total_entries,
          active_entries: total_entries - expired_entries,
          expired_entries: expired_entries,
          cache_ttl: @cache_ttl
        }
      end

      private

      # Find workflow step by ID using ActiveRecord
      def find_workflow_step(workflow_step_id)
        ActiveRecord::Base.with_connection do
          TaskerCore::Database::Models::WorkflowStep.find_by(workflow_step_id: workflow_step_id)
        end
      rescue ActiveRecord::ConnectionTimeoutError => e
        @logger.error("💥 STEP_REGISTRY: Connection timeout finding workflow step #{workflow_step_id}: #{e.message}")
        nil
      rescue StandardError => e
        @logger.error("💥 STEP_REGISTRY: Error finding workflow step #{workflow_step_id}: #{e.message}")
        nil
      end

      # Get task template configuration for a task (with caching) using ActiveRecord
      def get_task_template_for_task(task_id)
        # Use ActiveRecord with preloaded associations
        task = ActiveRecord::Base.with_connection do
          TaskerCore::Database::Models::Task.includes(named_task: :task_namespace).find_by(task_id: task_id)
        end
        return nil unless task

        named_task = task.named_task
        return nil unless named_task

        # Check cache first
        cache_key = "named_task_#{named_task.named_task_id}"
        cached_entry = @config_cache[cache_key]

        if cached_entry && !cache_expired?(cached_entry)
          @logger.debug("📋 STEP_REGISTRY: Using cached task template for named_task_id: #{named_task.named_task_id}")
          return cached_entry.task_template
        end

        # Get configuration (already parsed from JSON column)
        config_hash = named_task.configuration
        return nil unless config_hash
        # Convert string keys to symbols for dry-struct
        symbolized_config = config_hash.transform_keys(&:to_sym)

        # Transform step_templates to proper structure if present
        if symbolized_config[:step_templates].is_a?(Array)
          symbolized_config[:step_templates] = symbolized_config[:step_templates].map do |step|
            step.is_a?(Hash) ? step.transform_keys(&:to_sym) : step
          end
        end

        # Transform environments to proper structure if present (keys should remain strings)
        if symbolized_config[:environments].is_a?(Hash)
          symbolized_config[:environments] = symbolized_config[:environments].transform_values do |env|
            if env.is_a?(Hash) && env['step_overrides'].is_a?(Hash)
              {
                step_overrides: env['step_overrides'].transform_values do |override|
                  override.is_a?(Hash) ? override.transform_keys(&:to_sym) : override
                end
              }
            else
              env
            end
          end
        end

        task_template = TaskerCore::Types::TaskTemplate.new(symbolized_config)

        # Cache the parsed template
        cache_entry = CacheEntry.new(
          task_template: task_template,
          cached_at: Time.now,
          ttl: @cache_ttl
        )
        @config_cache[cache_key] = cache_entry

        @logger.debug("💾 STEP_REGISTRY: Cached task template for named_task_id: #{named_task.named_task_id}")
        task_template
      rescue ActiveRecord::ConnectionTimeoutError => e
        @logger.error("💥 STEP_REGISTRY: Connection timeout getting task template for task #{task_id}: #{e.message}")
        nil
      rescue StandardError => e
        @logger.error("💥 STEP_REGISTRY: Error getting task template for task #{task_id}: #{e.message}")
        nil
      end

      # Find step template by name within task template
      def find_step_template(task_template, step_name)
        step_template = task_template.step_templates.find { |st| st.name == step_name }

        unless step_template
          @logger.warn("⚠️ STEP_REGISTRY: Step template '#{step_name}' not found in task template")
          return nil
        end

        step_template
      end

      # Resolve handler class from class name string
      def resolve_handler_class(handler_class_name)
        # Try to constantize the handler class name
        handler_class = Object.const_get(handler_class_name)

        # Verify it's a class and has the required interface
        unless handler_class.is_a?(Class)
          @logger.warn("⚠️ STEP_REGISTRY: Handler '#{handler_class_name}' is not a class")
          return nil
        end

        # Check if it has a call method (duck typing check)
        unless handler_class.instance_methods.include?(:call)
          @logger.warn("⚠️ STEP_REGISTRY: Handler '#{handler_class_name}' does not implement #call method")
          return nil
        end

        handler_class
      rescue NameError => e
        @logger.debug("🔍 STEP_REGISTRY: Handler class '#{handler_class_name}' not available: #{e.message}")
        nil
      rescue StandardError => e
        @logger.error("💥 STEP_REGISTRY: Error resolving handler class '#{handler_class_name}': #{e.message}")
        nil
      end

      # Check if cache entry is expired
      def cache_expired?(cache_entry)
        Time.now - cache_entry.cached_at > cache_entry.ttl
      end

    end
  end
end

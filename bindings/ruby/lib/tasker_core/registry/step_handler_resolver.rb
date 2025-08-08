# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'
require 'singleton'

module TaskerCore
  module Registry
    # Step Handler Resolver - Resolves step handlers from templates and manages handler callables
    #
    # Responsibilities:
    # - Resolve step handlers from TaskTemplate and ActiveRecord step
    # - Manage handler callable registration (Procs, Lambdas, classes)
    # - Identify if handler classes exist in memory
    # - Create handler instances with proper configuration
    # - Bootstrap handler discovery and registration
    #
    # Does NOT handle:
    # - Database operations for task templates (TaskTemplateRegistry handles this)
    # - Task template storage or retrieval
    class StepHandlerResolver
      include Singleton

      # Resolved step handler information
      class ResolvedStepHandler < Dry::Struct
        attribute :handler_class_name, TaskerCore::Types::Types::Strict::String
        attribute :handler_class, TaskerCore::Types::Types::Strict::Class
        attribute :handler_config, TaskerCore::Types::Types::Strict::Hash
        attribute :step_template, TaskerCore::Types::Types::Any
        attribute :task_template, TaskerCore::Types::Types::Any
      end

      attr_reader :logger, :stats

      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        @stats = {}
        logger.info 'StepHandlerResolver initialized for handler resolution and callable management'
      end

      # Resolve step handler from task and step using TaskTemplateRegistry
      #
      # @param task [TaskerCore::Database::Models::Task] Task ActiveRecord model
      # @param step [TaskerCore::Database::Models::WorkflowStep] Step ActiveRecord model
      # @return [ResolvedStepHandler, nil] Resolved handler info or nil if not found
      def resolve_step_handler(task, step)
        # Get task template from TaskTemplateRegistry
        task_template = TaskTemplateRegistry.instance.get_task_template(task)
        unless task_template
          @logger.warn("⚠️ STEP_RESOLVER: No task template found for task #{task.task_id}")
          return nil
        end

        # Find step template by step name
        step_template = find_step_template(task_template, step.name)
        unless step_template
          @logger.warn("⚠️ STEP_RESOLVER: Step template '#{step}' not found in task template")
          return nil
        end

        # Resolve handler class
        handler_class = resolve_handler_class(step_template.handler_class)
        unless handler_class
          @logger.warn("⚠️ STEP_RESOLVER: Handler class '#{step_template.handler_class}' not available")
          return nil
        end

        @logger.info("✅ STEP_RESOLVER: Handler resolved - step: #{step.named_step_id}, class: #{step_template.handler_class}")

        ResolvedStepHandler.new(
          handler_class_name: step_template.handler_class,
          handler_class: handler_class,
          handler_config: step_template.handler_config || {},
          step_template: step_template,
          task_template: task_template
        )
      rescue StandardError => e
        @logger.error("💥 STEP_RESOLVER: Error resolving handler for step #{step.named_step_id}: #{e.message}")
        @logger.error("💥 STEP_RESOLVER: #{e.backtrace.first(3).join("\n")}")
        nil
      end

      # Check if we can handle a step (fast check)
      #
      # @param task [TaskerCore::Database::Models::Task] Task ActiveRecord model
      # @param step [TaskerCore::Database::Models::WorkflowStep] Step ActiveRecord model
      # @return [Boolean] true if handler is available
      def can_handle_step?(task, step)
        resolved = resolve_step_handler(task, step)
        !resolved.nil?
      end

      # Get callable for step execution using priority resolution order
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] Callable object or nil if none found
      def get_callable_for_class(handler_class)
        handler_key = handler_class.to_s
        begin
          klass = handler_key.constantize
          return klass if klass.respond_to?(:call)
        rescue NameError
          # Class doesn't exist, continue to next priority
        end
        # Priority 3: Instance with .call method
        instance = get_handler_instance(handler_key)
        return instance if instance.respond_to?(:call)

        nil
      end

      # Create handler instance with proper configuration
      #
      # @param resolved_handler [ResolvedStepHandler] Resolved handler info
      # @return [Object] Handler instance ready for execution
      def create_handler_instance(resolved_handler)
        handler_class = resolved_handler.handler_class
        handler_config = resolved_handler.handler_config

        # Check for registered callable first (Procs, Lambdas, etc.)
        callable = get_callable_for_class(resolved_handler.handler_class_name)
        return callable if callable

        # Otherwise create instance from class
        # Check if constructor accepts parameters (positive arity or negative arity with optional params)
        arity = handler_class.instance_method(:initialize).arity
        if arity.positive? || arity.negative?
          handler_class.new(handler_config)
        else
          handler_class.new
        end
      rescue StandardError => e
        @logger.error("💥 STEP_RESOLVER: Error creating handler instance for #{resolved_handler.handler_class_name}: #{e.message}")
        raise TaskerCore::Error, "Handler instantiation failed: #{e.message}"
      end

      # Get handler instance using traditional resolution
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] Handler instance or nil if not found
      def get_handler_instance(handler_class)
        klass = handler_class.to_s.constantize
        klass.new
      rescue NameError => e
        logger.debug "🔍 STEP_RESOLVER: Handler class '#{handler_class}' not available: #{e.message}"
        nil
      rescue StandardError => e
        logger.error "💥 STEP_RESOLVER: Error instantiating #{handler_class}: #{e.message}"
        nil
      end

      # Bootstrap handler discovery and registration
      # @return [Hash] Bootstrap operation result
      def bootstrap_handlers
        logger.info '🔧 Bootstrapping step handler resolver'

        registered_count = 0
        failed_count = 0

        # Discover handler classes from task templates
        discoverable_handlers = discover_handler_classes

        discoverable_handlers.each do |handler_class|
          register_class(handler_class, handler_class.constantize)
          registered_count += 1
          logger.debug "✅ Handler registered: #{handler_class}"
        rescue NameError
          logger.debug "⚠️ Handler class not found: #{handler_class}"
          failed_count += 1
        rescue StandardError => e
          logger.warn "❌ Failed to register handler #{handler_class}: #{e.message}"
          failed_count += 1
        end

        @stats = {
          'status' => 'success',
          'registered_handlers' => registered_count,
          'failed_handlers' => failed_count,
          'bootstrapped_at' => Time.now.utc.iso8601
        }
      rescue StandardError => e
        logger.error "❌ Handler bootstrap failed: #{e.message}"
        @stats = {
          'status' => 'error',
          'error' => e.message,
          'bootstrapped_at' => Time.now.utc.iso8601
        }
      end

      private

      # Find step template by name within task template
      # @param task_template [TaskerCore::Types::TaskTemplate] Task template
      # @param step_name [String] Step name to find
      # @return [TaskerCore::Types::StepTemplate, nil] Step template or nil if not found
      def find_step_template(task_template, step_name)
        task_template.step_templates.find { |st| st.name == step_name }
      end

      # Resolve handler class from class name string
      # @param handler_class_name [String] Handler class name
      # @return [Class, nil] Handler class or nil if not found
      def resolve_handler_class(handler_class_name)
        handler_class = Object.const_get(handler_class_name)

        unless handler_class.is_a?(Class)
          @logger.warn("⚠️ STEP_RESOLVER: Handler '#{handler_class_name}' is not a class")
          return nil
        end

        # Check if it has a call method (duck typing check)
        unless handler_class.instance_methods.include?(:call)
          @logger.warn("⚠️ STEP_RESOLVER: Handler '#{handler_class_name}' does not implement #call method")
          return nil
        end

        handler_class
      rescue NameError => e
        @logger.debug("🔍 STEP_RESOLVER: Handler class '#{handler_class_name}' not available: #{e.message}")
        nil
      rescue StandardError => e
        @logger.error("💥 STEP_RESOLVER: Error resolving handler class '#{handler_class_name}': #{e.message}")
        nil
      end

      # Discover handler classes from all registered task templates
      # @return [Array<String>] List of unique handler class names
      def discover_handler_classes
        handlers = []

        begin
          # Get all task templates from TaskTemplateRegistry
          task_templates = TaskTemplateRegistry.instance.load_task_templates_from_database

          task_templates.each do |template|
            # Add task-level handler
            handlers << template.task_handler_class if template.task_handler_class

            # Add step-level handlers
            template.step_templates.each do |step_template|
              handlers << step_template.handler_class if step_template.handler_class
            end
          end

          # Only include handlers that actually exist in the current environment
          handlers.uniq.select do |handler_class|
            handler_class.constantize
            true
          rescue NameError
            false
          end
        rescue StandardError => e
          @logger.error("💥 STEP_RESOLVER: Error discovering handler classes: #{e.message}")
          []
        end
      end
    end
  end
end

# frozen_string_literal: true

require 'yaml'
require 'json'
require 'logger'
require 'singleton'
require 'digest'
require_relative '../internal/orchestration_manager'
require_relative 'results'

module TaskerCore
  module TaskHandler
    # Results from Rust FFI are auto-registered here by Magnus:
    # - TaskerCore::TaskHandler::InitializeResult
    # - TaskerCore::TaskHandler::HandleResult
    class Base
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

      attr_reader :config, :logger, :rust_integration, :task_config, :step_handlers

      def initialize(config: {}, task_config_path: nil, task_config: nil)
        @config = config || {}
        @logger = TaskerCore::Logging::Logger.instance
        @task_config_path = task_config_path
        @task_config = task_config || (task_config_path ? load_task_config_from_path(task_config_path) : {})

        # 🚀 PHASE 1: Pre-instantiate step handlers for Ruby-centric architecture
        @step_handlers = register_step_handlers(@task_config['step_templates'] || [])
        logger.info "🔥 Pre-instantiated #{@step_handlers.size} step handlers for O(1) lookup" if @step_handlers.any?

        # 🎯 CRITICAL FIX: Get BaseTaskHandler from OrchestrationManager singleton
        # This prevents the "37 calls to BaseTaskHandler.new" problem by using memoized handler
        if @task_config && !@task_config.empty?
          begin
            # Convert to JSON for BaseTaskHandler
            task_config_json = @task_config.to_json
            task_config_hash = JSON.parse(task_config_json)

            # Get BaseTaskHandler from OrchestrationManager (memoized)
            orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
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

        # Register with Rust orchestration system (existing)
        result = TaskerCore::Internal::OrchestrationManager.instance.register_handler(handler_data)

        # 🚀 NEW: Register with Ruby TaskHandler registry for direct step execution
        ruby_result = TaskerCore::Internal::OrchestrationManager.instance.register_ruby_task_handler(
          handler_data['namespace'],
          handler_data['name'],
          handler_data['version'],
          self
        )

        if result == true && ruby_result
          logger.info "✅ Registered task handler: #{self.class.name} for task: #{handler_data['name']} (Rust + Ruby registries)"
          { 'status' => 'registered', 'handler' => self.class.name, 'ruby_step_handlers' => @step_handlers.size }
        else
          logger.error "❌ Failed to register task handler (Rust: #{result}, Ruby: #{ruby_result})"
          { 'status' => 'error', 'error' => 'Registration failed' }
        end
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
          'version' => @task_config['version'] || '0.1.0'
        }

        # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct FFI
        result = TaskerCore::Internal::OrchestrationManager.instance.handler_exists?(handler_key)
        result['exists'] == true
      rescue StandardError => e
        logger.error "Error checking handler registration: #{e.message}"
        false
      end

      # Get all registered task handlers
      def self.list_registered_handlers
        # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct FFI
        TaskerCore::Internal::OrchestrationManager.instance.list_handlers(nil)
      rescue StandardError => e
        { 'handlers' => [], 'count' => 0, 'error' => e.message }
      end

      # Main task processing method - Rails engine signature: handle(task_id)
      # We delegate to the Rust layer for orchestration via composition
      # @param task_id [Integer] Task ID to process
      # @return [Object] Task results (Hash, Array, String, etc.)
      def handle(task_id)
        unless @rust_handler
          raise NotImplementedError, 'handle method must be implemented by subclass (Rust handler not available)'
        end

        raise TaskerCore::ValidationError, 'task_id is required' unless task_id
        raise TaskerCore::ValidationError, 'task_id must be an integer' unless task_id.is_a?(Integer)

        # Rust FFI now returns a hash, convert it back to HandleResult object
        result_hash = @rust_handler.handle(task_id)

        # Convert hash back to HandleResult for backward compatibility
        TaskerCore::TaskHandler::HandleResult.new(
          task_id: result_hash['task_id'],
          status: result_hash['status'],
          completed_steps: result_hash['steps_executed'],
          error_message: result_hash['error']
        )
      end

      # Execute a single workflow step by ID with comprehensive dependency validation and debugging
      #
      # This method provides granular step-by-step testing and debugging by:
      # - Loading the workflow step and validating all dependencies are completed
      # - Executing the step handler with full context from dependency results
      # - Tracking state transitions and execution timing
      # - Providing detailed error information and context
      #
      # @param step_id [Integer] The workflow_step_id to execute
      # @return [StepHandleResult] Comprehensive result with execution details, dependency status, and context
      #
      # @example Basic usage
      #   result = handler.handle_one_step(step_id)
      #   if result.success?
      #     puts "Step completed: #{result.result_data}"
      #   elsif result.dependencies_not_met?
      #     puts "Missing dependencies: #{result.missing_dependencies.join(', ')}"
      #   else
      #     puts "Step failed: #{result.error_message}"
      #   end
      #
      # @example Debugging workflow issues
      #   result = handler.handle_one_step(step_id)
      #   puts "Step state transition: #{result.step_state_before} -> #{result.step_state_after}"
      #   puts "Dependencies available: #{result.dependency_results.keys}"
      #   puts "Task context: #{result.task_context}"
      #   puts "Execution time: #{result.execution_time_ms}ms"
      def handle_one_step(step_id)
        unless @rust_handler
          raise NotImplementedError, 'handle_one_step method requires Rust handler (not available)'
        end

        raise TaskerCore::ValidationError, 'step_id is required' unless step_id
        raise TaskerCore::ValidationError, 'step_id must be an integer' unless step_id.is_a?(Integer)

        # Rust FFI returns a hash, convert it back to StepHandleResult object
        result_hash = @rust_handler.handle_one_step(step_id)

        # Convert hash back to StepHandleResult for rich debugging interface
        TaskerCore::TaskHandler::StepHandleResult.new(
          step_id: result_hash['step_id'],
          task_id: result_hash['task_id'],
          step_name: result_hash['step_name'],
          status: result_hash['status'],
          execution_time_ms: result_hash['execution_time_ms'],
          result_data: result_hash['result_data'],
          error_message: result_hash['error_message'],
          retry_count: result_hash['retry_count'],
          handler_class: result_hash['handler_class'],
          dependencies_met: result_hash['dependencies_met'],
          missing_dependencies: result_hash['missing_dependencies'] || [],
          dependency_results: result_hash['dependency_results'] || {},
          step_state_before: result_hash['step_state_before'],
          step_state_after: result_hash['step_state_after'],
          task_context: result_hash['task_context'] || {}
        )
      end

      # Delegate other BaseTaskHandler methods to maintain interface compatibility

      def initialize_task(task_request)
        raise NotImplementedError, 'Rust handler not available' unless @rust_handler

        # Validate and structure the task request using dry-struct
        validated_request = if task_request.is_a?(TaskerCore::Types::TaskRequest)
          task_request
        else
          TaskerCore::Types::TaskRequest.from_hash(task_request)
        end

        # Validate the request is suitable for creation
        unless validated_request.valid_for_creation?
          raise TaskerCore::ValidationError,
                "TaskRequest validation failed: Missing required fields"
        end

        # Convert to FFI-suitable hash and delegate to Rust
        ffi_hash = validated_request.to_ffi_hash

        # Rust FFI now returns a hash, convert it back to InitializeResult object
        result_hash = @rust_handler.initialize_task(ffi_hash)

        # Convert hash back to InitializeResult for backward compatibility
        TaskerCore::TaskHandler::InitializeResult.new(
          task_id: result_hash['task_id'],
          step_count: result_hash['step_count'],
          step_mapping: result_hash['step_mapping'],
          handler_config_name: result_hash['handler_config_name'],
          workflow_steps: result_hash['workflow_steps']
        )
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

      # FFI method called from Rust - converts hashes to objects for step handlers
      # This is the bridge between Rust FFI hashes and Ruby object-oriented step handlers
      # @param task_hash [Hash] Task data from Rust FFI
      # @param sequence_hash [Hash] Step sequence data from Rust FFI
      # @param step_hash [Hash] Current step data from Rust FFI
      # @return [Object] Step results (Hash, Array, String, etc.)
      def process_step_with_handler(task_hash, sequence_hash, step_hash)
        # Convert FFI hashes to simple wrapper objects using OpenStruct for compatibility
        require 'ostruct'
        
        # Create task object with method access
        task = OpenStruct.new(task_hash)
        
        # Create step object with method access
        step = OpenStruct.new(step_hash)
        
        # Create sequence object with steps method
        sequence = OpenStruct.new(sequence_hash)
        # Convert steps array to objects with method access
        if sequence_hash['all_steps']
          sequence.steps = sequence_hash['all_steps'].map { |step_data| OpenStruct.new(step_data) }
        else
          sequence.steps = []
        end
        # Also add dependencies method
        if sequence_hash['dependencies']
          sequence.dependencies = sequence_hash['dependencies'].map { |dep_data| OpenStruct.new(dep_data) }
        else
          sequence.dependencies = []
        end

        # Find and call the appropriate step handler
        step_handler = get_step_handler(step)
        return { error: 'Step handler not found' } unless step_handler

        step_handler.handle(task, sequence, step)
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
      # PHASE 2: SIMPLIFIED STEP HANDLER METHODS (Ruby-Centric Architecture)
      # ========================================================================


      # Get step handler by name for debugging/introspection - O(1) lookup
      # @param step_name [String] Name of the step to get handler for
      # @return [Object, nil] Pre-instantiated step handler or nil if not found
      def get_step_handler_from_name(step_name)
        @step_handlers[step_name]
      end

      # ========================================================================
      # INTERNAL PROCESSING METHODS
      # ========================================================================

      private

      # 🚀 PHASE 1: Register step handlers for Ruby-centric architecture
      # Pre-instantiates all step handlers during initialize for O(1) lookup performance
      def register_step_handlers(step_templates)
        handlers = {}
        return handlers if step_templates.nil? || step_templates.empty?

        step_templates.each do |template|
          step_name = template['name']
          handler_class_name = template['handler_class']
          handler_config = template.fetch('handler_config', {})

          begin
            handler_class = safe_constantize(handler_class_name)
            if handler_class
              handlers[step_name] = handler_class.new(config: handler_config, logger: @logger)
              logger.debug "✅ Pre-instantiated step handler: #{step_name} (#{handler_class_name})"
            else
              logger.warn "⚠️  Step handler class not found: #{handler_class_name} for step: #{step_name}"
            end
          rescue StandardError => e
            logger.error "❌ Failed to instantiate step handler #{handler_class_name}: #{e.message}"
            # Continue processing other handlers instead of failing completely
          end
        end

        handlers
      end

      # Safe constantize implementation without ActiveSupport dependency
      def safe_constantize(class_name)
        return nil if class_name.nil? || class_name.empty?

        # Split the class name by :: to handle nested classes
        constants = class_name.split('::')

        # Start with Object and traverse the constant hierarchy
        constants.inject(Object) do |context, constant|
          context.const_get(constant)
        end
      rescue NameError, ArgumentError
        nil
      end

      def load_task_config_from_path(path)
        return {} unless path && File.exist?(path)

        YAML.load_file(path)
      rescue StandardError => e
        puts "Error loading task configuration: #{e.message}"
        {}
      end

      # Default Rust integration
      def default_rust_integration
        # Return nil - integration is optional
        nil
      end
    end
  end
end

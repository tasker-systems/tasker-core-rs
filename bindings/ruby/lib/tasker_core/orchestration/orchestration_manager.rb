# frozen_string_literal: true

require 'singleton'


module TaskerCore
  module Internal
    # Ruby-level wrapper for the unified orchestration system
    # This provides a singleton pattern at the Ruby level to avoid
    # re-initializing the orchestration system on every call
    class OrchestrationManager
    include Singleton

    attr_reader :initialized_at, :status, :logger, :base_task_handler

    def initialize
      @initialized = false
      @status = 'not_initialized'
      @initialized_at = nil
      @orchestration_system = nil
      @base_task_handler = nil  # 🎯 Memoized BaseTaskHandler
      @orchestration_handle = nil  # 🎯 NEW: Handle-based FFI architecture
      @logger = TaskerCore::Logging::Logger.instance
    end

    # Get the orchestration system (handle-based initialization)
    def orchestration_system
      # Use handle-based approach - no direct system initialization needed
      if orchestration_handle && !@initialized
        logger.debug "Initializing orchestration system with TCP architecture"
        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now
        @orchestration_system ||= { 'handle_based' => true, 'architecture' => 'tcp' }

        # NOTE: No batch orchestrator or ZeroMQ integration needed with TCP architecture
        # TCP executor handles all orchestration internally via embedded server
        logger.debug "TCP orchestration system initialized"
      end

      unless @orchestration_system
        raise TaskerCore::Errors::OrchestrationError, 'Orchestration system not initialized'
      end

      @orchestration_system
    end

    # Check if the orchestration system is initialized
    def initialized?
      # Trigger initialization if not already done
      orchestration_system
      @initialized && @orchestration_system
    end

    # Get orchestration system status information
    def info
      {
        initialized: @initialized,
        status: @status,
        initialized_at: @initialized_at,
        architecture: @orchestration_system&.dig('architecture'),
        pool_source: @orchestration_system&.dig('pool_source')
      }
    end

    # Reset the orchestration system (for testing)
    def reset!
      @initialized = false
      @status = 'reset'
      @initialized_at = nil
      @orchestration_system = nil
      @base_task_handler = nil  # Reset memoized handler too
      @orchestration_handle = nil  # Reset handle too
      # NOTE: No batch orchestrator to stop with TCP architecture
    end

    # Delegate common orchestration operations - use handle-based versions

    def register_handler(handler_data)
      register_handler_with_handle(handler_data)
    end

    def find_handler(task_request)
      find_handler_with_handle(task_request)
    end

    def list_handlers(namespace = nil)
      orchestration_system # Ensure initialized
      # TODO: Implement handle-based list_handlers once available
      [] # Return empty for now
    end

    def handler_exists?(handler_key)
      orchestration_system # Ensure initialized
      # Check if we can find the handler instead
      result = find_handler(handler_key)
      result && result['found']
    end

    # Provide a handler registry for the batch step orchestrator
    # This acts as a bridge between the ZeroMQ executor and the shared TaskHandlerRegistry
    # Uses the existing TaskerCore::Registry which already delegates to the shared registry
    def handler_registry
      @handler_registry ||= RegistryAdapter.new
    end

    # Simple adapter to make TaskerCore::Registry compatible with BatchStepExecutionOrchestrator
    class RegistryAdapter
      # Get callable for step execution by delegating to TaskerCore::Registry
      def get_callable_for_class(handler_class)
        logger.debug "Looking up callable for handler class: #{handler_class}"

        # Try to find and instantiate the handler
        handler_instance = get_handler_instance(handler_class)

        # Return the handler if it has a .call method (new callable interface)
        return handler_instance if handler_instance&.respond_to?(:call)

        # If it has a .process method, wrap it for backward compatibility
        if handler_instance&.respond_to?(:process)
          logger.debug "Wrapping .process method for handler: #{handler_class}"
          return create_process_wrapper(handler_instance)
        end

        logger.warn "No callable found for handler class: #{handler_class}"
        nil
      end

      # Get handler instance by using TaskerCore::Registry
      def get_handler_instance(handler_class)
        logger.debug "Getting handler instance for: #{handler_class}"

        begin
          # Try direct class instantiation first
          klass = handler_class.to_s.constantize
          instance = klass.new
          logger.debug "Successfully instantiated handler: #{handler_class}"
          instance
        rescue NameError => e
          logger.warn "Could not instantiate handler class #{handler_class}: #{e.message}"
          nil
        rescue StandardError => e
          logger.error "Error instantiating #{handler_class}: #{e.message}"
          nil
        end
      end

      private

      def logger
        TaskerCore::Logging::Logger.instance
      end

      # Create a wrapper that adapts .process method to .call interface
      def create_process_wrapper(handler_instance)
        ->(task, sequence, step) do
          begin
            handler_instance.process(task, sequence, step)
          rescue ArgumentError => e
            logger.warn "Process method argument mismatch for #{handler_instance.class.name}: #{e.message}"

            # Try with just the task if that's what the handler expects
            if handler_instance.method(:process).arity == 1
              handler_instance.process(task)
            else
              raise e
            end
          end
        end
      end
    end

    # OBSOLETE: Individual workflow step inspection incompatible with TCP architecture
    # Dependencies are resolved automatically by Rust orchestration via TCP executor
    def get_workflow_steps_for_task(task_id)
      orchestration_system # Ensure initialized

      logger.warn(
        "get_workflow_steps_for_task is obsolete in TCP architecture. " +
        "Individual step inspection is incompatible with TCP executor batch processing. " +
        "Use TaskerCore.create_orchestration_handle.create_test_task instead."
      )

      logger.debug("Legacy get_workflow_steps_for_task called: task_id=#{task_id}")

      # Return nil to indicate method is obsolete
      nil
    end

    # OBSOLETE: Individual step dependency inspection incompatible with TCP architecture
    # Dependencies are validated automatically by Rust orchestration via TCP executor
    def get_step_dependencies(step_id)
      orchestration_system # Ensure initialized

      logger.warn(
        "get_step_dependencies is obsolete in TCP architecture. " +
        "Individual step dependency checking is incompatible with TCP executor batch processing. " +
        "Dependencies are resolved automatically during Rust TCP orchestration."
      )

      logger.debug("Legacy get_step_dependencies called: step_id=#{step_id}")

      # Return nil to indicate method is obsolete
      nil
    end

    # OBSOLETE: Individual task metadata inspection incompatible with TCP architecture
    # Task metadata is managed automatically by orchestration system registry
    def get_task_metadata(task_id)
      orchestration_system # Ensure initialized

      logger.warn(
        "get_task_metadata is obsolete in TCP architecture. " +
        "Task metadata is managed automatically by orchestration system registry. " +
        "Use TaskerCore::Registry methods for handler discovery instead."
      )

      logger.debug("Legacy get_task_metadata called: task_id=#{task_id}")

      # Return nil to indicate method is obsolete
      nil
    end

    # 🎯 THREAD-SAFE FIX: Create stateless BaseTaskHandler singleton
    # The handler itself is stateless - configuration is passed per-call via handle(task_id, config)
    # This ensures thread safety and correct per-task configuration handling
    def get_base_task_handler(task_config_hash = nil)
      orchestration_system # Ensure initialized

      # Create a stateless singleton handler that receives config per-call
      # The config parameter here is ignored - it's only kept for API compatibility
      @stateless_base_task_handler ||= begin
        logger.info "✅ Creating stateless BaseTaskHandler singleton (config passed per-call)"
        # BaseTaskHandler constructor takes no arguments
        TaskerCore::BaseTaskHandler.new
      rescue StandardError => e
        error_msg = "Failed to create BaseTaskHandler: #{e.message}"
        logger.error error_msg
        nil
      end
    end

    # 🎯 STEP PROCESSING: Get BaseTaskHandler for step processing via orchestration system
    # Ruby step handlers use the same BaseTaskHandler pattern as task handlers for consistency
    # This provides access to the process_step method for individual step execution
    def get_step_processor
      orchestration_system # Ensure initialized

      # Reuse the same BaseTaskHandler singleton for step processing
      # This ensures consistent orchestration system integration
      @step_processor ||= begin
        logger.info "✅ Getting BaseTaskHandler for step processing"
        get_base_task_handler
      rescue StandardError => e
        error_msg = "Failed to get step processor: #{e.message}"
        logger.error error_msg
        nil
      end
    end

    # 🎯 NEW: Handle-based architecture methods for optimal FFI performance

    # Get or create the orchestration handle (SINGLE INITIALIZATION)
    def orchestration_handle
      @orchestration_handle ||= begin
        logger.info "🎯 HANDLE ARCHITECTURE: Creating orchestration handle with persistent references"
        TaskerCore::OrchestrationHandle.new
      rescue StandardError => e
        error_msg = "Failed to create orchestration handle: #{e.class.name}: #{e.message}\n#{e.backtrace.first(5).join("\n")}"
        logger.error error_msg
        raise TaskerCore::Errors::OrchestrationError, error_msg
      end
    end

    # Handle-based factory operations (NO global lookups!)
    def create_test_task_with_handle(options)
      handle = orchestration_handle
      return nil unless handle

      handle.create_test_task(options)
    end

    def create_test_workflow_step_with_handle(options)
      handle = orchestration_handle
      return nil unless handle

      handle.create_test_workflow_step(options)
    end

    def create_test_foundation_with_handle(options)
      handle = orchestration_handle
      return nil unless handle

      handle.create_test_foundation(options)
    end

    # Handle-based orchestration operations (NO global lookups!)
    def register_handler_with_handle(handler_data)
      handle = orchestration_handle
      return nil unless handle

      handle.register_handler(handler_data)
    end

    def find_handler_with_handle(task_request)
      handle = orchestration_handle
      return nil unless handle

      handle.find_handler(task_request)
    end

    # Get handle info for debugging
    def handle_info
      handle = orchestration_handle
      return { status: 'no_handle' } unless handle

      handle.info
    end

    # 🎯 PHASE 2: Performance operations with handle delegation (NO global lookups!)

    def analyze_dependencies_with_handle(task_id)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.analyze_dependencies_with_handle(handle, task_id)
    end

    def get_task_execution_context_with_handle(task_id)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Performance.get_task_execution_context_with_handle(handle, task_id)
    end

    def discover_viable_steps_with_handle(task_id)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.discover_viable_steps_with_handle(handle, task_id)
    end

    def get_system_health_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Performance.get_system_health_with_handle(handle)
    end

    def get_analytics_metrics_with_handle(time_range_hours = nil)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Performance.get_analytics_metrics_with_handle(handle, time_range_hours)
    end

    # 🎯 PHASE 2: Event operations with handle delegation (NO global lookups!)

    def publish_simple_event_with_handle(event_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.publish_simple_event_with_handle(handle, event_data)
    end

    def publish_orchestration_event_with_handle(event_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.publish_orchestration_event_with_handle(handle, event_data)
    end

    def subscribe_to_events_with_handle(subscription_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.subscribe_to_events_with_handle(handle, subscription_data)
    end

    def get_event_stats_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::Events.get_event_stats_with_handle(handle)
    end

    def register_external_event_callback_with_handle(callback_data)
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.register_external_event_callback_with_handle(handle, callback_data)
    end

    # 🎯 PHASE 2: Testing operations with handle delegation (NO global lookups!)

    def create_testing_framework_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::TestHelpers::TestingFramework.create_testing_framework_with_handle(handle)
    end

    def setup_test_environment_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::TestHelpers::TestingFramework.setup_test_environment_with_handle(handle)
    end

    def cleanup_test_environment_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore::TestHelpers::TestingFramework.cleanup_test_environment_with_handle(handle)
    end

    # TCP integration status - replaces ZeroMQ status
    def tcp_integration_status
      handle = orchestration_handle
      return { enabled: false, reason: 'no_handle' } unless handle

      # Check TCP executor enabled status with error handling
      tcp_enabled = begin
        handle.is_tcp_executor_enabled
      rescue StandardError => e
        logger.debug "TCP executor status check failed: #{e.message}"
        false
      end

      return { enabled: false, reason: 'tcp_executor_disabled' } unless tcp_enabled

      # Get TCP executor config with error handling
      tcp_config = begin
        handle.tcp_executor_config
      rescue StandardError => e
        logger.debug "TCP executor config retrieval failed: #{e.message}"
        {}
      end

      {
        enabled: true,
        running: true, # TCP executor manages its own lifecycle
        architecture: 'tcp',
        tcp_config: tcp_config
      }
    rescue StandardError => e
      { enabled: false, error: e.message }
    end

    # 🎯 RUST-BACKED COMMAND ARCHITECTURE INTEGRATION

    # Create Rust-backed command client for worker communication
    #
    # @param host [String] Server host (optional, uses config default)
    # @param port [Integer] Server port (optional, uses config default)
    # @param timeout [Integer] Connection timeout (optional, uses config default)
    # @return [TaskerCore::Execution::CommandClient] Rust-backed command client
    def create_command_client(host: nil, port: nil, timeout: nil)
      orchestration_system # Ensure initialized

      begin
        logger.info "🎯 Creating Rust-backed CommandClient for worker communication"
        TaskerCore::Execution::CommandClient.new(
          host: host,
          port: port,
          timeout: timeout
        )
      rescue StandardError => e
        error_msg = "Failed to create Rust-backed CommandClient: #{e.class.name}: #{e.message}"
        logger.error error_msg
        raise TaskerCore::Errors::OrchestrationError, error_msg
      end
    end

    # Create Rust-backed worker manager for lifecycle management
    #
    # @param worker_id [String] Unique worker identifier
    # @param supported_namespaces [Array<String>] Namespaces this worker supports (optional, auto-discovered)
    # @param max_concurrent_steps [Integer] Maximum concurrent steps (optional, uses default)
    # @param heartbeat_interval [Integer] Heartbeat interval in seconds (optional, uses default)
    # @param server_host [String] Server host (optional, uses config default)
    # @param server_port [Integer] Server port (optional, uses config default)
    # @param custom_capabilities [Hash] Additional worker capabilities (optional)
    # @return [TaskerCore::Execution::WorkerManager] Rust-backed worker manager
    def create_worker_manager(worker_id:, supported_namespaces: nil, 
                             max_concurrent_steps: nil, heartbeat_interval: nil,
                             server_host: nil, server_port: nil, custom_capabilities: {})
      orchestration_system # Ensure initialized

      begin
        logger.info "🎯 Creating Rust-backed WorkerManager for worker #{worker_id}"
        
        # Build configuration with defaults
        config = {
          worker_id: worker_id,
          custom_capabilities: custom_capabilities
        }
        
        # Add optional parameters if provided
        config[:supported_namespaces] = supported_namespaces if supported_namespaces
        config[:max_concurrent_steps] = max_concurrent_steps if max_concurrent_steps
        config[:heartbeat_interval] = heartbeat_interval if heartbeat_interval
        config[:server_host] = server_host if server_host
        config[:server_port] = server_port if server_port

        TaskerCore::Execution::WorkerManager.new(**config)
      rescue StandardError => e
        error_msg = "Failed to create Rust-backed WorkerManager: #{e.class.name}: #{e.message}"
        logger.error error_msg
        raise TaskerCore::Errors::OrchestrationError, error_msg
      end
    end

    # Get command architecture status and configuration
    #
    # @return [Hash] Command architecture information
    def command_architecture_status
      orchestration_system # Ensure initialized

      begin
        tcp_status = tcp_integration_status
        
        {
          architecture: 'rust_backed_commands',
          tcp_integration: tcp_status,
          rust_ffi_available: defined?(TaskerCore::CommandClient),
          worker_manager_available: defined?(TaskerCore::WorkerManager),
          command_listener_available: defined?(TaskerCore::CommandListener),
          components: {
            command_client: 'TaskerCore::Execution::CommandClient (Rust-backed)',
            worker_manager: 'TaskerCore::Execution::WorkerManager (Rust-backed)',
            orchestration_handle: 'TaskerCore::OrchestrationHandle (Handle-based FFI)'
          }
        }
      rescue StandardError => e
        {
          architecture: 'error',
          error: e.message,
          components: {}
        }
      end
    end

    # Auto-discover and register a Ruby worker with optimal configuration
    #
    # This method provides a high-level interface for Ruby applications to easily
    # register workers with the Rust orchestration system using auto-discovered
    # namespaces and optimal default configuration.
    #
    # @param worker_id [String] Unique worker identifier
    # @param custom_capabilities [Hash] Additional worker capabilities (optional)
    # @return [TaskerCore::Execution::WorkerManager] Configured and started worker manager
    # @raise [TaskerCore::Errors::OrchestrationError] if worker registration fails
    def register_ruby_worker(worker_id:, custom_capabilities: {})
      orchestration_system # Ensure initialized

      begin
        logger.info "🎯 Auto-registering Ruby worker: #{worker_id}"

        # Create worker manager with auto-discovery
        worker_manager = create_worker_manager(
          worker_id: worker_id,
          custom_capabilities: custom_capabilities.merge(
            'auto_registered' => true,
            'orchestration_manager' => true,
            'ruby_integration' => 'rust_backed'
          )
        )

        # Start the worker
        success = worker_manager.start
        unless success
          raise TaskerCore::Errors::OrchestrationError, "Failed to start worker #{worker_id}"
        end

        logger.info "✅ Ruby worker #{worker_id} registered and started successfully"
        worker_manager
      rescue StandardError => e
        error_msg = "Failed to register Ruby worker #{worker_id}: #{e.class.name}: #{e.message}"
        logger.error error_msg
        raise TaskerCore::Errors::OrchestrationError, error_msg
      end
    end

    # ========================================================================
    # RUBY TASK HANDLER REGISTRY (Ruby-Centric Architecture)
    # ========================================================================

    # Registry of Ruby TaskHandler instances for direct step execution
    def ruby_task_handlers
      @ruby_task_handlers ||= {}
    end

    # Register a Ruby TaskHandler instance for step execution
    # @param namespace [String] Task namespace
    # @param name [String] Task name
    # @param version [String] Task version
    # @param handler [TaskerCore::TaskHandler::Base] TaskHandler instance with pre-instantiated step handlers
    def register_ruby_task_handler(namespace, name, version, handler)
      key = "#{namespace}/#{name}/#{version}"
      ruby_task_handlers[key] = handler
      @logger&.debug "📝 Registered Ruby TaskHandler: #{key} with #{handler.step_handlers&.size || 0} step handlers"
      true
    end

    # Get Ruby TaskHandler for a specific task configuration
    # @param namespace [String] Task namespace
    # @param name [String] Task name
    # @param version [String] Task version
    # @return [TaskerCore::TaskHandler::Base, nil] TaskHandler instance or nil if not found
    def get_ruby_task_handler(namespace, name, version)
      key = "#{namespace}/#{name}/#{version}"
      ruby_task_handlers[key]
    end

    # Get Ruby TaskHandler for a specific task_id by looking up task metadata
    # This is the method called from Rust FFI to find the appropriate handler
    # @param task_id [Integer] Task ID to find handler for
    # @return [TaskerCore::TaskHandler::Base, nil] TaskHandler instance or nil if not found
    def get_task_handler_for_task(task_id)
      # TODO: Implement task metadata lookup to find namespace/name/version for task_id
      # For now, return the first available TaskHandler as fallback
      # In production, this would query the database to get task metadata, then lookup handler

      if ruby_task_handlers.any?
        handler = ruby_task_handlers.values.first
        @logger&.debug "🔍 Found TaskHandler for task_id #{task_id}: #{handler.class.name}"
        handler
      else
        @logger&.warn "⚠️  No TaskHandler found for task_id #{task_id}"
        nil
      end
    end

    # List all registered Ruby TaskHandlers
    # @return [Hash] Hash of registered handlers by key
    def list_ruby_task_handlers
      ruby_task_handlers.keys.map do |key|
        {
          key: key,
          handler_class: ruby_task_handlers[key].class.name,
          step_handler_count: ruby_task_handlers[key].step_handlers&.size || 0
        }
      end
    end
    end
  end
end

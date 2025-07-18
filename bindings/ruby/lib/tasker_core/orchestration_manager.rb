# frozen_string_literal: true

require 'singleton'

module TaskerCore
  # Ruby-level wrapper for the unified orchestration system
  # This provides a singleton pattern at the Ruby level to avoid
  # re-initializing the orchestration system on every call
  class OrchestrationManager
    include Singleton

    attr_reader :initialized_at, :status, :logger

    def initialize
      @initialized = false
      @status = 'not_initialized'
      @initialized_at = nil
      @orchestration_system = nil
      @base_task_handler = nil  # 🎯 Memoized BaseTaskHandler
      @orchestration_handle = nil  # 🎯 NEW: Handle-based FFI architecture
      @logger = TaskerCore::Logging::Logger.instance
    end

    # Initialize the unified orchestration system once
    def initialize_orchestration_system!
      return @orchestration_system if @initialized

      begin
        @orchestration_system = TaskerCore.initialize_unified_orchestration_system
        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now

        logger.info "OrchestrationManager: Unified orchestration system initialized successfully"
        @orchestration_system
      rescue StandardError => e
        @status = 'failed'
        error_msg = "Failed to initialize unified orchestration system: #{e.message}"

        if defined?(Rails)
          logger.error error_msg
        else
          logger.error error_msg
        end

        raise e
      end
    end

    # Get the orchestration system (initializing if needed)
    def orchestration_system
      initialize_orchestration_system! unless @initialized
      @orchestration_system
    end

    # Check if the orchestration system is initialized
    def initialized?
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
    end

    # Delegate common orchestration operations

    def register_handler(handler_data)
      orchestration_system # Ensure initialized
      TaskerCore.register_ffi_handler(handler_data)
    end

    def find_handler(task_request)
      orchestration_system # Ensure initialized
      TaskerCore.find_handler(task_request)
    end

    def list_handlers(namespace = nil)
      orchestration_system # Ensure initialized
      TaskerCore.list_handlers(namespace)
    end

    def handler_exists?(handler_key)
      orchestration_system # Ensure initialized
      TaskerCore.contains_handler(handler_key)
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
        # Pass empty config since the handler is now stateless
        TaskerCore::BaseTaskHandler.new({})
      rescue StandardError => e
        error_msg = "Failed to create BaseTaskHandler: #{e.message}"
        logger.error error_msg
        nil
      end
    end

    # 🎯 NEW: Handle-based architecture methods for optimal FFI performance

    # Get or create the orchestration handle (SINGLE INITIALIZATION)
    def orchestration_handle
      @orchestration_handle ||= begin
        logger.info "🎯 HANDLE ARCHITECTURE: Creating orchestration handle with persistent references"
        TaskerCore.create_orchestration_handle
      rescue StandardError => e
        error_msg = "Failed to create orchestration handle: #{e.message}"
        logger.error error_msg
        nil
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

      handle.register_ffi_handler(handler_data)
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

      TaskerCore.create_testing_framework_with_handle(handle)
    end

    def setup_test_environment_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.setup_test_environment_with_handle(handle)
    end

    def cleanup_test_environment_with_handle
      handle = orchestration_handle
      return nil unless handle

      TaskerCore.cleanup_test_environment_with_handle(handle)
    end
  end
end
